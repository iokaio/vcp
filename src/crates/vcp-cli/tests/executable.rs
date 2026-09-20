// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
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
            env!("CARGO_BIN_EXE_vcp"),
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
        let mut command = Command::new(env!("CARGO_BIN_EXE_vcp"));
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
