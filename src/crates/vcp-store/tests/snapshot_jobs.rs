// SPDX-License-Identifier: Apache-2.0
mod common;
use common::*;
use vcp_domain::{artifact::ArtifactDescriptor, CommandId, Revision};
use vcp_store::{
    artifact::ArtifactWriter,
    contract::*,
    keys::{LocalKeys, RecoveryDirectory},
    portable_snapshot::Archive,
    snapshot_jobs::{Jobs, Stage},
    vault_crypto::{Limits, PrivateStaging},
    vault_publish::{Checkpoint, LocalTrust, Vault},
    BackendKind, Store,
};

#[tokio::test]
async fn exact_snapshot_restarts_publishes_and_stages_history_on_both_backends() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let paths: [std::path::PathBuf; 5] =
            ["jobs", "stage", "vault", "recovery", "import"].map(|s| temp.path().join(s));
        for p in &paths {
            std::fs::create_dir(p).unwrap();
        }
        let forbidden = vec![paths[2].clone()];
        let jobs = Jobs::open(&paths[0], &forbidden).unwrap();
        let staging = PrivateStaging::open(&paths[1], &forbidden).unwrap();
        let vault = Vault::open(
            &paths[2],
            &[
                paths[0].clone(),
                paths[1].clone(),
                paths[3].clone(),
                paths[4].clone(),
            ],
        )
        .unwrap();
        let recovery = RecoveryDirectory::open(&paths[3], &forbidden).unwrap();
        let key = LocalKeys::generate().unwrap();
        let copy = key.export_recovery(&recovery).unwrap();
        let key = key.verify_recovery(&copy).unwrap();
        let workspace = workspace().id;
        let mut trust = LocalTrust::enroll(
            &key,
            workspace.clone(),
            "a".repeat(64),
            Checkpoint {
                sequence: 0,
                deletion: 0,
                parent: None,
            },
        )
        .unwrap();
        let root = temp.path().join("canonical");
        let mut store = Store::open(&root, backend, &forbidden).await.unwrap();
        store.transact(initial()).await.unwrap();
        let spec = spec();
        let mut writer = store.spool().create(spec.clone()).unwrap();
        writer.write_chunk(b"exact original source").unwrap();
        let descriptor = writer.finalize().unwrap();
        drop(writer);
        store
            .transact(attach(store.state(), descriptor.clone(), None))
            .await
            .unwrap();
        let original = store.state().clone();
        let id = CommandId::new();
        let capture = jobs
            .begin(&mut store, id.clone(), &workspace, &trust)
            .await
            .unwrap();
        assert!(store.try_snapshot_cleanup_guard().unwrap().is_none());
        drop(capture);
        store.close().await.unwrap();
        let mut store = Store::open(&root, backend, &forbidden).await.unwrap();
        let capture = jobs
            .begin(&mut store, id.clone(), &workspace, &trust)
            .await
            .unwrap();
        let prepared = jobs.prepare(&store, capture, &|| false).unwrap();
        let job = jobs
            .accept_prepared(&mut store, &workspace, prepared)
            .await
            .unwrap();
        // Lost accept reply: repeat deterministic private preparation does not
        // select a newer canonical cut or re-encrypt an existing random stream.
        let encrypted = jobs
            .encrypt(&job, &trust, &key, &staging, &|| false)
            .unwrap();
        drop(encrypted);
        store.close().await.unwrap();
        let mut store = Store::open(&root, backend, &forbidden).await.unwrap();
        let job = Jobs::inspect(&store, &id, &workspace).unwrap();
        let encrypted = jobs
            .encrypt(&job, &trust, &key, &staging, &|| false)
            .unwrap();
        jobs.accept_encrypted(&mut store, &workspace, encrypted)
            .await
            .unwrap();
        let (job, mut ciphertext, permit) = jobs
            .admit(&mut store, &id, &workspace, &trust)
            .await
            .unwrap();
        assert_eq!(job.stage, Stage::Admitted);
        let receipt = vault
            .publish(&mut ciphertext, &permit, &|| false, &|_| {})
            .unwrap();
        drop(ciphertext);
        // Simulate lost publication reply then reopen/retry exact immutable bytes.
        store.close().await.unwrap();
        let mut store = Store::open(&root, backend, &forbidden).await.unwrap();
        let (_, mut ciphertext, permit) = jobs
            .admit(&mut store, &id, &workspace, &trust)
            .await
            .unwrap();
        let retried = vault
            .publish(&mut ciphertext, &permit, &|| false, &|_| {})
            .unwrap();
        drop(ciphertext);
        assert_eq!(receipt.ciphertext_sha256, retried.ciphertext_sha256);
        let restored = trust
            .verify_restore(&paths[2].join(&receipt.object), &copy, Limits::default())
            .unwrap();
        let archive = Archive::decode(
            restored.restored().payloads.clone(),
            job.inventory.as_deref().unwrap(),
        )
        .unwrap();
        assert_eq!(archive.state(), &original);
        let staged = archive
            .stage_history(&paths[4], &forbidden, &|| false)
            .unwrap();
        assert_eq!(staged.state(), &original);
        assert!(Store::open(staged.root(), backend, &forbidden)
            .await
            .is_err());
        let spool =
            vcp_store::artifact::Spool::open(&staged.root().join("spool"), &forbidden, 1024 * 1024)
                .unwrap();
        let mut bytes = Vec::new();
        spool.read(&descriptor, &mut bytes).unwrap();
        assert_eq!(bytes, b"exact original source");
        jobs.complete(&mut store, &workspace, &receipt)
            .await
            .unwrap();
        trust.advance_after_publication(&receipt, 0).unwrap();
        assert!(trust
            .verify_restore(&paths[2].join(&receipt.object), &copy, Limits::default())
            .is_err());
        trust
            .verify_known_head(
                &paths[2].join(&receipt.object),
                &copy,
                &restored.restored().manifest,
                Limits::default(),
            )
            .unwrap();
        let released = jobs
            .release(&mut store, &id, &workspace, false)
            .await
            .unwrap();
        assert!(!released.active);
        assert!(released.pins.is_empty());
        let row = store
            .state()
            .record(Collection::Artifact, spec.id.as_str(), &workspace)
            .unwrap();
        assert_eq!(row.decode::<ArtifactDescriptor>().unwrap(), descriptor);
        assert_eq!(row.revision, Revision::ZERO);
        store.close().await.unwrap();
    }
}

#[tokio::test]
async fn stale_deletion_fence_and_foreign_workspace_fail_before_vault_copy() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let jobs_path = temp.path().join("jobs");
        let stage = temp.path().join("stage");
        let vault = temp.path().join("vault");
        let recovery = temp.path().join("recovery");
        for p in [&jobs_path, &stage, &vault, &recovery] {
            std::fs::create_dir(p).unwrap();
        }
        let forbidden = vec![vault.clone()];
        let jobs = Jobs::open(&jobs_path, &forbidden).unwrap();
        let staging = PrivateStaging::open(&stage, &forbidden).unwrap();
        let directory = RecoveryDirectory::open(&recovery, &forbidden).unwrap();
        let keys = LocalKeys::generate().unwrap();
        let copy = keys.export_recovery(&directory).unwrap();
        let keys = keys.verify_recovery(&copy).unwrap();
        let ws = workspace();
        let trust = LocalTrust::enroll(
            &keys,
            ws.id.clone(),
            "a".repeat(64),
            Checkpoint {
                sequence: 0,
                deletion: 0,
                parent: None,
            },
        )
        .unwrap();
        let mut store = Store::open(&temp.path().join("canonical"), backend, &forbidden)
            .await
            .unwrap();
        store.transact(initial()).await.unwrap();
        let id = CommandId::new();
        let capture = jobs
            .begin(&mut store, id.clone(), &ws.id, &trust)
            .await
            .unwrap();
        let prepared = jobs.prepare(&store, capture, &|| false).unwrap();
        let job = jobs
            .accept_prepared(&mut store, &ws.id, prepared)
            .await
            .unwrap();
        let encrypted = jobs
            .encrypt(&job, &trust, &keys, &staging, &|| false)
            .unwrap();
        jobs.accept_encrypted(&mut store, &ws.id, encrypted)
            .await
            .unwrap();
        let mut changed = ws.clone();
        changed.deletion = changed.deletion.next().unwrap();
        changed.revision = changed.revision.next().unwrap();
        store
            .transact(Transaction {
                id: vcp_domain::TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![Mutation::Put {
                    expected: Some(ws.revision),
                    record: Record::typed(
                        Collection::Workspace,
                        ws.id.as_str(),
                        ws.id.clone(),
                        changed.revision,
                        &changed,
                    )
                    .unwrap(),
                }],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        assert!(jobs.admit(&mut store, &id, &ws.id, &trust).await.is_err());
        assert_eq!(std::fs::read_dir(&vault).unwrap().count(), 0);
        assert!(
            !jobs
                .release(&mut store, &id, &ws.id, true)
                .await
                .unwrap()
                .active
        );
        let mut foreign = workspace();
        foreign.id = vcp_domain::WorkspaceId::new();
        store
            .transact(Transaction {
                id: vcp_domain::TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![Mutation::Put {
                    expected: None,
                    record: Record::typed(
                        Collection::Workspace,
                        foreign.id.as_str(),
                        foreign.id.clone(),
                        foreign.revision,
                        &foreign,
                    )
                    .unwrap(),
                }],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        let before = store.state().watermark;
        assert!(jobs
            .begin(&mut store, CommandId::new(), &ws.id, &trust)
            .await
            .is_err());
        assert_eq!(store.state().watermark, before);
        store.close().await.unwrap();
    }
}

#[test]
fn process_kill_job_recovery_at_durable_boundaries() {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    for backend in ["files", "sqlite"] {
        let temp = tempfile::tempdir().unwrap();
        for name in ["jobs", "stage", "vault", "recovery"] {
            std::fs::create_dir(temp.path().join(name)).unwrap();
        }
        let directory =
            RecoveryDirectory::open(&temp.path().join("recovery"), &[temp.path().join("vault")])
                .unwrap();
        let key = LocalKeys::generate().unwrap();
        let copy = key.export_recovery(&directory).unwrap();
        let id = CommandId::new();
        for phase in [
            "captured",
            "prepared",
            "encrypted",
            "admitted",
            "owned",
            "partial",
            "copied",
            "completed",
        ] {
            let marker = temp.path().join(format!("{phase}.marker"));
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "snapshot_process_child", "--nocapture"])
                .env("VCP_SNAPSHOT_CHILD_ROOT", temp.path())
                .env("VCP_SNAPSHOT_CHILD_BACKEND", backend)
                .env("VCP_SNAPSHOT_CHILD_KEY", copy.id())
                .env("VCP_SNAPSHOT_CHILD_JOB", id.as_str())
                .env("VCP_SNAPSHOT_CHILD_PHASE", phase)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(30);
            while !marker.exists() {
                if let Some(status) = child.try_wait().unwrap() {
                    panic!("snapshot child exited before {phase}: {status}");
                }
                if Instant::now() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("snapshot child barrier timed out: {phase}");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            child.kill().unwrap();
            child.wait().unwrap();
        }
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let store = Store::open(
                &temp.path().join("canonical"),
                if backend == "files" {
                    BackendKind::Files
                } else {
                    BackendKind::Sqlite
                },
                &[temp.path().join("vault")],
            )
            .await
            .unwrap();
            let job = Jobs::inspect(&store, &id, &workspace().id).unwrap();
            assert_eq!(job.stage, Stage::Published);
            assert!(job.active);
            assert_eq!(
                std::fs::read_dir(temp.path().join("vault"))
                    .unwrap()
                    .count(),
                1
            );
            assert_eq!(job.watermark.get(), 2);
            assert_eq!(
                store
                    .state()
                    .record(Collection::Task, "task", &workspace().id)
                    .unwrap()
                    .revision,
                Revision::ZERO
            );
            store.close().await.unwrap();
        });
    }
}

#[test]
fn snapshot_process_child() {
    let Some(root) = std::env::var_os("VCP_SNAPSHOT_CHILD_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let target = std::env::var("VCP_SNAPSHOT_CHILD_PHASE").unwrap();
    let barrier = |phase: &str| {
        if target == phase {
            std::fs::write(root.join(format!("{phase}.marker")), b"durable").unwrap();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    };
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let forbidden = vec![root.join("vault")];
        let backend = if std::env::var("VCP_SNAPSHOT_CHILD_BACKEND").unwrap() == "files" {
            BackendKind::Files
        } else {
            BackendKind::Sqlite
        };
        let jobs = Jobs::open(&root.join("jobs"), &forbidden).unwrap();
        let staging = PrivateStaging::open(&root.join("stage"), &forbidden).unwrap();
        let recovery = RecoveryDirectory::open(&root.join("recovery"), &forbidden).unwrap();
        let copy = recovery
            .open_copy(&std::env::var("VCP_SNAPSHOT_CHILD_KEY").unwrap())
            .unwrap();
        let keys = LocalKeys::import(&copy)
            .unwrap()
            .verify_recovery(&copy)
            .unwrap();
        let ws = workspace().id;
        let trust = LocalTrust::enroll(
            &keys,
            ws.clone(),
            "a".repeat(64),
            Checkpoint {
                sequence: 0,
                deletion: 0,
                parent: None,
            },
        )
        .unwrap();
        let id = CommandId::parse(std::env::var("VCP_SNAPSHOT_CHILD_JOB").unwrap()).unwrap();
        let mut store = Store::open(&root.join("canonical"), backend, &forbidden)
            .await
            .unwrap();
        if store.state().watermark.get() == 0 {
            store.transact(initial()).await.unwrap();
            let spec = spec();
            let mut writer = store.spool().create(spec).unwrap();
            for chunk in b"retained synthetic snapshot source"
                .repeat(9000)
                .chunks(vcp_store::artifact::CHUNK_BYTES)
            {
                writer.write_chunk(chunk).unwrap();
            }
            let descriptor = writer.finalize().unwrap();
            drop(writer);
            store
                .transact(attach(store.state(), descriptor, None))
                .await
                .unwrap();
        }
        let job = if let Ok(job) = Jobs::inspect(&store, &id, &ws) {
            job
        } else {
            let capture = jobs
                .begin(&mut store, id.clone(), &ws, &trust)
                .await
                .unwrap();
            drop(capture);
            Jobs::inspect(&store, &id, &ws).unwrap()
        };
        barrier("captured");
        let job = if job.stage == Stage::Captured {
            let capture = jobs.resume_capture(&store, &job).unwrap();
            let prepared = jobs.prepare(&store, capture, &|| false).unwrap();
            jobs.accept_prepared(&mut store, &ws, prepared)
                .await
                .unwrap()
        } else {
            job
        };
        barrier("prepared");
        let job = if job.stage == Stage::ArchiveReady {
            let encrypted = jobs
                .encrypt(&job, &trust, &keys, &staging, &|| false)
                .unwrap();
            jobs.accept_encrypted(&mut store, &ws, encrypted)
                .await
                .unwrap()
        } else {
            job
        };
        barrier("encrypted");
        assert!(matches!(
            job.stage,
            Stage::CiphertextReady | Stage::Admitted
        ));
        let (admitted, mut ciphertext, permit) =
            jobs.admit(&mut store, &id, &ws, &trust).await.unwrap();
        barrier("admitted");
        let vault = Vault::open(
            &root.join("vault"),
            &[
                root.join("jobs"),
                root.join("stage"),
                root.join("recovery"),
                root.join("canonical"),
            ],
        )
        .unwrap();
        let prior = admitted.copy_identity().cloned();
        let store_ref = &mut store;
        let receipt = vault
            .publish_recorded(
                &mut ciphertext,
                &permit,
                prior.as_ref(),
                |identity| async {
                    jobs.record_copy(store_ref, &id, &ws, identity).await?;
                    barrier("owned");
                    Ok(())
                },
                &|| false,
                &|phase| {
                    if matches!(phase, vcp_store::vault_publish::Phase::CiphertextBytes(_)) {
                        barrier("partial");
                    }
                },
            )
            .await
            .unwrap();
        drop(ciphertext);
        barrier("copied");
        jobs.complete(&mut store, &ws, &receipt).await.unwrap();
        barrier("completed");
    });
}

#[cfg(windows)]
#[tokio::test]
async fn native_dirty_and_untracked_checkpoint_is_complete_or_backup_is_refused() {
    use std::{collections::BTreeMap, ffi::OsString, process::Command, time::Duration};
    use vcp_store::snapshot_inputs::{Checkpoint as WorkspaceCheckpoint, GitArtifacts, Inputs};
    async fn capture(store: &mut Store, bytes: &[u8], schema: &str) -> vcp_domain::ArtifactId {
        let mut spec = spec();
        spec.schema = schema.into();
        let id = spec.id.clone();
        let mut writer = store.spool().create(spec).unwrap();
        writer.write_chunk(bytes).unwrap();
        let descriptor = writer.finalize().unwrap();
        drop(writer);
        let mut transaction = attach(store.state(), descriptor, None);
        if schema == "vcp-workspace-checkpoint/1" {
            let value: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            if let Mutation::Put { record, .. } = &mut transaction.mutations[0] {
                for entry in value["sources"].as_array().unwrap() {
                    record.references.insert(key(
                        Collection::Artifact,
                        entry["artifact"].as_str().unwrap(),
                    ));
                }
                for entry in value["git_artifacts"].as_object().unwrap().values() {
                    record
                        .references
                        .insert(key(Collection::Artifact, entry.as_str().unwrap()));
                }
            }
        }
        store.transact(transaction).await.unwrap();
        id
    }
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let checkout = temp.path().join("checkout");
        let vault = temp.path().join("vault");
        let recovery = temp.path().join("recovery");
        let jobs_path = temp.path().join("jobs");
        for path in [&checkout, &vault, &recovery, &jobs_path] {
            std::fs::create_dir(path).unwrap();
        }
        let git_exe = std::env::var_os("VCP_TEST_GIT").unwrap();
        let git = |args: &[&str]| {
            assert!(Command::new(&git_exe)
                .current_dir(&checkout)
                .args(args)
                .output()
                .unwrap()
                .status
                .success())
        };
        git(&["init", "--quiet"]);
        std::fs::write(checkout.join("tracked.txt"), b"staged bytes\n").unwrap();
        git(&["add", "tracked.txt"]);
        std::fs::write(checkout.join("tracked.txt"), b"dirty bytes\n").unwrap();
        std::fs::write(checkout.join("new.txt"), b"untracked bytes\n").unwrap();
        let index = std::fs::read(checkout.join(".git/index")).unwrap();
        let environment: BTreeMap<OsString, OsString> =
            ["SystemRoot", "WINDIR", "PATH", "TEMP", "TMP"]
                .into_iter()
                .filter_map(|name| std::env::var_os(name).map(|value| (name.into(), value)))
                .collect();
        let git = vcp_repository::git::Git::new(
            git_exe.into(),
            environment,
            Duration::from_secs(10),
            2 * 1024 * 1024,
        )
        .unwrap();
        let ws = workspace();
        let root = vcp_repository::Root::open(
            vcp_repository::RootIdentity {
                workspace: ws.id.clone(),
                root: vcp_domain::RootId::new(),
                repository: ws.binding.repository.clone(),
                worktree: ws.binding.worktree.clone(),
                binding: ws.binding.revision,
            },
            &checkout,
        )
        .unwrap();
        let observed = root
            .observe(Some(&git), &vcp_repository::discovery::Limits::default())
            .await
            .unwrap();
        assert_eq!(std::fs::read(checkout.join(".git/index")).unwrap(), index);
        let forbidden = vec![vault.clone()];
        let mut store = Store::open(&temp.path().join("canonical"), backend, &forbidden)
            .await
            .unwrap();
        store.transact(initial()).await.unwrap();
        let mut sources = BTreeMap::new();
        for source in &observed.sources {
            sources.insert(
                source.version.path.clone(),
                capture(&mut store, &source.bytes, "native-source/1").await,
            );
        }
        let g = observed.git.as_ref().unwrap();
        let git_artifacts = GitArtifacts {
            status: capture(&mut store, &g.status, "git-status/1").await,
            index: capture(&mut store, &g.index, "git-index/1").await,
            staged_diff: capture(&mut store, &g.staged_diff, "git-staged-diff/1").await,
            unstaged_diff: capture(&mut store, &g.unstaged_diff, "git-unstaged-diff/1").await,
        };
        let manifest = capture(
            &mut store,
            &vcp_protocol::canonical_bytes(&serde_json::json!({"manifest":observed.manifest,"sources":observed.sources.iter().map(|s|serde_json::json!({"artifact":sources[&s.version.path],"version":s.version})).collect::<Vec<_>>(),"git_artifacts":git_artifacts})).unwrap(),
            "vcp-workspace-checkpoint/1",
        )
        .await;
        observed
            .manifest
            .revalidate_selected(&root, &observed.manifest.files)
            .unwrap();
        let inputs = Inputs {
            checkpoint: Some(WorkspaceCheckpoint {
                manifest,
                sources: sources.clone(),
                git: Some(git_artifacts),
            }),
            generations: vec![],
        };
        let jobs = Jobs::open(&jobs_path, &forbidden).unwrap();
        let directory = RecoveryDirectory::open(&recovery, &forbidden).unwrap();
        let key = LocalKeys::generate().unwrap();
        let copy = key.export_recovery(&directory).unwrap();
        let keys = key.verify_recovery(&copy).unwrap();
        let trust = LocalTrust::enroll(
            &keys,
            ws.id.clone(),
            "a".repeat(64),
            Checkpoint {
                sequence: 0,
                deletion: 0,
                parent: None,
            },
        )
        .unwrap();
        assert!(jobs
            .begin_with_inputs(
                &mut store,
                CommandId::new(),
                &ws.id,
                &trust,
                Inputs::default()
            )
            .await
            .is_err());
        let mut missing = inputs.clone();
        missing
            .checkpoint
            .as_mut()
            .unwrap()
            .sources
            .remove("new.txt");
        assert!(jobs
            .begin_with_inputs(&mut store, CommandId::new(), &ws.id, &trust, missing)
            .await
            .is_err());
        let snap = store.snapshot().unwrap();
        let archive =
            Archive::capture_with_inputs(&store, &snap, &ws.id, &inputs, &|| false).unwrap();
        assert!(archive.coverage().workspace_checkpoint);
        assert_eq!(
            archive.retained_artifact(&sources["tracked.txt"]).unwrap(),
            b"dirty bytes\n"
        );
        assert_eq!(
            archive.retained_artifact(&sources["new.txt"]).unwrap(),
            b"untracked bytes\n"
        );
        let roundtrip = Archive::decode(
            archive.payloads().unwrap(),
            &archive.inventory_digest().unwrap(),
        )
        .unwrap();
        assert_eq!(roundtrip.inputs(), &inputs);
        let id = CommandId::new();
        let prepared_capture = jobs
            .begin_with_inputs(&mut store, id, &ws.id, &trust, inputs)
            .await
            .unwrap();
        jobs.prepare(&store, prepared_capture, &|| false).unwrap();
        drop(snap);
        store.close().await.unwrap();
    }
}

#[tokio::test]
async fn derivative_inventory_preserves_exact_bytes_but_never_claims_destination_readiness() {
    use std::collections::BTreeMap;
    use vcp_domain::{
        search::{Active, Generation, ACTIVE, GENERATION},
        AuthorityRevision, DeletionEpoch, GenerationId, MemorySeq, TransactionId,
    };
    use vcp_store::snapshot_inputs::{Compatibility, GenerationInput, Inputs};
    async fn artifact(
        store: &mut Store,
        bytes: &[u8],
        origin: Option<&vcp_domain::ArtifactId>,
    ) -> vcp_domain::ArtifactId {
        let spec = spec();
        let id = spec.id.clone();
        let mut writer = store.spool().create(spec).unwrap();
        writer.write_chunk(bytes).unwrap();
        let d = writer.finalize().unwrap();
        drop(writer);
        let mut transaction = attach(store.state(), d, None);
        if let Some(origin) = origin {
            if let Mutation::Put { record, .. } = &mut transaction.mutations[0] {
                record
                    .references
                    .insert(key(Collection::Artifact, origin.as_str()));
            }
        }
        store.transact(transaction).await.unwrap();
        id
    }
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("vault");
        let staged = temp.path().join("staged");
        std::fs::create_dir(&vault).unwrap();
        std::fs::create_dir(&staged).unwrap();
        let forbidden = vec![vault];
        let mut store = Store::open(&temp.path().join("canonical"), backend, &forbidden)
            .await
            .unwrap();
        store.transact(initial()).await.unwrap();
        // Opaque synthetic derivative bytes qualify preservation/checksum boundaries,
        // not Tantivy or DiskANN readability (destination must independently reopen).
        let origin = artifact(&mut store, b"original retained source", None).await;
        let inventory=vcp_protocol::canonical_bytes(&serde_json::json!({"workspace":workspace().id,"records":[{"scope":task().scope,"source":{"kind":"artifact","id":origin}}]})).unwrap();
        let component = b"synthetic lexical component";
        let vectors = b"synthetic vector bytes";
        let inventory_id = artifact(&mut store, &inventory, Some(&origin)).await;
        let component_id = artifact(&mut store, component, Some(&origin)).await;
        let vector_id = artifact(&mut store, vectors, Some(&origin)).await;
        let lexical=vcp_protocol::canonical_bytes(&serde_json::json!({"schema":1,"tokenizer":"code/1","engine":"qualified-engine-reopen-required","inventory":"a".repeat(64),"count":0,"components":[{"name":"segment.bin","bytes":component.len(),"sha256":vcp_protocol::digest_bytes(component)}]})).unwrap();
        let lexical_id = artifact(&mut store, &lexical, Some(&origin)).await;
        let id = GenerationId::new();
        let tx = TransactionId::new();
        let generation = Generation {
            document_type: GENERATION.into(),
            schema_version: 1,
            id: id.clone(),
            scope: task().scope,
            revision: Revision::ZERO,
            transaction: tx.clone(),
            previous: None,
            canonical_watermark: store.state().watermark,
            memory_seq: MemorySeq::ZERO,
            authority: AuthorityRevision::ZERO,
            deletion: DeletionEpoch::ZERO,
            inventory_digest: "a".repeat(64),
            inventory_checksum: vcp_protocol::digest_bytes(&inventory),
            lexical_schema: 1,
            tokenizer: "code/1".into(),
            lexical_checksum: vcp_protocol::digest_bytes(&lexical),
            embedding_specification: "b".repeat(64),
            vector_checksum: Some(vcp_protocol::digest_bytes(vectors)),
            empty_complete: false,
            vector_deficits: vec![],
            covered_intents: vec![],
        };
        let active = Active {
            document_type: ACTIVE.into(),
            schema_version: 1,
            id: workspace().id.clone(),
            workspace: workspace().id.clone(),
            revision: Revision::ZERO,
            generation: id.clone(),
            transaction: tx.clone(),
        };
        store
            .transact(Transaction {
                id: tx,
                expected_watermark: store.state().watermark,
                mutations: vec![
                    Mutation::Put {
                        expected: None,
                        record: Record::typed(
                            Collection::Generation,
                            id.as_str(),
                            workspace().id,
                            Revision::ZERO,
                            &generation,
                        )
                        .unwrap(),
                    },
                    Mutation::Put {
                        expected: None,
                        record: Record::typed(
                            Collection::Generation,
                            workspace().id.as_str(),
                            workspace().id,
                            Revision::ZERO,
                            &active,
                        )
                        .unwrap(),
                    },
                ],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        let snapshot = store.snapshot().unwrap();
        let omitted = Archive::capture(&store, &snapshot, &workspace().id, &|| false).unwrap();
        assert_eq!(
            omitted.coverage().generations[0].compatibility,
            Compatibility::RebuildRequired
        );
        let inputs = Inputs {
            checkpoint: None,
            generations: vec![GenerationInput {
                id: id.clone(),
                inventory: inventory_id,
                lexical_manifest: lexical_id,
                lexical_files: BTreeMap::from([("segment.bin".into(), component_id)]),
                vectors: Some(vector_id),
            }],
        };
        let archive =
            Archive::capture_with_inputs(&store, &snapshot, &workspace().id, &inputs, &|| false)
                .unwrap();
        assert_eq!(
            archive.coverage().generations[0].compatibility,
            Compatibility::PreservedReopenRequired
        );
        let generation_stage = archive
            .stage_generation(&id, &staged, &forbidden, &|| false)
            .unwrap();
        assert_eq!(
            std::fs::read(generation_stage.root().join("vectors.json")).unwrap(),
            vectors
        );
        assert_eq!(
            std::fs::read(generation_stage.root().join("lexical/segment.bin")).unwrap(),
            component
        );
        let mut missing = inputs.clone();
        missing.generations[0].lexical_files.clear();
        assert!(Archive::capture_with_inputs(
            &store,
            &snapshot,
            &workspace().id,
            &missing,
            &|| false
        )
        .is_err());
        let mut wrong = inputs.clone();
        wrong.generations[0].vectors = Some(inputs.generations[0].inventory.clone());
        assert!(
            Archive::capture_with_inputs(&store, &snapshot, &workspace().id, &wrong, &|| false)
                .is_err()
        );
        drop(snapshot);
        store.close().await.unwrap();
    }
}
