// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::sync::atomic::AtomicBool;
use vcp_lifecycle::foundation::local_memory::open_local_memory;
use vcp_store::Store;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn local_memory_paused_owner_is_explicit_cancellable_and_never_resumed() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let config = config(&temp.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        task(&host, &config, config.root_task.clone(), None);
        owner.close().await.unwrap();
        let before = host.snapshot().unwrap();
        let paused = before
            .record(
                Collection::Task,
                config.root_task.as_str(),
                &config.workspace,
            )
            .unwrap()
            .value
            .clone();
        drop(host);
        let (local, owner) = open_local_memory(config.clone()).unwrap();
        // The canonical lock remains exclusive for the entire operation.
        assert!(open_local_memory(config.clone()).is_err());
        let missing = temp.path().join("missing-assets");
        assert!(local
            .build(missing.clone(), false, Arc::new(AtomicBool::new(true)))
            .await
            .is_err());
        // Remote assets are rejected before opening a network path.
        assert!(local
            .build(
                std::path::PathBuf::from(r"\\not-a-host\share\assets"),
                true,
                Arc::new(AtomicBool::new(false))
            )
            .await
            .is_err());
        let built = local
            .build(missing.clone(), true, Arc::new(AtomicBool::new(false)))
            .await
            .unwrap();
        assert!(
            matches!(built["status"].as_str(), Some("published" | "lexical_only")),
            "{built}"
        );
        let request = vcp_memory::retrieval::Request {
            workspace: config.workspace.clone(),
            tasks: None,
            roots: None,
            paths: None,
            symbols: None,
            text: "retained evidence".into(),
            historical: None,
            minimum_sequence: None,
            timeout_ms: 5000,
            results: 5,
            tokens: 512,
            bytes: 4096,
        };
        let mut denied = request.clone();
        denied.workspace = WorkspaceId::new();
        assert!(local
            .query(missing.clone(), denied, Arc::new(AtomicBool::new(false)))
            .await
            .is_err());
        let failed = local
            .query(missing.clone(), request, Arc::new(AtomicBool::new(false)))
            .await
            .unwrap();
        assert_eq!(failed["status"], "failed");
        assert_eq!(failed["resources"]["observation_retained"], true);
        owner.close().await.unwrap();
        assert!(local
            .build(missing, true, Arc::new(AtomicBool::new(false)))
            .await
            .is_err());
        drop(local);
        let store = Store::open(&config.canonical_root, backend, &[workspace])
            .await
            .unwrap();
        assert_eq!(
            store
                .state()
                .record(
                    Collection::Task,
                    config.root_task.as_str(),
                    &config.workspace
                )
                .unwrap()
                .value,
            paused
        );
        assert_eq!(
            store
                .state()
                .records
                .values()
                .filter(|r| r.collection == Collection::Attempt)
                .count(),
            0
        );
        store.close().await.unwrap();
        let mut wrong_binding = config.clone();
        wrong_binding.binding.revision = Revision::new(1);
        assert!(open_local_memory(wrong_binding).is_err());
        // Failed startup releases its exclusive store ownership as well.
        let (local, owner) = open_local_memory(config).unwrap();
        owner.close().await.unwrap();
        drop(local);
    }
}
