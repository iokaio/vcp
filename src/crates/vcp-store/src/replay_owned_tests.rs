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
