// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    contract::{Commit, State, Transaction},
    durable_owner::{DurableOwner, Outcome},
    history_catalog::Catalog,
    original_commits::OriginalCommits,
};
use vcp_domain::{CommandId, TransactionId};
use vcp_protocol::canonical_bytes;
#[path = "../tests/common/mod.rs"]
mod common;

#[derive(Clone, Default)]
struct Memory {
    objects: BTreeMap<String, Vec<u8>>,
    reads: usize,
    read_only: bool,
}
impl Pages for Memory {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        self.reads += 1;
        let bytes = self
            .objects
            .get(digest)
            .ok_or(Error::Corruption("prototype missing object"))?;
        if bytes.len() > limit {
            return Err(Error::Limit("prototype object"));
        }
        Ok(bytes.clone())
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        assert!(
            !self.read_only,
            "replay must use physical ComparePages, never write"
        );
        if let Some(prior) = self.objects.get(digest) {
            assert_eq!(prior, bytes);
        } else {
            self.objects.insert(digest.into(), bytes.into());
        }
        Ok(())
    }
}
async fn owner(pages: &mut Memory, state: &State, commit: &Commit) -> DurableOwner {
    let originals = OriginalCommits::from_validated_base(pages, None)
        .await
        .unwrap()
        .append_verified(pages, &canonical_bytes(commit).unwrap(), commit)
        .await
        .unwrap();
    let catalog = Catalog::from_validated_state(pages, state).await.unwrap();
    DurableOwner::from_replayed(pages, state, None, catalog, originals)
        .await
        .unwrap()
}
fn transaction(state: &State, index: usize) -> Transaction {
    let mut tx = common::initial();
    tx.id = TransactionId::parse(format!("prototype-{index}")).unwrap();
    tx.expected_watermark = state.watermark;
    tx.mutations.clear();
    if index % 3 == 1 {
        let mut task: Task = state.records[&key(Collection::Task, "task")]
            .decode()
            .unwrap();
        let previous = task.revision;
        task.revision = previous.next().unwrap();
        task.reason = format!("generated current text {index}");
        tx.mutations.push(crate::contract::Mutation::Put {
            expected: Some(previous),
            record: Record::typed(
                Collection::Task,
                "task",
                task.scope.workspace.clone(),
                task.revision,
                &task,
            )
            .unwrap(),
        });
    }
    tx.command = None;
    tx.events[0].id = EventId::parse(format!("prototype-event-{index}")).unwrap();
    tx.events[0].correlation = CommandId::parse(format!("prototype-command-{index}")).unwrap();
    tx
}
struct Corpus {
    pages: Memory,
    initial: State,
    initial_commit: Commit,
    catalog: Catalog,
    originals: OriginalCommits,
    rows: Vec<(Vec<u8>, String, State)>,
}
async fn corpus(count: usize) -> Corpus {
    let mut initial_tx = common::initial();
    let mut spec = common::spec();
    spec.id = vcp_domain::ArtifactId::parse("prototype-artifact").unwrap();
    let artifact = ArtifactDescriptor {
        spec,
        state: vcp_domain::artifact::CaptureState::Pending,
        length: vcp_domain::ByteCount::ZERO,
        sha256: vcp_protocol::digest_bytes(b""),
        retained: vec![vcp_domain::artifact::Range {
            start: vcp_domain::ByteCount::ZERO,
            end: vcp_domain::ByteCount::ZERO,
        }],
    };
    initial_tx.events[0]
        .artifacts
        .push(artifact.spec.id.clone());
    initial_tx.mutations.push(crate::contract::Mutation::Put {
        expected: None,
        record: Record::typed(
            Collection::Artifact,
            artifact.spec.id.to_string(),
            common::workspace().id,
            vcp_domain::Revision::ZERO,
            &artifact,
        )
        .unwrap(),
    });
    let (initial, initial_commit) = State::default().prepare_reference(&initial_tx).unwrap();
    let mut pages = Memory::default();
    let mut cut = owner(&mut pages, &initial, &initial_commit).await;
    let catalog = cut.semantic().catalog().clone();
    let originals = cut.originals().clone();
    let mut state = initial.clone();
    let mut rows = Vec::new();
    for index in 0..count {
        let tx = transaction(&state, index);
        let expected = state.prepare_reference(&tx).unwrap();
        let Outcome::Prepared(prepared) = cut.prepare(&mut pages, &tx).await.unwrap() else {
            panic!("new");
        };
        assert_eq!(prepared.commit(), &expected.1);
        let payload = serde_json::to_vec_pretty(prepared.commit()).unwrap();
        let (next, publication) =
            crate::history_publication::stage(&mut pages, &cut, &prepared, &payload)
                .await
                .unwrap();
        state = expected.0;
        rows.push((payload, publication, state.clone()));
        cut = next;
    }
    Corpus {
        pages,
        initial,
        initial_commit,
        catalog,
        originals,
        rows,
    }
}

async fn cold(pages: &mut Memory, corpus: &Corpus, optimized: bool) -> Result<String> {
    pages.read_only = true;
    let mut cut = DurableOwner::from_replayed(
        pages,
        &corpus.initial,
        None,
        corpus.catalog.clone(),
        corpus.originals.clone(),
    )
    .await?;
    for (payload, publication, _) in &corpus.rows {
        cut = if optimized {
            production(pages, &cut, payload, publication).await?
        } else {
            full(pages, &cut, payload, publication).await?
        };
    }
    Ok(cut.identity().to_owned())
}

#[tokio::test]
async fn event_phase_dependencies_and_first_errors_match_frozen_reference() {
    let mut corpus = corpus(2).await;
    let cut = owner(&mut corpus.pages, &corpus.initial, &corpus.initial_commit).await;
    let mut proof = EventReplayProof::default();
    let (payload, publication, state) = &corpus.rows[0];
    let cut = candidate(&mut corpus.pages, &cut, payload, publication, &mut proof)
        .await
        .unwrap();
    let task_key = key(Collection::Task, "task");
    let session_key = key(Collection::Session, "session");
    let artifact_key = key(
        Collection::Artifact,
        corpus.initial.events[0].event.artifacts[0].as_str(),
    );
    for mutation in 0..14 {
        let tx = transaction(state, 80 + mutation);
        let crate::contract::current_preparation::Outcome::Proposed(mut proposed) =
            crate::contract::current_preparation::propose(
                cut.semantic().current(),
                &tx,
                &mut crate::contract::current_preparation::StateHistory(state),
            )
            .unwrap()
        else {
            panic!("new");
        };
        match mutation {
            0 => proposed.events[0].event.id = state.events[0].event.id.clone(),
            1 => proposed.events[0].sequence = proposed.events[0].sequence.next().unwrap(),
            2 => proposed.events[0].version = 2,
            3 => proposed.events[0].watermark = proposed.current.watermark.next().unwrap(),
            4 | 10 => {
                proposed.current.records.get_mut(&task_key).unwrap().value["scope"]["session"] =
                    "other-session".into();
                proposed.touched.insert(task_key.clone());
                if mutation == 10 {
                    proposed.events[0].event.id = state.events[0].event.id.clone();
                }
            }
            5 => {
                proposed
                    .current
                    .records
                    .get_mut(&session_key)
                    .unwrap()
                    .workspace = WorkspaceId::parse("other-workspace").unwrap();
                proposed.touched.insert(session_key.clone());
            }
            6 => {
                proposed
                    .current
                    .records
                    .get_mut(&artifact_key)
                    .unwrap()
                    .value["spec"]["scope"]["session"] = "other-session".into();
                proposed.touched.insert(artifact_key.clone());
            }
            7 => {
                proposed.current.records.remove(&artifact_key);
                proposed.touched.insert(artifact_key.clone());
            }
            8 => {
                proposed.current.records.remove(&task_key);
                proposed.touched.insert(task_key.clone());
            }
            9 => {
                proposed.current.records.get_mut(&task_key).unwrap().value["reason"] =
                    "only task text changed".into();
                proposed.touched.insert(task_key.clone());
            }
            11 => proposed.current.sequences.clear(),
            12 => proposed.events[0]
                .event
                .artifacts
                .push(vcp_domain::ArtifactId::new()),
            13 => {
                proposed
                    .current
                    .records
                    .get_mut(&artifact_key)
                    .unwrap()
                    .workspace = WorkspaceId::parse("other-workspace").unwrap();
                proposed.touched.insert(artifact_key.clone());
                proposed.events[0].sequence = SessionSeq::ZERO;
            }
            _ => unreachable!(),
        }
        let mut reference = state.clone();
        reference.watermark = proposed.current.watermark;
        reference.records = proposed.current.records.clone();
        reference.sequences = proposed.current.sequences.clone();
        reference.events.extend(proposed.events.clone());
        let expected = super::super::tests::reference(&reference);
        let bounded = super::super::incremental::validate_appended(
            &mut corpus.pages,
            cut.semantic(),
            &proposed,
            &mut crate::EventValidationWork::default(),
        )
        .await;
        let bounded = match bounded {
            Ok(true) => Ok(()),
            Ok(false) => {
                let mut validator = EventHistoryValidator::new(
                    reference.watermark,
                    &reference.records,
                    &reference.sequences,
                );
                validator
                    .extend(reference.events.iter().map(Ok))
                    .and_then(|_| validator.finish())
            }
            Err(error) => Err(error),
        };
        assert_eq!(
            format!("{bounded:?}"),
            format!("{expected:?}"),
            "bounded mutation {mutation}"
        );
        let before = proof.source_identity().unwrap().to_owned();
        let got = proof
            .validate(&mut corpus.pages, cut.semantic(), &proposed)
            .await;
        assert_eq!(
            format!("{got:?}"),
            format!("{expected:?}"),
            "mutation {mutation}"
        );
        assert_eq!(proof.source_identity(), Some(before.as_str()));
        proof.discard_pending();
    }
    assert!(proof.full_passes >= 7);
    assert!(proof.incremental_passes >= 7);
}

#[tokio::test]
async fn static_missing_and_corrupt_pages_fail_same_cold_replay_without_repairs() {
    let corpus = corpus(3).await;
    let mut keys = Vec::new();
    for (key, bytes) in &corpus.pages.objects {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
            continue;
        };
        if value.get("event").is_some()
            || value.get("body").is_some()
            || value.get("original_payload").is_some()
            || (value.get("transaction").is_some() && value.get("receipt").is_some())
        {
            keys.push(key.clone());
        }
    }
    assert!(keys.len() > 10);
    // All fixture event, index and publication objects, including interior
    // objects no longer reachable from the final head, are corrupted in turn.
    for key in keys {
        for missing in [false, true] {
            let mut pages = corpus.pages.clone();
            if missing {
                pages.objects.remove(&key);
            } else {
                pages.objects.get_mut(&key).unwrap()[0] ^= 1;
            }
            let mut reference_pages = pages.clone();
            let expected = cold(&mut reference_pages, &corpus, false).await;
            let got = cold(&mut pages, &corpus, true).await;
            assert!(expected.is_err(), "unread fixture object {key}");
            assert_eq!(
                format!("{got:?}"),
                format!("{expected:?}"),
                "object {key}, missing {missing}"
            );
        }
    }
}

#[tokio::test]
async fn invalid_interior_then_valid_later_state_cannot_advance_replay_proof() {
    let mut corpus = corpus(5).await;
    let cut = owner(&mut corpus.pages, &corpus.initial, &corpus.initial_commit).await;
    let mut proof = EventReplayProof::default();
    let cut = candidate(
        &mut corpus.pages,
        &cut,
        &corpus.rows[0].0,
        &corpus.rows[0].1,
        &mut proof,
    )
    .await
    .unwrap();
    let before = proof.source_identity().unwrap().to_owned();
    let mut invalid: Commit = serde_json::from_slice(&corpus.rows[1].0).unwrap();
    let crate::contract::Mutation::Put { record, .. } = &mut invalid.transaction.mutations[0]
    else {
        panic!("task update");
    };
    record.value["scope"]["session"] = "nonexistent-session".into();
    // The later original final state is independently valid; that does not
    // make the earlier bad transition admissible under either replay path.
    corpus.rows.last().unwrap().2.validate().unwrap();
    let final_commit: Commit = serde_json::from_slice(&corpus.rows.last().unwrap().0).unwrap();
    assert!(final_commit.transaction.mutations.iter().any(|mutation| {
        matches!(mutation, crate::contract::Mutation::Put { record, .. }
            if record.collection == Collection::Task && record.value["scope"]["session"] == "session")
    }), "a later transition explicitly overwrites the invalid task scope");
    let frozen = corpus.rows[0]
        .2
        .prepare_reference(&invalid.transaction)
        .err()
        .unwrap();
    let payload = canonical_bytes(&invalid).unwrap();
    let reference = full(&mut corpus.pages, &cut, &payload, &corpus.rows[1].1)
        .await
        .err()
        .unwrap();
    let got = candidate(
        &mut corpus.pages,
        &cut,
        &payload,
        &corpus.rows[1].1,
        &mut proof,
    )
    .await
    .err()
    .unwrap();
    assert_eq!(format!("{got:?}"), format!("{reference:?}"));
    assert_eq!(format!("{got:?}"), format!("{frozen:?}"));
    assert_eq!(proof.source_identity(), Some(before.as_str()));
    assert!(proof.pending.is_none());
    let next = candidate(
        &mut corpus.pages,
        &cut,
        &corpus.rows[1].0,
        &corpus.rows[1].1,
        &mut proof,
    )
    .await
    .unwrap();
    // A failed final publication likewise leaves the preceding proof intact.
    let before = proof.source_identity().unwrap().to_owned();
    let mut corrupt = corpus.pages.clone();
    corrupt.objects.remove(&corpus.rows[2].1);
    assert!(candidate(
        &mut corrupt,
        &next,
        &corpus.rows[2].0,
        &corpus.rows[2].1,
        &mut proof
    )
    .await
    .is_err());
    assert_eq!(proof.source_identity(), Some(before.as_str()));
    assert!(proof.pending.is_none());
}

#[tokio::test]
async fn generated_invalid_transitions_preserve_frozen_first_error_and_prior_proof() {
    let mut corpus = corpus(8).await;
    let mut cut = owner(&mut corpus.pages, &corpus.initial, &corpus.initial_commit).await;
    let mut source = corpus.initial.clone();
    let mut proof = EventReplayProof::default();
    for (payload, publication, expected) in &corpus.rows {
        for kind in 0..3 {
            let mut invalid: Commit = serde_json::from_slice(payload).unwrap();
            match kind {
                0 => invalid.transaction.events[0].id = source.events[0].event.id.clone(),
                1 => invalid.transaction.events[0]
                    .artifacts
                    .push(vcp_domain::ArtifactId::parse("missing-artifact").unwrap()),
                2 => {
                    invalid.transaction.events[0].session =
                        SessionId::parse("missing-session").unwrap()
                }
                _ => unreachable!(),
            }
            let before = proof.source_identity().map(str::to_owned);
            let frozen = source
                .prepare_reference(&invalid.transaction)
                .err()
                .unwrap();
            let payload = canonical_bytes(&invalid).unwrap();
            let reference = full(&mut corpus.pages, &cut, &payload, publication)
                .await
                .err()
                .unwrap();
            let live = production(&mut corpus.pages, &cut, &payload, publication)
                .await
                .err()
                .unwrap();
            assert_eq!(format!("{live:?}"), format!("{frozen:?}"));
            let got = candidate(&mut corpus.pages, &cut, &payload, publication, &mut proof)
                .await
                .err()
                .unwrap();
            assert_eq!(format!("{got:?}"), format!("{frozen:?}"));
            assert_eq!(format!("{got:?}"), format!("{reference:?}"));
            assert_eq!(proof.source_identity(), before.as_deref());
            assert!(proof.pending.is_none());
        }
        cut = candidate(&mut corpus.pages, &cut, payload, publication, &mut proof)
            .await
            .unwrap();
        source = expected.clone();
    }
}

#[tokio::test]
async fn out_of_band_backing_mutation_changes_incidental_physical_error_order() {
    let mut corpus = corpus(2).await;
    let cut = owner(&mut corpus.pages, &corpus.initial, &corpus.initial_commit).await;
    let mut proof = EventReplayProof::default();
    let cut = candidate(
        &mut corpus.pages,
        &cut,
        &corpus.rows[0].0,
        &corpus.rows[0].1,
        &mut proof,
    )
    .await
    .unwrap();
    let before = proof.source_identity().unwrap().to_owned();
    let old_blob = corpus
        .pages
        .objects
        .iter()
        .find_map(|(key, bytes)| {
            serde_json::from_slice::<EventEnvelope>(bytes)
                .ok()
                .filter(|event| event.event.id.as_str() == "created")
                .map(|_| key.clone())
        })
        .unwrap();
    corpus.pages.objects.remove(&old_blob);
    let mut bad: Commit = serde_json::from_slice(&corpus.rows[1].0).unwrap();
    bad.transaction.events[0].id = EventId::parse("created").unwrap();
    let payload = canonical_bytes(&bad).unwrap();
    let reference = full(&mut corpus.pages, &cut, &payload, &corpus.rows[1].1)
        .await
        .err()
        .unwrap();
    let got = candidate(
        &mut corpus.pages,
        &cut,
        &payload,
        &corpus.rows[1].1,
        &mut proof,
    )
    .await
    .err()
    .unwrap();
    assert!(matches!(
        reference,
        Error::Corruption("prototype missing object")
    ));
    assert!(matches!(got, Error::Corruption("event identity")));
    assert_eq!(proof.source_identity(), Some(before.as_str()));
    assert!(proof.pending.is_none());
    // Both reject, but first physical-error order differs after an external
    // mutation during the owner lifetime. This records the observation model's
    // boundary; it does not invent a requirement to reread untouched bytes on
    // every transition. Static corruption and canonical semantic error order
    // remain mandatory and are covered independently above.
}
async fn full(
    pages: &mut Memory,
    source: &DurableOwner,
    payload: &[u8],
    publication: &str,
) -> Result<DurableOwner> {
    crate::backend::current_publication::replay::replay_full_events(
        pages,
        source,
        payload,
        publication,
    )
    .await
}

async fn production(
    pages: &mut Memory,
    source: &DurableOwner,
    payload: &[u8],
    publication: &str,
) -> Result<DurableOwner> {
    crate::backend::current_publication::replay::replay_current(
        pages,
        source,
        &crate::journal_frame::Frame {
            payload: payload.into(),
            publication: Some(publication.into()),
            chain: "0".repeat(64),
            end: 0,
        },
    )
    .await
}
async fn candidate(
    pages: &mut Memory,
    source: &DurableOwner,
    payload: &[u8],
    publication: &str,
    proof: &mut EventReplayProof,
) -> Result<DurableOwner> {
    crate::backend::current_publication::replay::replay_with_event_proof(
        pages,
        source,
        payload,
        publication,
        proof,
    )
    .await
}

#[tokio::test]
async fn generated_replay_preserves_every_publication_and_reduces_event_visits() {
    let mut corpus = corpus(24).await;
    let mut cut = owner(&mut corpus.pages, &corpus.initial, &corpus.initial_commit).await;
    corpus.pages.read_only = true;
    let mut proof = EventReplayProof::default();
    for (payload, publication, expected) in &corpus.rows {
        let commit: Commit = serde_json::from_slice(payload).unwrap();
        let mut full_source = corpus.initial.clone();
        if cut.semantic().current().watermark != corpus.initial.watermark {
            full_source = corpus
                .rows
                .iter()
                .find(|(_, _, state)| state.watermark == cut.semantic().current().watermark)
                .unwrap()
                .2
                .clone();
        }
        let crate::contract::current_preparation::Outcome::Proposed(proposed) =
            crate::contract::current_preparation::propose(
                cut.semantic().current(),
                &commit.transaction,
                &mut crate::contract::current_preparation::StateHistory(&full_source),
            )
            .unwrap()
        else {
            panic!("new");
        };
        assert!(super::super::incremental::validate_appended(
            &mut corpus.pages,
            cut.semantic(),
            &proposed,
            &mut crate::EventValidationWork::default()
        )
        .await
        .unwrap());
        let reference = full(&mut corpus.pages, &cut, payload, publication)
            .await
            .unwrap();
        let live = production(&mut corpus.pages, &cut, payload, publication)
            .await
            .unwrap();
        assert_eq!(live.identity(), reference.identity());
        let next = candidate(&mut corpus.pages, &cut, payload, publication, &mut proof)
            .await
            .unwrap();
        assert_eq!(next.identity(), reference.identity());
        assert_eq!(
            next.semantic().current(),
            &crate::CurrentState::from_state(expected)
        );
        assert_eq!(proof.source_identity(), Some(next.semantic().identity()));
        cut = next;
    }
    assert_eq!(proof.full_passes, 1);
    assert_eq!(proof.full_rows, 2);
    assert_eq!(proof.incremental_passes, 23);
    assert_eq!(proof.incremental_rows, 23);
    assert_eq!(proof.ids.len(), 25);
    // Frozen complete validation visits 2+3+...+25 = 324 events. The event
    // phase prototype visits 25; all other historical phases remain full.
}
