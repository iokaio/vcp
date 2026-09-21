// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::collections::BTreeSet;
use vcp_domain::policy::*;
use vcp_store::BackendKind;

#[test]
fn startup_applies_due_saved_retention_without_resuming_or_model_work() {
    use vcp_domain::retention_selector::{Criterion, Selector, Tree};
    use vcp_memory::{
        retention::Action,
        retention_policy::{self, Automatic},
    };
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut context, binding) = setup(&temp, backend);
        let config = context.config.clone();
        let access = context.memory_access();
        context
            .runtime
            .block_on(retention_policy::set(
                context.engine.store_mut(),
                &access,
                None,
                7,
                Some(Automatic {
                    selector: Selector {
                        schema_version: 1,
                        tree: Tree::Match(Criterion::Workspace(access.workspace.clone())),
                    },
                    action: Action::Exclude,
                    cadence_days: 7,
                }),
                Timestamp::ZERO,
            ))
            .unwrap();
        assert!(
            retention_policy::latest_run(context.engine.store(), &access)
                .unwrap()
                .is_none()
        );
        context.close().unwrap();
        let reopened = Context::open(config.clone()).unwrap();
        let run = retention_policy::latest_run(reopened.engine.store(), &reopened.memory_access())
            .unwrap()
            .unwrap();
        assert_eq!(run.status, "completed");
        let task: Task = reopened
            .engine
            .store()
            .state()
            .record(
                Collection::Task,
                binding.scope.task.as_str(),
                &access.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(task.state, TaskState::Paused);
        assert!(!reopened
            .engine
            .store()
            .state()
            .records
            .values()
            .any(|r| r.collection == Collection::Attempt));
        let workspace: Workspace = reopened
            .engine
            .store()
            .state()
            .record(
                Collection::Workspace,
                access.workspace.as_str(),
                &access.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        reopened.close().unwrap();
        let again = Context::open(config).unwrap();
        let current: Workspace = again
            .engine
            .store()
            .state()
            .record(
                Collection::Workspace,
                access.workspace.as_str(),
                &access.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(current.deletion, workspace.deletion);
        again.close().unwrap();
    }
}

fn setup(temp: &tempfile::TempDir, backend: BackendKind) -> (Context, ThreadBinding) {
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let currency: Currency = "USD".to_owned().try_into().unwrap();
    let config = Config {
        canonical_root: temp.path().join("store"),
        backend,
        workspace: WorkspaceId::parse("workspace").unwrap(),
        session: SessionId::parse("session").unwrap(),
        binding: Binding {
            host: HostId::parse("host").unwrap(),
            root: workspace.to_string_lossy().into_owned(),
            repository: "fixture".into(),
            worktree: "main".into(),
            revision: Revision::ZERO,
        },
        actor: ActorId::parse("owner").unwrap(),
        root_task: TaskId::parse("task").unwrap(),
        cap: Money {
            currency: currency.clone(),
            micros: Micros::new(1000),
        },
        protected: Micros::ZERO,
        price: PriceSnapshot {
            id: "a".repeat(64),
            provider: "fixture".into(),
            model: "fixture".into(),
            currency,
            capability: "b".repeat(64),
            valid_until: Timestamp::new(u64::MAX),
            rates: Default::default(),
        },
        input_ceiling: Units::new(100),
        output_ceiling: Units::new(100),
        artifact_limit: ByteCount::new(16 * 1024 * 1024),
        max_transport_retries: 2,
        host_tool_denials: vec![],
    };
    let mut context = Context::open(config).unwrap();
    let binding = ThreadBinding {
        scope: Scope {
            workspace: context.config.workspace.clone(),
            session: context.config.session.clone(),
            task: context.config.root_task.clone(),
        },
        agent: AgentId::new(),
        role: RequestRole::Main,
    };
    context
        .command(
            Command::CreateTask {
                root: binding.scope.task.clone(),
                parent: None,
                fork_origin: None,
                objective: Objective {
                    text: "recover fixture".into(),
                    constraints: vec![],
                    acceptance: vec!["no replay".into()],
                    source: EventId::new(),
                    steering: SteeringRevision::ZERO,
                },
                fingerprint: vcp_domain::verification::Fingerprint {
                    repository: "a".repeat(64),
                    buffers: "b".repeat(64),
                    environment: "c".repeat(64),
                },
                editing: false,
                required_checks: vec![],
            },
            Some(binding.scope.task.clone()),
            Revision::ZERO,
        )
        .unwrap();
    context
        .command(
            Command::Transition {
                next: TaskState::Running,
                reason: "fixture".into(),
                verification: None,
            },
            Some(binding.scope.task.clone()),
            Revision::ZERO,
        )
        .unwrap();
    context
        .command(
            Command::SetWorkspaceTrust {
                trust: Trust::Trusted,
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
    let policy = Policy {
        workspace: binding.scope.workspace.clone(),
        revision: PolicyRevision::ZERO,
        mode: Autonomy::Workspace,
        denials: vec![],
        workspace_roots: BTreeSet::from([RootId::parse("workspace").unwrap()]),
        automatic_effects: BTreeSet::new(),
        timeout_ceiling_ms: Units::new(30_000),
        output_ceiling_bytes: ByteCount::new(1024 * 1024),
    };
    context
        .command(Command::SetPolicy { policy }, None, Revision::ZERO)
        .unwrap();
    (context, binding)
}
fn effect(context: &Context, id: &ToolRunId) -> Effect {
    context
        .engine
        .store()
        .state()
        .record(Collection::Effect, id.as_str(), &context.config.workspace)
        .unwrap()
        .decode()
        .unwrap()
}
fn pending(context: &mut Context, binding: &ThreadBinding, patch: &str) -> ToolRunId {
    let prepared = vcp_tools::prepare(
        context.tool_root().unwrap(),
        context.tool_identity(binding, "vcp_patch").unwrap(),
        vcp_tools::Request::Patch {
            patch: patch.into(),
        },
        ByteCount::new(1024 * 1024),
    )
    .unwrap();
    let (id, plan, _, _) = context.tool_propose(binding, &prepared).unwrap();
    let execution = ExecutionId::new();
    for next in [
        EffectState::Authorized,
        EffectState::DispatchRecorded,
        EffectState::Running,
    ] {
        context
            .tool_advance(
                binding,
                &id,
                next,
                Some(execution.clone()),
                vec![plan.clone()],
                "fixture fault barrier",
            )
            .unwrap();
    }
    id
}

#[test]
fn recovery_reconciles_applied_unapplied_partial_and_conflicted_files_on_both_stores() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for (first, second, expected) in [
            (false, false, EffectState::Failed),
            (true, false, EffectState::Failed),
            (true, true, EffectState::Succeeded),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let (mut context, binding) = setup(&temp, backend);
            let id=pending(&mut context,&binding,"*** Begin Patch\n*** Add File: first.txt\n+one\n*** Add File: second.txt\n+two\n*** End Patch");
            if first {
                std::fs::write(temp.path().join("workspace/first.txt"), b"one\n").unwrap();
            }
            if second {
                std::fs::write(temp.path().join("workspace/second.txt"), b"two\n").unwrap();
            }
            let config = context.config.clone();
            context.close().unwrap();
            let mut reopened = Context::open(config).unwrap();
            assert_eq!(effect(&reopened, &id).state, expected);
            assert!(reopened.reconcile_effects().unwrap().is_empty());
            assert_eq!(temp.path().join("workspace/first.txt").exists(), first);
            assert_eq!(temp.path().join("workspace/second.txt").exists(), second);
        }
        let temp = tempfile::tempdir().unwrap();
        let (mut context, binding) = setup(&temp, backend);
        let id = pending(
            &mut context,
            &binding,
            "*** Begin Patch\n*** Add File: first.txt\n+one\n*** End Patch",
        );
        std::fs::write(temp.path().join("workspace/first.txt"), b"human change\n").unwrap();
        let config = context.config.clone();
        context.close().unwrap();
        let mut reopened = Context::open(config).unwrap();
        assert_eq!(effect(&reopened, &id).state, EffectState::OutcomeUnknown);
        assert_eq!(
            std::fs::read(temp.path().join("workspace/first.txt")).unwrap(),
            b"human change\n"
        );
        std::fs::write(temp.path().join("workspace/first.txt"), b"one\n").unwrap();
        reopened.reconcile_effects().unwrap();
        assert_eq!(effect(&reopened, &id).state, EffectState::Succeeded);
    }
}

#[test]
fn recovery_accepts_only_matching_process_execution_receipts_and_never_reuses_pid() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for matching in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let (mut context, binding) = setup(&temp, backend);
            let id = ToolRunId::new();
            context
                .command(
                    Command::ProposeEffect {
                        id: id.clone(),
                        operation_digest: "a".repeat(64),
                    },
                    Some(binding.scope.task.clone()),
                    Revision::new(1),
                )
                .unwrap();
            let execution = ExecutionId::new();
            for next in [
                EffectState::Validated,
                EffectState::Authorized,
                EffectState::DispatchRecorded,
            ] {
                context
                    .tool_advance(
                        &binding,
                        &id,
                        next,
                        Some(execution.clone()),
                        vec![],
                        "fault after dispatch",
                    )
                    .unwrap();
            }
            context
                .capture(
                    &binding.scope,
                    Channel::Evidence,
                    &canonical_bytes(
                        &json!({"execution":execution,"process_id":std::process::id(),"process_identity":{"created":1}}),
                    )
                    .unwrap(),
                    "vcp-process-start-v1",
                )
                .unwrap();
            context.capture(&binding.scope,Channel::Evidence,&canonical_bytes(&json!({"execution":if matching {execution} else {ExecutionId::new()},"effect":id,"exit_code":0,"owned_processes_remaining":0,"output_complete":true})).unwrap(),"vcp-process-outcome-v1").unwrap();
            let config = context.config.clone();
            context.close().unwrap();
            let reopened = Context::open(config).unwrap();
            assert_eq!(
                effect(&reopened, &id).state,
                if matching {
                    EffectState::Succeeded
                } else {
                    EffectState::OutcomeUnknown
                }
            );
            let report: ArtifactDescriptor = reopened
                .engine
                .store()
                .state()
                .records
                .values()
                .filter(|row| row.collection == Collection::Artifact)
                .filter_map(|row| row.decode::<ArtifactDescriptor>().ok())
                .find(|a| a.spec.schema == "vcp-effect-reconciliation-v1")
                .unwrap();
            let report = reopened.recovery_artifact(&report).unwrap();
            let process = report["observations"]
                .as_array()
                .unwrap()
                .iter()
                .find(|o| o["class"] == "process")
                .unwrap();
            assert_eq!(process["creation_identity_matches"], false);
            assert_eq!(process["current_process"]["pid"], std::process::id());
        }
    }
}

#[test]
fn recovery_checks_rename_delete_and_unresolved_staging_without_rollback() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let (mut context, binding) = setup(&temp, backend);
        let root = temp.path().join("workspace");
        std::fs::write(root.join("old.txt"), b"before\n").unwrap();
        std::fs::write(root.join("delete.txt"), b"delete\n").unwrap();
        let id=pending(&mut context,&binding,"*** Begin Patch\n*** Update File: old.txt\n*** Move to: renamed.txt\n@@\n-before\n+after\n*** Delete File: delete.txt\n*** End Patch");
        std::fs::rename(root.join("old.txt"), root.join("renamed.txt")).unwrap();
        std::fs::write(root.join("renamed.txt"), b"after\n").unwrap();
        std::fs::remove_file(root.join("delete.txt")).unwrap();
        std::fs::write(root.join(".vcp-stage-fixture"), b"after\n").unwrap();
        let execution = effect(&context, &id).execution;
        context
            .capture(
                &binding.scope,
                Channel::Evidence,
                &canonical_bytes(&json!({"effect":id,"execution":execution,
            "observation":{"complete":false,"staging_path":".vcp-stage-fixture"}}))
                .unwrap(),
                "vcp-file-outcome-v1",
            )
            .unwrap();
        let config = context.config.clone();
        context.close().unwrap();
        let mut reopened = Context::open(config).unwrap();
        assert_eq!(effect(&reopened, &id).state, EffectState::OutcomeUnknown);
        assert!(root.join(".vcp-stage-fixture").exists());
        assert!(!root.join("old.txt").exists());
        assert!(!root.join("delete.txt").exists());
        assert_eq!(std::fs::read(root.join("renamed.txt")).unwrap(), b"after\n");
        // An explicit cleanup of this fixture's residual is observed, not
        // inferred from a failed write or repaired by rolling back user bytes.
        std::fs::remove_file(root.join(".vcp-stage-fixture")).unwrap();
        reopened.reconcile_effects().unwrap();
        assert_eq!(effect(&reopened, &id).state, EffectState::Succeeded);
    }
}

#[test]
fn recovery_observes_actual_spelling_for_case_only_windows_rename() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let (mut context, binding) = setup(&temp, backend);
        let root = temp.path().join("workspace");
        std::fs::write(root.join("case.txt"), b"before\n").unwrap();
        let id=pending(&mut context,&binding,"*** Begin Patch\n*** Update File: case.txt\n*** Move to: CASE.txt\n@@\n-before\n+after\n*** End Patch");
        std::fs::rename(root.join("case.txt"), root.join("CASE.txt")).unwrap();
        std::fs::write(root.join("CASE.txt"), b"after\n").unwrap();
        let config = context.config.clone();
        context.close().unwrap();
        let reopened = Context::open(config).unwrap();
        assert_eq!(effect(&reopened, &id).state, EffectState::Succeeded);
    }
}

#[test]
fn public_recovery_waits_for_paused_scheduled_and_retained_producers() {
    let temp = tempfile::tempdir().unwrap();
    let (context, binding) = setup(&temp, BackendKind::Sqlite);
    let prepared = vcp_tools::prepare(
        context.tool_root().unwrap(),
        context.tool_identity(&binding, "vcp_patch").unwrap(),
        vcp_tools::Request::Patch {
            patch: "*** Begin Patch\n*** Add File: file.txt\n+after\n*** End Patch".into(),
        },
        ByteCount::new(1024 * 1024),
    )
    .unwrap();
    let config = context.config.clone();
    context.close().unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let (host, owner) = super::super::super::CanonicalHost::open(config.clone()).unwrap();
    let lease = host
        .scheduler
        .try_acquire(prepared.authority().operation())
        .unwrap();
    assert!(host.reconcile_effects().unwrap_err().contains("quiescent"));
    drop(lease);
    let thread = codex_protocol::ThreadId::new();
    host.runtime.0.state.lock().unwrap().entries.insert(
        thread,
        crate::Entry {
            thread: std::sync::Weak::new(),
            parent: None,
            held: true,
            interrupted: false,
            interruption_error: None,
            starts: 1,
            admission_generation: 0,
        },
    );
    assert!(host.reconcile_effects().unwrap_err().contains("quiescent"));
    host.runtime.0.state.lock().unwrap().entries.clear();
    assert!(host.reconcile_effects().unwrap().is_empty());
    runtime.block_on(owner.close()).unwrap();
    drop(host);
    // Public orderly shutdown joins the SQLite worker before releasing ownership;
    // the next owner must open immediately without a sleep or retry workaround.
    let (reopened, owner) = super::super::super::CanonicalHost::open(config).unwrap();
    assert!(reopened.reconcile_effects().unwrap().is_empty());
    runtime.block_on(owner.close()).unwrap();
    drop(reopened);
}
