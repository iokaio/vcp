// SPDX-License-Identifier: Apache-2.0
use super::*;
use codex_core::{StartThreadOptions, TurnStartOptions};
use codex_protocol::protocol::{SessionSource, SubAgentSource};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};
use vcp_domain::{agents::*, policy::*};
use vcp_repository::{dirty_snapshot::CapturePolicy, worktree::Snapshotter, Root, RootIdentity};
use vcp_tools::Request;
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

fn current_task(host: &CanonicalHost, config: &Config, id: &TaskId) -> Task {
    host.snapshot()
        .unwrap()
        .record(Collection::Task, id.as_str(), &config.workspace)
        .unwrap()
        .decode()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn graph_child_dispatch_uses_isolated_bytes_root_budget_and_parent_pause_gate() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for (mode, pause_before_materialize) in [
            (ChildMode::ReadOnly, false),
            (ChildMode::IsolatedWrite, false),
            (ChildMode::ReadOnly, true),
        ] {
            child_case(backend, mode, pause_before_materialize, None, false, false).await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn child_result_integration_uses_parent_broker_and_rejects_concurrent_parent_changes() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for (mode, concurrent_parent_change) in [
            (ChildMode::ReadOnly, false),
            (ChildMode::IsolatedWrite, false),
            (ChildMode::IsolatedWrite, true),
        ] {
            child_case(
                backend,
                mode,
                false,
                Some(concurrent_parent_change),
                false,
                false,
            )
            .await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn child_fixed_model_mismatch_never_sends_or_reserves_budget() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        child_case(backend, ChildMode::ReadOnly, false, None, true, false).await;
    }
}

pub(super) async fn child_case(
    backend: BackendKind,
    mode: ChildMode,
    pause_before_materialize: bool,
    integration_case: Option<bool>,
    model_mismatch: bool,
    integration_fault: bool,
) {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    let disposable = temp.path().join("children");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&disposable).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    fs::write(workspace.join("file.txt"), "captured\n").unwrap();
    if integration_fault {
        fs::write(workspace.join("second.txt"), "second base\n").unwrap();
    }
    let snapshot_inputs = if integration_fault {
        BTreeSet::from(["file.txt".into(), "second.txt".into()])
    } else {
        BTreeSet::from(["file.txt".into()])
    };
    fs::write(workspace.join(".env"), "excluded fixture data").unwrap();
    let config = config(&temp.path().join("canonical"), &workspace, backend);
    let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
    let objective = Objective {
        text: "Observe isolated child work".into(),
        constraints: vec![],
        acceptance: vec!["Child evidence remains isolated and charges the root budget".into()],
        source: EventId::new(),
        steering: SteeringRevision::ZERO,
    };
    let fingerprint = Fingerprint {
        repository: "a".repeat(64),
        buffers: "b".repeat(64),
        environment: "c".repeat(64),
    };
    host.command(
        Command::CreateTask {
            root: config.root_task.clone(),
            parent: None,
            fork_origin: None,
            objective: objective.clone(),
            fingerprint: fingerprint.clone(),
            editing: true,
            required_checks: vec![],
        },
        Some(config.root_task.clone()),
        Revision::ZERO,
    )
    .unwrap();
    host.command(
        Command::Transition {
            next: TaskState::Running,
            reason: "fixture start".into(),
            verification: None,
        },
        Some(config.root_task.clone()),
        Revision::ZERO,
    )
    .unwrap();
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
                mode: Autonomy::Autonomous,
                denials: vec![],
                workspace_roots: BTreeSet::from([source_root.clone()]),
                automatic_effects: BTreeSet::from([EffectClass::Read, EffectClass::Write]),
                timeout_ceiling_ms: Units::new(30000),
                output_ceiling_bytes: ByteCount::new(1024 * 1024),
            },
        },
        None,
        Revision::ZERO,
    )
    .unwrap();
    host.initialize_root_budget().unwrap();
    let root_binding = ThreadBinding {
        scope: Scope {
            workspace: config.workspace.clone(),
            session: config.session.clone(),
            task: config.root_task.clone(),
        },
        agent: AgentId::new(),
        role: RequestRole::Main,
    };
    let server = start_mock_server().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse(vec![
                    ev_response_created("graph-child"),
                    ev_assistant_message("answer", "Observed child."),
                    ev_completed_with_tokens("graph-child", 7),
                ])),
        )
        .expect(if pause_before_materialize || model_mismatch {
            0
        } else {
            1
        })
        .mount(&server)
        .await;
    let mut registry = ExtensionRegistryBuilder::new();
    registry.turn_start_admission(Arc::new(host.clone()));
    registry.work_admission(Arc::new(host.clone()));
    let starter = host.clone();
    let cwd = workspace.clone();
    let model = config.price.model.clone();
    let test = test_codex()
        .with_extensions(Arc::new(registry.build()))
        .with_auth(codex_login::CodexAuth::from_api_key(
            "synthetic-child-fixture",
        ))
        .with_allowed_tools(AllowedTools(vec![]))
        .with_config(move |c| {
            c.cwd = cwd.try_into().unwrap();
            c.model = Some(model.clone());
            configure_fixture_provider(c);
            starter
                .lifecycle()
                .authorize_startup(c.cwd.as_path(), None)
                .unwrap();
        })
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let root_thread = host.lifecycle().attach_root(test.codex.clone()).unwrap();
    host.register(root_thread, root_binding.clone()).unwrap();
    let disposable = Root::open(
        RootIdentity {
            workspace: config.workspace.clone(),
            root: RootId::new(),
            repository: config.binding.repository.clone(),
            worktree: "disposable-parent".into(),
            binding: config.binding.revision,
        },
        &disposable,
    )
    .unwrap();
    let snapshotter = Snapshotter::new(
        std::env::var_os("VCP_TEST_GIT")
            .expect("native Git fixture")
            .into(),
        ["SystemRoot", "WINDIR", "PATH", "TEMP", "TMP"]
            .into_iter()
            .filter_map(|key| std::env::var_os(key).map(|value| (key.into(), value)))
            .collect(),
        Duration::from_secs(20),
        8 * 1024 * 1024,
    )
    .unwrap();
    let child_id = TaskId::new();
    let inputs = host
        .capture_child_workspace(
            root_thread,
            child_id.clone(),
            &snapshotter,
            &CapturePolicy {
                untracked: snapshot_inputs.clone(),
                required: snapshot_inputs.clone(),
                ..Default::default()
            },
            &disposable,
        )
        .await
        .unwrap();
    let state = host.snapshot().unwrap();
    let current: Workspace = state
        .record(
            Collection::Workspace,
            config.workspace.as_str(),
            &config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    let ledger = vcp_budget::ledger(&state, &root_binding.scope).unwrap();
    let parent = current_task(&host, &config, &config.root_task);
    host.command(
        Command::CreateChild {
            id: child_id.clone(),
            objective,
            fingerprint,
            required_checks: vec![],
            spec: ChildSpec {
                parent: config.root_task.clone(),
                actor: config.actor.clone(),
                parent_steering: parent.steering,
                dependencies: BTreeSet::new(),
                mode,
                role: "reviewer".into(),
                model_policy: if model_mismatch {
                    "different-qualified-model".into()
                } else {
                    config.price.model.clone()
                },
                paths: vec![ChildPath {
                    root: source_root,
                    path: "".into(),
                    write: mode == ChildMode::IsolatedWrite,
                }],
                effects: if mode == ChildMode::ReadOnly {
                    BTreeSet::from([EffectClass::Read])
                } else {
                    BTreeSet::from([EffectClass::Read, EffectClass::Write])
                },
                authority: current.authority,
                policy: PolicyRevision::ZERO,
                binding: current.binding.revision,
                grants: BTreeMap::new(),
                allocation: Micros::new(400),
                deadline: Timestamp::new(u64::MAX),
                snapshot: inputs.snapshot,
                snapshot_digest: inputs.snapshot_digest,
                registration: Some(inputs.registration),
                registration_digest: Some(inputs.registration_digest),
                isolated_root: Some(inputs.isolated_root),
            },
            limits: GraphLimits::default(),
            expected_graph: None,
            expected_ledger: ledger.revision,
        },
        Some(config.root_task.clone()),
        parent.revision,
    )
    .unwrap();
    if pause_before_materialize {
        let parent = current_task(&host, &config, &config.root_task);
        host.stop(
            host.control_envelope(
                CommandId::new(),
                config.root_task.clone(),
                parent.revision,
                Command::Transition {
                    next: TaskState::Paused,
                    reason: "pause before child materialization".into(),
                    verification: None,
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert!(host
            .materialize_child_workspace(root_thread, child_id.clone(), &snapshotter, &disposable)
            .await
            .is_err());
        assert!(!disposable.path().join(child_id.as_str()).exists());
        assert!(server.received_requests().await.unwrap().is_empty());
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        return;
    }
    host.materialize_child_workspace(root_thread, child_id.clone(), &snapshotter, &disposable)
        .await
        .unwrap();
    let child_path = disposable.path().join(child_id.as_str());
    assert!(!child_path.join(".env").exists());
    fs::write(
        workspace.join("file.txt"),
        "parent changed after snapshot\n",
    )
    .unwrap();
    fs::write(child_path.join("file.txt"), "tampered before launch\n").unwrap();
    assert!(host
        .prepare_child_start(root_thread, child_id.clone(), &snapshotter)
        .await
        .is_err());
    assert_eq!(
        current_task(&host, &config, &child_id).state,
        TaskState::Pending
    );
    fs::write(child_path.join("file.txt"), "captured\n").unwrap();
    let ticket = host
        .prepare_child_start(root_thread, child_id.clone(), &snapshotter)
        .await
        .unwrap();
    assert_eq!(ticket.workspace(), child_path.canonicalize().unwrap());
    assert!(host
        .prepare_child_start(root_thread, child_id.clone(), &snapshotter)
        .await
        .is_err());
    host.lifecycle()
        .authorize_startup(&child_path, None)
        .unwrap();
    let mut extension_init = codex_extension_api::ExtensionDataInit::default();
    extension_init.insert(AllowedTools(vec![]));
    let mut child_config = test.config.clone();
    child_config.cwd = child_path.clone().try_into().unwrap();
    let child = test
        .thread_manager
        .start_thread(StartThreadOptions {
            thread_extension_init: extension_init,
            session_source: Some(SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                parent_thread_id: root_thread,
                depth: 1,
                agent_path: None,
                agent_nickname: None,
                agent_role: None,
            })),
            environments: Some(test.codex.environment_selections().await),
            ..StartThreadOptions::new(child_config)
        })
        .await
        .unwrap()
        .thread;
    let thread = host.attach_prepared_child(ticket, child.clone()).unwrap();
    let read = || Request::Read {
        path: "file.txt".into(),
        max_bytes: 1024,
    };
    let captured = host.prepare_tool(thread, read()).unwrap();
    assert_eq!(
        host.dispatch_tool(captured).unwrap().result["text"],
        "captured\n"
    );
    let patch = host.prepare_tool(thread, Request::Patch { patch: "*** Begin Patch\n*** Update File: file.txt\n@@\n-captured\n+child changed\n*** End Patch".into() });
    if mode == ChildMode::ReadOnly {
        assert!(patch.is_err());
        assert_eq!(
            fs::read_to_string(child_path.join("file.txt")).unwrap(),
            "captured\n"
        );
    } else {
        host.dispatch_tool(patch.unwrap()).unwrap();
        if integration_fault {
            let second = host.prepare_tool(thread, Request::Patch { patch: "*** Begin Patch\n*** Update File: second.txt\n@@\n-second base\n+second child changed\n*** End Patch".into() }).unwrap();
            host.dispatch_tool(second).unwrap();
        }
        assert_eq!(
            fs::read_to_string(child_path.join("file.txt")).unwrap(),
            "child changed\n"
        );
    }
    assert_eq!(
        fs::read_to_string(workspace.join("file.txt")).unwrap(),
        "parent changed after snapshot\n"
    );
    assert!(host
        .prepare_tool(
            thread,
            Request::Patch {
                patch: "*** Begin Patch\n*** Delete File: .vcp-child-owner\n*** End Patch".into()
            }
        )
        .is_err());
    child
        .start_or_steer_turn(
            TurnInputRequest::user_input(vec![UserInput::Text {
                text: "Observe child request.".into(),
                text_elements: vec![],
            }])
            .on_start(TurnStartOptions {
                parent_turn_id: Some("graph-fixture".into()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
    if model_mismatch {
        let event = tokio::time::timeout(
            Duration::from_secs(30),
            wait_for_event(&child, |event| matches!(event, EventMsg::Error(_))),
        )
        .await
        .unwrap();
        assert!(format!("{event:?}").contains("fixed model assignment"));
        let state = host.snapshot().unwrap();
        assert!(!state
            .records
            .values()
            .any(|row| row.collection == Collection::Attempt));
        let ledger = vcp_budget::ledger(&state, &root_binding.scope).unwrap();
        assert_eq!(ledger.active, Micros::ZERO);
        assert_eq!(ledger.settled, Micros::ZERO);
        assert_eq!(ledger.unresolved, Micros::ZERO);
        assert!(server.received_requests().await.unwrap().is_empty());
        owner.close().await.unwrap();
        child.shutdown_and_wait().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        return;
    }
    tokio::time::timeout(
        Duration::from_secs(30),
        wait_for_event(&child, |event| {
            if let EventMsg::Error(error) = event {
                panic!("graph child request failed: {error:?}");
            }
            matches!(event, EventMsg::TurnComplete(_))
        }),
    )
    .await
    .unwrap();
    let state = host.snapshot().unwrap();
    let ledger = vcp_budget::ledger(&state, &root_binding.scope).unwrap();
    assert_eq!(ledger.allocations.get(&child_id), Some(&Micros::new(400)));
    assert_eq!(ledger.settled, Micros::new(100));
    assert_eq!(
        state
            .records
            .values()
            .filter(|row| row.collection == Collection::Ledger)
            .count(),
        1
    );
    let attempts: Vec<Attempt> = state
        .records
        .values()
        .filter(|row| row.collection == Collection::Attempt)
        .map(|row| row.decode().unwrap())
        .collect();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].scope.task, child_id);
    assert_eq!(attempts[0].role, RequestRole::Child);
    let packet = vcp_repository::merge::ChildPacket {
        base_fingerprint: "a".repeat(64),
        result_fingerprint: "b".repeat(64),
        changed_paths: BTreeSet::new(),
        findings: vec!["Untrusted finding survives rejected running-child integration".into()],
    };
    let packet_hash = vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&packet).unwrap());
    let integration = host
        .prepare_child_integration(root_thread, child_id.clone(), &snapshotter, packet)
        .await
        .unwrap();
    assert!(integration.proposal.is_none());
    assert!(integration
        .rejection
        .as_deref()
        .unwrap()
        .contains("stopped or result-ready"));
    let state = host.snapshot().unwrap();
    let captured: ArtifactDescriptor = state
        .record(
            Collection::Artifact,
            integration.packet.as_str(),
            &config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(captured.sha256, packet_hash);
    assert_eq!(captured.state, CaptureState::Complete);
    let plan: ArtifactDescriptor = state
        .record(
            Collection::Artifact,
            integration.plan.as_str(),
            &config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(plan.spec.schema, "child-integration-plan/1");
    assert_eq!(plan.state, CaptureState::Complete);
    if let Some(concurrent_parent_change) = integration_case {
        let mut partial_effect = None;
        let task = current_task(&host, &config, &child_id);
        host.stop(
            host.control_envelope(
                CommandId::new(),
                child_id.clone(),
                task.revision,
                Command::Transition {
                    next: TaskState::Paused,
                    reason: "child result ready for parent review".into(),
                    verification: None,
                },
            )
            .unwrap(),
        )
        .unwrap();
        let registration: vcp_repository::worktree::WorkspaceRegistration =
            serde_json::from_slice(&fs::read(child_path.join(".vcp-child-owner")).unwrap())
                .unwrap();
        let parent_root = Root::open(registration.source.clone(), &workspace).unwrap();
        let child_root = Root::open(registration.child.clone(), &child_path).unwrap();
        let result = snapshotter
            .capture_registered(
                &child_root,
                &parent_root,
                &registration,
                &CapturePolicy {
                    untracked: snapshot_inputs.clone(),
                    required: snapshot_inputs.clone(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let packet = vcp_repository::merge::ChildPacket {
            base_fingerprint: registration.snapshot,
            result_fingerprint: result.fingerprint,
            changed_paths: if mode == ChildMode::ReadOnly {
                BTreeSet::new()
            } else {
                BTreeSet::from(["file.txt".into()])
            },
            findings: vec!["Observed child result requires independent parent verification".into()],
        };
        // The parent returns to the shared base before considering the child's
        // independent change, then may race after preview to invalidate it.
        fs::write(workspace.join("file.txt"), "captured\n").unwrap();
        let prepared = if mode == ChildMode::ReadOnly {
            host.prepare_child_integration(root_thread, child_id.clone(), &snapshotter, packet)
                .await
        } else {
            host.prepare_observed_child_integration(root_thread, child_id.clone(), &snapshotter)
                .await
        }
        .unwrap();
        if mode == ChildMode::ReadOnly {
            assert!(prepared
                .rejection
                .as_deref()
                .unwrap()
                .contains("read-only findings retained"));
            assert!(prepared.proposal.is_none());
            assert_eq!(
                fs::read_to_string(workspace.join("file.txt")).unwrap(),
                "captured\n"
            );
        } else {
            assert!(prepared.rejection.is_none(), "{:?}", prepared.rejection);
            let proposal = prepared
                .proposal
                .expect("isolated change must reach the ordinary parent broker");
            if integration_fault {
                partial_effect = Some(super::child_integration_fault::apply_partial(
                    &host,
                    proposal,
                    &workspace,
                    &root_binding.scope,
                ));
            } else if concurrent_parent_change {
                fs::write(workspace.join("file.txt"), "concurrent parent edit\n").unwrap();
                assert!(host.dispatch_tool(proposal).is_err());
                assert_eq!(
                    fs::read_to_string(workspace.join("file.txt")).unwrap(),
                    "concurrent parent edit\n"
                );
            } else {
                let applied = host.dispatch_tool(proposal).unwrap();
                assert_eq!(applied.result["complete"], true);
                assert_eq!(
                    fs::read_to_string(workspace.join("file.txt")).unwrap(),
                    "child changed\n"
                );
            }
        }
        let parent = current_task(&host, &config, &config.root_task);
        assert_eq!(parent.state, TaskState::Running);
        assert!(host
            .command(
                Command::Transition {
                    next: TaskState::Completed,
                    reason: "child finding cannot prove parent acceptance".into(),
                    verification: None
                },
                Some(config.root_task.clone()),
                parent.revision
            )
            .is_err());
        assert_eq!(
            current_task(&host, &config, &child_id).state,
            TaskState::Paused
        );
        let graph = vcp_engine::agents::graph(
            &host.snapshot().unwrap(),
            &root_binding.scope,
            &config.root_task,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            graph.results[&child_id].len(),
            2,
            "rejected and accepted result packets remain inspectable"
        );
        if let Some(effect) = partial_effect {
            super::child_integration_fault::reconcile_preserves_edits(
                &host,
                root_thread,
                &workspace,
                &root_binding.scope,
                &effect,
            );
            owner.close().await.unwrap();
            child.shutdown_and_wait().await.unwrap();
            test.codex.shutdown_and_wait().await.unwrap();
            return;
        }
        let task = current_task(&host, &config, &child_id);
        host.stop(
            host.control_envelope(
                CommandId::new(),
                child_id.clone(),
                task.revision,
                Command::Transition {
                    next: TaskState::Cancelled,
                    reason: "cancelled child cannot integrate".into(),
                    verification: None,
                },
            )
            .unwrap(),
        )
        .unwrap();
        let rejected = host
            .prepare_child_integration(
                root_thread,
                child_id.clone(),
                &snapshotter,
                vcp_repository::merge::ChildPacket {
                    base_fingerprint: "a".repeat(64),
                    result_fingerprint: "b".repeat(64),
                    changed_paths: BTreeSet::new(),
                    findings: vec!["Retain cancelled child finding without applying it".into()],
                },
            )
            .await
            .unwrap();
        assert!(rejected.proposal.is_none());
        assert!(rejected
            .rejection
            .as_deref()
            .unwrap()
            .contains("stopped or result-ready"));
        assert_eq!(
            vcp_engine::agents::graph(
                &host.snapshot().unwrap(),
                &root_binding.scope,
                &config.root_task
            )
            .unwrap()
            .unwrap()
            .results[&child_id]
                .len(),
            3
        );
        owner.close().await.unwrap();
        child.shutdown_and_wait().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        return;
    }
    let stale = host.prepare_tool(thread, read()).unwrap();
    let parent = current_task(&host, &config, &config.root_task);
    host.stop(
        host.control_envelope(
            CommandId::new(),
            config.root_task.clone(),
            parent.revision,
            Command::Transition {
                next: TaskState::Paused,
                reason: "parent pause invalidates child dispatch".into(),
                verification: None,
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert!(host.dispatch_tool(stale).is_err());
    assert!(host.prepare_tool(thread, read()).is_err());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    owner.close().await.unwrap();
    child.shutdown_and_wait().await.unwrap();
    test.codex.shutdown_and_wait().await.unwrap();
}
