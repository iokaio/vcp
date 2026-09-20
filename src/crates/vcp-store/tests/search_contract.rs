// SPDX-License-Identifier: Apache-2.0
//! Canonical publication validation rejects incomplete and forged coverage.
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{
    memory::{IndexIntent, IndexStatus},
    search::{Active, Generation, ACTIVE, GENERATION},
    task::Task,
    workspace::Workspace,
    *,
};
use vcp_store::{contract::*, Error, Result};
#[path = "common/mod.rs"]
mod common;
#[path = "../src/search_contract.rs"]
#[allow(dead_code)]
mod search_contract;

fn manifest(state: &State) -> Generation {
    Generation {
        document_type: GENERATION.into(),
        schema_version: 1,
        id: GenerationId::new(),
        scope: common::task().scope,
        revision: Revision::ZERO,
        transaction: TransactionId::new(),
        previous: None,
        canonical_watermark: state.watermark,
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
fn row(value: &Generation) -> Record {
    Record::typed(
        Collection::Generation,
        value.id.to_string(),
        value.scope.workspace.clone(),
        value.revision,
        value,
    )
    .unwrap()
}
fn tx(before: &State, value: &Generation) -> Transaction {
    let active = Active {
        document_type: ACTIVE.into(),
        schema_version: 1,
        id: value.scope.workspace.clone(),
        workspace: value.scope.workspace.clone(),
        revision: Revision::ZERO,
        generation: value.id.clone(),
        transaction: value.transaction.clone(),
    };
    Transaction {
        id: value.transaction.clone(),
        expected_watermark: before.watermark,
        events: vec![],
        command: None,
        mutations: vec![
            Mutation::Put {
                expected: None,
                record: row(value),
            },
            Mutation::Put {
                expected: None,
                record: Record::typed(
                    Collection::Generation,
                    active.id.to_string(),
                    active.workspace.clone(),
                    active.revision,
                    &active,
                )
                .unwrap(),
            },
        ],
    }
}
fn after(before: &State, tx: &Transaction) -> State {
    let mut next = before.clone();
    for mutation in &tx.mutations {
        if let Mutation::Put { record, .. } = mutation {
            next.records.insert(record.key(), record.clone());
        }
    }
    next
}
fn initial() -> State {
    State::default().prepare(&common::initial()).unwrap().0
}

#[test]
fn typed_tags_collections_and_manifest_immutability_cannot_use_generic_put() {
    let state = initial();
    let value = manifest(&state);
    let record = row(&value);
    search_contract::shape(&record).unwrap();
    assert!(search_contract::scope(&record).unwrap().is_some());
    assert!(search_contract::references(&record)
        .unwrap()
        .contains(&key(Collection::Task, value.scope.task.as_str())));
    let mut unknown = record.clone();
    unknown.value["document_type"] = serde_json::json!("vcp_search_future_v1");
    assert!(search_contract::kind(&unknown).is_err());
    let mut wrong = record.clone();
    wrong.collection = Collection::Claim;
    assert!(search_contract::kind(&wrong).is_err());
    assert!(search_contract::transition(&record, &record).is_err());
    let mut generic = record.clone();
    generic.value = serde_json::json!({"schema_version":1});
    assert!(search_contract::transition(&record, &generic).is_err());
}

#[test]
fn publication_requires_atomic_pointer_and_current_epochs_and_predecessor() {
    let state = initial();
    let value = manifest(&state);
    let transaction = tx(&state, &value);
    search_contract::publication(&state, &after(&state, &transaction), &transaction).unwrap();
    let mut missing = transaction.clone();
    missing.mutations.pop();
    assert!(search_contract::publication(&state, &after(&state, &missing), &missing).is_err());
    for bad in [
        Generation {
            authority: AuthorityRevision::new(1),
            ..value.clone()
        },
        Generation {
            deletion: DeletionEpoch::new(1),
            ..value.clone()
        },
        Generation {
            previous: Some(GenerationId::new()),
            ..value.clone()
        },
        Generation {
            canonical_watermark: state.watermark.next().unwrap(),
            ..value.clone()
        },
    ] {
        let transaction = tx(&state, &bad);
        assert!(
            search_contract::publication(&state, &after(&state, &transaction), &transaction)
                .is_err()
        );
    }
}

#[test]
fn exact_covered_intents_require_same_transaction_acknowledgements() {
    // Minimal typed rows isolate the publication hook. State::validate separately
    // enforces each intent's version/evidence references in integrated tests.
    let mut state = initial();
    let mut value = manifest(&state);
    let intent = IndexIntent {
        deletion: None,
        document_type: vcp_domain::memory::DocumentType::IndexIntent,
        schema_version: 1,
        id: IndexIntentId::new(),
        scope: value.scope.clone(),
        revision: Revision::ZERO,
        transaction: TransactionId::new(),
        versions: vec![ClaimVersionId::new()],
        supersedes: vec![],
        memory_seq: MemorySeq::new(1),
        canonical_watermark: state.watermark,
        status: IndexStatus::Pending,
    };
    let intent_row = Record::typed(
        Collection::IndexIntent,
        intent.id.to_string(),
        intent.scope.workspace.clone(),
        intent.revision,
        &intent,
    )
    .unwrap();
    state.records.insert(intent_row.key(), intent_row);
    let skipped = tx(&state, &value);
    assert!(search_contract::publication(&state, &after(&state, &skipped), &skipped).is_err());
    value.covered_intents = vec![intent.id.clone()];
    let mut transaction = tx(&state, &value);
    assert!(
        search_contract::publication(&state, &after(&state, &transaction), &transaction).is_err()
    );
    let mut ready = intent.clone();
    ready.status = IndexStatus::Ready;
    ready.revision = Revision::new(1);
    let record = Record::typed(
        Collection::IndexIntent,
        ready.id.to_string(),
        ready.scope.workspace.clone(),
        ready.revision,
        &ready,
    )
    .unwrap();
    transaction.mutations.push(Mutation::Put {
        expected: Some(Revision::ZERO),
        record,
    });
    search_contract::publication(&state, &after(&state, &transaction), &transaction).unwrap();
    let mut alone = transaction.clone();
    alone.mutations.drain(..2);
    assert!(search_contract::publication(&state, &after(&state, &alone), &alone).is_err());
    let mut deficient = value.clone();
    deficient.vector_checksum = None;
    assert!(deficient.validate().is_err());
    let mut deficient_tx = transaction.clone();
    if let Mutation::Put { record, .. } = &mut deficient_tx.mutations[0] {
        record.value = serde_json::to_value(&deficient).unwrap();
    }
    assert!(state.prepare(&deficient_tx).is_err());
    assert!(
        search_contract::publication(&state, &after(&state, &deficient_tx), &deficient_tx).is_err()
    );
    let mut inflated = value;
    inflated.memory_seq = MemorySeq::new(100);
    let inflated = tx(&state, &inflated);
    assert!(search_contract::publication(&state, &after(&state, &inflated), &inflated).is_err());
}
