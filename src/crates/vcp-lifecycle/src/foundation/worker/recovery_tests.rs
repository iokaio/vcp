// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::collections::BTreeSet;
use vcp_domain::policy::*;
use vcp_store::BackendKind;

#[cfg(windows)]
#[test]
fn retained_artifact_read_never_resurrects_redacted_or_purged_bytes() {
    use crate::foundation::coding::CodingConfig;
    use vcp_domain::artifact::CaptureState;
    use vcp_models::catalog::{Compatibility, Snapshot};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for redacted in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let (mut context, binding) = setup(&temp, backend);
            std::fs::write(temp.path().join("workspace/anchor.txt"), "fixture anchor").unwrap();
            let raw = serde_json::to_vec(&json!({"data":{"id":"fixture/model","endpoints":[{"tag":"fixture/region","status":0,"context_length":200000,"max_prompt_tokens":180000,"max_completion_tokens":8000,"supported_parameters":["tools","max_tokens"],"pricing":{"prompt":"0","completion":"0","request":"0"}}]}})).unwrap();
            let snapshot = Snapshot::from_endpoints(
                &raw,
                Timestamp::ZERO,
                Timestamp::new(u64::MAX),
                Compatibility {
                    id: "artifact-access-fixture/1".into(),
                    model: "fixture/model".into(),
                    endpoint: "fixture/region".into(),
                    qualified_at: Timestamp::ZERO,
                    valid_until: Timestamp::new(u64::MAX),
                    responses_text_tools: true,
                    byte_ceiling_qualified: true,
                    provider_preferences_qualified: true,
                    deny_data_collection: true,
                    require_zdr: true,
                    request_price_limit: "0".into(),
                    required_parameters: BTreeSet::from(["tools".into(), "max_tokens".into()]),
                    qualified_reasoning_efforts: BTreeSet::new(),
                },
            )
            .unwrap();
            context
                .configure_provider(snapshot, raw, Duration::from_secs(30))
                .unwrap();
            context
                .configure_coding(
                    &binding,
                    CodingConfig {
                        canonical_tools: Default::default(),
                        operating: "Fixture artifact reads only".into(),
                        affected_paths: vec!["anchor.txt".into()],
                        max_requests: 4,
                        deadline: Timestamp::new(now().get() + 60_000).into(),
                    },
                )
                .unwrap();
            let artifact = context
                .capture(
                    &binding.scope,
                    Channel::Stdout,
                    b"retained private fixture bytes",
                    "fixture-artifact/1",
                )
                .unwrap();
            let arguments = json!({"artifact":artifact.spec.id,"offset":0,"length":64}).to_string();
            let (before, source) = context.read_coding_artifact(&binding, &arguments).unwrap();
            assert_eq!(before["content"], "retained private fixture bytes");
            assert_eq!(source, Some(artifact.spec.id.clone()));
            let prior: Task = context
                .engine
                .store()
                .current()
                .record(
                    Collection::Task,
                    binding.scope.task.as_str(),
                    &binding.scope.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            context
                .command(
                    Command::Transition {
                        next: TaskState::Cancelled,
                        reason: "fixture owner stops before explicit retention".into(),
                        verification: None,
                    },
                    Some(binding.scope.task.clone()),
                    prior.revision,
                )
                .unwrap();
            let mut workspace: Workspace = context
                .engine
                .store()
                .current()
                .record(
                    Collection::Workspace,
                    binding.scope.workspace.as_str(),
                    &binding.scope.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            let prior_revision = workspace.revision;
            workspace.revision = workspace.revision.next().unwrap();
            workspace.deletion = workspace.deletion.next().unwrap();
            let transaction = Transaction {
                id: TransactionId::new(),
                expected_watermark: context.engine.store().current().watermark,
                events: vec![],
                command: None,
                mutations: vec![Mutation::Put {
                    expected: Some(prior_revision),
                    record: Record::typed(
                        Collection::Workspace,
                        workspace.id.as_str(),
                        workspace.id.clone(),
                        workspace.revision,
                        &workspace,
                    )
                    .unwrap(),
                }],
            };
            context
                .runtime
                .block_on(context.engine.store_mut().transact(transaction))
                .unwrap();
            let mut current = context
                .runtime
                .block_on(context.engine.store().archive_state())
                .unwrap();
            if redacted {
                let record = current
                    .records
                    .get_mut(&vcp_store::contract::key(
                        Collection::Task,
                        binding.scope.task.as_str(),
                    ))
                    .unwrap();
                let mut task: Task = record.decode().unwrap();
                task.state = TaskState::Cancelled;
                record.value = serde_json::to_value(
                    vcp_protocol::redaction::task(&task, DeletionEpoch::new(1)).unwrap(),
                )
                .unwrap();
            } else {
                // Keep the old physical spool bytes: current canonical purge
                // state must deny reads even before physical cleanup finishes.
                let record = current
                    .records
                    .get_mut(&vcp_store::contract::key(
                        Collection::Artifact,
                        artifact.spec.id.as_str(),
                    ))
                    .unwrap();
                let mut descriptor: ArtifactDescriptor = record.decode().unwrap();
                descriptor.state = CaptureState::Purged;
                descriptor.retained.clear();
                record.value = serde_json::to_value(descriptor).unwrap();
            }
            let requests_before = current
                .records
                .values()
                .filter(|record| record.collection == Collection::Attempt)
                .count();
            context
                .runtime
                .block_on(context.engine.store_mut().rewrite_base(current, &[]))
                .unwrap();
            match context.read_coding_artifact(&binding, &arguments) {
                Ok((result, source)) => {
                    assert_eq!(result["availability"], "unavailable");
                    assert!(result["content"].is_null());
                    assert!(source.is_none());
                }
                Err(_) => {} // Terminal authority denies even tool admission.
            }
            if !redacted {
                let mut bytes = Vec::new();
                assert!(context
                    .runtime
                    .block_on(vcp_audit::history::History::read_artifact(
                        context.engine.store(),
                        &context.history_access(),
                        &artifact.spec.id,
                        &mut bytes
                    ))
                    .is_err());
                assert!(
                    bytes.is_empty(),
                    "the retained reader must not recover purged payload bytes"
                );
            }
            assert_eq!(
                context
                    .engine
                    .store()
                    .current()
                    .records
                    .values()
                    .filter(|record| record.collection == Collection::Attempt)
                    .count(),
                requests_before
            );
            context.close().unwrap();
        }
    }
}

#[test]
fn startup_preserves_due_saved_retention_and_evidence_without_resuming() {
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
        let saved_policy = retention_policy::show(context.engine.store(), &access).unwrap();
        let before: Workspace = context
            .engine
            .store()
            .current()
            .record(
                Collection::Workspace,
                access.workspace.as_str(),
                &access.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        context.close().unwrap();
        let reopened = Context::open(config.clone()).unwrap();
        assert!(
            retention_policy::latest_run(reopened.engine.store(), &reopened.memory_access())
                .unwrap()
                .is_none()
        );
        assert_eq!(
            retention_policy::show(reopened.engine.store(), &reopened.memory_access()).unwrap(),
            saved_policy
        );
        let task: Task = reopened
            .engine
            .store()
            .current()
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
            .current()
            .records
            .values()
            .any(|r| r.collection == Collection::Attempt));
        let workspace: Workspace = reopened
            .engine
            .store()
            .current()
            .record(
                Collection::Workspace,
                access.workspace.as_str(),
                &access.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(workspace.deletion, before.deletion);
        reopened.close().unwrap();
        let again = Context::open(config).unwrap();
        let current: Workspace = again
            .engine
            .store()
            .current()
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
        }
        .into(),
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

#[test]
fn canonical_worker_waits_for_slow_admitted_outcome_on_both_stores() {
    std::thread::scope(|threads| {
        for backend in [BackendKind::Files, BackendKind::Sqlite] {
            threads.spawn(move || {
                let temp = tempfile::tempdir().unwrap();
                let (context, _) = setup(&temp, backend);
                let config = context.config.clone();
                context.close().unwrap();
                let worker = Worker::open(config, None).unwrap();
                let before = worker
                    .run(|context| Ok(context.engine.store().current().watermark))
                    .unwrap();
                let result = worker
                    .run(|context| {
                        // Cross the former production wait, not a reduced test timer.
                        std::thread::sleep(Duration::from_millis(30_100));
                        Ok(context.engine.store().current().watermark)
                    })
                    .unwrap();
                assert_eq!(result, before);
                assert!(!worker.fenced());
                assert_eq!(worker.run(|_| Ok(17)).unwrap(), 17);
                drop(worker); // joins and closes SQLite before TempDir cleanup
            });
        }
    });
}

#[test]
fn canonical_worker_explicit_fence_keeps_admitted_result_and_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let (context, _) = setup(&temp, BackendKind::Files);
    let config = context.config.clone();
    context.close().unwrap();
    let worker = Worker::open(config, None).unwrap();
    let (arrived, arrival) = std::sync::mpsc::sync_channel(1);
    let (release, released) = std::sync::mpsc::sync_channel(1);
    let pending = worker.clone();
    let caller = std::thread::spawn(move || {
        pending.run(move |_| {
            arrived.send(()).unwrap();
            released.recv().unwrap();
            Ok(23)
        })
    });
    arrival.recv_timeout(Duration::from_secs(5)).unwrap();
    worker.fence();
    assert!(worker.run(|_| Ok(())).unwrap_err().contains("fenced"));
    release.send(()).unwrap();
    assert_eq!(caller.join().unwrap().unwrap(), 23);
    assert!(worker.fenced());
    assert_eq!(worker.run_cleanup(|_| Ok(29)).unwrap(), 29);
}

#[test]
fn canonical_worker_disconnect_and_self_reentry_remain_fenced() {
    let temp = tempfile::tempdir().unwrap();
    let (context, _) = setup(&temp, BackendKind::Files);
    let config = context.config.clone();
    context.close().unwrap();
    let worker = Worker::open(config, None).unwrap();
    let nested = worker.clone();
    let error = worker
        .run(move |_| Ok(nested.run(|_| Ok(())).unwrap_err()))
        .unwrap();
    assert!(error.contains("cannot synchronously reenter"));
    assert!(worker.fenced());
    drop(worker);

    // Accept the queued job, then lose it before an outcome can be delivered.
    let (sender, receiver) = std::sync::mpsc::sync_channel::<Job>(32);
    let thread = std::thread::spawn(move || {
        drop(receiver.recv().unwrap());
        Ok(())
    });
    let worker = Worker(Arc::new(Inner {
        thread_id: thread.thread().id(),
        sender: Mutex::new(Some(sender)),
        thread: Mutex::new(Some(thread)),
        fenced: AtomicBool::new(false),
    }));
    let error = worker.run(|_| Ok(())).unwrap_err();
    assert!(error.contains("worker disconnected"));
    assert!(error.contains("outcome unknown; reopen required"));
    assert!(worker.fenced());
}
fn effect(context: &Context, id: &ToolRunId) -> Effect {
    context
        .engine
        .store()
        .current()
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
fn recovery_retains_directory_only_partial_effect_without_replaying_file() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let (mut context, binding) = setup(&temp, backend);
        let id = pending(
            &mut context,
            &binding,
            "*** Begin Patch\n*** Add File: Data/Models/one.cs\n+one\n*** End Patch",
        );
        std::fs::create_dir(temp.path().join("workspace/Data")).unwrap();
        let config = context.config.clone();
        context.close().unwrap();
        let reopened = Context::open(config).unwrap();
        assert_eq!(effect(&reopened, &id).state, EffectState::Failed);
        assert!(temp.path().join("workspace/Data").is_dir());
        assert!(!temp.path().join("workspace/Data/Models").exists());
        let report: ArtifactDescriptor = reopened
            .engine
            .store()
            .current()
            .records
            .values()
            .filter(|row| row.collection == Collection::Artifact)
            .filter_map(|row| row.decode::<ArtifactDescriptor>().ok())
            .find(|a| a.spec.schema == "vcp-effect-reconciliation-v1")
            .unwrap();
        let report = reopened.recovery_artifact(&report).unwrap();
        let observations = report["observations"].as_array().unwrap();
        assert!(observations.iter().any(|o| o["class"] == "directory"
            && o["path"] == "Data"
            && o["certainty"] == "present"));
        assert!(observations.iter().any(|o| o["class"] == "directory"
            && o["path"] == "Data/Models"
            && o["certainty"] == "absent"));
    }
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
                .current()
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
