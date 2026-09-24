// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
#[path = "support/child_output_owner.rs"]
mod child_output_owner;
#[path = "support/history_notice.rs"]
mod history_notice;
#[path = "support/hooks.rs"]
mod hooks;
#[path = "support/live_adapter.rs"]
mod live_adapter;
#[path = "support/packaged_crypto.rs"]
mod packaged_crypto;
#[path = "support/packaged_history.rs"]
mod packaged_history;
#[path = "support/packaged_long_check.rs"]
mod packaged_long_check;
#[path = "support/packaged_mcp_history.rs"]
mod packaged_mcp_history;
#[path = "support/packaged_mcp_preflight.rs"]
mod packaged_mcp_preflight;
#[path = "support/packaged_sensitive_surfaces.rs"]
mod packaged_sensitive_surfaces;
#[path = "support/public_execution.rs"]
mod public_execution;
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use vcp_domain::Timestamp;
use vcp_models::catalog::{Compatibility, Snapshot};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

struct Fixture {
    _temp: tempfile::TempDir,
    workspace: PathBuf,
    data: PathBuf,
    profile: PathBuf,
    binary: PathBuf,
}
impl Fixture {
    fn maximum_skill_sources(&self) {
        let mut profile: Value = serde_json::from_slice(&fs::read(&self.profile).unwrap()).unwrap();
        profile["skills"] = json!({"version":1,"revision":"0","sources":(0..32).map(|index| json!({
            "id":format!("source-{index}"),"kind":"workspace","enabled":false,
            "root_id":vcp_domain::RootId::new(),"path":self.workspace,
        })).collect::<Vec<_>>()});
        fs::write(&self.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
    }
    fn package(&mut self, assets: bool) -> PathBuf {
        // Qualification-only input for exercising an independently built and
        // extracted archive. Production asset lookup has no environment override.
        let archive = std::env::var_os("VCP_TEST_SKILL_PACKAGE").map(PathBuf::from);
        let compiled = PathBuf::from(env!("CARGO_BIN_EXE_vcp"));
        let executable = archive
            .as_ref()
            .map_or_else(|| compiled.clone(), |path| path.join("vcp.exe"));
        if archive.is_some() {
            assert_eq!(
                vcp_protocol::digest_bytes(&fs::read(&executable).unwrap()),
                vcp_protocol::digest_bytes(&fs::read(&compiled).unwrap()),
                "archive executable must match the qualified build"
            );
        }
        let installation = self._temp.path().join("installed");
        fs::create_dir_all(&installation).unwrap();
        fs::copy(executable, installation.join("vcp.exe")).unwrap();
        if assets {
            let frozen = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/builtin");
            let source = archive
                .as_ref()
                .map_or_else(|| frozen.clone(), |path| path.join("skills/builtin"));
            assert_eq!(
                fs::read(source.join("catalog.json")).unwrap(),
                fs::read(frozen.join("catalog.json")).unwrap(),
                "archive catalog must match the frozen inventory"
            );
            let target = installation.join("skills/builtin");
            fs::create_dir_all(&target).unwrap();
            let catalog: Value =
                serde_json::from_slice(&fs::read(source.join("catalog.json")).unwrap()).unwrap();
            for name in ["catalog.json", "coverage.json"] {
                fs::copy(source.join(name), target.join(name)).unwrap();
            }
            for skill in catalog["skills"].as_array().unwrap() {
                let descriptor = PathBuf::from(skill["descriptor"].as_str().unwrap());
                let package = descriptor.parent().unwrap();
                fs::create_dir_all(target.join(package)).unwrap();
                fs::copy(source.join(&descriptor), target.join(&descriptor)).unwrap();
                let metadata: Value =
                    serde_json::from_slice(&fs::read(source.join(&descriptor)).unwrap()).unwrap();
                for content in std::iter::once(&metadata["body"])
                    .chain(metadata["resources"].as_array().unwrap())
                {
                    let relative = package.join(content["path"].as_str().unwrap());
                    fs::create_dir_all(target.join(&relative).parent().unwrap()).unwrap();
                    fs::copy(source.join(&relative), target.join(&relative)).unwrap();
                }
            }
        }
        // Resolve from the running executable after the complete tree moves.
        let relocated = self._temp.path().join("relocated package");
        fs::rename(&installation, &relocated).unwrap();
        self.binary = relocated.join("vcp.exe");
        relocated.join("skills/builtin")
    }
    fn skills(&self) -> PathBuf {
        let collection = self.workspace.join(".vcp-skills");
        let package = collection.join("review");
        fs::create_dir_all(&package).unwrap();
        let body = b"Use the observed project instructions. Report verification results without granting permissions.";
        fs::write(package.join("SKILL.md"), body).unwrap();
        fs::write(package.join("skill.json"),serde_json::to_vec(&json!({
            "schema_version":1,"id":"review","version":"1.2.0","description":"Review the current JavaScript project.",
            "source":"VCP original test fixture","license":"Apache-2.0","vcp_version":1,
            "cues":["package.json"],"environments":["windows"],"required_tools":["vcp_read"],
            "body":{"path":"SKILL.md","sha256":vcp_protocol::digest_bytes(body)},"resources":[]
        })).unwrap()).unwrap();
        let mut profile: Value = serde_json::from_slice(&fs::read(&self.profile).unwrap()).unwrap();
        profile["skills"] = json!({"version":1,"revision":"0","sources":[{"id":"project","root_id":vcp_domain::RootId::new(),"kind":"workspace","enabled":true,"path":collection}]});
        fs::write(&self.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
        package
    }
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
    async fn terminal(&self, objective: &str) -> codex_utils_pty::SpawnedProcess {
        self.terminal_args(&["run", objective, "--autonomy", "autonomous"])
            .await
    }
    async fn terminal_args(&self, command: &[&str]) -> codex_utils_pty::SpawnedProcess {
        use codex_utils_pty::{spawn_pty_process, TerminalSize};
        use std::collections::HashMap;
        let mut environment: HashMap<String, String> = std::env::vars().collect();
        environment.remove("RUST_MIN_STACK");
        environment.insert(
            "OPENROUTER_API_KEY".into(),
            "synthetic-cli-qualification".into(),
        );
        let mut args: Vec<String> = vec![
            "--workspace".into(),
            self.workspace.to_string_lossy().into_owned(),
            "--data-dir".into(),
            self.data.to_string_lossy().into_owned(),
            "--config".into(),
            self.profile.to_string_lossy().into_owned(),
        ];
        args.extend(command.iter().map(|argument| (*argument).to_owned()));
        spawn_pty_process(
            self.binary.to_str().unwrap(),
            &args,
            &self.workspace,
            &environment,
            &None,
            TerminalSize {
                rows: 30,
                cols: 100,
            },
            &[],
        )
        .await
        .unwrap()
    }
    fn task_id(&self) -> String {
        let directory = fs::read_dir(self.data.join("workspaces"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let descriptor: Value =
            serde_json::from_slice(&fs::read(directory.join("workspace.json")).unwrap()).unwrap();
        descriptor["config"]["root_task"]
            .as_str()
            .unwrap()
            .to_owned()
    }
    async fn approval(&self, task: &str, id: Option<&str>) -> Option<Value> {
        let output = self
            .run(&["inspect", task, "--view", "policy", "--limit", "128"])
            .await;
        let values = records(&output);
        values[0]["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["collection"] == "approval" && id.is_none_or(|id| item["id"] == id))
            .map(|item| item["record"].clone())
    }
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
            .env("OPENROUTER_API_KEY", "synthetic-cli-qualification");
        command
    }
    async fn run(&self, args: &[&str]) -> Output {
        let mut command = self.command(args);
        tokio::time::timeout(
            std::time::Duration::from_secs(90),
            tokio::task::spawn_blocking(move || command.output().unwrap()),
        )
        .await
        .expect("CLI subprocess deadline")
        .unwrap()
    }

    async fn paused_before_send(&self, objective: &str) -> String {
        let mut child = self
            .command(&["run", objective, "--autonomy", "autonomous"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        drop(child.stdout.take());
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(90),
            tokio::task::spawn_blocking(move || child.wait_with_output().unwrap()),
        )
        .await
        .expect("closed consumer pause deadline")
        .unwrap();
        assert_eq!(output.status.code(), Some(1));
        let task = self.task_id();
        let status = self.run(&["tasks", "status", &task]).await;
        assert_eq!(records(&status)[0]["data"]["records"][0]["state"], "paused");
        task
    }
}
fn records(output: &Output) -> Vec<Value> {
    use std::io::Write;
    let mut consumer = Command::new(std::env::var_os("VCP_TEST_NODE").unwrap())
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/consume-executable.cjs"
        ))
        .arg(output.status.code().unwrap().to_string())
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    consumer
        .stdin
        .take()
        .unwrap()
        .write_all(&output.stdout)
        .unwrap();
    let consumed = consumer.wait_with_output().unwrap();
    assert!(
        consumed.status.success(),
        "{}",
        String::from_utf8_lossy(&consumed.stderr)
    );
    let text = String::from_utf8(output.stdout.clone()).unwrap();
    assert!(!text.contains("synthetic-cli-qualification"));
    let values: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        values.iter().filter(|v| v["type"] == "result").count(),
        1,
        "stderr: {} stdout: {text}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(values.iter().all(|v| v["schema_version"] == 1));
    values
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_run_skill_activation_precedes_first_provider_request() {
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let responses = calls.clone();
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(move |_: &wiremock::Request| {
            let index = responses.fetch_add(1, Ordering::SeqCst);
            // Verification may first refresh its instruction scope. The next
            // request has that refreshed context and explicitly retries the check.
            let body = if index == 2 {
                response(1, "complete")
                    .replace("item-1", "item-2")
                    .replace("call-1", "call-2")
                    .replace("response-1", "response-2")
            } else {
                response(index, "complete")
            };
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(body)
        })
        .mount(&server)
        .await;
    let fixture = Fixture::new(&server.uri(), "complete");
    fixture.skills();
    let output = fixture
        .run(&[
            "run",
            "Change value to 42 and verify",
            "--autonomy",
            "autonomous",
            "--skill",
            "project::review::review",
        ])
        .await;
    let values = records(&output);
    let requests = server.received_requests().await.unwrap();
    let tool_results: Vec<Value> = requests
        .iter()
        .flat_map(|request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            body["input"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|item| item["type"] == "function_call_output")
                .cloned()
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        output.status.success(),
        "final={:?} tool_results={tool_results:?} {}",
        values.last(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!requests.is_empty());
    let first = String::from_utf8_lossy(&requests[0].body);
    assert!(first.contains("Use the observed project instructions. Report verification results without granting permissions."));
    assert!(first.contains("project::review::review"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_run_unknown_skill_stops_before_provider_dispatch() {
    let server = MockServer::start().await;
    let fixture = Fixture::new(&server.uri(), "complete");
    fixture.skills();
    let output = fixture
        .run(&[
            "run",
            "Change value to 42 and verify",
            "--autonomy",
            "autonomous",
            "--skill",
            "project::review::missing",
        ])
        .await;
    records(&output);
    assert!(!output.status.success());
    assert!(server.received_requests().await.unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(fixture.workspace.join("value.txt")).unwrap(),
        "41\n"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_owner_controls_authenticate_and_cancel_inflight_work() {
    use std::io::{BufRead, BufReader};
    use vcp_cli::control::{self, Request};
    use vcp_protocol::command::CommandEnvelope;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(response(0, "complete"))
                .set_delay(std::time::Duration::from_secs(30)),
        )
        .mount(&server)
        .await;
    let fixture = Fixture::new(&server.uri(), "complete");
    let mut child = fixture
        .command(&[
            "run",
            "Change value to 42 and verify",
            "--autonomy",
            "autonomous",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            send.send(line.unwrap()).unwrap();
        }
    });
    let accepted: Value = serde_json::from_str(
        &tokio::time::timeout(std::time::Duration::from_secs(30), receive.recv())
            .await
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let workspace: vcp_domain::ids::WorkspaceId =
        serde_json::from_value(accepted["scope"]["workspace"].clone()).unwrap();
    let task: vcp_domain::ids::TaskId =
        serde_json::from_value(accepted["scope"]["task"].clone()).unwrap();
    let key = vcp_protocol::digest_bytes(
        fixture
            .workspace
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_lowercase()
            .as_bytes(),
    );
    let pipe = control::pipe(&vcp_protocol::digest_bytes(
        format!("{}:{key}", fixture.data.canonicalize().unwrap().display())
            .to_lowercase()
            .as_bytes(),
    ));
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            if !server.received_requests().await.unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let status = fixture.run(&["tasks", "status", task.as_str()]).await;
    assert_eq!(status.status.code(), Some(0));
    records(&status);
    let prompts = fixture
        .run(&["inspect", task.as_str(), "--view", "prompts"])
        .await;
    assert_eq!(
        prompts.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&prompts.stderr)
    );
    let prompts = records(&prompts)[0]["data"]["items"].clone();
    let artifact = prompts
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["collection"] == "artifact")
        .unwrap();
    let content = fixture
        .run(&[
            "inspect",
            artifact["id"].as_str().unwrap(),
            "--view",
            "prompts",
            "--offset",
            "0",
            "--length",
            "65536",
        ])
        .await;
    assert_eq!(
        content.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&content.stderr)
    );
    let bytes: Vec<u8> =
        serde_json::from_value(records(&content)[0]["data"]["items"][0]["bytes"].clone()).unwrap();
    assert_eq!(bytes, server.received_requests().await.unwrap()[0].body);
    let envelope = control::request(
        &pipe,
        &Request::Prepare {
            workspace: workspace.clone(),
            task: task.clone(),
            cancel: true,
        },
    )
    .await
    .unwrap();
    let mut command: CommandEnvelope = serde_json::from_value(envelope).unwrap();
    command.caller = vcp_domain::ids::ActorId::new();
    assert!(control::request(
        &pipe,
        &Request::Stop {
            command: Box::new(command)
        }
    )
    .await
    .is_err());
    let cancel = fixture.run(&["tasks", "cancel", task.as_str()]).await;
    assert_eq!(
        cancel.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&cancel.stderr)
    );
    records(&cancel);
    let mut values = vec![accepted];
    while let Some(line) = tokio::time::timeout(std::time::Duration::from_secs(30), receive.recv())
        .await
        .unwrap()
    {
        values.push(serde_json::from_str(&line).unwrap());
    }
    reader.join().unwrap();
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(7));
    let final_result = values.last().unwrap();
    assert_eq!(final_result["conditions"]["cancelled"], true);
    assert_eq!(final_result["conditions"]["unresolved_effect"], true);
    let unreachable = fixture.run(&["tasks", "pause", task.as_str()]).await;
    assert_eq!(unreachable.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&unreachable.stderr).contains("unreachable"));
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_debug_v2_discovers_native_check_and_preserves_missing_access() {
    let manifest: Value = serde_json::from_str(include_str!(
        "../../../evals/skills/builtin/debug-v2/manifest.json"
    ))
    .unwrap();
    for available in [true, false] {
        let server = MockServer::start().await;
        let fixture = Fixture::new(&server.uri(), "complete");
        for file in ["value.txt", "acceptance.cjs"] {
            fs::remove_file(fixture.workspace.join(file)).unwrap();
        }
        for (file, bytes) in manifest["files"].as_object().unwrap() {
            fs::write(fixture.workspace.join(file), bytes.as_str().unwrap()).unwrap();
        }
        let source = manifest["cases"][0]["source"].as_str().unwrap();
        fs::write(fixture.workspace.join("shipping.cjs"), source).unwrap();
        let mut profile: Value =
            serde_json::from_slice(&fs::read(&fixture.profile).unwrap()).unwrap();
        // Project check discovery must still observe package.json/test inputs
        // when the task's directly affected source is only shipping.cjs.
        profile["affected_paths"] = json!(["shipping.cjs"]);
        if available {
            profile["processes"][0]["name"] = json!("cr06-check");
            if let Some(receipt) = std::env::var_os("VCP_CR06_BUILD_RECEIPT") {
                let receipt: Value = serde_json::from_slice(&fs::read(receipt).unwrap()).unwrap();
                profile["processes"][0]["executable"] = receipt["launcher"].clone();
            }
            profile["checks"] = json!([{"manifest":"package.json","runner":"node",
                "profile":"cr06-check","timeout_ms":10000,
                "expected_tests":["shipping fee threshold includes 50"],
                "rationale":"Frozen CR06 v2 current-source verification"}]);
        } else {
            profile["maximum_autonomy"] = json!("workspace");
            profile["automatic_effects"] = json!(["read", "write"]);
            profile["processes"] = json!([]);
            profile["checks"] = json!([]);
        }
        fs::write(&fixture.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let responses = calls.clone();
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .respond_with(move |_: &wiremock::Request| {
                let index = responses.fetch_add(1, Ordering::SeqCst);
                let step = if available { index } else { index + 1 };
                let call = match step {
                    0 => Some(("vcp_exec", json!({"profile":"cr06-check",
                        "arguments":["--test","--test-reporter=tap","--test-concurrency=1","shipping.test.cjs"],
                        "directory":"","timeout_ms":10000,"output_bytes":65536,"input":null}))),
                    1 => Some(("vcp_patch", json!({"patch":"*** Begin Patch\n*** Update File: shipping.cjs\n@@\n-exports.shippingFee = subtotal => subtotal > 50 ? 0 : 5;\n+exports.shippingFee = subtotal => subtotal >= 50 ? 0 : 5;\n*** End Patch"}))),
                    // A first verification can refresh instruction scope. The
                    // second explicit call checks that current revision.
                    2 | 3 => Some(("vcp_verify", json!({"citations":[]}))),
                    _ => None,
                };
                let item = match call {
                    Some((name, args)) => json!({"type":"function_call","id":format!("debug-item-{index}"),"call_id":format!("debug-call-{index}"),"name":name,"arguments":args.to_string(),"status":"completed"}),
                    None => json!({"type":"message","id":format!("debug-final-{index}"),"role":"assistant","status":"completed","content":[{"type":"output_text","text":if available {"Threshold fixed and native check observed."} else {"Threshold edited; execution not-run because reproduction access is unavailable; acceptance remains incomplete."},"annotations":[]}]}),
                };
                let body = [json!({"type":"response.output_item.done","output_index":0,"item":item}),json!({"type":"response.completed","response":{"id":format!("debug-response-{index}"),"status":"completed","output":[item],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}})].into_iter().map(|event|format!("data: {event}\n\n")).collect::<String>();
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(body)
            })
            .mount(&server)
            .await;
        let output = fixture
            .run(&[
                "run",
                "Fix shipping threshold and verify current evidence",
                "--autonomy",
                if available { "autonomous" } else { "workspace" },
            ])
            .await;
        let values = records(&output);
        assert_eq!(
            output.status.code(),
            Some(if available { 0 } else { 3 }),
            "available={available}: {values:?} stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(values.last().unwrap()["conditions"]["completed"], available);
        assert_eq!(
            fs::read_to_string(fixture.workspace.join("shipping.cjs")).unwrap(),
            source.replace("subtotal > 50", "subtotal >= 50")
        );
        for (file, bytes) in manifest["files"].as_object().unwrap() {
            assert_eq!(
                fs::read_to_string(fixture.workspace.join(file)).unwrap(),
                bytes.as_str().unwrap()
            );
        }
        let task = values.last().unwrap()["scope"]["task"].as_str().unwrap();
        let inspected = fixture
            .run(&["inspect", task, "--view", "verification"])
            .await;
        let observed = records(&inspected);
        let rows = observed[0]["data"]["items"].as_array().unwrap();
        if available {
            assert!(rows
                .iter()
                .any(|row| row["record"]["checks"]
                    .as_array()
                    .is_some_and(|checks| checks
                        .iter()
                        .any(|check| check["specification"] == "package.json#test"
                            && check["outcome"]["status"] == "passed"
                            && check["exit_code"] == 0))));
        } else {
            assert!(rows.iter().any(|row| row["record"]["outstanding_issues"]
                .to_string()
                .contains("changed work requires discovered checks")));
            assert!(rows.iter().all(|row| row["record"]["checks"]
                .as_array()
                .is_some_and(Vec::is_empty)));
        }
        assert!(calls.load(Ordering::SeqCst) <= 5);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_terminal_delegates_real_child_with_canonical_transcript() {
    terminal_delegation_case(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_terminal_delegates_real_child_and_pause_fences_both_requests() {
    terminal_delegation_case(true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_terminal_hard_close_reopens_two_active_children_without_dispatch() {
    terminal_delegation_variant(true, 2, true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_terminal_pause_fences_two_children_and_preserves_quiet_status() {
    terminal_delegation_variant(true, 2, false).await;
}

async fn terminal_delegation_case(pause_active: bool) {
    terminal_delegation_variant(pause_active, 1, false).await;
}

async fn terminal_delegation_variant(pause_active: bool, child_count: usize, hard_close: bool) {
    use std::{sync::Mutex, time::Duration};
    let server = MockServer::start().await;
    let fixture = Fixture::new(&server.uri(), "complete");
    let git = PathBuf::from(std::env::var_os("VCP_TEST_GIT").expect("native Git required"));
    for args in [
        vec!["init", "--quiet"],
        vec!["add", "value.txt", "package.json", "acceptance.cjs"],
    ] {
        assert!(Command::new(&git)
            .args(args)
            .current_dir(&fixture.workspace)
            .status()
            .unwrap()
            .success());
    }
    let disposable = fixture._temp.path().join("children");
    assert!(Command::new(&git)
        .args([
            "-c",
            "user.name=VCP fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "fixture base"
        ])
        .current_dir(&fixture.workspace)
        .status()
        .unwrap()
        .success());
    fs::create_dir(&disposable).unwrap();
    let spec = fixture._temp.path().join("delegate.json");
    fs::write(&spec, serde_json::to_vec(&json!({"version":1,"git":git,"disposable_parent":disposable,
        "objective":"Inspect the isolated child source and report observations", "acceptance":["Report source observations"],
        "mode":"read_only","write_paths":[],"untracked_inputs":[],"allocation_usd":"0.1","seconds":120,"required_checks":[]})).unwrap()).unwrap();
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(response(3, "complete"))
                .set_delay(Duration::from_secs(if pause_active { 30 } else { 8 })),
        )
        .expect((child_count + 1) as u64)
        .mount(&server)
        .await;
    let mut child = fixture
        .terminal("Inspect the project while independent review runs")
        .await;
    let writer = child.session.writer_sender();
    let captured = Arc::new(Mutex::new(Vec::new()));
    let output = captured.clone();
    let reader = tokio::spawn(async move {
        while let Some(bytes) = child.stdout_rx.recv().await {
            let mut output = output.lock().unwrap();
            assert!(output.len() + bytes.len() < 4 * 1024 * 1024);
            output.extend(bytes);
        }
    });
    let exercise = async {
        loop {
            if String::from_utf8_lossy(&captured.lock().unwrap()).contains("/skills") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        for index in 0..child_count {
            writer
                .send(format!("/agents delegate \"{}\"\r", spec.display()).into_bytes())
                .await
                .unwrap();
            if child_count > 1 {
                // Child preparation is deliberately serialized by the owner.
                // Wait for actual admitted provider work before asking for the
                // next child, rather than racing two setup requests.
                while server.received_requests().await.unwrap().len() < index + 2 {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            }
        }
        loop {
            let text = String::from_utf8_lossy(&captured.lock().unwrap()).into_owned();
            assert!(
                !text.contains("preparation failed") && !text.contains("Command rejected"),
                "{text}"
            );
            if pause_active && server.received_requests().await.unwrap().len() == child_count + 1 {
                if hard_close {
                    // All root/child provider requests are active at actual PTY
                    // process termination. This is an offline provider fixture,
                    // not qualification of child process execution/isolation.
                    child.session.terminate();
                    break;
                }
                writer.send(b"/pause\r/agents\r".to_vec()).await.unwrap();
                tokio::time::sleep(Duration::from_secs(2)).await;
                let queried = fixture
                    .run(&["tasks", "agents", &fixture.task_id(), "--offset", "0"])
                    .await;
                assert!(
                    queried.status.success(),
                    "{}",
                    String::from_utf8_lossy(&queried.stderr)
                );
                let values = records(&queried);
                let page = &values[0]["data"];
                assert_eq!(page["total"], child_count);
                for item in page["items"].as_array().unwrap() {
                    assert_eq!(item["state"], "paused");
                    assert!(!item["registration"].is_null());
                    assert_eq!(item["cost"]["scope"], "this node only");
                    assert!(item["reason"]
                        .as_str()
                        .is_some_and(|reason| !reason.is_empty()));
                }
                assert_eq!(
                    server.received_requests().await.unwrap().len(),
                    child_count + 1
                );
                break;
            }
            if text.contains("turn ended; canonical status") || text.contains("stopped:") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if !hard_close {
            writer.send(b"/exit\r".to_vec()).await.unwrap();
        }
        (&mut child.exit_rx).await.unwrap()
    };
    if tokio::time::timeout(Duration::from_secs(60), exercise)
        .await
        .is_err()
    {
        child.session.terminate();
        panic!(
            "delegation timeout: {}",
            String::from_utf8_lossy(&captured.lock().unwrap())
        );
    }
    reader.await.unwrap();
    let output = String::from_utf8_lossy(&captured.lock().unwrap()).into_owned();
    assert!(
        output.contains("started in its registered isolated workspace"),
        "{output}"
    );
    let directory = fs::read_dir(fixture.data.join("workspaces"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.join("workspace.json").is_file())
        .unwrap();
    let entry: vcp_cli::settings::WorkspaceEntry =
        serde_json::from_slice(&fs::read(directory.join("workspace.json")).unwrap()).unwrap();
    if hard_close {
        let (recovered, owner) =
            vcp_lifecycle::foundation::CanonicalHost::open(entry.config.clone()).unwrap();
        let state = recovered.snapshot().unwrap();
        let attempts = state
            .records
            .values()
            .filter(|record| record.collection == vcp_store::contract::Collection::Attempt)
            .collect::<Vec<_>>();
        assert_eq!(attempts.len(), child_count + 1);
        assert!(attempts
            .iter()
            .all(|record| record.value["phase"] == "reconciliation_pending"));
        let ledger: vcp_domain::accounting::Ledger = state
            .record(
                vcp_store::contract::Collection::Ledger,
                entry.config.root_task.as_str(),
                &entry.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert!(ledger.unresolved > vcp_domain::Micros::ZERO);
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            child_count + 1,
            "reopening must not redispatch unresolved work"
        );
        owner.close().await.unwrap();
    }
    let store = vcp_store::Store::open(
        &entry.config.canonical_root,
        entry.config.backend,
        &[fixture.workspace.canonicalize().unwrap()],
    )
    .await
    .unwrap();
    let state = store.state();
    let children: Vec<_> = state
        .records
        .values()
        .filter(|r| {
            r.collection == vcp_store::contract::Collection::Task
                && r.value["scope"]["task"] != entry.config.root_task.as_str()
        })
        .collect();
    assert_eq!(children.len(), child_count);
    if pause_active {
        for record in state
            .records
            .values()
            .filter(|r| r.collection == vcp_store::contract::Collection::Task)
        {
            assert_eq!(
                record.value["state"], "paused",
                "root and child must retain the explicit pause: {}",
                record.value
            );
        }
    } else {
        assert!(
            state.records.values().any(|r| r.collection
                == vcp_store::contract::Collection::Artifact
                && r.value.to_string().contains("child_transcript")),
            "child transcript must survive terminal exit"
        );
    }
    store.close().await.unwrap();
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        child_count + 1
    );
    assert_eq!(
        fs::read_to_string(fixture.workspace.join("value.txt")).unwrap(),
        "41\n"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_plan_final_answer_observes_unchanged_analysis_without_waiving_checks() {
    for (required_check, external_change) in [(false, false), (true, false), (false, true)] {
        let server = MockServer::start().await;
        let fixture = Fixture::new(&server.uri(), "complete");
        if !required_check {
            let mut profile: Value =
                serde_json::from_slice(&fs::read(&fixture.profile).unwrap()).unwrap();
            profile["checks"] = json!([]);
            profile["processes"] = json!([]);
            fs::write(&fixture.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
            fs::remove_file(fixture.workspace.join("package.json")).unwrap();
            fs::remove_file(fixture.workspace.join("acceptance.cjs")).unwrap();
        }
        let file = fixture.workspace.join("value.txt");
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .respond_with(move |_: &wiremock::Request| {
                if external_change {
                    fs::write(&file, "43\n").unwrap();
                }
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(response(2, "complete"))
            })
            .expect(1)
            .mount(&server)
            .await;
        let output = fixture
            .run(&[
                "run",
                "Analyze the supplied value without editing.",
                "--autonomy",
                "plan",
            ])
            .await;
        let values = records(&output);
        let expected = if required_check || external_change {
            3
        } else {
            0
        };
        assert_eq!(
            output.status.code(),
            Some(expected),
            "required={required_check} changed={external_change}: {} {}",
            String::from_utf8_lossy(&output.stderr),
            values.last().unwrap()
        );
        assert_eq!(
            fs::read_to_string(fixture.workspace.join("value.txt")).unwrap(),
            if external_change { "43\n" } else { "41\n" }
        );
        if expected == 0 {
            assert!(values
                .iter()
                .any(|v| v.to_string().contains("verification_recorded")));
            assert_eq!(values.last().unwrap()["conditions"]["completed"], true);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_terminal_pauses_steers_resizes_and_resumes_in_same_console() {
    use codex_utils_pty::TerminalSize;
    use std::time::Duration;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(response(0, "complete"))
                .set_delay(Duration::from_secs(30)),
        )
        .mount(&server)
        .await;
    let fixture = Fixture::new(&server.uri(), "complete");
    let mut child = fixture
        .terminal("Inspect café e\u{301} 漢字 and wait for guidance")
        .await;
    let writer = child.session.writer_sender();
    let output = tokio::spawn(async move {
        let mut captured = Vec::new();
        while let Some(bytes) = child.stdout_rx.recv().await {
            assert!(
                captured.len() + bytes.len() <= 2 * 1024 * 1024,
                "terminal flood exceeded test bound"
            );
            captured.extend(bytes);
        }
        captured
    });
    let exercise = async {
        while server.received_requests().await.unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let directory = fs::read_dir(fixture.data.join("workspaces"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let descriptor: Value =
            serde_json::from_slice(&fs::read(directory.join("workspace.json")).unwrap()).unwrap();
        let task = descriptor["config"]["root_task"].as_str().unwrap();
        eprintln!("terminal qualification: provider started, pausing");
        writer.send(b"/pause\r".to_vec()).await.unwrap();
        loop {
            let status = fixture.run(&["tasks", "status", task]).await;
            let value = records(&status);
            if value[0]["data"]["records"][0]["state"] == "paused" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        eprintln!("terminal qualification: paused, steering");
        let request_count = server.received_requests().await.unwrap().len();
        child
            .session
            .resize(TerminalSize { rows: 12, cols: 24 })
            .unwrap();
        writer
            .send(
                "/pause\r/cost\r/history\rKeep café e\u{301} 漢字 intact\r"
                    .as_bytes()
                    .to_vec(),
            )
            .await
            .unwrap();
        loop {
            let status = fixture.run(&["tasks", "status", task]).await;
            let value = records(&status);
            let task = &value[0]["data"]["records"][0];
            if task["objectives"].as_array().unwrap().last().unwrap()["text"]
                .as_str()
                .unwrap()
                .contains("Keep café")
            {
                assert_eq!(task["state"], "paused");
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            request_count,
            "paused input dispatched work"
        );
        eprintln!("terminal qualification: guidance applied, resuming");
        fs::write(fixture.workspace.join("value.txt"), "43\n").unwrap();
        fs::write(
            fixture.workspace.join("AGENTS.md"),
            "Preserve the continuation-instruction-marker-43 input change.\n",
        )
        .unwrap();
        child
            .session
            .resize(TerminalSize {
                rows: 30,
                cols: 100,
            })
            .unwrap();
        loop {
            writer.send(b"/resume\r".to_vec()).await.unwrap();
            tokio::time::sleep(Duration::from_millis(250)).await;
            if server.received_requests().await.unwrap().len() > request_count {
                break;
            }
        }
        eprintln!("terminal qualification: resumed, cancelling");
        let requests = server.received_requests().await.unwrap();
        assert!(
            requests.iter().skip(request_count).any(|request| {
                String::from_utf8_lossy(&request.body)
                    .contains("continuation-instruction-marker-43")
            }),
            "resumed provider request must use externally changed instructions"
        );
        writer.send(b"/cancel\r".to_vec()).await.unwrap();
        let exit = (&mut child.exit_rx).await.unwrap();
        assert!(
            matches!(exit, 6 | 7),
            "cancel must report cancellation or unresolved effects: {exit}"
        );
        let status = fixture.run(&["tasks", "status", task]).await;
        assert_eq!(
            records(&status)[0]["data"]["records"][0]["state"],
            "cancelled"
        );
    };
    if tokio::time::timeout(Duration::from_secs(90), exercise)
        .await
        .is_err()
    {
        child.session.terminate();
        let captured = tokio::time::timeout(Duration::from_secs(5), output)
            .await
            .ok()
            .and_then(Result::ok)
            .map(|v| {
                let text = String::from_utf8_lossy(&v);
                text.chars()
                    .rev()
                    .take(12000)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>()
            });
        panic!("same-console terminal workflow timed out; output: {captured:?}");
    }
    child.session.terminate();
    let captured = tokio::time::timeout(Duration::from_secs(10), output)
        .await
        .unwrap()
        .unwrap();
    let text = String::from_utf8_lossy(&captured);
    assert!(
        text.contains("/pause /resume"),
        "interactive renderer never started: {text}"
    );
    assert!(!text.contains("synthetic-cli-qualification"));
    assert_eq!(
        fs::read_to_string(fixture.workspace.join("value.txt")).unwrap(),
        "43\n"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_terminal_question_requires_explicit_answer_and_resume() {
    use codex_utils_pty::TerminalSize;
    use std::time::Duration;
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(move |_: &wiremock::Request| {
            let index = count.fetch_add(1, Ordering::SeqCst);
            let response = ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(response(if index == 0 { 0 } else { 2 }, "question"));
            if index == 0 {
                response
            } else {
                response.set_delay(Duration::from_secs(30))
            }
        })
        .mount(&server)
        .await;
    let fixture = Fixture::new(&server.uri(), "question");
    let mut child = fixture
        .terminal("Change value to 42 only after explicit approval")
        .await;
    let writer = child.session.writer_sender();
    let output = tokio::spawn(async move {
        let mut captured = Vec::new();
        while let Some(bytes) = child.stdout_rx.recv().await {
            assert!(captured.len() + bytes.len() <= 2 * 1024 * 1024);
            captured.extend(bytes);
        }
        captured
    });
    let exercise = async {
        while calls.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let task = fixture.task_id();
        let approval = loop {
            if let Some(approval) = fixture.approval(&task, None).await {
                break approval;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        assert_eq!(approval["state"], "pending");
        let id = approval["id"].as_str().unwrap();
        writer.send(b"/pause\r".to_vec()).await.unwrap();
        loop {
            let status = fixture.run(&["tasks", "status", &task]).await;
            if records(&status)[0]["data"]["records"][0]["state"] == "paused" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let before = calls.load(Ordering::SeqCst);
        child
            .session
            .resize(TerminalSize { rows: 10, cols: 20 })
            .unwrap();
        writer.send(b"\r/status\r/pause\r".to_vec()).await.unwrap();
        // Allow at least two reducer ticks: redraw and blank Enter cannot choose an answer.
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(
            fixture.approval(&task, Some(id)).await.unwrap()["state"],
            "pending"
        );
        assert_eq!(calls.load(Ordering::SeqCst), before);
        writer
            .send(format!("/answer {id} allow\r").into_bytes())
            .await
            .unwrap();
        loop {
            if fixture.approval(&task, Some(id)).await.unwrap()["state"] == "allowed" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let status = fixture.run(&["tasks", "status", &task]).await;
        assert_eq!(records(&status)[0]["data"]["records"][0]["state"], "paused");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            before,
            "answer dispatched a model request"
        );
        assert_eq!(
            fs::read_to_string(fixture.workspace.join("value.txt")).unwrap(),
            "41\n",
            "answer dispatched the pending write"
        );
        loop {
            writer.send(b"/resume\r".to_vec()).await.unwrap();
            tokio::time::sleep(Duration::from_millis(250)).await;
            if calls.load(Ordering::SeqCst) > before {
                break;
            }
        }
        eprintln!("terminal qualification: resumed, cancelling");
        writer.send(b"/cancel\r".to_vec()).await.unwrap();
        assert!(matches!((&mut child.exit_rx).await.unwrap(), 6 | 7));
    };
    if tokio::time::timeout(Duration::from_secs(90), exercise)
        .await
        .is_err()
    {
        child.session.terminate();
        let captured = tokio::time::timeout(Duration::from_secs(5), output)
            .await
            .ok()
            .and_then(Result::ok)
            .map(|v| {
                let text = String::from_utf8_lossy(&v);
                text.chars()
                    .rev()
                    .take(12000)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>()
            });
        panic!("terminal question workflow timed out; output: {captured:?}");
    }
    child.session.terminate();
    let captured = tokio::time::timeout(Duration::from_secs(10), output)
        .await
        .unwrap()
        .unwrap();
    let text = String::from_utf8_lossy(&captured);
    assert!(
        text.contains("Answer recorded"),
        "explicit answer receipt was not rendered: {text}"
    );
    assert!(!text.contains("synthetic-cli-qualification"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_startup_chooser_waits_for_explicit_selection_and_allows_blank_exit() {
    use std::time::Duration;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(response(0, "complete"))
                .set_delay(Duration::from_secs(30)),
        )
        .mount(&server)
        .await;
    let fixture = Fixture::new(&server.uri(), "complete");
    let first = fixture.paused_before_send("Older chooser objective").await;
    let second = fixture.paused_before_send("Newest chooser objective").await;
    for choose in [false, true] {
        let mut child = fixture.terminal_args(&[]).await;
        let writer = child.session.writer_sender();
        let (display, mut observed) = tokio::sync::watch::channel(String::new());
        let output = tokio::spawn(async move {
            let mut bytes = Vec::new();
            while let Some(chunk) = child.stdout_rx.recv().await {
                assert!(bytes.len() + chunk.len() <= 2 * 1024 * 1024);
                bytes.extend(chunk);
                display.send_replace(String::from_utf8_lossy(&bytes).into_owned());
            }
            bytes
        });
        let exercise = async {
            loop {
                if observed
                    .borrow()
                    .contains("press Enter to leave it paused:")
                {
                    break;
                }
                observed
                    .changed()
                    .await
                    .expect("chooser exited before prompt");
            }
            let prompt = observed.borrow().clone();
            assert!(prompt.contains(&first) && prompt.contains(&second));
            assert!(server.received_requests().await.unwrap().is_empty());
            writer
                .send(if choose {
                    b"1\r".to_vec()
                } else {
                    b"\r".to_vec()
                })
                .await
                .unwrap();
            if choose {
                while server.received_requests().await.unwrap().is_empty() {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                let status = fixture.run(&["tasks", "status", &second]).await;
                assert_eq!(
                    records(&status)[0]["data"]["records"][0]["state"],
                    "running"
                );
                let status = fixture.run(&["tasks", "status", &first]).await;
                assert_eq!(records(&status)[0]["data"]["records"][0]["state"], "paused");
                writer.send(b"/exit\r".to_vec()).await.unwrap();
            }
            let exit = (&mut child.exit_rx).await.unwrap();
            if !choose {
                assert_eq!(exit, 0);
                assert!(server.received_requests().await.unwrap().is_empty());
            }
        };
        let result = tokio::time::timeout(Duration::from_secs(30), exercise).await;
        child.session.terminate();
        let captured = tokio::time::timeout(Duration::from_secs(5), output)
            .await
            .unwrap()
            .unwrap();
        assert!(
            result.is_ok(),
            "chooser exercise timed out: {}",
            String::from_utf8_lossy(&captured)
        );
        for task in [&first, &second] {
            let status = fixture.run(&["tasks", "status", task]).await;
            assert_eq!(records(&status)[0]["data"]["records"][0]["state"], "paused");
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_startup_discovers_paused_roots_and_rejects_stale_selection_without_send() {
    let server = MockServer::start().await;
    let fixture = Fixture::new(&server.uri(), "complete");
    let first = fixture
        .paused_before_send("First unfinished objective")
        .await;
    let second = fixture
        .paused_before_send("Second unfinished objective")
        .await;
    assert_ne!(first, second);
    let mut command = fixture.command(&[]);
    command.env_remove("OPENROUTER_API_KEY");
    let startup = tokio::task::spawn_blocking(move || command.output().unwrap())
        .await
        .unwrap();
    assert_eq!(
        startup.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&startup.stderr)
    );
    let values = records(&startup);
    let candidates = values[0]["data"]["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0]["task"], second);
    assert_eq!(candidates[1]["task"], first);
    for candidate in candidates {
        assert_eq!(candidate["state"], "paused");
        assert!(
            candidate["expected_revision"]
                .as_str()
                .unwrap()
                .parse::<u64>()
                .unwrap()
                > 0
        );
    }
    let stale = fixture
        .run(&["resume", &second, "--expected-revision", "0"])
        .await;
    assert_eq!(stale.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&stale.stderr).contains("revision")
            || String::from_utf8_lossy(&stale.stderr).contains("selection")
    );
    assert!(server.received_requests().await.unwrap().is_empty());
    for task in [&first, &second] {
        let status = fixture.run(&["tasks", "status", task]).await;
        assert_eq!(records(&status)[0]["data"]["records"][0]["state"], "paused");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_missing_or_replaced_root_requires_reconciliation_without_dispatch() {
    let server = MockServer::start().await;
    let mut fixture = Fixture::new(&server.uri(), "complete");
    let task = fixture
        .paused_before_send("Preserve this moved workspace")
        .await;
    let original = records(&fixture.run(&[]).await);
    let workspace_id = original[0]["data"]["workspace"]
        .as_str()
        .unwrap()
        .to_owned();
    let moved = fixture._temp.path().join("moved-workspace");
    fs::rename(&fixture.workspace, &moved).unwrap();
    let missing = fixture.run(&[]).await;
    assert_eq!(missing.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("rebind"));
    fs::create_dir(&fixture.workspace).unwrap();
    let replacement = fixture.run(&[]).await;
    assert_eq!(replacement.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&replacement.stderr).contains("rebind"));
    assert_eq!(fs::read_to_string(moved.join("value.txt")).unwrap(), "41\n");
    assert_eq!(
        fixture.task_id(),
        task,
        "history descriptor must be preserved"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
    fixture.workspace = moved;
    let rebound = fixture.run(&["rebind", &workspace_id]).await;
    assert_eq!(
        rebound.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&rebound.stderr)
    );
    let rebound_values = records(&rebound);
    let result = &rebound_values[0]["data"];
    assert_eq!(result["workspace"], workspace_id);
    assert_eq!(result["rebound"], true);
    assert_eq!(result["trust"], "untrusted");
    assert_eq!(result["history_preserved"], true);
    assert_eq!(result["tasks_resumed"], false);
    let startup = fixture.run(&[]).await;
    assert_eq!(
        startup.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&startup.stderr)
    );
    let startup_values = records(&startup);
    assert_eq!(startup_values[0]["data"]["workspace"], workspace_id);
    let candidates = startup_values[0]["data"]["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0]["task"], task);
    assert_eq!(candidates[0]["state"], "paused");
    let repeated = fixture.run(&["rebind", &workspace_id]).await;
    assert_eq!(
        repeated.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&repeated.stderr)
    );
    let repeated_values = records(&repeated);
    assert_eq!(repeated_values[0]["data"]["rebound"], false);
    assert_eq!(repeated_values[0]["data"]["authority"], result["authority"]);
    assert_eq!(
        repeated_values[0]["data"]["binding_revision"],
        result["binding_revision"]
    );
    assert_eq!(repeated_values[0]["data"]["trust"], "untrusted");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_resume_last_skips_a_newer_completed_root() {
    let server = MockServer::start().await;
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(move |_: &wiremock::Request| {
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(response(
                    calls.fetch_add(1, Ordering::SeqCst) % 3,
                    "complete",
                ))
        })
        .mount(&server)
        .await;
    let fixture = Fixture::new(&server.uri(), "complete");
    let paused = fixture
        .paused_before_send("Change value to 42 and verify the older objective")
        .await;
    let completed = fixture
        .run(&[
            "run",
            "Change value to 42 and verify",
            "--autonomy",
            "autonomous",
        ])
        .await;
    assert_eq!(
        completed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&completed.stderr)
    );
    let completed_values = records(&completed);
    assert_ne!(completed_values.last().unwrap()["scope"]["task"], paused);
    let before = count.load(Ordering::SeqCst);
    let startup = fixture.run(&[]).await;
    assert_eq!(startup.status.code(), Some(0));
    let startup_values = records(&startup);
    let candidates = startup_values[0]["data"]["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0]["task"], paused);
    assert_eq!(count.load(Ordering::SeqCst), before);
    fs::write(fixture.workspace.join("value.txt"), "41\n").unwrap();
    let resumed = fixture.run(&["resume", "--last"]).await;
    let resumed_values = records(&resumed);
    assert_eq!(
        resumed.status.code(),
        Some(0),
        "{} {}",
        String::from_utf8_lossy(&resumed.stderr),
        resumed_values.last().unwrap()
    );
    assert_eq!(resumed_values.last().unwrap()["scope"]["task"], paused);
    assert!(count.load(Ordering::SeqCst) > before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_closed_consumer_pauses_before_send_and_resume_keeps_its_cap() {
    let server = MockServer::start().await;
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(move |_: &wiremock::Request| {
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(response(calls.fetch_add(1, Ordering::SeqCst), "complete"))
        })
        .mount(&server)
        .await;
    let fixture = Fixture::new(&server.uri(), "complete");
    let input = fixture._temp.path().join("task-é.txt");
    fs::write(&input, "Change value to 42.\nVerify the actual change.").unwrap();
    let mut command = fixture.command(&[
        "run",
        "--file",
        input.to_str().unwrap(),
        "--autonomy",
        "autonomous",
        "--budget-usd",
        "0.01",
    ]);
    let mut child = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let output = tokio::task::spawn_blocking(move || child.wait_with_output().unwrap())
        .await
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(count.load(Ordering::SeqCst), 0);
    let entry_dir = fs::read_dir(fixture.data.join("workspaces"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let entry: Value =
        serde_json::from_slice(&fs::read(entry_dir.join("workspace.json")).unwrap()).unwrap();
    let task = entry["config"]["root_task"].as_str().unwrap();
    let status = fixture.run(&["tasks", "status", task]).await;
    let values = records(&status);
    assert_eq!(values[0]["data"]["records"][0]["state"], "paused");
    let mut profile: Value = serde_json::from_slice(&fs::read(&fixture.profile).unwrap()).unwrap();
    profile["budget_usd"] = json!("99");
    fs::write(&fixture.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
    let resumed = fixture.run(&["resume", "--last"]).await;
    let values = records(&resumed);
    assert_eq!(
        resumed.status.code(),
        Some(0),
        "{} {}",
        String::from_utf8_lossy(&resumed.stderr),
        values.last().unwrap()
    );
    let costs = fixture.run(&["inspect", task, "--view", "costs"]).await;
    let values = records(&costs);
    let ledger = values[0]["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["collection"] == "ledger")
        .unwrap();
    assert_eq!(ledger["record"]["cap"], "10000");
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_inspector_reopens_declared_output_without_passing_failed_check() {
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST")).and(path("/v1/responses"))
        .respond_with(move |_: &wiremock::Request| {
            let index = calls.fetch_add(1, Ordering::SeqCst);
            let body = if index == 0 {
                let item = json!({"type":"function_call","id":"encoded-item","call_id":"encoded-call","name":"vcp_exec","arguments":json!({"profile":"encoded","arguments":["-e","process.stdout.write(Buffer.concat([Buffer.from('é終\\u001b[31m','utf16le'),Buffer.from([255])]),()=>process.exit(7))"],"directory":"","timeout_ms":10000,"output_bytes":65536,"input":null}).to_string(),"status":"completed"});
                [json!({"type":"response.output_item.done","output_index":0,"item":item}),json!({"type":"response.completed","response":{"id":"encoded-response","status":"completed","output":[item],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}})].into_iter().map(|event|format!("data: {event}\n\n")).collect::<String>()
            } else { response(index.min(2), "complete") };
            ResponseTemplate::new(200).insert_header("content-type", "text/event-stream").set_body_string(body)
        }).mount(&server).await;
    let fixture = Fixture::new(&server.uri(), "complete");
    let mut profile: Value = serde_json::from_slice(&fs::read(&fixture.profile).unwrap()).unwrap();
    let mut encoded = profile["processes"][0].clone();
    encoded["name"] = json!("encoded");
    encoded["output_encoding"] = json!("utf16_le");
    profile["processes"].as_array_mut().unwrap().push(encoded);
    fs::write(&fixture.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
    let result = fixture
        .run(&[
            "run",
            "Observe the configured encoded output and run verification",
            "--autonomy",
            "autonomous",
        ])
        .await;
    assert_eq!(
        result.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let values = records(&result);
    let task = values.last().unwrap()["scope"]["task"].as_str().unwrap();
    // Select the two exact process receipts from the run's canonical facts.
    // Inspecting unrelated evidence would launch one extra CLI per artifact.
    let facts: Vec<_> = values
        .iter()
        .flat_map(|value| {
            value
                .pointer("/event/event/data/facts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .collect();
    let failed = facts
        .iter()
        .find(|fact| {
            fact["collection"] == "effect"
                && fact["value"]["exit_code"] == 7
                && fact["value"]["state"] == "failed"
        })
        .expect("observed failing process effect");
    let receipts: BTreeSet<_> = failed["value"]["observed_changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap())
        .collect();
    let artifacts: BTreeSet<_> = facts
        .iter()
        .filter(|fact| {
            fact["collection"] == "artifact"
                && receipts.contains(fact["id"].as_str().unwrap())
                && (fact.pointer("/value/spec/channel") == Some(&json!("stdout"))
                    || fact.pointer("/value/spec/schema") == Some(&json!("vcp-process-outcome-v1")))
        })
        .map(|fact| fact["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        artifacts.len(),
        2,
        "stdout and process outcome receipts required"
    );
    let mut expected: Vec<u8> = "é終\u{1b}[31m"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    expected.push(255);
    let mut raw_found = false;
    let mut decision_found = false;
    for artifact in artifacts {
        let output = fixture
            .run(&[
                "inspect", artifact, "--view", "tools", "--offset", "0", "--length", "65536",
            ])
            .await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let bytes: Vec<u8> =
            serde_json::from_value(records(&output)[0]["data"]["items"][0]["bytes"].clone())
                .unwrap();
        raw_found |= bytes == expected;
        if let Ok(evidence) = serde_json::from_slice::<Value>(&bytes) {
            if evidence.pointer("/presentation/stdout/encoding") == Some(&json!("utf16_le")) {
                assert_eq!(evidence["exit_code"], 7);
                assert_eq!(
                    evidence["presentation"]["stdout"]["decision"],
                    "trusted_profile"
                );
                assert_eq!(
                    evidence["presentation"]["stdout"]["replacement_characters"],
                    1
                );
                assert_eq!(
                    evidence["presentation"]["stdout"]["tail"],
                    "é終\u{1b}[31m\u{fffd}"
                );
                decision_found = true;
            }
        }
    }
    assert!(
        raw_found && decision_found,
        "fresh inspectors must recover raw bytes and decoding evidence"
    );
    let verification = fixture
        .run(&["inspect", task, "--view", "verification"])
        .await;
    assert!(verification.status.success());
    assert!(String::from_utf8_lossy(&verification.stdout).contains("failed"));
    assert_eq!(
        fs::read_to_string(fixture.workspace.join("value.txt")).unwrap(),
        "41\n"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_runs_verifies_lists_inspects_and_forks() {
    let server = MockServer::start().await;
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(move |_: &wiremock::Request| {
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(response(
                    calls.fetch_add(1, Ordering::SeqCst) % 3,
                    "complete",
                ))
        })
        .mount(&server)
        .await;
    let fixture = Fixture::new(&server.uri(), "complete");
    let output = fixture
        .run(&[
            "run",
            "Change value to 42 and verify",
            "--autonomy",
            "autonomous",
        ])
        .await;
    let values = records(&output);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {} result: {} calls: {} verification: {:?}",
        String::from_utf8_lossy(&output.stderr),
        values.last().unwrap(),
        count.load(Ordering::SeqCst),
        values
            .iter()
            .filter(|v| v.to_string().contains("verification_recorded"))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        fs::read_to_string(fixture.workspace.join("value.txt")).unwrap(),
        "42\n"
    );
    let scope = &values.last().unwrap()["scope"];
    let task = scope["task"].as_str().unwrap();
    let session = scope["session"].as_str().unwrap();
    for args in [
        vec!["sessions", "list"],
        vec!["tasks", "status", task],
        vec!["inspect", task, "--view", "verification"],
        vec!["resume", task],
    ] {
        let output = fixture.run(&args).await;
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        records(&output);
    }
    // P3-03: page canonical evidence and recover the exact bytes sent to HTTP.
    let mut cursor: Option<String> = None;
    let mut inspected = Vec::new();
    loop {
        let mut args = vec!["inspect", task, "--view", "chain", "--limit", "32"];
        if let Some(token) = &cursor {
            args.extend(["--cursor", token]);
        }
        let output = fixture.run(&args).await;
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let data = records(&output)[0]["data"].clone();
        inspected.extend(data["items"].as_array().unwrap().iter().cloned());
        if data["next_cursor"].is_null() {
            break;
        }
        cursor = Some(data["next_cursor"].to_string());
    }
    for collection in ["attempt", "reservation", "effect", "verification", "access"] {
        assert!(
            inspected.iter().any(|i| i["collection"] == collection),
            "missing {collection}"
        );
    }
    let request_artifacts: Vec<_> = inspected
        .iter()
        .filter(|i| i.pointer("/record/spec/channel") == Some(&json!("request_body")))
        .collect();
    assert!(!request_artifacts.is_empty());
    let http_requests = server.received_requests().await.unwrap();
    for artifact in request_artifacts {
        let id = artifact["id"].as_str().unwrap();
        let mut offset = 0u64;
        let mut bytes = Vec::new();
        loop {
            let offset_text = offset.to_string();
            let output = fixture
                .run(&[
                    "inspect",
                    id,
                    "--view",
                    "prompts",
                    "--offset",
                    &offset_text,
                    "--length",
                    "65536",
                ])
                .await;
            assert_eq!(
                output.status.code(),
                Some(0),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let item = records(&output)[0]["data"]["items"][0].clone();
            bytes.extend(serde_json::from_value::<Vec<u8>>(item["bytes"].clone()).unwrap());
            let Some(next) = item["next_offset"].as_u64() else {
                break;
            };
            offset = next;
        }
        assert!(
            http_requests.iter().any(|r| r.body == bytes),
            "inspector must return actual serialized request bytes"
        );
    }
    let turns: Vec<_> = values
        .iter()
        .flat_map(|v| {
            v.pointer("/event/event/data/facts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter(|r| r["collection"] == "turn" && r["value"]["state"] == "completed")
        .collect();
    assert!(!turns.is_empty(), "completed turn event must be visible");
    let turn = turns.last().unwrap()["id"].as_str().unwrap();
    let before = records(&fixture.run(&["inspect", task, "--view", "tools"]).await)[0]["data"]
        ["items"]
        .clone();
    let fork = fixture
        .run(&["sessions", "fork", session, "--through-turn", turn])
        .await;
    let fork_values = records(&fork);
    assert_ne!(
        fork_values.last().unwrap()["scope"]["session"],
        scope["session"]
    );
    assert!(
        matches!(fork.status.code(), Some(0 | 3)),
        "fork result {} stderr {}",
        fork_values.last().unwrap(),
        String::from_utf8_lossy(&fork.stderr)
    );
    let requests = server.received_requests().await.unwrap();
    let inherited = String::from_utf8(requests[3].body.clone()).unwrap();
    assert!(inherited.contains("canonical-coding-fork-history/1"));
    assert!(inherited.contains("Observed completion."));
    let after = records(&fixture.run(&["inspect", task, "--view", "tools"]).await)[0]["data"]
        ["items"]
        .clone();
    assert_eq!(
        before, after,
        "fork must not rewrite source history or replay source effects"
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_preflight_budget_question_and_incomplete_are_truthful() {
    for mode in ["budget", "question", "incomplete"] {
        let server = MockServer::start().await;
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .respond_with(move |_: &wiremock::Request| {
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(response(calls.fetch_add(1, Ordering::SeqCst), mode))
            })
            .mount(&server)
            .await;
        let fixture = Fixture::new(&server.uri(), mode);
        for args in [
            vec!["run"],
            vec!["run", "--file", "missing-task-file"],
            vec!["run", "x", "--budget-usd", "nan"],
        ] {
            let output = fixture.run(&args).await;
            assert_eq!(output.status.code(), Some(2));
            records(&output);
            assert!(!fixture.data.join("workspaces").exists());
        }
        let output = fixture
            .run(&[
                "run",
                "Change value to 42 and verify",
                "--autonomy",
                "autonomous",
                "--budget-usd",
                if mode == "budget" { "0.000001" } else { "1" },
            ])
            .await;
        let values = records(&output);
        assert_eq!(
            output.status.code(),
            Some(match mode {
                "budget" => 5,
                "question" => 4,
                _ => 3,
            }),
            "{mode}: {} stderr {} calls {}",
            values.last().unwrap(),
            String::from_utf8_lossy(&output.stderr),
            count.load(Ordering::SeqCst)
        );
        if mode == "budget" {
            assert_eq!(count.load(Ordering::SeqCst), 0);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_packaged_skills_are_relocatable_lazy_and_integrity_checked() {
    let server = MockServer::start().await;
    let mut fixture = Fixture::new(&server.uri(), "budget");
    let assets = fixture.package(true);
    let output = fixture
        .run(&[
            "run",
            "Register packaged workspace",
            "--autonomy",
            "autonomous",
            "--budget-usd",
            "0.000001",
        ])
        .await;
    assert_eq!(
        output.status.code(),
        Some(5),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    // Listing uses installed metadata, without reading any skill body or provider catalog.
    fs::remove_file(assets.join("architecture/SKILL.md")).unwrap();
    fs::remove_file(fixture.data.join("catalog.json")).unwrap();
    let mut command = fixture.command(&["skills", "list"]);
    command.env_remove("OPENROUTER_API_KEY");
    let output = tokio::task::spawn_blocking(move || command.output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let values = records(&output);
    let data = &values.last().unwrap()["data"];
    assert_eq!(data["total_skills"], 21);
    assert_eq!(data["reads"]["bodies"], 0);
    assert_eq!(data["reads"]["resources"], 0);
    assert!(!data["integrity"].is_null());
    assert!(data["skills"]
        .as_array()
        .unwrap()
        .iter()
        .all(|skill| skill["source"] == "vcp-builtin"));
    for relative in ["architecture/skill.json", "catalog.json", "coverage.json"] {
        let path = assets.join(relative);
        let original = fs::read(&path).unwrap();
        fs::write(&path, b"{}").unwrap();
        let output = fixture.run(&["skills", "list"]).await;
        assert!(!output.status.success(), "tampered {relative} accepted");
        fs::write(&path, original).unwrap();
    }
    fs::remove_file(assets.join("architecture/skill.json")).unwrap();
    assert!(!fixture.run(&["skills", "list"]).await.status.success());
    fixture.maximum_skill_sources();
    let full = fixture.run(&["skills", "list"]).await;
    assert!(!full.status.success());
    let diagnostic = format!(
        "{}{}",
        String::from_utf8_lossy(&full.stdout),
        String::from_utf8_lossy(&full.stderr)
    );
    assert!(diagnostic.contains("31 additional sources"), "{diagnostic}");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_bare_binary_reports_missing_assets_without_ambient_fallback() {
    let server = MockServer::start().await;
    let mut fixture = Fixture::new(&server.uri(), "budget");
    fixture.package(false);
    // An apparently usable cwd asset tree is deliberately ignored.
    fs::create_dir_all(fixture.workspace.join("skills/builtin")).unwrap();
    fs::write(fixture.workspace.join("skills/builtin/catalog.json"), b"{}").unwrap();
    let output = fixture
        .run(&[
            "run",
            "Register bare workspace",
            "--autonomy",
            "autonomous",
            "--budget-usd",
            "0.000001",
        ])
        .await;
    assert_eq!(output.status.code(), Some(5));
    let output = fixture.run(&["skills", "list"]).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let values = records(&output);
    let data = &values.last().unwrap()["data"];
    assert_eq!(data["total_skills"], 0);
    assert!(data["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["code"] == "builtin_assets_missing"));
    fixture.maximum_skill_sources();
    assert!(
        fixture.run(&["skills", "list"]).await.status.success(),
        "bare binaries preserve the existing 32-source limit"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_skills_inspection_is_lazy_without_provider_or_budget_admission() {
    let server = MockServer::start().await;
    let fixture = Fixture::new(&server.uri(), "budget");
    let output = fixture
        .run(&[
            "run",
            "Register this workspace",
            "--autonomy",
            "autonomous",
            "--budget-usd",
            "0.000001",
        ])
        .await;
    assert_eq!(output.status.code(), Some(5));
    let package = fixture.skills();
    // Bodies remain unavailable without preventing descriptor inspection.
    fs::remove_file(package.join("SKILL.md")).unwrap();
    fs::remove_file(fixture.data.join("catalog.json")).unwrap();
    let mut command = fixture.command(&["skills", "list"]);
    command.env_remove("OPENROUTER_API_KEY");
    let output = tokio::task::spawn_blocking(move || command.output())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let values = records(&output);
    let data = &values.last().unwrap()["data"];
    assert_eq!(data["skills"][0]["qualified_id"], "project::review::review");
    assert_eq!(data["skills"][0]["version"], "1.2.0");
    assert_eq!(data["skills"][0]["matches_project"], true);
    assert_eq!(data["reads"]["bodies"], 0);
    assert_eq!(data["reads"]["resources"], 0);
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_terminal_skill_activation_reports_source_version_reason_and_setup_failures() {
    use std::sync::Mutex;
    use std::time::Duration;
    async fn wait_for(captured: &Arc<Mutex<Vec<u8>>>, needle: &str) {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if String::from_utf8_lossy(&captured.lock().unwrap()).contains(needle) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "terminal never showed {needle}: {}",
                String::from_utf8_lossy(&captured.lock().unwrap())
            )
        });
    }
    let server = MockServer::start().await;
    let mut fixture = Fixture::new(&server.uri(), "budget");
    let assets = fixture.package(true);
    let project_package = fixture.skills();
    let descriptor_path = project_package.join("skill.json");
    let mut descriptor: Value =
        serde_json::from_slice(&fs::read(&descriptor_path).unwrap()).unwrap();
    descriptor["id"] = json!("architecture");
    fs::write(&descriptor_path, serde_json::to_vec(&descriptor).unwrap()).unwrap();
    let mut child = fixture
        .terminal_args(&[
            "run",
            "Inspect the current project",
            "--autonomy",
            "autonomous",
            "--budget-usd",
            "0.000001",
        ])
        .await;
    let writer = child.session.writer_sender();
    let captured = Arc::new(Mutex::new(Vec::new()));
    let output = captured.clone();
    let reader = tokio::spawn(async move {
        while let Some(bytes) = child.stdout_rx.recv().await {
            let mut output = output.lock().unwrap();
            assert!(output.len() + bytes.len() <= 2 * 1024 * 1024);
            output.extend(bytes);
        }
    });
    let exercise = async {
        wait_for(&captured, "/skills").await;
        writer
            .send(
                b"/pause\r/skills activate vcp-builtin::architecture::architecture explicit-builtin-evidence\r"
                    .to_vec(),
            )
            .await
            .unwrap();
        wait_for(&captured, "explicit-builtin-evidence").await;
        writer
            .send(b"/skills disable vcp-builtin::architecture::architecture\r".to_vec())
            .await
            .unwrap();
        wait_for(&captured, "disabled").await;
        // Descriptor discovery stays lazy; an altered body fails on activation.
        fs::write(
            assets.join("review-debug/SKILL.md"),
            b"untrusted changed instructions",
        )
        .unwrap();
        writer
            .send(b"/skills activate vcp-builtin::review-debug::review-debug\r".to_vec())
            .await
            .unwrap();
        wait_for(&captured, "skill source or dependency changed").await;
        // The unqualified ID resolves to the higher-precedence workspace source.
        writer
            .send(b"/skills activate architecture cli-explicit-review-evidence\r".to_vec())
            .await
            .unwrap();
        wait_for(&captured, "1.2.0").await;
        let inspected = fixture.run(&["skills", "list"]).await;
        assert_eq!(
            inspected.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&inspected.stderr)
        );
        let values = records(&inspected);
        let data = &values.last().unwrap()["data"];
        assert_eq!(data["reads"]["bodies"], 0);
        assert_eq!(data["total_skills"], 22);
        assert!(!data["integrity"].is_null());
        assert!(data["configuration"]
            .as_str()
            .unwrap()
            .contains("current canonical owner"));
        writer
            .send(b"/skills activate missing-skill\r".to_vec())
            .await
            .unwrap();
        wait_for(&captured, "missing skill").await;
        writer
            .send(b"/skills disable project::review::architecture\r".to_vec())
            .await
            .unwrap();
        wait_for(&captured, "disabled").await;
        writer.send(b"/exit\r".to_vec()).await.unwrap();
        let code = (&mut child.exit_rx).await.unwrap();
        assert!(
            matches!(code, 5 | 8),
            "paused or budget-exhausted exit required: {code}"
        );
    };
    if tokio::time::timeout(Duration::from_secs(75), exercise)
        .await
        .is_err()
    {
        child.session.terminate();
        panic!("terminal skill controls timed out");
    }
    reader.await.unwrap();
    let output = String::from_utf8_lossy(&captured.lock().unwrap()).into_owned();
    assert!(output.contains("cli-explicit-review-evidence") && output.contains("project"));
    let directory = fs::read_dir(fixture.data.join("workspaces"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.join("workspace.json").is_file())
        .unwrap();
    let entry: vcp_cli::settings::WorkspaceEntry =
        serde_json::from_slice(&fs::read(directory.join("workspace.json")).unwrap()).unwrap();
    let store = vcp_store::Store::open(
        &entry.config.canonical_root,
        entry.config.backend,
        &[fixture.workspace.canonicalize().unwrap()],
    )
    .await
    .unwrap();
    let state = store
        .state()
        .records
        .values()
        .find(|record| {
            record.collection == vcp_store::contract::Collection::Projection
                && record.value["document_type"] == "vcp_task_skills_v1"
                && record.value["scope"]["task"] == entry.config.root_task.as_str()
        })
        .unwrap();
    assert!(state.value["active"].as_object().unwrap().is_empty());
    assert!(state.value["disabled"]
        .as_array()
        .unwrap()
        .iter()
        .any(|id| id == "project::review::architecture"));
    assert!(state.value["disabled"]
        .as_array()
        .unwrap()
        .iter()
        .any(|id| id == "vcp-builtin::architecture::architecture"));
    assert_eq!(
        state.value["revision"], "4",
        "only the two successful activations and disables persist revisions"
    );
    store.close().await.unwrap();
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "skill controls dispatched paid inference"
    );
}
