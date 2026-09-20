// SPDX-License-Identifier: Apache-2.0
//! Scripted provider transport plus independent native MCP effects; no model quality claim.
use super::*;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicUsize, Ordering},
};
use vcp_domain::policy::{Autonomy, EffectClass, Policy};
use vcp_lifecycle::foundation::{
    coding::{allowed_tools, CodingConfig},
    mcp::Registration,
};
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

fn result(body: &Value, call: &str) -> Value {
    let item = body["input"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["type"] == "function_call_output" && item["call_id"] == call)
        .unwrap_or_else(|| panic!("missing result {call}: {body}"));
    serde_json::from_str(item["output"].as_str().unwrap()).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coding_mcp_discovery_call_disconnect_retains_accounted_source_provenance() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        std::fs::write(workspace.join("input.txt"), "MCP_CODING_SOURCE_MARKER").unwrap();
        std::fs::write(
            workspace.join("AGENTS.md"),
            "Use only configured MCP tools; disconnect before native file reads.",
        )
        .unwrap();
        let executable = temp.path().join("mcp-fixture.exe");
        std::fs::copy(env!("CARGO_BIN_EXE_vcp-mcp-fixture"), &executable).unwrap();
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
                    workspace_roots: BTreeSet::from([
                        RootId::parse(config.workspace.as_str()).unwrap(),
                        RootId::parse("exec-fixture").unwrap(),
                    ]),
                    automatic_effects: BTreeSet::from([
                        EffectClass::Read,
                        EffectClass::Write,
                        EffectClass::Execute,
                        EffectClass::Network,
                        EffectClass::Install,
                        EffectClass::Publish,
                        EffectClass::Opaque,
                    ]),
                    timeout_ceiling_ms: Units::new(120_000),
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
                BTreeMap::from([("SystemRoot".into(), std::env::var("SystemRoot").unwrap())]),
                Default::default(),
                true,
            )
            .unwrap()
            .with_inputs(vec!["input.txt".into()])
            .unwrap(),
        )
        .unwrap();
        host.configure_mcp(Registration {
            name: "fixture".into(),
            process: vcp_tools::process::Request {
                profile: "fixture".into(),
                arguments: vec![workspace.to_string_lossy().into_owned(), "normal".into()],
                directory: String::new(),
                timeout_ms: 120_000,
                output_bytes: 1024 * 1024,
                input: None,
            },
            allowed_tools: BTreeSet::from(["write_marker".into()]),
            limits: vcp_extensions::mcp::registration::Limits {
                frame_bytes: 64 * 1024,
                total_discovery_bytes: 1024 * 1024,
                tools: 16,
                pages: 8,
                timeout_ms: 10_000,
                stderr_bytes: 64 * 1024,
            },
        })
        .unwrap();
        let (snapshot, raw) = provider_snapshot();
        host.configure_provider(snapshot, raw).unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let observed = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
        let calls = count.clone();
        let requests = observed.clone();
        let server = start_mock_server().await;
        Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request: &wiremock::Request| {
            let index = calls.fetch_add(1, Ordering::SeqCst);
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let mut events = vec![];
            let mut output = vec![];
            if index < 4 {
                let (name, arguments) = match index {
                    0 => ("vcp_mcp", json!({"action":"list","server":"fixture","tool":"","identity_digest":"","arguments_json":""})),
                    1 => {
                        let listing = result(&body, "mcp-0");
                        let tools = listing["catalog"]["tools"].as_array().unwrap();
                        assert_eq!(tools.len(),1,"server allowlist must filter discovery");
                        assert_eq!(tools[0]["tool"],"write_marker");
                        ("vcp_mcp",json!({"action":"call","server":"fixture","tool":"write_marker","identity_digest":tools[0]["identity_digest"],"arguments_json":json!({"value":"coding-observed-effect"}).to_string()}))
                    },
                    2 => {
                        let outcome=result(&body,"mcp-1");
                        assert!(outcome["error"].is_null(),"MCP call rejected: {outcome}");
                        ("vcp_mcp", json!({"action":"disconnect","server":"fixture","tool":"","identity_digest":"","arguments_json":""}))
                    },
                    _ => {
                        assert_eq!(result(&body,"mcp-2")["disconnected"],true);
                        ("vcp_read",json!({"path":"value.txt","max_bytes":1024}))
                    },
                };
                let item=json!({"type":"function_call","id":format!("item-{index}"),"call_id":format!("mcp-{index}"),"name":name,"arguments":arguments.to_string(),"status":"completed"});
                events.push(json!({"type":"response.output_item.done","output_index":0,"item":item}));
                output.push(item);
            } else {
                let read=result(&body,"mcp-3");
                assert!(read.to_string().contains("coding-observed-effect"),"native post-disconnect read failed: {read}");
                events.push(ev_assistant_message("done","Observed the MCP marker and disconnected the server."));
            }
            requests.lock().unwrap().push(body);
            events.push(json!({"type":"response.completed","response":{"id":format!("mcp-coding-{index}"),"status":"completed","output":output,"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}}));
            ResponseTemplate::new(200).insert_header("content-type","text/event-stream").set_body_string(sse(events))
        }).mount(&server).await;
        let mut registry = ExtensionRegistryBuilder::new();
        registry.turn_start_admission(Arc::new(host.clone()));
        registry.work_admission(Arc::new(host.clone()));
        registry.tool_contributor(Arc::new(host.clone()));
        let starter = host.clone();
        let cwd = workspace.clone();
        let test = test_codex()
            .with_extensions(Arc::new(registry.build()))
            .with_auth(codex_login::CodexAuth::from_api_key("synthetic-mcp-coding"))
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
        let thread = host.lifecycle().attach_root(test.codex.clone()).unwrap();
        host.register(thread, binding).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        host.configure_coding(thread,CodingConfig {operating:"Discover, use, and disconnect configured MCP tools; report only observed evidence.".into(),affected_paths:vec!["input.txt".into()],max_requests:8,deadline:Timestamp::new(now+300_000)}).unwrap();
        host.begin_coding_turn(thread, "Exercise the configured MCP marker.".into())
            .unwrap();
        test.codex
            .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                text: "Exercise the configured MCP marker.".into(),
                text_elements: vec![],
            }]))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(120), async {
            loop {
                if matches!(
                    test.codex.next_event().await.unwrap().msg,
                    EventMsg::TurnComplete(_)
                ) {
                    break;
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 5, "{backend:?}");
        assert!(!host.mcp_connections_present());
        assert_eq!(
            std::fs::read_to_string(workspace.join("value.txt")).unwrap(),
            "coding-observed-effect"
        );
        assert_eq!(
            std::fs::read_to_string(workspace.join("writes.jsonl"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        let state = host.snapshot().unwrap();
        let results = state
            .records
            .values()
            .filter(|row| row.collection == Collection::Artifact)
            .map(|row| row.decode::<ArtifactDescriptor>().unwrap())
            .filter(|artifact| artifact.spec.schema == "vcp-mcp-call-result-v1")
            .collect::<Vec<_>>();
        assert_eq!(results.len(), 1);
        let evidence: Value =
            serde_json::from_slice(&host.read_artifact(results[0].spec.id.clone()).unwrap())
                .unwrap();
        assert_eq!(evidence["external_content"], true);
        assert_eq!(evidence["grants_authority"], false);
        assert!(evidence["source"]["context_manifest"].as_str().is_some());
        assert!(
            evidence["source"]["owner_input_sha256"].is_null(),
            "model calls cannot claim literal-owner authority"
        );
        let sources = evidence["source"]["source_artifacts"].as_array().unwrap();
        assert!(!sources.is_empty());
        for source in sources {
            assert!(state
                .record(
                    Collection::Artifact,
                    source.as_str().unwrap(),
                    &config.workspace
                )
                .is_ok());
        }
        let attempts = state
            .records
            .values()
            .filter(|row| row.collection == Collection::Attempt)
            .map(|row| row.decode::<Attempt>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(attempts.len(), 5);
        assert!(attempts
            .iter()
            .all(|attempt| attempt.phase == ReservationState::Settled));
        assert!(observed.lock().unwrap()[0].to_string().contains("vcp_mcp"));
        owner.close().await.unwrap();
    }
}
