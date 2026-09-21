// SPDX-License-Identifier: Apache-2.0
//! Actual retained coding admissions feed controlled shadow evaluator requests.
//! Synthetic fixtures qualify accounting/lifecycle mechanics, never model quality.
use super::decision_shadow_peer::{Behavior, Peer};
use super::*;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use vcp_domain::policy::{Autonomy, EffectClass, Policy};
use vcp_lifecycle::foundation::coding::{allowed_tools, CodingConfig};
use vcp_models::{
    decision::{Mode, Operation, Purpose, QualifiedEvaluator},
    routing::Profile,
};
use vcp_protocol::event::EventKind;
use vcp_store::contract::key;
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

fn now() -> Timestamp {
    Timestamp::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
    )
}
fn evaluator(operation: Operation) -> QualifiedEvaluator {
    QualifiedEvaluator {
        model: match operation {
            Operation::JevDecisions => "typesafe/jev-1.13",
            Operation::ConventionalChat => "fixture/comparator",
        }
        .into(),
        provider: "fixture-provider".into(),
        served_model: "fixture-served-model".into(),
        served_provider: "FixtureProvider".into(),
        operation,
        purpose: Purpose::Routing,
        mode: Mode::Shadow,
        evidence_digest: "e".repeat(64),
        configuration_digest: "f".repeat(64),
        valid_until: Timestamp::new(now().get() + 600_000),
        require_distributions: false,
        require_confidence: false,
        deny_data_collection: true,
        require_zdr: true,
        prompt_price_per_million: "0".into(),
        output_price_per_million: "0".into(),
        request_price: "0.00001".into(),
    }
}
fn response(body: &Value) -> Value {
    let answers = if let Some(questions) = body.get("questions") {
        let mut answers = serde_json::Map::new();
        for (id, question) in questions.as_object().unwrap() {
            if question["type"] == "noul" {
                answers.insert(id.clone(), json!({"type":"noul","noul":0.9}));
                continue;
            }
            assert_eq!(question["type"], "choice");
            let choice = question["criteria"]
                .as_object()
                .unwrap()
                .keys()
                .next_back()
                .unwrap();
            answers.insert(id.clone(), json!({"type":"choice","choice":choice}));
        }
        json!({"answers":answers})
    } else {
        let schema = &body["response_format"]["json_schema"]["schema"];
        let mut answers = serde_json::Map::new();
        for (id, property) in schema["properties"].as_object().unwrap() {
            if property["type"]
                .as_array()
                .is_some_and(|types| types.contains(&json!("boolean")))
            {
                answers.insert(id.clone(), json!(true));
                continue;
            }
            answers.insert(
                id.clone(),
                property["enum"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .rfind(|v| v.is_string())
                    .unwrap()
                    .clone(),
            );
        }
        json!({"choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":Value::Object(answers).to_string()}}]})
    };
    let mut reply = answers;
    reply["id"] = "controlled-shadow-response".into();
    reply["model"] = "fixture-served-model".into();
    reply["provider"] = "FixtureProvider".into();
    reply["usage"] = serde_json::from_str(r#"{"input_tokens":10,"output_tokens":4,"prompt_tokens":10,"completion_tokens":4,"cost":0.00001}"#).unwrap();
    reply
}
struct Fixture {
    _temp: tempfile::TempDir,
    workspace: std::path::PathBuf,
    host: CanonicalHost,
    owner: vcp_lifecycle::foundation::CanonicalOwner,
    test: TestCodex,
    _server: wiremock::MockServer,
    thread: codex_protocol::ThreadId,
    config: Config,
    main_bodies: Arc<std::sync::Mutex<Vec<Value>>>,
}
impl Fixture {
    async fn new(backend: BackendKind) -> Self {
        Self::with_limits(backend, 1000, true).await
    }
    async fn with_limits(backend: BackendKind, cap: u64, permit_evaluator: bool) -> Self {
        Self::with_escalation(backend, cap, permit_evaluator, false).await
    }
    async fn with_escalation(
        backend: BackendKind,
        cap: u64,
        permit_evaluator: bool,
        escalation: bool,
    ) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        std::fs::write(workspace.join("file.txt"), "SHADOW_CANONICAL_SOURCE_MARKER").unwrap();
        std::fs::write(
            workspace.join("AGENTS.md"),
            "Project fixture instruction: SHADOW_CANONICAL_INSTRUCTION_MARKER.\n",
        )
        .unwrap();
        let mut config = config(&temp.path().join("canonical"), &workspace, backend);
        config.cap.micros = Micros::new(cap);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let binding = task(&host, &config, config.root_task.clone(), None);
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
                    automatic_effects: BTreeSet::from([
                        EffectClass::Read,
                        EffectClass::Network,
                        EffectClass::Opaque,
                    ]),
                    timeout_ceiling_ms: Units::new(120_000),
                    output_ceiling_bytes: ByteCount::new(1024 * 1024),
                },
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        let (snapshot, raw) = provider_snapshot();
        host.configure_provider(snapshot, raw).unwrap();
        let mut routing = super::routing::routing_configuration(Profile::Low, false);
        if escalation {
            routing.escalation = Some(vcp_models::escalation::Policy {
                max_transport_retries: 2,
                max_quality_switches: 1,
                max_decompositions: 0,
                max_total_attempts: 3,
                minimum_repeated_failures: 1,
                deadline: Timestamp::new(now().get() + 300_000),
            });
        }
        // Trusted configuration explicitly permits helper egress; it does not add
        // either evaluator to the main routing candidate catalog.
        if permit_evaluator {
            routing
                .policy
                .allowed_models
                .extend(["typesafe/jev-1.13".into(), "fixture/comparator".into()]);
            routing
                .policy
                .allowed_endpoints
                .insert("fixture-provider".into());
        }
        routing.policy = routing.policy.seal().unwrap();
        host.configure_routing(routing).unwrap();
        let server = start_mock_server().await;
        let main_bodies = Arc::new(std::sync::Mutex::new(vec![]));
        let observed = main_bodies.clone();
        Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let mut captured = observed.lock().unwrap();
            let index = captured.len();
            assert_eq!(body["model"], if escalation && index > 0 { "fixture/stronger" } else { "fixture/economical" });
            captured.push(body);
            let mut events = Vec::new();
            let mut output = Vec::new();
            if escalation && index == 0 {
                let call = json!({"type":"function_call","id":"missing-read","call_id":"missing-read","name":"vcp_read","arguments":"{\"path\":\"missing-file.txt\",\"max_bytes\":1024}","status":"completed"});
                events.push(json!({"type":"response.output_item.done","output_index":0,"item":call}));
                output.push(call);
            } else {
                events.push(ev_assistant_message("main-done", "Observed synthetic source."));
            }
            events.push(json!({"type":"response.completed","response":{"id":format!("shadow-main-response-{index}"),"status":"completed","output":output,"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":if escalation && index > 0 { 0.0002 } else { 0.0001 }}}}));
            ResponseTemplate::new(200).insert_header("content-type", "text/event-stream").set_body_string(sse(events))
        }).mount(&server).await;
        let mut registry = ExtensionRegistryBuilder::new();
        registry.turn_start_admission(Arc::new(host.clone()));
        registry.work_admission(Arc::new(host.clone()));
        registry.tool_contributor(Arc::new(host.clone()));
        let starter = host.clone();
        let cwd = workspace.clone();
        let test = test_codex()
            .with_extensions(Arc::new(registry.build()))
            .with_auth(codex_login::CodexAuth::from_api_key(
                "synthetic-shadow-main-key",
            ))
            .with_allowed_tools(allowed_tools())
            .with_config(move |c| {
                c.cwd = cwd.try_into().unwrap();
                configure_provider_fixture(c);
                starter
                    .lifecycle()
                    .authorize_startup(c.cwd.as_path(), None)
                    .unwrap();
            })
            .build_with_auto_env(&server)
            .await
            .unwrap();
        let thread = host.lifecycle().attach_root(test.codex.clone()).unwrap();
        host.register(thread, binding).unwrap();
        host.configure_coding(
            thread,
            CodingConfig {
                operating: "Report the observed fixture only.".into(),
                affected_paths: vec!["file.txt".into()],
                max_requests: if escalation { 2 } else { 1 },
                deadline: Timestamp::new(now().get() + 300_000),
            },
        )
        .unwrap();
        Self {
            _temp: temp,
            workspace,
            host,
            owner,
            test,
            _server: server,
            thread,
            config,
            main_bodies,
        }
    }
    async fn start(&self) {
        self.host
            .begin_coding_turn(self.thread, "Observe the synthetic fixture.".into())
            .unwrap();
        self.test
            .codex
            .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                text: "Observe the synthetic fixture.".into(),
                text_elements: vec![],
            }]))
            .await
            .unwrap();
    }
    async fn main_complete(&self) {
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if matches!(
                    self.test.codex.next_event().await.unwrap().msg,
                    EventMsg::TurnComplete(_)
                ) {
                    break;
                }
            }
        })
        .await
        .unwrap();
    }
    fn attempts(&self) -> Vec<Attempt> {
        self.host
            .snapshot()
            .unwrap()
            .records
            .values()
            .filter(|r| r.collection == Collection::Attempt)
            .map(|r| r.decode().unwrap())
            .collect()
    }
    fn decisions(&self) -> Vec<Value> {
        self.host
            .snapshot()
            .unwrap()
            .records
            .values()
            .filter(|r| r.value["document_type"] == "vcp_routing_decision_v1")
            .map(|r| r.value.clone())
            .collect()
    }
    async fn close(self) {
        let Self {
            _temp,
            host,
            owner,
            test,
            ..
        } = self;
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        drop(test);
        drop(host);
        drop(_temp);
    }
}
fn configure(f: &Fixture, peer: &Peer, operation: Operation) {
    configure_rate(f, peer, operation, 10);
}
fn configure_escalation(f: &Fixture, peer: &Peer, operation: Operation) {
    let mut configuration = configuration(peer, operation, 10);
    configuration.evaluator.purpose = Purpose::Escalation;
    configuration.evaluator.mode = Mode::Advisory;
    f.host
        .qualification_configure_decisions(configuration)
        .unwrap();
}

fn record_local_failures(f: &Fixture, output: &ArtifactId, count: usize) {
    use vcp_domain::verification::{Check, CheckOutcome, CostCertainty, Verification};
    let task: Task = f
        .host
        .snapshot()
        .unwrap()
        .record(
            Collection::Task,
            f.config.root_task.as_str(),
            &f.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    for _ in 0..count {
        f.host
            .command(
                Command::RecordVerification {
                    verification: Verification {
                        redaction: None,
                        id: VerificationId::new(),
                        scope: task.scope.clone(),
                        steering: task.steering,
                        fingerprint: task.fingerprint.clone(),
                        outputs: vec![output.clone()],
                        checks: vec![Check {
                            specification: "synthetic repeated local-shadow check".into(),
                            outcome: CheckOutcome::Failed {
                                reason: "same synthetic diagnostic".into(),
                            },
                            output: output.clone(),
                            exit_code: Some(1),
                        }],
                        unresolved_effects: vec![],
                        outstanding_issues: vec![],
                        cost: CostCertainty::Known,
                    },
                },
                Some(task.scope.task.clone()),
                task.revision,
            )
            .unwrap();
    }
}

async fn configure_local_shadow(
    f: &Fixture,
) -> (
    vcp_lifecycle::foundation::routing_state::local_stall::Fit,
    vcp_lifecycle::foundation::decision::EvidencePin,
    ArtifactId,
) {
    f.host
        .configure_decisions(vcp_lifecycle::foundation::decision::Configuration {
            mode: Mode::Shadow,
            qualification: None,
        })
        .unwrap();
    let output = f
        .host
        .capture(
            f.thread,
            Channel::Evidence,
            b"Synthetic failure for local lifecycle qualification only.".to_vec(),
        )
        .unwrap()
        .spec
        .id;
    record_local_failures(f, &output, 12);
    let until = Timestamp::new(now().get() + 1);
    let fit = f
        .host
        .fit_local_shadow(
            f.thread,
            vcp_lifecycle::foundation::routing_state::HistoryWindow { from: None, until },
            3,
        )
        .unwrap();
    let pin = f.host.install_local_shadow(fit.clone()).unwrap();
    while now() <= until {
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    record_local_failures(f, &output, 3);
    (fit, pin, output)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn local_shadow_uses_frozen_statistics_without_helper_spend_and_purges_source_copies() {
    use vcp_domain::retention_selector::{Bound, Criterion, Selector, TimeWindow, Tree};
    use vcp_lifecycle::foundation::history_retention::Request as Retention;
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let f = Fixture::with_escalation(backend, 1000, true, true).await;
        let (fit, pin, source) = configure_local_shadow(&f).await;
        let installed_bytes = f.host.read_artifact(pin.artifact.clone()).unwrap();
        f.start().await;
        f.main_complete().await;
        let attempts = f.attempts();
        assert_eq!(attempts.len(), 2);
        assert!(attempts
            .iter()
            .all(|attempt| attempt.role == RequestRole::Main));
        let baseline = f.decisions();
        let outcome = f
            .host
            .evaluate_pending_decision_shadow(f.thread)
            .await
            .unwrap();
        assert!(outcome.evaluator_attempt.is_none());
        let statistics: vcp_lifecycle::foundation::routing_state::local_stall::Outcome =
            serde_json::from_value(outcome.outcome["local_statistics"].clone()).unwrap();
        assert_eq!(statistics.fit, fit.id);
        assert!(statistics.abstention.is_none(), "{statistics:?}");
        assert!(
            statistics
                .signal
                .as_ref()
                .unwrap()
                .repeated_strategy_suspected
        );
        assert!(!statistics.serving_qualified);
        assert_eq!(outcome.artifacts.len(), 1);
        assert_eq!(
            f.host.read_artifact(pin.artifact.clone()).unwrap(),
            installed_bytes
        );
        assert_eq!(f.attempts(), attempts);
        assert_eq!(f.decisions(), baseline);
        assert_eq!(f.main_bodies.lock().unwrap().len(), 2);
        let repeated = f
            .host
            .evaluate_pending_decision_shadow(f.thread)
            .await
            .unwrap();
        assert!(repeated.evaluator_attempt.is_none());
        assert!(repeated.artifacts.is_empty());

        let retained = f.host.snapshot().unwrap();
        let fit_key = key(Collection::Artifact, pin.artifact.as_str());
        for verification in &fit.source_verifications {
            assert!(retained.records[&fit_key]
                .references
                .contains(&key(Collection::Verification, verification.as_str())));
        }
        let local_key = key(Collection::Artifact, outcome.artifacts[0].as_str());
        assert!(retained.records[&local_key].references.contains(&fit_key));
        let source_event = retained
            .events
            .iter()
            .find(|event| {
                event.event.kind == EventKind::ArtifactAttached
                    && event.event.artifacts.contains(&source)
            })
            .unwrap();
        let instant = retention_instant(source_event.event.timestamp);
        let selector = Selector {
            schema_version: 1,
            tree: Tree::All(vec![
                Tree::Match(Criterion::Event("artifact_attached".into())),
                Tree::Match(Criterion::Date(TimeWindow {
                    lower: Some(Bound {
                        instant: instant.clone(),
                        inclusive: true,
                    }),
                    upper: Some(Bound {
                        instant,
                        inclusive: true,
                    }),
                })),
            ]),
        };
        let task: Task = retained
            .record(
                Collection::Task,
                f.config.root_task.as_str(),
                &f.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        f.host
            .command(
                Command::Transition {
                    next: TaskState::Failed,
                    reason: "terminal local-shadow purge fixture".into(),
                    verification: None,
                },
                Some(task.scope.task.clone()),
                task.revision,
            )
            .unwrap();
        for row in retained
            .records
            .values()
            .filter(|row| row.collection == Collection::Turn)
        {
            let turn: Turn = row.decode().unwrap();
            if !matches!(
                turn.state,
                TurnState::Completed | TurnState::Failed | TurnState::Cancelled
            ) {
                f.host
                    .command(
                        Command::AdvanceTurn {
                            id: turn.id,
                            next: TurnState::Failed,
                            reason: "close local-shadow fixture turn".into(),
                        },
                        Some(turn.scope.task),
                        turn.revision,
                    )
                    .unwrap();
            }
        }
        let preview = f
            .host
            .history_retention(Retention::Preview {
                selector,
                action: vcp_memory::retention::Action::Purge,
            })
            .unwrap();
        assert_eq!(preview["protected_count"], 0, "{preview}");
        assert_eq!(preview["dependent_truncated"], false);
        let dependent = preview["dependent"].as_array().unwrap();
        assert!(dependent.contains(&json!({"kind":"record","id":fit_key})));
        assert!(dependent.contains(&json!({"kind":"record","id":local_key})));
        let applied = f
            .host
            .history_retention(Retention::Apply {
                preview: preview["id"].as_str().unwrap().into(),
            })
            .unwrap();
        assert!(f.host.select_local_shadow(pin.clone()).is_err());
        assert!(f.host.read_artifact(pin.artifact.clone()).is_err());
        assert!(f.host.read_artifact(outcome.artifacts[0].clone()).is_err());
        let cleaned = f
            .host
            .history_retention(Retention::Cleanup {
                receipt: applied["id"].as_str().unwrap().into(),
            })
            .unwrap();
        assert_eq!(cleaned["rewrite_complete"], true);
        let purged = f.host.snapshot().unwrap();
        for artifact in [&pin.artifact, &outcome.artifacts[0]] {
            let descriptor: ArtifactDescriptor = purged.records
                [&key(Collection::Artifact, artifact.as_str())]
                .decode()
                .unwrap();
            assert_eq!(descriptor.state, CaptureState::Purged);
        }
        f.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn local_shadow_rejects_paused_or_steered_seed_without_recording_statistics() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for pause in [false, true] {
            let f = Fixture::with_escalation(backend, 1000, true, true).await;
            configure_local_shadow(&f).await;
            f.start().await;
            f.main_complete().await;
            let baseline = f.decisions();
            let attempts = f.attempts();
            let task: Task = f
                .host
                .snapshot()
                .unwrap()
                .record(
                    Collection::Task,
                    f.config.root_task.as_str(),
                    &f.config.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            let command = if pause {
                Command::Transition {
                    next: TaskState::Paused,
                    reason: "pause local shadow before evaluation".into(),
                    verification: None,
                }
            } else {
                let mut objective = task.objectives.last().unwrap().clone();
                objective.text = "Changed local-shadow task objective".into();
                objective.source = EventId::new();
                objective.steering = task.steering.next().unwrap();
                Command::Steer { objective }
            };
            f.host
                .command(command, Some(task.scope.task), task.revision)
                .unwrap();
            let outcome = f
                .host
                .evaluate_pending_decision_shadow(f.thread)
                .await
                .unwrap();
            assert!(outcome.evaluator_attempt.is_none());
            assert!(outcome.artifacts.is_empty());
            assert_eq!(f.attempts(), attempts);
            assert_eq!(f.decisions(), baseline);
            assert!(f
                .host
                .snapshot()
                .unwrap()
                .records
                .values()
                .filter(|row| row.collection == Collection::Artifact)
                .all(|row| row.value["spec"]["schema"] != "vcp-local-stall-shadow-result-v1"));
            f.close().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn local_shadow_pause_after_compute_prevents_publication_and_keeps_owner_responsive() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let f = Fixture::with_escalation(backend, 1000, true, true).await;
        configure_local_shadow(&f).await;
        f.start().await;
        f.main_complete().await;
        let baseline = f.decisions();
        let attempts = f.attempts();
        let arrived = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        f.host
            .qualification_block_local_after_compute(arrived.clone(), release.clone())
            .unwrap();
        let host = f.host.clone();
        let thread = f.thread;
        let waiter =
            tokio::spawn(async move { host.evaluate_pending_decision_shadow(thread).await });
        tokio::time::timeout(Duration::from_secs(10), arrived.notified())
            .await
            .unwrap();
        let task: Task = f
            .host
            .snapshot()
            .unwrap()
            .record(
                Collection::Task,
                f.config.root_task.as_str(),
                &f.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let host = f.host.clone();
        let pause = tokio::task::spawn_blocking(move || {
            host.command(
                Command::Transition {
                    next: TaskState::Paused,
                    reason: "pause after local computation before publication".into(),
                    verification: None,
                },
                Some(task.scope.task),
                task.revision,
            )
        });
        let paused = tokio::time::timeout(Duration::from_secs(5), pause).await;
        release.notify_one();
        paused.unwrap().unwrap().unwrap();
        let outcome = tokio::time::timeout(Duration::from_secs(5), waiter)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(outcome.outcome.get("local_statistics").is_none());
        assert_eq!(outcome.outcome["local_shadow"]["outcome"], "skipped");
        assert_eq!(outcome.outcome["local_shadow"]["historical_only"], true);
        assert_eq!(outcome.artifacts.len(), 1);
        let receipt: Value =
            serde_json::from_slice(&f.host.read_artifact(outcome.artifacts[0].clone()).unwrap())
                .unwrap();
        assert!(receipt.get("local_statistics").is_none());
        assert_eq!(receipt["baseline_changed"], false);
        assert!(outcome.evaluator_attempt.is_none());
        assert_eq!(f.attempts(), attempts);
        assert_eq!(f.decisions(), baseline);
        assert!(f
            .host
            .snapshot()
            .unwrap()
            .records
            .values()
            .filter(|row| row.collection == Collection::Artifact)
            .all(|row| row.value["spec"]["schema"] != "vcp-local-stall-shadow-result-v1"));
        assert_eq!(
            f.host
                .lifecycle()
                .inspect(f.thread)
                .unwrap()
                .unresolved_work,
            0
        );
        f.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn local_shadow_reopen_requires_explicit_selection_and_does_not_replay_admitted_main() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let f = Fixture::with_escalation(backend, 1000, true, true).await;
        let (_, pin, _) = configure_local_shadow(&f).await;
        f.start().await;
        f.main_complete().await;
        let outcome = f
            .host
            .evaluate_pending_decision_shadow(f.thread)
            .await
            .unwrap();
        assert_eq!(outcome.artifacts.len(), 1);
        let attempts = f.attempts();
        let binding = ThreadBinding {
            scope: attempts[0].scope.clone(),
            agent: attempts[0].agent.clone(),
            role: RequestRole::Main,
        };
        let Fixture {
            _temp,
            host,
            owner,
            test,
            config,
            thread,
            ..
        } = f;
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        drop(test);
        drop(host);
        let (restored, owner) = CanonicalHost::open(config.clone()).unwrap();
        restored.register(thread, binding).unwrap();
        assert!(!restored.decision_shadow_pending(thread).unwrap());
        assert!(restored
            .evaluate_pending_decision_shadow(thread)
            .await
            .unwrap()
            .artifacts
            .is_empty());
        restored
            .configure_decisions(vcp_lifecycle::foundation::decision::Configuration {
                mode: Mode::Shadow,
                qualification: None,
            })
            .unwrap();
        restored.select_local_shadow(pin).unwrap();
        assert!(!restored.decision_shadow_pending(thread).unwrap());
        assert!(restored
            .evaluate_pending_decision_shadow(thread)
            .await
            .unwrap()
            .artifacts
            .is_empty());
        let snapshot = restored.snapshot().unwrap();
        let retained: Vec<Attempt> = snapshot
            .records
            .values()
            .filter(|row| row.collection == Collection::Attempt)
            .map(|row| row.decode().unwrap())
            .collect();
        assert_eq!(retained, attempts);
        assert_eq!(
            snapshot
                .records
                .values()
                .filter(|row| row.collection == Collection::Artifact
                    && row.value["spec"]["schema"] == "vcp-local-stall-shadow-result-v1")
                .count(),
            1
        );
        owner.close().await.unwrap();
        drop(restored);
        drop(_temp);
    }
}

fn advisory_document(f: &Fixture, kind: &str) -> Value {
    let records: Vec<_> = f
        .host
        .snapshot()
        .unwrap()
        .records
        .values()
        .filter(|record| record.value["document_type"] == kind)
        .map(|record| record.value.clone())
        .collect();
    assert_eq!(records.len(), 1, "{kind}: {records:?}");
    records.into_iter().next().unwrap()
}

fn retention_instant(timestamp: Timestamp) -> vcp_domain::retention_selector::InstantSpec {
    let mut days = timestamp.get() / 86_400_000;
    let mut year = 1970u64;
    let leap = |year| year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    while days >= if leap(year) { 366 } else { 365 } {
        days -= if leap(year) { 366 } else { 365 };
        year += 1;
    }
    let months = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 0;
    while days >= months[month] {
        days -= months[month];
        month += 1;
    }
    let millis = timestamp.get() % 86_400_000;
    let text = format!(
        "{year:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        month + 1,
        days + 1,
        millis / 3_600_000,
        millis / 60_000 % 60,
        millis / 1000 % 60,
        millis % 1000
    );
    let instant = vcp_domain::retention_selector::InstantSpec::parse(&text, None).unwrap();
    assert_eq!(instant.utc, timestamp);
    instant
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn escalation_shadow_requires_advisory_qualification_and_admitted_escalation() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for escalation in [false, true] {
            let f = Fixture::with_escalation(backend, 1000, true, escalation).await;
            let peer = Peer::start(Behavior::Reply, response).await;
            let mut configuration = configuration(&peer, Operation::JevDecisions, 10);
            configuration.evaluator.purpose = Purpose::Escalation;
            configuration.evaluator.mode = if escalation {
                Mode::Shadow
            } else {
                Mode::Advisory
            };
            f.host
                .qualification_configure_decisions(configuration)
                .unwrap();
            f.start().await;
            f.main_complete().await;
            let attempts = f.attempts();
            assert_eq!(attempts.len(), if escalation { 2 } else { 1 });
            let baseline = f.decisions();
            let outcome = f
                .host
                .evaluate_pending_decision_shadow(f.thread)
                .await
                .unwrap();
            assert!(outcome.evaluator_attempt.is_none(), "{}", outcome.outcome);
            assert!(peer.observations().is_empty());
            assert_eq!(f.attempts(), attempts);
            assert_eq!(f.decisions(), baseline);
            assert!(f
                .host
                .snapshot()
                .unwrap()
                .records
                .values()
                .all(
                    |record| record.value["document_type"] != "vcp_escalation_advisory_request_v1"
                ));
            f.close().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn escalation_shadow_retains_canonical_advice_without_changing_admitted_handoff() {
    use vcp_domain::retention_selector::{Bound, Criterion, Selector, TimeWindow, Tree};
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for operation in [Operation::JevDecisions, Operation::ConventionalChat] {
            let f = Fixture::with_escalation(backend, 1000, true, true).await;
            let peer = Peer::start(Behavior::Reply, response).await;
            configure_escalation(&f, &peer, operation);
            f.start().await;
            f.main_complete().await;
            assert_eq!(f.main_bodies.lock().unwrap().len(), 2);
            let baseline = f.decisions();
            tokio::time::sleep(Duration::from_millis(2)).await;
            let outcome = f
                .host
                .evaluate_pending_decision_shadow(f.thread)
                .await
                .unwrap();
            assert_eq!(
                outcome.outcome["result"]["outcome"], "advice",
                "{}",
                outcome.outcome
            );
            assert_eq!(
                outcome.outcome["result"]["answers"]["next_action"]["choice"],
                "stop"
            );
            let attempts = f.attempts();
            assert_eq!(attempts.len(), 3);
            let helper = attempts
                .iter()
                .find(|attempt| attempt.role == RequestRole::Helper)
                .unwrap();
            assert_eq!(helper.phase, ReservationState::Settled);
            assert_eq!(helper.charged, Micros::new(10));
            assert_eq!(outcome.evaluator_attempt.as_ref(), Some(&helper.id));
            assert_eq!(f.decisions(), baseline);
            let request = advisory_document(&f, "vcp_escalation_advisory_request_v1");
            assert_eq!(request["request"]["purpose"], "escalation");
            assert_eq!(request["request"]["state"]["checks_complete"], false);
            assert_eq!(request["request"]["state"]["required_review"], true);
            assert_eq!(request["request"]["state"]["hard_failure"], true);
            assert!(request["request"]["state"]["observations"][0]["summary"]
                .as_str()
                .unwrap()
                .contains("missing-file.txt"));
            let result = advisory_document(&f, "vcp_escalation_advisory_result_v1");
            assert_eq!(result["request_id"], request["id"]);
            assert_eq!(result["disposition"], "accepted_current");
            let schedule = advisory_document(&f, "vcp_escalation_advisory_schedule_v1");
            assert_eq!(schedule["state"]["state"], "completed");
            let accounting = advisory_document(&f, "vcp_escalation_advisory_accounting_v1");
            assert_eq!(
                accounting["attempt"],
                serde_json::to_value(&helper.id).unwrap()
            );
            let evidence = ArtifactId::parse(
                request["request"]["state"]["trigger"]["evidence"][0]
                    .as_str()
                    .unwrap(),
            )
            .unwrap();
            let retained = f.host.snapshot().unwrap();
            let source_event = retained
                .events
                .iter()
                .find(|event| {
                    event.event.kind == EventKind::ArtifactAttached
                        && event.event.artifacts.contains(&evidence)
                })
                .unwrap();
            let instant = retention_instant(source_event.event.timestamp);
            let preview = f
                .host
                .history_retention(
                    vcp_lifecycle::foundation::history_retention::Request::Preview {
                        selector: Selector {
                            schema_version: 1,
                            tree: Tree::All(vec![
                                Tree::Match(Criterion::Event("artifact_attached".into())),
                                Tree::Match(Criterion::Date(TimeWindow {
                                    lower: Some(Bound {
                                        instant: instant.clone(),
                                        inclusive: true,
                                    }),
                                    upper: Some(Bound {
                                        instant,
                                        inclusive: true,
                                    }),
                                })),
                            ]),
                        },
                        action: vcp_memory::retention::Action::Purge,
                    },
                )
                .unwrap();
            let selected = preview["selected"].as_array().unwrap();
            let dependent = preview["dependent"].as_array().unwrap();
            assert_eq!(preview["selected_truncated"], false);
            assert_eq!(preview["dependent_truncated"], false);
            let source_target =
                json!({"kind":"record","id":key(Collection::Artifact, evidence.as_str())});
            let request_key = key(Collection::Projection, request["id"].as_str().unwrap());
            assert!(retained.records[&request_key]
                .references
                .contains(&key(Collection::Artifact, evidence.as_str())));
            assert!(
                selected.contains(&source_target) || dependent.contains(&source_target),
                "source missing: {preview}"
            );
            for artifact in &outcome.artifacts {
                let attached = retained
                    .events
                    .iter()
                    // Request capture is attached by the budget admission
                    // event; response and receipt use ArtifactAttached.
                    .find(|event| event.event.artifacts.contains(artifact))
                    .unwrap();
                assert!(attached.event.timestamp > source_event.event.timestamp);
                assert!(
                    retained.records[&key(Collection::Artifact, artifact.as_str())]
                        .references
                        .contains(&request_key)
                );
                let target =
                    json!({"kind":"record","id":key(Collection::Artifact, artifact.as_str())});
                assert!(dependent.contains(&target), "advisory copied artifact missing from dependency closure: {artifact}: {preview}");
            }
            assert_eq!(peer.observations().len(), 1);
            assert!(f
                .host
                .evaluate_pending_decision_shadow(f.thread)
                .await
                .unwrap()
                .evaluator_attempt
                .is_none());
            assert_eq!(peer.observations().len(), 1);
            assert_eq!(f.attempts().len(), 3);
            assert_eq!(
                f.host
                    .lifecycle()
                    .inspect(f.thread)
                    .unwrap()
                    .unresolved_work,
                0
            );
            let task: Task = f
                .host
                .snapshot()
                .unwrap()
                .record(
                    Collection::Task,
                    f.config.root_task.as_str(),
                    &f.config.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            f.host
                .command(
                    Command::Transition {
                        next: TaskState::Failed,
                        reason: "terminal fixture permits exact retained-evidence purge".into(),
                        verification: None,
                    },
                    Some(f.config.root_task.clone()),
                    task.revision,
                )
                .unwrap();
            let turns: Vec<Turn> = f
                .host
                .snapshot()
                .unwrap()
                .records
                .values()
                .filter(|record| record.collection == Collection::Turn)
                .map(|record| record.decode().unwrap())
                .collect();
            for turn in turns {
                if !matches!(
                    turn.state,
                    TurnState::Completed | TurnState::Failed | TurnState::Cancelled
                ) {
                    f.host
                        .command(
                            Command::AdvanceTurn {
                                id: turn.id,
                                next: TurnState::Failed,
                                reason: "terminal fixture closes canonical turn before purge"
                                    .into(),
                            },
                            Some(turn.scope.task),
                            turn.revision,
                        )
                        .unwrap();
                }
            }
            let purge = f
                .host
                .history_retention(
                    vcp_lifecycle::foundation::history_retention::Request::Preview {
                        selector: serde_json::from_value(preview["selector"].clone()).unwrap(),
                        action: vcp_memory::retention::Action::Purge,
                    },
                )
                .unwrap();
            assert_eq!(purge["protected_count"], 0, "{purge}");
            let applied = f
                .host
                .history_retention(
                    vcp_lifecycle::foundation::history_retention::Request::Apply {
                        preview: purge["id"].as_str().unwrap().into(),
                    },
                )
                .unwrap();
            assert_eq!(applied["logical_unavailable"], true);
            let cleaned = f
                .host
                .history_retention(
                    vcp_lifecycle::foundation::history_retention::Request::Cleanup {
                        receipt: applied["id"].as_str().unwrap().into(),
                    },
                )
                .unwrap();
            assert_eq!(cleaned["rewrite_complete"], true, "{cleaned}");
            let purged = f.host.snapshot().unwrap();
            for original in [&request, &result, &schedule, &accounting] {
                let record =
                    &purged.records[&key(Collection::Projection, original["id"].as_str().unwrap())];
                let tombstone: vcp_domain::redaction::RedactedAdvisory = record.decode().unwrap();
                tombstone.validate().unwrap();
                assert_eq!(
                    record.value["document_type"],
                    "vcp_escalation_redacted_advisory_v1"
                );
                assert!(record.value.get("request").is_none());
                assert!(record.value.get("outcome").is_none());
                assert!(!record.value.to_string().contains("missing-file.txt"));
            }
            for artifact in &outcome.artifacts {
                let descriptor: ArtifactDescriptor = purged.records
                    [&key(Collection::Artifact, artifact.as_str())]
                    .decode()
                    .unwrap();
                assert_eq!(descriptor.state, CaptureState::Purged);
                let page = f
                    .host
                    .inspect(vcp_audit::inspection::InspectionQuery {
                        id: artifact.to_string(),
                        view: vcp_audit::inspection::View::Outputs,
                        limit: 1,
                        cursor: None,
                        range: Some(vcp_audit::inspection::RangeRequest {
                            offset: 0,
                            length: 1024,
                        }),
                    })
                    .unwrap();
                assert!(page.items.is_empty());
                assert!(page.gaps.iter().any(|gap| gap["visibility"] == "pruned"));
            }
            f.close().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn escalation_shadow_late_response_settles_usage_and_retains_historical_advice() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::with_escalation(backend, 1000, true, true).await;
        let peer = Peer::start(Behavior::Delayed, response).await;
        configure_escalation(&f, &peer, Operation::JevDecisions);
        f.start().await;
        f.main_complete().await;
        let baseline = f.decisions();
        let host = f.host.clone();
        let thread = f.thread;
        let waiter =
            tokio::spawn(async move { host.evaluate_pending_decision_shadow(thread).await });
        peer.wait_request().await;
        std::fs::write(
            f.workspace.join("AGENTS.md"),
            "Source changed after evaluator received request.",
        )
        .unwrap();
        peer.release();
        let outcome = tokio::time::timeout(Duration::from_secs(10), waiter)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(outcome.outcome["outcome"], "recorded");
        let helper = f
            .attempts()
            .into_iter()
            .find(|attempt| attempt.role == RequestRole::Helper)
            .unwrap();
        assert_eq!(helper.phase, ReservationState::Settled);
        assert_eq!(helper.charged, Micros::new(10));
        let result = advisory_document(&f, "vcp_escalation_advisory_result_v1");
        assert_eq!(result["disposition"], "historical_stale");
        assert_eq!(result["outcome"]["outcome"], "advice");
        assert_eq!(
            advisory_document(&f, "vcp_escalation_advisory_schedule_v1")["state"]["state"],
            "cancelled"
        );
        assert_eq!(f.decisions(), baseline);
        assert!(f
            .host
            .evaluate_pending_decision_shadow(f.thread)
            .await
            .unwrap()
            .evaluator_attempt
            .is_none());
        assert_eq!(peer.observations().len(), 1);
        assert_eq!(f.attempts().len(), 3);
        assert_eq!(
            f.host
                .lifecycle()
                .inspect(f.thread)
                .unwrap()
                .unresolved_work,
            0
        );
        f.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn escalation_shadow_pause_before_payload_and_interruption_close_claim_without_replay() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for interrupt in [false, true] {
            let f = Fixture::with_escalation(backend, 1000, true, true).await;
            let peer = Peer::start(
                if interrupt {
                    Behavior::Delayed
                } else {
                    Behavior::Reply
                },
                response,
            )
            .await;
            configure_escalation(&f, &peer, Operation::JevDecisions);
            let arrived = Arc::new(tokio::sync::Notify::new());
            let release = Arc::new(tokio::sync::Notify::new());
            if !interrupt {
                f.host
                    .qualification_block_decision_after_tls(arrived.clone(), release.clone())
                    .unwrap();
            }
            f.start().await;
            f.main_complete().await;
            let baseline = f.decisions();
            let host = f.host.clone();
            let thread = f.thread;
            let waiter =
                tokio::spawn(async move { host.evaluate_pending_decision_shadow(thread).await });
            if interrupt {
                peer.wait_request().await;
                waiter.abort();
                assert!(waiter.await.unwrap_err().is_cancelled());
            } else {
                tokio::time::timeout(Duration::from_secs(10), arrived.notified())
                    .await
                    .unwrap();
                let task: Task = f
                    .host
                    .snapshot()
                    .unwrap()
                    .record(
                        Collection::Task,
                        f.config.root_task.as_str(),
                        &f.config.workspace,
                    )
                    .unwrap()
                    .decode()
                    .unwrap();
                let command = f
                    .host
                    .control_envelope(
                        CommandId::new(),
                        f.config.root_task.clone(),
                        task.revision,
                        Command::Transition {
                            next: TaskState::Paused,
                            reason: "pause advisory before payload".into(),
                            verification: None,
                        },
                    )
                    .unwrap();
                f.host.stop(command).unwrap();
                release.notify_one();
                let outcome = tokio::time::timeout(Duration::from_secs(5), waiter)
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();
                assert_eq!(outcome.outcome["outcome"], "unknown");
                assert!(peer.observations().is_empty());
                let result = advisory_document(&f, "vcp_escalation_advisory_result_v1");
                assert_eq!(result["disposition"], "historical_stale");
            }
            let helper = f
                .attempts()
                .into_iter()
                .find(|attempt| attempt.role == RequestRole::Helper)
                .unwrap();
            assert_eq!(helper.phase, ReservationState::ReconciliationPending);
            assert_eq!(
                advisory_document(&f, "vcp_escalation_advisory_schedule_v1")["state"]["state"],
                "cancelled"
            );
            assert_eq!(f.decisions(), baseline);
            assert!(f
                .host
                .evaluate_pending_decision_shadow(f.thread)
                .await
                .unwrap()
                .evaluator_attempt
                .is_none());
            assert_eq!(peer.observations().len(), usize::from(interrupt));
            assert_eq!(f.attempts().len(), 3);
            assert_eq!(
                f.host
                    .lifecycle()
                    .inspect(f.thread)
                    .unwrap()
                    .unresolved_work,
                0
            );
            peer.release();
            if interrupt {
                let schedule = advisory_document(&f, "vcp_escalation_advisory_schedule_v1");
                let accounting = advisory_document(&f, "vcp_escalation_advisory_accounting_v1");
                let Fixture {
                    _temp,
                    host,
                    owner,
                    test,
                    config,
                    thread,
                    ..
                } = f;
                owner.close().await.unwrap();
                test.codex.shutdown_and_wait().await.unwrap();
                drop(test);
                drop(host);
                let (restored, owner) = CanonicalHost::open(config.clone()).unwrap();
                let snapshot = restored.snapshot().unwrap();
                let retained: Attempt = snapshot
                    .record(Collection::Attempt, helper.id.as_str(), &config.workspace)
                    .unwrap()
                    .decode()
                    .unwrap();
                assert_eq!(retained, helper);
                assert_eq!(
                    snapshot.records
                        [&key(Collection::Projection, schedule["id"].as_str().unwrap())]
                        .value,
                    schedule
                );
                assert_eq!(
                    snapshot.records
                        [&key(Collection::Projection, accounting["id"].as_str().unwrap())]
                        .value,
                    accounting
                );
                restored
                    .register(
                        thread,
                        ThreadBinding {
                            scope: helper.scope.clone(),
                            agent: helper.agent.clone(),
                            role: RequestRole::Main,
                        },
                    )
                    .unwrap();
                assert!(!restored.decision_shadow_pending(thread).unwrap());
                assert!(restored
                    .evaluate_pending_decision_shadow(thread)
                    .await
                    .unwrap()
                    .evaluator_attempt
                    .is_none());
                assert_eq!(peer.observations().len(), 1);
                owner.close().await.unwrap();
                drop(restored);
                drop(_temp);
            } else {
                f.close().await;
            }
        }
    }
}
fn configure_rate(f: &Fixture, peer: &Peer, operation: Operation, charge: u64) {
    f.host
        .qualification_configure_decisions(configuration(peer, operation, charge))
        .unwrap();
}
fn configuration(
    peer: &Peer,
    operation: Operation,
    charge: u64,
) -> vcp_lifecycle::foundation::decision::FixtureConfiguration {
    let mut evaluator = evaluator(operation);
    evaluator.request_price = format!("0.{charge:06}");
    let price = PriceSnapshot {
        id: "c".repeat(64),
        provider: evaluator.provider.clone(),
        model: evaluator.model.clone(),
        currency: "USD".to_string().try_into().unwrap(),
        capability: evaluator.configuration_digest.clone(),
        valid_until: evaluator.valid_until,
        rates: [
            ChargeCategory::Input,
            ChargeCategory::Output,
            ChargeCategory::CacheRead,
            ChargeCategory::CacheWrite,
            ChargeCategory::Request,
            ChargeCategory::ProviderTool,
        ]
        .into_iter()
        .map(|category| {
            (
                category,
                Rate {
                    micros: Micros::new(if category == ChargeCategory::Request {
                        charge
                    } else {
                        0
                    }),
                    per_units: Units::new(1),
                },
            )
        })
        .collect(),
    };
    vcp_lifecycle::foundation::decision::FixtureConfiguration {
        evaluator,
        price,
        input_ceiling: Units::new(200_000),
        output_ceiling: Units::new(1024),
        endpoint: peer.endpoint(),
        root_certificate: peer.root_certificate(),
        credential: vcp_lifecycle::foundation::decision::CredentialMaterial::bearer(
            "synthetic-shadow-credential".into(),
        )
        .unwrap(),
    }
}
async fn pending(f: &Fixture) {
    tokio::time::timeout(Duration::from_secs(15), async {
        while !f.host.decision_shadow_pending(f.thread).unwrap() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qualification_capture_checks_owner_added_fields_before_persistence() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend).await;
        let peer = Peer::start(Behavior::Reply, response).await;
        let before = f.host.snapshot().unwrap();
        let mut setup = configuration(&peer, Operation::ConventionalChat, 10);
        let secret = f.config.actor.to_string();
        setup.credential =
            vcp_lifecycle::foundation::decision::CredentialMaterial::bearer(secret.clone())
                .unwrap();
        let error = f.host.qualification_configure_decisions(setup).unwrap_err();
        assert!(!error.contains(&secret));
        for (key, row) in &f.host.snapshot().unwrap().records {
            if row.collection == Collection::Artifact && !before.records.contains_key(key) {
                let artifact: ArtifactDescriptor = row.decode().unwrap();
                let bytes = f.host.read_artifact(artifact.spec.id).unwrap();
                assert!(!String::from_utf8_lossy(&bytes).contains(&secret));
            }
        }
        assert!(peer.observations().is_empty());
        assert_eq!(peer.handshakes(), 0);
        f.close().await;
    }
}
fn assert_wire(f: &Fixture, peer: &Peer, attempt: &Attempt, operation: Operation) {
    let observations = peer.observations();
    assert_eq!(observations.len(), 1);
    let wire = &observations[0];
    assert!(wire.authorization_matches);
    assert_eq!(
        wire.path,
        match operation {
            Operation::JevDecisions => "/api/alpha/decisions",
            Operation::ConventionalChat => "/api/v1/chat/completions",
        }
    );
    let body: Value = serde_json::from_slice(&wire.body).unwrap();
    assert_eq!(body["provider"]["allow_fallbacks"], false);
    assert_eq!(body["provider"]["data_collection"], "deny");
    assert_eq!(body["provider"]["zdr"], true);
    if operation == Operation::JevDecisions {
        assert!(body.get("max_tokens").is_none());
        assert!(body.get("max_output_tokens").is_none());
        assert!(body.get("questions").is_some());
    } else {
        assert_eq!(body["max_tokens"], 1024);
        assert_eq!(body["stream"], false);
    }
    let captured = f.host.read_artifact(attempt.request.clone()).unwrap();
    let request: Value = serde_json::from_slice(&captured).unwrap();
    assert_eq!(
        vcp_protocol::canonical_bytes(&request["body"]).unwrap(),
        wire.body,
        "captured closed body must equal actual wire bytes"
    );
    assert_eq!(
        request["body_digest"],
        vcp_protocol::digest_bytes(&wire.body)
    );
    assert_eq!(
        attempt.request_digest,
        vcp_protocol::digest_bytes(&captured)
    );
    assert!(attempt.send_intent.is_some());
    for row in f
        .host
        .snapshot()
        .unwrap()
        .records
        .values()
        .filter(|r| r.collection == Collection::Artifact)
    {
        let artifact: ArtifactDescriptor = row.decode().unwrap();
        let bytes = f.host.read_artifact(artifact.spec.id).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("synthetic-shadow-credential"));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coding_routing_seed_admits_separate_native_and_comparator_shadow_without_changing_baseline(
) {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for operation in [Operation::JevDecisions, Operation::ConventionalChat] {
            let f = Fixture::new(backend).await;
            let peer = Peer::start(Behavior::Reply, response).await;
            configure(&f, &peer, operation);
            f.start().await;
            pending(&f).await;
            let baseline = f.decisions();
            assert_eq!(baseline.len(), 1);
            let outcome = f
                .host
                .evaluate_pending_routing_shadow(f.thread)
                .await
                .unwrap();
            assert_eq!(
                outcome.outcome["result"]["outcome"], "advice",
                "{}",
                outcome.outcome
            );
            assert_eq!(
                outcome.outcome["result"]["answers"]["route"]["choice"], "candidate-1",
                "scripted advice deliberately prefers the other eligible route"
            );
            f.main_complete().await;
            let attempts = f.attempts();
            assert_eq!(attempts.len(), 2);
            let main = attempts
                .iter()
                .find(|a| a.role == RequestRole::Main)
                .unwrap();
            let helper = attempts
                .iter()
                .find(|a| a.role == RequestRole::Helper)
                .unwrap();
            assert_eq!(outcome.main_attempt.as_ref(), Some(&main.id));
            assert_eq!(outcome.evaluator_attempt.as_ref(), Some(&helper.id));
            assert_ne!(main.reservation, helper.reservation);
            assert_eq!(main.quote.price.model, "fixture/economical");
            assert_eq!(main.phase, ReservationState::Settled);
            assert_eq!(helper.phase, ReservationState::Settled);
            assert_eq!(helper.charged, Micros::new(10));
            assert_eq!(helper.quote.amount.micros, Micros::new(10));
            assert_eq!(f.decisions(), baseline);
            assert_eq!(f.main_bodies.lock().unwrap().len(), 1);
            assert_wire(&f, &peer, helper, operation);
            assert_eq!(
                f.host
                    .lifecycle()
                    .inspect(f.thread)
                    .unwrap()
                    .unresolved_work,
                0
            );
            let repeated = f
                .host
                .evaluate_pending_routing_shadow(f.thread)
                .await
                .unwrap();
            assert!(repeated.evaluator_attempt.is_none());
            assert_eq!(peer.observations().len(), 1);
            assert_eq!(f.attempts().len(), 2);
            f.close().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disabled_deterministic_and_unqualified_shadow_do_not_create_remote_work() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for mode in [Mode::Disabled, Mode::Deterministic, Mode::Shadow] {
            let f = Fixture::new(backend).await;
            let peer = Peer::start(Behavior::Reply, response).await;
            f.host
                .configure_decisions(vcp_lifecycle::foundation::decision::Configuration {
                    mode,
                    qualification: None,
                })
                .unwrap();
            f.start().await;
            f.main_complete().await;
            let before = f.host.snapshot().unwrap();
            let attempts = f.attempts();
            let baseline = f.decisions();
            let observed = f
                .host
                .evaluate_pending_routing_shadow(f.thread)
                .await
                .unwrap();
            assert_eq!(observed.outcome["outcome"], "baseline");
            assert!(observed.evaluator_attempt.is_none());
            assert!(observed.artifacts.is_empty());
            assert_eq!(f.attempts(), attempts);
            assert_eq!(f.decisions(), baseline);
            assert_eq!(f.host.snapshot().unwrap().records, before.records);
            assert!(peer.observations().is_empty());
            assert_eq!(peer.handshakes(), 0);
            assert_eq!(
                f.host
                    .lifecycle()
                    .inspect(f.thread)
                    .unwrap()
                    .unresolved_work,
                0
            );
            f.close().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn observed_reply_without_money_keeps_helper_liability_and_never_replays() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend).await;
        let peer = Peer::start(Behavior::UnknownCost, response).await;
        configure(&f, &peer, Operation::JevDecisions);
        f.start().await;
        pending(&f).await;
        let baseline = f.decisions();
        let outcome = f
            .host
            .evaluate_pending_routing_shadow(f.thread)
            .await
            .unwrap();
        f.main_complete().await;
        let helper = f
            .attempts()
            .into_iter()
            .find(|a| a.role == RequestRole::Helper)
            .unwrap();
        assert_eq!(outcome.evaluator_attempt.as_ref(), Some(&helper.id));
        assert_eq!(helper.phase, ReservationState::ReconciliationPending);
        assert_eq!(helper.charged, Micros::ZERO);
        assert_eq!(f.decisions(), baseline);
        assert_wire(&f, &peer, &helper, Operation::JevDecisions);
        assert_eq!(
            f.host
                .lifecycle()
                .inspect(f.thread)
                .unwrap()
                .unresolved_work,
            0
        );
        let again = f
            .host
            .evaluate_pending_routing_shadow(f.thread)
            .await
            .unwrap();
        assert!(again.evaluator_attempt.is_none());
        assert_eq!(peer.observations().len(), 1);
        f.close().await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn delayed_shadow_keeps_owner_commands_responsive_and_drains_model_work() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend).await;
        let peer = Peer::start(Behavior::Delayed, response).await;
        configure(&f, &peer, Operation::ConventionalChat);
        f.start().await;
        pending(&f).await;
        let host = f.host.clone();
        let thread = f.thread;
        let waiter =
            tokio::spawn(async move { host.evaluate_pending_routing_shadow(thread).await });
        peer.wait_request().await;
        f.main_complete().await;
        assert!(!waiter.is_finished());
        assert_eq!(peer.replies(), 0);
        let baseline = f.decisions();
        let task: Task = f
            .host
            .snapshot()
            .unwrap()
            .record(
                Collection::Task,
                f.config.root_task.as_str(),
                &f.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(task.state, TaskState::Running);
        let envelope = f
            .host
            .control_envelope(
                CommandId::new(),
                f.config.root_task.clone(),
                task.revision,
                Command::Transition {
                    next: TaskState::Paused,
                    reason: "owner pause during delayed evaluator".into(),
                    verification: None,
                },
            )
            .unwrap();
        let control = f.host.clone();
        tokio::time::timeout(
            Duration::from_secs(3),
            tokio::task::spawn_blocking(move || control.stop(envelope)),
        )
        .await
        .unwrap()
        .unwrap()
        .unwrap();
        // Command completed before the server released its deliberately delayed reply.
        assert_eq!(peer.replies(), 0);
        let outcome = tokio::time::timeout(Duration::from_secs(5), waiter)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(outcome.outcome["outcome"], "unknown");
        assert_eq!(
            f.host
                .lifecycle()
                .inspect(f.thread)
                .unwrap()
                .unresolved_work,
            0
        );
        assert_eq!(f.decisions(), baseline);
        let helper = f
            .attempts()
            .into_iter()
            .find(|a| a.role == RequestRole::Helper)
            .unwrap();
        assert_eq!(helper.phase, ReservationState::ReconciliationPending);
        let task: Task = f
            .host
            .snapshot()
            .unwrap()
            .record(
                Collection::Task,
                f.config.root_task.as_str(),
                &f.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(task.state, TaskState::Paused);
        tokio::time::timeout(Duration::from_secs(15), async {
            while !f
                .host
                .lifecycle()
                .inspect(f.thread)
                .unwrap()
                .interrupt_complete
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let retained = f.host.lifecycle().inspect(f.thread).unwrap();
        f.host
            .lifecycle()
            .resume(f.thread, &retained.revision)
            .unwrap();
        f.host
            .resume(f.thread, task.revision, task.fingerprint)
            .unwrap();
        assert_eq!(peer.observations().len(), 1);
        peer.release();
        f.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn interrupted_shadow_reopens_with_unknown_liability_and_no_replay() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for owner_shutdown in [false, true] {
            let f = Fixture::new(backend).await;
            let peer = Peer::start(Behavior::Delayed, response).await;
            configure(&f, &peer, Operation::JevDecisions);
            f.start().await;
            pending(&f).await;
            let host = f.host.clone();
            let thread = f.thread;
            let waiter =
                tokio::spawn(async move { host.evaluate_pending_routing_shadow(thread).await });
            peer.wait_request().await;
            f.main_complete().await;
            let helper = f
                .attempts()
                .into_iter()
                .find(|a| a.role == RequestRole::Helper)
                .unwrap();
            assert_eq!(helper.phase, ReservationState::Submitted);
            let mut waiting = Some(waiter);
            if !owner_shutdown {
                let waiter = waiting.take().unwrap();
                waiter.abort();
                assert!(waiter.await.unwrap_err().is_cancelled());
                assert_eq!(
                    f.host
                        .lifecycle()
                        .inspect(f.thread)
                        .unwrap()
                        .unresolved_work,
                    0
                );
            }
            let Fixture {
                _temp,
                host,
                owner,
                test,
                config,
                ..
            } = f;
            tokio::time::timeout(Duration::from_secs(5), owner.close())
                .await
                .unwrap()
                .unwrap();
            if let Some(waiter) = waiting {
                let completed = tokio::time::timeout(Duration::from_secs(5), waiter)
                    .await
                    .unwrap()
                    .unwrap();
                match completed {
                    Ok(value) => assert_eq!(value.outcome["outcome"], "unknown"),
                    Err(error) => assert_eq!(error, "owner checkpoint is closed"),
                }
            }
            test.codex.shutdown_and_wait().await.unwrap();
            drop(test);
            drop(host);
            for _ in 0..2 {
                let (restored, owner) = CanonicalHost::open(config.clone()).unwrap();
                let attempt: Attempt = restored
                    .snapshot()
                    .unwrap()
                    .record(Collection::Attempt, helper.id.as_str(), &config.workspace)
                    .unwrap()
                    .decode()
                    .unwrap();
                assert_eq!(attempt.phase, ReservationState::ReconciliationPending);
                assert_eq!(attempt.charged, Micros::ZERO);
                assert!(attempt.send_intent.is_some());
                owner.close().await.unwrap();
                drop(restored);
                assert_eq!(peer.observations().len(), 1);
            }
            peer.release();
            drop(_temp);
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn original_coding_source_change_abstains_before_any_shadow_payload() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend).await;
        let peer = Peer::start(Behavior::Reply, response).await;
        configure(&f, &peer, Operation::JevDecisions);
        f.start().await;
        pending(&f).await;
        f.main_complete().await;
        let baseline = f.decisions();
        let count = f.attempts().len();
        assert!(f.main_bodies.lock().unwrap().iter().any(|body| body
            .to_string()
            .contains("SHADOW_CANONICAL_INSTRUCTION_MARKER")));
        std::fs::write(
            f.workspace.join("AGENTS.md"),
            "CHANGED_AFTER_CANONICAL_ROUTING_ADMISSION",
        )
        .unwrap();
        let outcome = f
            .host
            .evaluate_pending_routing_shadow(f.thread)
            .await
            .unwrap();
        assert_eq!(
            outcome.outcome["outcome"], "baseline",
            "{}",
            outcome.outcome
        );
        assert!(outcome.evaluator_attempt.is_none());
        assert_eq!(f.attempts().len(), count);
        assert_eq!(f.decisions(), baseline);
        assert!(peer.observations().is_empty());
        assert_eq!(peer.handshakes(), 0);
        assert_eq!(
            f.host
                .lifecycle()
                .inspect(f.thread)
                .unwrap()
                .unresolved_work,
            0
        );
        f.close().await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lost_shadow_reply_retains_submission_without_hidden_retry() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend).await;
        let peer = Peer::start(Behavior::Lost, response).await;
        configure(&f, &peer, Operation::ConventionalChat);
        f.start().await;
        pending(&f).await;
        let baseline = f.decisions();
        let outcome = f
            .host
            .evaluate_pending_routing_shadow(f.thread)
            .await
            .unwrap();
        assert_eq!(outcome.outcome["outcome"], "unknown");
        f.main_complete().await;
        let helper = f
            .attempts()
            .into_iter()
            .find(|a| a.role == RequestRole::Helper)
            .unwrap();
        assert_eq!(helper.phase, ReservationState::ReconciliationPending);
        assert_eq!(helper.charged, Micros::ZERO);
        assert_wire(&f, &peer, &helper, Operation::ConventionalChat);
        assert_eq!(peer.replies(), 0);
        assert_eq!(f.decisions(), baseline);
        assert_eq!(
            f.host
                .lifecycle()
                .inspect(f.thread)
                .unwrap()
                .unresolved_work,
            0
        );
        let repeat = f
            .host
            .evaluate_pending_routing_shadow(f.thread)
            .await
            .unwrap();
        assert!(repeat.evaluator_attempt.is_none());
        assert_eq!(peer.observations().len(), 1);
        f.close().await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn late_http_framing_failure_retains_observed_cost_but_cannot_supply_advice() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for operation in [Operation::JevDecisions, Operation::ConventionalChat] {
            let f = Fixture::new(backend).await;
            let peer = Peer::start(Behavior::TruncatedEnvelope, response).await;
            configure(&f, &peer, operation);
            f.start().await;
            pending(&f).await;
            let baseline = f.decisions();
            let outcome = f
                .host
                .evaluate_pending_routing_shadow(f.thread)
                .await
                .unwrap();
            f.main_complete().await;
            assert_eq!(
                outcome.outcome["transport_failed"], true,
                "{}",
                outcome.outcome
            );
            assert_eq!(outcome.outcome["request_written"], true);
            assert_eq!(outcome.outcome["result"]["outcome"], "abstain");
            let helper = f
                .attempts()
                .into_iter()
                .find(|a| a.role == RequestRole::Helper)
                .unwrap();
            assert_eq!(helper.phase, ReservationState::Settled);
            assert_eq!(helper.charged, Micros::new(10));
            assert_wire(&f, &peer, &helper, operation);
            assert_eq!(f.decisions(), baseline);
            assert_eq!(peer.observations().len(), 1);
            assert_eq!(
                f.host
                    .lifecycle()
                    .inspect(f.thread)
                    .unwrap()
                    .unresolved_work,
                0
            );
            f.close().await;
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn source_credential_and_pause_changes_after_tls_block_application_payload() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for change in ["source", "credential", "pause"] {
            let f = Fixture::new(backend).await;
            let peer = Peer::start(Behavior::Reply, response).await;
            configure(&f, &peer, Operation::JevDecisions);
            let arrived = Arc::new(tokio::sync::Notify::new());
            let release = Arc::new(tokio::sync::Notify::new());
            f.host
                .qualification_block_decision_after_tls(arrived.clone(), release.clone())
                .unwrap();
            f.start().await;
            pending(&f).await;
            let host = f.host.clone();
            let thread = f.thread;
            let waiter =
                tokio::spawn(async move { host.evaluate_pending_routing_shadow(thread).await });
            tokio::time::timeout(Duration::from_secs(10), arrived.notified())
                .await
                .unwrap();
            f.main_complete().await;
            assert!(peer.observations().is_empty());
            let baseline = f.decisions();
            match change {
                "source" => {
                    assert!(f.main_bodies.lock().unwrap().iter().any(|body| body
                        .to_string()
                        .contains("SHADOW_CANONICAL_INSTRUCTION_MARKER")));
                    std::fs::write(f.workspace.join("AGENTS.md"), "SOURCE_REPLACED_DURING_TLS")
                        .unwrap()
                }
                "credential" => f.host.revoke_decision_credential().unwrap(),
                "pause" => {
                    let task: Task = f
                        .host
                        .snapshot()
                        .unwrap()
                        .record(
                            Collection::Task,
                            f.config.root_task.as_str(),
                            &f.config.workspace,
                        )
                        .unwrap()
                        .decode()
                        .unwrap();
                    let command = f
                        .host
                        .control_envelope(
                            CommandId::new(),
                            f.config.root_task.clone(),
                            task.revision,
                            Command::Transition {
                                next: TaskState::Paused,
                                reason: "owner pause at evaluator TLS barrier".into(),
                                verification: None,
                            },
                        )
                        .unwrap();
                    f.host.stop(command).unwrap();
                }
                _ => unreachable!(),
            }
            release.notify_one();
            let outcome = tokio::time::timeout(Duration::from_secs(5), waiter)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(outcome.outcome["outcome"], "unknown");
            assert!(
                peer.observations().is_empty(),
                "{backend:?}/{change}: payload escaped final fence"
            );
            assert_eq!(peer.replies(), 0);
            let helper = f
                .attempts()
                .into_iter()
                .find(|a| a.role == RequestRole::Helper)
                .unwrap();
            // Existing accounting contract treats a recorded SendIntent as
            // liability even when this fixture proves no application payload.
            assert!(helper.send_intent.is_some());
            assert_eq!(helper.phase, ReservationState::ReconciliationPending);
            assert_eq!(f.decisions(), baseline);
            assert_eq!(
                f.host
                    .lifecycle()
                    .inspect(f.thread)
                    .unwrap()
                    .unresolved_work,
                0
            );
            f.close().await;
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn helper_budget_and_recipient_denials_preserve_main_admission_without_evaluator_io() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for reason in ["budget", "recipient"] {
            let f = Fixture::with_limits(
                backend,
                if reason == "budget" { 200 } else { 1000 },
                reason != "recipient",
            )
            .await;
            let peer = Peer::start(Behavior::Reply, response).await;
            configure_rate(
                &f,
                &peer,
                Operation::ConventionalChat,
                if reason == "budget" { 150 } else { 10 },
            );
            f.start().await;
            pending(&f).await;
            f.main_complete().await;
            let baseline = f.decisions();
            assert_eq!(baseline.len(), 1);
            assert_eq!(
                baseline[0]["decision"]["candidates"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|candidate| candidate["exclusions"].as_array().unwrap().is_empty())
                    .count(),
                2,
                "both routes must remain eligible before helper rejection"
            );
            let attempts = f.attempts();
            assert_eq!(attempts.len(), 1);
            assert_eq!(attempts[0].role, RequestRole::Main);
            assert_eq!(attempts[0].phase, ReservationState::Settled);
            assert_eq!(attempts[0].quote.amount.micros, Micros::new(100));
            let outcome = f
                .host
                .evaluate_pending_routing_shadow(f.thread)
                .await
                .unwrap();
            assert_eq!(
                outcome.outcome["outcome"], "baseline",
                "{backend:?}/{reason}: {}",
                outcome.outcome
            );
            assert!(outcome.evaluator_attempt.is_none());
            assert_eq!(f.attempts(), attempts);
            assert_eq!(f.decisions(), baseline);
            assert_eq!(peer.handshakes(), 0);
            assert!(peer.observations().is_empty());
            assert_eq!(
                f.host
                    .lifecycle()
                    .inspect(f.thread)
                    .unwrap()
                    .unresolved_work,
                0
            );
            f.close().await;
        }
    }
}
