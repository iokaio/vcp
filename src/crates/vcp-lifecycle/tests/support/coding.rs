// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use vcp_domain::policy::{Autonomy, EffectClass, Policy};
use vcp_lifecycle::foundation::coding::{allowed_tools, CodingConfig};
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_coding_loop_assembles_current_sources_and_dispatches_prepared_files() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for mode in [
            "complete",
            "nested",
            "process_fail",
            "limit",
            "missing_cost",
            "stale_instructions",
            "deadline",
            "empty",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let workspace = temp.path().join("workspace");
            std::fs::create_dir(&workspace).unwrap();
            let workspace = workspace.canonicalize().unwrap();
            std::fs::write(workspace.join("file.txt"), "before\n").unwrap();
            std::fs::write(workspace.join("AGENTS.md"), "instruction version one").unwrap();
            std::fs::create_dir(workspace.join("nested")).unwrap();
            std::fs::write(workspace.join("nested/AGENTS.md"), "nested scoped guidance").unwrap();
            std::fs::write(workspace.join("nested/file.txt"), "nested evidence").unwrap();
            std::fs::write(
                workspace.join("fixture.txt"),
                if mode == "process_fail" {
                    "answer = 41\n"
                } else {
                    "answer = 42\n"
                },
            )
            .unwrap();
            let executable = temp.path().join("fixture.exe");
            std::fs::copy(env!("CARGO_BIN_EXE_vcp-process-fixture"), &executable).unwrap();
            let config = config(&temp.path().join("canonical"), &workspace, backend);
            let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
            let binding = task(&host, &config, config.root_task.clone(), None);
            host.command(
                Command::SetWorkspaceTrust {
                    trust: Trust::Trusted,
                },
                None,
                Revision::ZERO,
            )
            .unwrap();
            host.command(
                Command::SetPolicy {
                    policy: Policy {
                        workspace: config.workspace.clone(),
                        revision: PolicyRevision::ZERO,
                        mode: Autonomy::Autonomous,
                        denials: vec![],
                        workspace_roots: std::collections::BTreeSet::from([
                            RootId::parse(config.workspace.as_str()).unwrap(),
                            RootId::parse("exec-fixture").unwrap(),
                        ]),
                        automatic_effects: std::collections::BTreeSet::from([
                            EffectClass::Read,
                            EffectClass::Write,
                            EffectClass::Execute,
                            EffectClass::Network,
                            EffectClass::Install,
                            EffectClass::Publish,
                            EffectClass::Opaque,
                        ]),
                        timeout_ceiling_ms: Units::new(30_000),
                        output_ceiling_bytes: ByteCount::new(1024 * 1024),
                    },
                },
                None,
                Revision::ZERO,
            )
            .unwrap();
            host.configure_process_profile(
                vcp_tools::process::Profile::new(
                    "fixture".into(),
                    executable,
                    vcp_tools::process::Mode::Direct,
                    std::collections::BTreeMap::from([(
                        "SystemRoot".into(),
                        std::env::var("SystemRoot").unwrap(),
                    )]),
                    Default::default(),
                    true,
                )
                .unwrap()
                .with_inputs(vec!["fixture.txt".into()])
                .unwrap(),
            )
            .unwrap();
            let (snapshot, raw) = provider_snapshot();
            host.configure_provider(snapshot, raw).unwrap();
            let count = Arc::new(AtomicUsize::new(0));
            let observed = Arc::new(std::sync::Mutex::new(Vec::new()));
            let requests = observed.clone();
            let calls = count.clone();
            let directory = workspace.clone();
            let server = start_mock_server().await;
            Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request: &wiremock::Request| {
                let index = calls.fetch_add(1, Ordering::SeqCst);
                let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                requests.lock().unwrap().push(body);
                let mut events = vec![];
                let mut output = vec![];
                if mode == "empty" {
                    events.push(ev_assistant_message("empty-answer", " \n\t "));
                } else if index < 4 {
                    let (name, arguments) = if index == 1 {
                        ("vcp_patch", serde_json::json!({"patch":"*** Begin Patch\n*** Update File: file.txt\n@@\n-before\n+after\n*** Update File: AGENTS.md\n@@\n-instruction version one\n+instruction version two\n*** End Patch"}))
                    } else if index==3 { ("vcp_exec", serde_json::json!({"profile":"fixture","arguments":["verify",directory.to_str().unwrap()],"directory":"","timeout_ms":10_000,"output_bytes":1_048_576,"input":null})) }
                    else { ("vcp_read", serde_json::json!({"path":if mode=="nested" && index==0 {"nested/file.txt"}else{"file.txt"},"max_bytes":1024})) };
                    let item = serde_json::json!({"type":"function_call","id":format!("item-{index}"),"call_id":format!("call-{index}"),"name":name,"arguments":arguments.to_string(),"status":"completed"});
                    events.push(serde_json::json!({"type":"response.output_item.done","output_index":0,"item":item}));
                    output.push(item);
                    if mode == "complete" && index == 0 {
                        let sibling = serde_json::json!({"type":"function_call","id":"sibling-item","call_id":"sibling-read","name":"vcp_read","arguments":serde_json::json!({"path":"fixture.txt","max_bytes":1024}).to_string(),"status":"completed"});
                        events.push(serde_json::json!({"type":"response.output_item.done","output_index":1,"item":sibling}));
                        output.push(sibling);
                    }
                } else { events.push(ev_assistant_message("done", "Observed the file change.")); }
                let cost = if mode=="missing_cost" {serde_json::Value::Null}else{serde_json::json!(0.0001)};
                if mode=="stale_instructions" { std::fs::write(directory.join("AGENTS.md"), "concurrent human guidance").unwrap(); }
                events.push(serde_json::json!({"type":"response.completed","response":{"id":format!("coding-{index}"),"status":"completed","output":output,"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":cost}}}));
                let response = ResponseTemplate::new(200).insert_header("content-type", "text/event-stream").set_body_string(sse(events));
                if mode=="deadline" {response.set_delay(Duration::from_secs(20))}else{response}
            }).mount(&server).await;
            let mut registry = ExtensionRegistryBuilder::new();
            registry.turn_start_admission(Arc::new(host.clone()));
            registry.work_admission(Arc::new(host.clone()));
            registry.tool_contributor(Arc::new(host.clone()));
            let starter = host.clone();
            let cwd = workspace.clone();
            let test = test_codex()
                .with_extensions(Arc::new(registry.build()))
                .with_auth(codex_login::CodexAuth::from_api_key("synthetic-coding-key"))
                .with_allowed_tools(allowed_tools())
                .with_config(move |config| {
                    config.cwd = cwd.try_into().unwrap();
                    configure_provider_fixture(config);
                    starter
                        .lifecycle()
                        .authorize_startup(config.cwd.as_path(), None)
                        .unwrap();
                })
                .build_with_auto_env(&server)
                .await
                .unwrap();
            let id = host.lifecycle().attach_root(test.codex.clone()).unwrap();
            host.register(id, binding.clone()).unwrap();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            host.configure_coding(
                id,
                CodingConfig {
                    operating: "Use prepared tools and report observed evidence only.".into(),
                    affected_paths: vec!["file.txt".into()],
                    max_requests: if mode == "limit" { 2 } else { 8 },
                    // Leave native capture/startup headroom before submission;
                    // the server then withholds its response beyond this bound.
                    deadline: Timestamp::new(
                        now + if mode == "deadline" { 10_000 } else { 300_000 },
                    ),
                },
            )
            .unwrap();
            assert!(codex_extension_api::HostWorkAdmission::admit_tool(
                &host,
                id,
                "call-0",
                &codex_extension_api::ToolName::plain("vcp_read")
            )
            .is_err());
            assert_eq!(
                host.project().unwrap().tasks[&config.root_task].state,
                TaskState::Running,
                "invented pre-response calls cannot pause the root"
            );
            let turn = host
                .begin_coding_turn(id, "Run the synthetic request.".into())
                .unwrap();
            coding_turn(&test, backend, mode).await;
            let canonical_turn: vcp_domain::task::Turn = host
                .snapshot()
                .unwrap()
                .record(Collection::Turn, turn.as_str(), &config.workspace)
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(
                canonical_turn.state,
                if matches!(mode, "complete" | "nested" | "process_fail") {
                    vcp_domain::task::TurnState::Verifying
                } else {
                    vcp_domain::task::TurnState::Paused
                },
                "{backend:?} {mode}"
            );
            let expected = if matches!(mode, "complete" | "nested" | "process_fail") {
                5
            } else if mode == "limit" {
                2
            } else {
                1
            };
            assert_eq!(count.load(Ordering::SeqCst), expected, "{backend:?} {mode}");
            let requests = observed.lock().unwrap().clone();
            assert_eq!(requests[0]["parallel_tool_calls"], true);
            if mode == "complete" {
                let input = requests[1]["input"].as_array().unwrap();
                for (call, expected) in [("call-0", "before"), ("sibling-read", "answer = 42")] {
                    let output = input
                        .iter()
                        .find(|item| {
                            item["type"] == "function_call_output" && item["call_id"] == call
                        })
                        .unwrap();
                    assert!(
                        output["output"].as_str().unwrap().contains(expected),
                        "result must retain original call correlation: {output}"
                    );
                }
            }
            let first = requests[0].to_string();
            assert!(first.contains("Observe retained request and response"));
            assert!(!first.contains("Run the synthetic request."));
            assert!(first.contains("instruction version one"));
            assert!(first.contains("not_ready"));
            assert!(first.contains("fixed_qualified_model"));
            assert!(!first.contains("nested scoped guidance"));
            if mode == "nested" {
                assert!(requests[1].to_string().contains("nested scoped guidance"));
                let item = requests[1]["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|item| item["type"] == "function_call_output")
                    .unwrap();
                assert!(item["output"]
                    .as_str()
                    .unwrap()
                    .contains("New instruction scope selected"));
            }
            if !matches!(
                mode,
                "missing_cost" | "stale_instructions" | "deadline" | "empty"
            ) {
                if matches!(mode, "complete" | "process_fail") {
                    assert!(requests[2].to_string().contains("instruction version two"));
                    let item = requests[4]["input"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|item| {
                            item["type"] == "function_call_output" && item["call_id"] == "call-3"
                        })
                        .unwrap();
                    let output: serde_json::Value =
                        serde_json::from_str(item["output"].as_str().unwrap()).unwrap();
                    assert_eq!(
                        output["exit_code"],
                        if mode == "process_fail" { 1 } else { 0 }
                    );
                    let stdout =
                        ArtifactId::parse(output["stdout"]["artifact"].as_str().unwrap()).unwrap();
                    if mode == "complete" {
                        assert!(String::from_utf8(host.read_artifact(stdout).unwrap())
                            .unwrap()
                            .contains("fixture assertion passed"));
                    } else {
                        assert!(output["stderr"]["tail"]
                            .as_str()
                            .unwrap()
                            .contains("fixture assertion failed"));
                    }
                }
                assert!(requests[1]["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|item| item["type"] == "function_call_output"
                        && item["call_id"] == "call-0"));
                assert_eq!(
                    std::fs::read_to_string(workspace.join("file.txt")).unwrap(),
                    "after\n"
                );
            } else {
                assert_eq!(
                    std::fs::read_to_string(workspace.join("file.txt")).unwrap(),
                    "before\n"
                );
            }
            let view = host
                .project()
                .unwrap_or_else(|error| panic!("{backend:?} {mode}: {error}"));
            if mode == "empty" {
                assert!(
                    host.complete_coding_turn(id).is_err(),
                    "empty response must not establish an accounted final answer"
                );
            }
            assert_eq!(
                view.effects.len(),
                if mode == "complete" {
                    5
                } else if mode == "process_fail" {
                    4
                } else if mode == "nested" {
                    3
                } else if mode == "limit" {
                    2
                } else {
                    0
                }
            );
            assert_eq!(
                view.effects
                    .values()
                    .filter(|effect| effect.state == vcp_domain::effect::EffectState::Failed)
                    .count(),
                usize::from(mode == "process_fail")
            );
            assert!(view.effects.values().all(|effect| matches!(
                effect.state,
                vcp_domain::effect::EffectState::Succeeded
                    | vcp_domain::effect::EffectState::Failed
            )));
            if matches!(
                mode,
                "limit" | "missing_cost" | "stale_instructions" | "deadline" | "empty"
            ) {
                assert_eq!(view.tasks[&config.root_task].state, TaskState::Paused);
            }
            assert_eq!(
                view.ledgers[&config.root_task].settled.get(),
                if matches!(mode, "missing_cost" | "deadline") {
                    0
                } else {
                    expected as u64 * 100
                }
            );
            assert!(codex_extension_api::HostWorkAdmission::admit_tool(
                &host,
                id,
                "call-0",
                &codex_extension_api::ToolName::plain("vcp_read")
            )
            .is_err());
            assert!(codex_extension_api::HostWorkAdmission::admit_tool(
                &host,
                id,
                "call-0",
                &codex_extension_api::ToolName::plain("exec_command")
            )
            .is_err());
            owner.close().await.unwrap();
            test.codex.shutdown_and_wait().await.unwrap();
            drop(test);
            drop(host);
            if mode == "complete" {
                std::fs::write(
                    workspace.join("AGENTS.md"),
                    "instruction version three after reopen",
                )
                .unwrap();
                let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
                let (snapshot, raw) = provider_snapshot();
                host.configure_provider(snapshot, raw).unwrap();
                let mut registry = ExtensionRegistryBuilder::new();
                registry.turn_start_admission(Arc::new(host.clone()));
                registry.work_admission(Arc::new(host.clone()));
                registry.tool_contributor(Arc::new(host.clone()));
                let starter = host.clone();
                let cwd = workspace.clone();
                let test = test_codex()
                    .with_extensions(Arc::new(registry.build()))
                    .with_auth(codex_login::CodexAuth::from_api_key(
                        "synthetic-coding-reopen",
                    ))
                    .with_allowed_tools(allowed_tools())
                    .with_config(move |config| {
                        config.cwd = cwd.try_into().unwrap();
                        configure_provider_fixture(config);
                        starter
                            .lifecycle()
                            .authorize_startup(config.cwd.as_path(), None)
                            .unwrap();
                    })
                    .build_with_auto_env(&server)
                    .await
                    .unwrap();
                let id = host.lifecycle().attach_root(test.codex.clone()).unwrap();
                host.register(id, binding.clone()).unwrap();
                let task = host.project().unwrap().tasks[&config.root_task].clone();
                host.resume(id, task.revision, task.fingerprint).unwrap();
                host.configure_coding(
                    id,
                    CodingConfig {
                        operating: "Use prepared tools and report observed evidence only.".into(),
                        affected_paths: vec!["file.txt".into()],
                        max_requests: 6,
                        deadline: Timestamp::new(now + 300_000),
                    },
                )
                .unwrap();
                assert!(codex_extension_api::HostWorkAdmission::admit_tool(
                    &host,
                    id,
                    "call-3",
                    &codex_extension_api::ToolName::plain("vcp_exec")
                )
                .is_err());
                coding_turn(&test, backend, "reopen").await;
                assert_eq!(count.load(Ordering::SeqCst), 6);
                let request = observed.lock().unwrap()[5].clone();
                assert!(request
                    .to_string()
                    .contains("instruction version three after reopen"));
                let old = requests[4]["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|item| item["type"] == "function_call_output")
                    .collect::<Vec<_>>();
                let restored = request["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|item| item["type"] == "function_call_output")
                    .collect::<Vec<_>>();
                assert_eq!(restored, old);
                let mut helper_binding = super::task(
                    &host,
                    &config,
                    TaskId::new(),
                    Some(config.root_task.clone()),
                );
                helper_binding.role = RequestRole::Helper;
                host.lifecycle()
                    .authorize_startup(&workspace, None)
                    .unwrap();
                let mut extension_init = codex_extension_api::ExtensionDataInit::default();
                extension_init.insert(AllowedTools(vec![]));
                let helper = test
                    .thread_manager
                    .start_thread(codex_core::StartThreadOptions {
                        thread_extension_init: extension_init,
                        environments: Some(test.codex.environment_selections().await),
                        ..codex_core::StartThreadOptions::new(test.config.clone())
                    })
                    .await
                    .unwrap()
                    .thread;
                let helper_id = host
                    .lifecycle()
                    .attach_child(
                        id,
                        &host.lifecycle().inspect(id).unwrap().revision,
                        helper.clone(),
                    )
                    .unwrap();
                host.register(helper_id, helper_binding).unwrap();
                host.configure_coding(
                    helper_id,
                    CodingConfig {
                        operating: "Bounded helper fixture".into(),
                        affected_paths: vec!["file.txt".into()],
                        max_requests: 128,
                        deadline: Timestamp::new(now + 300_000),
                    },
                )
                .unwrap();
                helper
                    .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                        text: "A helper must not widen the root request limit".into(),
                        text_elements: vec![],
                    }]))
                    .await
                    .unwrap();
                coding_turn_complete(&helper, backend, "helper-limit").await;
                assert_eq!(
                    count.load(Ordering::SeqCst),
                    6,
                    "root request limit survives reopen and a larger helper allowance"
                );
                assert_eq!(
                    host.project().unwrap().tasks[&config.root_task].state,
                    TaskState::Paused
                );
                owner.close().await.unwrap();
                helper.shutdown_and_wait().await.unwrap();
                test.codex.shutdown_and_wait().await.unwrap();
            }
        }
    }
}

async fn coding_turn(test: &TestCodex, backend: BackendKind, mode: &str) {
    test.codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "Run the synthetic request.".into(),
            text_elements: vec![],
        }]))
        .await
        .unwrap();
    coding_turn_complete(&test.codex, backend, mode).await;
}

async fn coding_turn_complete(thread: &codex_core::CodexThread, backend: BackendKind, mode: &str) {
    let mut last = None;
    // Native durable capture and process setup can contend with other local
    // qualification. Product request/deadline limits remain independently set.
    let result = tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            let event = thread.next_event().await.unwrap();
            if matches!(event.msg, EventMsg::TurnComplete(_)) {
                break;
            }
            last = Some(event.msg);
        }
    })
    .await;
    assert!(
        result.is_ok(),
        "{backend:?}/{mode}: turn did not finish; last event: {last:?}"
    );
}
