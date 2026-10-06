// SPDX-License-Identifier: Apache-2.0
//! MR-01/03/04/05: real production CLI routing/admission with synthetic loopback.
use super::*;
use std::{collections::BTreeMap, sync::Mutex};
use vcp_cli::model_preferences::{ChoicePreferences, Preferences};
use vcp_domain::{accounting::RequestRole, Micros};
use vcp_store::contract::CanonicalStore;

const FIRST: &str = "fixture/rotation-first";
const PEER: &str = "fixture/rotation-peer";
const RESERVE: &str = "fixture/rotation-reserve";

fn configure_rotation(fixture: &Fixture, diverse_endpoints: bool) {
    let now = Timestamp::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
    );
    let mut prepared = Vec::new();
    for (model, tags) in [
        (
            FIRST,
            if diverse_endpoints {
                vec!["first/region-1", "first/region-2"]
            } else {
                vec!["first/region-1"]
            },
        ),
        (PEER, vec!["peer/region"]),
        (RESERVE, vec!["reserve/region"]),
    ] {
        let raw = serde_json::to_vec(&json!({"data":{"id":model,"endpoints":tags.into_iter().map(|tag|json!({
            "tag":tag,"status":0,"context_length":500000,"max_prompt_tokens":400000,"max_completion_tokens":8000,
            "supported_parameters":["tools","tool_choice","max_tokens"],"pricing":{"prompt":"0","completion":"0","request":"0.0001"}
        })).collect::<Vec<_>>()}})).unwrap();
        prepared.extend(
            vcp_models::catalog::compatibility::snapshots(&raw, now)
                .unwrap()
                .into_iter()
                .map(|snapshot| (snapshot, raw.clone())),
        );
    }
    let mut preferences = Preferences::default();
    preferences.set.roles = BTreeMap::from([(
        "main".into(),
        vec![FIRST.into(), PEER.into(), RESERVE.into()],
    )]);
    preferences.choice_sets = BTreeMap::from([(
        "main".into(),
        vec![
            ChoicePreferences {
                models: vec![FIRST.into(), PEER.into()],
                endpoints: BTreeMap::new(),
                max_reference_request_cost_usd: None,
            },
            ChoicePreferences {
                models: vec![RESERVE.into()],
                endpoints: BTreeMap::new(),
                max_reference_request_cost_usd: Some("0.001".into()),
            },
        ],
    )]);
    let configuration =
        vcp_cli::model_preferences::routing_configuration(&preferences, &prepared).unwrap();
    let main = &configuration
        .rotation
        .as_ref()
        .unwrap()
        .sets(RequestRole::Main)[0]
        .members[0];
    let snapshot = configuration.catalog.snapshot(main).unwrap();
    let raw = configuration.raw_catalogs.get(&snapshot.id).unwrap();
    let mut profile: Value = serde_json::from_slice(&fs::read(&fixture.profile).unwrap()).unwrap();
    fs::write(profile["catalog"].as_str().unwrap(), raw).unwrap();
    profile["provider"] = json!(snapshot);
    profile["routing"] = json!(configuration);
    profile["max_transport_retries"] = json!(2);
    fs::write(&fixture.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
}

async fn run_rotation(
    fixture: &mut Fixture,
    backend: BackendKind,
    rejected: u64,
) -> (TaskId, Value) {
    // Five sequential provider attempts plus native verification exceed the
    // old 90s whole-process guard on the debug Files backend. The independent
    // rejection-to-replacement check below still forbids a 60s cooldown wait.
    const PROCESS_DEADLINE: Duration = Duration::from_secs(180);
    fixture._temp.disable_cleanup(true);
    let root = fixture._temp.path().to_owned();
    let local = fixture._temp.path().join("rotation-local-app-data");
    fs::create_dir(&local).unwrap();
    let mut command = fixture.command(&["run", OBJECTIVE, "--autonomy", "autonomous"]);
    // This process still installs the production shared-account gate. A private
    // account namespace isolates this bounded fixture from parallel tests.
    command.env("LOCALAPPDATA", local);
    let stdout_path = root.join("rotation-stdout.jsonl");
    let stderr_path = root.join("rotation-stderr.log");
    fs::write(
        root.join("rotation-command.json"),
        serde_json::to_vec_pretty(&json!({
            "backend":format!("{backend:?}"), "rejected":rejected, "expected_attempts":3+rejected,
            "executable":fixture.binary, "profile":fixture.profile,
            "deadline_seconds":PROCESS_DEADLINE.as_secs(), "replacement_deadline_seconds":60,
        }))
        .unwrap(),
    )
    .unwrap();
    let mut command = tokio::process::Command::from(command);
    command
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::from(fs::File::create(&stdout_path).unwrap()))
        .stderr(Stdio::from(fs::File::create(&stderr_path).unwrap()));
    let mut child = command.spawn().unwrap();
    eprintln!(
        "rotation {backend:?}/rejected={rejected}: {} attempts, child {:?}, captures {}",
        3 + rejected,
        child.id(),
        root.display()
    );
    let began = Instant::now();
    let status = match tokio::time::timeout(PROCESS_DEADLINE, child.wait()).await {
        Ok(status) => status.unwrap(),
        Err(_) => {
            let stopped = child.start_kill();
            let reaped = tokio::time::timeout(Duration::from_secs(10), child.wait()).await;
            panic!("rotation {backend:?}/rejected={rejected} exceeded {PROCESS_DEADLINE:?}; owned child stop {stopped:?}, reap {reaped:?}; captures {}", root.display());
        }
    };
    eprintln!(
        "rotation {backend:?}/rejected={rejected}: exited {status} after {:?}",
        began.elapsed()
    );
    let stdout = String::from_utf8(fs::read(stdout_path).unwrap()).unwrap();
    let stderr = fs::read(stderr_path).unwrap();
    assert!(!stdout.contains(SECRET));
    let result: Value = stdout
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|frame| frame["type"] == "result")
        .unwrap_or_else(|| panic!("rotation {backend:?}/rejected={rejected}: missing result, status {status}; captures {}", root.display()));
    assert!(
        status.success(),
        "rotation {backend:?}/rejected={rejected}: {result}; {}; captures {}",
        String::from_utf8_lossy(&stderr),
        root.display()
    );
    (
        serde_json::from_value(result["scope"]["task"].clone()).unwrap(),
        result,
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_cli_rotates_then_fails_over_and_reconciles_rejected_generations() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        // 0 proves request rotation and model-before-endpoint weighting. 1
        // proves a healthy peer replaces rejection within 60s despite the
        // failed endpoint's 300s cooldown (longer than the whole case guard).
        // 2 combines a model 429 and transient 503, exhausts the preferred
        // choice and admits the approved second set without settling either.
        for rejected in 0..=2 {
            let server = MockServer::start().await;
            let observed = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
            let calls = observed.clone();
            let arrivals = Arc::new(Mutex::new(Vec::<(Instant, Option<u16>)>::new()));
            let timing = arrivals.clone();
            let successes = Arc::new(AtomicUsize::new(0));
            let success = successes.clone();
            Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request:&wiremock::Request| {
                let arrived = Instant::now();
                assert_eq!(request.headers["authorization"],format!("Bearer {SECRET}"));
                let body:Value = serde_json::from_slice(&request.body).unwrap();
                let model = body["model"].as_str().unwrap().to_owned();
                let endpoint = body["provider"]["only"][0].as_str().unwrap().to_owned();
                assert_eq!(body["provider"]["only"].as_array().unwrap().len(),1);
                assert_eq!(body["provider"]["allow_fallbacks"],false);
                assert_eq!(body["provider"]["require_parameters"],true);
                assert_eq!(body["provider"]["data_collection"],"deny");
                assert_eq!(body["store"],false);
                assert!(body["input"].as_array().is_some_and(|input|!input.is_empty()));
                assert!(body["tools"].as_array().is_some_and(|tools|!tools.is_empty()));
                calls.lock().unwrap().push((model.clone(),endpoint.clone()));
                let failed = (rejected>=1 && model==FIRST) || (rejected==2 && model==PEER);
                if failed {
                    let id = if model==FIRST {"gen-rotation-first-rejected"} else {"gen-rotation-peer-rejected"};
                    let status = if model==FIRST {429} else {503};
                    timing.lock().unwrap().push((arrived, Some(status)));
                    // Longer than the complete fixture window: a slow native
                    // verification must not turn this case into a recovery probe.
                    ResponseTemplate::new(status).insert_header("retry-after","300").insert_header("x-request-id",id)
                        .set_body_json(json!({"error":{"code":status,"message":"synthetic shared pool overload","metadata":{"limit_source":"upstream_provider_shared_pool","generation_id":id}}}))
                } else {
                    timing.lock().unwrap().push((arrived, None));
                    let index = success.fetch_add(1,Ordering::SeqCst);
                    ResponseTemplate::new(200).insert_header("content-type","text/event-stream").set_body_string(response(index))
                }
            }).mount(&server).await;
            Mock::given(method("GET"))
                .and(path("/v1/generation"))
                .respond_with(|request: &wiremock::Request| {
                    assert_eq!(request.headers["authorization"], format!("Bearer {SECRET}"));
                    let id = request
                        .url
                        .query_pairs()
                        .find(|(name, _)| name == "id")
                        .unwrap()
                        .1
                        .into_owned();
                    assert!(matches!(
                        id.as_str(),
                        "gen-rotation-first-rejected" | "gen-rotation-peer-rejected"
                    ));
                    assert!(request.body.is_empty());
                    ResponseTemplate::new(200).set_body_json(
                        json!({"data":{"id":id,"total_cost":0,"currency":"USD","cancelled":true}}),
                    )
                })
                // EE-01: normal execution never fetches billing receipts.
                .expect(0)
                .mount(&server)
                .await;
            let mut fixture = Fixture::new(&server.uri());
            configure_rotation(&fixture, rejected == 0);
            let entry = fixture.seed(backend).await;
            assert!(observed.lock().unwrap().is_empty());
            let (task_id, _) = run_rotation(&mut fixture, backend, rejected).await;
            // Measure only rejected-response to replacement admission. The
            // successful patch/verification/final rounds can independently take
            // longer than the failed endpoint's advertised cooldown.
            let arrivals = arrivals.lock().unwrap().clone();
            for (index, (rejected_at, status)) in arrivals.iter().enumerate() {
                let Some(status) = status else {
                    continue;
                };
                let (replacement_at, _) = arrivals
                    .get(index + 1)
                    .expect("rejection needs a replacement request");
                let interval = replacement_at.duration_since(*rejected_at);
                assert!(interval < Duration::from_secs(60),
                    "{backend:?}/{rejected} rejections: HTTP {status} replacement waited {interval:?} for the failed endpoint cooldown");
            }
            let routes = observed.lock().unwrap().clone();
            let expected: Vec<(&str, &str)> = match rejected {
                0 => vec![
                    (FIRST, "first/region-1"),
                    (PEER, "peer/region"),
                    (FIRST, "first/region-2"),
                ],
                1 => vec![
                    (FIRST, "first/region-1"),
                    (PEER, "peer/region"),
                    (PEER, "peer/region"),
                    (PEER, "peer/region"),
                ],
                _ => vec![
                    (FIRST, "first/region-1"),
                    (PEER, "peer/region"),
                    (RESERVE, "reserve/region"),
                    (RESERVE, "reserve/region"),
                    (RESERVE, "reserve/region"),
                ],
            };
            assert_eq!(
                routes,
                expected
                    .into_iter()
                    .map(|(model, endpoint)| (model.into(), endpoint.into()))
                    .collect::<Vec<_>>()
            );
            assert_eq!(successes.load(Ordering::SeqCst), 3);
            assert_eq!(
                fs::read(fixture.workspace.join("value.txt")).unwrap(),
                b"42\n"
            );
            let store = reopen(&entry).await;
            let task: Task = store
                .current()
                .record(Collection::Task, task_id.as_str(), &entry.config.workspace)
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(task.state, TaskState::Completed);
            let attempts = store
                .current()
                .records
                .values()
                .filter(|record| record.collection == Collection::Attempt)
                .map(|record| record.decode::<Attempt>().unwrap())
                .filter(|attempt| attempt.scope.task == task_id)
                .collect::<Vec<_>>();
            assert_eq!(attempts.len(), 3 + rejected as usize);
            assert_eq!(
                attempts
                    .iter()
                    .filter(|attempt| attempt.phase == ReservationState::Settled
                        && attempt.uncertain.is_none()
                        && attempt.charged == Micros::new(100))
                    .count(),
                3
            );
            assert_eq!(
                attempts
                    .iter()
                    .filter(
                        |attempt| attempt.phase == ReservationState::ReconciliationPending
                            && attempt.uncertain.is_some()
                            && attempt.charged == Micros::ZERO
                    )
                    .count(),
                rejected as usize
            );
            assert_eq!(
                attempts
                    .iter()
                    .filter(|attempt| attempt.charged == Micros::ZERO)
                    .count(),
                rejected as usize
            );
            assert_eq!(
                attempts
                    .iter()
                    .filter(|attempt| attempt.previous.is_some())
                    .count(),
                rejected as usize
            );
            let ledgers = store
                .current()
                .records
                .values()
                .filter(|record| record.collection == Collection::Ledger)
                .map(|record| record.decode::<Ledger>().unwrap())
                .filter(|ledger| ledger.scope == task.scope)
                .collect::<Vec<_>>();
            assert_eq!(ledgers.len(), 1);
            let ledger = &ledgers[0];
            assert_eq!(ledger.settled.get(), 300);
            assert_eq!(ledger.active, Micros::ZERO);
            assert_eq!(ledger.unresolved.known().unwrap().get(), 100 * rejected);
            assert!(!ledger.overrun);
            store.close().await.unwrap();
            fixture._temp.disable_cleanup(false);
        }
    }
}
