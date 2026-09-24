// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::foundation::{Config, ThreadBinding};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Duration,
};
use vcp_domain::{
    accounting::*,
    ids::*,
    policy::{Autonomy, Policy},
    revision::*,
    task::*,
    verification::Fingerprint,
    workspace::*,
};
use vcp_protocol::command::Command;
use vcp_store::{contract::Collection, BackendKind};
fn config(root: &std::path::Path, workspace: &std::path::Path, backend: BackendKind) -> Config {
    let currency: Currency = "USD".to_owned().try_into().unwrap();
    Config {
        canonical_root: root.into(),
        backend,
        workspace: WorkspaceId::parse("canonical-workspace").unwrap(),
        session: SessionId::parse("canonical-session").unwrap(),
        binding: Binding {
            host: HostId::parse("native-fixture").unwrap(),
            root: workspace.to_string_lossy().into_owned(),
            repository: "synthetic".into(),
            worktree: "main".into(),
            revision: Revision::ZERO,
        },
        actor: ActorId::parse("fixture-owner").unwrap(),
        root_task: TaskId::parse("root-task").unwrap(),
        cap: Money {
            currency: currency.clone(),
            micros: Micros::new(1000),
        },
        protected: Micros::ZERO,
        price: PriceSnapshot {
            id: "a".repeat(64),
            provider: "scripted-loopback".into(),
            model: "gpt-5.1".into(),
            currency,
            capability: "b".repeat(64),
            valid_until: Timestamp::new(u64::MAX),
            rates: [
                ChargeCategory::Input,
                ChargeCategory::Output,
                ChargeCategory::CacheRead,
                ChargeCategory::CacheWrite,
                ChargeCategory::Request,
                ChargeCategory::ProviderTool,
            ]
            .into_iter()
            .map(|kind| {
                (
                    kind,
                    Rate {
                        micros: Micros::new(if kind == ChargeCategory::Request {
                            100
                        } else {
                            0
                        }),
                        per_units: Units::new(1),
                    },
                )
            })
            .collect(),
        },
        input_ceiling: Units::new(500_000),
        output_ceiling: Units::new(1024),
        artifact_limit: ByteCount::new(vcp_store::artifact::DEFAULT_ARTIFACT_LIMIT),
        max_transport_retries: 2,
        host_tool_denials: vec![],
    }
}
fn task(
    host: &CanonicalHost,
    config: &Config,
    id: TaskId,
    parent: Option<TaskId>,
) -> ThreadBinding {
    host.command(
        Command::CreateTask {
            root: config.root_task.clone(),
            parent,
            fork_origin: None,
            objective: Objective {
                text: "Observe retained request and response".into(),
                constraints: vec![],
                acceptance: vec!["independent transport agrees".into()],
                source: EventId::new(),
                steering: SteeringRevision::ZERO,
            },
            fingerprint: Fingerprint {
                repository: "a".repeat(64),
                buffers: "b".repeat(64),
                environment: "c".repeat(64),
            },
            editing: false,
            required_checks: vec![],
        },
        Some(id.clone()),
        Revision::ZERO,
    )
    .unwrap();
    host.command(
        Command::Transition {
            next: TaskState::Running,
            reason: "explicit fixture start".into(),
            verification: None,
        },
        Some(id.clone()),
        Revision::ZERO,
    )
    .unwrap();
    ThreadBinding {
        scope: Scope {
            workspace: config.workspace.clone(),
            session: config.session.clone(),
            task: id,
        },
        agent: AgentId::new(),
        role: RequestRole::Main,
    }
}
pub(super) fn load(
    h: &CanonicalHost,
    c: &Config,
    temporary: &Path,
) -> (Arc<Capabilities>, Arc<vcp_repository::git::Git>, PathBuf) {
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
    let setup = crate::foundation::backup::Setup {
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
        crate::foundation::backup_run::Capabilities::open(
            trust,
            keys,
            &setup,
            root,
            &c.canonical_root,
        )
        .unwrap(),
    );
    h.load_backup_with_revision(capabilities.clone(), git.clone(), false, setup.revision)
        .unwrap();
    (capabilities, git, vault)
}

pub(super) fn fixture(
    backend: BackendKind,
) -> (
    tempfile::TempDir,
    Config,
    CanonicalHost,
    crate::foundation::CanonicalOwner,
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

#[test]
fn public_backup_cancellation_latches_across_clones() {
    let signal = Arc::new(AtomicBool::new(false));
    let current = signal.clone();
    let fence = PublicFence::new(|_| Ok(()), move || current.load(Ordering::Acquire));
    let detached = fence.clone();
    assert!(!detached.cancelled());
    signal.store(true, Ordering::Release);
    assert!(fence.cancelled());
    signal.store(false, Ordering::Release);
    assert!(
        detached.cancelled(),
        "a lost connection cannot revive admitted work"
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn public_backup_fences_capture_and_every_fresh_job_stage() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let (temporary, config, host, owner) = fixture(backend);
        let (caps, git, vault) = load(&host, &config, temporary.path());
        let loaded = host.backup_manager_snapshot().unwrap();
        let stale = loaded.capability.unwrap();
        assert_eq!(stale.configuration_revision, 7);
        assert!(!loaded.busy);
        host.load_backup_with_revision(caps.clone(), git.clone(), false, 7)
            .unwrap();
        let fresh = host.backup_manager_snapshot().unwrap().capability.unwrap();
        assert_ne!(stale.reference, fresh.reference);
        assert!(host
            .start_backup_fenced(
                CommandId::new(),
                false,
                &stale,
                PublicFence::new(|_| Ok(()), || false)
            )
            .is_err());
        assert!(!host.backup_manager_snapshot().unwrap().busy);
        let original = host.snapshot().unwrap().watermark;
        let deny = PublicFence::new(|_| Err("private authority reason".into()), || false);
        assert!(host
            .capture_backup_inputs_fenced(
                git.clone(),
                Arc::new(AtomicBool::new(false)),
                Some(deny.clone())
            )
            .await
            .is_err());
        assert!(deny.cancelled());
        assert_eq!(host.snapshot().unwrap().watermark, original);
        let inputs = host
            .capture_backup_inputs(git, Arc::new(AtomicBool::new(false)))
            .await
            .unwrap();
        for (target, during_copy) in [
            (Stage::Captured, false),
            (Stage::ArchiveReady, false),
            (Stage::CiphertextReady, false),
            (Stage::Admitted, false),
            (Stage::Admitted, true),
        ] {
            let operation = CommandId::new();
            let check_operation = operation.clone();
            let expected = target.clone();
            let fence = PublicFence::new(
                move |context| {
                    let store = context.engine.store();
                    if store.state().records.values().any(|row| {
                        row.collection == Collection::SnapshotPin
                            && row.id == check_operation.as_str()
                    }) {
                        let job = Jobs::inspect(store, &check_operation, &context.config.workspace)
                            .map_err(|error| error.to_string())?;
                        if job.stage == expected && (!during_copy || job.copy_identity().is_some())
                        {
                            return Err("stage authority revoked".into());
                        }
                    }
                    Ok(())
                },
                || false,
            );
            assert!(host
                .publish_backup_fenced(
                    caps.clone(),
                    operation.clone(),
                    Some(inputs.clone()),
                    Arc::new(AtomicBool::new(false)),
                    Some(fence.clone())
                )
                .await
                .is_err());
            assert!(fence.cancelled());
            let inspect_id = operation.clone();
            let actual = host
                .worker
                .run(move |context| {
                    Ok(Jobs::inspect(
                        context.engine.store(),
                        &inspect_id,
                        &context.config.workspace,
                    )?)
                })
                .unwrap();
            assert_eq!(actual.stage, target);
            assert_eq!(actual.copy_identity().is_some(), during_copy);
            let cleanup = host.release_cancelled_backup(caps.clone(), operation.clone());
            if target == Stage::Admitted {
                assert!(
                    cleanup.is_err(),
                    "admitted copy keeps an honest reconciliation obligation"
                );
            } else {
                cleanup.unwrap();
            }
            let cleaned = host
                .worker
                .run(move |context| {
                    Ok(Jobs::inspect(
                        context.engine.store(),
                        &operation,
                        &context.config.workspace,
                    )?)
                })
                .unwrap();
            if target == Stage::Admitted {
                assert!(cleaned.active);
                assert_eq!(cleaned.stage, Stage::Admitted);
            } else {
                assert!(!cleaned.active);
                assert_eq!(cleaned.stage, Stage::Cancelled);
            }
        }
        assert_eq!(
            std::fs::read_dir(&vault).unwrap().count(),
            0,
            "cancelled publication must not leave a finalized vault object"
        );
        owner.close().await.unwrap();
    }
}
