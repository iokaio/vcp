// SPDX-License-Identifier: Apache-2.0
//! BETA-02: real CLI/public execution must consume the same import selection.
use super::*;
use std::sync::Mutex;

impl Fixture {
    fn import_peer(&self, delayed: bool) {
        let script = format!(
            r#"const fs=require('node:fs'),rl=require('node:readline'),marker={};
rl.createInterface({{input:process.stdin}}).on('line',line=>{{
 const q=JSON.parse(line);fs.appendFileSync(marker,q.method+'\n');
 if(q.id===undefined)return;
 const result=q.method==='initialize'?{{protocolVersion:'2025-11-25',capabilities:{{tools:{{}}}},serverInfo:{{name:'import-peer',version:'1'}}}}:
 {{tools:['read','write'].map(name=>({{name,inputSchema:{{type:'object',properties:{{}},additionalProperties:false}}}}))}};
 setTimeout(()=>process.stdout.write(JSON.stringify({{jsonrpc:'2.0',id:q.id,result}})+'\n'),q.method==='tools/list'?{}:0);
}});"#,
            serde_json::to_string(&self.data.join("mcp-observed.txt")).unwrap(),
            if delayed { 3000 } else { 0 }
        );
        fs::write(self.workspace.join("import-peer.cjs"), script).unwrap();
        let mut profile: Value = serde_json::from_slice(&fs::read(&self.profile).unwrap()).unwrap();
        profile["checks"] = json!([]);
        profile["mcp"] = json!([{"name":"imported","process":{"profile":"node","arguments":["import-peer.cjs"],"directory":"","timeout_ms":10000,"output_bytes":65536,"input":null},"allowed_tools":["read","write"],"limits":{"frame_bytes":4096,"total_discovery_bytes":8192,"tools":8,"pages":2,"timeout_ms":10000,"stderr_bytes":4096}}]);
        fs::write(&self.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
        fs::write(self.data.join("import.json"), serde_json::to_vec(&json!({"mcpServers":{"imported":{"command":profile["processes"][0]["executable"],"args":["import-peer.cjs"],"cwd":self.workspace,"includeTools":["read"],"timeout":1000}}})).unwrap()).unwrap();
    }

    async fn import_restrictions(&self) {
        let source = self.data.join("import.json");
        let arguments = |action: &str| {
            vec![
                "config".into(),
                "import".into(),
                action.into(),
                "--source".into(),
                source.to_string_lossy().into_owned(),
                "--source-root".into(),
                self.data.to_string_lossy().into_owned(),
                "--source-format".into(),
                "gemini-6a466a7e-v1".into(),
            ]
        };
        let preview = self.import_command(arguments("preview")).await;
        let mut args = arguments("apply");
        args.extend([
            "--preview".into(),
            preview["preview_id"].as_str().unwrap().into(),
            "--select".into(),
            "server.imported.allowed_tools".into(),
            "--select".into(),
            "server.imported.timeout_ms".into(),
        ]);
        self.import_command(args).await;
    }

    async fn import_command(&self, args: Vec<String>) -> Value {
        let mut command = self.command(&args.iter().map(String::as_str).collect::<Vec<_>>());
        let output = tokio::task::spawn_blocking(move || command.output().unwrap())
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .last()
            .unwrap()["data"]
            .clone()
    }

    async fn reselect_import(&self) {
        let preview = self
            .import_command(
                ["config", "import", "rollback-preview", "--revision", "1"]
                    .map(str::to_owned)
                    .into(),
            )
            .await;
        self.import_command(vec![
            "config".into(),
            "import".into(),
            "rollback".into(),
            "--revision".into(),
            "1".into(),
            "--preview".into(),
            preview["preview_id"].as_str().unwrap().into(),
        ])
        .await;
    }

    fn import_revision(&self) -> PathBuf {
        self.data
            .join("profile.json.vcp-imports/revision-00000000000000000001.json")
    }
}

fn mcp_response(index: usize, action: &str) -> String {
    let arguments = json!({"action":action,"server":"imported","tool":if action=="call" {"write"} else {""},"identity_digest":if action=="call" {"0".repeat(64)} else {String::new()},"arguments_json":if action=="call" {"{}"} else {""}});
    let item = json!({"type":"function_call","id":format!("import-item-{index}"),"call_id":format!("import-call-{index}"),"name":"vcp_mcp","arguments":arguments.to_string(),"status":"completed"});
    [json!({"type":"response.output_item.done","output_index":0,"item":item}),json!({"type":"response.completed","response":{"id":format!("import-response-{index}"),"status":"completed","output":[item],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}})].into_iter().map(|event|format!("data: {event}\n\n")).collect()
}

fn selected_task(client: &mut wire::Client, entry: &WorkspaceEntry, root: &str) -> Value {
    let reply = client.rpc(20, "task/read", json!({"scope":scope(entry),"task":root}));
    assert!(reply.get("error").is_none(), "{reply}");
    reply["result"]["value"].clone()
}

fn resume(client: &mut wire::Client, entry: &WorkspaceEntry) -> Value {
    let before = selected_task(client, entry, entry.config.root_task.as_str());
    json!({"scope":scope(entry),"task":entry.config.root_task,"mutation":{"command_id":"import-resume","expected_revision":before["revision"],"steering_revision":before["steering_revision"]}})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn imported_tools_and_deadline_apply_to_cli_public_start_and_resume_on_both_stores() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for delayed in [false, true] {
            for mode in ["cli", "start", "resume"] {
                let server = MockServer::start().await;
                let observed = Arc::new(Mutex::new(Vec::<Value>::new()));
                let outputs = observed.clone();
                let count = Arc::new(AtomicUsize::new(0));
                let calls = count.clone();
                Mock::given(method("POST"))
                    .and(path("/v1/responses"))
                    .respond_with(move |request: &wiremock::Request| {
                        let body: Value = serde_json::from_slice(&request.body).unwrap();
                        for item in body["input"].as_array().unwrap() {
                            if item["type"] == "function_call_output"
                                && item["call_id"]
                                    .as_str()
                                    .is_some_and(|id| id.starts_with("import-call-"))
                            {
                                outputs.lock().unwrap().push(item.clone());
                            }
                        }
                        let index = calls.fetch_add(1, Ordering::SeqCst);
                        let body = match index {
                            0 => mcp_response(index, "list"),
                            1 if !delayed => mcp_response(index, "call"),
                            2 if !delayed => mcp_response(index, "disconnect"),
                            _ => response(3),
                        };
                        ResponseTemplate::new(200)
                            .insert_header("content-type", "text/event-stream")
                            .set_body_string(body)
                    })
                    .mount(&server)
                    .await;
                let fixture = Fixture::new(&server.uri());
                fixture.import_peer(delayed);
                fixture.import_restrictions().await;
                let loaded = vcp_cli::settings::load(
                    &fixture.profile,
                    &fixture.workspace.canonicalize().unwrap(),
                )
                .unwrap();
                assert_eq!(loaded.mcp[0].allowed_tools, BTreeSet::from(["read".into()]));
                assert_eq!(loaded.mcp[0].limits.timeout_ms, 1000);
                assert_eq!(loaded.mcp[0].process.timeout_ms, 1000);
                let entry = fixture.seed(backend).await;
                assert_eq!(count.load(Ordering::SeqCst), 0);
                assert!(!fixture.data.join("mcp-observed.txt").exists());
                if mode == "cli" {
                    let mut command = fixture.command(&[
                        "run",
                        "Inspect configured MCP tools",
                        "--autonomy",
                        "autonomous",
                    ]);
                    let output = tokio::time::timeout(
                        Duration::from_secs(90),
                        tokio::task::spawn_blocking(move || command.output().unwrap()),
                    )
                    .await
                    .unwrap()
                    .unwrap();
                    assert!(
                        output.status.success(),
                        "{backend:?}/{delayed}/{mode}: {} {}",
                        String::from_utf8_lossy(&output.stdout)
                            .lines()
                            .last()
                            .unwrap_or_default(),
                        String::from_utf8_lossy(&output.stderr)
                    );
                } else {
                    let root = if mode == "start" {
                        ROOT
                    } else {
                        entry.config.root_task.as_str()
                    };
                    let mut client = fixture.launch("controller", Some(root));
                    acquire(&mut client, &entry, "import-owner");
                    let request = if mode == "start" {
                        start(&entry)
                    } else {
                        resume(&mut client, &entry)
                    };
                    wire::accepted(&client.rpc(
                        4,
                        if mode == "start" {
                            "turn/start"
                        } else {
                            "session/resume"
                        },
                        request,
                    ));
                    let until = Instant::now() + Duration::from_secs(45);
                    loop {
                        let view = selected_task(&mut client, &entry, root);
                        if view["state"] == "completed" {
                            break;
                        }
                        assert!(
                            Instant::now() < until,
                            "{backend:?}/{delayed}/{mode}: {view}"
                        );
                        tokio::time::sleep(Duration::from_millis(25)).await;
                    }
                    assert!(client.finish().await.0.success());
                }
                let seen = observed.lock().unwrap();
                let listed = seen
                    .iter()
                    .find(|item| item["call_id"] == "import-call-0")
                    .expect("MCP list result must reach the provider");
                let value: Value =
                    serde_json::from_str(listed["output"].as_str().unwrap()).unwrap();
                if delayed {
                    assert!(
                        value.get("catalog").is_none(),
                        "imported deadline must reject the slow list: {value}"
                    );
                    assert!(
                        value.to_string().contains("deadline")
                            || value.to_string().contains("timed out"),
                        "{value}"
                    );
                } else {
                    let names: Vec<_> = value["catalog"]["tools"]
                        .as_array()
                        .unwrap_or_else(|| {
                            panic!("{backend:?}/{mode}: expected MCP catalog, got {value}")
                        })
                        .iter()
                        .map(|tool| tool["tool"].as_str().unwrap())
                        .collect();
                    assert_eq!(names, ["read"], "{backend:?}/{mode}: {value}");
                    assert_eq!(
                        value["catalog"]["rejected"],
                        json!([{"name":"write","reason":"not_allowed"}])
                    );
                    let denied = seen
                        .iter()
                        .find(|item| item["call_id"] == "import-call-1")
                        .expect("denied tool call result");
                    assert!(!denied["output"]
                        .as_str()
                        .unwrap()
                        .contains("\"outcome\":\"succeeded\""));
                }
                assert_eq!(
                    fs::read_to_string(fixture.data.join("mcp-observed.txt")).unwrap(),
                    "initialize\nnotifications/initialized\ntools/list\n",
                    "removed tool must never dispatch"
                );
                assert_eq!(
                    fs::read(fixture.workspace.join("value.txt")).unwrap(),
                    b"41\n"
                );
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn changed_base_revision_content_or_chain_refuses_start_and_resume_without_dispatch() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for change in [
            "base",
            "revision",
            "content",
            "corrupt",
            "removed",
            "first-import",
        ] {
            for mode in ["start", "resume"] {
                let server = MockServer::start().await;
                let fixture = Fixture::new(&server.uri());
                fixture.import_peer(false);
                if change != "first-import" {
                    fixture.import_restrictions().await;
                }
                if change == "corrupt" {
                    // Corrupt an older revision while the selected bytes stay
                    // unchanged: admission must revalidate the whole chain.
                    fixture.reselect_import().await;
                }
                let entry = fixture.seed(backend).await;
                let root = if mode == "start" {
                    ROOT
                } else {
                    entry.config.root_task.as_str()
                };
                let mut client = fixture.launch("controller", Some(root));
                acquire(&mut client, &entry, "stale-import-owner");
                match change {
                    "base" => {
                        let mut bytes = fs::read(&fixture.profile).unwrap();
                        bytes.push(b' ');
                        fs::write(&fixture.profile, bytes).unwrap();
                    }
                    "revision" => fixture.reselect_import().await,
                    "first-import" => fixture.import_restrictions().await,
                    "content" => {
                        let mut bytes = fs::read(fixture.import_revision()).unwrap();
                        bytes.push(b' ');
                        fs::write(fixture.import_revision(), bytes).unwrap();
                    }
                    "corrupt" => {
                        fs::write(fixture.import_revision(), b"{broken chain").unwrap();
                    }
                    "removed" => {
                        fs::remove_file(fixture.import_revision()).unwrap();
                    }
                    _ => unreachable!(),
                }
                let request = if mode == "start" {
                    start(&entry)
                } else {
                    resume(&mut client, &entry)
                };
                let rejected = client.rpc(
                    4,
                    if mode == "start" {
                        "turn/start"
                    } else {
                        "session/resume"
                    },
                    request,
                );
                assert_eq!(
                    rejected["error"]["data"]["details"]["code"], "POLICY_DENIED",
                    "{backend:?}/{change}/{mode}: {rejected}"
                );
                if mode == "resume" {
                    assert_eq!(selected_task(&mut client, &entry, root)["state"], "paused");
                }
                assert!(client.finish().await.0.success());
                assert!(server.received_requests().await.unwrap().is_empty());
                assert!(!fixture.data.join("mcp-observed.txt").exists());
                let store = reopen(&entry).await;
                assert!(!store.state().commands.values().any(|receipt| matches!(
                    receipt.command.as_str(),
                    "compiled-start-once" | "import-resume"
                )));
                store.close().await.unwrap();
                // Invalid persisted imports also refuse the ordinary CLI before
                // provider work; valid new revisions can be selected on relaunch.
                if matches!(change, "base" | "corrupt") {
                    let mut command = fixture.command(&[
                        "run",
                        "Must refuse stale import",
                        "--autonomy",
                        "autonomous",
                    ]);
                    let output = tokio::task::spawn_blocking(move || command.output().unwrap())
                        .await
                        .unwrap();
                    assert_eq!(output.status.code(), Some(2));
                    assert!(server.received_requests().await.unwrap().is_empty());
                    let mut restarted = wire::Client::spawn("local-bridge");
                    restarted.send(json!({"schema":"vcp-local-bootstrap/1","workspace":fixture.workspace,"data":fixture.data,"role":"controller","execution":{"profile":fixture.profile,"provider_credential":SECRET,"credentials":{}}}));
                    let (status, diagnostics) = restarted.rejected().await;
                    assert!(!status.success(), "invalid import must refuse bootstrap");
                    assert!(!diagnostics.contains(SECRET));
                    assert!(server.received_requests().await.unwrap().is_empty());
                }
            }
        }
    }
}
