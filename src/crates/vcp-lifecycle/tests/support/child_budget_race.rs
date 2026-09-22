// SPDX-License-Identifier: Apache-2.0
use super::*;
use codex_core::StartThreadOptions;
use core_test_support::{
    streaming_sse::{start_streaming_sse_server, StreamingSseChunk},
    wait_for_event_with_timeout,
};
use std::{collections::BTreeSet, fs};
use vcp_domain::{agents::ChildMode, policy::*};
use vcp_lifecycle::foundation::DelegationRequest;
use vcp_repository::{worktree::Snapshotter, Root, RootIdentity};
use vcp_tools::Request;

fn request() -> TurnInputRequest {
    TurnInputRequest::user_input(vec![UserInput::Text {
        text: "Report the bounded synthetic observation.".into(),
        text_elements: vec![],
    }])
}

fn completed(id: &str) -> serde_json::Value {
    serde_json::json!({"type":"response.completed","response":{
        "id":id,"status":"completed","output":[],
        "usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}
    }})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn child_near_budget_race_admits_one_wire_request_and_preserves_shared_root() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let copies = temp.path().join("children");
        fs::create_dir(&workspace).unwrap();
        fs::create_dir(&copies).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        fs::write(workspace.join("file.txt"), b"parent source\n").unwrap();
        fs::write(workspace.join(".env"), b"excluded fixture input\n").unwrap();
        let mut config = config(&temp.path().join("canonical"), &workspace, backend);
        // Each request costs exactly 100 micros. The two 125-micro allocations
        // fit the 250-micro ordinary root ceiling but do not reserve or spend it.
        config.cap.micros = Micros::new(300);
        config.protected = Micros::new(50);
        config.max_transport_retries = 0;
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
        host.command(
            Command::SetPolicy {
                policy: Policy {
                    workspace: config.workspace.clone(),
                    revision: PolicyRevision::ZERO,
                    mode: Autonomy::Autonomous,
                    denials: vec![],
                    workspace_roots: BTreeSet::from([
                        RootId::parse(config.workspace.as_str()).unwrap()
                    ]),
                    automatic_effects: BTreeSet::from([EffectClass::Read]),
                    timeout_ceiling_ms: Units::new(30_000),
                    output_ceiling_bytes: ByteCount::new(1024 * 1024),
                },
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        host.initialize_root_budget().unwrap();
        let (snapshot, raw) = provider_snapshot();
        host.configure_provider(snapshot, raw).unwrap();

        let (release, gate) = tokio::sync::oneshot::channel();
        let (server, _) = start_streaming_sse_server(vec![
            vec![StreamingSseChunk {
                gate: None,
                body: sse(vec![
                    ev_response_created("budget-parent"),
                    ev_assistant_message("parent-answer", "Parent observation."),
                    completed("budget-parent"),
                ]),
            }],
            vec![
                StreamingSseChunk {
                    gate: None,
                    body: sse(vec![
                        ev_response_created("budget-child"),
                        ev_message_item_added("child-answer", ""),
                        ev_output_text_delta("Child observation."),
                    ]),
                },
                StreamingSseChunk {
                    gate: Some(gate),
                    body: sse(vec![completed("budget-child")]),
                },
            ],
        ])
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
                "synthetic-child-budget-fixture",
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
            .build_with_streaming_server(&server)
            .await
            .unwrap();
        let parent = host.lifecycle().attach_root(test.codex.clone()).unwrap();
        host.register(parent, root_binding.clone()).unwrap();
        let disposable = Root::open(
            RootIdentity {
                workspace: config.workspace.clone(),
                root: RootId::new(),
                repository: config.binding.repository.clone(),
                worktree: "budget-child-copies".into(),
                binding: config.binding.revision,
            },
            &copies,
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
        let mut children = Vec::new();
        let mut child_threads = Vec::new();
        let mut ids = Vec::new();
        for _ in 0..2 {
            let id = host
                .delegate_child(
                    parent,
                    DelegationRequest {
                        role: "bounded budget observation".into(),
                        read_paths: BTreeSet::from([String::new()]),
                        helper: None,
                        objective: "Observe the isolated source under the shared root cap".into(),
                        acceptance: vec!["Retain scoped evidence and exact root charges".into()],
                        mode: ChildMode::ReadOnly,
                        write_paths: BTreeSet::new(),
                        untracked_inputs: BTreeSet::from(["file.txt".into()]),
                        allocation: Micros::new(125),
                        deadline: Timestamp::new(u64::MAX),
                        required_checks: vec![],
                    },
                    &snapshotter,
                    &disposable,
                )
                .await
                .unwrap();
            let ticket = host
                .prepare_child_start(parent, id.clone(), &snapshotter)
                .await
                .unwrap();
            let child_path = ticket.workspace().to_path_buf();
            host.lifecycle()
                .authorize_startup(&child_path, None)
                .unwrap();
            let mut child_config = test.config.clone();
            child_config.cwd = child_path.clone().try_into().unwrap();
            let mut extensions = codex_extension_api::ExtensionDataInit::default();
            extensions.insert(AllowedTools(vec![]));
            let child = test
                .thread_manager
                .start_thread(StartThreadOptions {
                    thread_extension_init: extensions,
                    environments: Some(test.codex.environment_selections().await),
                    ..StartThreadOptions::new(child_config)
                })
                .await
                .unwrap()
                .thread;
            let thread = host.attach_prepared_child(ticket, child.clone()).unwrap();
            let read = host
                .prepare_tool(
                    thread,
                    Request::Read {
                        path: "file.txt".into(),
                        max_bytes: 1024,
                        start_line: None,
                        end_line: None,
                    },
                )
                .unwrap();
            assert_eq!(
                host.dispatch_tool(read).unwrap().result["text"],
                "parent source\n"
            );
            assert!(!child_path.join(".env").exists());
            assert!(child_path.join(".vcp-child-owner").is_file());
            ids.push(id);
            children.push(child);
            child_threads.push(thread);
        }
        let state = host.snapshot().unwrap();
        let ledger = vcp_budget::ledger(&state, &root_binding.scope).unwrap();
        assert_eq!(ledger.allocations.len(), 2);
        assert_eq!(
            (
                ledger.settled.get(),
                ledger.active.get(),
                ledger.unresolved.get()
            ),
            (0, 0, 0)
        );

        // A real parent request consumes 100, leaving 150 ordinary micros.
        // Both child allocations independently admit 100; the shared root can
        // admit only one while its response is held, and also after it settles.
        let (snapshot, _) = provider_snapshot();
        let sealed = sealed_provider_context(&host, parent, &snapshot, None);
        host.prepare_context(parent, sealed, serde_json::json!([]), vec![])
            .unwrap();
        test.codex.start_or_steer_turn(request()).await.unwrap();
        wait_for_event_with_timeout(
            &test.codex,
            |event| {
                if let EventMsg::Error(error) = event {
                    panic!("parent fixture failed: {error:?}");
                }
                matches!(event, EventMsg::TurnComplete(_))
            },
            Duration::from_secs(30),
        )
        .await;
        assert_eq!(
            vcp_budget::ledger(&host.snapshot().unwrap(), &root_binding.scope)
                .unwrap()
                .settled
                .get(),
            100
        );
        for thread in child_threads {
            let sealed = sealed_provider_context(&host, thread, &snapshot, None);
            host.prepare_context(thread, sealed, serde_json::json!([]), vec![])
                .unwrap();
        }
        let (a, b) = tokio::join!(
            children[0].start_or_steer_turn(request()),
            children[1].start_or_steer_turn(request()),
        );
        a.unwrap();
        b.unwrap();
        let first = |event: &EventMsg| {
            matches!(
                event,
                EventMsg::Error(_) | EventMsg::AgentMessageContentDelta(_)
            )
        };
        let (a, b) = tokio::join!(
            wait_for_event_with_timeout(&children[0], first, Duration::from_secs(30)),
            wait_for_event_with_timeout(&children[1], first, Duration::from_secs(30)),
        );
        let winner = match (&a, &b) {
            (EventMsg::AgentMessageContentDelta(_), EventMsg::Error(error)) => {
                assert!(
                    error.message.contains("budget exhausted: root cap"),
                    "{error:?}"
                );
                0
            }
            (EventMsg::Error(error), EventMsg::AgentMessageContentDelta(_)) => {
                assert!(
                    error.message.contains("budget exhausted: root cap"),
                    "{error:?}"
                );
                1
            }
            _ => panic!("expected one admitted child and one denied child: {a:?}, {b:?}"),
        };
        let state = host.snapshot().unwrap();
        let ledger = vcp_budget::ledger(&state, &root_binding.scope).unwrap();
        assert_eq!(
            (
                ledger.settled.get(),
                ledger.active.get(),
                ledger.unresolved.get(),
                ledger.protected.get()
            ),
            (100, 100, 0, 50)
        );
        assert_eq!(
            server.requests().await.len(),
            2,
            "one parent and one child reached the wire"
        );
        let attempts: Vec<Attempt> = state
            .records
            .values()
            .filter(|row| row.collection == Collection::Attempt)
            .map(|row| row.decode().unwrap())
            .collect();
        assert_eq!(attempts.len(), 2);
        let admitted = attempts
            .iter()
            .find(|attempt| attempt.role == RequestRole::Child)
            .unwrap();
        assert_eq!(admitted.scope.task, ids[winner]);
        assert_eq!(admitted.root, config.root_task);
        assert!(admitted.send_intent.is_some());
        assert!(!attempts
            .iter()
            .any(|attempt| attempt.scope.task == ids[1 - winner]));
        let reservations: Vec<Reservation> = state
            .records
            .values()
            .filter(|row| row.collection == Collection::Reservation)
            .map(|row| row.decode().unwrap())
            .collect();
        assert_eq!(reservations.len(), 2);
        assert!(reservations.iter().all(|reservation| {
            reservation.root == config.root_task
                && reservation.protected_draw == Micros::ZERO
                && reservation.scope.task != ids[1 - winner]
        }));
        assert_eq!(
            state
                .records
                .values()
                .filter(|row| row.collection == Collection::Ledger)
                .count(),
            1
        );
        let graph = vcp_engine::agents::graph(&state, &root_binding.scope, &config.root_task)
            .unwrap()
            .unwrap();
        assert_eq!(graph.children.len(), 2);
        assert_eq!(graph.ready.len(), 2);
        for id in &ids {
            assert_eq!(ledger.allocations.get(id), Some(&Micros::new(125)));
            let child: Task = state
                .record(Collection::Task, id.as_str(), &config.workspace)
                .unwrap()
                .decode()
                .unwrap();
            assert_ne!(child.state, TaskState::Completed);
            assert_eq!(
                fs::read(copies.join(id.as_str()).join("file.txt")).unwrap(),
                b"parent source\n"
            );
        }
        release.send(()).unwrap();
        wait_for_event_with_timeout(
            &children[winner],
            |event| {
                if let EventMsg::Error(error) = event {
                    panic!("admitted child failed: {error:?}");
                }
                matches!(event, EventMsg::TurnComplete(_))
            },
            Duration::from_secs(30),
        )
        .await;
        let state = host.snapshot().unwrap();
        let ledger = vcp_budget::ledger(&state, &root_binding.scope).unwrap();
        assert_eq!(
            (
                ledger.settled.get(),
                ledger.active.get(),
                ledger.unresolved.get(),
                ledger.protected.get()
            ),
            (200, 0, 0, 50)
        );
        assert_eq!(server.requests().await.len(), 2);
        assert_eq!(
            fs::read(workspace.join("file.txt")).unwrap(),
            b"parent source\n"
        );
        assert_eq!(
            fs::read(workspace.join(".env")).unwrap(),
            b"excluded fixture input\n"
        );
        owner.close().await.unwrap();
        for child in children {
            child.shutdown_and_wait().await.unwrap();
        }
        test.codex.shutdown_and_wait().await.unwrap();
        server.shutdown().await;
    }
}
