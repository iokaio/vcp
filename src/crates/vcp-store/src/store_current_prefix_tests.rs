// SPDX-License-Identifier: Apache-2.0
use crate::{contract::*, BackendKind, Store};
use vcp_domain::{CommandId, EventId, TransactionId};
use vcp_protocol::{canonical_bytes, digest_bytes};
#[path = "../tests/common/mod.rs"]
mod common;

fn next(source: &State, number: u64) -> Transaction {
    let mut tx = common::initial();
    tx.id = TransactionId::parse(format!("prefix-{number}")).unwrap();
    tx.expected_watermark = source.watermark;
    tx.mutations.clear();
    tx.events[0].id = EventId::parse(format!("prefix-event-{number}")).unwrap();
    tx.events[0].correlation = CommandId::parse(format!("prefix-command-{number}")).unwrap();
    tx.command.as_mut().unwrap().command = tx.events[0].correlation.clone();
    tx.command.as_mut().unwrap().digest = format!("{number:064x}");
    tx
}

#[tokio::test]
async fn migrated_owner_reconstructs_exact_cuts_before_at_and_after_origin_without_resident_history(
) {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let mut old =
            crate::legacy_store_fixture::LegacyFixture::open(&root, kind, &[workspace.clone()])
                .await
                .unwrap();
        old.transact(common::initial()).await.unwrap();
        let first = old.state().clone();
        old.transact(next(&first, 2)).await.unwrap();
        let origin = old.state().clone();
        old.close().await.unwrap();
        let mut store = Store::open(&root, kind, &[workspace.clone()])
            .await
            .unwrap();
        store.transact(next(&origin, 3)).await.unwrap();
        let last = store.archive_state().await.unwrap();
        for expected in [&State::default(), &first, &origin, &last] {
            let snapshot = store.snapshot_at(expected.watermark).await.unwrap();
            assert_eq!(snapshot.current().records, &*expected.records);
            assert_eq!(snapshot.current().sequences, &expected.sequences);
            assert_eq!(
                snapshot.history_event_count().await.unwrap(),
                expected.events.len() as u64
            );
            assert_eq!(
                snapshot.history_events(None, 4096).await.unwrap(),
                *expected.events
            );
            assert_eq!(snapshot.archive_state().await.unwrap(), *expected);
            for (ordinal, event) in last.events.iter().enumerate() {
                assert_eq!(
                    snapshot.history_event_at(ordinal as u64).await.unwrap(),
                    expected.events.get(ordinal).cloned()
                );
                assert_eq!(
                    snapshot.history_event(&event.event.id).await.unwrap(),
                    expected
                        .events
                        .iter()
                        .find(|row| row.event.id == event.event.id)
                        .cloned()
                );
            }
            for receipt in last.commands.values() {
                assert_eq!(
                    snapshot
                        .command_receipt_by_id(&receipt.workspace, &receipt.command)
                        .await
                        .unwrap(),
                    expected
                        .commands
                        .get(&command_key(&receipt.workspace, &receipt.command))
                        .cloned()
                );
            }
            assert_eq!(
                snapshot.logical_digest().await.unwrap(),
                digest_bytes(&canonical_bytes(expected).unwrap())
            );
            assert_eq!(
                store.prefix_digest(expected.watermark).await.unwrap(),
                snapshot.logical_digest().await.unwrap()
            );
            snapshot.close().await.unwrap();
        }
        let snapshot = store.snapshot_at(first.watermark).await.unwrap();
        store.close().await.unwrap();
        let mut reopened = Store::open(&root, kind, &[workspace]).await.unwrap();
        reopened.transact(next(&last, 4)).await.unwrap();
        assert_eq!(snapshot.archive_state().await.unwrap(), first);
        assert_eq!(
            snapshot.history_events(None, 4096).await.unwrap(),
            *first.events
        );
        assert!(snapshot
            .history_events(Some(first.events.len() as u64), 1)
            .await
            .is_err());
        snapshot.close().await.unwrap();
        reopened.close().await.unwrap();
    }
}
