// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{backend::Backend, BackendKind};
#[path = "../tests/common/mod.rs"]
mod common;

fn history() -> (State, Commit, State, Commit) {
    let (first, initial) = State::default().prepare(&common::initial()).unwrap();
    let mut event = initial.transaction.events[0].clone();
    event.id = EventId::new();
    let transaction = Transaction {
        id: TransactionId::new(),
        expected_watermark: first.watermark,
        mutations: Vec::new(),
        events: vec![event],
        command: None,
    };
    let (second, next) = first.prepare(&transaction).unwrap();
    (first, initial, second, next)
}

#[test]
fn owned_and_public_replay_match_and_public_failure_keeps_the_prefix() {
    let (first, _, second, next) = history();
    let mut public = first.clone();
    public.replay(&next).unwrap();
    assert_eq!(public, second);
    assert_eq!(first.clone().into_replayed(&next).unwrap(), second);
    let mut invalid = next;
    invalid.transaction.events[0].task = Some(TaskId::parse("missing-task").unwrap());
    invalid.receipt.digest = digest_bytes(&canonical_bytes(&invalid.transaction).unwrap());
    let mut public = first.clone();
    assert!(matches!(
        public.replay(&invalid),
        Err(Error::Conflict("record not found"))
    ));
    assert_eq!(public, first);
    assert!(matches!(
        first.into_replayed(&invalid),
        Err(Error::Conflict("record not found"))
    ));
}

#[test]
fn prior_receipt_view_excludes_only_the_new_transaction() {
    let (first, initial, second, next) = history();
    let prior = RecordView {
        watermark: first.watermark,
        records: &first.records,
        transactions: &second.transactions,
        excluded_transaction: Some(&next.transaction.id),
    };
    assert_eq!(
        prior.transaction(&initial.transaction.id),
        first.transactions.get(&initial.transaction.id)
    );
    assert!(prior.transaction(&next.transaction.id).is_none());
    assert!(second
        .record_view()
        .transaction(&next.transaction.id)
        .is_some());
}

#[tokio::test]
async fn both_backends_reject_semantically_invalid_commits_with_valid_checksums() {
    for kind in [BackendKind::Sqlite, BackendKind::Files] {
        let temporary = tempfile::tempdir().unwrap();
        let (first, initial, second, mut invalid) = history();
        invalid.transaction.events[0].task = Some(TaskId::parse("missing-task").unwrap());
        invalid.receipt.digest = digest_bytes(&canonical_bytes(&invalid.transaction).unwrap());
        // The physical writer intentionally receives an invalid semantic commit:
        // its envelope, transaction digest, journal checksum and durable marker
        // remain valid, so reopen must reach and reject per-commit validation.
        let (mut backend, _, _) = Backend::open(temporary.path(), kind).await.unwrap();
        backend.append(&initial, &first, |_| {}).await.unwrap();
        backend.append(&invalid, &second, |_| {}).await.unwrap();
        backend.close().await.unwrap();
        assert!(matches!(
            Backend::open(temporary.path(), kind).await,
            Err(Error::Conflict("record not found"))
        ));
    }
}

#[tokio::test]
async fn later_valid_state_and_checkpoint_cannot_hide_invalid_interior_transition() {
    for kind in [BackendKind::Sqlite, BackendKind::Files] {
        let temporary = tempfile::tempdir().unwrap();
        let (first, initial, second, mut invalid) = history();
        invalid.transaction.events[0].task = Some(TaskId::parse("missing-task").unwrap());
        invalid.receipt.digest = digest_bytes(&canonical_bytes(&invalid.transaction).unwrap());
        let mut next_event = initial.transaction.events[0].clone();
        next_event.id = EventId::new();
        let next = Transaction {
            id: TransactionId::new(),
            expected_watermark: second.watermark,
            mutations: Vec::new(),
            events: vec![next_event],
            command: None,
        };
        let (third, final_commit) = second.prepare(&next).unwrap();
        let (mut backend, _, _) = Backend::open(temporary.path(), kind).await.unwrap();
        backend.append(&initial, &first, |_| {}).await.unwrap();
        backend.append(&invalid, &second, |_| {}).await.unwrap();
        backend.append(&final_commit, &third, |_| {}).await.unwrap();
        backend.checkpoint(&third).unwrap();
        backend.close().await.unwrap();
        assert!(matches!(
            Backend::open(temporary.path(), kind).await,
            Err(Error::Conflict("record not found"))
        ));
    }
}

#[test]
fn generated_observed_replay_agrees_with_reference_at_every_transaction() {
    let mut reference = State::default();
    let mut observed = State::default();
    let mut size = StateSize::measure(&observed).unwrap();
    let mut diagnostics = crate::StoreDiagnostics::new(BackendKind::Files);
    let mut transaction = common::initial();
    for index in 0..80 {
        if index > 0 {
            let mut event = common::initial().events.remove(0);
            event.id = EventId::parse(format!("generated-event-{index}")).unwrap();
            transaction = Transaction {
                id: TransactionId::parse(format!("generated-transaction-{index}")).unwrap(),
                expected_watermark: reference.watermark,
                mutations: vec![],
                events: vec![event],
                command: None,
            };
            let projection_id = "generated-projection";
            let projection_key = key(Collection::Projection, projection_id);
            match index % 4 {
                1 | 2 => {
                    let prior = reference.records.get(&projection_key);
                    let revision =
                        prior.map_or(Revision::ZERO, |record| record.revision.next().unwrap());
                    transaction.mutations.push(Mutation::Put {
                        expected: prior.map(|record| record.revision),
                        record: Record::typed(
                            Collection::Projection,
                            projection_id,
                            common::workspace().id,
                            revision,
                            &serde_json::json!({"schema_version": 1, "text": "quoted\"\n雪".repeat(index), "step": index}),
                        ).unwrap(),
                    });
                }
                3 => transaction.mutations.push(Mutation::DropProjection {
                    id: projection_id.to_owned(),
                    expected: reference.records[&projection_key].revision,
                }),
                _ => {}
            }
            if index % 7 == 0 {
                let mut session = common::session();
                session.id = SessionId::parse(format!("generated-session-{index}")).unwrap();
                transaction.events[0].session = session.id.clone();
                transaction.events[0].task = None;
                transaction.mutations.push(Mutation::Put {
                    expected: None,
                    record: Record::typed(
                        Collection::Session,
                        session.id.to_string(),
                        session.workspace.clone(),
                        session.revision,
                        &session,
                    )
                    .unwrap(),
                });
            }
            if index % 3 == 0 {
                let mut command = common::initial().command.unwrap();
                command.command = CommandId::parse(format!("generated-command-{index}")).unwrap();
                command.session = transaction.events[0].session.clone();
                transaction.events[0].correlation = command.command.clone();
                transaction.command = Some(command);
            }
        }
        let mut invalid = transaction.clone();
        invalid.events[0].task = Some(TaskId::parse("absent-task").unwrap());
        let expected_error = reference.prepare(&invalid).unwrap_err().to_string();
        let observed_error = observed
            .prepare_observed(&invalid, &mut diagnostics, &mut size)
            .unwrap_err()
            .to_string();
        assert_eq!(expected_error, observed_error, "invalid step {index}");
        assert_eq!(observed, reference, "rejection mutated step {index}");
        let (next, commit) = reference.prepare(&transaction).unwrap();
        observed = observed
            .into_replayed_observed(&commit, &mut diagnostics, &mut size)
            .unwrap();
        reference = next;
        assert_eq!(observed, reference, "accepted step {index}");
        assert_eq!(
            size.bytes(),
            canonical_bytes(&reference).unwrap().len(),
            "size step {index}"
        );
        reference.validate().unwrap();
        let (duplicate, duplicate_commit) = observed
            .prepare_observed(&transaction, &mut diagnostics, &mut size)
            .unwrap();
        assert_eq!(duplicate, reference, "duplicate step {index}");
        assert_eq!(duplicate_commit, commit);
    }
}
