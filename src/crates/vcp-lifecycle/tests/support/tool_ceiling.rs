// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::collections::BTreeSet;
use vcp_domain::policy::{Autonomy, EffectClass, Policy};
use vcp_lifecycle::foundation::{
    coding::{CanonicalTools, CodingConfig},
    verification::VerificationConfig,
};
use vcp_store::artifact::ArtifactWriter;
use vcp_store::contract::{CanonicalStore, Mutation, Record, Transaction};
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

fn read_tools() -> CanonicalTools {
    serde_json::from_value(serde_json::json!(["vcp_read", "vcp_list", "vcp_search"])).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_tool_ceiling_is_immutable_inherited_and_restored_on_both_stores() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let config = config(&temp.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        task(&host, &config, config.root_task.clone(), None);
        let child = TaskId::new();
        task(
            &host,
            &config,
            child.clone(),
            Some(config.root_task.clone()),
        );
        assert!(host
            .canonical_tools(config.root_task.clone())
            .unwrap()
            .is_all());
        host.configure_canonical_tools(read_tools()).unwrap();
        host.configure_canonical_tools(read_tools()).unwrap();
        assert!(host
            .configure_canonical_tools(CanonicalTools::default())
            .is_err());
        assert_eq!(host.canonical_tools(child.clone()).unwrap(), read_tools());
        owner.close().await.unwrap();
        drop(host);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        assert_eq!(
            host.canonical_tools(config.root_task.clone()).unwrap(),
            read_tools()
        );
        assert_eq!(host.canonical_tools(child).unwrap(), read_tools());
        assert!(host
            .configure_canonical_tools(CanonicalTools::default())
            .is_err());
        host.configure_canonical_tools(read_tools()).unwrap();
        owner.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_tool_ceiling_filters_provider_and_preserves_host_analysis_completion() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for forbidden in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let workspace = temp.path().join("workspace");
            std::fs::create_dir(&workspace).unwrap();
            let workspace = workspace.canonicalize().unwrap();
            std::fs::write(workspace.join("file.txt"), "unchanged source").unwrap();
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
                        mode: Autonomy::Plan,
                        denials: vec![],
                        workspace_roots: BTreeSet::from([
                            RootId::parse(config.workspace.as_str()).unwrap()
                        ]),
                        automatic_effects: BTreeSet::from([EffectClass::Read]),
                        timeout_ceiling_ms: Units::new(30_000),
                        output_ceiling_bytes: ByteCount::new(1024 * 1024),
                    },
                },
                None,
                Revision::ZERO,
            )
            .unwrap();
            host.configure_canonical_tools(read_tools()).unwrap();
            let (snapshot, raw) = provider_snapshot();
            host.configure_provider(snapshot, raw).unwrap();
            let observed = Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
            let requests = observed.clone();
            let server = start_mock_server().await;
            Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request: &wiremock::Request| {
                requests.lock().unwrap().push(serde_json::from_slice(&request.body).unwrap());
                let response = if forbidden {
                    let call = serde_json::json!({"type":"function_call","id":"excluded-item","call_id":"excluded-call","name":"vcp_verify","arguments":"{\"citations\":[]}","status":"completed"});
                    sse(vec![serde_json::json!({"type":"response.output_item.done","output_index":0,"item":call}),
                        serde_json::json!({"type":"response.completed","response":{"id":"excluded-response","status":"completed","output":[call],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}})])
                } else {
                    sse(vec![ev_assistant_message("analysis", "The observed source remains unchanged."),
                        serde_json::json!({"type":"response.completed","response":{"id":"analysis-response","status":"completed","output":[],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}})])
                };
                ResponseTemplate::new(200).insert_header("content-type", "text/event-stream").set_body_string(response)
            }).mount(&server).await;
            let mut registry = ExtensionRegistryBuilder::new();
            registry.turn_start_admission(Arc::new(host.clone()));
            registry.work_admission(Arc::new(host.clone()));
            registry.tool_contributor(Arc::new(host.clone()));
            let starter = host.clone();
            let cwd = workspace.clone();
            let test = test_codex()
                .with_extensions(Arc::new(registry.build()))
                .with_auth(codex_login::CodexAuth::from_api_key(
                    "synthetic-tool-ceiling",
                ))
                .with_allowed_tools(
                    host.canonical_tools(config.root_task.clone())
                        .unwrap()
                        .allowed_tools(),
                )
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
            host.configure_verification(
                thread,
                VerificationConfig {
                    requirements: vec![],
                    rationale: "unchanged analysis".into(),
                },
            )
            .unwrap();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            let mut coding = CodingConfig {
                canonical_tools: CanonicalTools::default(),
                operating: "Inspect only; final integrity completion is host-owned.".into(),
                affected_paths: vec!["file.txt".into()],
                max_requests: 2,
                deadline: Timestamp::new(now + 300_000),
            };
            assert!(host.configure_coding(thread, coding.clone()).is_err());
            coding.canonical_tools = read_tools();
            host.configure_coding(thread, coding).unwrap();
            test.codex
                .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                    text: "Analyze the source without model verification.".into(),
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
            let requests = observed.lock().unwrap().clone();
            assert_eq!(requests.len(), 1);
            let names: BTreeSet<_> = requests[0]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value["name"].as_str().unwrap())
                .collect();
            assert_eq!(
                names,
                BTreeSet::from(["vcp_read", "vcp_list", "vcp_search"])
            );
            assert!(!requests[0]
                .to_string()
                .contains("Use vcp_mcp list/resources/prompts"));
            assert!(codex_extension_api::HostWorkAdmission::admit_tool(
                &host,
                thread,
                "excluded-call",
                &codex_extension_api::ToolName::plain("vcp_verify")
            )
            .is_err());
            if forbidden {
                assert!(host.complete_coding_turn(thread).is_err());
            } else {
                host.complete_coding_turn(thread).unwrap();
            }
            assert_eq!(
                std::fs::read_to_string(workspace.join("file.txt")).unwrap(),
                "unchanged source"
            );
            assert!(!host
                .snapshot()
                .unwrap()
                .records
                .values()
                .any(|r| r.collection == Collection::Effect));
            owner.close().await.unwrap();
            test.codex.shutdown_and_wait().await.unwrap();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_tool_ceiling_missing_marker_fails_closed_for_root_and_child_history() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for (child_history, restricted, malformed) in [
            (false, true, false),
            (true, true, false),
            (true, false, false),
            (true, true, true),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let workspace = temp.path().join("workspace");
            std::fs::create_dir(&workspace).unwrap();
            let cfg = config(&temp.path().join("canonical"), &workspace, backend);
            let (host, owner) = CanonicalHost::open(cfg.clone()).unwrap();
            let root = task(&host, &cfg, cfg.root_task.clone(), None);
            let binding = if child_history {
                task(&host, &cfg, TaskId::new(), Some(cfg.root_task.clone()))
            } else {
                root
            };
            owner.close().await.unwrap();
            drop(host);
            // Retain independent restricted coding history without its ceiling marker.
            let mut store = vcp_store::Store::open(&cfg.canonical_root, backend, &[])
                .await
                .unwrap();
            let spec = ArtifactSpec {
                id: ArtifactId::new(),
                scope: binding.scope.clone(),
                media_type: "application/json".into(),
                schema: "canonical-coding-configuration/1".into(),
                source: "missing ceiling fixture".into(),
                channel: Channel::Evidence,
                retention: "history".into(),
                omissions: vec![],
            };
            let mut writer = store.spool().create(spec).unwrap();
            writer
                .write_chunk(if malformed {
                    b"null"
                } else if restricted {
                    br#"{"canonical_tools":["vcp_read","vcp_list","vcp_search"]}"#
                } else {
                    b"{}"
                })
                .unwrap();
            let artifact = writer.finalize().unwrap();
            drop(writer);
            store
                .transact(Transaction {
                    id: TransactionId::new(),
                    expected_watermark: store.state().watermark,
                    mutations: vec![Mutation::Put {
                        record: Record::typed(
                            Collection::Artifact,
                            artifact.spec.id.as_str(),
                            cfg.workspace.clone(),
                            Revision::ZERO,
                            &artifact,
                        )
                        .unwrap(),
                        expected: None,
                    }],
                    events: vec![],
                    command: None,
                })
                .await
                .unwrap();
            drop(store);
            let (host, owner) = CanonicalHost::open(cfg.clone()).unwrap();
            if restricted {
                assert!(host.canonical_tools(cfg.root_task.clone()).is_err());
                assert!(host.startup_canonical_tools(binding.scope.task).is_err());
                assert!(host
                    .configure_canonical_tools(CanonicalTools::default())
                    .is_err());
            } else {
                assert!(host
                    .canonical_tools(cfg.root_task.clone())
                    .unwrap()
                    .is_all());
            }
            assert!(host.configure_canonical_tools(read_tools()).is_err());
            owner.close().await.unwrap();
        }
    }
}
