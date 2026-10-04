// SPDX-License-Identifier: Apache-2.0
mod common;
use std::sync::Arc;
use vcp_domain::{EventId, TransactionId, WorkspaceId};
use vcp_store::{
    contract::{CanonicalStore, Collection, Transaction},
    BackendKind, Error, Store,
};

#[tokio::test]
async fn current_snapshots_share_reads_and_remain_stable_across_rejections_and_commits() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temporary = tempfile::tempdir().unwrap();
        let mut store = Store::open(temporary.path(), backend, &[]).await.unwrap();
        let initial = common::initial();
        store.transact(initial.clone()).await.unwrap();
        let old = store.current_state();
        assert!(Arc::ptr_eq(&old, &store.current_state()));
        assert_eq!(
            old.records_in(Collection::Session, &common::workspace().id)
                .count(),
            1
        );
        assert_eq!(
            old.records_in(Collection::Session, &WorkspaceId::new())
                .count(),
            0
        );
        assert_eq!(
            old.records_in(Collection::Artifact, &common::workspace().id)
                .count(),
            0
        );
        store.transact(initial.clone()).await.unwrap();
        assert!(Arc::ptr_eq(&old, &store.current_state()));
        let mut invalid = initial.clone();
        invalid.events[0].data = serde_json::json!({"different": true});
        assert!(store.transact(invalid).await.is_err());
        assert!(Arc::ptr_eq(&old, &store.current_state()));
        assert!(matches!(
            old.record(Collection::Task, "task", &WorkspaceId::new()),
            Err(Error::Access)
        ));

        let mut event = initial.events[0].clone();
        event.id = EventId::new();
        event.data = serde_json::json!({"history_only": "retained-payload".repeat(4096)});
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![],
                events: vec![event],
                command: None,
            })
            .await
            .unwrap();
        let current = store.current_state();
        assert!(!Arc::ptr_eq(&old, &current));
        assert_eq!(old.watermark.get(), 1);
        assert_eq!(current.watermark.get(), 2);
        assert_eq!(old.records, current.records);
        let projected = vcp_protocol::canonical_bytes(&*current).unwrap();
        let historical = vcp_protocol::canonical_bytes(store.state()).unwrap();
        assert!(projected.len() * 10 < historical.len());
        let json = serde_json::to_value(&*current).unwrap();
        assert!(json.get("events").is_none());
        assert!(json.get("commands").is_none());
        assert!(json.get("transactions").is_none());
        assert_ne!(
            current.projection_digest().unwrap(),
            vcp_protocol::digest_bytes(&historical)
        );
        store.close().await.unwrap();
        // A retained reader remains a fixed snapshot, not a capability to mutate.
        assert_eq!(current.watermark.get(), 2);
    }
}
