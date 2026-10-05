// SPDX-License-Identifier: Apache-2.0
//! Domain routing and failure ordering through the complete current pipeline.
use super::*;
use crate::{admitted_history::AdmittedCut, history_catalog::Catalog, history_index::Pages};
use vcp_domain::{
    ingestion::*,
    search::{Active, Generation, ACTIVE, GENERATION},
};
#[path = "../tests/common/mod.rs"]
mod common;

#[derive(Default)]
struct Memory(BTreeMap<String, Vec<u8>>);
impl Pages for Memory {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        let bytes = self
            .0
            .get(digest)
            .ok_or(Error::Corruption("domain fixture page missing"))?;
        if bytes.len() > limit {
            return Err(Error::Limit("domain fixture page"));
        }
        Ok(bytes.clone())
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        if let Some(prior) = self.0.insert(digest.to_owned(), bytes.to_vec()) {
            assert_eq!(prior, bytes);
        }
        Ok(())
    }
}
async fn admitted(state: &State) -> (Memory, AdmittedCut) {
    let mut pages = Memory::default();
    let catalog = Catalog::from_validated_state(&mut pages, state)
        .await
        .unwrap();
    let cut = AdmittedCut::from_replayed(&mut pages, state, catalog)
        .await
        .unwrap();
    (pages, cut)
}
fn put(collection: Collection, id: impl Into<String>, value: &impl Serialize) -> Mutation {
    Mutation::Put {
        expected: None,
        record: Record::typed(
            collection,
            id,
            common::workspace().id,
            Revision::ZERO,
            value,
        )
        .unwrap(),
    }
}
fn transaction(source: &State, mutations: Vec<Mutation>) -> Transaction {
    Transaction {
        id: TransactionId::new(),
        expected_watermark: source.watermark,
        mutations,
        events: vec![],
        command: None,
    }
}
fn manifest(source: &State, tx: &Transaction, previous: Option<GenerationId>) -> Generation {
    Generation {
        document_type: GENERATION.into(),
        schema_version: 1,
        id: GenerationId::new(),
        scope: common::task().scope,
        revision: Revision::ZERO,
        transaction: tx.id.clone(),
        previous,
        canonical_watermark: source.watermark,
        memory_seq: MemorySeq::ZERO,
        authority: AuthorityRevision::ZERO,
        deletion: DeletionEpoch::ZERO,
        inventory_digest: "a".repeat(64),
        inventory_checksum: "b".repeat(64),
        lexical_schema: 1,
        tokenizer: "code/1".into(),
        lexical_checksum: "c".repeat(64),
        embedding_specification: "d".repeat(64),
        empty_complete: false,
        vector_checksum: Some("e".repeat(64)),
        vector_deficits: vec![],
        covered_intents: vec![],
    }
}
fn publication(source: &State, previous: Option<GenerationId>) -> (Transaction, Generation) {
    let mut tx = transaction(source, vec![]);
    let generation = manifest(source, &tx, previous);
    let prior = source
        .records
        .get(&key(Collection::Generation, "workspace"));
    let revision = prior
        .map(|row| row.revision.next().unwrap())
        .unwrap_or_default();
    let active = Active {
        document_type: ACTIVE.into(),
        schema_version: 1,
        id: generation.scope.workspace.clone(),
        workspace: generation.scope.workspace.clone(),
        revision,
        generation: generation.id.clone(),
        transaction: tx.id.clone(),
    };
    tx.mutations = vec![
        put(
            Collection::Generation,
            generation.id.to_string(),
            &generation,
        ),
        Mutation::Put {
            expected: prior.map(|row| row.revision),
            record: Record::typed(
                Collection::Generation,
                active.id.to_string(),
                active.workspace.clone(),
                revision,
                &active,
            )
            .unwrap(),
        },
    ];
    (tx, generation)
}
fn ingestion(source: &State) -> (Cursor, Job) {
    let scope = common::task().scope;
    let extractor = ExtractorSpec {
        name: "fixture".into(),
        version: 1,
        event_kinds: vec!["task_created".into()],
    };
    let id = CommandId::parse(digest_bytes(
        &canonical_bytes(&("ingestion-cursor/1", &scope, &extractor)).unwrap(),
    ))
    .unwrap();
    let event = source.events.last().unwrap();
    let cursor = Cursor {
        document_type: DocumentType::Cursor,
        schema_version: 1,
        id,
        scope,
        revision: Revision::ZERO,
        extractor,
        after: Units::new(source.events.len() as u64),
        scanned_through: event.watermark,
    };
    let job = Job {
        document_type: DocumentType::Job,
        schema_version: 1,
        id: CommandId::parse(digest_bytes(
            &canonical_bytes(&("ingestion-job/1", &cursor.id, &event.event.id)).unwrap(),
        ))
        .unwrap(),
        scope: cursor.scope.clone(),
        root: cursor.scope.task.clone(),
        revision: Revision::ZERO,
        cursor: cursor.id.clone(),
        extractor: cursor.extractor.clone(),
        origin: event.event.id.clone(),
        origin_watermark: event.watermark,
        state: JobState::Pending,
        attempts: Units::ZERO,
        max_attempts: Units::new(2),
        lease: None,
        not_before: Timestamp::ZERO,
        last_failure: None,
        results: vec![],
        finding: None,
    };
    (cursor, job)
}
async fn rejected(
    source: &State,
    pages: &mut Memory,
    cut: &AdmittedCut,
    tx: &Transaction,
    message: &str,
) {
    let expected = source.prepare_reference(tx).err().unwrap().to_string();
    assert!(expected.contains(message), "{expected}");
    let actual = current_transition::prepare(pages, cut, tx)
        .await
        .err()
        .unwrap()
        .to_string();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn search_publication_routes_new_and_retained_receipts_and_checks_prior_pointer() {
    let source = State::default()
        .prepare_reference(&common::initial())
        .unwrap()
        .0;
    let (mut pages, cut) = admitted(&source).await;
    let (tx, generation) = publication(&source, None);
    let expected = source.prepare_reference(&tx).unwrap();
    let current_transition::Outcome::Prepared(prepared) =
        current_transition::prepare(&mut pages, &cut, &tx)
            .await
            .unwrap()
    else {
        panic!("new publication")
    };
    assert_eq!(prepared.commit(), &expected.1);
    assert_eq!(
        prepared.proposed().current,
        crate::CurrentState::from_state(&expected.0)
    );
    // Current validation must see the newly admitted transaction receipt even
    // though the source resolver proves its prior absence during preparation.
    assert!(!source.transactions.contains_key(&tx.id));
    let source = expected.0;
    let (mut pages, cut) = admitted(&source).await;
    let (next, _) = publication(&source, Some(generation.id));
    let expected = source.prepare_reference(&next).unwrap();
    let current_transition::Outcome::Prepared(prepared) =
        current_transition::prepare(&mut pages, &cut, &next)
            .await
            .unwrap()
    else {
        panic!("next publication")
    };
    assert_eq!(
        prepared.proposed().current,
        crate::CurrentState::from_state(&expected.0)
    );
    assert_eq!(prepared.commit(), &expected.1);
    assert!(
        prepared.history_work().resolutions.transaction_reads >= 2,
        "new receipt absence and retained generation receipt are distinct lookups"
    );
    let mut no_pointer = next.clone();
    no_pointer.mutations.pop();
    rejected(
        &source,
        &mut pages,
        &cut,
        &no_pointer,
        "requires one manifest and active pointer",
    )
    .await;
}

#[tokio::test]
async fn ingestion_uses_complete_history_pass_and_preserves_domain_error_order() {
    let source = State::default()
        .prepare_reference(&common::initial())
        .unwrap()
        .0;
    let (mut pages, cut) = admitted(&source).await;
    let (cursor, job) = ingestion(&source);
    let good = transaction(
        &source,
        vec![
            put(Collection::Claim, cursor.id.to_string(), &cursor),
            put(Collection::Claim, job.id.to_string(), &job),
        ],
    );
    let expected = source.prepare_reference(&good).unwrap();
    let current_transition::Outcome::Prepared(prepared) =
        current_transition::prepare(&mut pages, &cut, &good)
            .await
            .unwrap()
    else {
        panic!("new ingestion")
    };
    assert_eq!(prepared.commit(), &expected.1);
    assert_eq!(
        prepared.proposed().current,
        crate::CurrentState::from_state(&expected.0)
    );
    assert_eq!(prepared.history_work().full_passes, 2);
    assert_eq!(prepared.history_work().rows, source.events.len() as u64 * 2);
    assert_eq!(prepared.history_work().event_validation.prefix_reuses, 1);
    assert_eq!(
        prepared.history_work().event_validation.rows_examined,
        good.events.len() as u64
    );
    let (mut bad, _) = publication(&source, None);
    bad.mutations.pop(); // A later publication error, after the ingestion phase.
    bad.mutations
        .push(put(Collection::Claim, cursor.id.to_string(), &cursor));
    bad.events = common::initial().events; // Duplicate source ID fails event phase first.
    rejected(&source, &mut pages, &cut, &bad, "event identity").await;
    bad.events.clear();
    rejected(
        &source,
        &mut pages,
        &cut,
        &bad,
        "cursor advanced without durable job",
    )
    .await;
    bad.mutations.pop();
    rejected(
        &source,
        &mut pages,
        &cut,
        &bad,
        "requires one manifest and active pointer",
    )
    .await;
}
