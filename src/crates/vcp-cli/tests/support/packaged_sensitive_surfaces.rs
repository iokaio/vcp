// SPDX-License-Identifier: Apache-2.0
//! P8-03: synthetic typed secrets versus intentionally retained ordinary text.
use super::*;
use std::io::{Read, Seek, SeekFrom};
use std::process::{Child, Stdio};
use std::time::Duration;
use vcp_domain::artifact::ArtifactDescriptor;
use vcp_store::{contract::Collection, Store};

const PROVIDER: &str = "synthetic-cli-qualification";
const ORDINARY: &str = "sk-synthetic-ordinary-source-must-remain-visible-p803";

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(Some(_))) {
            return;
        }
        let _ = self.0.kill();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            if matches!(self.0.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

async fn bounded_output(mut command: Command) -> Output {
    // File-backed capture cannot block a child on a full stdout/stderr pipe.
    // This future retains ownership; timeout/unwind kills and reaps the CLI.
    let mut stdout = tempfile::tempfile().unwrap();
    let mut stderr = tempfile::tempfile().unwrap();
    let mut child = OwnedChild(
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout.try_clone().unwrap()))
            .stderr(Stdio::from(stderr.try_clone().unwrap()))
            .spawn()
            .unwrap(),
    );
    let status = tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            for file in [&stdout, &stderr] {
                assert!(
                    file.metadata().unwrap().len() <= 2 * 1024 * 1024,
                    "surface output ceiling"
                );
            }
            if let Some(status) = child.0.try_wait().unwrap() {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    drop(child);
    let status = status.expect("surface CLI deadline");
    let mut captured = Vec::new();
    for file in [&mut stdout, &mut stderr] {
        assert!(
            file.metadata().unwrap().len() <= 2 * 1024 * 1024,
            "surface output ceiling"
        );
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        captured.push(bytes);
    }
    Output {
        status,
        stdout: captured.remove(0),
        stderr: captured.remove(0),
    }
}

fn excluded(bytes: &[u8], secrets: &[String], surface: &str) {
    for secret in secrets {
        assert!(
            !bytes
                .windows(secret.len())
                .any(|part| part == secret.as_bytes()),
            "typed secret reached {surface}"
        );
    }
}

async fn data(fixture: &Fixture, arguments: &[&str], secrets: &[String]) -> Value {
    let output = bounded_output(fixture.command(arguments)).await;
    checked_data(&output, secrets)
}

fn checked_data(output: &Output, secrets: &[String]) -> Value {
    excluded(&output.stdout, secrets, "JSONL stdout");
    excluded(&output.stderr, secrets, "CLI diagnostic stderr");
    assert!(
        output.status.success(),
        "synthetic surface command rejected"
    );
    records(output)
        .into_iter()
        .find(|row| row["type"] == "result")
        .unwrap()["data"]
        .clone()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the exact extracted package; run explicit P8 qualification"]
async fn packaged_typed_secrets_stay_out_of_capture_environment_and_presentations() {
    std::env::var_os("VCP_TEST_SKILL_PACKAGE").expect("exact extracted package required for P8-03");
    for backend in ["sqlite", "files"] {
        let server = MockServer::start().await;
        let mut fixture = Fixture::new(&server.uri(), "complete");
        fixture._temp.disable_cleanup(true);
        eprintln!(
            "p803-sensitive-surfaces backend={backend} retained_root={}",
            fixture._temp.path().display()
        );
        fixture.package(true);
        let mut secrets = vec![
            PROVIDER.to_owned(),
            "AGE-SECRET-KEY-".into(),
            "VCP writer Ed25519".into(),
        ];
        data(
            &fixture,
            &["storage", "configure", "--backend", backend],
            &secrets,
        )
        .await;
        fixture
            .paused_before_send("Initialize synthetic key qualification scope")
            .await;
        assert!(server.received_requests().await.unwrap().is_empty());
        let recovery = fixture._temp.path().join("independent-recovery");
        fs::create_dir(&recovery).unwrap();
        let enrollment_output = bounded_output(fixture.command(&[
            "backup",
            "keys",
            "create",
            "--recovery-dir",
            recovery.to_str().unwrap(),
        ]))
        .await;
        let enrolled = checked_data(&enrollment_output, &secrets);
        let key = PathBuf::from(enrolled["recovery_copy"].as_str().unwrap());
        // Generated disposable fixture keys are read only inside this test. They
        // never enter an objective, workspace script, argv, assertion or receipt.
        let material = fs::read_to_string(&key).unwrap();
        secrets.push(
            material
                .lines()
                .find(|line| line.starts_with("AGE-SECRET-KEY-"))
                .unwrap()
                .to_owned(),
        );
        secrets.push(
            material
                .lines()
                .find_map(|line| line.strip_prefix("# VCP writer Ed25519 "))
                .unwrap()
                .to_owned(),
        );
        // Enrollment precedes knowledge of the generated values. Rescan its
        // original bytes so a bare writer seed cannot evade prefix checks.
        excluded(&enrollment_output.stdout, &secrets, "key enrollment stdout");
        excluded(&enrollment_output.stderr, &secrets, "key enrollment stderr");
        data(
            &fixture,
            &["backup", "keys", "verify", "--key", key.to_str().unwrap()],
            &secrets,
        )
        .await;
        fs::write(fixture.workspace.join("environment.cjs"), format!(
            "const assert=require('node:assert/strict');assert.equal(process.env.OPENROUTER_API_KEY,undefined);assert.equal(process.env.VCP_TEST_RECOVERY_SECRET,undefined);process.stdout.write({});\n",
            serde_json::to_string(&format!("filtered-environment:{ORDINARY}")).unwrap()
        )).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |_: &wiremock::Request| {
            let index = observed.fetch_add(1, Ordering::SeqCst);
            let body = if index == 0 {
                let item = json!({"type":"function_call","id":"environment-item","call_id":"environment-call","name":"vcp_exec",
                    "arguments":json!({"profile":"node","arguments":["environment.cjs"],"directory":"","timeout_ms":10000,"output_bytes":32768,"input":null}).to_string(),"status":"completed"});
                [json!({"type":"response.output_item.done","output_index":0,"item":item}), json!({"type":"response.completed","response":{"id":"environment-response","status":"completed","output":[item],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}})]
                    .into_iter().map(|event| format!("data: {event}\n\n")).collect()
            } else { response(index - 1, "complete") };
            ResponseTemplate::new(200).insert_header("content-type", "text/event-stream").set_body_string(body)
        }).mount(&server).await;
        let mut command = fixture.command(&[
            "run",
            "Check the filtered environment, change value to 42 and verify",
            "--autonomy",
            "autonomous",
        ]);
        // Deliberately place disposable recovery material in an unrelated
        // parent variable to detect accidental ambient child inheritance.
        command.env("VCP_TEST_RECOVERY_SECRET", &secrets[3]);
        let output = bounded_output(command).await;
        excluded(&output.stdout, &secrets, "task JSONL");
        excluded(&output.stderr, &secrets, "task diagnostics");
        assert!(output.status.success(), "synthetic surface task failed");
        let task = records(&output).last().unwrap()["scope"]["task"]
            .as_str()
            .unwrap()
            .to_owned();
        let requests = server.received_requests().await.unwrap();
        assert!(!requests.is_empty());
        for request in &requests {
            excluded(&request.body, &secrets, "provider input body");
            // This is the sole authorized secret-bearing surface in the run.
            assert_eq!(
                request
                    .headers
                    .get("authorization")
                    .unwrap()
                    .to_str()
                    .unwrap(),
                format!("Bearer {PROVIDER}")
            );
        }
        let tool_output = requests
            .iter()
            .find_map(|request| {
                let body: Value = serde_json::from_slice(&request.body).unwrap();
                body["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|item| {
                        item["type"] == "function_call_output"
                            && item["call_id"] == "environment-call"
                    })
                    .map(|item| {
                        serde_json::from_str::<Value>(item["output"].as_str().unwrap()).unwrap()
                    })
            })
            .expect("native filtered environment result");
        assert_eq!(
            tool_output["stdout"]["tail"],
            format!("filtered-environment:{ORDINARY}")
        );
        let artifact = tool_output["stdout"]["artifact"].as_str().unwrap();
        let page = data(
            &fixture,
            &[
                "inspect", artifact, "--view", "tools", "--offset", "0", "--length", "32768",
            ],
            &secrets,
        )
        .await;
        let bytes: Vec<u8> = serde_json::from_value(page["items"][0]["bytes"].clone()).unwrap();
        assert_eq!(bytes, format!("filtered-environment:{ORDINARY}").as_bytes());
        assert_eq!(
            page["items"][0]["descriptor"]["spec"]["omissions"],
            json!(["authentication_headers", "recovery_material"])
        );
        assert!(page["gaps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|gap| gap["visibility"] == "redacted" && gap["range"].is_null()));
        for view in [
            "chain",
            "context",
            "prompts",
            "outputs",
            "routing",
            "policy",
            "tools",
            "costs",
            "verification",
            "memory",
        ] {
            let pages = tokio::time::timeout(Duration::from_secs(90), async {
                let mut cursor: Option<String> = None;
                let mut seen = std::collections::BTreeSet::new();
                for count in 1..=32 {
                    let mut arguments = vec!["inspect", &task, "--view", view, "--limit", "128"];
                    if let Some(token) = &cursor {
                        arguments.extend(["--cursor", token]);
                    }
                    // Every page's stdout and diagnostics cross the same
                    // exclusion checks, including pages after the first.
                    let page = data(&fixture, &arguments, &secrets).await;
                    let next = page.get("next_cursor").expect("inspector cursor field");
                    if next.is_null() {
                        return count;
                    }
                    let token = next.to_string();
                    assert!(
                        seen.insert(token.clone()),
                        "inspector cursor repeated: {view}"
                    );
                    cursor = Some(token);
                }
                panic!("finite fixture inspector page ceiling: {view}");
            })
            .await
            .expect("inspector view deadline");
            eprintln!("p803-sensitive-inspector backend={backend} view={view} pages={pages}");
        }
        data(&fixture, &["doctor"], &secrets).await;
        // The native console renders the actual artifact inspection, including
        // the retained ordinary marker. No replacement renderer is used.
        let mut child = fixture
            .terminal_args(&[
                "inspect", artifact, "--view", "tools", "--offset", "0", "--length", "32768",
            ])
            .await;
        let reader = tokio::spawn(async move {
            let mut bytes = Vec::new();
            while let Some(chunk) = child.stdout_rx.recv().await {
                assert!(bytes.len() + chunk.len() <= 2 * 1024 * 1024);
                bytes.extend(chunk);
            }
            bytes
        });
        let exit = tokio::time::timeout(Duration::from_secs(30), &mut child.exit_rx).await;
        child.session.terminate();
        let terminal = tokio::time::timeout(Duration::from_secs(5), reader)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(exit.expect("terminal surface deadline").unwrap(), 0);
        excluded(&terminal, &secrets, "native terminal");
        let unwrapped: Vec<u8> = terminal
            .iter()
            .copied()
            .filter(|byte| !matches!(byte, b'\r' | b'\n'))
            .collect();
        excluded(&unwrapped, &secrets, "native terminal across line wrapping");
        assert!(String::from_utf8_lossy(&unwrapped).contains(ORDINARY));

        // Inspect every canonical artifact, not just the prompt/UI tail. Open
        // only after all CLI owners exit, and release before fixture cleanup.
        let directory = fs::read_dir(fixture.data.join("workspaces"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let selected: vcp_cli::settings::WorkspaceEntry =
            serde_json::from_slice(&fs::read(directory.join("workspace.json")).unwrap()).unwrap();
        let store = Store::open(
            &selected.config.canonical_root,
            selected.config.backend,
            &[],
        )
        .await
        .unwrap();
        excluded(
            &serde_json::to_vec(store.state()).unwrap(),
            &secrets,
            "canonical records/events/receipts",
        );
        let mut artifacts = 0;
        let mut ordinary_retained = false;
        for row in store
            .state()
            .records
            .values()
            .filter(|row| row.collection == Collection::Artifact)
        {
            let descriptor: ArtifactDescriptor = row.decode().unwrap();
            assert!(descriptor.length.get() <= 2 * 1024 * 1024);
            let mut bytes = Vec::new();
            store.spool().read(&descriptor, &mut bytes).unwrap();
            excluded(&bytes, &secrets, "full artifact payload");
            ordinary_retained |= bytes
                .windows(ORDINARY.len())
                .any(|part| part == ORDINARY.as_bytes());
            artifacts += 1;
        }
        assert!(artifacts > 0 && ordinary_retained);
        store.close().await.unwrap();
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            requests.len(),
            "inspection and diagnostics cannot dispatch inference"
        );
        eprintln!("p803-sensitive-surfaces backend={backend} artifacts={artifacts} provider_body=true wire_auth=true tool_environment=true jsonl=true diagnostics=true inspectors=10 terminal=true ordinary_text_retained=true diagnostic_bundle=unsupported");
    }
}
