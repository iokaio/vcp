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
