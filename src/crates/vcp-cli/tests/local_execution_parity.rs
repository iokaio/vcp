// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! Same synthetic coding scenario through the real CLI and authenticated API.
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
    accounting::{Attempt, Ledger, Reservation, ReservationState, Settlement},
    effect::{Effect, EffectState},
    ids::TaskId,
    task::{Task, TaskState, Turn, TurnState},
    verification::{CheckOutcome, CostCertainty, Verification},
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
const OBJECTIVE: &str = "Change value to 42 and verify";

#[derive(Debug, PartialEq, Eq)]
struct SemanticOutcome {
    task: TaskSemantics,
    turns: Vec<(TurnState, u64)>,
    ledger: LedgerSemantics,
    attempts: Vec<Value>,
    reservations: Vec<Value>,
    settlements: Vec<Value>,
    effects: Vec<Value>,
    verification: Vec<Value>,
    file: Vec<u8>,
    provider_requests: usize,
}
#[derive(Debug, PartialEq, Eq)]
struct TaskSemantics {
    state: TaskState,
    steering: u64,
    objective: String,
    constraints: Vec<String>,
    acceptance: Vec<String>,
    editing: bool,
    checks: Vec<String>,
}
#[derive(Debug, PartialEq, Eq)]
struct LedgerSemantics {
    currency: String,
    cap: vcp_domain::Micros,
    protected: vcp_domain::Micros,
    settled: vcp_domain::Micros,
    active: vcp_domain::Micros,
    unresolved: vcp_domain::Micros,
    overrun: bool,
}
fn sorted(mut values: Vec<Value>) -> Vec<Value> {
    values.sort_by_cached_key(Value::to_string);
    values
}
async fn outcome(
    fixture: &Fixture,
    entry: &WorkspaceEntry,
    task_id: &TaskId,
    provider_requests: usize,
) -> SemanticOutcome {
    // These are independent process/file observations, not tool self-report.
    let file = fs::read(fixture.workspace.join("value.txt")).unwrap();
    assert_eq!(file, b"42\n");
    let mut check = Command::new(std::env::var_os("VCP_TEST_NODE").unwrap());
    check
        .args(["--test", "acceptance.cjs"])
        .current_dir(&fixture.workspace);
    let checked = tokio::time::timeout(
        Duration::from_secs(20),
        tokio::task::spawn_blocking(move || check.output().unwrap()),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    assert_eq!(
        provider_requests, 3,
        "one patch, one verification, one final response"
    );
    let store = reopen(entry).await;
    let state = store.state();
    let task: Task = state
        .record(Collection::Task, task_id.as_str(), &entry.config.workspace)
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(task.state, TaskState::Completed);
    assert_eq!(&task.root, task_id);
    assert_eq!(task.parent, None);
    assert_eq!(task.fork_origin, None);
    assert_eq!(task.objectives.len(), 1);
    let scoped = |value: &Value| value["scope"] == serde_json::to_value(&task.scope).unwrap();
    let mut attempts = Vec::new();
    let mut reservations = Vec::new();
    let mut settlements = Vec::new();
    let mut effects = Vec::new();
    let mut verification = Vec::new();
    let mut turns = Vec::new();
    let mut ledger = None;
    let mut current_verification = false;
    for record in state
        .records
        .values()
        .filter(|record| scoped(&record.value))
    {
        match record.collection {
            Collection::Turn => {
                let turn: Turn = record.decode().unwrap();
                assert_eq!(turn.state, TurnState::Completed);
                assert!(state
                    .record(
                        Collection::Artifact,
                        turn.trigger.as_str(),
                        &task.scope.workspace
                    )
                    .is_ok());
                turns.push((turn.state, turn.steering.get()));
            }
            Collection::Ledger => {
                let value: Ledger = record.decode().unwrap();
                assert!(value.allocations.is_empty());
                assert_eq!(value.active, vcp_domain::Micros::ZERO);
                assert_eq!(value.unresolved, vcp_domain::Micros::ZERO);
                assert!(!value.overrun);
                assert_eq!(value.settled.get(), 300);
                ledger = Some(LedgerSemantics {
                    currency: value.currency.code().into(),
                    cap: value.cap,
                    protected: value.protected,
                    settled: value.settled,
                    active: value.active,
                    unresolved: value.unresolved,
                    overrun: value.overrun,
                });
            }
            Collection::Attempt => {
                let value: Attempt = record.decode().unwrap();
                assert_eq!(value.phase, ReservationState::Settled);
                assert_eq!(&value.root, task_id);
                assert!(value.uncertain.is_none());
                let reservation: Reservation = state
                    .record(
                        Collection::Reservation,
                        value.reservation.as_str(),
                        &task.scope.workspace,
                    )
                    .unwrap()
                    .decode()
                    .unwrap();
                assert_eq!(reservation.attempt, value.id);
                assert_eq!(reservation.scope, task.scope);
                assert!(state
                    .record(
                        Collection::Artifact,
                        value.request.as_str(),
                        &task.scope.workspace
                    )
                    .is_ok());
                attempts.push(json!({"phase":value.phase,"role":value.role,"steering":value.steering,"charged":value.charged,"usage_watermark":value.usage_watermark,"quote":value.quote.amount}));
            }
            Collection::Reservation => {
                let value: Reservation = record.decode().unwrap();
                assert_eq!(value.phase, ReservationState::Settled);
                reservations.push(json!({"phase":value.phase,"role":value.role,"amount":value.amount,"charged":value.charged,"liability":value.liability,"protected_draw":value.protected_draw,"protected_returned":value.protected_returned}));
            }
            Collection::Settlement => {
                let value: Settlement = record.decode().unwrap();
                assert!(value.applied);
                assert!(state
                    .record(
                        Collection::Attempt,
                        value.attempt.as_str(),
                        &task.scope.workspace
                    )
                    .is_ok());
                assert!(state
                    .record(
                        Collection::Artifact,
                        value.observation.raw.as_str(),
                        &task.scope.workspace
                    )
                    .is_ok());
                settlements.push(json!({"applied":value.applied,"direction":value.direction,"adjustment":value.adjustment,"total":value.total,"amount":value.observation.amount,"final":value.observation.final_usage,"mode":value.observation.mode}));
            }
            Collection::Effect => {
                let value: Effect = record.decode().unwrap();
                assert_eq!(value.state, EffectState::Succeeded);
                let mut schemas = Vec::new();
                for artifact in &value.observed_changes {
                    let row = state
                        .record(
                            Collection::Artifact,
                            artifact.as_str(),
                            &task.scope.workspace,
                        )
                        .unwrap();
                    schemas.push(row.value["spec"]["schema"].clone());
                }
                effects.push(json!({"state":value.state,"steering":value.steering,"exit_code":value.exit_code,"observed_change_schemas":sorted(schemas)}));
            }
            Collection::Verification => {
                let value: Verification = record.decode().unwrap();
                assert!(value.unresolved_effects.is_empty());
                assert!(value.outstanding_issues.is_empty());
                assert!(matches!(value.cost, CostCertainty::Known));
                current_verification |=
                    value.applies(&task.scope, task.steering, &task.fingerprint)
                        && value.satisfies(&task.required_checks, task.editing);
                let checks:Vec<_>=value.checks.iter().map(|check| {
                    assert_eq!(check.outcome,CheckOutcome::Passed);
                    assert!(state.record(Collection::Artifact,check.output.as_str(),&task.scope.workspace).is_ok());
                    json!({"specification":check.specification,"outcome":check.outcome,"exit_code":check.exit_code})
                }).collect();
                verification.push(json!({"steering":value.steering,"checks":sorted(checks),"outputs":value.outputs.len(),"cost":"known"}));
            }
            _ => (),
        }
    }
    assert_eq!(turns.len(), 1);
    assert_eq!(attempts.len(), 3);
    assert_eq!(reservations.len(), 3);
    assert!(!effects.is_empty());
    assert!(
        current_verification,
        "completion requires evidence for the current fingerprint"
    );
    let objective = &task.objectives[0];
    let outcome = SemanticOutcome {
        task: TaskSemantics {
            state: task.state,
            steering: task.steering.get(),
            objective: objective.text.clone(),
            constraints: objective.constraints.clone(),
            acceptance: objective.acceptance.clone(),
            editing: task.editing,
            checks: task.required_checks.clone(),
        },
        turns,
        ledger: ledger.unwrap(),
        attempts: sorted(attempts),
        reservations: sorted(reservations),
        settlements: sorted(settlements),
        effects: sorted(effects),
        verification: sorted(verification),
        file,
        provider_requests,
    };
    store.close().await.unwrap();
    outcome
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn same_coding_scenario_has_cli_api_state_effect_and_accounting_parity() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let mut outcomes = Vec::new();
        for api in [false, true] {
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
                })
                .mount(&server)
                .await;
            let fixture = Fixture::new(&server.uri());
            let entry = fixture.seed(backend).await;
            assert_eq!(count.load(Ordering::SeqCst), 0);
            let selected = if api {
                let mut client = fixture.launch("controller", Some(ROOT));
                acquire(&mut client, &entry, "parity-controller");
                wire::accepted(&client.rpc(4, "turn/start", start(&entry)));
                let until = Instant::now() + Duration::from_secs(40);
                loop {
                    let view = task(&mut client, &entry, 5);
                    if view["state"] == "completed" {
                        break;
                    }
                    assert_eq!(view["state"], "running", "API coding completion: {view}");
                    assert!(Instant::now() < until, "API coding completion deadline");
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                assert!(client.finish().await.0.success());
                TaskId::parse(ROOT).unwrap()
            } else {
                let mut command = fixture.command(&["run", OBJECTIVE, "--autonomy", "autonomous"]);
                let output = tokio::time::timeout(
                    Duration::from_secs(90),
                    tokio::task::spawn_blocking(move || command.output().unwrap()),
                )
                .await
                .unwrap()
                .unwrap();
                let text = String::from_utf8(output.stdout).unwrap();
                assert!(!text.contains(SECRET));
                let frames: Vec<Value> = text
                    .lines()
                    .map(|line| serde_json::from_str(line).unwrap())
                    .collect();
                let result = frames
                    .iter()
                    .find(|frame| frame["type"] == "result")
                    .unwrap();
                assert!(
                    output.status.success(),
                    "CLI completion: {result}; {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                serde_json::from_value(result["scope"]["task"].clone()).unwrap()
            };
            assert_ne!(
                selected, entry.config.root_task,
                "same paused bootstrap remains outside compared execution scope"
            );
            outcomes.push(outcome(&fixture, &entry, &selected, count.load(Ordering::SeqCst)).await);
        }
        assert_eq!(
            outcomes[0], outcomes[1],
            "CLI/API selected-task semantic parity on {backend:?}"
        );
    }
}
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
    json!({"scope":scope(entry),"mutation":{"command_id":"compiled-start-once","expected_revision":"0","steering_revision":"0"},"task":ROOT,"turn":TURN,"objective":OBJECTIVE,"constraints":[],"acceptance":["changed source acceptance"],"budget":{"cap_micros":"1000000","currency":"USD","max_requests":8,"deadline_seconds":300}})
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
        fs::write(&profile,serde_json::to_vec(&json!({"version":1,"workspace":workspace,"trust_workspace":true,"maximum_autonomy":"autonomous","automatic_effects":["read","write","execute","network","install","publish","opaque"],"budget_usd":"1","provider":snapshot,"catalog":catalog,"affected_paths":["value.txt"],"max_requests":8,"deadline_seconds":300,"processes":[{"name":"node","executable":node,"environment":{"SystemRoot":std::env::var("SystemRoot").unwrap()},"required_isolation":[],"reduced_isolation":true,"inputs":[]}],"checks":[{"manifest":"package.json","runner":"node","profile":"node","expected_tests":["changed_value"],"rationale":"changed source acceptance"}],"qualification_endpoint":format!("{endpoint}/v1")})).unwrap()).unwrap();
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
        1 => Some(("vcp_verify", json!({"citations":[]}))),
        _ => None,
    };
    let item = if let Some((name, args)) = call {
        json!({"type":"function_call","id":format!("item-{index}"),"call_id":format!("call-{index}"),"name":name,"arguments":args.to_string(),"status":"completed"})
    } else {
        json!({"type":"message","id":format!("final-{index}"),"role":"assistant","status":"completed","content":[{"type":"output_text","text":"Observed completion.","annotations":[]}]})
    };
    [json!({"type":"response.output_item.done","output_index":0,"item":item}),json!({"type":"response.completed","response":{"id":format!("response-{index}"),"status":"completed","output":[item],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}})].into_iter().map(|event|format!("data: {event}\n\n")).collect()
}
