// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! Pending approval reconnect through a real compiled server and loopback provider.
#[path = "support/local_fixture.rs"]
mod wire;
use base64::Engine as _;
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use vcp_cli::settings::WorkspaceEntry;
use vcp_domain::{
    task::{Task, TaskState},
    Timestamp,
};
use vcp_models::catalog::{Compatibility, Snapshot};
use vcp_store::{contract::Collection, BackendKind, Store};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};
const ROOT: &str = "caller-start-root";
const TURN: &str = "caller-start-turn";
const SECRET: &str = "synthetic-cli-qualification";
impl Fixture {
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(&self.binary);
        command
            .args(["--format", "jsonl", "--non-interactive", "--workspace"])
            .arg(&self.workspace)
            .arg("--data-dir")
            .arg(&self.data)
            .arg("--config")
            .arg(&self.profile)
            .args(args)
            .env_remove("RUST_MIN_STACK")
            .env("OPENROUTER_API_KEY", SECRET);
        command
    }
    async fn seed(&self, backend: BackendKind) -> WorkspaceEntry {
        let name = match backend {
            BackendKind::Files => "files",
            BackendKind::Sqlite => "sqlite",
        };
        let mut configure = self.command(&["storage", "configure", "--backend", name]);
        let output = tokio::task::spawn_blocking(move || configure.output().unwrap())
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        // The ordinary executable bootstraps its real policy and descriptor,
        // then loses its stdout consumer before any provider admission.
        let mut child = self
            .command(&[
                "run",
                "Seed local public-start qualification",
                "--autonomy",
                "autonomous",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        drop(child.stdout.take());
        let output = tokio::time::timeout(
            Duration::from_secs(90),
            tokio::task::spawn_blocking(move || child.wait_with_output().unwrap()),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(output.status.code(), Some(1));
        let directory = vcp_cli::settings::workspace_directory(
            &self.data,
            &self.workspace.canonicalize().unwrap(),
        )
        .unwrap()
        .unwrap();
        let entry: WorkspaceEntry =
            serde_json::from_slice(&fs::read(directory.join("workspace.json")).unwrap()).unwrap();
        assert_eq!(entry.config.backend, backend);
        entry
    }
}
fn scope(entry: &WorkspaceEntry) -> Value {
    json!({"workspace":entry.config.workspace,"session":entry.config.session})
}
fn initialize(client: &mut wire::Client) {
    let methods = [
        "turn/start",
        "task/read",
        "approval/respond",
        "approval/source-revisions/1",
        "controller/read",
        "controller/acquire",
        "command/read",
        "diff/read",
        "artifact/read",
        "session/snapshot",
        "events/next",
    ];
    let reply = client.rpc(1, "initialize", json!({"protocol_version":"1.0","client":{"name":"pending-input-qualification","version":"1"},"capabilities":methods,"required_capabilities":methods}));
    assert!(reply.get("error").is_none(), "{reply}");
}
fn attach(attachment: &Value, role: &str) -> wire::Client {
    let mut client = wire::Client::spawn("local-bridge");
    client.send(json!({"schema":"vcp-local-attach/1","attachment":attachment,"role":role}));
    assert_eq!(client.receive()["schema"], "vcp-local-ready/1");
    initialize(&mut client);
    client
}
fn task(client: &mut wire::Client, entry: &WorkspaceEntry) -> Value {
    let reply = client.rpc(90, "task/read", json!({"scope":scope(entry),"task":ROOT}));
    assert!(reply.get("error").is_none(), "{reply}");
    reply["result"]["value"].clone()
}
fn controller(client: &mut wire::Client, entry: &WorkspaceEntry) -> Value {
    let reply = client.rpc(91, "controller/read", json!({"scope":scope(entry)}));
    assert!(reply.get("error").is_none(), "{reply}");
    reply["result"]["value"].clone()
}
fn acquire(client: &mut wire::Client, entry: &WorkspaceEntry, command: &str) {
    let lease = controller(client, entry);
    wire::accepted(&client.rpc(
        2,
        "controller/acquire",
        json!({"scope":scope(entry),"command_id":command,"expected_revision":lease["revision"]}),
    ));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn real_pending_approval_survives_connection_loss_and_requires_current_controller() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let server = MockServer::start().await;
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .respond_with(move |_: &wiremock::Request| {
                let index = calls.fetch_add(1, Ordering::SeqCst);
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(response(index))
                    .set_delay(if index == 0 {
                        Duration::ZERO
                    } else {
                        Duration::from_secs(30)
                    })
            })
            .mount(&server)
            .await;
        let fixture = Fixture::new(&server.uri());
        let entry = fixture.seed(backend).await;
        assert_eq!(count.load(Ordering::SeqCst), 0);
        let mut first = wire::Client::spawn("local-bridge");
        first.send(json!({"schema":"vcp-local-bootstrap/1","workspace":fixture.workspace,"data":fixture.data,"role":"controller","transport":"windows_pipe","root_task":ROOT,"execution":{"profile":fixture.profile,"provider_credential":SECRET,"credentials":{}}}));
        let ready = first.receive();
        assert_eq!(ready["schema"], "vcp-local-ready/1");
        assert!(!ready.to_string().contains(SECRET));
        initialize(&mut first);
        acquire(&mut first, &entry, "question-first-controller");
        let snapshot = first.rpc(
            101,
            "session/snapshot",
            json!({"scope":scope(&entry),"limit":128,"cursor":null}),
        );
        assert_eq!(snapshot["result"]["kind"], "snapshot", "{snapshot}");
        wire::accepted(&first.rpc(3,"turn/start",json!({"scope":scope(&entry),"mutation":{"command_id":"question-run","expected_revision":"0","steering_revision":"0"},"task":ROOT,"turn":TURN,"objective":"Change value.txt once and verify it","constraints":[],"acceptance":["value.txt contains 42"],"budget":{"cap_micros":"1000000","currency":"USD","max_requests":8,"deadline_seconds":300}})));
        let deadline = Instant::now() + Duration::from_secs(20);
        let pending = loop {
            let view = task(&mut first, &entry);
            if view["pending_inputs"]
                .as_array()
                .is_some_and(|inputs| !inputs.is_empty())
            {
                break view;
            }
            assert!(
                Instant::now() < deadline,
                "real patch must produce canonical pending approval"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        let question = pending["pending_inputs"][0].clone();
        let proposal = discover_diff(&mut first, &entry, &snapshot["result"]["value"]);
        assert_eq!(proposal["schema"], "vcp-public-diff/1");
        assert_eq!(proposal["disposition"], "proposed");
        assert_eq!(proposal["scope"]["task"], ROOT);
        assert_eq!(proposal["operation_digest"], question["operation_digest"]);
        assert_eq!(proposal["files"][0]["path"], "value.txt");
        assert_eq!(proposal["files"][0]["before"]["content_base64"], "NDEK");
        assert_eq!(proposal["files"][0]["after"]["content_base64"], "NDIK");
        assert!(proposal.get("controller").is_none());
        assert!(proposal.get("owner").is_none());
        assert_eq!(question["kind"], "approval");
        assert_eq!(pending["task"], ROOT);
        assert_eq!(pending["turn"], TURN);
        assert!(question["operation_digest"]
            .as_str()
            .is_some_and(|digest| !digest.is_empty()));
        // All response preconditions must be supplied by the public projection,
        // not guessed from fixture internals or read through the canonical store.
        assert!(
            question["effect_revision"].is_string(),
            "approval source revision must be public"
        );
        assert!(
            question["policy_revision"].is_string(),
            "approval policy revision must be public"
        );
        let answer = json!({"scope":scope(&entry),"mutation":{"command_id":"question-denied-once","expected_revision":question["revision"],"steering_revision":pending["steering_revision"]},"task":ROOT,"approval":question["id"],"operation_digest":question["operation_digest"],"effect_revision":question["effect_revision"],"policy_revision":question["policy_revision"],"decision":"deny"});
        let mut observer = attach(&ready["observer_attachment"], "observer");
        assert_eq!(
            task(&mut observer, &entry)["pending_inputs"],
            pending["pending_inputs"]
        );
        assert!(observer
            .rpc(3, "approval/respond", answer.clone())
            .get("error")
            .is_some());
        assert!(first.finish().await.0.success());
        let deadline = Instant::now() + Duration::from_secs(15);
        while controller(&mut observer, &entry)["ownership"] != "released" {
            assert!(
                Instant::now() < deadline,
                "disconnect must finish releasing lease"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let paused = task(&mut observer, &entry);
        assert_eq!(paused["state"], "paused");
        assert_eq!(paused["pending_inputs"], pending["pending_inputs"]);
        let after_loss = count.load(Ordering::SeqCst);
        // The read must remain historical after an independent workspace edit.
        fs::write(fixture.workspace.join("value.txt"), "external edit\n").unwrap();
        assert_eq!(
            read_diff(&mut observer, &entry, &proposal["change"]),
            proposal
        );
        fs::write(fixture.workspace.join("value.txt"), "41\n").unwrap();
        let mut replacement = attach(&ready["attachment"], "controller");
        assert!(replacement
            .rpc(3, "approval/respond", answer.clone())
            .get("error")
            .is_some());
        acquire(&mut replacement, &entry, "question-replacement-controller");
        let mut stale = answer.clone();
        stale["mutation"]["command_id"] = json!("question-stale-source");
        stale["effect_revision"] = json!("18446744073709551615");
        assert!(replacement
            .rpc(4, "approval/respond", stale)
            .get("error")
            .is_some());
        let accepted = replacement.rpc(5, "approval/respond", answer.clone());
        wire::accepted(&accepted);
        assert_eq!(
            replacement.rpc(6, "approval/respond", answer)["result"],
            accepted["result"]
        );
        assert_eq!(
            observer.rpc(
                7,
                "command/read",
                json!({"scope":scope(&entry),"command_id":"question-denied-once"})
            )["result"],
            accepted["result"]
        );
        let answered = task(&mut observer, &entry);
        assert_eq!(answered["state"], "paused");
        assert!(answered["pending_inputs"].as_array().unwrap().is_empty());
        assert_eq!(
            read_diff(&mut observer, &entry, &proposal["change"]),
            proposal
        );
        assert_eq!(count.load(Ordering::SeqCst), after_loss);
        assert_eq!(
            fs::read_to_string(fixture.workspace.join("value.txt")).unwrap(),
            "41\n"
        );
        assert!(replacement.finish().await.0.success());
        assert!(observer.finish().await.0.success());
        let deadline = Instant::now() + Duration::from_secs(45);
        let store = loop {
            match Store::open(&entry.config.canonical_root, backend, &[]).await {
                Ok(store) => break store,
                Err(error) if Instant::now() >= deadline => panic!("writer release: {error}"),
                Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        };
        assert_eq!(
            store
                .state()
                .commands
                .values()
                .filter(|r| r.command.as_str() == "question-denied-once")
                .count(),
            1
        );
        assert!(!store
            .state()
            .commands
            .values()
            .any(|r| r.command.as_str() == "question-stale-source"));
        let canonical: Task = store
            .state()
            .record(Collection::Task, ROOT, &entry.config.workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(canonical.state, TaskState::Paused);
        assert!(!store
            .state()
            .records
            .values()
            .any(|r| r.collection == Collection::Approval && r.value["state"] == "pending"));
        store.close().await.unwrap();
    }
}
fn decode_range(reply: &Value) -> Vec<u8> {
    assert_eq!(reply["result"]["kind"], "artifact", "{reply}");
    let range = &reply["result"]["value"];
    assert_eq!(range["encoding"], "base64");
    base64::engine::general_purpose::STANDARD
        .decode(range["content"].as_str().unwrap())
        .unwrap()
}
fn read_diff(client: &mut wire::Client, entry: &WorkspaceEntry, change: &Value) -> Value {
    let reply = client.rpc(
        105,
        "diff/read",
        json!({"scope":scope(entry),"task":ROOT,"change":change,"offset":"0","length":49152}),
    );
    let bytes = decode_range(&reply);
    assert_eq!(reply["result"]["value"]["complete"], true);
    assert_eq!(
        reply["result"]["value"]["sha256"],
        vcp_protocol::digest_bytes(&bytes)
    );
    serde_json::from_slice(&bytes).unwrap()
}
fn discover_diff(client: &mut wire::Client, entry: &WorkspaceEntry, snapshot: &Value) -> Value {
    let mut cursor = snapshot["event_cursor"].clone();
    let mut seen = BTreeSet::new();
    for _ in 0..16 {
        let reply = client.rpc(
            102,
            "events/next",
            json!({"scope":scope(entry),"subscription":snapshot["subscription"],"cursor":cursor}),
        );
        assert_eq!(reply["result"]["kind"], "events", "{reply}");
        let batch = &reply["result"]["value"];
        for event in batch["events"].as_array().unwrap() {
            if event["task"] != ROOT {
                continue;
            }
            for evidence in event["evidence"].as_array().unwrap() {
                let artifact = evidence["artifact"].as_str().unwrap();
                if !seen.insert(artifact.to_owned()) {
                    continue;
                }
                let bytes = client.rpc(103, "artifact/read", json!({"scope":scope(entry),"task":ROOT,"artifact":artifact,"offset":"0","length":49152}));
                if let Ok(document) = serde_json::from_slice::<Value>(&decode_range(&bytes)) {
                    if document["schema"] == "vcp-public-diff/1" {
                        assert_eq!(read_diff(client, entry, &document["change"]), document);
                        return document;
                    }
                }
            }
        }
        assert_ne!(
            batch["at_end"], true,
            "public diff must be discoverable from retained event evidence"
        );
        cursor = batch["cursor"].clone();
    }
    panic!("bounded fixture event history must include the public proposal");
}
struct Fixture {
    _temp: tempfile::TempDir,
    workspace: PathBuf,
    data: PathBuf,
    profile: PathBuf,
    binary: PathBuf,
}
impl Fixture {
    fn new(endpoint: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let data = temp.path().join("data");
        fs::create_dir(&workspace).unwrap();
        fs::create_dir(&data).unwrap();
        fs::write(workspace.join("value.txt"), "41\n").unwrap();
        fs::write(
            workspace.join("package.json"),
            r#"{"scripts":{"test":"node --test acceptance.cjs"}}"#,
        )
        .unwrap();
        fs::write(workspace.join("acceptance.cjs"),"const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs');test('changed_value',()=>assert.equal(fs.readFileSync('value.txt','utf8').trim(),'42'));\n").unwrap();
        let raw=serde_json::to_vec(&json!({"data":{"id":"fixture/model","endpoints":[{"tag":"fixture/region","status":0,"context_length":500000,"max_prompt_tokens":400000,"max_completion_tokens":8000,"supported_parameters":["tools","max_tokens"],"pricing":{"prompt":"0","completion":"0","request":"0.0001"}}]}})).unwrap();
        let snapshot = Snapshot::from_endpoints(
            &raw,
            Timestamp::ZERO,
            Timestamp::new(u64::MAX),
            Compatibility {
                id: "synthetic-cli/1".into(),
                model: "fixture/model".into(),
                endpoint: "fixture/region".into(),
                qualified_at: Timestamp::ZERO,
                valid_until: Timestamp::new(u64::MAX),
                responses_text_tools: true,
                byte_ceiling_qualified: true,
                provider_preferences_qualified: true,
                deny_data_collection: true,
                require_zdr: true,
                request_price_limit: "0.0001".into(),
                required_parameters: BTreeSet::from(["tools".into(), "max_tokens".into()]),
                qualified_reasoning_efforts: BTreeSet::new(),
            },
        )
        .unwrap();
        let catalog = data.join("catalog.json");
        fs::write(&catalog, raw).unwrap();
        let profile = data.join("profile.json");
        let node = temp.path().join("node.exe");
        fs::copy(
            std::env::var_os("VCP_TEST_NODE").expect("native Node required"),
            &node,
        )
        .unwrap();
        fs::write(&profile,serde_json::to_vec(&json!({"version":1,"workspace":workspace,"trust_workspace":true,"maximum_autonomy":"autonomous","automatic_effects":["read"],"budget_usd":"1","provider":snapshot,"catalog":catalog,"affected_paths":["value.txt"],"max_requests":8,"deadline_seconds":300,"processes":[{"name":"node","executable":node,"environment":{"SystemRoot":std::env::var("SystemRoot").unwrap()},"required_isolation":[],"reduced_isolation":true,"inputs":[]}],"checks":[{"manifest":"package.json","runner":"node","profile":"node","expected_tests":["changed_value"],"rationale":"changed source acceptance"}],"qualification_endpoint":format!("{endpoint}/v1")})).unwrap()).unwrap();
        Self {
            _temp: temp,
            workspace,
            data,
            profile,
            binary: PathBuf::from(env!("CARGO_BIN_EXE_vcp")),
        }
    }
}
fn response(index: usize) -> String {
    let call = match index {
        0 => Some((
            "vcp_patch",
            json!({"patch":"*** Begin Patch\n*** Update File: value.txt\n@@\n-41\n+42\n*** End Patch"}),
        )),
        _ => None,
    };
    let item = if let Some((name, args)) = call {
        json!({"type":"function_call","id":format!("item-{index}"),"call_id":format!("call-{index}"),"name":name,"arguments":args.to_string(),"status":"completed"})
    } else {
        json!({"type":"message","id":format!("final-{index}"),"role":"assistant","status":"completed","content":[{"type":"output_text","text":"Observed completion.","annotations":[]}]})
    };
    [json!({"type":"response.output_item.done","output_index":0,"item":item}),json!({"type":"response.completed","response":{"id":format!("response-{index}"),"status":"completed","output":[item],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}})].into_iter().map(|event|format!("data: {event}\n\n")).collect()
}
