// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! Reopen actual predecessor roots retained by the production CLI campaign.
//! The input is an explicitly supplied local qualification receipt, not archive authority.
use serde_json::Value;
use std::{fs, path::Path};
use vcp_domain::{task::Task, workspace::Workspace, SessionId, TaskId, WorkspaceId};
use vcp_store::{contract::Collection, BackendKind, Store};

#[tokio::test]
#[ignore = "requires VCP_TEST_PREDECESSOR_RECEIPT from the actual CLI predecessor campaign"]
async fn retained_predecessors_reopen_after_descendant_activation() {
    let path = std::env::var_os("VCP_TEST_PREDECESSOR_RECEIPT")
        .expect("explicit public qualification receipt");
    assert!(fs::metadata(&path).unwrap().len() <= 2 * 1024 * 1024);
    let receipt: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let rows = receipt["results"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    for row in rows {
        let root = Path::new(row["prior_root"].as_str().unwrap());
        assert_ne!(root, Path::new(row["selected_root"].as_str().unwrap()));
        let backend = match row["backend"].as_str().unwrap() {
            "files" => BackendKind::Files,
            "sqlite" => BackendKind::Sqlite,
            _ => panic!("unsupported qualification backend"),
        };
        let workspace = WorkspaceId::parse(row["workspace"].as_str().unwrap()).unwrap();
        let task = TaskId::parse(row["root_task"].as_str().unwrap()).unwrap();
        let session = SessionId::parse(row["session"].as_str().unwrap()).unwrap();
        let inventory: Vec<Value> =
            serde_json::from_str(row["prior_root_inventory"].as_str().unwrap()).unwrap();
        assert!(!inventory.is_empty() && inventory.len() <= 4096);
        verify_bytes(root, &inventory);
        let store = Store::open(root, backend, &[]).await.unwrap();
        let state = store.state();
        let retained: Workspace = state
            .record(Collection::Workspace, workspace.as_str(), &workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(retained.id, workspace);
        let retained: Task = state
            .record(Collection::Task, task.as_str(), &workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(retained.scope.task, task.clone());
        assert_eq!(retained.root, task);
        assert_eq!(retained.scope.session, session);
        assert!(state.watermark.get() > 0);
        store.close().await.unwrap();
        verify_bytes(root, &inventory);
    }
}

fn verify_bytes(root: &Path, rows: &[Value]) {
    for row in rows {
        let relative = Path::new(row["path"].as_str().unwrap());
        assert!(!relative.is_absolute());
        assert!(relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_))));
        // Opening a Store deliberately replaces its ephemeral owner token.
        assert_ne!(relative, Path::new("owner.lock"));
        let bytes = fs::read(root.join(relative)).unwrap();
        assert_eq!(bytes.len() as u64, row["bytes"].as_u64().unwrap());
        assert_eq!(
            vcp_protocol::digest_bytes(&bytes),
            row["sha256"].as_str().unwrap().to_lowercase()
        );
    }
}

/// Compares the original A capture with the actual returned descendant. Authority,
/// task presentation, search heads and snapshot-job progress intentionally evolve;
/// original event/receipt history and retained evidence must not.
#[tokio::test]
#[ignore = "requires actual return receipt and VCP_TEST_RETURN_A_PRIVATE metadata"]
async fn actual_return_preserves_captured_history_and_evidence() {
    use vcp_domain::{artifact::ArtifactDescriptor, CommandId};
    use vcp_store::snapshot_jobs::Jobs;
    let load = |name: &str| -> Value {
        let path = std::env::var_os(name).expect("explicit local qualification input");
        assert!(fs::metadata(&path).unwrap().len() <= 2 * 1024 * 1024);
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
    };
    let receipt = load("VCP_TEST_PREDECESSOR_RECEIPT");
    let original = load("VCP_TEST_RETURN_A_PRIVATE");
    assert_eq!(receipt["campaign"], original["campaign"]);
    let rows = receipt["results"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    for row in rows {
        let backend = match row["backend"].as_str().unwrap() {
            "files" => BackendKind::Files,
            "sqlite" => BackendKind::Sqlite,
            _ => panic!("unsupported backend"),
        };
        let case_id = if backend == BackendKind::Files {
            "files-sqlite-files"
        } else {
            "sqlite-files-sqlite"
        };
        let case = original["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["id"] == case_id)
            .unwrap();
        let workspace = WorkspaceId::parse(row["workspace"].as_str().unwrap()).unwrap();
        let operation =
            CommandId::parse(case["publication"]["operation"].as_str().unwrap()).unwrap();
        let before = Store::open(Path::new(row["prior_root"].as_str().unwrap()), backend, &[])
            .await
            .unwrap();
        let after = Store::open(
            Path::new(row["selected_root"].as_str().unwrap()),
            backend,
            &[],
        )
        .await
        .unwrap();
        let job = Jobs::inspect(&before, &operation, &workspace).unwrap();
        assert_eq!(
            before.prefix_digest(job.watermark).unwrap(),
            job.state_digest
        );
        let events: Vec<_> = before
            .state()
            .events
            .iter()
            .filter(|e| e.watermark <= job.watermark)
            .collect();
        assert!(!events.is_empty());
        let returned: Vec<_> = after
            .state()
            .events
            .iter()
            .filter(|e| e.watermark <= job.watermark)
            .collect();
        assert_eq!(events, returned, "original event envelopes changed");
        let commands: Vec<_> = before
            .state()
            .commands
            .iter()
            .filter(|(_, r)| r.watermark <= job.watermark)
            .collect();
        assert!(!commands.is_empty());
        for (key, value) in &commands {
            assert_eq!(after.state().commands.get(*key), Some(*value));
        }
        let transactions: Vec<_> = before
            .state()
            .transactions
            .iter()
            .filter(|(_, r)| r.watermark <= job.watermark)
            .collect();
        assert!(!transactions.is_empty());
        for (key, value) in &transactions {
            assert_eq!(after.state().transactions.get(*key), Some(*value));
        }
        let mut records = 0;
        let mut artifacts = 0;
        let mut bytes = 0u64;
        for (key, record) in &before.state().records {
            if !matches!(
                record.collection,
                Collection::Session
                    | Collection::Turn
                    | Collection::Effect
                    | Collection::Artifact
                    | Collection::Verification
                    | Collection::Ledger
                    | Collection::Reservation
                    | Collection::Attempt
                    | Collection::Settlement
                    | Collection::Claim
            ) {
                continue;
            }
            assert_eq!(
                after.state().records.get(key),
                Some(record),
                "retained canonical record changed: {}",
                record.collection.name()
            );
            records += 1;
            if record.collection == Collection::Artifact {
                let descriptor: ArtifactDescriptor = record.decode().unwrap();
                // Streaming read validates the actual payload digest, not merely its descriptor.
                before.spool().read(&descriptor, std::io::sink()).unwrap();
                after.spool().read(&descriptor, std::io::sink()).unwrap();
                artifacts += 1;
                bytes += descriptor.length.get();
            }
        }
        assert!(records > 0 && artifacts > 0 && bytes > 0);
        println!("{case_id}: original cut={}, events={}, commands={}, transactions={}, retained_records={records}, artifacts={artifacts}, artifact_bytes={bytes}", job.watermark.get(), events.len(), commands.len(), transactions.len());
        after.close().await.unwrap();
        before.close().await.unwrap();
    }
}
