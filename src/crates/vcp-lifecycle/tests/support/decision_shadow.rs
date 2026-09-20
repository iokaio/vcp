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
            assert_eq!(body["model"], "fixture/economical");
            observed.lock().unwrap().push(body);
            let completed: Value = serde_json::from_str(r#"{"type":"response.completed","response":{"id":"shadow-main-response","status":"completed","output":[],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}}"#).unwrap();
            ResponseTemplate::new(200).insert_header("content-type", "text/event-stream").set_body_string(sse(vec![ev_assistant_message("main-done", "Observed synthetic source."), completed]))
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
                max_requests: 1,
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
