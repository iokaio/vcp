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
