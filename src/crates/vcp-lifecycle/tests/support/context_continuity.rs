// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};
use vcp_domain::policy::{Autonomy, EffectClass, Policy};
use vcp_lifecycle::foundation::{
    coding::{allowed_tools, CodingConfig},
    verification::VerificationConfig,
};
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn retained_compaction_preserves_current_facts_originals_and_unknown_cost_on_reopen() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        run(backend, false).await;
        run(backend, true).await;
    }
}
async fn run(backend: BackendKind, oversized: bool) {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    fs::write(workspace.join("AGENTS.md"), "Original scoped guidance\ndata: {\"reasoning\":\"repository fixture, not provider state\"}\n").unwrap();
    fs::write(
        workspace.join("evidence.txt"),
        format!(
            "Obsolete assumption: answer is 41. {}",
            "synthetic historical evidence; ".repeat(200)
        ),
    )
    .unwrap();
    let config = config(&temp.path().join("canonical"), &workspace, backend);
    let server = start_mock_server().await;
    let count = Arc::new(AtomicUsize::new(0));
    let requests = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let observed = requests.clone();
    let calls = count.clone();
    Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request: &wiremock::Request| {
        let n = calls.fetch_add(1, Ordering::SeqCst);
        observed.lock().unwrap().push(serde_json::from_slice(&request.body).unwrap());
        let mut events = vec![];
        let mut output = vec![];
        if n < 5 {
            let item = serde_json::json!({"type":"function_call","id":format!("item-{n}"),"call_id":format!("read-{n}"),"name":"vcp_read","arguments":serde_json::json!({"path":"evidence.txt","max_bytes":8192,"start_line":null,"end_line":null}).to_string(),"status":"completed"});
            events.push(serde_json::json!({"type":"response.output_item.done","output_index":0,"item":item}));
            output.push(item);
        } else { events.push(ev_assistant_message("final", "Observed historical sources; no completion claim.")); }
        events.push(serde_json::json!({"type":"response.completed","response":{"id":format!("continuity-{n}"),"status":"completed","output":output,"reasoning_details":{"opaque":"synthetic-private-provider-state"},"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":if n==6 {serde_json::Value::Null}else{serde_json::json!(0.0001)}}}}));
        ResponseTemplate::new(200).insert_header("content-type","text/event-stream").set_body_string(sse(events))
    }).mount(&server).await;
    let mut originals: BTreeMap<ArtifactId, Vec<u8>> = BTreeMap::new();
    for phase in 0..3 {
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let binding = if phase == 0 {
            task(&host, &config, config.root_task.clone(), None)
        } else {
            ThreadBinding {
                scope: Scope {
                    workspace: config.workspace.clone(),
                    session: config.session.clone(),
                    task: config.root_task.clone(),
                },
                agent: AgentId::new(),
                role: RequestRole::Main,
            }
        };
        if phase == 0 {
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
        }
        let (snapshot, raw) = provider_snapshot();
        // P7-02 added 891 schema bytes and 38 quoted bytes per nullable read.
        // Completion guidance adds 372 read/patch, 305 citation and 465 process
        // discovery/verification schema bytes.
        // P7-05 numeric/null descriptions add 198 serialized read bytes and
        // 337 search bytes, preserving the same source/history budget below.
        // Offset exactly that later growth to preserve this fixture's effective
        // source/history budget without changing the shared provider fixture.
        let mut endpoint: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        endpoint["data"]["endpoints"][0]["max_prompt_tokens"] = serde_json::json!(26_677);
        let raw = serde_json::to_vec(&endpoint).unwrap();
        let snapshot = vcp_models::catalog::Snapshot::from_endpoints(
            &raw,
            snapshot.observed_at,
            snapshot.valid_until,
            snapshot.compatibility,
        )
        .unwrap();
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
                "synthetic-continuity-key",
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
        let thread = host.lifecycle().attach_root(test.codex.clone()).unwrap();
        host.register(thread, binding.clone()).unwrap();
        if phase > 0 {
            let task = host.project().unwrap().tasks[&config.root_task].clone();
            host.resume(thread, task.revision, task.fingerprint)
                .unwrap();
        }
        if phase == 1 {
            let task = host.project().unwrap().tasks[&config.root_task].clone();
            host.command(
                Command::Steer {
                    objective: Objective {
                        text: "Corrected answer is 42".into(),
                        constraints: vec!["Never treat the earlier assumption as current".into()],
                        acceptance: vec!["cite observed facts".into()],
                        source: EventId::new(),
                        steering: task.steering.next().unwrap(),
                    },
                },
                Some(config.root_task.clone()),
                task.revision,
            )
            .unwrap();
        }
        host.configure_verification(
            thread,
            VerificationConfig {
                requirements: vec![],
                rationale: "Preserve the original analysis baseline".into(),
            },
        )
        .unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        host.configure_coding(
            thread,
            CodingConfig {
                operating: "Use observed sources and preserve uncertainty".into(),
                affected_paths: vec!["evidence.txt".into()],
                max_requests: 10,
                deadline: Timestamp::new(now + 600_000),
            },
        )
        .unwrap();
        host.configure_continuity(
            thread,
            vcp_context::compaction::Config {
                keep_recent_pairs: 1,
                // The 26,677 prompt ceiling minus the 512 safety reserve leaves
                // 26,165 input bytes. A larger historical preview must still fail
                // closed when current facts and the recent pair cannot fit.
                preview_bytes: if oversized && phase == 2 { 1024 } else { 64 },
                minimum_gain_bytes: 256,
            },
        )
        .unwrap();
        for (id, bytes) in &originals {
            assert_eq!(
                &host.read_artifact(id.clone()).unwrap(),
                bytes,
                "original history survives reopen"
            );
        }
        test.codex
            .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                text: "Inspect current canonical state".into(),
                text_elements: vec![],
            }]))
            .await
            .unwrap();
        let mut errors = Vec::new();
        tokio::time::timeout(Duration::from_secs(300), async {
            loop {
                match test.codex.next_event().await.unwrap().msg {
                    EventMsg::TurnComplete(_) => break,
                    EventMsg::Error(error) => errors.push(format!("{error:?}")),
                    _ => {}
                }
            }
        })
        .await
        .unwrap();
        if oversized && phase == 2 {
            assert_eq!(count.load(Ordering::SeqCst), 7, "no oversized HTTP request");
            assert!(
                errors
                    .iter()
                    .any(|e| e.contains("required context cannot fit")),
                "{errors:?}"
            );
            assert_eq!(
                host.project().unwrap().tasks[&config.root_task].state,
                TaskState::Paused
            );
            let state = host.snapshot().unwrap();
            let ledger: Ledger = state
                .record(
                    Collection::Ledger,
                    config.root_task.as_str(),
                    &config.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(serde_json::to_value(ledger).unwrap()["unresolved"], "100");
            for (id, bytes) in &originals {
                assert_eq!(&host.read_artifact(id.clone()).unwrap(), bytes);
            }
            owner.close().await.unwrap();
            test.codex.shutdown_and_wait().await.unwrap();
            continue;
        }
        if count.load(Ordering::SeqCst) != 6 + phase {
            for record in host
                .snapshot()
                .unwrap()
                .records
                .values()
                .filter(|r| r.collection == Collection::Artifact)
            {
                let artifact: ArtifactDescriptor = record.decode().unwrap();
                if artifact.spec.schema == "canonical-compaction-projection/1" {
                    let value: serde_json::Value =
                        serde_json::from_slice(&host.read_artifact(artifact.spec.id).unwrap())
                            .unwrap();
                    eprintln!(
                        "projection bytes: before={} after={} steering={}",
                        value["input_estimate_before"],
                        value["input_estimate_after"],
                        value["projection"]["revisions"]["steering"]
                    );
                } else if artifact.spec.schema == "canonical-coding-content/1" {
                    let bytes = host.read_artifact(artifact.spec.id).unwrap();
                    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                        if value["schema"] == "canonical-current-continuity/1" {
                            let sizes: BTreeMap<_, _> = value
                                .as_object()
                                .unwrap()
                                .iter()
                                .map(|(key, value)| (key, value.to_string().len()))
                                .collect();
                            eprintln!("current fact field bytes: {sizes:?}");
                        }
                    }
                }
            }
        }
        assert_eq!(
            count.load(Ordering::SeqCst),
            6 + phase,
            "{backend:?}/phase-{phase}; errors: {errors:?}; task: {:?}",
            host.project().unwrap().tasks[&config.root_task]
        );
        let state = host.snapshot().unwrap();
        let artifacts: Vec<ArtifactDescriptor> = state
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
            .map(|r| r.decode().unwrap())
            .collect();
        let projections: Vec<_> = artifacts
            .iter()
            .filter(|a| a.spec.schema == "canonical-compaction-projection/1")
            .collect();
        assert!(!projections.is_empty());
        let handoffs: Vec<_> = artifacts
            .iter()
            .filter(|a| a.spec.schema == "canonical-context-handoff/1")
            .collect();
        assert_eq!(handoffs.len(), count.load(Ordering::SeqCst));
        let mut saw_opaque_omission = false;
        for artifact in handoffs {
            let bytes = host.read_artifact(artifact.spec.id.clone()).unwrap();
            let packet: vcp_context::handoff::Packet = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(packet.manifest.revisions.scope, binding.scope);
            assert!(packet.discarded.iter().all(|field| packet
                .references
                .iter()
                .any(|source| source.spec.id == field.artifact
                    && source.spec.channel == Channel::Response)));
            assert!(packet.current_state["task"]["objectives"].is_array());
            assert!(packet.current_state["continuity"]["original_base"].is_object());
            assert_eq!(
                packet.remaining.get(),
                packet.ledger.cap.get().saturating_sub(
                    packet.ledger.settled.get()
                        + packet.ledger.active.get()
                        + packet.ledger.unresolved.get()
                        + packet.ledger.protected.get()
                )
            );
            for reference in &packet.references {
                let original = host.read_artifact(reference.spec.id.clone()).unwrap();
                assert_eq!(vcp_protocol::digest_bytes(&original), reference.sha256);
            }
            if packet
                .discarded
                .iter()
                .any(|d| d.field.ends_with("/reasoning_details"))
            {
                saw_opaque_omission = true;
                // Opaque values remain only in originals, never portable text.
                assert!(!String::from_utf8(bytes.clone())
                    .unwrap()
                    .contains("synthetic-private-provider-state"));
            }
            if packet.manifest.revisions.steering.get() > 0 {
                let workspace = &packet.current_state["workspace"];
                assert!(workspace["changes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|c| c["path"] == "evidence.txt"));
                let source = workspace["sources"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|s| s["version"]["path"] == "evidence.txt")
                    .unwrap();
                let id: ArtifactId = serde_json::from_value(source["artifact"].clone()).unwrap();
                assert!(packet.references.iter().any(|r| r.spec.id == id));
                assert_eq!(
                    host.read_artifact(id).unwrap(),
                    b"Current independently edited source: 42"
                );
            }
        }
        assert!(saw_opaque_omission);
        for artifact in &projections {
            let projection: serde_json::Value =
                serde_json::from_slice(&host.read_artifact(artifact.spec.id.clone()).unwrap())
                    .unwrap();
            assert!(projection["gain"].as_u64().unwrap() >= 256);
            assert!(
                projection["input_estimate_after"].as_u64().unwrap()
                    < projection["input_estimate_before"].as_u64().unwrap()
            );
            assert!(
                projection["projection"]["compacted_pairs"]
                    .as_u64()
                    .unwrap()
                    >= 1
            );
        }
        let last = requests.lock().unwrap().last().unwrap().clone();
        let quoted: Vec<serde_json::Value> = last["input"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["type"] == "message")
            .map(|i| serde_json::from_str(i["content"][0]["text"].as_str().unwrap()).unwrap())
            .collect();
        assert!(quoted
            .iter()
            .any(|p| p["kind"] == "history" && p["trust"] == "untrusted"));
        assert_eq!(
            last["input"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|i| i["type"] == "function_call")
                .count(),
            1
        );
        let facts = quoted
            .iter()
            .filter_map(|p| serde_json::from_str::<serde_json::Value>(p["text"].as_str()?).ok())
            .find(|v| v["schema"] == "canonical-current-continuity/1")
            .unwrap();
        assert_eq!(facts["effects"].as_array().unwrap().len(), 5);
        assert_ne!(facts["original_base"], serde_json::Value::Null);
        if phase > 0 {
            assert!(quoted.iter().any(|p| {
                p["kind"] == "objective"
                    && p["text"]
                        .as_str()
                        .unwrap()
                        .contains("Corrected answer is 42")
            }));
            assert_ne!(facts["original_base"], facts["current_source"]);
        }
        if phase == 2 {
            assert_eq!(facts["ledger"]["unresolved"], "100");
            let pending = facts["uncertain_attempts"].as_array().unwrap();
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0]["held_liability"], "100");
            assert_eq!(pending[0]["quoted_reserve"]["micros"], "100");
            assert_eq!(pending[0]["quoted_reserve"]["currency"], "USD");
            assert!(pending[0]["source_records_sha256"].as_str().unwrap().len() == 64);
        }
        if phase == 1 {
            assert_eq!(
                host.project().unwrap().tasks[&config.root_task].state,
                TaskState::Paused
            );
        }
        if phase == 0 {
            for artifact in artifacts
                .iter()
                .filter(|a| a.spec.schema == "canonical-coding-pair/1")
            {
                originals.insert(
                    artifact.spec.id.clone(),
                    host.read_artifact(artifact.spec.id.clone()).unwrap(),
                );
            }
            assert_eq!(originals.len(), 5);
        }
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        drop(test);
        drop(host);
        let store = vcp_store::Store::open(&config.canonical_root, backend, &[])
            .await
            .unwrap();
        let retained_workspace: vcp_domain::workspace::Workspace = store
            .state()
            .record(
                Collection::Workspace,
                config.workspace.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let access = vcp_memory::access::Access {
            workspace: config.workspace.clone(),
            actor: config.actor.clone(),
            authority: retained_workspace.authority,
            read: true,
            write: false,
            tasks: Some(BTreeSet::from([config.root_task.clone()])),
        };
        let before = store.state().clone();
        let diagnostics =
            vcp_lifecycle::foundation::routing_state::compaction_diagnostics::observe(
                &store,
                &access,
                vcp_lifecycle::foundation::routing_state::HistoryWindow {
                    from: None,
                    until: Timestamp::new(u64::MAX),
                },
            )
            .unwrap();
        assert!(!diagnostics.entries.is_empty());
        assert!(
            diagnostics
                .entries
                .iter()
                .any(|entry| entry.consumed_by.is_some()),
            "real submitted compacted context must be linked: {diagnostics:?}"
        );
        assert!(diagnostics
            .entries
            .iter()
            .all(|entry| entry.abstention.is_some()
                || (entry.before.is_some() && entry.after.is_some())));
        let rendered = serde_json::to_string(&diagnostics).unwrap();
        assert!(!rendered.contains("synthetic historical evidence"));
        assert!(!rendered.contains("Obsolete assumption"));
        assert!(!diagnostics.serving_qualified);
        assert_eq!(store.state(), &before);
        drop(store);
        if phase == 0 {
            fs::write(
                workspace.join("AGENTS.md"),
                "Current guidance: corrected answer is 42",
            )
            .unwrap();
            fs::write(
                workspace.join("evidence.txt"),
                "Current independently edited source: 42",
            )
            .unwrap();
        }
    }
}
