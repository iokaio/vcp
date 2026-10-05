// SPDX-License-Identifier: Apache-2.0
use super::*;
use codex_protocol::ThreadId;
use std::{
    collections::BTreeSet,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};
use vcp_domain::policy::{Autonomy, EffectClass, Policy};
use vcp_lifecycle::foundation::coding::{allowed_tools, CodingConfig};
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn retained_artifact_ranges_preserve_scope_bytes_and_tool_provenance() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        std::fs::write(workspace.join("anchor.txt"), "unchanged fixture source").unwrap();
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
        let (snapshot, raw) = provider_snapshot_capacity(240_000, 200_000);
        host.configure_provider(snapshot, raw).unwrap();
        let server = start_mock_server().await;
        let mut registry = ExtensionRegistryBuilder::new();
        registry.turn_start_admission(Arc::new(host.clone()));
        registry.work_admission(Arc::new(host.clone()));
        registry.tool_contributor(Arc::new(host.clone()));
        let starter = host.clone();
        let cwd = workspace.clone();
        let test = test_codex()
            .with_extensions(Arc::new(registry.build()))
            .with_auth(codex_login::CodexAuth::from_api_key(
                "synthetic-artifact-fixture",
            ))
            .with_allowed_tools(allowed_tools())
            .with_config(move |c| {
                c.cwd = cwd.try_into().unwrap();
                configure_provider_fixture(c);
                starter
                    .lifecycle()
                    .authorize_startup(c.cwd.as_path(), None)
                    .unwrap();
            })
            .build_with_auto_env(&server)
            .await
            .unwrap();
        let thread = host.lifecycle().attach_root(test.codex.clone()).unwrap();
        host.register(thread, binding).unwrap();
        let source = host
            .capture(
                thread,
                Channel::Stdout,
                format!("αβ{}", "x".repeat(70_000)).into_bytes(),
            )
            .unwrap();
        let child = task(
            &host,
            &config,
            TaskId::new(),
            Some(config.root_task.clone()),
        );
        let child_thread = ThreadId::new();
        host.register(child_thread, child).unwrap();
        let foreign = host
            .capture(
                child_thread,
                Channel::Stdout,
                b"foreign private evidence".to_vec(),
            )
            .unwrap();
        let missing = ArtifactId::new();
        let arguments = vec![
            serde_json::json!({"artifact":source.spec.id,"offset":2,"length":2}),
            serde_json::json!({"artifact":source.spec.id,"offset":1,"length":1}),
            serde_json::json!({"artifact":foreign.spec.id,"offset":0,"length":64}),
            serde_json::json!({"artifact":missing,"offset":0,"length":64}),
            serde_json::json!({"artifact":source.spec.id,"offset":0,"length":65537}),
            serde_json::json!({"artifact":source.spec.id,"offset":0,"length":65536}),
        ];
        let requests = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let observed = requests.clone();
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request: &wiremock::Request| {
            let index = calls.fetch_add(1, Ordering::SeqCst);
            observed.lock().unwrap().push(serde_json::from_slice(&request.body).unwrap());
            let mut events = vec![];
            let mut output = vec![];
            if let Some(arguments) = arguments.get(index) {
                let item = serde_json::json!({"type":"function_call","id":format!("item-{index}"),"call_id":format!("artifact-{index}"),"name":"vcp_artifact_read","arguments":arguments.to_string(),"status":"completed"});
                events.push(serde_json::json!({"type":"response.output_item.done","output_index":0,"item":item})); output.push(item);
            } else { events.push(ev_assistant_message("final", "Observed retained artifact ranges.")); }
            events.push(serde_json::json!({"type":"response.completed","response":{"id":format!("artifact-response-{index}"),"status":"completed","output":output,"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}}));
            ResponseTemplate::new(200).insert_header("content-type", "text/event-stream").set_body_string(sse(events))
        }).mount(&server).await;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        host.configure_coding(
            thread,
            CodingConfig {
                canonical_tools: Default::default(),
                operating: "Read only retained ranges; unavailable evidence remains unavailable."
                    .into(),
                affected_paths: vec!["anchor.txt".into()],
                max_requests: 12,
                deadline: Timestamp::new(now + 600_000).into(),
            },
        )
        .unwrap();
        host.begin_coding_turn(
            thread,
            "Read the supplied artifacts without changing files.".into(),
        )
        .unwrap();
        test.codex
            .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                text: "Read the supplied artifacts without changing files.".into(),
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
        assert_eq!(count.load(Ordering::SeqCst), 7, "{backend:?}");
        let requests = requests.lock().unwrap();
        let results: Vec<serde_json::Value> = (0..6)
            .map(|index| {
                let id = format!("artifact-{index}");
                let item = requests[index + 1]["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|item| item["type"] == "function_call_output" && item["call_id"] == id)
                    .unwrap();
                serde_json::from_str(item["output"].as_str().unwrap()).unwrap()
            })
            .collect();
        assert_eq!(results[0]["content"], "β");
        assert_eq!(results[0]["source_sha256"], source.sha256);
        assert_eq!(results[0]["range"], serde_json::json!({"start":2,"end":4}));
        assert_eq!(results[1]["encoding"], "hex");
        assert_eq!(results[1]["content"], "b1");
        for index in [2, 3] {
            assert_eq!(results[index]["availability"], "unavailable");
            assert!(results[index]["content"].is_null());
        }
        assert!(results[4].to_string().contains("1..65536"));
        assert_eq!(results[5]["content"].as_str().unwrap().len(), 65536);
        assert_eq!(results[5]["next_offset"], 65536);
        let pairs: Vec<serde_json::Value> = host
            .snapshot()
            .unwrap()
            .records
            .values()
            .filter(|record| record.collection == Collection::Artifact)
            .map(|record| record.decode::<ArtifactDescriptor>().unwrap())
            .filter(|artifact| artifact.spec.schema == "canonical-coding-pair/1")
            .map(|artifact| {
                serde_json::from_slice(&host.read_artifact(artifact.spec.id).unwrap()).unwrap()
            })
            .collect();
        for index in 0..6 {
            let pair = pairs
                .iter()
                .find(|pair| pair["parts"][0]["content"]["id"] == format!("artifact-{index}"))
                .unwrap();
            assert_eq!(pair["parts"][1]["trust"], "untrusted");
            assert_eq!(
                pair["parts"][1]["scope"]["task"],
                config.root_task.to_string()
            );
            let sources = pair["sources"].as_array().unwrap();
            assert!(!sources.contains(&serde_json::json!(foreign.spec.id)));
            if [0, 1, 5].contains(&index) {
                assert!(sources.contains(&serde_json::json!(source.spec.id)));
            }
        }
        drop(requests);
        assert_eq!(host.read_artifact(source.spec.id).unwrap().len(), 70_004);
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
    }
}
