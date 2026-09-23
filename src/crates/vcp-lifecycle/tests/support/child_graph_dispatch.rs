// SPDX-License-Identifier: Apache-2.0
use super::*;
use codex_core::StartThreadOptions;
use core_test_support::wait_for_event_with_timeout;
use std::{collections::BTreeSet, fs, path::PathBuf};
use vcp_domain::{agents::ChildMode, policy::*};
use vcp_lifecycle::foundation::{
    verification::VerificationConfig, CanonicalOwner, DelegationRequest,
};
use vcp_repository::{worktree::Snapshotter, Root, RootIdentity};
use vcp_tools::Request;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

#[cfg(feature = "qualification")]
#[path = "child_write_crash.rs"]
mod child_write_crash;
#[cfg(feature = "qualification")]
#[path = "model_dispatch_crash.rs"]
mod model_dispatch_crash;

#[path = "seeded_child_graph.rs"]
mod seeded_child_graph;

struct Fixture {
    _temp: tempfile::TempDir,
    workspace: PathBuf,
    host: CanonicalHost,
    owner: CanonicalOwner,
    config: Config,
    binding: ThreadBinding,
    parent: codex_protocol::ThreadId,
    test: TestCodex,
    server: MockServer,
    snapshotter: Snapshotter,
    disposable: Root,
    children: Vec<Arc<codex_core::CodexThread>>,
}
impl Fixture {
    async fn new(backend: BackendKind, requests: u64) -> Self {
        Self::new_in(backend, requests, tempfile::tempdir().unwrap()).await
    }

    async fn new_in(backend: BackendKind, requests: u64, temp: tempfile::TempDir) -> Self {
        Self::new_in_endpoint(backend, requests, temp, None).await
    }

    async fn new_in_endpoint(
        backend: BackendKind,
        requests: u64,
        temp: tempfile::TempDir,
        endpoint: Option<String>,
    ) -> Self {
        let workspace = temp.path().join("workspace");
        let copies = temp.path().join("children");
        fs::create_dir(&workspace).unwrap();
        fs::create_dir(&copies).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        for name in ["left.txt", "right.txt", "outside.txt"] {
            fs::write(workspace.join(name), b"base\n").unwrap();
        }
        let mut config = config(&temp.path().join("canonical"), &workspace, backend);
        config.max_transport_retries = 0;
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        // This owner explicitly permits editing assignments. The generic
        // analysis-only task helper must not be used to broaden child scope.
        host.command(
            Command::CreateTask {
                root: config.root_task.clone(),
                parent: None,
                fork_origin: None,
                objective: Objective {
                    text: "Qualify bounded child isolation and dependencies".into(),
                    constraints: vec![],
                    acceptance: vec!["Observe actual scoped effects and retained evidence".into()],
                    source: EventId::new(),
                    steering: SteeringRevision::ZERO,
                },
                fingerprint: Fingerprint {
                    repository: "a".repeat(64),
                    buffers: "b".repeat(64),
                    environment: "c".repeat(64),
                },
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
                reason: "explicit fixture start".into(),
                verification: None,
            },
            Some(config.root_task.clone()),
            Revision::ZERO,
        )
        .unwrap();
        let binding = ThreadBinding {
            scope: Scope {
                workspace: config.workspace.clone(),
                session: config.session.clone(),
                task: config.root_task.clone(),
            },
            agent: AgentId::new(),
            role: RequestRole::Main,
        };
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
                    automatic_effects: BTreeSet::from([EffectClass::Read, EffectClass::Write]),
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
        let server = start_mock_server().await;
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse(vec![
                        ev_response_created("dependent-observation"),
                        ev_assistant_message(
                            "answer",
                            "Observed after the verified dependency completed.",
                        ),
                    serde_json::json!({"type":"response.completed","response":{
                        "id":"dependent-observation","status":"completed","output":[],
                        "usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}
                    }}),
                    ])),
            )
            .expect(requests)
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
                "synthetic-child-graph-fixture",
            ))
            .with_allowed_tools(AllowedTools(vec![]))
            .with_config(move |c| {
                c.cwd = cwd.try_into().unwrap();
                c.model = Some(model.clone());
                configure_fixture_provider(c);
                if let Some(endpoint) = endpoint {
                    c.model_provider.base_url = Some(format!("{endpoint}/v1"));
                }
                starter
                    .lifecycle()
                    .authorize_startup(c.cwd.as_path(), None)
                    .unwrap();
            })
            .build_with_auto_env(&server)
            .await
            .unwrap();
        let parent = host.lifecycle().attach_root(test.codex.clone()).unwrap();
        host.register(parent, binding.clone()).unwrap();
        let disposable = Root::open(
            RootIdentity {
                workspace: config.workspace.clone(),
                root: RootId::new(),
                repository: config.binding.repository.clone(),
                worktree: "graph-child-copies".into(),
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
        Self {
            _temp: temp,
            workspace,
            host,
            owner,
            config,
            binding,
            parent,
            test,
            server,
            snapshotter,
            disposable,
            children: vec![],
        }
    }

    async fn assign(&self, write_paths: &[&str]) -> TaskId {
        self.host
            .delegate_child(
                self.parent,
                DelegationRequest {
                    role: "bounded graph observation".into(),
                    read_paths: BTreeSet::from([String::new()]),
                    helper: None,
                    objective: "Observe assigned sources and perform only the assigned scoped work"
                        .into(),
                    acceptance: vec![
                        "Retain scoped evidence without affecting siblings or parent".into(),
                    ],
                    mode: if write_paths.is_empty() {
                        ChildMode::ReadOnly
                    } else {
                        ChildMode::IsolatedWrite
                    },
                    write_paths: write_paths.iter().map(|path| (*path).into()).collect(),
                    untracked_inputs: ["left.txt", "right.txt", "outside.txt"]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                    allocation: Micros::new(400),
                    deadline: Timestamp::new(u64::MAX),
                    required_checks: vec![],
                },
                &self.snapshotter,
                &self.disposable,
            )
            .await
            .unwrap()
    }

    async fn start(
        &mut self,
        id: &TaskId,
    ) -> (
        codex_protocol::ThreadId,
        PathBuf,
        Arc<codex_core::CodexThread>,
    ) {
        let ticket = self
            .host
            .prepare_child_start(self.parent, id.clone(), &self.snapshotter)
            .await
            .unwrap();
        let child_path = ticket.workspace().to_path_buf();
        self.host
            .lifecycle()
            .authorize_startup(&child_path, None)
            .unwrap();
        let mut config = self.test.config.clone();
        config.cwd = child_path.clone().try_into().unwrap();
        let mut extensions = codex_extension_api::ExtensionDataInit::default();
        extensions.insert(AllowedTools(vec![]));
        let child = self
            .test
            .thread_manager
            .start_thread(StartThreadOptions {
                thread_extension_init: extensions,
                environments: Some(self.test.codex.environment_selections().await),
                ..StartThreadOptions::new(config)
            })
            .await
            .unwrap()
            .thread;
        let thread = self
            .host
            .attach_prepared_child(ticket, child.clone())
            .unwrap();
        self.children.push(child.clone());
        (thread, child_path, child)
    }

    fn current(&self, id: &TaskId) -> Task {
        self.host
            .snapshot()
            .unwrap()
            .record(Collection::Task, id.as_str(), &self.config.workspace)
            .unwrap()
            .decode()
            .unwrap()
    }

    async fn close(self) {
        self.owner.close().await.unwrap();
        for child in self.children {
            child.shutdown_and_wait().await.unwrap();
        }
        self.test.codex.shutdown_and_wait().await.unwrap();
    }
}

fn patch(path: &str, before: &str, after: &str) -> Request {
    Request::Patch {
        patch: format!(
            "*** Begin Patch\n*** Update File: {path}\n@@\n-{before}\n+{after}\n*** End Patch"
        ),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn child_sibling_disjoint_and_overlapping_writes_remain_isolated_and_scope_bound() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for overlap in [false, true] {
            let mut f = Fixture::new(backend, 0).await;
            let right_path = if overlap { "left.txt" } else { "right.txt" };
            let a = f.assign(&["left.txt"]).await;
            let b = f.assign(&[right_path]).await;
            let (a_thread, a_path, _) = f.start(&a).await;
            let (b_thread, b_path, _) = f.start(&b).await;
            assert_ne!(a_path, b_path);
            assert_eq!(f.current(&a).state, TaskState::Running);
            assert_eq!(f.current(&b).state, TaskState::Running);
            let state = f.host.snapshot().unwrap();
            let graph = vcp_engine::agents::graph(&state, &f.binding.scope, &f.config.root_task)
                .unwrap()
                .unwrap();
            assert_ne!(
                graph.children[&a].isolated_root,
                graph.children[&b].isolated_root
            );

            // Both assignments exist before either native edit. Even overlapping
            // logical names must refer to separate verified disposable roots.
            let proposed = f
                .host
                .prepare_tool(a_thread, patch("left.txt", "base", "child A"))
                .unwrap();
            assert_eq!(
                f.host.dispatch_tool(proposed).unwrap().result["complete"],
                true
            );
            assert_eq!(fs::read(b_path.join("left.txt")).unwrap(), b"base\n");
            let proposed = f
                .host
                .prepare_tool(b_thread, patch(right_path, "base", "child B"))
                .unwrap();
            assert_eq!(
                f.host.dispatch_tool(proposed).unwrap().result["complete"],
                true
            );

            // A newly discovered desired write does not expand either assignment.
            // Reject a mixed batch before applying its otherwise permitted edit.
            let mixed = Request::Patch {
                patch: format!(
                    "*** Begin Patch\n*** Update File: {right_path}\n@@\n-child B\n+must not apply\n*** Update File: outside.txt\n@@\n-base\n+forbidden\n*** End Patch"
                ),
            };
            assert!(f.host.prepare_tool(b_thread, mixed).is_err());
            assert!(f
                .host
                .prepare_tool(a_thread, patch("right.txt", "base", "forbidden"))
                .is_err());
            assert_eq!(fs::read(a_path.join("left.txt")).unwrap(), b"child A\n");
            assert_eq!(fs::read(a_path.join("right.txt")).unwrap(), b"base\n");
            assert_eq!(fs::read(b_path.join(right_path)).unwrap(), b"child B\n");
            if !overlap {
                assert_eq!(fs::read(b_path.join("left.txt")).unwrap(), b"base\n");
            }
            for root in [&a_path, &b_path, &f.workspace] {
                assert_eq!(fs::read(root.join("outside.txt")).unwrap(), b"base\n");
            }
            for name in ["left.txt", "right.txt"] {
                assert_eq!(fs::read(f.workspace.join(name)).unwrap(), b"base\n");
            }
            let state = f.host.snapshot().unwrap();
            let ledger = vcp_budget::ledger(&state, &f.binding.scope).unwrap();
            assert_eq!(
                (
                    ledger.active.get(),
                    ledger.settled.get(),
                    ledger.unresolved.get()
                ),
                (0, 0, 0)
            );
            assert_eq!(ledger.allocations.len(), 2);
            assert!(f.server.received_requests().await.unwrap().is_empty());
            f.close().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn child_dependency_blocks_launch_until_current_analysis_verification_completes() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let mut f = Fixture::new(backend, 1).await;
        let predecessor = f.assign(&[]).await;
        let dependent = f.assign(&[]).await;
        let state = f.host.snapshot().unwrap();
        let graph = vcp_engine::agents::graph(&state, &f.binding.scope, &f.config.root_task)
            .unwrap()
            .unwrap();
        f.host
            .command(
                Command::SetChildDependencies {
                    child: dependent.clone(),
                    dependencies: BTreeSet::from([predecessor.clone()]),
                    expected_graph: graph.revision,
                },
                Some(f.config.root_task.clone()),
                f.current(&f.config.root_task).revision,
            )
            .unwrap();
        assert!(f
            .host
            .prepare_child_start(f.parent, dependent.clone(), &f.snapshotter)
            .await
            .is_err());
        assert_eq!(f.current(&dependent).state, TaskState::Pending);
        assert!(f.server.received_requests().await.unwrap().is_empty());
        assert!(!f
            .host
            .snapshot()
            .unwrap()
            .records
            .values()
            .any(|row| row.collection == Collection::Attempt));

        let (thread, _, _) = f.start(&predecessor).await;
        f.host.configure_verification(thread, VerificationConfig {
            requirements: vec![], rationale: "Analysis-only predecessor: cite the unchanged isolated source; no process check is claimed".into(),
        }).unwrap();
        let read = f
            .host
            .prepare_tool(
                thread,
                Request::Read {
                    path: "left.txt".into(),
                    max_bytes: 1024,
                    start_line: None,
                    end_line: None,
                },
            )
            .unwrap();
        let observed = f.host.dispatch_tool(read).unwrap();
        assert_eq!(observed.result["text"], "base\n");
        assert!(f
            .host
            .command(
                Command::Transition {
                    next: TaskState::Completed,
                    reason: "An observation without verification is not completion".into(),
                    verification: None,
                },
                Some(predecessor.clone()),
                f.current(&predecessor).revision
            )
            .is_err());
        let verification = f
            .host
            .verify(thread, vec![observed.evidence.spec.id])
            .await
            .unwrap();
        assert_eq!(verification.scope.task, predecessor);
        assert!(verification.checks.is_empty());
        assert!(verification.outstanding_issues.is_empty());
        // A passing report by itself is not a terminal dependency state.
        assert!(f
            .host
            .prepare_child_start(f.parent, dependent.clone(), &f.snapshotter)
            .await
            .is_err());
        f.host
            .complete_verified(thread, verification.id.clone())
            .unwrap();
        assert_eq!(f.current(&predecessor).state, TaskState::Completed);
        let state = f.host.snapshot().unwrap();
        let retained: vcp_domain::verification::Verification = state
            .record(
                Collection::Verification,
                verification.id.as_str(),
                &f.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(retained, verification);
        let (dependent_thread, _, child) = f.start(&dependent).await;
        let (snapshot, _) = provider_snapshot();
        let sealed = sealed_provider_context(&f.host, dependent_thread, &snapshot, None);
        f.host
            .prepare_context(dependent_thread, sealed, serde_json::json!([]), vec![])
            .unwrap();
        child
            .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                text: "Report the observation after the predecessor completed.".into(),
                text_elements: vec![],
            }]))
            .await
            .unwrap();
        wait_for_event_with_timeout(
            &child,
            |event| {
                if let EventMsg::Error(error) = event {
                    panic!("dependent request failed: {error:?}");
                }
                matches!(event, EventMsg::TurnComplete(_))
            },
            Duration::from_secs(30),
        )
        .await;
        assert_eq!(f.server.received_requests().await.unwrap().len(), 1);
        let state = f.host.snapshot().unwrap();
        let attempts: Vec<Attempt> = state
            .records
            .values()
            .filter(|row| row.collection == Collection::Attempt)
            .map(|row| row.decode().unwrap())
            .collect();
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0].scope.task, dependent);
        assert_eq!(attempts[0].root, f.config.root_task);
        assert_eq!(attempts[0].role, RequestRole::Child);
        let ledger = vcp_budget::ledger(&state, &f.binding.scope).unwrap();
        assert_eq!(
            (
                ledger.settled.get(),
                ledger.active.get(),
                ledger.unresolved.get()
            ),
            (100, 0, 0)
        );
        let graph = vcp_engine::agents::graph(&state, &f.binding.scope, &f.config.root_task)
            .unwrap()
            .unwrap();
        assert_eq!(
            graph.children[&dependent].dependencies,
            BTreeSet::from([predecessor])
        );
        for name in ["left.txt", "right.txt", "outside.txt"] {
            assert_eq!(fs::read(f.workspace.join(name)).unwrap(), b"base\n");
        }
        f.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn child_individual_cancellation_preserves_calling_sibling_and_exact_liability() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for next in [TaskState::Paused, TaskState::Cancelled] {
            let mut f = Fixture::new(backend, 0).await;
            f.server.reset().await;
            let counter = Arc::new(AtomicUsize::new(0));
            let observed = counter.clone();
            Mock::given(method("POST")).and(path("/v1/responses"))
            .respond_with(move |_: &wiremock::Request| {
                let index = observed.fetch_add(1, Ordering::SeqCst);
                ResponseTemplate::new(200).insert_header("content-type", "text/event-stream")
                    .set_body_string(sse(vec![serde_json::json!({"type":"response.completed","response":{
                        "id":format!("sibling-response-{index}"),"status":"completed","output":[],
                        "usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}
                    }})])).set_delay(Duration::from_secs(5))
            }).expect(2).mount(&f.server).await;
            let cancelled = f.assign(&[]).await;
            let sibling = f.assign(&[]).await;
            let (cancelled_thread, cancelled_path, cancelled_loop) = f.start(&cancelled).await;
            let (sibling_thread, sibling_path, sibling_loop) = f.start(&sibling).await;
            let (snapshot, _) = provider_snapshot();
            for thread in [cancelled_thread, sibling_thread] {
                let sealed = sealed_provider_context(&f.host, thread, &snapshot, None);
                f.host
                    .prepare_context(thread, sealed, serde_json::json!([]), vec![])
                    .unwrap();
            }
            let request = || {
                TurnInputRequest::user_input(vec![UserInput::Text {
                    text: "Observe the assigned source without edits.".into(),
                    text_elements: vec![],
                }])
            };
            let (a, b) = tokio::join!(
                cancelled_loop.start_or_steer_turn(request()),
                sibling_loop.start_or_steer_turn(request())
            );
            a.unwrap();
            b.unwrap();
            tokio::time::timeout(Duration::from_secs(10), async {
                while f.server.received_requests().await.unwrap().len() != 2 {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            let state = f.host.snapshot().unwrap();
            let ledger = vcp_budget::ledger(&state, &f.binding.scope).unwrap();
            assert_eq!(
                (
                    ledger.active.get(),
                    ledger.settled.get(),
                    ledger.unresolved.get()
                ),
                (200, 0, 0)
            );
            f.host
                .stop(
                    f.host
                        .control_envelope(
                            CommandId::new(),
                            cancelled.clone(),
                            f.current(&cancelled).revision,
                            Command::Transition {
                                next,
                                reason: "Explicitly stop one child while its sibling is calling"
                                    .into(),
                                verification: None,
                            },
                        )
                        .unwrap(),
                )
                .unwrap();
            wait_for_event_with_timeout(
                &cancelled_loop,
                |event| matches!(event, EventMsg::TurnAborted(_)),
                Duration::from_secs(10),
            )
            .await;
            let state = f.host.snapshot().unwrap();
            let ledger = vcp_budget::ledger(&state, &f.binding.scope).unwrap();
            assert_eq!(
                (
                    ledger.active.get(),
                    ledger.settled.get(),
                    ledger.unresolved.get()
                ),
                (100, 0, 100),
                "sibling remains active; cancelled request is not refunded without a receipt"
            );
            assert_eq!(f.current(&cancelled).state, next);
            assert_eq!(f.current(&sibling).state, TaskState::Running);
            assert_eq!(f.current(&f.config.root_task).state, TaskState::Running);
            let sibling_view = f.host.lifecycle().inspect(sibling_thread).unwrap();
            assert!(!sibling_view.local_hold && !sibling_view.inherited_hold);
            assert!(f
                .host
                .prepare_tool(
                    cancelled_thread,
                    Request::Read {
                        path: "left.txt".into(),
                        max_bytes: 1024,
                        start_line: None,
                        end_line: None
                    }
                )
                .is_err());
            let read = f
                .host
                .prepare_tool(
                    sibling_thread,
                    Request::Read {
                        path: "left.txt".into(),
                        max_bytes: 1024,
                        start_line: None,
                        end_line: None,
                    },
                )
                .unwrap();
            assert_eq!(f.host.dispatch_tool(read).unwrap().result["text"], "base\n");
            wait_for_event_with_timeout(
                &sibling_loop,
                |event| {
                    if let EventMsg::Error(error) = event {
                        panic!("unrelated sibling failed: {error:?}");
                    }
                    matches!(event, EventMsg::TurnComplete(_))
                },
                Duration::from_secs(15),
            )
            .await;
            let state = f.host.snapshot().unwrap();
            let ledger = vcp_budget::ledger(&state, &f.binding.scope).unwrap();
            assert_eq!(
                (
                    ledger.active.get(),
                    ledger.settled.get(),
                    ledger.unresolved.get()
                ),
                (0, 100, 100)
            );
            let attempts: Vec<Attempt> = state
                .records
                .values()
                .filter(|row| row.collection == Collection::Attempt)
                .map(|row| row.decode().unwrap())
                .collect();
            assert_eq!(attempts.len(), 2);
            assert_eq!(
                attempts
                    .iter()
                    .find(|a| a.scope.task == cancelled)
                    .unwrap()
                    .phase,
                ReservationState::ReconciliationPending
            );
            assert_eq!(
                attempts
                    .iter()
                    .find(|a| a.scope.task == sibling)
                    .unwrap()
                    .phase,
                ReservationState::Settled
            );
            assert_eq!(counter.load(Ordering::SeqCst), 2);
            for root in [&cancelled_path, &sibling_path, &f.workspace] {
                assert_eq!(fs::read(root.join("left.txt")).unwrap(), b"base\n");
            }
            f.close().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn child_unexpected_provider_disconnect_still_pauses_root_with_full_liability() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let mut f = Fixture::new(backend, 0).await;
        f.server.reset().await;
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse(vec![ev_response_created(
                        "unexpected-child-disconnect",
                    )])),
            )
            .expect(1)
            .mount(&f.server)
            .await;
        let child_id = f.assign(&[]).await;
        let (thread, _, child) = f.start(&child_id).await;
        let (snapshot, _) = provider_snapshot();
        let sealed = sealed_provider_context(&f.host, thread, &snapshot, None);
        f.host
            .prepare_context(thread, sealed, serde_json::json!([]), vec![])
            .unwrap();
        child
            .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                text: "Observe the source; the fixture disconnects unexpectedly.".into(),
                text_elements: vec![],
            }]))
            .await
            .unwrap();
        wait_for_event_with_timeout(
            &child,
            |event| matches!(event, EventMsg::Error(_)),
            Duration::from_secs(15),
        )
        .await;
        assert_eq!(f.current(&f.config.root_task).state, TaskState::Paused);
        let state = f.host.snapshot().unwrap();
        let ledger = vcp_budget::ledger(&state, &f.binding.scope).unwrap();
        assert_eq!(
            (
                ledger.active.get(),
                ledger.settled.get(),
                ledger.unresolved.get()
            ),
            (0, 0, 100)
        );
        assert_eq!(f.server.received_requests().await.unwrap().len(), 1);
        assert!(f
            .host
            .prepare_tool(
                thread,
                Request::Read {
                    path: "left.txt".into(),
                    max_bytes: 1024,
                    start_line: None,
                    end_line: None
                }
            )
            .is_err());
        f.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn child_late_result_after_parent_steering_retains_evidence_without_integration() {
    use vcp_repository::{dirty_snapshot::CapturePolicy, worktree::WorkspaceRegistration};
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let mut f = Fixture::new(backend, 0).await;
        let child_id = f.assign(&["left.txt"]).await;
        let (thread, child_path, _) = f.start(&child_id).await;
        let change = f
            .host
            .prepare_tool(thread, patch("left.txt", "base", "prior assignment result"))
            .unwrap();
        f.host.dispatch_tool(change).unwrap();
        let transcript = f
            .host
            .open_output(thread, Channel::ChildTranscript)
            .unwrap();
        transcript
            .write(b"Completed the prior assigned edit; parent must review current applicability.")
            .unwrap();
        let transcript = transcript.finish().unwrap();
        f.host
            .stop(
                f.host
                    .control_envelope(
                        CommandId::new(),
                        child_id.clone(),
                        f.current(&child_id).revision,
                        Command::Transition {
                            next: TaskState::Paused,
                            reason: "Result ready before parent steering".into(),
                            verification: None,
                        },
                    )
                    .unwrap(),
            )
            .unwrap();
        let registration: WorkspaceRegistration =
            serde_json::from_slice(&fs::read(child_path.join(".vcp-child-owner")).unwrap())
                .unwrap();
        let source = Root::open(registration.source.clone(), &f.workspace).unwrap();
        let child_root = Root::open(registration.child.clone(), &child_path).unwrap();
        let snapshot = f
            .snapshotter
            .capture_registered(
                &child_root,
                &source,
                &registration,
                &CapturePolicy {
                    untracked: ["left.txt", "right.txt", "outside.txt"]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                    required: BTreeSet::from(["left.txt".into()]),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let packet = vcp_repository::merge::ChildPacket {
            base_fingerprint: registration.snapshot, result_fingerprint: snapshot.fingerprint,
            changed_paths: BTreeSet::from(["left.txt".into()]),
            findings: vec!["Prior assignment produced a scoped edit; this report does not authorize integration after steering.".into()],
        };
        let bytes = vcp_protocol::canonical_bytes(&packet).unwrap();
        let before = f.host.snapshot().unwrap();
        let parent = f.current(&f.config.root_task);
        let new_steering = parent.steering.next().unwrap();
        // All retained work is quiescent. Use the ordinary revision-checked
        // command; the stale child assignment must still block later integration.
        f.host.command(Command::Steer { objective: Objective {
            text: "Keep the parent source unchanged; the former edit is no longer assigned".into(),
            constraints: vec!["Do not apply results from the earlier assignment".into()],
            acceptance: vec!["Retain the old result for inspection without applying it".into()],
            source: EventId::new(), steering: new_steering,
        }}, Some(f.config.root_task.clone()), parent.revision).unwrap();
        assert_eq!(f.current(&f.config.root_task).steering, new_steering);
        let result = f
            .host
            .prepare_child_integration(f.parent, child_id.clone(), &f.snapshotter, packet)
            .await
            .unwrap();
        assert!(result.proposal.is_none());
        assert!(
            result
                .rejection
                .as_deref()
                .unwrap()
                .contains("child assignment requires current parent"),
            "{:?}",
            result.rejection
        );
        assert_eq!(f.host.read_artifact(result.packet.clone()).unwrap(), bytes);
        assert_eq!(
            f.host.read_artifact(transcript.spec.id.clone()).unwrap(),
            b"Completed the prior assigned edit; parent must review current applicability."
        );
        let state = f.host.snapshot().unwrap();
        let graph = vcp_engine::agents::graph(&state, &f.binding.scope, &f.config.root_task)
            .unwrap()
            .unwrap();
        assert_eq!(graph.children[&child_id].parent_steering, parent.steering);
        let retained = &graph.results[&child_id];
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].packet, result.packet);
        assert_eq!(retained[0].plan, result.plan);
        assert!(retained[0].effect.is_none());
        assert_eq!(
            vcp_budget::ledger(&before, &f.binding.scope).unwrap(),
            vcp_budget::ledger(&state, &f.binding.scope).unwrap()
        );
        assert_eq!(
            before
                .records
                .values()
                .filter(|r| r.collection == Collection::Effect)
                .collect::<Vec<_>>(),
            state
                .records
                .values()
                .filter(|r| r.collection == Collection::Effect)
                .collect::<Vec<_>>()
        );
        assert_eq!(fs::read(f.workspace.join("left.txt")).unwrap(), b"base\n");
        assert_eq!(
            fs::read(child_path.join("left.txt")).unwrap(),
            b"prior assignment result\n"
        );
        assert_ne!(f.current(&f.config.root_task).state, TaskState::Completed);
        assert_eq!(f.current(&child_id).state, TaskState::Paused);
        assert!(f.server.received_requests().await.unwrap().is_empty());
        f.close().await;
    }
}
