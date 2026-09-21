// SPDX-License-Identifier: Apache-2.0
use super::*;
use codex_core::StartThreadOptions;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
use vcp_domain::{agents::*, policy::*};
use vcp_repository::{dirty_snapshot::CapturePolicy, worktree::Snapshotter, Root, RootIdentity};

fn current(host: &CanonicalHost, config: &Config, id: &TaskId) -> Task {
    host.snapshot()
        .unwrap()
        .record(Collection::Task, id.as_str(), &config.workspace)
        .unwrap()
        .decode()
        .unwrap()
}
async fn retained(
    host: &CanonicalHost,
    workspace: &Path,
    server: &wiremock::MockServer,
) -> TestCodex {
    let mut registry = ExtensionRegistryBuilder::new();
    registry.turn_start_admission(Arc::new(host.clone()));
    registry.work_admission(Arc::new(host.clone()));
    let starter = host.clone();
    let cwd = workspace.to_path_buf();
    test_codex()
        .with_extensions(Arc::new(registry.build()))
        .with_auth(codex_login::CodexAuth::from_api_key(
            "synthetic-recovery-fixture",
        ))
        .with_allowed_tools(AllowedTools(vec![]))
        .with_config(move |c| {
            c.cwd = cwd.try_into().unwrap();
            configure_fixture_provider(c);
            starter
                .lifecycle()
                .authorize_startup(c.cwd.as_path(), None)
                .unwrap();
        })
        .build_with_auto_env(server)
        .await
        .unwrap()
}
async fn spawn(test: &TestCodex, path: &Path) -> Arc<codex_core::CodexThread> {
    let mut config = test.config.clone();
    config.cwd = path.to_path_buf().try_into().unwrap();
    let mut extensions = codex_extension_api::ExtensionDataInit::default();
    extensions.insert(AllowedTools(vec![]));
    test.thread_manager
        .start_thread(StartThreadOptions {
            thread_extension_init: extensions,
            environments: Some(test.codex.environment_selections().await),
            ..StartThreadOptions::new(config)
        })
        .await
        .unwrap()
        .thread
}
fn configure_parent(host: &CanonicalHost, id: codex_protocol::ThreadId) {
    let (snapshot, raw) = provider_snapshot();
    host.configure_provider(snapshot, raw).unwrap();
    host.configure_verification(
        id,
        vcp_lifecycle::foundation::verification::VerificationConfig {
            requirements: vec![],
            rationale: "unchanged fixture analysis requires current evidence".into(),
        },
    )
    .unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    host.configure_coding(
        id,
        vcp_lifecycle::foundation::coding::CodingConfig {
            operating: "Observe current child scope only.".into(),
            affected_paths: vec!["file.txt".into()],
            max_requests: 8,
            deadline: Timestamp::new(now + 300_000),
        },
    )
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_owner_recovers_registered_child_edits_and_keeps_sibling_paused() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let copies = temp.path().join("children");
        fs::create_dir(&workspace).unwrap();
        fs::create_dir(&copies).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        fs::write(workspace.join("file.txt"), b"base\n").unwrap();
        fs::write(workspace.join("z.txt"), b"last file\n").unwrap();
        let config = config(&temp.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let root_binding = task(&host, &config, config.root_task.clone(), None);
        host.command(
            Command::SetWorkspaceTrust {
                trust: Trust::Trusted,
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        let source_root = RootId::parse(config.workspace.as_str()).unwrap();
        host.command(
            Command::SetPolicy {
                policy: Policy {
                    workspace: config.workspace.clone(),
                    revision: PolicyRevision::ZERO,
                    mode: Autonomy::Workspace,
                    denials: vec![],
                    workspace_roots: BTreeSet::from([source_root.clone()]),
                    automatic_effects: BTreeSet::new(),
                    timeout_ceiling_ms: Units::new(30000),
                    output_ceiling_bytes: ByteCount::new(1024 * 1024),
                },
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        host.initialize_root_budget().unwrap();
        let server = start_mock_server().await;
        let test = retained(&host, &workspace, &server).await;
        let root_thread = host.lifecycle().attach_root(test.codex.clone()).unwrap();
        host.register(root_thread, root_binding.clone()).unwrap();
        configure_parent(&host, root_thread);
        let disposable = Root::open(
            RootIdentity {
                workspace: config.workspace.clone(),
                root: RootId::new(),
                repository: config.binding.repository.clone(),
                worktree: "child-copies".into(),
                binding: config.binding.revision,
            },
            &copies,
        )
        .unwrap();
        let snapshotter = Snapshotter::new(
            std::env::var_os("VCP_TEST_GIT")
                .expect("native Git dependency")
                .into(),
            ["SystemRoot", "WINDIR", "PATH", "TEMP", "TMP"]
                .into_iter()
                .filter_map(|key| std::env::var_os(key).map(|value| (key.into(), value)))
                .collect(),
            Duration::from_secs(20),
            8 * 1024 * 1024,
        )
        .unwrap();
        let children = [TaskId::new(), TaskId::new(), TaskId::new()];
        let cancelled = TaskId::new();
        for child in children.iter().chain(std::iter::once(&cancelled)) {
            let inputs = host
                .capture_child_workspace(
                    root_thread,
                    child.clone(),
                    &snapshotter,
                    &CapturePolicy {
                        untracked: BTreeSet::from(["file.txt".into(), "z.txt".into()]),
                        required: BTreeSet::from(["file.txt".into(), "z.txt".into()]),
                        ..Default::default()
                    },
                    &disposable,
                )
                .await
                .unwrap();
            let state = host.snapshot().unwrap();
            let workspace_state: Workspace = state
                .record(
                    Collection::Workspace,
                    config.workspace.as_str(),
                    &config.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            let parent = current(&host, &config, &config.root_task);
            let ledger: Ledger = state
                .record(
                    Collection::Ledger,
                    config.root_task.as_str(),
                    &config.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            let graph =
                vcp_engine::agents::graph(&state, &root_binding.scope, &config.root_task).unwrap();
            host.command(
                Command::CreateChild {
                    id: child.clone(),
                    objective: parent.objectives.last().unwrap().clone(),
                    fingerprint: parent.fingerprint.clone(),
                    required_checks: vec![],
                    spec: ChildSpec {
                        parent: config.root_task.clone(),
                        actor: config.actor.clone(),
                        parent_steering: parent.steering,
                        dependencies: BTreeSet::new(),
                        mode: ChildMode::ReadOnly,
                        role: "reviewer".into(),
                        model_policy: config.price.model.clone(),
                        paths: vec![ChildPath {
                            root: source_root.clone(),
                            path: String::new(),
                            write: false,
                        }],
                        effects: BTreeSet::from([EffectClass::Read]),
                        authority: workspace_state.authority,
                        policy: PolicyRevision::ZERO,
                        binding: workspace_state.binding.revision,
                        grants: BTreeMap::new(),
                        allocation: Micros::new(100),
                        deadline: Timestamp::new(u64::MAX),
                        snapshot: inputs.snapshot,
                        snapshot_digest: inputs.snapshot_digest,
                        registration: Some(inputs.registration),
                        registration_digest: Some(inputs.registration_digest),
                        isolated_root: Some(inputs.isolated_root),
                    },
                    limits: GraphLimits::default(),
                    expected_graph: graph.map(|g| g.revision),
                    expected_ledger: ledger.revision,
                },
                Some(config.root_task.clone()),
                parent.revision,
            )
            .unwrap();
            if child == &cancelled {
                // Poll only to the first completed file, then durably cancel
                // while the next copy yield is suspended. No timing race.
                let path = copies.join(child.as_str());
                let mut materializing = Box::pin(host.materialize_child_workspace(
                    root_thread,
                    child.clone(),
                    &snapshotter,
                    &disposable,
                ));
                for _ in 0..1000 {
                    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
                    assert!(std::future::Future::poll(materializing.as_mut(), &mut cx).is_pending());
                    if path.join("file.txt").exists() {
                        break;
                    }
                }
                assert!(path.join("file.txt").exists());
                assert!(!path.join("z.txt").exists());
                host.stop(
                    host.control_envelope(
                        CommandId::new(),
                        child.clone(),
                        Revision::ZERO,
                        Command::Transition {
                            next: TaskState::Cancelled,
                            reason: "cancel after first materialized file".into(),
                            verification: None,
                        },
                    )
                    .unwrap(),
                )
                .unwrap();
                let error = materializing.await.unwrap_err();
                assert!(error.contains("child changed"), "{error}");
                assert!(!path.join("z.txt").exists());
                assert_eq!(fs::read(path.join("file.txt")).unwrap(), b"base\n");
                let graph = vcp_engine::agents::graph(
                    &host.snapshot().unwrap(),
                    &root_binding.scope,
                    &config.root_task,
                )
                .unwrap()
                .unwrap();
                assert!(!graph.ready.contains_key(child));
                assert_eq!(current(&host, &config, child).state, TaskState::Cancelled);
                continue;
            }
            host.materialize_child_workspace(root_thread, child.clone(), &snapshotter, &disposable)
                .await
                .unwrap();
        }
        let first_path = copies.join(children[0].as_str());
        let ticket = host
            .prepare_child_start(root_thread, children[0].clone(), &snapshotter)
            .await
            .unwrap();
        host.lifecycle()
            .authorize_startup(ticket.workspace(), None)
            .unwrap();
        let old_child = spawn(&test, &first_path).await;
        let old_child_id = host
            .attach_prepared_child(ticket, old_child.clone())
            .unwrap();
        host.configure_child_from_parent(root_thread, old_child_id)
            .unwrap();
        let abandoned = host
            .prepare_child_start(root_thread, children[2].clone(), &snapshotter)
            .await
            .unwrap();
        assert_eq!(
            current(&host, &config, &children[2]).state,
            TaskState::Running
        );
        drop(abandoned);
        assert_eq!(
            current(&host, &config, &children[2]).state,
            TaskState::Paused
        );
        assert_eq!(
            current(&host, &config, &children[0]).state,
            TaskState::Running,
            "attached child is never paused by launch cleanup"
        );
        // These are legitimate current user edits in the disposable child tree;
        // recovery must not demand or restore the earlier materialized bytes.
        fs::write(first_path.join("file.txt"), b"retained child edits\n").unwrap();
        host.command(
            Command::Transition {
                next: TaskState::Paused,
                reason: "individually paused sibling".into(),
                verification: None,
            },
            Some(children[1].clone()),
            Revision::ZERO,
        )
        .unwrap();
        owner.close().await.unwrap();
        old_child.shutdown_and_wait().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        drop(old_child);
        drop(test);
        drop(host);

        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let (snapshot, raw) = provider_snapshot();
        host.configure_provider(snapshot, raw).unwrap();
        let test = retained(&host, &workspace, &server).await;
        let root_thread = host.lifecycle().attach_root(test.codex.clone()).unwrap();
        host.register(root_thread, root_binding).unwrap();
        let root_view = host.lifecycle().inspect(root_thread).unwrap();
        host.lifecycle()
            .hold(root_thread, &root_view.revision)
            .unwrap()
            .wait()
            .await
            .unwrap();
        let missing_path = copies.join(children[1].as_str());
        let moved = copies.join("temporarily-moved-child");
        fs::rename(&missing_path, &moved).unwrap();
        let blocked = host
            .prepare_child_recovery(root_thread, children[1].clone(), &snapshotter)
            .await
            .unwrap();
        assert!(blocked.ticket.is_none() && blocked.blocked.is_some());
        assert!(host
            .read_artifact(blocked.evidence)
            .unwrap()
            .windows(7)
            .any(|w| w == b"blocked"));
        fs::rename(&moved, &missing_path).unwrap();
        let mut attached = Vec::new();
        for child in &children {
            let recovery = host
                .prepare_child_recovery(root_thread, child.clone(), &snapshotter)
                .await
                .unwrap();
            assert!(recovery.blocked.is_none(), "{:?}", recovery.blocked);
            let mut ticket = recovery.ticket.unwrap();
            host.authorize_child_recovery_startup(&mut ticket).unwrap();
            assert!(host.authorize_child_recovery_startup(&mut ticket).is_err());
            let thread = spawn(&test, ticket.workspace()).await;
            let id = host
                .attach_recovered_child(ticket, thread.clone())
                .await
                .unwrap();
            assert!(host.lifecycle().inspect(id).unwrap().local_hold);
            assert_eq!(current(&host, &config, child).state, TaskState::Paused);
            attached.push((id, thread));
        }
        assert_eq!(
            fs::read(first_path.join("file.txt")).unwrap(),
            b"retained child edits\n"
        );
        assert_eq!(fs::read(workspace.join("file.txt")).unwrap(), b"base\n");
        let view = host.lifecycle().inspect(root_thread).unwrap();
        host.lifecycle()
            .resume(root_thread, &view.revision)
            .unwrap();
        let parent = current(&host, &config, &config.root_task);
        host.resume(root_thread, parent.revision, parent.fingerprint)
            .unwrap();
        configure_parent(&host, root_thread);
        for (child_id, _) in &attached {
            host.configure_child_from_parent(root_thread, *child_id)
                .unwrap();
            assert!(host.lifecycle().inspect(*child_id).unwrap().local_hold);
        }
        let (first_id, _) = &attached[0];
        let view = host.lifecycle().inspect(*first_id).unwrap();
        host.lifecycle().resume(*first_id, &view.revision).unwrap();
        let first = current(&host, &config, &children[0]);
        host.resume(*first_id, first.revision, first.fingerprint)
            .unwrap();
        assert_eq!(
            current(&host, &config, &children[0]).state,
            TaskState::Running
        );
        assert_eq!(
            current(&host, &config, &children[1]).state,
            TaskState::Paused
        );
        assert!(host.lifecycle().inspect(attached[1].0).unwrap().local_hold);
        // A canonical running sibling effect is not this child's resume debt.
        // Root resume must still include that same effect in its subtree.
        let effect = ToolRunId::new();
        let first = current(&host, &config, &children[0]);
        host.command(
            Command::ProposeEffect {
                id: effect.clone(),
                operation_digest: "d".repeat(64),
            },
            Some(children[0].clone()),
            first.revision,
        )
        .unwrap();
        let execution = ExecutionId::new();
        use vcp_domain::effect::EffectState;
        for (revision, next) in [
            EffectState::Validated,
            EffectState::Authorized,
            EffectState::DispatchRecorded,
            EffectState::Running,
        ]
        .into_iter()
        .enumerate()
        {
            host.command(
                Command::AdvanceEffect {
                    id: effect.clone(),
                    next,
                    reason: "synthetic canonical sibling execution".into(),
                    execution: if revision >= 2 {
                        Some(execution.clone())
                    } else {
                        None
                    },
                    exit_code: None,
                    observed_changes: vec![],
                },
                Some(children[0].clone()),
                Revision::new(revision as u64),
            )
            .unwrap();
        }
        let second_id = attached[1].0;
        let second_view = host.lifecycle().inspect(second_id).unwrap();
        host.lifecycle()
            .resume(second_id, &second_view.revision)
            .unwrap();
        let second = current(&host, &config, &children[1]);
        host.resume(second_id, second.revision, second.fingerprint)
            .unwrap();
        let parent = current(&host, &config, &config.root_task);
        host.stop(
            host.control_envelope(
                CommandId::new(),
                config.root_task.clone(),
                parent.revision,
                Command::Transition {
                    next: TaskState::Paused,
                    reason: "check root subtree recovery".into(),
                    verification: None,
                },
            )
            .unwrap(),
        )
        .unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while !host
                .lifecycle()
                .inspect(root_thread)
                .unwrap()
                .interrupt_complete
            {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let parent_view = host.lifecycle().inspect(root_thread).unwrap();
        host.lifecycle()
            .resume(root_thread, &parent_view.revision)
            .unwrap();
        // The retained gate is open, but canonical root resume still requires
        // reconciliation of the descendant effect.
        let parent = current(&host, &config, &config.root_task);
        assert!(host
            .resume(root_thread, parent.revision, parent.fingerprint)
            .is_err());
        host.command(
            Command::AdvanceEffect {
                id: effect,
                next: EffectState::Cancelled,
                reason: "fixture terminal observation".into(),
                execution: Some(execution),
                exit_code: None,
                observed_changes: vec![],
            },
            Some(children[0].clone()),
            Revision::new(4),
        )
        .unwrap();
        assert!(server.received_requests().await.unwrap().is_empty());
        owner.close().await.unwrap();
        for (_, child) in attached {
            child.shutdown_and_wait().await.unwrap();
        }
        test.codex.shutdown_and_wait().await.unwrap();
    }
}
