// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    backend::current_publication::open::Opened, canonical_lock::CanonicalLock,
    durable_owner::Outcome,
};
#[path = "../tests/common/mod.rs"]
mod common;

fn next_transaction(watermark: Watermark, index: usize) -> Transaction {
    let mut tx = common::initial();
    tx.id = TransactionId::parse(format!("owner-{index}")).unwrap();
    tx.expected_watermark = watermark;
    tx.mutations.clear();
    tx.events[0].id = EventId::parse(format!("owner-event-{index}")).unwrap();
    tx.events[0].correlation = CommandId::parse(format!("owner-command-{index}")).unwrap();
    tx.command.as_mut().unwrap().command = tx.events[0].correlation.clone();
    tx.command.as_mut().unwrap().digest = format!("{index:064x}");
    tx
}

#[tokio::test]
async fn current_owner_retains_more_than_legacy_state_capacity_and_cold_replays_bounded_history() {
    const PAYLOAD: usize = 6 * 1024 * 1024;
    const COMMITS: usize = 11;
    assert!(PAYLOAD * COMMITS > crate::contract::MAX_STATE_BYTES);
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        let mut store = Store::open(&root, kind, &[]).await.unwrap();
        store.transact(common::initial()).await.unwrap();
        let staged = store.stage_current().await.unwrap();
        let origin = staged.origin_digest.clone();
        drop(staged);
        // No archival State survives this boundary; the native owner retains
        // current records, fixed-size roots and native ownership only.
        store.close().await.unwrap();
        let mut owner = Opened::open(
            CanonicalLock::acquire(&root, &[]).unwrap(),
            kind,
            &origin,
            &mut crate::StoreDiagnostics::new(kind),
        )
        .await
        .unwrap();
        for index in 0..COMMITS {
            let mut tx = next_transaction(owner.current().watermark, index);
            tx.events[0].data = serde_json::json!({"payload": "x".repeat(PAYLOAD), "index": index});
            let receipt = owner.transact(tx, &|_| {}).await.unwrap();
            assert_eq!(receipt.watermark.get(), index as u64 + 2);
            assert!(canonical_bytes(&owner.current()).unwrap().len() < 16 * 1024);
        }
        assert_eq!(
            owner.history_event_count().await.unwrap(),
            COMMITS as u64 + 1
        );
        assert!(matches!(owner.archive_state().await, Err(Error::Limit(_))));
        let digest = owner.logical_digest().await.unwrap();
        owner.close().await.unwrap();
        // Cold open must replay and validate every original transition, even
        // though their logical historical State can no longer fit the v1 DTO.
        let reopened = Opened::open(
            CanonicalLock::acquire(&root, &[]).unwrap(),
            kind,
            &origin,
            &mut crate::StoreDiagnostics::new(kind),
        )
        .await
        .unwrap();
        assert_eq!(reopened.logical_digest().await.unwrap(), digest);
        assert_eq!(reopened.current().watermark.get(), COMMITS as u64 + 1);
        for index in 0..COMMITS {
            let page = reopened
                .history_events(Some(index as u64), 1)
                .await
                .unwrap();
            assert_eq!(page.len(), 1);
            assert_eq!(page[0].event.data["index"], index);
            assert_eq!(
                page[0].event.data["payload"].as_str().unwrap().len(),
                PAYLOAD
            );
            assert_eq!(
                page[0].event.id,
                EventId::parse(format!("owner-event-{index}")).unwrap()
            );
        }
        assert!(reopened
            .history_events(Some(COMMITS as u64), 1)
            .await
            .unwrap()
            .is_empty());
        assert!(matches!(
            reopened.archive_state().await,
            Err(Error::Limit(_))
        ));
        reopened.close().await.unwrap();
    }
}

#[tokio::test]
async fn current_owner_methods_match_reference_without_resident_history_and_reopen() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        let mut store = Store::open(&root, kind, &[]).await.unwrap();
        store.transact(common::initial()).await.unwrap();
        let mut reference = store.state().clone();
        let staged = store.stage_current().await.unwrap();
        let origin = staged.origin_digest.clone();
        drop(staged);
        store.close().await.unwrap();
        let mut owner = Opened::open(
            CanonicalLock::acquire(&root, &[]).unwrap(),
            kind,
            &origin,
            &mut crate::StoreDiagnostics::new(kind),
        )
        .await
        .unwrap();
        let old_current = owner.current_state();
        let mut old_snapshot = owner.snapshot().unwrap();
        let old_reference = reference.clone();
        assert!(std::sync::Arc::ptr_eq(&old_current, &owner.current_state()));
        for index in 0..4 {
            let tx = next_transaction(reference.watermark, index);
            let (next, commit) = reference.prepare_reference(&tx).unwrap();
            let receipt = commit.receipt;
            assert_eq!(owner.transact(tx.clone(), &|_| {}).await.unwrap(), receipt);
            assert_eq!(owner.archive_state().await.unwrap(), next);
            assert_eq!(owner.transact(tx.clone(), &|_| {}).await.unwrap(), receipt);
            assert_eq!(
                owner.transaction_receipt(&tx.id).await.unwrap(),
                Some(receipt.clone())
            );
            let command = tx.command.as_ref().unwrap();
            assert_eq!(
                owner
                    .command_receipt_by_id(&command.workspace, &command.command)
                    .await
                    .unwrap(),
                receipt.command
            );
            assert_eq!(
                owner
                    .command_receipt(&command.workspace, &command.command, &command.digest)
                    .await
                    .unwrap(),
                receipt.command
            );
            assert_eq!(
                owner
                    .scoped_command_receipt(&command.workspace, &command.session, &command.command)
                    .await
                    .unwrap(),
                receipt.command
            );
            assert!(owner
                .command_receipt(&command.workspace, &command.command, &"f".repeat(64))
                .await
                .is_err());
            let mut rejected = next_transaction(next.watermark, 100 + index);
            rejected.expected_watermark = Watermark::ZERO;
            assert_eq!(
                owner
                    .transact(rejected.clone(), &|_| {})
                    .await
                    .unwrap_err()
                    .to_string(),
                next.prepare_reference(&rejected).unwrap_err().to_string()
            );
            assert_eq!(owner.current().watermark, next.watermark);
            reference = next;
        }
        assert_eq!(old_current.watermark, Watermark::new(1));
        assert_eq!(old_snapshot.current().watermark, old_reference.watermark);
        assert!(!std::sync::Arc::ptr_eq(
            &old_current,
            &owner.current_state()
        ));
        assert_eq!(
            owner.history_event_count().await.unwrap(),
            reference.events.len() as u64
        );
        let mut events = Vec::new();
        let mut after = None;
        loop {
            let page = owner.history_events(after, 2).await.unwrap();
            if page.is_empty() {
                break;
            }
            events.extend(page);
            after = Some(events.len() as u64 - 1);
        }
        assert_eq!(events, *reference.events);
        for (ordinal, event) in events.iter().enumerate() {
            assert_eq!(
                owner.history_event_at(ordinal as u64).await.unwrap(),
                Some(event.clone())
            );
            assert_eq!(
                owner.history_event(&event.event.id).await.unwrap(),
                Some(event.clone())
            );
        }
        assert!(owner
            .history_event_at(events.len() as u64)
            .await
            .unwrap()
            .is_none());
        let commands: std::collections::BTreeMap<_, _> = owner
            .history_commands(None, 64)
            .await
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(commands, *reference.commands);
        assert!(owner
            .history_artifact_events(
                &common::workspace().id,
                &vcp_domain::ArtifactId::new(),
                None,
                64
            )
            .await
            .unwrap()
            .is_empty());
        let digest = owner.logical_digest().await.unwrap();
        assert_eq!(digest, digest_bytes(&canonical_bytes(&reference).unwrap()));
        assert_eq!(owner.diagnostics.duplicate_transactions, 4);
        owner.close().await.unwrap();
        let reopened = Opened::open(
            CanonicalLock::acquire(&root, &[]).unwrap(),
            kind,
            &origin,
            &mut crate::StoreDiagnostics::new(kind),
        )
        .await
        .unwrap();
        assert_eq!(reopened.archive_state().await.unwrap(), reference);
        assert_eq!(reopened.logical_digest().await.unwrap(), digest);
        assert_eq!(old_snapshot.archive_state().await.unwrap(), old_reference);
        assert_eq!(
            old_snapshot.logical_digest().await.unwrap(),
            digest_bytes(&canonical_bytes(&old_reference).unwrap())
        );
        old_snapshot.close().await.unwrap();
        reopened.close().await.unwrap();
    }
}

#[tokio::test]
async fn current_owner_indeterminate_append_poison_requires_reopen() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        let mut store = Store::open(&root, kind, &[]).await.unwrap();
        store.transact(common::initial()).await.unwrap();
        let reference = store.state().clone();
        let staged = store.stage_current().await.unwrap();
        let origin = staged.origin_digest.clone();
        drop(staged);
        store.close().await.unwrap();
        let mut owner = Opened::open(
            CanonicalLock::acquire(&root, &[]).unwrap(),
            kind,
            &origin,
            &mut crate::StoreDiagnostics::new(kind),
        )
        .await
        .unwrap();
        match &mut owner.backend {
            Backend::Files(journal) => journal.write_budget = Some(23),
            Backend::Sqlite(db) => {
                sqlx::query("CREATE TRIGGER injected_owner_failure BEFORE INSERT ON commits BEGIN SELECT RAISE(ABORT,'injected owner failure'); END")
                    .execute(db).await.unwrap();
            }
        }
        let tx = next_transaction(reference.watermark, 0);
        assert!(owner.transact(tx.clone(), &|_| {}).await.is_err());
        assert!(owner.poisoned);
        assert!(owner.transact(tx.clone(), &|_| {}).await.is_err());
        assert!(owner.archive_state().await.is_err());
        assert!(owner.history_event_count().await.is_err());
        assert!(owner.history().await.is_err());
        if let Backend::Sqlite(db) = &mut owner.backend {
            sqlx::query("DROP TRIGGER injected_owner_failure")
                .execute(db)
                .await
                .unwrap();
        }
        owner.close().await.unwrap();
        let mut reopened = Opened::open(
            CanonicalLock::acquire(&root, &[]).unwrap(),
            kind,
            &origin,
            &mut crate::StoreDiagnostics::new(kind),
        )
        .await
        .unwrap();
        assert_eq!(reopened.archive_state().await.unwrap(), reference);
        let (expected, commit) = reference.prepare_reference(&tx).unwrap();
        let receipt = commit.receipt;
        assert_eq!(reopened.transact(tx, &|_| {}).await.unwrap(), receipt);
        assert_eq!(reopened.archive_state().await.unwrap(), expected);
        reopened.close().await.unwrap();
    }
}

#[tokio::test]
async fn native_factory_replays_without_resident_store_then_appends_and_reopens_exact_history() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        let mut store = Store::open(&root, kind, &[]).await.unwrap();
        store.transact(common::initial()).await.unwrap();
        let expected = store.state().clone();
        let staged = store.stage_current().await.unwrap();
        let digest = staged.origin_digest.clone();
        let identity = staged.owner.identity().to_owned();
        drop(staged);
        store.close().await.unwrap();
        let lock = CanonicalLock::acquire(&root, &[]).unwrap();
        let mut opened = Opened::open(lock, kind, &digest, &mut crate::StoreDiagnostics::new(kind))
            .await
            .unwrap();
        assert_eq!(opened.owner.identity(), identity);
        assert!(Store::open(&root, kind, &[]).await.is_err());
        let mut tx = common::initial();
        tx.id = TransactionId::new();
        tx.expected_watermark = expected.watermark;
        tx.mutations.clear();
        tx.events[0].id = EventId::new();
        tx.events[0].correlation = CommandId::new();
        tx.command.as_mut().unwrap().command = tx.events[0].correlation.clone();
        tx.command.as_mut().unwrap().digest = "f".repeat(64);
        let expected = expected.prepare_reference(&tx).unwrap().0;
        let original;
        let next = match &mut opened.backend {
            Backend::Files(journal) => {
                let mut pages = io::Files::new(opened.pages.as_ref().unwrap());
                let Outcome::Prepared(prepared) =
                    opened.owner.prepare(&mut pages, &tx).await.unwrap()
                else {
                    panic!("new commit")
                };
                original = serde_json::to_vec_pretty(prepared.commit()).unwrap();
                let (next, publication) = crate::history_publication::stage(
                    &mut pages,
                    &opened.owner,
                    &prepared,
                    &original,
                )
                .await
                .unwrap();
                journal
                    .append_current(&original, expected.watermark, &publication, &|_| {})
                    .unwrap();
                next
            }
            Backend::Sqlite(db) => {
                let Outcome::Prepared(prepared) = opened
                    .owner
                    .prepare(&mut io::Sqlite::new(db), &tx)
                    .await
                    .unwrap()
                else {
                    panic!("new commit")
                };
                original = serde_json::to_vec_pretty(prepared.commit()).unwrap();
                current_publication::append_sqlite(db, &opened.owner, &prepared, &original, &|_| {})
                    .await
                    .unwrap()
            }
        };
        let identity = next.identity().to_owned();
        opened.close().await.unwrap();
        let lock = CanonicalLock::acquire(&root, &[]).unwrap();
        let mut reopened =
            Opened::open(lock, kind, &digest, &mut crate::StoreDiagnostics::new(kind))
                .await
                .unwrap();
        assert_eq!(reopened.owner.identity(), identity);
        let mut history = reopened.history().await.unwrap();
        let mut retained = Vec::new();
        while let Some(commit) = history.next_original().await.unwrap() {
            retained.push(commit);
        }
        assert_eq!(retained.len(), 2);
        assert_eq!(retained[1].bytes, original);
        history.close().await.unwrap();
        match &mut reopened.backend {
            Backend::Files(_) => {
                let mut pages = io::Files::new(reopened.pages.as_ref().unwrap());
                assert_eq!(
                    reopened.owner.archive_state(&mut pages).await.unwrap(),
                    expected
                );
                assert_eq!(
                    reopened
                        .owner
                        .originals()
                        .get(&mut pages, expected.watermark)
                        .await
                        .unwrap()
                        .unwrap()
                        .bytes,
                    original
                );
            }
            Backend::Sqlite(db) => {
                let mut pages = io::Sqlite::new(db);
                assert_eq!(
                    reopened.owner.archive_state(&mut pages).await.unwrap(),
                    expected
                );
                assert_eq!(
                    reopened
                        .owner
                        .originals()
                        .get(&mut pages, expected.watermark)
                        .await
                        .unwrap()
                        .unwrap()
                        .bytes,
                    original
                );
            }
        }
        reopened.close().await.unwrap();
    }
}

#[tokio::test]
async fn native_factory_never_creates_missing_data_or_trusts_a_missing_origin() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        let mut store = Store::open(&root, kind, &[]).await.unwrap();
        store.transact(common::initial()).await.unwrap();
        let staged = store.stage_current().await.unwrap();
        let digest = staged.origin_digest.clone();
        drop(staged);
        store.close().await.unwrap();
        let lock = CanonicalLock::acquire(&root, &[]).unwrap();
        assert!(Opened::open(
            lock,
            kind,
            &"f".repeat(64),
            &mut crate::StoreDiagnostics::new(kind)
        )
        .await
        .is_err());
        let data = root.join(if kind == BackendKind::Files {
            "canonical.frames"
        } else {
            "canonical.sqlite"
        });
        let saved = root.join("owned-fixture-saved-data");
        fs::rename(&data, &saved).unwrap();
        let lock = CanonicalLock::acquire(&root, &[]).unwrap();
        assert!(
            Opened::open(lock, kind, &digest, &mut crate::StoreDiagnostics::new(kind))
                .await
                .is_err()
        );
        assert!(!data.exists());
        fs::rename(&saved, &data).unwrap();
        let lock = CanonicalLock::acquire(&root, &[]).unwrap();
        let reopened = Opened::open(lock, kind, &digest, &mut crate::StoreDiagnostics::new(kind))
            .await
            .unwrap();
        reopened.close().await.unwrap();
    }
}

#[tokio::test]
async fn durable_native_cursor_rechecks_physical_bytes_and_poisoned_retry_cannot_report_absence() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        let mut store = Store::open(&root, kind, &[]).await.unwrap();
        store.transact(common::initial()).await.unwrap();
        let staged = store.stage_current().await.unwrap();
        let digest = staged.origin_digest.clone();
        drop(staged);
        store.close().await.unwrap();
        let lock = CanonicalLock::acquire(&root, &[]).unwrap();
        let opened = Opened::open(lock, kind, &digest, &mut crate::StoreDiagnostics::new(kind))
            .await
            .unwrap();
        let mut reader = opened.history().await.unwrap();
        match kind {
            BackendKind::Files => {
                let path = root.join("canonical.frames");
                let mut bytes = fs::read(&path).unwrap();
                bytes[0] ^= 1;
                fs::write(path, bytes).unwrap();
            }
            BackendKind::Sqlite => {
                use sqlx::sqlite::SqliteConnectOptions;
                let mut attacker = sqlx::SqliteConnection::connect_with(
                    &SqliteConnectOptions::new()
                        .filename(root.join("canonical.sqlite"))
                        .create_if_missing(false),
                )
                .await
                .unwrap();
                let bytes: Vec<u8> =
                    sqlx::query_scalar("SELECT payload FROM commits WHERE watermark=1")
                        .fetch_one(&mut attacker)
                        .await
                        .unwrap();
                let commit: crate::contract::Commit = serde_json::from_slice(&bytes).unwrap();
                let altered = serde_json::to_vec_pretty(&commit).unwrap();
                assert_ne!(altered, bytes);
                sqlx::query("UPDATE commits SET payload=?,digest=? WHERE watermark=1")
                    .bind(&altered)
                    .bind(digest_bytes(&altered))
                    .execute(&mut attacker)
                    .await
                    .unwrap();
                attacker.close().await.unwrap();
            }
        }
        assert!(reader.next_original().await.is_err());
        assert!(matches!(
            reader.next_original().await,
            Err(Error::Unavailable("retained history reader failed"))
        ));
        reader.close().await.unwrap();
        opened.close().await.unwrap();
    }
}
