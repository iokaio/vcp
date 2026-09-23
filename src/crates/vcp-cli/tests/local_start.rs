// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! Caller-identified public starts through the compiled private bootstrap.
#[path = "support/local_fixture.rs"]
mod wire;
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
    ids::TaskId,
    task::{Task, TaskState, Turn},
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
    fn launch(&self, role: &str, root: Option<&str>) -> wire::Client {
        let mut client = wire::Client::spawn("local-bridge");
        let mut bootstrap = json!({"schema":"vcp-local-bootstrap/1","workspace":self.workspace,"data":self.data,"role":role});
        if role == "controller" {
            bootstrap["execution"] =
                json!({"profile":self.profile,"provider_credential":SECRET,"credentials":{}});
        }
        if let Some(root) = root {
            bootstrap["root_task"] = json!(root);
        }
        client.send(bootstrap);
        let ready = client.receive();
        assert_eq!(ready["schema"], "vcp-local-ready/1");
        assert!(!ready.to_string().contains(SECRET));
        let methods = [
            "turn/start",
            "task/read",
            "controller/read",
            "controller/acquire",
            "command/read",
        ];
        let required = if role == "controller" {
            methods.to_vec()
        } else {
            vec!["task/read", "command/read"]
        };
        let initialized=client.rpc(1,"initialize",json!({"protocol_version":"1.0","client":{"name":"compiled-start","version":"1"},"capabilities":methods,"required_capabilities":required}));
        assert!(initialized.get("error").is_none(), "{initialized}");
        client
    }
}
fn scope(entry: &WorkspaceEntry) -> Value {
    json!({"workspace":entry.config.workspace,"session":entry.config.session})
}
fn start(entry: &WorkspaceEntry) -> Value {
    json!({"scope":scope(entry),"mutation":{"command_id":"compiled-start-once","expected_revision":"0","steering_revision":"0"},"task":ROOT,"turn":TURN,"objective":"Change value.txt once and run its acceptance check","constraints":[],"acceptance":["value.txt contains 42"],"budget":{"cap_micros":"1000000","currency":"USD","max_requests":8,"deadline_seconds":300}})
}
fn acquire(client: &mut wire::Client, entry: &WorkspaceEntry, name: &str) {
    let lease = client.rpc(2, "controller/read", json!({"scope":scope(entry)}));
    assert!(lease.get("error").is_none(), "{lease}");
    wire::accepted(&client.rpc(3,"controller/acquire",json!({"scope":scope(entry),"command_id":name,"expected_revision":lease["result"]["value"]["revision"]})));
}
fn task(client: &mut wire::Client, entry: &WorkspaceEntry, id: u64) -> Value {
    let reply = client.rpc(id, "task/read", json!({"scope":scope(entry),"task":ROOT}));
    assert!(reply.get("error").is_none(), "{reply}");
    reply["result"]["value"].clone()
}
async fn requests(count: &AtomicUsize, expected: usize) {
    let until = Instant::now() + Duration::from_secs(15);
    while count.load(Ordering::SeqCst) < expected {
        assert!(
            Instant::now() < until,
            "expected provider dispatch not observed"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
async fn reopen(entry: &WorkspaceEntry) -> Store {
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        match Store::open(&entry.config.canonical_root, entry.config.backend, &[]).await {
            Ok(store) => return store,
            Err(error) if Instant::now() >= until => panic!("writer release: {error}"),
            Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_start_preserves_caller_identity_and_never_reexecutes_after_owner_loss() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
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
        let entry = fixture.seed(backend).await;
        assert_eq!(count.load(Ordering::SeqCst), 0);
        let request = start(&entry);
        let mut client = fixture.launch("controller", Some(ROOT));
        assert!(client
            .rpc(4, "turn/start", request.clone())
            .get("error")
            .is_some());
        acquire(&mut client, &entry, "start-owner-one");
        let mut mismatched = request.clone();
        mismatched["mutation"]["command_id"] = json!("wrong-budget");
        mismatched["budget"]["cap_micros"] = json!("999999");
        assert!(client
            .rpc(5, "turn/start", mismatched)
            .get("error")
            .is_some());
        assert_eq!(count.load(Ordering::SeqCst), 0);
        let accepted = client.rpc(6, "turn/start", request.clone());
        wire::accepted(&accepted);
        assert_eq!(accepted["result"]["value"]["task"], ROOT);
        assert_eq!(accepted["result"]["value"]["turn"], TURN);
        assert_eq!(
            client.rpc(7, "turn/start", request.clone())["result"],
            accepted["result"]
        );
        requests(&count, 2).await;
        assert_eq!(
            fs::read_to_string(fixture.workspace.join("value.txt")).unwrap(),
            "42\n"
        );
        assert_eq!(task(&mut client, &entry, 8)["turn"], TURN);
        let mut occupied = request.clone();
        occupied["mutation"]["command_id"] = json!("occupied-task");
        assert!(client.rpc(9, "turn/start", occupied).get("error").is_some());
        let mut changed = request.clone();
        changed["objective"] = json!("different objective with the same durable identity");
        assert_eq!(
            client.rpc(10, "turn/start", changed)["error"]["data"]["details"]["code"],
            "COMMAND_CONFLICT"
        );
        let (status, diagnostics) = client.finish().await;
        assert!(status.success(), "{diagnostics}");
        assert!(!diagnostics.contains(SECRET));
        let store = reopen(&entry).await;
        let canonical: Task = store
            .state()
            .record(Collection::Task, ROOT, &entry.config.workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(canonical.state, TaskState::Paused);
        assert_eq!(canonical.root, TaskId::parse(ROOT).unwrap());
        let turn: Turn = store
            .state()
            .record(Collection::Turn, TURN, &entry.config.workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(turn.scope, canonical.scope);
        let accepted_commands = store
            .state()
            .commands
            .values()
            .filter(|receipt| receipt.command.as_str() == "compiled-start-once")
            .count();
        assert_eq!(accepted_commands, 1);
        let receipt = store
            .state()
            .commands
            .values()
            .find(|receipt| receipt.command.as_str() == "compiled-start-once")
            .unwrap();
        let events: Vec<_> = store
            .state()
            .events
            .iter()
            .filter(|event| event.watermark == receipt.watermark)
            .collect();
        assert_eq!(events.len(), 4);
        let fact = |collection: &str| {
            events
                .iter()
                .flat_map(|event| event.event.data["facts"].as_array().into_iter().flatten())
                .find(|fact| fact["collection"] == collection)
                .unwrap()["value"]
                .clone()
        };
        let genesis: Task = serde_json::from_value(fact("task")).unwrap();
        let queued: Turn = serde_json::from_value(fact("turn")).unwrap();
        assert_eq!(genesis.state, TaskState::Pending);
        assert_eq!(queued.state, vcp_domain::task::TurnState::Queued);
        assert_eq!(queued.id.as_str(), TURN);
        assert_eq!(genesis.scope.task.as_str(), ROOT);
        assert!(events
            .iter()
            .all(|event| event.event.correlation == receipt.command));
        store.close().await.unwrap();
        let mut restarted = fixture.launch("controller", Some(ROOT));
        acquire(&mut restarted, &entry, "start-owner-two");
        assert_eq!(
            restarted.rpc(4, "turn/start", request.clone())["result"],
            accepted["result"]
        );
        assert_eq!(task(&mut restarted, &entry, 5)["state"], "paused");
        assert_eq!(count.load(Ordering::SeqCst), 2);
        assert!(restarted.finish().await.0.success());
        let mut observer = fixture.launch("observer", None);
        assert!(observer
            .rpc(4, "turn/start", request)
            .get("error")
            .is_some());
        assert_eq!(
            observer.rpc(
                5,
                "command/read",
                json!({"scope":scope(&entry),"command_id":"compiled-start-once"})
            )["result"],
            accepted["result"]
        );
        assert_eq!(count.load(Ordering::SeqCst), 2);
        assert!(observer.finish().await.0.success());
    }
}
struct Fixture {
    _temp: tempfile::TempDir,
    workspace: PathBuf,
    data: PathBuf,
    profile: PathBuf,
    binary: PathBuf,
}
impl Fixture {
    fn new(endpoint: &str, mode: &str) -> Self {
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
        fs::write(&profile,serde_json::to_vec(&json!({"version":1,"workspace":workspace,"trust_workspace":true,"maximum_autonomy":"autonomous","automatic_effects":if mode=="question"{vec!["read"]}else{vec!["read","write","execute","network","install","publish","opaque"]},"budget_usd":"1","provider":snapshot,"catalog":catalog,"affected_paths":["value.txt"],"max_requests":8,"deadline_seconds":300,"processes":[{"name":"node","executable":node,"environment":{"SystemRoot":std::env::var("SystemRoot").unwrap()},"required_isolation":[],"reduced_isolation":true,"inputs":[]}],"checks":[{"manifest":"package.json","runner":"node","profile":"node","expected_tests":["changed_value"],"rationale":"changed source acceptance"}],"qualification_endpoint":format!("{endpoint}/v1")})).unwrap()).unwrap();
        Self {
            _temp: temp,
            workspace,
            data,
            profile,
            binary: PathBuf::from(env!("CARGO_BIN_EXE_vcp")),
        }
    }
}
fn response(index: usize, mode: &str) -> String {
    let call = match index {
        0 => Some((
            "vcp_patch",
            json!({"patch":"*** Begin Patch\n*** Update File: value.txt\n@@\n-41\n+42\n*** End Patch"}),
        )),
        1 if mode != "incomplete" => Some(("vcp_verify", json!({"citations":[]}))),
        _ => None,
    };
    let item = if let Some((name, args)) = call {
        json!({"type":"function_call","id":format!("item-{index}"),"call_id":format!("call-{index}"),"name":name,"arguments":args.to_string(),"status":"completed"})
    } else {
        json!({"type":"message","id":format!("final-{index}"),"role":"assistant","status":"completed","content":[{"type":"output_text","text":"Observed completion.","annotations":[]}]})
    };
    [json!({"type":"response.output_item.done","output_index":0,"item":item}),json!({"type":"response.completed","response":{"id":format!("response-{index}"),"status":"completed","output":[item],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}})].into_iter().map(|event|format!("data: {event}\n\n")).collect()
}
