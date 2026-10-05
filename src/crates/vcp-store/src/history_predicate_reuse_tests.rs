// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::contract::current_preparation::{self, ProposedTransition, StateHistory};
use crate::history_predicate_reuse::{ingestion_unchanged, redaction_unchanged};

fn seed() -> (State, Cursor, Job) {
    let mut source = State::default()
        .prepare_reference(&common::initial())
        .unwrap()
        .0;
    let (cursor, job) = ingestion(&source);
    source = source
        .prepare_reference(&transaction(
            &source,
            vec![
                put(Collection::Claim, cursor.id.to_string(), &cursor),
                put(Collection::Claim, job.id.to_string(), &job),
            ],
        ))
        .unwrap()
        .0;
    // A retained rewritten seed must be fully admitted, including the existing
    // redacted payload and its workspace epoch. Ordinary transactions cannot
    // introduce an explicitly redacted envelope.
    source
        .records
        .get_mut(&key(Collection::Workspace, "workspace"))
        .unwrap()
        .value["deletion"] = "1".into();
    source.events[0] =
        vcp_protocol::redaction::event(&source.events[0], DeletionEpoch::new(1)).unwrap();
    source.validate().unwrap();
    (source, cursor, job)
}
fn replacement(
    source: &State,
    collection: Collection,
    id: &str,
    edit: impl FnOnce(&mut serde_json::Value),
) -> Mutation {
    let mut record = source.records[&key(collection, id)].clone();
    let prior = record.revision;
    record.revision = prior.next().unwrap();
    record.value["revision"] = serde_json::to_value(record.revision).unwrap();
    edit(&mut record.value);
    Mutation::Put {
        expected: Some(prior),
        record,
    }
}
fn proposed(source: &State, tx: &Transaction) -> ProposedTransition {
    let current = crate::CurrentState::from_state(source);
    let current_preparation::Outcome::Proposed(proposed) =
        current_preparation::propose(&current, tx, &mut StateHistory(source)).unwrap()
    else {
        panic!("new transaction");
    };
    proposed
}
async fn compare_all(
    source: &State,
    pages: &mut Memory,
    cut: &AdmittedCut,
    tx: &Transaction,
) -> Option<current_transition::PreparedCurrent> {
    let frozen = source.prepare_reference(tx).map_err(|e| e.to_string());
    let full = current_transition::prepare_full_events(pages, cut, tx).await;
    let fast = current_transition::prepare(pages, cut, tx).await;
    match frozen {
        Err(expected) => {
            assert_eq!(full.err().unwrap().to_string(), expected);
            assert_eq!(fast.err().unwrap().to_string(), expected);
            None
        }
        Ok((expected_state, expected_commit)) => {
            for candidate in [&full, &fast] {
                let Ok(current_transition::Outcome::Prepared(candidate)) = candidate else {
                    panic!("expected new commit");
                };
                assert_eq!(candidate.commit(), &expected_commit);
                assert_eq!(
                    candidate.proposed().current,
                    crate::CurrentState::from_state(&expected_state)
                );
            }
            let current_transition::Outcome::Prepared(full) = full.unwrap() else {
                unreachable!()
            };
            let current_transition::Outcome::Prepared(fast) = fast.unwrap() else {
                unreachable!()
            };
            assert_eq!(
                full.history_work().redaction_validation.rows_examined,
                (source.events.len() + tx.events.len()) as u64
            );
            assert_eq!(
                full.history_work().ingestion_validation.rows_examined,
                (source.events.len() + tx.events.len()) as u64
            );
            Some(fast)
        }
    }
}

#[tokio::test]
async fn history_reuse_generated_transitions_match_full_reference_and_exact_work() {
    let (mut source, cursor, _) = seed();
    let (mut pages, mut cut) = admitted(&source).await;
    let mut fast_rows = 0;
    let mut full_rows = 0;
    for index in 0..20 {
        let mut tx = transaction(
            &source,
            vec![replacement(&source, Collection::Task, "task", |v| {
                v["reason"] = format!("same scope, new reason {index}").into();
            })],
        );
        let mut event = common::initial().events[0].clone();
        event.id = EventId::parse(format!("reuse-new-{index}")).unwrap();
        tx.events.push(event);
        let prior_identity = cut.identity().to_owned();
        // A changed cursor crosses an unprocessed matching origin; it must
        // use the full pass and preserve the old first semantic rejection.
        if index > 0 {
            let mut bad = tx.clone();
            bad.id = TransactionId::new();
            bad.mutations.push(replacement(
                &source,
                Collection::Claim,
                cursor.id.as_str(),
                |v| {
                    v["after"] = (source.events.len() as u64).to_string().into();
                    v["scanned_through"] =
                        serde_json::to_value(source.events.last().unwrap().watermark).unwrap();
                },
            ));
            assert!(compare_all(&source, &mut pages, &cut, &bad).await.is_none());
        }
        for kind in 0..4 {
            let mut bad = tx.clone();
            bad.id = TransactionId::new();
            match kind {
                0 => bad.events[0].id = source.events[0].event.id.clone(),
                1 => bad.mutations.push(replacement(
                    &source,
                    Collection::Workspace,
                    "workspace",
                    |v| v["deletion"] = "0".into(),
                )),
                2 => {
                    let Mutation::Put { record, .. } = &mut bad.mutations[0] else {
                        unreachable!()
                    };
                    record.value["root"] = "missing-root".into();
                }
                3 => {
                    let mut record =
                        source.records[&key(Collection::Claim, cursor.id.as_str())].clone();
                    record.collection = Collection::Projection;
                    bad.mutations.push(Mutation::Put {
                        expected: None,
                        record,
                    });
                }
                _ => unreachable!(),
            }
            assert!(compare_all(&source, &mut pages, &cut, &bad).await.is_none());
            assert_eq!(cut.identity(), prior_identity);
        }
        let prepared = compare_all(&source, &mut pages, &cut, &tx).await.unwrap();
        let work = prepared.history_work();
        assert_eq!(work.redaction_validation.prefix_reuses, 1);
        assert_eq!(work.redaction_validation.rows_examined, 1);
        assert_eq!(work.ingestion_validation.prefix_reuses, 1);
        assert_eq!(work.ingestion_validation.rows_examined, 0);
        assert_eq!(work.full_passes, 0);
        fast_rows +=
            work.redaction_validation.rows_examined + work.ingestion_validation.rows_examined;
        full_rows += (source.events.len() as u64 + 1) * 2;
        cut = cut.advance(&mut pages, &prepared).await.unwrap();
        source = source.prepare_reference(&tx).unwrap().0;
        let current_transition::Outcome::Duplicate(receipt) =
            current_transition::prepare(&mut pages, &cut, &tx)
                .await
                .unwrap()
        else {
            panic!("duplicate");
        };
        assert_eq!(receipt, prepared.commit().receipt);
    }
    assert_eq!(fast_rows, 20);
    assert_eq!(full_rows, 460);
}

#[tokio::test]
async fn history_reuse_dependency_inventory_falls_back_for_ambiguous_records() {
    let (source, cursor, job) = seed();
    let (_pages, cut) = admitted(&source).await;
    let tx = transaction(&source, vec![]);
    for (collection, id, field, value) in [
        (
            Collection::Task,
            "task",
            "reason",
            serde_json::json!("new narrative"),
        ),
        (Collection::Task, "task", "root", serde_json::json!("other")),
        (
            Collection::Task,
            "task",
            "parent",
            serde_json::json!("other"),
        ),
        (Collection::Task, "task", "scope", serde_json::json!({})),
        (
            Collection::Workspace,
            "workspace",
            "deletion",
            serde_json::json!("2"),
        ),
        (
            Collection::Workspace,
            "workspace",
            "deletion",
            serde_json::json!("0"),
        ),
        (
            Collection::Workspace,
            "workspace",
            "deletion",
            serde_json::json!({}),
        ),
        (
            Collection::Claim,
            cursor.id.as_str(),
            "after",
            serde_json::json!("0"),
        ),
        (
            Collection::Claim,
            job.id.as_str(),
            "results",
            serde_json::json!(["result"]),
        ),
    ] {
        let mut next = proposed(&source, &tx);
        let key = key(collection, id);
        next.current.records.get_mut(&key).unwrap().value[field] = value;
        next.touched.insert(key);
        assert_eq!(
            redaction_unchanged(&cut, &next),
            collection != Collection::Workspace
        );
        assert_eq!(
            ingestion_unchanged(&cut, &next),
            collection == Collection::Workspace || field == "reason"
        );
    }
    for collection in [
        Collection::Workspace,
        Collection::Task,
        Collection::Claim,
        Collection::Projection,
    ] {
        let mut next = proposed(&source, &tx);
        let id = match collection {
            Collection::Workspace => "workspace",
            Collection::Task => "task",
            Collection::Claim => cursor.id.as_str(),
            _ => "new-result",
        };
        let record_key = key(collection, id);
        if collection == Collection::Projection {
            let mut row = source.records[&key(Collection::Claim, cursor.id.as_str())].clone();
            row.collection = collection;
            row.id = id.into();
            next.current.records.insert(record_key.clone(), row);
        } else {
            next.current.records.remove(&record_key);
        }
        next.touched.insert(record_key);
        assert_eq!(
            redaction_unchanged(&cut, &next),
            collection != Collection::Workspace
        );
        assert_eq!(
            ingestion_unchanged(&cut, &next),
            collection == Collection::Workspace
        );
    }
    // Newly supplied envelopes still use the exact protocol redaction checks.
    let mut event = source.events[0].clone();
    event.event.data = serde_json::json!({"resurrected":true});
    assert!(matches!(
        crate::redaction_contract::validate_event_rows(&source.records, [Ok(&event)]),
        Err(Error::Corruption("explicit event redaction"))
    ));
}

#[tokio::test]
async fn history_reuse_redaction_fallback_keeps_full_error_and_io_order() {
    let (source, _, _) = seed();
    let (mut pages, cut) = admitted(&source).await;
    for deletion in [0, 1, 2] {
        let tx = transaction(
            &source,
            vec![replacement(
                &source,
                Collection::Workspace,
                "workspace",
                |v| {
                    v["deletion"] = deletion.to_string().into();
                },
            )],
        );
        let before = cut.identity().to_owned();
        let actual = compare_all(&source, &mut pages, &cut, &tx).await;
        if deletion == 0 {
            assert!(actual.is_none());
        } else {
            let work = actual.unwrap().history_work();
            assert_eq!(
                work.redaction_validation.full_passes,
                u64::from(deletion != 1)
            );
            assert_eq!(
                work.redaction_validation.prefix_reuses,
                u64::from(deletion == 1)
            );
            assert_eq!(work.ingestion_validation.prefix_reuses, 1);
        }
        assert_eq!(cut.identity(), before);
    }
    let tx = transaction(
        &source,
        vec![replacement(
            &source,
            Collection::Workspace,
            "workspace",
            |v| v["deletion"] = "2".into(),
        )],
    );
    // The changed epoch requires fresh historical bytes; loss of those bytes
    // is not an empty/successful full pass and cannot mint a certificate.
    pages.0.clear();
    assert!(current_transition::prepare(&mut pages, &cut, &tx)
        .await
        .is_err());
    assert_eq!(cut.current().watermark, source.watermark);
}

#[tokio::test]
async fn history_reuse_changed_result_and_proposal_provenance_cannot_reuse_admission() {
    use vcp_domain::{memory::Outcome as MemoryOutcome, redaction::*};
    let (mut source, _, mut job) = seed();
    let proposal = RedactedProposal {
        document_type: PROPOSAL.into(),
        schema_version: 1,
        id: ProposalId::parse("reuse-proposal").unwrap(),
        scope: job.scope.clone(),
        revision: Revision::ZERO,
        deletion: DeletionEpoch::new(1),
        original_digest: "a".repeat(64),
        payload_digest: "b".repeat(64),
        command: CommandId::parse("create").unwrap(),
        claim: ClaimId::parse("claim").unwrap(),
        actor: ActorId::parse("human").unwrap(),
        sources: Sources {
            origins: vec![job.origin.clone()],
            ..Sources::default()
        },
        predecessor: None,
        outcome: MemoryOutcome::Rejected,
        recorded_at: Timestamp::new(100),
        origin_output_keys: vec![],
        extractor_digest: digest_bytes(job.extractor.identity().as_bytes()),
    };
    let result = RedactedResult {
        document_type: RESULT.into(),
        schema_version: 1,
        id: proposal.command.clone(),
        scope: proposal.scope.clone(),
        revision: Revision::ZERO,
        deletion: proposal.deletion,
        original_digest: "d".repeat(64),
        proposal: proposal.id.clone(),
        payload_digest: proposal.payload_digest.clone(),
        transaction: TransactionId::parse("initial").unwrap(),
        version: None,
        intent: None,
        outcome: proposal.outcome,
        memory_seq: MemorySeq::ZERO,
    };
    job.results = vec![result.id.clone()];
    job.state = JobState::Completed;
    job.attempts = Units::new(1);
    for record in [
        Record::typed(
            Collection::Claim,
            proposal.id.to_string(),
            proposal.scope.workspace.clone(),
            proposal.revision,
            &proposal,
        )
        .unwrap(),
        Record::typed(
            Collection::Projection,
            result.id.to_string(),
            result.scope.workspace.clone(),
            result.revision,
            &result,
        )
        .unwrap(),
        Record::typed(
            Collection::Claim,
            job.id.to_string(),
            job.scope.workspace.clone(),
            job.revision,
            &job,
        )
        .unwrap(),
    ] {
        source.records.insert(record.key(), record);
    }
    source.validate().unwrap();
    let (mut pages, cut) = admitted(&source).await;
    let tx = transaction(&source, vec![]);
    assert!(compare_all(&source, &mut pages, &cut, &tx).await.is_some());
    for kind in 0..4 {
        let mut changed = proposed(&source, &tx);
        let record_key = if kind < 2 {
            key(Collection::Claim, proposal.id.as_str())
        } else {
            key(Collection::Projection, result.id.as_str())
        };
        let record = changed.current.records.get_mut(&record_key).unwrap();
        match kind {
            0 => record.value["sources"]["origins"] = serde_json::json!([]),
            1 => record.value["extractor_digest"] = "f".repeat(64).into(),
            2 => record.value["proposal"] = "absent".into(),
            3 => record.value["scope"]["task"] = "other".into(),
            _ => unreachable!(),
        }
        changed.touched.insert(record_key);
        assert!(!ingestion_unchanged(&cut, &changed));
        // The independent complete predicate rejects each changed dependency.
        // These redacted records are immutable to normal Put; testing the
        // dependency proof directly also guards rewrite/candidate integration.
        let inputs = ingestion_contract::Inputs::new((&changed.current).into()).unwrap();
        let mut history = inputs.history(source.events.len());
        history.extend(source.events.iter().map(Ok)).unwrap();
        assert!(inputs.finish(history.finish().unwrap()).is_err());
    }
}

#[tokio::test]
async fn history_reuse_cold_replay_rejects_invalid_interior_epoch_before_later_repair() {
    use crate::{
        durable_owner::DurableOwner, original_commits::OriginalCommits, replay_base::ReplayBase,
    };
    let (source, _, _) = seed();
    let temporary = tempfile::tempdir().unwrap();
    ReplayBase::write(temporary.path(), &source, &source, &[]).unwrap();
    let base = ReplayBase::load(temporary.path()).unwrap().unwrap();
    let original = std::fs::read(temporary.path().join("replay-base.json")).unwrap();
    let mut pages = Memory::default();
    let originals = OriginalCommits::from_validated_base(&mut pages, Some((&base, &original)))
        .await
        .unwrap();
    let catalog = Catalog::from_validated_state(&mut pages, &source)
        .await
        .unwrap();
    let owner = DurableOwner::from_replayed(&mut pages, &source, Some(&base), catalog, originals)
        .await
        .unwrap();
    let valid = transaction(
        &source,
        vec![replacement(
            &source,
            Collection::Workspace,
            "workspace",
            |v| v["deletion"] = "2".into(),
        )],
    );
    let crate::durable_owner::Outcome::Prepared(prepared) =
        owner.prepare(&mut pages, &valid).await.unwrap()
    else {
        panic!("new");
    };
    let payload = canonical_bytes(prepared.commit()).unwrap();
    let (next, publication) =
        crate::history_publication::stage(&mut pages, &owner, &prepared, &payload)
            .await
            .unwrap();
    let next_state = source.prepare_reference(&valid).unwrap().0;
    let repair = transaction(
        &next_state,
        vec![replacement(
            &next_state,
            Collection::Workspace,
            "workspace",
            |v| v["deletion"] = "3".into(),
        )],
    );
    let repaired = next_state.prepare_reference(&repair).unwrap().0;
    repaired.validate().unwrap();
    let crate::durable_owner::Outcome::Prepared(last) =
        next.prepare(&mut pages, &repair).await.unwrap()
    else {
        panic!("new");
    };
    crate::history_publication::stage(
        &mut pages,
        &next,
        &last,
        &canonical_bytes(last.commit()).unwrap(),
    )
    .await
    .unwrap();
    let mut bad = prepared.commit().clone();
    let Mutation::Put { record, .. } = &mut bad.transaction.mutations[0] else {
        unreachable!()
    };
    record.value["deletion"] = "0".into();
    let frozen_error = source
        .prepare_reference(&bad.transaction)
        .err()
        .unwrap()
        .to_string();
    let bad = canonical_bytes(&bad).unwrap();
    let identity = owner.identity().to_owned();
    let full = crate::backend::current_publication::replay::replay_full_events(
        &mut pages,
        &owner,
        &bad,
        &publication,
    )
    .await
    .err()
    .unwrap();
    let frame = crate::journal_frame::Frame {
        payload: bad,
        publication: Some(publication.clone()),
        chain: String::new(),
        end: 0,
    };
    let fast =
        crate::backend::current_publication::replay::replay_current(&mut pages, &owner, &frame)
            .await
            .err()
            .unwrap();
    assert_eq!(full.to_string(), frozen_error);
    assert_eq!(fast.to_string(), frozen_error);
    assert_eq!(owner.identity(), identity);
    // Failed replay did not modify prior admission or canonical bytes; the
    // unchanged valid original still replays through ComparePages afterward.
    let frame = crate::journal_frame::Frame {
        payload,
        publication: Some(publication),
        chain: String::new(),
        end: 0,
    };
    let replayed =
        crate::backend::current_publication::replay::replay_current(&mut pages, &owner, &frame)
            .await
            .unwrap();
    assert_eq!(replayed.identity(), next.identity());
}
