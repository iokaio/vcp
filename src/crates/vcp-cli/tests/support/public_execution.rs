// SPDX-License-Identifier: Apache-2.0
//! Actual controlled bridge/server execution with a synthetic loopback provider.
use super::*;
#[path = "local_fixture.rs"]
mod wire;
use std::time::{Duration, Instant};
use vcp_cli::settings::WorkspaceEntry;
use vcp_domain::task::{Task, TaskState};
use vcp_store::{contract::Collection, Store};

fn entry(fixture: &Fixture) -> WorkspaceEntry {
    let directory = vcp_cli::settings::workspace_directory(
        &fixture.data,
        &fixture.workspace.canonicalize().unwrap(),
    )
    .unwrap()
    .unwrap();
    serde_json::from_slice(&fs::read(directory.join("workspace.json")).unwrap()).unwrap()
}

fn scope(entry: &WorkspaceEntry) -> Value {
    json!({"workspace":entry.config.workspace,"session":entry.config.session})
}

fn launch(fixture: &Fixture) -> wire::Client {
    launch_transport(fixture, "stdio").0
}

fn launch_transport(fixture: &Fixture, transport: &str) -> (wire::Client, Value) {
    let mut client = wire::Client::spawn("local-bridge");
    client.send(json!({"schema":"vcp-local-bootstrap/1","workspace":fixture.workspace,
        "data":fixture.data,"role":"controller","transport":transport,"execution":{
            "profile":fixture.profile,"provider_credential":"synthetic-cli-qualification","credentials":{}}}));
    let ready = client.receive();
    assert_eq!(ready["schema"], "vcp-local-ready/1");
    assert!(!ready.to_string().contains("synthetic-cli-qualification"));
    initialize(&mut client);
    (client, ready)
}

fn initialize(client: &mut wire::Client) {
    let methods = [
        "session/resume",
        "task/read",
        "turn/pause",
        "controller/read",
        "controller/acquire",
        "command/read",
    ];
    let initialized = client.rpc(
        1,
        "initialize",
        json!({"protocol_version":"1.0",
        "client":{"name":"compiled-execution","version":"1"},
        "capabilities":methods,"required_capabilities":methods}),
    );
    assert!(initialized.get("error").is_none(), "{initialized}");
}

fn attach(attachment: &Value, role: &str) -> wire::Client {
    let mut client = wire::Client::spawn("local-bridge");
    client.send(json!({"schema":"vcp-local-attach/1","attachment":attachment,"role":role}));
    assert_eq!(client.receive()["schema"], "vcp-local-ready/1");
    initialize(&mut client);
    client
}

fn task(client: &mut wire::Client, entry: &WorkspaceEntry, id: u64) -> Value {
    let reply = client.rpc(
        id,
        "task/read",
        json!({"scope":scope(entry),"task":entry.config.root_task}),
    );
    assert!(reply.get("error").is_none(), "{reply}");
    reply["result"]["value"].clone()
}

async fn requests(count: &AtomicUsize, expected: usize) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while count.load(Ordering::SeqCst) < expected {
        assert!(
            Instant::now() < deadline,
            "retained provider request was not observed"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn acquire(client: &mut wire::Client, entry: &WorkspaceEntry, id: u64, command: &str) {
    let lease = client.rpc(id, "controller/read", json!({"scope":scope(entry)}));
    let view = &lease["result"]["value"];
    let expected = if view["ownership"] == "unclaimed" {
        Value::Null
    } else {
        view["revision"].clone()
    };
    wire::accepted(&client.rpc(
        id + 1,
        "controller/acquire",
        json!({"scope":scope(entry),"command_id":command,"expected_revision":expected}),
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_public_resume_submits_once_pause_stays_connected_and_restart_never_replays_effect(
) {
    let server = MockServer::start().await;
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(move |_: &wiremock::Request| {
            let index = calls.fetch_add(1, Ordering::SeqCst);
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(response(index, "complete"))
                .set_delay(if index == 0 {
                    Duration::ZERO
                } else {
                    Duration::from_secs(30)
                })
        })
        .mount(&server)
        .await;
    let fixture = Fixture::new(&server.uri(), "complete");
    fixture
        .paused_before_send("Change value.txt once and run its acceptance check")
        .await;
    assert_eq!(count.load(Ordering::SeqCst), 0);
    let entry = entry(&fixture);
    let mut client = launch(&fixture);
    let before = task(&mut client, &entry, 2);
    assert_eq!(before["state"], "paused");
    let resume = json!({"scope":scope(&entry),"task":entry.config.root_task,
        "mutation":{"command_id":"compiled-resume-once","expected_revision":before["revision"],
            "steering_revision":before["steering_revision"]}});
    assert!(client
        .rpc(3, "session/resume", resume.clone())
        .get("error")
        .is_some());
    assert_eq!(count.load(Ordering::SeqCst), 0);
    wire::accepted(&client.rpc(
        4,
        "controller/acquire",
        json!({"scope":scope(&entry),"command_id":"execution-owner","expected_revision":null}),
    ));
    assert_eq!(count.load(Ordering::SeqCst), 0);
    let accepted = client.rpc(5, "session/resume", resume.clone());
    wire::accepted(&accepted);
    assert_eq!(
        client.rpc(6, "session/resume", resume.clone())["result"],
        accepted["result"]
    );
    requests(&count, 2).await;
    assert_eq!(
        fs::read_to_string(fixture.workspace.join("value.txt"))
            .unwrap()
            .trim(),
        "42"
    );
    let active = task(&mut client, &entry, 7);
    assert_eq!(active["state"], "running");
    assert!(active["turn"].is_string(), "{active}");
    wire::accepted(&client.rpc(8,"turn/pause",json!({"scope":scope(&entry),"task":entry.config.root_task,
        "turn":active["turn"],"reason":"compiled connected pause",
        "mutation":{"command_id":"compiled-pause","expected_revision":active["revision"],"steering_revision":active["steering_revision"]}})));
    assert_eq!(task(&mut client, &entry, 9)["state"], "paused");
    assert_eq!(
        client.rpc(10, "controller/read", json!({"scope":scope(&entry)}))["result"]["value"]
            ["ownership"],
        "this_connection"
    );
    assert_eq!(
        client.rpc(11, "session/resume", resume.clone())["result"],
        accepted["result"]
    );
    assert_eq!(count.load(Ordering::SeqCst), 2);
    let (status, diagnostics) = client.finish().await;
    assert!(status.success(), "{diagnostics}");
    assert!(!diagnostics.contains("synthetic-cli-qualification"));

    let mut restarted = launch(&fixture);
    assert_eq!(task(&mut restarted, &entry, 2)["state"], "paused");
    assert_eq!(count.load(Ordering::SeqCst), 2);
    acquire(&mut restarted, &entry, 3, "restart-owner");
    assert_eq!(
        restarted.rpc(4, "session/resume", resume.clone())["result"],
        accepted["result"]
    );
    assert_eq!(task(&mut restarted, &entry, 5)["state"], "paused");
    let paused = task(&mut restarted, &entry, 6);
    let mut fresh = resume;
    fresh["mutation"]["command_id"] = json!("unreconciled-new-resume");
    fresh["mutation"]["expected_revision"] = paused["revision"].clone();
    fresh["mutation"]["steering_revision"] = paused["steering_revision"].clone();
    assert!(restarted
        .rpc(7, "session/resume", fresh)
        .get("error")
        .is_some());
    assert_eq!(task(&mut restarted, &entry, 8)["state"], "paused");
    assert_eq!(count.load(Ordering::SeqCst), 2);
    let (status, diagnostics) = restarted.finish().await;
    assert!(status.success(), "{diagnostics}");
    assert!(!diagnostics.contains("synthetic-cli-qualification"));
    let store = Store::open(&entry.config.canonical_root, entry.config.backend, &[])
        .await
        .unwrap();
    let durable: Task = store
        .state()
        .record(
            Collection::Task,
            entry.config.root_task.as_str(),
            &entry.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(durable.state, TaskState::Paused);
    assert_eq!(
        store
            .state()
            .commands
            .values()
            .filter(|receipt| receipt.command.as_str() == "compiled-resume-once")
            .count(),
        1
    );
    store.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_public_execution_owner_loss_and_pipe_reconnect_require_explicit_admission() {
    let server = MockServer::start().await;
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(move |_: &wiremock::Request| {
            calls.fetch_add(1, Ordering::SeqCst);
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(response(3, "complete"))
                .set_delay(Duration::from_secs(30))
        })
        .mount(&server)
        .await;
    let fixture = Fixture::new(&server.uri(), "complete");
    fixture
        .paused_before_send("Observe value.txt after explicit reconnect")
        .await;
    let entry = entry(&fixture);
    let (mut first, ready) = launch_transport(&fixture, "windows_pipe");
    let mut observer = attach(&ready["observer_attachment"], "observer");
    wire::accepted(&first.rpc(
        2,
        "controller/acquire",
        json!({"scope":scope(&entry),"command_id":"before-start-owner","expected_revision":null}),
    ));
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert!(first.finish().await.0.success());
    let paused = task(&mut observer, &entry, 2);
    assert_eq!(paused["state"], "paused");
    let request = json!({"scope":scope(&entry),"task":entry.config.root_task,
        "mutation":{"command_id":"pipe-execution-once","expected_revision":paused["revision"],"steering_revision":paused["steering_revision"]}});
    assert!(observer
        .rpc(3, "session/resume", request.clone())
        .get("error")
        .is_some());
    let mut owner = attach(&ready["attachment"], "controller");
    assert_eq!(count.load(Ordering::SeqCst), 0);
    acquire(&mut owner, &entry, 2, "after-start-owner");
    assert_eq!(count.load(Ordering::SeqCst), 0);
    let receipt = owner.rpc(3, "session/resume", request.clone());
    wire::accepted(&receipt);
    requests(&count, 1).await;
    assert!(owner.finish().await.0.success());
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut id = 4;
    loop {
        if task(&mut observer, &entry, id)["state"] == "paused" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "controller loss did not pause work"
        );
        id += 1;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut reattached = attach(&ready["attachment"], "controller");
    assert_eq!(count.load(Ordering::SeqCst), 1);
    acquire(&mut reattached, &entry, 2, "reconnected-owner");
    assert_eq!(
        reattached.rpc(3, "session/resume", request)["result"],
        receipt["result"]
    );
    assert_eq!(task(&mut reattached, &entry, 4)["state"], "paused");
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(
        fs::read_to_string(fixture.workspace.join("value.txt"))
            .unwrap()
            .trim(),
        "41"
    );
    assert!(reattached.finish().await.0.success());
    assert!(observer.finish().await.0.success());
    let deadline = Instant::now() + Duration::from_secs(40);
    let store = loop {
        match Store::open(&entry.config.canonical_root, entry.config.backend, &[]).await {
            Ok(store) => break store,
            Err(_) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await
            }
            Err(error) => panic!("idle server did not release writer: {error}"),
        }
    };
    let durable: Task = store
        .state()
        .record(
            Collection::Task,
            entry.config.root_task.as_str(),
            &entry.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(durable.state, TaskState::Paused);
    store.close().await.unwrap();
}
