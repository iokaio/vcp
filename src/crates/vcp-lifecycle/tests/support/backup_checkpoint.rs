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
        let source = super::memory_publication::retained_source(&host, &config, thread);
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
                    sources: vec![source],
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
        assert!(!source_refs.is_empty());
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
        let shared_cancel = Arc::new(AtomicBool::new(false));
        let generation = host
            .capture_backup_generation(publisher, view, shared_cancel.clone())
            .await
            .unwrap();
        let checkpoint = host
            .capture_backup_checkpoint(git.clone(), shared_cancel.clone())
            .await
            .unwrap();
        assert!(!shared_cancel.load(std::sync::atomic::Ordering::Acquire));
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
        // The production pipeline keeps encryption/copy off the canonical
        // owner and reuses the same durable operation on lost acknowledgment.
        let staging = temp.path().join("backup-staging");
        let vault = temp.path().join("backup-vault");
        let recovery = temp.path().join("backup-recovery");
        let trust_path = temp.path().join("backup-trust");
        for directory in [&staging, &vault, &recovery, &trust_path] {
            std::fs::create_dir(directory).unwrap();
        }
        let forbidden = vec![workspace.to_owned(), config.canonical_root.clone()];
        let keys = vcp_store::keys::LocalKeys::generate().unwrap();
        let copy = keys
            .export_recovery(
                &vcp_store::keys::RecoveryDirectory::open(&recovery, &forbidden).unwrap(),
            )
            .unwrap();
        let keys = keys.verify_recovery(&copy).unwrap();
        let enrolled = vcp_store::vault_publish::LocalTrust::enroll(
            &keys,
            config.workspace.clone(),
            "e".repeat(64),
            vcp_store::vault_publish::Checkpoint {
                sequence: 0,
                deletion: 0,
                parent: None,
            },
        )
        .unwrap();
        let trust =
            vcp_store::trust_store::TrustStore::enroll(&trust_path, &forbidden, enrolled).unwrap();
        let setup = vcp_lifecycle::foundation::backup::Setup {
            schema_version: 1,
            revision: 0,
            previous: "0".repeat(64),
            workspace: config.workspace.clone(),
            vault,
            staging,
            sync_roots: vec![],
            automatic: false,
        };
        let capabilities = Arc::new(
            vcp_lifecycle::foundation::backup_run::Capabilities::open(
                trust,
                keys,
                &setup,
                workspace,
                &config.canonical_root,
            )
            .unwrap(),
        );
        let operation = CommandId::new();
        let published = host
            .publish_backup(
                capabilities.clone(),
                operation.clone(),
                Some(vcp_store::snapshot_inputs::Inputs {
                    checkpoint: Some(checkpoint),
                    generations: vec![generation],
                }),
                Arc::new(AtomicBool::new(false)),
            )
            .await
            .unwrap();
        assert_eq!(published.stage, vcp_store::snapshot_jobs::Stage::Published);
        assert!(!published.active);
        let retry = host
            .publish_backup(
                capabilities,
                operation,
                None,
                Arc::new(AtomicBool::new(false)),
            )
            .await
            .unwrap();
        assert_eq!(retry.revision, published.revision);
        assert!(!host
            .snapshot()
            .unwrap()
            .records
            .values()
            .any(|row| row.collection == Collection::Attempt));
        // A separate recovery enrollment authenticates the actual published
        // object before cross-backend import and explicit workspace recovery.
        let restored_keys = vcp_store::keys::LocalKeys::import(&copy)
            .unwrap()
            .verify_recovery(&copy)
            .unwrap();
        let restored_trust = vcp_store::vault_publish::LocalTrust::enroll(
            &restored_keys,
            config.workspace.clone(),
            "e".repeat(64),
            vcp_store::vault_publish::Checkpoint {
                sequence: 0,
                deletion: 0,
                parent: None,
            },
        )
        .unwrap();
        let restore_staging = temp.path().join("restore-staging");
        let restored_roots = temp.path().join("restored-roots");
        std::fs::create_dir(&restore_staging).unwrap();
        std::fs::create_dir(&restored_roots).unwrap();
        let restore_operation = CommandId::new();
        let restored_root = restored_roots.join(restore_operation.as_str());
        let receipt = published.publication.as_ref().unwrap();
        let object = setup.vault.join(receipt["object"].as_str().unwrap());
        // Opt-in native CLI qualification artifacts stay outside tracked source.
        // Export recovery through the private-directory API, never by copying
        // secret files or serializing key material into the public manifest.
        if let Some(export) = std::env::var_os("VCP_TEST_PORTABILITY_EXPORT") {
            let export = std::path::PathBuf::from(export).join(match backend {
                BackendKind::Files => "files",
                BackendKind::Sqlite => "sqlite",
            });
            std::fs::create_dir_all(&export).unwrap();
            let recovery_directory = export.join("recovery");
            std::fs::create_dir(&recovery_directory).unwrap();
            let export_keys = vcp_store::keys::LocalKeys::import(&copy).unwrap();
            export_keys
                .export_recovery(
                    &vcp_store::keys::RecoveryDirectory::open(
                        &recovery_directory,
                        &[workspace.to_owned(), config.canonical_root.clone()],
                    )
                    .unwrap(),
                )
                .unwrap();
            let ciphertext = export.join("snapshot.age");
            std::fs::copy(&object, &ciphertext).unwrap();
            std::fs::write(
                export.join("fixture.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "workspace": config.workspace,
                    "lineage": "e".repeat(64),
                    "checkpoint": {"sequence": 0, "deletion": 0, "parent": null},
                    "source": ciphertext,
                    "recovery_directory": recovery_directory,
                    "ciphertext_sha256": receipt["ciphertext_sha256"],
                    "bytes": receipt["bytes"],
                    "expected_files": {"tracked.txt": "unstaged\n", "untracked.txt": "required untracked bytes\n"}
                }))
                .unwrap(),
            )
            .unwrap();
        }
        let restore_forbidden = vec![
            workspace.to_owned(),
            config.canonical_root.clone(),
            setup.vault.clone(),
            setup.staging.clone(),
            recovery.clone(),
            trust_path.clone(),
        ];
        let mut restore = vcp_store::restore_stage::Restore::begin(
            &restore_staging,
            &restore_forbidden,
            restore_operation.clone(),
            &restored_trust,
            receipt["ciphertext_sha256"].as_str().unwrap().into(),
            receipt["bytes"].as_u64().unwrap(),
        )
        .unwrap();
        restore.acquire(&object, &|| false).unwrap();
        let validated = restore
            .authenticate(
                &restored_trust,
                &copy,
                vcp_store::vault_crypto::Limits::default(),
                &|| false,
            )
            .unwrap();
        let target_backend = if backend == BackendKind::Files {
            BackendKind::Sqlite
        } else {
            BackendKind::Files
        };
        let imported = restore
            .import(
                &validated,
                &restored_trust,
                target_backend,
                &restored_root,
                config.actor.clone(),
                vcp_domain::Timestamp::new(2000),
                &restore_forbidden,
                &|| false,
            )
            .await
            .unwrap();
        let destination = temp.path().join("materialized-workspace");
        let cancellation = Arc::new(AtomicBool::new(false));
        let materialized = vcp_lifecycle::foundation::restore_workspace::materialize(
            &imported,
            &destination,
            &restore_operation,
            &restore_forbidden,
            cancellation.clone(),
        )
        .await
        .unwrap();
        materialized.revalidate().unwrap();
        assert!(!cancellation.load(std::sync::atomic::Ordering::Acquire));
        assert_eq!(materialized.state_digest(), imported.state_digest());
        assert_eq!(materialized.source_manifest(), imported.source_manifest());
        assert_eq!(
            std::fs::read(destination.join("tracked.txt")).unwrap(),
            b"unstaged\n"
        );
        assert_eq!(
            std::fs::read(destination.join("untracked.txt")).unwrap(),
            b"required untracked bytes\n"
        );
        assert_eq!(std::fs::read(workspace.join(".git/index")).unwrap(), index);
        let restored = imported.reopen_verified().await.unwrap();
        let before_configuration = restored.state().clone();
        let descriptor = vcp_lifecycle::foundation::restore_workspace::restored_configuration(
            &restored,
            &imported,
            &materialized,
            config.actor.clone(),
            vcp_domain::HostId::new(),
            None,
        )
        .unwrap();
        assert_eq!(descriptor.workspace, config.workspace);
        assert_eq!(descriptor.session, config.session);
        assert_eq!(descriptor.root_task, config.root_task);
        assert_eq!(
            std::path::Path::new(&descriptor.binding.root),
            materialized.root().path()
        );
        assert_eq!(descriptor.price.valid_until, vcp_domain::Timestamp::ZERO);
        assert!(descriptor.price.rates.is_empty());
        assert_eq!(restored.state(), &before_configuration);
        let restored_workspace: vcp_domain::workspace::Workspace = restored
            .state()
            .record(
                Collection::Workspace,
                config.workspace.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(restored_workspace.trust, Trust::Untrusted);
        assert!(!restored
            .state()
            .records
            .values()
            .any(|row| row.collection == Collection::Attempt));
        restored.close().await.unwrap();
        drop(materialized);
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        drop(test);
        drop(host);
    }
}
