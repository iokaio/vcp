// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};
use vcp_domain::{
    policy::*,
    verification::{CheckOutcome, Verification},
};
use vcp_lifecycle::foundation::{
    coding::{allowed_tools, CodingConfig},
    verification::VerificationConfig,
};
use vcp_tools::{
    process::{Mode, Profile},
    verification::{Requirement, Runner},
};
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn retained_verification_checks_changed_source_and_accounts_final_response() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for mode in [
            "pass",
            "failed",
            "missing",
            "siblings",
            "later_effect",
            "denied",
            "denied_context",
        ] {
            run(backend, mode).await;
        }
    }
}
async fn run(backend: BackendKind, mode: &'static str) {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    let node = temp.path().join("node.exe");
    fs::copy(
        std::env::var_os("VCP_TEST_NODE").expect("explicit native Node"),
        &node,
    )
    .unwrap();
    let oracle = temp.path().join("assertion-observed");
    fs::write(workspace.join("value.txt"), b"41\n").unwrap();
    fs::write(
        workspace.join("AGENTS.md"),
        b"Verify the changed value with the configured acceptance check.\n",
    )
    .unwrap();
    fs::write(
        workspace.join("package.json"),
        br#"{"scripts":{"test":"node --test acceptance.cjs"}}"#,
    )
    .unwrap();
    fs::write(workspace.join("acceptance.cjs"), format!("const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs');test('changed_value',()=>{{fs.writeFileSync({},'ran');assert.equal(fs.readFileSync('value.txt','utf8').trim(),'42');}});", serde_json::to_string(&oracle).unwrap())).unwrap();
    let mut config = config(&temp.path().join("canonical"), &workspace, backend);
    if matches!(mode, "denied" | "denied_context") {
        config.host_tool_denials.push(Denial {
            id: "verification-ceiling".into(),
            origin: RuleOrigin::Host,
            reason: "synthetic named verification ceiling".into(),
            effects: BTreeSet::from([if mode == "denied_context" {
                EffectClass::Read
            } else {
                EffectClass::Execute
            }]),
            tool: Some("vcp_verify".into()),
            roots: BTreeSet::new(),
            paths: vec![],
        });
    }
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
                    RootId::parse("exec-node").unwrap(),
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
        Profile::new(
            "node".into(),
            node,
            Mode::Direct,
            BTreeMap::from([("SystemRoot".into(), std::env::var("SystemRoot").unwrap())]),
            BTreeSet::new(),
            true,
        )
        .unwrap(),
    )
    .unwrap();
    let (snapshot, raw) = provider_snapshot();
    host.configure_provider(snapshot, raw).unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = requests.clone();
    let calls = count.clone();
    let server = start_mock_server().await;
    Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request:&wiremock::Request| {
        let index = calls.fetch_add(1,Ordering::SeqCst);
        observed.lock().unwrap().push(serde_json::from_slice::<serde_json::Value>(&request.body).unwrap());
        let mut events=Vec::new();
        let mut output=Vec::new();
        let selected = match index {
            0 => Some(("vcp_read",serde_json::json!({"path":"value.txt","max_bytes":1024}))),
            1 => Some(("vcp_patch",serde_json::json!({"patch":format!("*** Begin Patch\n*** Update File: value.txt\n@@\n-41\n+{}\n*** End Patch",if mode=="failed" {43}else{42})}))),
            2 if mode != "missing" => Some(("vcp_verify",serde_json::json!({"citations":[]}))),
            3 if mode == "later_effect" => Some(("vcp_read",serde_json::json!({"path":"value.txt","max_bytes":1024}))),
            3 if mode == "siblings" => Some(("vcp_verify",serde_json::json!({"citations":[]}))),
            _ => None,
        };
        if let Some((name,arguments))=selected {
            let item=serde_json::json!({"type":"function_call","id":format!("item-{index}"),"call_id":format!("call-{index}"),"name":name,"arguments":arguments.to_string(),"status":"completed"});
            events.push(serde_json::json!({"type":"response.output_item.done","output_index":0,"item":item})); output.push(item);
            if mode=="siblings" && index==2 {
                let item=serde_json::json!({"type":"function_call","id":"item-stale","call_id":"call-stale","name":"vcp_patch","arguments":serde_json::json!({"patch":"*** Begin Patch\n*** Add File: must-not-exist.txt\n+stale\n*** End Patch"}).to_string(),"status":"completed"});
                events.push(serde_json::json!({"type":"response.output_item.done","output_index":1,"item":item})); output.push(item);
            }
        } else { events.push(ev_assistant_message("final", "Done; the observed check result is retained.")); }
        events.push(serde_json::json!({"type":"response.completed","response":{"id":format!("verified-loop-{index}"),"status":"completed","output":output,"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}}));
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
        .with_auth(codex_login::CodexAuth::from_api_key(
            "synthetic-loop-verification",
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
    host.configure_verification(
        thread,
        VerificationConfig {
            requirements: vec![Requirement {
                manifest: "package.json".into(),
                runner: Runner::Node,
                profile: "node".into(),
                expected_tests: vec!["changed_value".into()],
                rationale: "Assert the source changed by the retained patch".into(),
            }],
            rationale: "Native retained-loop acceptance".into(),
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
            operating: "Read, edit, verify, then report actual results.".into(),
            affected_paths: vec!["value.txt".into()],
            max_requests: 8,
            deadline: Timestamp::new(now + 600_000),
        },
    )
    .unwrap();
    assert!(host.complete_coding_turn(thread).is_err());
    test.codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "Run the accepted task".into(),
            text_elements: vec![],
        }]))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(300), async {
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
    let expected = if mode == "denied_context" {
        0
    } else if mode == "missing" {
        3
    } else if matches!(mode, "later_effect" | "siblings") {
        5
    } else {
        4
    };
    assert_eq!(count.load(Ordering::SeqCst), expected, "{backend:?}/{mode}");
    assert_eq!(
        fs::read_to_string(workspace.join("value.txt"))
            .unwrap()
            .trim(),
        if mode == "denied_context" {
            "41"
        } else if mode == "failed" {
            "43"
        } else {
            "42"
        }
    );
    assert_eq!(
        oracle.exists(),
        !matches!(mode, "missing" | "denied" | "denied_context"),
        "actual independent test execution: {mode}"
    );
    assert!(!workspace.join("must-not-exist.txt").exists());
    let before = host.snapshot().unwrap();
    let reports: Vec<Verification> = before
        .records
        .values()
        .filter(|r| r.collection == Collection::Verification)
        .map(|r| r.decode().unwrap())
        .collect();
    if !matches!(mode, "missing" | "denied" | "denied_context") {
        assert_eq!(reports.len(), 1, "{mode}");
        assert_eq!(
            reports[0].checks[0].outcome == CheckOutcome::Passed,
            mode != "failed",
            "{mode}: {:?}",
            reports[0]
        );
    } else {
        assert!(reports.is_empty());
    }
    if mode == "siblings" {
        let last = requests.lock().unwrap().last().unwrap().to_string();
        assert!(
            last.contains("call-stale") && last.contains("isolated response"),
            "unexecuted mixed-call pair survives reassembly: {last}"
        );
        assert_eq!(host.project().unwrap().effects.len(), 3);
    }
    let complete = host.complete_coding_turn(thread);
    if matches!(mode, "pass" | "siblings") {
        complete.unwrap_or_else(|e| panic!("{backend:?}/{mode}: {e}"));
        let state = host.snapshot().unwrap();
        let refreshed: Vec<Verification> = state
            .records
            .values()
            .filter(|r| r.collection == Collection::Verification)
            .map(|r| r.decode().unwrap())
            .collect();
        assert_eq!(refreshed.len(), 2, "immutable final-cost refresh");
        assert!(refreshed.iter().all(|v| v.checks == reports[0].checks));
        let refresh = state
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
            .map(|r| r.decode::<ArtifactDescriptor>().unwrap())
            .find(|a| a.spec.schema == "verification-completion-refresh/1")
            .unwrap();
        let receipt: serde_json::Value =
            serde_json::from_slice(&host.read_artifact(refresh.spec.id).unwrap()).unwrap();
        assert_eq!(
            receipt["ledger"]["settled"],
            (expected as u64 * 100).to_string()
        );
        assert_eq!(receipt["ledger"]["active"], "0");
        assert_eq!(receipt["ledger"]["unresolved"], "0");
        assert_eq!(
            host.project().unwrap().tasks[&config.root_task].state,
            TaskState::Completed
        );
    } else {
        assert!(complete.is_err(), "{mode} cannot finish from model prose");
    }
    if mode == "denied_context" {
        assert!(host.project().unwrap().ledgers.is_empty());
    } else {
        assert_eq!(
            host.project().unwrap().ledgers[&config.root_task]
                .settled
                .get(),
            expected as u64 * 100
        );
    }
    assert_eq!(
        host.project().unwrap().effects.len(),
        if mode == "denied_context" {
            0
        } else if matches!(mode, "missing" | "denied") {
            2
        } else if mode == "later_effect" {
            4
        } else {
            3
        }
    );
    owner.close().await.unwrap();
    test.codex.shutdown_and_wait().await.unwrap();
}
