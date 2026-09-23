// SPDX-License-Identifier: Apache-2.0
//! P8-02: actual owner kills around root/registered-child provider delivery.
use super::*;
use std::{
    io::Write,
    path::Path,
    process::Stdio,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
use vcp_lifecycle::foundation::model_dispatch_qualification::Point;
use vcp_store::contract::State;

fn durable(path: &Path, value: &impl serde::Serialize) {
    let temporary = path.with_extension("tmp");
    let mut file = fs::File::create(&temporary).unwrap();
    file.write_all(&vcp_protocol::canonical_bytes(value).unwrap())
        .unwrap();
    file.sync_all().unwrap();
    drop(file);
    fs::rename(temporary, path).unwrap();
}

#[test]
#[ignore = "supervised actual model-dispatch owner process"]
fn model_dispatch_fault_process() {
    let root = PathBuf::from(std::env::var_os("VCP_MODEL_KILL_ROOT").unwrap());
    let backend = match std::env::var("VCP_MODEL_KILL_BACKEND").unwrap().as_str() {
        "files" => BackendKind::Files,
        "sqlite" => BackendKind::Sqlite,
        _ => panic!("unknown backend"),
    };
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let endpoint = std::env::var("VCP_MODEL_KILL_ENDPOINT").unwrap();
        // The supervisor owns cleanup, including failures before the kill.
        let mut f = std::mem::ManuallyDrop::new(Fixture::new_in_endpoint(backend, 0, tempfile::tempdir_in(&root).unwrap(), Some(endpoint)).await);
        let role = std::env::var("VCP_MODEL_KILL_ROLE").unwrap();
        let (thread, task, retained) = if role == "child" {
            let task = f.assign(&[]).await;
            let (thread, _, retained) = f.start(&task).await;
            (thread, task, retained)
        } else {
            assert_eq!(role, "root");
            (f.parent, f.config.root_task.clone(), f.test.codex.clone())
        };
        let (snapshot, _) = provider_snapshot();
        let sealed = sealed_provider_context(&f.host, thread, &snapshot, None);
        f.host.prepare_context(thread, sealed, serde_json::json!([]), vec![]).unwrap();
        durable(&root.join("setup.json"), &serde_json::json!({"config":f.config,"task":task,"role":role}));
        let phase = std::env::var("VCP_MODEL_KILL_PHASE").unwrap();
        let observed_root = root.clone();
        f.host.qualification_observe_model_dispatch(move |point, attempt, state| {
            let name = match point { Point::BeforeTransport => "before_transport", Point::BeforeSettlement => "before_settlement" };
            durable(&observed_root.join(format!("{name}-state.json")), state);
            durable(&observed_root.join(format!("{name}.json")), &serde_json::json!({"point":name,"attempt":attempt,"watermark":state.watermark}));
            if phase == name {
                // The independent supervisor must kill this process. No normal
                // release path can turn a missed barrier into a passing case.
                let deadline = Instant::now() + Duration::from_secs(120);
                while Instant::now() < deadline { std::thread::sleep(Duration::from_millis(20)); }
                return Err("model-dispatch supervisor missed its deadline".into());
            }
            Ok(())
        }).unwrap();
        retained.start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text:"Observe one bounded model request; never perform a file effect.".into(), text_elements:vec![],
        }])).await.unwrap();
        tokio::time::timeout(Duration::from_secs(120), async {
            loop {
                match retained.next_event().await.unwrap().msg {
                    EventMsg::Error(error) => panic!("model dispatch fixture failed before kill: {error:?}"),
                    EventMsg::TurnComplete(_) => panic!("model dispatch fixture completed instead of being killed"),
                    _ => {}
                }
            }
        }).await.unwrap();
    });
}

struct Supervised(std::process::Child);
impl Supervised {
    fn terminate(&mut self) {
        self.0.kill().unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                assert!(!status.success());
                return;
            }
            assert!(Instant::now() < deadline, "model owner reap deadline");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Supervised {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if self.0.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn root_and_child_model_dispatch_process_kills_preserve_liability_without_replay() {
    let retained = match std::env::var_os("VCP_TEST_RECOVERY_EVIDENCE") {
        Some(parent) => tempfile::Builder::new()
            .prefix("model-dispatch-")
            .tempdir_in(parent)
            .unwrap(),
        None => tempfile::Builder::new()
            .prefix("vcp-p802-model-dispatch-")
            .tempdir()
            .unwrap(),
    }
    .keep();
    eprintln!("P8-02 model dispatch evidence: {}", retained.display());
    for backend in ["files", "sqlite"] {
        for role in ["root", "child"] {
            for phase in ["before_transport", "wire_arrival", "before_settlement"] {
                let root = retained.join(format!("{backend}-{role}-{phase}"));
                fs::create_dir(&root).unwrap();
                let server = MockServer::start().await;
                let count = Arc::new(AtomicUsize::new(0));
                let observed = count.clone();
                let wire_root = root.clone();
                Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request: &wiremock::Request| {
                    let sequence = observed.fetch_add(1, Ordering::SeqCst);
                    let request: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                    // This server belongs to the supervisor, outside the killed
                    // owner. Arrival is the independent external-effect oracle.
                    durable(&wire_root.join(format!("wire-{sequence}.json")), &serde_json::json!({"sequence":sequence,"body_sha256":vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&request).unwrap())}));
                    let body = sse(vec![ev_response_created("model-kill-response"), ev_assistant_message("answer", "Ordinary retained model bytes; no tool action."), serde_json::json!({"type":"response.completed","response":{"id":"model-kill-response","status":"completed","output":[],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}})]);
                    let response = ResponseTemplate::new(200).insert_header("content-type", "text/event-stream").set_body_string(body);
                    if phase == "wire_arrival" { response.set_delay(Duration::from_secs(120)) } else { response }
                }).mount(&server).await;
                let mut process = Supervised(std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "child_graph_dispatch::model_dispatch_crash::model_dispatch_fault_process", "--ignored", "--nocapture"])
                    .env("VCP_MODEL_KILL_ROOT", &root).env("VCP_MODEL_KILL_BACKEND", backend)
                    .env("VCP_MODEL_KILL_ROLE", role).env("VCP_MODEL_KILL_PHASE", phase)
                    .env("VCP_MODEL_KILL_ENDPOINT", server.uri())
                    .stdin(Stdio::null()).stdout(fs::File::create(root.join("child-stdout.log")).unwrap())
                    .stderr(fs::File::create(root.join("child-stderr.log")).unwrap()).spawn().unwrap());
                let marker = root.join(if phase == "wire_arrival" {
                    "wire-0.json".into()
                } else {
                    format!("{phase}.json")
                });
                let deadline = Instant::now() + Duration::from_secs(60);
                while !marker.exists() {
                    assert!(
                        process.0.try_wait().unwrap().is_none(),
                        "{backend}/{role}/{phase}: owner exited early; retained logs at {}",
                        root.display()
                    );
                    assert!(
                        Instant::now() < deadline,
                        "{backend}/{role}/{phase}: barrier deadline; {}",
                        root.display()
                    );
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                let barrier: serde_json::Value =
                    serde_json::from_slice(&fs::read(&marker).unwrap()).unwrap();
                durable(
                    &root.join("supervisor-arrived.json"),
                    &serde_json::json!({"phase":phase,"role":role,"backend":backend,"child_pid":process.0.id(),"observed":barrier}),
                );
                process.terminate();
                durable(
                    &root.join("supervisor-killed.json"),
                    &serde_json::json!({"terminated":true,"phase":phase}),
                );
                let expected_requests = usize::from(phase != "before_transport");
                assert_eq!(count.load(Ordering::SeqCst), expected_requests);
                let setup: serde_json::Value =
                    serde_json::from_slice(&fs::read(root.join("setup.json")).unwrap()).unwrap();
                let config: Config = serde_json::from_value(setup["config"].clone()).unwrap();
                let task = TaskId::parse(setup["task"].as_str().unwrap()).unwrap();
                assert!(config
                    .canonical_root
                    .canonicalize()
                    .unwrap()
                    .starts_with(root.canonicalize().unwrap()));
                let before: State = serde_json::from_slice(
                    &fs::read(root.join(if phase == "before_settlement" {
                        "before_settlement-state.json"
                    } else {
                        "before_transport-state.json"
                    }))
                    .unwrap(),
                )
                .unwrap();
                let attempts: Vec<Attempt> = before
                    .records
                    .values()
                    .filter(|row| row.collection == Collection::Attempt)
                    .map(|row| row.decode().unwrap())
                    .collect();
                assert_eq!(attempts.len(), 1);
                let attempt = &attempts[0];
                assert_eq!(attempt.scope.task, task);
                assert_eq!(
                    attempt.role,
                    if role == "child" {
                        RequestRole::Child
                    } else {
                        RequestRole::Main
                    }
                );
                assert_eq!(attempt.phase, ReservationState::Submitted);
                assert!(attempt.send_intent.is_some());
                assert_eq!(attempt.charged, Micros::ZERO);
                assert!(attempt.quote.amount.micros > Micros::ZERO);
                assert_eq!(
                    before
                        .records
                        .values()
                        .filter(|row| row.collection == Collection::Settlement)
                        .count(),
                    0
                );
                if expected_requests == 1 {
                    let wire: serde_json::Value =
                        serde_json::from_slice(&fs::read(root.join("wire-0.json")).unwrap())
                            .unwrap();
                    assert_eq!(wire["body_sha256"], attempt.request_digest);
                }
                let mut previous_ledger = None;
                for reopen in 0..2 {
                    let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
                    let after = host.snapshot().unwrap();
                    let recovered: Attempt = after
                        .record(Collection::Attempt, attempt.id.as_str(), &config.workspace)
                        .unwrap()
                        .decode()
                        .unwrap();
                    assert_eq!(recovered.phase, ReservationState::ReconciliationPending);
                    assert!(recovered.uncertain.is_some());
                    assert_eq!(recovered.request, attempt.request);
                    assert_eq!(recovered.quote, attempt.quote);
                    assert_eq!(recovered.send_intent, attempt.send_intent);
                    assert_eq!(recovered.charged, Micros::ZERO);
                    assert_eq!(
                        after
                            .records
                            .values()
                            .filter(|row| row.collection == Collection::Attempt)
                            .count(),
                        1
                    );
                    assert_eq!(
                        after
                            .records
                            .values()
                            .filter(|row| row.collection == Collection::Settlement)
                            .count(),
                        0
                    );
                    let ledger = vcp_budget::ledger(&after, &attempt.scope).unwrap();
                    assert_eq!(ledger.active, Micros::ZERO);
                    assert_eq!(ledger.settled, Micros::ZERO);
                    assert_eq!(ledger.unresolved, attempt.quote.amount.micros);
                    if let Some(previous) = &previous_ledger {
                        assert_eq!(&ledger, previous);
                    }
                    previous_ledger = Some(ledger.clone());
                    for task_id in [&config.root_task, &task] {
                        let held: Task = after
                            .record(Collection::Task, task_id.as_str(), &config.workspace)
                            .unwrap()
                            .decode()
                            .unwrap();
                        assert_eq!(held.state, TaskState::Paused);
                    }
                    for (id, receipt) in &before.commands {
                        assert_eq!(after.commands.get(id), Some(receipt));
                    }
                    assert_eq!(
                        &after.events[..before.events.len()],
                        before.events.as_slice()
                    );
                    let mut response_artifacts = 0;
                    for row in before
                        .records
                        .values()
                        .filter(|row| row.collection == Collection::Artifact)
                    {
                        assert_eq!(
                            after
                                .record(Collection::Artifact, &row.id, &config.workspace)
                                .unwrap(),
                            row
                        );
                        let descriptor: ArtifactDescriptor = row.decode().unwrap();
                        let bytes = host.read_artifact(descriptor.spec.id.clone()).unwrap();
                        assert_eq!(vcp_protocol::digest_bytes(&bytes), descriptor.sha256);
                        if descriptor.spec.channel == Channel::Response {
                            response_artifacts += 1;
                            assert!(String::from_utf8(bytes)
                                .unwrap()
                                .contains("Ordinary retained model bytes"));
                        }
                    }
                    assert_eq!(
                        response_artifacts,
                        usize::from(phase == "before_settlement")
                    );
                    assert_eq!(
                        fs::read(Path::new(&config.binding.root).join("left.txt")).unwrap(),
                        b"base\n"
                    );
                    durable(
                        &root.join(format!("reopen-{reopen}.json")),
                        &serde_json::json!({"attempt":recovered,"ledger":ledger,"retained_response_artifacts":response_artifacts,"wire_requests":count.load(Ordering::SeqCst),"no_replay":true}),
                    );
                    owner.close().await.unwrap();
                }
                assert_eq!(count.load(Ordering::SeqCst), expected_requests);
                assert_eq!(
                    server.received_requests().await.unwrap().len(),
                    expected_requests
                );
                durable(
                    &root.join("result.json"),
                    &serde_json::json!({"pass":true,"backend":backend,"role":role,"phase":phase,"wire_requests":expected_requests,"liability_preserved":true,"reopens":2}),
                );
            }
        }
    }
}
