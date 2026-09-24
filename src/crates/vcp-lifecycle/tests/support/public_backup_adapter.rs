// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use vcp_domain::policy::{Autonomy, Policy};
use vcp_engine::{rpc::RpcHost, Access};
use vcp_protocol::{
    backup_publisher as wire,
    methods::{self, Call, ResultValue},
};

fn id(s: &str) -> methods::Id {
    s.to_owned().try_into().unwrap()
}
fn scope(c: &Config) -> methods::Scope {
    methods::Scope {
        workspace: id(c.workspace.as_str()),
        session: id(c.session.as_str()),
    }
}
fn workspace(h: &CanonicalHost, c: &Config) -> Workspace {
    h.snapshot()
        .unwrap()
        .record(Collection::Workspace, c.workspace.as_str(), &c.workspace)
        .unwrap()
        .decode()
        .unwrap()
}
fn access(h: &CanonicalHost, c: &Config, write: bool) -> Access {
    Access {
        actor: c.actor.clone(),
        workspace: c.workspace.clone(),
        session: c.session.clone(),
        authority: workspace(h, c).authority,
        read: true,
        write,
        bootstrap: false,
    }
}
fn mutation(h: &CanonicalHost, c: &Config, command: &CommandId) -> methods::Mutation {
    methods::Mutation {
        command_id: id(command.as_str()),
        expected_revision: workspace(h, c).revision.get().into(),
        steering_revision: 0.into(),
    }
}
fn read(c: &Config, operation: &CommandId) -> Call {
    Call::BackupRead(wire::Read {
        scope: scope(c),
        operation: id(operation.as_str()),
    })
}
fn job(value: ResultValue) -> wire::JobView {
    let ResultValue::BackupJob(value) = value else {
        panic!("backup job required")
    };
    value
}
fn status(value: ResultValue) -> wire::StatusView {
    let ResultValue::BackupStatus(value) = value else {
        panic!("backup status required")
    };
    value
}

pub(super) fn load(h: &CanonicalHost, c: &Config, temporary: &Path) -> PathBuf {
    let root = Path::new(&c.binding.root);
    let executable = PathBuf::from(std::env::var_os("VCP_TEST_GIT").unwrap());
    std::fs::write(root.join("tracked.txt"), b"retained fixture source\n").unwrap();
    for args in [vec!["init", "--quiet"], vec!["add", "--", "tracked.txt"]] {
        assert!(std::process::Command::new(&executable)
            .current_dir(root)
            .args(args)
            .output()
            .unwrap()
            .status
            .success());
    }
    std::fs::write(root.join("tracked.txt"), b"dirty fixture source\n").unwrap();
    let git = Arc::new(
        vcp_repository::git::Git::new(
            executable,
            ["PATH", "SystemRoot", "WINDIR", "TEMP", "TMP"]
                .into_iter()
                .filter_map(|n| std::env::var_os(n).map(|v| (n.into(), v)))
                .collect(),
            Duration::from_secs(15),
            4 * 1024 * 1024,
        )
        .unwrap(),
    );
    let staging = temporary.join("private-staging");
    let vault = temporary.join("encrypted-vault");
    let recovery = temporary.join("private-recovery");
    let trust_path = temporary.join("private-trust");
    for path in [&staging, &vault, &recovery, &trust_path] {
        std::fs::create_dir(path).unwrap();
    }
    let forbidden = vec![root.to_owned(), c.canonical_root.clone()];
    let keys = vcp_store::keys::LocalKeys::generate().unwrap();
    let copy = keys
        .export_recovery(&vcp_store::keys::RecoveryDirectory::open(&recovery, &forbidden).unwrap())
        .unwrap();
    let keys = keys.verify_recovery(&copy).unwrap();
    let enrolled = vcp_store::vault_publish::LocalTrust::enroll(
        &keys,
        c.workspace.clone(),
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
        revision: 7,
        previous: "0".repeat(64),
        workspace: c.workspace.clone(),
        vault: vault.clone(),
        staging,
        sync_roots: vec![],
        automatic: false,
    };
    let capabilities = Arc::new(
        vcp_lifecycle::foundation::backup_run::Capabilities::open(
            trust,
            keys,
            &setup,
            root,
            &c.canonical_root,
        )
        .unwrap(),
    );
    h.load_backup_with_revision(capabilities, git, false, setup.revision)
        .unwrap();
    vault
}

pub(super) fn fixture(
    backend: BackendKind,
) -> (
    tempfile::TempDir,
    Config,
    CanonicalHost,
    vcp_lifecycle::foundation::CanonicalOwner,
) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("workspace");
    std::fs::create_dir(&root).unwrap();
    let c = config(
        &temp.path().join("canonical"),
        &root.canonicalize().unwrap(),
        backend,
    );
    let (h, owner) = CanonicalHost::open(c.clone()).unwrap();
    let binding = task(&h, &c, c.root_task.clone(), None);
    h.command(
        Command::SetWorkspaceTrust {
            trust: Trust::Trusted,
        },
        None,
        Revision::ZERO,
    )
    .unwrap();
    h.command(
        Command::SetPolicy {
            policy: Policy {
                workspace: c.workspace.clone(),
                revision: PolicyRevision::ZERO,
                mode: Autonomy::Workspace,
                denials: vec![],
                workspace_roots: BTreeSet::from([RootId::parse(c.workspace.as_str()).unwrap()]),
                automatic_effects: BTreeSet::new(),
                timeout_ceiling_ms: Units::new(30000),
                output_ceiling_bytes: ByteCount::new(1024 * 1024),
            },
        },
        None,
        Revision::ZERO,
    )
    .unwrap();
    h.command(
        Command::Transition {
            next: TaskState::Paused,
            reason: "explicit local backup fixture".into(),
            verification: None,
        },
        Some(binding.scope.task),
        Revision::new(1),
    )
    .unwrap();
    (temp, c, h, owner)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn public_backup_encrypts_with_opaque_capability_and_reconciles_owned_receipts() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let (temp, c, h, owner) = fixture(backend);
        let auth = access(&h, &c, true);
        let observing = access(&h, &c, false);
        let mut controller = h.public_connection(auth.clone()).unwrap();
        controller.acquire(CommandId::new(), None).unwrap();
        let mut observer = h.public_connection(observing.clone()).unwrap();
        let unavailable = status(
            observer
                .call(
                    Call::BackupStatus(wire::StatusRequest { scope: scope(&c) }),
                    &observing,
                )
                .await
                .unwrap(),
        );
        assert!(matches!(
            unavailable.capability,
            wire::Capability::Unavailable { .. }
        ));
        let vault = load(&h, &c, temp.path());
        let ready = status(
            controller
                .call(
                    Call::BackupStatus(wire::StatusRequest { scope: scope(&c) }),
                    &auth,
                )
                .await
                .unwrap(),
        );
        let wire::Capability::Loaded {
            reference,
            generation,
            configuration_revision,
        } = ready.capability
        else {
            panic!("loaded capability")
        };
        assert_eq!(configuration_revision.as_str(), "7");
        let operation = CommandId::new();
        let create = Call::BackupCreate(wire::Create {
            scope: scope(&c),
            mutation: mutation(&h, &c, &operation),
            expected_binding_revision: workspace(&h, &c).binding.revision.get().into(),
            capability: reference.clone(),
            expected_capability_generation: generation.clone(),
        });
        let denied = observer.call(create.clone(), &observing).await.unwrap_err();
        let encoded = serde_json::to_string(&denied).unwrap();
        assert!(!encoded.contains(&c.binding.root));
        assert!(!encoded.contains("private-recovery"));
        assert!(controller.controller_token().is_ok());
        let receipt = controller.call(create.clone(), &auth).await.unwrap();
        assert_eq!(
            controller.call(create.clone(), &auth).await.unwrap(),
            receipt
        );
        let mut conflicting = create.clone();
        if let Call::BackupCreate(value) = &mut conflicting {
            value.capability = id("different-capability");
        }
        assert!(controller.call(conflicting, &auth).await.is_err());
        let published = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let value = job(observer
                    .call(read(&c, &operation), &observing)
                    .await
                    .unwrap());
                if value.phase == wire::Phase::Published
                    && value.source_pins == wire::SourcePins::Released
                {
                    break value;
                }
                assert!(
                    value.failure.is_none(),
                    "job {value:?}; manager {:?}",
                    h.backup_progress().unwrap()
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            published.local_publication,
            wire::LocalPublication::Published
        );
        assert_eq!(published.cloud_transfer, wire::CloudTransfer::Unknown);
        assert_eq!(
            published.restore_verification,
            wire::RestoreVerification::NotObserved
        );
        let encrypted = std::fs::read(vault.join(format!("{operation}.age"))).unwrap();
        assert!(encrypted.starts_with(b"age-encryption.org/v1"));
        assert!(!encrypted
            .windows(b"dirty fixture source".len())
            .any(|w| w == b"dirty fixture source"));
        assert_eq!(
            controller
                .call(
                    Call::CommandRead(methods::CommandRead {
                        scope: scope(&c),
                        command_id: id(operation.as_str())
                    }),
                    &auth
                )
                .await
                .unwrap(),
            receipt
        );
        let unknown = CommandId::new();
        assert!(observer.call(read(&c, &unknown), &observing).await.is_err());
        let mut foreign = observing.clone();
        foreign.actor = ActorId::new();
        assert!(observer.call(read(&c, &operation), &foreign).await.is_err());
        let retry_command = CommandId::new();
        let retry = Call::BackupRetry(wire::Retry {
            scope: scope(&c),
            mutation: mutation(&h, &c, &retry_command),
            expected_binding_revision: workspace(&h, &c).binding.revision.get().into(),
            operation: id(operation.as_str()),
            expected_operation_revision: published.revision.clone(),
            expected_job_revision: published.job_revision.clone().unwrap(),
            capability: reference,
            expected_capability_generation: generation,
        });
        assert!(observer.call(retry.clone(), &observing).await.is_err());
        // Released completed jobs are not newly published merely by inspection or retry.
        let retried = controller.call(retry.clone(), &auth).await.unwrap();
        assert_eq!(controller.call(retry, &auth).await.unwrap(), retried);
        tokio::time::timeout(Duration::from_secs(10), async {
            while h.backup_progress().unwrap().is_some_and(|v| v.running()) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let complete = job(controller.call(read(&c, &operation), &auth).await.unwrap());
        let cancel_command = CommandId::new();
        let cancel = Call::BackupCancel(wire::Cancel {
            scope: scope(&c),
            mutation: mutation(&h, &c, &cancel_command),
            expected_binding_revision: workspace(&h, &c).binding.revision.get().into(),
            operation: id(operation.as_str()),
            expected_operation_revision: complete.revision,
            expected_job_revision: complete.job_revision,
        });
        assert!(observer.call(cancel.clone(), &observing).await.is_err());
        let cancelled = controller.call(cancel.clone(), &auth).await.unwrap();
        assert_eq!(controller.call(cancel, &auth).await.unwrap(), cancelled);
        let retained = job(observer
            .call(read(&c, &operation), &observing)
            .await
            .unwrap());
        assert!(retained.cancel_requested);
        assert_eq!(retained.phase, wire::Phase::Published);
        assert_eq!(
            retained.local_publication,
            wire::LocalPublication::Published
        );
        assert_eq!(
            std::fs::read(vault.join(format!("{operation}.age"))).unwrap(),
            encrypted
        );
        assert!(!h
            .snapshot()
            .unwrap()
            .records
            .values()
            .any(|r| r.collection == Collection::Attempt));
        observer.disconnect().unwrap().wait().await.unwrap();
        assert!(controller.controller_token().is_ok());
        controller.disconnect().unwrap().wait().await.unwrap();
        drop(owner);
        drop(h);
    }
}

#[tokio::test]
async fn public_backup_loss_and_cancel_before_first_poll_capture_no_sources() {
    // A current-thread runtime cannot poll the spawned publisher until this test
    // yields. Public RPC calls below finish synchronously on the canonical worker.
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for cancel in [false, true] {
            let (temp, c, h, owner) = fixture(backend);
            let auth = access(&h, &c, true);
            let observing = access(&h, &c, false);
            let mut controller = h.public_connection(auth.clone()).unwrap();
            controller.acquire(CommandId::new(), None).unwrap();
            let mut observer = h.public_connection(observing.clone()).unwrap();
            let vault = load(&h, &c, temp.path());
            let vault_names = || {
                std::fs::read_dir(&vault)
                    .unwrap()
                    .map(|entry| entry.unwrap().file_name())
                    .collect::<BTreeSet<_>>()
            };
            let vault_before = vault_names();
            let ready = status(
                controller
                    .call(
                        Call::BackupStatus(wire::StatusRequest { scope: scope(&c) }),
                        &auth,
                    )
                    .await
                    .unwrap(),
            );
            let wire::Capability::Loaded {
                reference,
                generation,
                ..
            } = ready.capability
            else {
                panic!("loaded capability")
            };
            let baseline = h.snapshot().unwrap();
            let operation = CommandId::new();
            let create = Call::BackupCreate(wire::Create {
                scope: scope(&c),
                mutation: mutation(&h, &c, &operation),
                expected_binding_revision: workspace(&h, &c).binding.revision.get().into(),
                capability: reference,
                expected_capability_generation: generation,
            });
            let receipt = controller.call(create, &auth).await.unwrap();
            if cancel {
                let stop = Call::BackupCancel(wire::Cancel {
                    scope: scope(&c),
                    mutation: mutation(&h, &c, &CommandId::new()),
                    expected_binding_revision: workspace(&h, &c).binding.revision.get().into(),
                    operation: id(operation.as_str()),
                    expected_operation_revision: 0.into(),
                    expected_job_revision: None,
                });
                let accepted = controller.call(stop.clone(), &auth).await.unwrap();
                assert_eq!(controller.call(stop, &auth).await.unwrap(), accepted);
            } else {
                controller.loss_signal().invalidate();
            }
            // No publisher task was polled between acceptance and invalidation.
            assert!(!h
                .snapshot()
                .unwrap()
                .records
                .values()
                .any(|r| r.collection == Collection::SnapshotPin));
            tokio::time::timeout(Duration::from_secs(10), async {
                while h.backup_progress().unwrap().is_some_and(|p| p.running()) {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            let after = h.snapshot().unwrap();
            let non_intents = |state: &vcp_store::contract::State| {
                state
                    .records
                    .iter()
                    .filter(|(_, r)| r.collection != Collection::Projection)
                    .map(|(k, r)| (k.clone(), r.clone()))
                    .collect::<std::collections::BTreeMap<_, _>>()
            };
            assert_eq!(non_intents(&after), non_intents(&baseline));
            assert_eq!(vault_names(), vault_before);
            let retained = job(observer
                .call(read(&c, &operation), &observing)
                .await
                .unwrap());
            assert_eq!(retained.cancel_requested, cancel);
            assert_eq!(
                retained.phase,
                if cancel {
                    wire::Phase::Cancelled
                } else {
                    wire::Phase::Interrupted
                }
            );
            assert_eq!(retained.job_revision, None);
            assert_eq!(retained.source_pins, wire::SourcePins::NotObserved);
            assert_eq!(
                retained.local_publication,
                wire::LocalPublication::NotObserved
            );
            assert_eq!(
                observer
                    .call(
                        Call::CommandRead(methods::CommandRead {
                            scope: scope(&c),
                            command_id: id(operation.as_str()),
                        }),
                        &observing
                    )
                    .await
                    .unwrap(),
                receipt
            );
            observer.disconnect().unwrap().wait().await.unwrap();
            if cancel {
                assert!(controller.controller_token().is_ok());
            }
            controller.disconnect().unwrap().wait().await.unwrap();
            drop(owner);
            drop(h);
        }
    }
}
