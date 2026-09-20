// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::sync::atomic::AtomicBool;
use vcp_lifecycle::foundation::{memory_publication, memory_vectors};
use vcp_store::contract::{key, Collection};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn paused_backup_captures_native_dirty_untracked_and_generation_lineage_without_model_work() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let (temp, config, host, owner, test, thread) =
            super::memory_publication::vector_fixture(backend).await;
        let workspace = std::path::Path::new(&config.binding.root);
        let executable = std::path::PathBuf::from(std::env::var_os("VCP_TEST_GIT").unwrap());
        std::fs::write(workspace.join("tracked.txt"), "staged\n").unwrap();
        for args in [vec!["init", "--quiet"], vec!["add", "--", "tracked.txt"]] {
            assert!(std::process::Command::new(&executable)
                .current_dir(workspace)
                .args(args)
                .output()
                .unwrap()
                .status
                .success());
        }
        std::fs::write(workspace.join("tracked.txt"), "unstaged\n").unwrap();
        std::fs::write(
            workspace.join("untracked.txt"),
            "required untracked bytes\n",
        )
        .unwrap();
        let index = std::fs::read(workspace.join(".git/index")).unwrap();
        let git = Arc::new(
            vcp_repository::git::Git::new(
                executable,
                ["PATH", "SystemRoot", "WINDIR", "TEMP", "TMP"]
                    .into_iter()
                    .filter_map(|name| std::env::var_os(name).map(|value| (name.into(), value)))
                    .collect(),
                Duration::from_secs(15),
                4 * 1024 * 1024,
            )
            .unwrap(),
        );
        let private = temp.path().join("private");
        std::fs::create_dir(&private).unwrap();
        let publisher = Arc::new(
            vcp_memory::publication::Publisher::new(
                &config.canonical_root.join("search-generations"),
            )
            .unwrap(),
        );
        let manager = host.publication_manager(thread, publisher.clone()).unwrap();
        host.publish_memory(
            thread,
            manager,
            memory_publication::Request {
                vectors: memory_vectors::Request {
                    assets: temp.path().join("absent-assets"),
                    private_root: private,
                    sources: vec![],
                    chunker: Default::default(),
                    cancelled: Arc::new(AtomicBool::new(false)),
                },
                allow_lexical_only: true,
            },
        )
        .await
        .unwrap();
        let (opened, _) = host
            .open_memory_view(thread, publisher.clone(), Arc::new(AtomicBool::new(false)))
            .await
            .unwrap();
        let view = opened.view.unwrap();
        let source_refs: std::collections::BTreeSet<_> = view
            .inventory
            .records
            .iter()
            .map(|row| match &row.source {
                vcp_memory::search_record::TextSource::Artifact { id } => {
                    key(Collection::Artifact, id.as_str())
                }
                vcp_memory::search_record::TextSource::Claim { version, .. } => {
                    key(Collection::Claim, version.as_str())
                }
            })
            .collect();
        let task: Task = host
            .snapshot()
            .unwrap()
            .record(
                Collection::Task,
                config.root_task.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        host.command(
            Command::Transition {
                next: TaskState::Paused,
                reason: "explicit backup pause".into(),
                verification: None,
            },
            Some(config.root_task.clone()),
            task.revision,
        )
        .unwrap();
        let generation = host
            .capture_backup_generation(publisher, view, Arc::new(AtomicBool::new(false)))
            .await
            .unwrap();
        let checkpoint = host
            .capture_backup_checkpoint(git.clone(), Arc::new(AtomicBool::new(false)))
            .await
            .unwrap();
        assert_eq!(
            host.read_artifact(checkpoint.sources["tracked.txt"].clone())
                .unwrap(),
            b"unstaged\n"
        );
        assert_eq!(
            host.read_artifact(checkpoint.sources["untracked.txt"].clone())
                .unwrap(),
            b"required untracked bytes\n"
        );
        assert_eq!(std::fs::read(workspace.join(".git/index")).unwrap(), index);
        let state = host.snapshot().unwrap();
        let row = state
            .record(
                Collection::Artifact,
                checkpoint.manifest.as_str(),
                &config.workspace,
            )
            .unwrap();
        for id in checkpoint.sources.values() {
            assert!(row
                .references
                .contains(&key(Collection::Artifact, id.as_str())));
        }
        for id in std::iter::once(&generation.inventory)
            .chain(std::iter::once(&generation.lexical_manifest))
            .chain(generation.lexical_files.values())
            .chain(generation.vectors.iter())
        {
            let row = state
                .record(Collection::Artifact, id.as_str(), &config.workspace)
                .unwrap();
            assert!(source_refs.is_subset(&row.references));
            assert!(row
                .references
                .contains(&key(Collection::Generation, generation.id.as_str())));
        }
        assert!(!state
            .records
            .values()
            .any(|row| row.collection == Collection::Attempt));
        assert_eq!(
            state
                .record(
                    Collection::Task,
                    config.root_task.as_str(),
                    &config.workspace
                )
                .unwrap()
                .decode::<Task>()
                .unwrap()
                .state,
            TaskState::Paused
        );
        assert!(host
            .capture_backup_checkpoint(git, Arc::new(AtomicBool::new(true)))
            .await
            .is_err());
        assert_eq!(host.snapshot().unwrap().watermark, state.watermark);
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        drop(test);
        drop(host);
    }
}
