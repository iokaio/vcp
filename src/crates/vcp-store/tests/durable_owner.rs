// SPDX-License-Identifier: Apache-2.0
mod common;
use vcp_domain::{CommandId, EventId, TransactionId};
use vcp_protocol::{canonical_bytes, digest_bytes};
use vcp_store::{
    contract::{CanonicalStore, State},
    BackendKind, Store,
};

#[tokio::test]
async fn public_owner_activates_current_layout_preserves_replay_and_pinned_reads() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        let forbidden = temp.path().join("workspace");
        std::fs::create_dir(&forbidden).unwrap();
        let mut store = Store::open(&root, kind, &[forbidden.clone()])
            .await
            .unwrap();
        let marker: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("format.json")).unwrap()).unwrap();
        assert_eq!(marker["version"], 3);
        assert!(root
            .join(format!(
                "format-legacy-{}.json",
                marker["legacy"].as_str().unwrap()
            ))
            .exists());
        let initial = common::initial();
        let (first, receipt) = State::default().prepare(&initial).unwrap();
        assert_eq!(
            store.transact(initial.clone()).await.unwrap(),
            receipt.receipt
        );
        assert_eq!(store.transact(initial).await.unwrap(), receipt.receipt);
        let snapshot = store.snapshot().unwrap();
        let expected_digest = digest_bytes(&canonical_bytes(&first).unwrap());
        assert_eq!(snapshot.logical_digest().await.unwrap(), expected_digest);
        let expected_bytes = canonical_bytes(&first).unwrap().len();
        assert_eq!(
            store.archive_size(expected_bytes).await.unwrap(),
            expected_bytes
        );
        assert!(matches!(
            store.archive_size(expected_bytes - 1).await,
            Err(vcp_store::Error::Limit("archival encoding byte budget"))
        ));
        let checks = std::cell::Cell::new(0);
        assert!(matches!(
            store
                .archive_size_with_check(usize::MAX, &|| {
                    checks.set(checks.get() + 1);
                    if checks.get() == 3 {
                        Err(vcp_store::Error::Unavailable("cancelled size probe"))
                    } else {
                        Ok(())
                    }
                })
                .await,
            Err(vcp_store::Error::Unavailable("cancelled size probe"))
        ));
        assert_eq!(checks.get(), 3);
        assert!(store.healthy());
        let mut next = common::initial();
        next.id = TransactionId::parse("after-pin").unwrap();
        next.expected_watermark = first.watermark;
        next.mutations.clear();
        next.events[0].id = EventId::parse("after-pin-event").unwrap();
        next.events[0].correlation = CommandId::parse("after-pin-command").unwrap();
        next.command.as_mut().unwrap().command = next.events[0].correlation.clone();
        next.command.as_mut().unwrap().digest = "e".repeat(64);
        let (last, last_commit) = first.prepare(&next).unwrap();
        assert_eq!(store.transact(next).await.unwrap(), last_commit.receipt);
        assert_eq!(store.archive_state().await.unwrap(), last);
        assert_eq!(
            store.prefix_digest(first.watermark).await.unwrap(),
            expected_digest
        );
        assert_eq!(
            snapshot.history_events(None, 4096).await.unwrap(),
            *first.events
        );
        assert_eq!(snapshot.archive_state().await.unwrap(), first);
        store.checkpoint().unwrap();
        store.close().await.unwrap();
        assert_eq!(snapshot.current().watermark, first.watermark);
        assert_eq!(
            snapshot.history_event_count().await.unwrap(),
            first.events.len() as u64
        );
        let reopened = Store::open(&root, kind, &[forbidden]).await.unwrap();
        assert_eq!(reopened.archive_state().await.unwrap(), last);
        assert_eq!(
            reopened.prefix_digest(last.watermark).await.unwrap(),
            digest_bytes(&canonical_bytes(&last).unwrap())
        );
        assert!(reopened.try_snapshot_cleanup_guard().unwrap().is_none());
        snapshot.close().await.unwrap();
        assert!(reopened.try_snapshot_cleanup_guard().unwrap().is_some());
        reopened.close().await.unwrap();
    }
}
