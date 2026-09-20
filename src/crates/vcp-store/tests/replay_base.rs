// SPDX-License-Identifier: Apache-2.0
mod common;
use common::*;
use vcp_domain::*;
use vcp_store::artifact::ArtifactWriter;

#[tokio::test]
async fn outer_migration_anchor_survives_rewrite_and_subsequent_backend_switch() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut active = migration::ActiveRoot::open(temp.path(), Some(kind), &[])
            .await
            .unwrap();
        active
            .store_mut()
            .unwrap()
            .transact(initial())
            .await
            .unwrap();
        let state = active.store().state().clone();
        active
            .store_mut()
            .unwrap()
            .rewrite_base(state.clone(), &[])
            .await
            .unwrap();
        drop(active);
        let other = if kind == BackendKind::Files {
            BackendKind::Sqlite
        } else {
            BackendKind::Files
        };
        let mut active = migration::ActiveRoot::open(temp.path(), Some(kind), &[])
            .await
            .unwrap();
        assert_eq!(active.store().state(), &state);
        active.switch(other).await.unwrap();
        drop(active);
        let active = migration::ActiveRoot::open(temp.path(), Some(other), &[])
            .await
            .unwrap();
        assert_eq!(active.store().state(), &state);
    }
}
use vcp_store::{contract::*, *};

#[tokio::test]
async fn typed_content_redaction_removes_new_root_bytes_but_reports_old_root_pending() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let anchor = temp.path().join("source");
        let mut store = Store::open(&anchor, kind, &[]).await.unwrap();
        let marker = "FORBIDDEN_SYNTHETIC_RETENTION_MARKER";
        let mut tx = initial();
        for mutation in &mut tx.mutations {
            let Mutation::Put { record, .. } = mutation else {
                continue;
            };
            if record.collection == Collection::Task {
                let mut task: vcp_domain::task::Task = record.decode().unwrap();
                task.state = vcp_domain::task::TaskState::Cancelled;
                task.objectives[0].text = marker.into();
                record.value = serde_json::to_value(task).unwrap();
            }
            if record.collection == Collection::Workspace {
                let mut workspace: vcp_domain::workspace::Workspace = record.decode().unwrap();
                workspace.deletion = DeletionEpoch::new(1);
                record.value = serde_json::to_value(workspace).unwrap();
            }
        }
        tx.events[0].data = serde_json::json!({"text":marker});
        store.transact(tx.clone()).await.unwrap();
        let mut writer = store.spool().create(spec()).unwrap();
        writer.write_chunk(marker.as_bytes()).unwrap();
        let mut descriptor = writer.finalize().unwrap();
        drop(writer);
        store
            .transact(attach(store.state(), descriptor.clone(), None))
            .await
            .unwrap();
        let old_snapshot = store.snapshot().unwrap();
        let mut baseline = store.state().clone();
        for record in baseline.records.values_mut() {
            if record.collection == Collection::Task {
                record.value = serde_json::to_value(
                    vcp_protocol::redaction::task(&record.decode().unwrap(), DeletionEpoch::new(1))
                        .unwrap(),
                )
                .unwrap();
            }
            if record.collection == Collection::Artifact {
                descriptor.state = vcp_domain::artifact::CaptureState::Purged;
                descriptor.retained.clear();
                record.value = serde_json::to_value(&descriptor).unwrap();
            }
        }
        baseline.events = baseline
            .events
            .iter()
            .map(|e| vcp_protocol::redaction::event(e, DeletionEpoch::new(1)).unwrap())
            .collect();
        let receipt = store.rewrite_base(baseline, &[]).await.unwrap();
        assert_eq!(receipt.pending_roots, vec!["anchor"]);
        assert!(store.spool().read(&descriptor, Vec::new()).is_err());
        let bytes = vcp_protocol::canonical_bytes(store.snapshot().unwrap().state()).unwrap();
        assert!(!bytes.windows(marker.len()).any(|w| w == marker.as_bytes()));
        for file in vcp_store::rewrite::owned_files(store.root()).unwrap() {
            let bytes = std::fs::read(file).unwrap();
            assert!(!bytes.windows(marker.len()).any(|w| w == marker.as_bytes()));
        }
        assert!(vcp_protocol::canonical_bytes(old_snapshot.state())
            .unwrap()
            .windows(marker.len())
            .any(|w| w == marker.as_bytes()));
        assert!(vcp_store::rewrite::owned_files(&anchor)
            .unwrap()
            .iter()
            .any(|p| p.extension().is_some_and(|e| e == "chunk")));
        let original_receipt = store.state().transactions[&tx.id].clone();
        assert_eq!(store.transact(tx).await.unwrap(), original_receipt);
        store.close().await.unwrap();
        let reopened = Store::open(&anchor, kind, &[]).await.unwrap();
        assert!(reopened.spool().read(&descriptor, Vec::new()).is_err());
        reopened.close().await.unwrap();
    }
}

#[tokio::test]
async fn replay_base_preserves_finalized_artifact_bytes_and_conversion() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(&temp.path().join("source"), kind, &[])
            .await
            .unwrap();
        store.transact(initial()).await.unwrap();
        let mut writer = store.spool().create(spec()).unwrap();
        writer.write_chunk(b"retained synthetic payload").unwrap();
        let descriptor = writer.finalize().unwrap();
        drop(writer);
        store
            .transact(attach(store.state(), descriptor.clone(), None))
            .await
            .unwrap();
        store
            .rewrite_base(store.state().clone(), &[])
            .await
            .unwrap();
        let mut bytes = Vec::new();
        store.spool().read(&descriptor, &mut bytes).unwrap();
        assert_eq!(bytes, b"retained synthetic payload");
        let converted = store
            .convert(&temp.path().join("copy"), kind, &[])
            .await
            .unwrap();
        assert_eq!(converted.state(), store.state());
        let mut bytes = Vec::new();
        converted.spool().read(&descriptor, &mut bytes).unwrap();
        assert_eq!(bytes, b"retained synthetic payload");
        converted.close().await.unwrap();
        store.close().await.unwrap();
    }
}

fn suffix(state: &State) -> Transaction {
    let mut event = initial().events.remove(0);
    event.id = EventId::new();
    event.kind = vcp_protocol::event::EventKind::Diagnostic;
    event.data = serde_json::json!({"marker":"after replay boundary"});
    Transaction {
        id: TransactionId::new(),
        expected_watermark: state.watermark,
        mutations: vec![],
        events: vec![event],
        command: None,
    }
}

#[tokio::test]
async fn rewrite_reopens_suffix_checkpoint_retry_and_conversion_on_both_backends() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let anchor = temp.path().join("root");
        let mut store = Store::open(&anchor, kind, &[]).await.unwrap();
        let original = store.transact(initial()).await.unwrap();
        let state = store.state().clone();
        let receipt = store.rewrite_base(state.clone(), &[]).await.unwrap();
        assert_eq!(receipt.pending_roots, vec!["anchor"]);
        assert_eq!(store.state(), &state);
        assert_ne!(store.root(), anchor.canonicalize().unwrap());
        assert!(Store::open(&anchor, kind, &[]).await.is_err());
        assert_eq!(store.transact(initial()).await.unwrap(), original);
        let next = suffix(store.state());
        let next_receipt = store.transact(next.clone()).await.unwrap();
        store.checkpoint().unwrap();
        let expected = store.state().clone();
        store.close().await.unwrap();
        let mut store = Store::open(&anchor, kind, &[]).await.unwrap();
        assert_eq!(store.state(), &expected);
        assert_eq!(store.transact(next).await.unwrap(), next_receipt);
        let other = if kind == BackendKind::Files {
            BackendKind::Sqlite
        } else {
            BackendKind::Files
        };
        let converted = store
            .convert(&temp.path().join("converted"), other, &[])
            .await
            .unwrap();
        assert_eq!(converted.state(), &expected);
        converted.close().await.unwrap();
        let again = store.rewrite_base(expected.clone(), &[]).await.unwrap();
        assert_eq!(again.generation, 1);
        assert_eq!(again.pending_roots.len(), 2);
        store.close().await.unwrap();
        let store = Store::open(&anchor, kind, &[]).await.unwrap();
        assert_eq!(store.state(), &expected);
        store.close().await.unwrap();
        // Original acknowledged history remains explicit recovery material.
        assert!(anchor
            .join(if kind == BackendKind::Files {
                "canonical.frames"
            } else {
                "canonical.sqlite"
            })
            .exists());
    }
}

#[tokio::test]
async fn root_snapshot_lease_covers_payload_free_snapshots() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path(), kind, &[]).await.unwrap();
        let snapshot = store.snapshot().unwrap();
        store.close().await.unwrap();
        let lease = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(temp.path().join("root-snapshot.lock"))
            .unwrap();
        assert!(lease.try_lock().is_err());
        drop(snapshot);
        lease.try_lock().unwrap();
    }
}

#[tokio::test]
async fn invalid_commitments_and_corrupt_base_fail_without_reopening_old_root() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path(), kind, &[]).await.unwrap();
        store.transact(initial()).await.unwrap();
        let mut invalid = store.state().clone();
        invalid.transactions.clear();
        assert!(store.rewrite_base(invalid, &[]).await.is_err());
        assert_eq!(store.state().watermark, Watermark::new(1));
        store
            .rewrite_base(store.state().clone(), &[])
            .await
            .unwrap();
        let forbidden = temp.path().join("sync-root");
        std::fs::create_dir(&forbidden).unwrap();
        let rejected = forbidden.join("conversion");
        assert!(store.convert(&rejected, kind, &[forbidden]).await.is_err());
        assert!(!rejected.exists());
        let root = store.root().to_owned();
        store.close().await.unwrap();
        std::fs::write(root.join("replay-base.seal"), "0".repeat(64)).unwrap();
        assert!(Store::open(temp.path(), kind, &[]).await.is_err());
    }
}

#[cfg(feature = "qualification")]
#[tokio::test]
async fn rewrite_process_child() {
    let Some(path) = std::env::var_os("VCP_REWRITE_CHILD_ROOT") else {
        return;
    };
    let kind: BackendKind = std::env::var("VCP_REWRITE_CHILD_KIND")
        .unwrap()
        .parse()
        .unwrap();
    let after = std::env::var("VCP_REWRITE_CHILD_AFTER").unwrap() == "1";
    let mut store = Store::open(std::path::Path::new(&path), kind, &[])
        .await
        .unwrap();
    store.transact(initial()).await.unwrap();
    store.observe(std::sync::Arc::new(move |actual| {
        if actual
            == if after {
                Barrier::AfterActivation
            } else {
                Barrier::BeforeActivation
            }
        {
            std::process::exit(73);
        }
    }));
    store
        .rewrite_base(store.state().clone(), &[])
        .await
        .unwrap();
    panic!("activation barrier not reached");
}
#[cfg(feature = "qualification")]
#[tokio::test]
async fn process_exit_at_activation_boundaries_is_reopenable() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        for after in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "rewrite_process_child", "--nocapture"])
                .env("VCP_REWRITE_CHILD_ROOT", temp.path())
                .env(
                    "VCP_REWRITE_CHILD_KIND",
                    if kind == BackendKind::Files {
                        "files"
                    } else {
                        "sqlite"
                    },
                )
                .env("VCP_REWRITE_CHILD_AFTER", if after { "1" } else { "0" })
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(73));
            let mut store = Store::open(temp.path(), kind, &[]).await.unwrap();
            assert_eq!(
                store.transact(initial()).await.unwrap().watermark,
                Watermark::new(1)
            );
            assert_eq!(store.root() != temp.path().canonicalize().unwrap(), after);
            store.close().await.unwrap();
        }
    }
}
