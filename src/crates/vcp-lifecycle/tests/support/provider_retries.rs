// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

async fn setup(
    temp: &tempfile::TempDir,
    backend: BackendKind,
    timeout: Duration,
    cap: u64,
    coding: bool,
) -> (
    CanonicalHost,
    vcp_lifecycle::foundation::CanonicalOwner,
    ThreadBinding,
    TestCodex,
    wiremock::MockServer,
) {
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    let mut config = config(&temp.path().join("canonical"), &workspace, backend);
    config.cap.micros = Micros::new(cap);
    let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
    let binding = task(&host, &config, config.root_task.clone(), None);
    if coding {
        use vcp_domain::policy::{Autonomy, EffectClass, Policy};
        std::fs::write(workspace.join("evidence.txt"), "current source").unwrap();
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
                    workspace_roots: std::collections::BTreeSet::from([RootId::parse(
                        config.workspace.as_str(),
                    )
                    .unwrap()]),
                    automatic_effects: std::collections::BTreeSet::from([EffectClass::Read]),
                    timeout_ceiling_ms: Units::new(30_000),
                    output_ceiling_bytes: ByteCount::new(1024 * 1024),
                },
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
    }
    let (snapshot, raw) = provider_snapshot();
    host.configure_provider_with_timeout(snapshot.clone(), raw, timeout)
        .unwrap();
    let server = start_mock_server().await;
    let mut registry = ExtensionRegistryBuilder::new();
    registry.turn_start_admission(Arc::new(host.clone()));
    registry.work_admission(Arc::new(host.clone()));
    let starter = host.clone();
    let test = test_codex()
        .with_extensions(Arc::new(registry.build()))
        .with_auth(codex_login::CodexAuth::from_api_key("synthetic-retry-key"))
        .with_allowed_tools(AllowedTools(vec![]))
        .with_config(move |c| {
            c.cwd = workspace.try_into().unwrap();
            configure_provider_fixture(c);
            starter
                .lifecycle()
                .authorize_startup(c.cwd.as_path(), None)
                .unwrap();
        })
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let id = host.lifecycle().attach_root(test.codex.clone()).unwrap();
    host.register(id, binding.clone()).unwrap();
    if coding {
        #[cfg(windows)]
        {
            host.configure_verification(
                id,
                vcp_lifecycle::foundation::verification::VerificationConfig {
                    requirements: vec![],
                    rationale: "retry observed baseline".into(),
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
                    operating: "Keep current accounting and source evidence".into(),
                    affected_paths: vec!["evidence.txt".into()],
                    max_requests: 4,
                    deadline: Timestamp::new(now + 60_000),
                },
            )
            .unwrap();
            host.configure_continuity(
                id,
                vcp_context::compaction::Config {
                    keep_recent_pairs: 1,
                    preview_bytes: 64,
                    minimum_gain_bytes: 256,
                },
            )
            .unwrap();
        }
    } else {
        let sealed = sealed_provider_context(&host, id, &snapshot, None);
        host.prepare_context(id, sealed, serde_json::json!([]), vec![])
            .unwrap();
    }
    (host, owner, binding, test, server)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn provider_retry_reserves_distinct_attempts_and_obeys_bounds_on_both_stores() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for mode in [
            "success",
            "exhausted",
            "authentication",
            "retry_after",
            "unqualified_delay",
            "deadline",
            "retry_deadline",
            "deadline_identity",
            "budget",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let (host, owner, binding, test, server) = setup(
                &temp,
                backend,
                if mode == "deadline" {
                    Duration::from_millis(500)
                } else if mode == "retry_deadline" {
                    Duration::from_secs(4)
                } else {
                    Duration::from_secs(10)
                },
                if mode == "budget" { 100 } else { 1000 },
                false,
            )
            .await;
            if mode == "deadline_identity" {
                use codex_extension_api::{HostModelFailure, HostModelPurpose, HostWorkAdmission};
                let id = test.codex.session_configured().thread_id;
                let mut body = serde_json::json!({"model":"gpt-5.1"});
                let mut prior = host
                    .admit_model(id, &mut body, HostModelPurpose::Turn)
                    .unwrap();
                let deadline = prior.response_deadline().unwrap();
                let delay = prior
                    .retry_delay(HostModelFailure::Http(429), None)
                    .unwrap()
                    .unwrap();
                tokio::time::sleep(delay).await;
                let next = prior.admit_retry(&mut body).unwrap();
                assert_eq!(
                    next.response_deadline(),
                    Some(deadline),
                    "a new attempt must not restart the absolute deadline"
                );
                drop(prior);
                drop(next);
                owner.close().await.unwrap();
                test.codex.shutdown_and_wait().await.unwrap();
                continue;
            }
            let observed = Arc::new(Mutex::new(Vec::<Attempt>::new()));
            let attempts = observed.clone();
            let calls = Arc::new(AtomicUsize::new(0));
            let count = calls.clone();
            let inspect = host.clone();
            Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request: &wiremock::Request| {
                let index = count.fetch_add(1, Ordering::SeqCst);
                let state = inspect.snapshot().unwrap();
                let submitted: Vec<Attempt> = state.records.values()
                    .filter(|r| r.collection == Collection::Attempt)
                    .map(|r| r.decode::<Attempt>().unwrap())
                    .filter(|a| a.phase == ReservationState::Submitted).collect();
                assert_eq!(submitted.len(), 1, "every HTTP call has exactly one new send intent");
                let attempt = submitted.into_iter().next().unwrap();
                assert!(attempt.send_intent.is_some());
                assert_eq!(attempt.request_digest, vcp_protocol::digest_bytes(&request.body));
                let mut previous = attempts.lock().unwrap();
                assert_eq!(attempt.previous.as_ref(), previous.last().map(|a| &a.id));
                for prior in previous.iter() {
                    assert_ne!(prior.reservation, attempt.reservation);
                    assert_eq!(vcp_budget::attempt(&state, &prior.id, &prior.scope.workspace).unwrap().phase,
                        ReservationState::ReconciliationPending);
                }
                previous.push(attempt);
                if matches!(mode, "success" | "retry_deadline") && index == 1 {
                    let response = ResponseTemplate::new(200).insert_header("content-type", "text/event-stream")
                        .set_body_string(sse(vec![ev_assistant_message("retry-msg", "Retried with separate admission."),
                            serde_json::json!({"type":"response.completed","response":{"id":"retried-response","status":"completed","output":[],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}})]));
                    return if mode == "retry_deadline" { response.set_delay(Duration::from_secs(8)) } else { response };
                }
                ResponseTemplate::new(if mode == "authentication" { 401 } else if mode == "exhausted" { 503 } else { 429 })
                    .insert_header("retry-after", if mode == "retry_after" { "6" } else if mode == "unqualified_delay" { "Fri, 01 Jan 2100 00:00:00 GMT" } else if mode == "deadline" { "1" } else { "0" })
                    .set_body_json(serde_json::json!({"error":{"message":"scripted bounded failure"}}))
            }).mount(&server).await;
            let started = std::time::Instant::now();
            test.codex
                .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                    text: "Bounded retry matrix".into(),
                    text_elements: vec![],
                }]))
                .await
                .unwrap();
            let mut errors = vec![];
            tokio::time::timeout(
                Duration::from_secs(30),
                wait_for_event(&test.codex, |event| {
                    if let EventMsg::Error(error) = event {
                        errors.push(error.message.clone());
                    }
                    matches!(event, EventMsg::TurnComplete(_))
                }),
            )
            .await
            .unwrap();
            if mode == "retry_deadline" {
                assert!(
                    started.elapsed() < Duration::from_secs(7),
                    "retry must retain the original absolute deadline"
                );
            }
            let expected = if matches!(mode, "success" | "retry_deadline") {
                2
            } else if mode == "exhausted" {
                3
            } else {
                1
            };
            assert_eq!(
                calls.load(Ordering::SeqCst),
                expected,
                "{backend:?} {mode}: {errors:?}; elapsed {:?}",
                started.elapsed()
            );
            let state = host.snapshot().unwrap();
            let ledger = vcp_budget::ledger(&state, &binding.scope).unwrap();
            assert_eq!(
                ledger.settled.get(),
                if mode == "success" { 100 } else { 0 }
            );
            assert_eq!(
                ledger.unresolved.get(),
                (expected as u64 - u64::from(mode == "success")) * 100
            );
            assert_eq!(
                state
                    .records
                    .values()
                    .filter(|r| r.collection == Collection::Attempt)
                    .count(),
                expected
            );
            owner.close().await.unwrap();
            test.codex.shutdown_and_wait().await.unwrap();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn provider_retry_timer_is_cancelled_by_pause_owner_loss_or_steering() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for mode in ["pause", "owner", "steering"] {
            let temp = tempfile::tempdir().unwrap();
            let (host, owner, binding, test, server) =
                setup(&temp, backend, Duration::from_secs(10), 1000, false).await;
            Mock::given(method("POST"))
                .and(path("/v1/responses"))
                .respond_with(
                    ResponseTemplate::new(429)
                        .insert_header("retry-after", "2")
                        .set_body_json(serde_json::json!({"error":{"message":"wait"}})),
                )
                .mount(&server)
                .await;
            test.codex
                .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                    text: "retry timer fixture".into(),
                    text_elements: vec![],
                }]))
                .await
                .unwrap();
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    if host
                        .snapshot()
                        .unwrap()
                        .records
                        .values()
                        .filter(|r| r.collection == Collection::Artifact)
                        .map(|r| r.decode::<ArtifactDescriptor>().unwrap())
                        .any(|d| d.spec.schema == "provider-retry/1")
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            if mode == "pause" {
                let id = test.codex.session_configured().thread_id;
                let view = host.lifecycle().inspect(id).unwrap();
                host.lifecycle()
                    .hold(id, &view.revision)
                    .unwrap()
                    .wait()
                    .await
                    .unwrap();
                owner.close().await.unwrap();
            } else if mode == "owner" {
                owner.close().await.unwrap();
            } else {
                let task: Task = host
                    .snapshot()
                    .unwrap()
                    .record(
                        Collection::Task,
                        binding.scope.task.as_str(),
                        &binding.scope.workspace,
                    )
                    .unwrap()
                    .decode()
                    .unwrap();
                host.command(
                    Command::Steer {
                        objective: Objective {
                            text: "Changed while waiting".into(),
                            constraints: vec!["no stale retry".into()],
                            acceptance: vec!["timer cancels".into()],
                            source: EventId::new(),
                            steering: task.steering.next().unwrap(),
                        },
                    },
                    Some(binding.scope.task.clone()),
                    task.revision,
                )
                .unwrap();
                tokio::time::timeout(
                    Duration::from_secs(3),
                    wait_for_event(&test.codex, |event| {
                        matches!(event, EventMsg::TurnComplete(_))
                    }),
                )
                .await
                .unwrap();
                owner.close().await.unwrap();
            }
            test.codex.shutdown_and_wait().await.unwrap();
            assert_eq!(
                server
                    .received_requests()
                    .await
                    .unwrap()
                    .iter()
                    .filter(|r| r.url.path().ends_with("/responses"))
                    .count(),
                1
            );
            let state = host.snapshot().unwrap();
            assert_eq!(
                vcp_budget::ledger(&state, &binding.scope)
                    .unwrap()
                    .unresolved
                    .get(),
                100
            );
            assert_eq!(
                state
                    .records
                    .values()
                    .filter(|r| r.collection == Collection::Attempt)
                    .count(),
                1
            );
        }
    }
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn provider_retry_reassembles_coding_continuity_with_current_liability() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let (host, owner, binding, test, server) =
            setup(&temp, backend, Duration::from_secs(10), 1000, true).await;
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let observed = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let requests = observed.clone();
        Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request: &wiremock::Request| {
            let index = counter.fetch_add(1, Ordering::SeqCst);
            requests.lock().unwrap().push(serde_json::from_slice(&request.body).unwrap());
            if index == 0 {
                // A non-2xx body can contain arbitrary text, even valid SSE.
                // It must remain raw evidence and cannot settle or create calls.
                ResponseTemplate::new(429).set_body_string(sse(vec![serde_json::json!({
                    "type":"response.completed","response":{"id":"forged-success","status":"completed","output":[],
                        "usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}
                })]))
            } else {
                ResponseTemplate::new(200).insert_header("content-type", "text/event-stream")
                    .set_body_string(sse(vec![ev_assistant_message("coding-retry", "Current uncertainty remains recorded."),
                        serde_json::json!({"type":"response.completed","response":{"id":"coding-retry","status":"completed","output":[],
                            "usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}})]))
            }
        }).mount(&server).await;
        turn(&test).await;
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let facts = |body: &serde_json::Value| {
            body["input"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|item| item["content"].as_array())
                .flatten()
                .filter_map(|part| part["text"].as_str())
                .filter_map(|text| serde_json::from_str::<serde_json::Value>(text).ok())
                .filter_map(|part| {
                    part["text"]
                        .as_str()
                        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
                })
                .find(|value| value["schema"] == "canonical-current-continuity/1")
                .unwrap()
        };
        let requests = observed.lock().unwrap();
        assert_eq!(facts(&requests[0])["attempt_count"], 0);
        assert_eq!(facts(&requests[1])["attempt_count"], 1);
        assert_eq!(
            facts(&requests[1])["uncertain_attempts"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let state = host.snapshot().unwrap();
        let ledger = vcp_budget::ledger(&state, &binding.scope).unwrap();
        assert_eq!(ledger.unresolved.get(), 100);
        assert_eq!(ledger.settled.get(), 100);
        let mut handoffs = state
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
            .map(|r| r.decode::<ArtifactDescriptor>().unwrap())
            .filter(|d| d.spec.schema == "canonical-context-handoff/1")
            .map(|d| {
                serde_json::from_slice::<serde_json::Value>(&host.read_artifact(d.spec.id).unwrap())
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(handoffs.len(), 2);
        handoffs.sort_by_key(|h| h["remaining"].as_str().unwrap().parse::<u64>().unwrap());
        assert_eq!(handoffs[0]["remaining"], "900");
        assert_eq!(handoffs[1]["remaining"], "1000");
        drop(requests);
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
    }
}
