// SPDX-License-Identifier: Apache-2.0
use vcp_store::{BackendKind, Store};

#[tokio::test]
async fn payload_free_snapshot_pins_survive_owner_reopen_and_exclude_cleanup() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path(), backend, &[]).await.unwrap();
        let first = store.snapshot().unwrap();
        let second = store.snapshot().unwrap();
        assert!(first.state().records.is_empty());
        #[cfg(windows)]
        {
            let lock = temp.path().join("root-snapshot.lock");
            assert!(std::fs::remove_file(&lock).is_err());
            assert!(std::fs::rename(&lock, temp.path().join("replaced.lock")).is_err());
        }
        assert!(store.try_snapshot_cleanup_guard().unwrap().is_none());
        store.close().await.unwrap();
        let reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert!(reopened.try_snapshot_cleanup_guard().unwrap().is_none());
        drop(first);
        assert!(reopened.try_snapshot_cleanup_guard().unwrap().is_none());
        drop(second);
        let guard = reopened.try_snapshot_cleanup_guard().unwrap().unwrap();
        assert!(reopened.snapshot().is_err());
        assert!(reopened.try_snapshot_cleanup_guard().unwrap().is_none());
        drop(guard);
        let snapshot = reopened.snapshot().unwrap();
        assert!(reopened.try_snapshot_cleanup_guard().unwrap().is_none());
        drop(snapshot);
        assert!(reopened.try_snapshot_cleanup_guard().unwrap().is_some());
        reopened.close().await.unwrap();
    }
}

#[tokio::test]
async fn malformed_snapshot_lock_is_an_error_not_a_live_snapshot() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path(), backend, &[]).await.unwrap();
        std::fs::create_dir(temp.path().join("root-snapshot.lock")).unwrap();
        assert!(store.try_snapshot_cleanup_guard().is_err());
        assert!(store.snapshot().is_err());
        store.close().await.unwrap();
    }
}
