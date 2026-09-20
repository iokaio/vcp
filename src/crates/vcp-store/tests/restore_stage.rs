// SPDX-License-Identifier: Apache-2.0
mod common;
use common::*;
use vcp_domain::{
    policy::{
        AuthorityData, AuthorityDocument, AuthorityFormat, Grant, GrantScope, GrantTarget,
        RuleOrigin,
    },
    task::{Task, TaskState},
    workspace::{Trust, Workspace},
    ActorId, AuthorityId, CommandId, GrantId, PolicyRevision, Revision, Timestamp,
};
use vcp_store::{
    contract::*,
    keys::{LocalKeys, RecoveryDirectory},
    portable_snapshot::Archive,
    restore_stage::{Restore, Stage},
    vault_crypto::{Limits, Manifest, Object, PrivateStaging, FORMAT},
    vault_publish::{Checkpoint, LocalTrust},
    BackendKind, Store,
};

#[tokio::test]
async fn authenticated_restore_preserves_history_and_sanitizes_before_cross_backend_handoff() {
    for (source_backend, target_backend) in [
        (BackendKind::Files, BackendKind::Sqlite),
        (BackendKind::Sqlite, BackendKind::Files),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let paths: [std::path::PathBuf; 4] =
            ["stage", "recovery", "restore", "vault"].map(|s| temp.path().join(s));
        for path in &paths {
            std::fs::create_dir(path).unwrap();
        }
        let forbidden = vec![paths[3].clone()];
        let recovery = RecoveryDirectory::open(&paths[1], &forbidden).unwrap();
        let keys = LocalKeys::generate().unwrap();
        let copy = keys.export_recovery(&recovery).unwrap();
        let keys = keys.verify_recovery(&copy).unwrap();
        let trust = LocalTrust::enroll(
            &keys,
            workspace().id,
            "a".repeat(64),
            Checkpoint {
                sequence: 0,
                deletion: 0,
                parent: None,
            },
        )
        .unwrap();
        let mut source = Store::open(&temp.path().join("source"), source_backend, &forbidden)
            .await
            .unwrap();
        let mut initial = initial();
        let grant = AuthorityDocument {
            document_type: AuthorityFormat::VcpAuthorityV1,
            schema_version: 1,
            id: AuthorityId::parse("restore-grant").unwrap(),
            workspace: workspace().id,
            revision: Revision::ZERO,
            data: AuthorityData::Grant {
                grant: Grant {
                    id: GrantId::parse("restore-grant").unwrap(),
                    actor: ActorId::parse("developer").unwrap(),
                    scope: GrantScope::Task {
                        scope: task().scope,
                    },
                    host: workspace().binding.host,
                    binding: Revision::ZERO,
                    authority: workspace().authority,
                    policy: PolicyRevision::ZERO,
                    expires_at: Timestamp::new(10000),
                    target: GrantTarget::Exact {
                        digest: "a".repeat(64),
                    },
                    origin: RuleOrigin::User,
                    reason: "original host approval".into(),
                    revoked: false,
                    revision: Revision::ZERO,
                    approval: None,
                },
            },
        };
        initial.mutations.push(Mutation::Put {
            expected: None,
            record: Record::typed(
                Collection::Access,
                grant.id.as_str(),
                workspace().id,
                grant.revision,
                &grant,
            )
            .unwrap(),
        });
        source.transact(initial).await.unwrap();
        let original = source.state().clone();
        let archive = Archive::capture(
            &source,
            &source.snapshot().unwrap(),
            &workspace().id,
            &|| false,
        )
        .unwrap();
        let payloads = archive.payloads().unwrap();
        let manifest = Manifest {
            format: FORMAT.into(),
            workspace: workspace().id,
            lineage: "a".repeat(64),
            sequence: 1,
            deletion: 0,
            parent: None,
            objects: payloads
                .iter()
                .map(|(hash, bytes)| {
                    (
                        hash.clone(),
                        Object {
                            bytes: bytes.len() as u64,
                            sha256: hash.clone(),
                        },
                    )
                })
                .collect(),
        };
        let staging = PrivateStaging::open(&paths[0], &forbidden).unwrap();
        let mut ciphertext = trust
            .encrypt(
                &keys,
                &staging,
                manifest,
                payloads,
                trust.configuration().revision,
                Limits::default(),
            )
            .unwrap();
        let operation = CommandId::new();
        let target_parent = temp.path().join("canonical-roots");
        std::fs::create_dir(&target_parent).unwrap();
        let target = target_parent.join(operation.as_str());
        let mut restore = Restore::begin(
            &paths[2],
            &forbidden,
            operation,
            &trust,
            ciphertext.sha256().into(),
            ciphertext.bytes(),
        )
        .unwrap();
        let acquired_source = paths[3].join("opaque.age");
        let mut output = std::fs::File::create(&acquired_source).unwrap();
        ciphertext.copy_ciphertext(&mut output).unwrap();
        output.sync_all().unwrap();
        drop(output);
        assert!(Restore::open(&paths[2], &forbidden).is_err());
        assert!(restore.acquire(&acquired_source, &|| true).is_err());
        assert_eq!(restore.status().stage, Stage::Pending);
        restore.acquire(&acquired_source, &|| false).unwrap();
        drop(restore);
        let mut restore = Restore::open(&paths[2], &forbidden).unwrap();
        let wrong = LocalKeys::generate().unwrap();
        let wrong_copy = wrong.export_recovery(&recovery).unwrap();
        assert!(restore
            .authenticate(&trust, &wrong_copy, Limits::default(), &|| false)
            .is_err());
        assert_eq!(restore.status().stage, Stage::Acquired);
        assert_eq!(source.state(), &original);
        let proof = restore
            .authenticate(&trust, &copy, Limits::default(), &|| false)
            .unwrap();
        let imported = restore
            .import(
                &proof,
                &trust,
                target_backend,
                &target,
                ActorId::parse("developer").unwrap(),
                Timestamp::new(200),
                &forbidden,
                &|| false,
            )
            .await
            .unwrap();
        assert_eq!(restore.status().stage, Stage::Imported);
        assert!(restore.status().canonical_imported);
        assert!(!restore.status().search_ready);
        let restored = imported.reopen_verified().await.unwrap();
        let new_workspace: Workspace = restored
            .state()
            .record(
                Collection::Workspace,
                workspace().id.as_str(),
                &workspace().id,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(new_workspace.trust, Trust::Untrusted);
        assert_eq!(
            new_workspace.authority,
            workspace().authority.next().unwrap()
        );
        assert_eq!(
            new_workspace.binding.revision,
            workspace().binding.revision.next().unwrap()
        );
        let task: Task = restored
            .state()
            .record(
                Collection::Task,
                task().scope.task.as_str(),
                &workspace().id,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(task.state, TaskState::Paused);
        let restored_grant: AuthorityDocument = restored
            .state()
            .record(Collection::Access, "restore-grant", &workspace().id)
            .unwrap()
            .decode()
            .unwrap();
        assert!(
            matches!(restored_grant.data,AuthorityData::Grant {grant} if grant.revoked && grant.revision==Revision::new(1))
        );
        for (id, receipt) in &original.transactions {
            assert_eq!(restored.state().transactions.get(id), Some(receipt));
        }
        assert_eq!(
            &restored.state().events[..original.events.len()],
            original.events.as_slice()
        );
        assert_eq!(source.state(), &original);
        restored.close().await.unwrap();
        drop(imported);
        drop(restore);
        let mut restore = Restore::open(&paths[2], &forbidden).unwrap();
        let proof = restore
            .authenticate(&trust, &copy, Limits::default(), &|| false)
            .unwrap();
        let imported = restore
            .import(
                &proof,
                &trust,
                target_backend,
                &target,
                ActorId::parse("developer").unwrap(),
                Timestamp::new(999),
                &forbidden,
                &|| false,
            )
            .await
            .unwrap();
        let restored = imported.reopen_verified().await.unwrap();
        assert_eq!(
            restored.state().watermark,
            original.watermark.next().unwrap()
        );
        restored.close().await.unwrap();
        source.close().await.unwrap();
    }
}

#[tokio::test]
async fn process_kill_restore_reconciles_owned_import_without_replaying_authority_reset() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        for name in ["stage", "recovery", "restore", "vault", "canonical-roots"] {
            std::fs::create_dir(temp.path().join(name)).unwrap();
        }
        let recovery =
            RecoveryDirectory::open(&temp.path().join("recovery"), &[temp.path().join("vault")])
                .unwrap();
        let keys = LocalKeys::generate().unwrap();
        let copy = keys.export_recovery(&recovery).unwrap();
        let copy_id = copy.id().to_owned();
        let operation = CommandId::new();
        for phase in [
            "acquired",
            "validated",
            "intent",
            "partial",
            "sanitized",
            "imported",
            "finish",
        ] {
            let marker = temp.path().join(format!("barrier-{phase}"));
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "restore_process_child", "--nocapture"])
                .env("VCP_RESTORE_CHILD_ROOT", temp.path())
                .env(
                    "VCP_RESTORE_CHILD_BACKEND",
                    if backend == BackendKind::Files {
                        "files"
                    } else {
                        "sqlite"
                    },
                )
                .env("VCP_RESTORE_CHILD_COPY", &copy_id)
                .env("VCP_RESTORE_CHILD_OPERATION", operation.as_str())
                .env("VCP_RESTORE_CHILD_PHASE", phase)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::inherit())
                .spawn()
                .unwrap();
            let started = std::time::Instant::now();
            loop {
                if phase != "finish" && marker.exists() {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    break;
                }
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(
                        phase == "finish" && status.success(),
                        "restore child exited before {phase}: {status}"
                    );
                    break;
                }
                if started.elapsed() > std::time::Duration::from_secs(30) {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("restore child timeout at {phase}");
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        let target = if backend == BackendKind::Files {
            BackendKind::Sqlite
        } else {
            BackendKind::Files
        };
        let restored = Store::open(
            &temp.path().join("canonical-roots").join(operation.as_str()),
            target,
            &[temp.path().join("vault")],
        )
        .await
        .unwrap();
        assert_eq!(restored.state().watermark.get(), 3); // Initial facts, source artifact, one authority reset.
        let workspace: Workspace = restored
            .state()
            .record(
                Collection::Workspace,
                workspace().id.as_str(),
                &workspace().id,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(workspace.authority.get(), 1);
        assert_eq!(workspace.trust, Trust::Untrusted);
        restored.close().await.unwrap();
        let journal =
            Restore::open(&temp.path().join("restore"), &[temp.path().join("vault")]).unwrap();
        assert_eq!(journal.status().stage, Stage::Imported);
    }
}

#[test]
fn restore_process_child() {
    let Some(root) = std::env::var_os("VCP_RESTORE_CHILD_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let phase = std::env::var("VCP_RESTORE_CHILD_PHASE").unwrap();
    let barrier = |name: &str| {
        if phase == name {
            std::fs::write(root.join(format!("barrier-{name}")), b"durable boundary").unwrap();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    };
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        use vcp_store::artifact::ArtifactWriter;
        let source_backend = if std::env::var("VCP_RESTORE_CHILD_BACKEND").unwrap() == "files" {
            BackendKind::Files
        } else {
            BackendKind::Sqlite
        };
        let target_backend = if source_backend == BackendKind::Files {
            BackendKind::Sqlite
        } else {
            BackendKind::Files
        };
        let forbidden = vec![root.join("vault")];
        let recovery = RecoveryDirectory::open(&root.join("recovery"), &forbidden).unwrap();
        let copy = recovery
            .open_copy(&std::env::var("VCP_RESTORE_CHILD_COPY").unwrap())
            .unwrap();
        let keys = LocalKeys::import(&copy)
            .unwrap()
            .verify_recovery(&copy)
            .unwrap();
        let trust = LocalTrust::enroll(
            &keys,
            workspace().id,
            "a".repeat(64),
            Checkpoint {
                sequence: 0,
                deletion: 0,
                parent: None,
            },
        )
        .unwrap();
        let mut source = Store::open(&root.join("source"), source_backend, &forbidden)
            .await
            .unwrap();
        if source.state().watermark.get() == 0 {
            source.transact(initial()).await.unwrap();
            let mut writer = source.spool().create(spec()).unwrap();
            for _ in 0..3 {
                writer
                    .write_chunk(&vec![b'x'; vcp_store::artifact::CHUNK_BYTES])
                    .unwrap();
            }
            let descriptor = writer.finalize().unwrap();
            drop(writer);
            source
                .transact(attach(source.state(), descriptor, None))
                .await
                .unwrap();
        }
        let ciphertext = root.join("vault").join("opaque.age");
        if !ciphertext.exists() {
            let archive = Archive::capture(
                &source,
                &source.snapshot().unwrap(),
                &workspace().id,
                &|| false,
            )
            .unwrap();
            let payloads = archive.payloads().unwrap();
            let manifest = Manifest {
                format: FORMAT.into(),
                workspace: workspace().id,
                lineage: "a".repeat(64),
                sequence: 1,
                deletion: 0,
                parent: None,
                objects: payloads
                    .iter()
                    .map(|(id, b)| {
                        (
                            id.clone(),
                            Object {
                                bytes: b.len() as u64,
                                sha256: id.clone(),
                            },
                        )
                    })
                    .collect(),
            };
            let staging = PrivateStaging::open(&root.join("stage"), &forbidden).unwrap();
            let mut encrypted = trust
                .encrypt(&keys, &staging, manifest, payloads, 0, Limits::default())
                .unwrap();
            let mut file = std::fs::File::create(&ciphertext).unwrap();
            encrypted.copy_ciphertext(&mut file).unwrap();
            file.sync_all().unwrap();
        }
        let operation =
            CommandId::parse(std::env::var("VCP_RESTORE_CHILD_OPERATION").unwrap()).unwrap();
        let destination = root.join("canonical-roots").join(operation.as_str());
        let mut restore = if root
            .join("restore")
            .join("restore-00000000000000000000.json")
            .exists()
        {
            Restore::open(&root.join("restore"), &forbidden).unwrap()
        } else {
            let bytes = std::fs::read(&ciphertext).unwrap();
            Restore::begin(
                &root.join("restore"),
                &forbidden,
                operation,
                &trust,
                vcp_protocol::digest_bytes(&bytes),
                bytes.len() as u64,
            )
            .unwrap()
        };
        if restore.status().stage == Stage::Pending {
            restore.acquire(&ciphertext, &|| false).unwrap();
        }
        barrier("acquired");
        let proof = restore
            .authenticate(&trust, &copy, Limits::default(), &|| false)
            .unwrap();
        barrier("validated");
        let artifact = source
            .state()
            .records
            .values()
            .find(|r| r.collection == Collection::Artifact)
            .unwrap()
            .id
            .clone();
        let cancel = || {
            if phase == "intent" {
                let path = root
                    .join("restore")
                    .join("restore-00000000000000000003.json");
                if path.exists() {
                    barrier("intent");
                }
            }
            if phase == "partial" {
                let path = destination.join("spool").join(&artifact);
                if std::fs::read_dir(path).is_ok_and(|entries| {
                    entries
                        .filter_map(Result::ok)
                        .any(|entry| entry.file_name().to_string_lossy().ends_with(".chunk"))
                }) {
                    barrier("partial");
                }
            }
            if phase == "sanitized" && destination.join("format.json").exists() {
                barrier("sanitized");
            }
            false
        };
        let imported = restore
            .import(
                &proof,
                &trust,
                target_backend,
                &destination,
                ActorId::parse("developer").unwrap(),
                Timestamp::new(200),
                &forbidden,
                &cancel,
            )
            .await
            .unwrap();
        let reopened = imported.reopen_verified().await.unwrap();
        reopened.close().await.unwrap();
        barrier("imported");
        source.close().await.unwrap();
    });
}
