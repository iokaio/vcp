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
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn retained_verification_diagnostics_reach_model_without_reexecution() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for mode in ["failed_large", "not_run"] {
            run(backend, mode).await;
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owner_completion_rechecks_missing_and_stale_proof_without_inference() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for mode in ["owner_missing", "owner_later_effect", "owner_denied"] {
            run(backend, mode).await;
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owner_completion_distinguishes_instruction_refresh_and_required_approval() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for mode in ["owner_refresh", "owner_approval"] {
            run(backend, mode).await;
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owner_completion_refreshes_changed_existing_guidance_at_final() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        run(backend, "owner_changed_instructions").await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owner_completion_pauses_exact_repeated_failure_without_progress() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        run(backend, "owner_failed").await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owner_completion_repairs_then_reverifies_same_task() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        run(backend, "owner_repaired").await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn retained_focused_tool_checks_are_diagnostic_until_full_completion() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for mode in [
            "focused",
            "focused_fallback",
            "focused_denied",
            "focused_named_denied",
        ] {
            run(backend, mode).await;
        }
        // Unchanged legacy script omits focus entirely, through actual retained
        // schema/tool dispatch and ordinary full completion, not just serde.
        run(backend, "pass").await;
    }
}
async fn run(backend: BackendKind, mode: &'static str) {
    let focused = mode.starts_with("focused");
    let focused_named_denied = mode == "focused_named_denied";
    let mode = if mode == "focused_denied" || focused_named_denied {
        "denied"
    } else {
        mode
    };
    let owner_recheck = mode.starts_with("owner_");
    let scope_refresh = mode == "owner_refresh";
    let changed_instructions = mode == "owner_changed_instructions";
    let approval_required = mode == "owner_approval";
    let repair_pass = mode == "owner_repaired";
    let mode = mode.strip_prefix("owner_").unwrap_or(mode);
    let mode = if repair_pass { "failed" } else { mode };
    let mode = if scope_refresh || changed_instructions || approval_required {
        "missing"
    } else {
        mode
    };
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
    if scope_refresh {
        fs::create_dir(workspace.join("nested")).unwrap();
        fs::write(
            workspace.join("nested/AGENTS.md"),
            "Nested guidance must be observed before operation reissue.",
        )
        .unwrap();
        fs::write(
            workspace.join("nested/source.txt"),
            "Additional verification scope",
        )
        .unwrap();
    }
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
    if focused {
        fs::create_dir(workspace.join("other")).unwrap();
        fs::write(
            workspace.join("other/package.json"),
            br#"{"scripts":{"test":"node --test acceptance.cjs"}}"#,
        )
        .unwrap();
        fs::write(
            workspace.join("other/acceptance.cjs"),
            "require('node:test')('other_acceptance',()=>{});",
        )
        .unwrap();
        fs::write(
            workspace.join("other/AGENTS.md"),
            "Focused fixture guidance: review the other project before reissuing verification.",
        )
        .unwrap();
    }
    if mode == "failed_large" {
        // Actual configured check output. The mock provider cannot manufacture
        // the diagnostic; assertions below inspect its next received context.
        fs::write(workspace.join("acceptance.cjs"), format!("const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs');test('changed_value',()=>{{fs.writeFileSync({},'ran');process.stderr.write('x'.repeat(100000)+'\\nrequired source link missing: adr/002-pipe.md\\n');assert.fail('seeded document check failure');}});", serde_json::to_string(&oracle).unwrap())).unwrap();
    }
    if mode == "not_run" {
        fs::write(
            workspace.join("package.json"),
            br#"{"scripts":{"test":"unavailable-runner acceptance.cjs"}}"#,
        )
        .unwrap();
    }
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
            tool: Some(
                if focused_named_denied {
                    "vcp_verify_focused"
                } else {
                    "vcp_verify"
                }
                .into(),
            ),
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
                mode: if approval_required {
                    Autonomy::Workspace
                } else {
                    Autonomy::Autonomous
                },
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
    // The repair adds another complete tool pair after failed-check output.
    // The historical 24-KiB fixture legitimately cannot contain that request;
    // this workflow fixture declares its larger synthetic endpoint explicitly.
    let (snapshot, raw) = if owner_recheck || focused {
        provider_snapshot_capacity(240_000, 200_000)
    } else {
        provider_snapshot()
    };
    host.configure_provider(snapshot, raw).unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = requests.clone();
    let calls = count.clone();
    let server = start_mock_server().await;
    Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request:&wiremock::Request| {
        let index = calls.fetch_add(1,Ordering::SeqCst);
        let received: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        observed.lock().unwrap().push(received.clone());
        let mut events=Vec::new();
        let mut output=Vec::new();
        let selected = match index {
            0 => Some(("vcp_read",serde_json::json!({"path":"value.txt","max_bytes":1024,"start_line":null,"end_line":null}))),
            1 => Some(("vcp_patch",serde_json::json!({"patch":format!("*** Begin Patch\n*** Update File: value.txt\n@@\n-41\n+{}\n*** End Patch",if mode=="failed" {43}else{42})}))),
            2 | 3 if focused && (index == 2 || mode != "denied") => {
                if index == 3 {
                    let prior = received["input"].as_array().unwrap().iter().find(|item| item["type"] == "function_call_output" && item["call_id"] == "call-2").unwrap();
                    let prior: serde_json::Value = serde_json::from_str(prior["output"].as_str().unwrap()).unwrap();
                    assert_eq!(prior["executed"], false);
                    assert_eq!(prior["code"], "instruction_scope_refresh");
                    assert!(received.to_string().contains("Focused fixture guidance: review the other project before reissuing verification."));
                }
                Some(("vcp_verify_focused",serde_json::json!({"affected_paths":if mode=="focused" {vec!["value.txt"]} else {vec![]},"failed_checks":if mode=="focused_fallback" {vec!["unknown#test"]} else {vec![]}})))
            },
            2 if mode != "missing" => Some(("vcp_verify",serde_json::json!({"citations":[]}))),
            3 if mode == "later_effect" => Some(("vcp_read",serde_json::json!({"path":"value.txt","max_bytes":1024,"start_line":null,"end_line":null}))),
            3 if mode == "siblings" => Some(("vcp_verify",serde_json::json!({"citations":[]}))),
            4 if repair_pass => Some(("vcp_patch",serde_json::json!({"patch":"*** Begin Patch\n*** Update File: value.txt\n@@\n-43\n+42\n*** End Patch"}))),
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
            requirements: {
                let mut requirements = vec![Requirement {
                    timeout_ms: None,
                    manifest: "package.json".into(),
                    runner: Runner::Node,
                    maven: None,
                    profile: "node".into(),
                    expected_tests: vec!["changed_value".into()],
                    rationale: "Assert the source changed by the retained patch".into(),
                }];
                if focused {
                    requirements.push(Requirement {
                        timeout_ms: None,
                        manifest: "other/package.json".into(),
                        runner: Runner::Node,
                        maven: None,
                        profile: "node".into(),
                        expected_tests: vec!["other_acceptance".into()],
                        rationale: "Independent full completion requirement".into(),
                    });
                }
                requirements
            },
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
            canonical_tools: Default::default(),
            operating: "Read, edit, verify, then report actual results.".into(),
            affected_paths: vec!["value.txt".into()],
            max_requests: 8,
            deadline: Timestamp::new(now + 600_000).into(),
        },
    )
    .unwrap();
    assert!(host.complete_coding_turn(thread).is_err());
    if owner_recheck {
        host.configure_continuity(
            thread,
            vcp_lifecycle::foundation::coding::continuity_defaults(),
        )
        .unwrap();
    }
    let turn = host
        .begin_coding_turn(thread, "Run the accepted task".into())
        .unwrap();
    test.codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "Run the accepted task".into(),
            text_elements: vec![],
        }]))
        .await
        .unwrap();
    let mut loop_errors = Vec::new();
    tokio::time::timeout(Duration::from_secs(300), async {
        loop {
            match test.codex.next_event().await.unwrap().msg {
                EventMsg::Error(error) => loop_errors.push(format!("{error:?}")),
                EventMsg::TurnComplete(_) => break,
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    let expected = if mode == "denied_context" {
        0
    } else if mode == "missing" {
        3
    } else if matches!(mode, "later_effect" | "siblings") || (focused && mode != "denied") {
        5
    } else {
        4
    };
    assert_eq!(
        count.load(Ordering::SeqCst),
        expected,
        "{backend:?}/{mode}: {loop_errors:?}"
    );
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
        !matches!(mode, "missing" | "denied" | "denied_context" | "not_run"),
        "actual independent test execution: {mode}; tool result: {:?}",
        requests
            .lock()
            .unwrap()
            .last()
            .and_then(|request| request["input"].as_array())
            .and_then(|input| input
                .iter()
                .find(|item| item["type"] == "function_call_output" && item["call_id"] == "call-2"))
            .map(|item| item["output"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(4096)
                .collect::<String>())
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
            !matches!(mode, "failed" | "failed_large" | "not_run"),
            "{mode}: {:?}",
            reports[0]
        );
    } else {
        assert!(reports.is_empty());
    }
    if focused && mode != "denied" {
        let fallback = mode == "focused_fallback";
        assert_eq!(reports[0].checks.len(), if fallback { 2 } else { 1 });
        let captured = requests.lock().unwrap();
        let output = captured.last().unwrap()["input"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["type"] == "function_call_output" && item["call_id"] == "call-3")
            .unwrap();
        let output: serde_json::Value =
            serde_json::from_str(output["output"].as_str().unwrap()).unwrap();
        assert_eq!(output["diagnostic_only"], true);
        assert_eq!(
            output["diagnostics"].as_array().unwrap().len(),
            if fallback { 2 } else { 1 }
        );
        drop(captured);
        let plan = before
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
            .map(|r| r.decode::<ArtifactDescriptor>().unwrap())
            .find(|a| a.spec.schema == "verification-plan/1")
            .unwrap();
        let plan: serde_json::Value =
            serde_json::from_slice(&host.read_artifact(plan.spec.id).unwrap()).unwrap();
        assert_eq!(plan["completion"], false);
        assert_eq!(plan["selection"]["mode"], "focused");
        assert_eq!(plan["full_fallback"], fallback);
        assert_eq!(
            plan["plans"].as_array().unwrap().len(),
            if fallback { 2 } else { 1 }
        );
        assert!(host.complete_coding_turn(thread).is_err());
        assert!(matches!(
            host.try_complete_verified(thread, reports[0].id.clone())
                .unwrap(),
            vcp_lifecycle::foundation::verification::CompletionAttempt::Rejected(_)
        ));
        let full = host.verify_for_completion(thread).await.unwrap();
        assert_eq!(full.checks.len(), 2);
        assert!(full
            .checks
            .iter()
            .all(|check| check.outcome == CheckOutcome::Passed));
        assert!(matches!(
            host.try_complete_verified(thread, full.id).unwrap(),
            vcp_lifecycle::foundation::verification::CompletionAttempt::Completed(_)
        ));
        assert_eq!(
            count.load(Ordering::SeqCst),
            expected,
            "owner full verification does not dispatch inference"
        );
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        return;
    }
    if !matches!(mode, "missing" | "denied_context" | "siblings") {
        let captured = requests.lock().unwrap();
        let result = captured.last().unwrap()["input"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["type"] == "function_call_output" && item["call_id"] == "call-2")
            .unwrap();
        let result: serde_json::Value =
            serde_json::from_str(result["output"].as_str().unwrap()).unwrap();
        if mode == "denied" {
            assert!(
                result.get("diagnostics").is_none(),
                "denied workflow exposes no process output"
            );
        } else {
            let diagnostics = result["diagnostics"].as_array().unwrap();
            if mode == "not_run" {
                assert!(
                    diagnostics.is_empty(),
                    "unexecuted check has no invented output"
                );
                assert!(matches!(
                    reports[0].checks[0].outcome,
                    CheckOutcome::NotRun { .. }
                ));
            } else if mode != "siblings" {
                assert_eq!(diagnostics.len(), 1);
                assert_eq!(diagnostics[0]["specification"], "package.json#test");
                for channel in ["stdout", "stderr"] {
                    let id =
                        ArtifactId::parse(diagnostics[0][channel]["artifact"].as_str().unwrap())
                            .unwrap();
                    assert!(reports[0].outputs.contains(&id));
                    assert_eq!(
                        diagnostics[0][channel]["bytes"],
                        host.read_artifact(id).unwrap().len().to_string()
                    );
                }
                if mode == "failed" {
                    assert!(diagnostics[0].to_string().contains("43"));
                }
                if mode == "failed_large" {
                    assert!(diagnostics[0]
                        .to_string()
                        .contains("required source link missing: adr/002-pipe.md"));
                    let streams = ["stdout", "stderr"].map(|channel| &diagnostics[0][channel]);
                    assert!(streams.iter().any(|value| value["truncated"] == true));
                    assert!(streams
                        .iter()
                        .all(|value| value["tail"].as_str().unwrap().len() <= 2048));
                    let pairs: Vec<serde_json::Value> = before
                        .records
                        .values()
                        .filter(|record| record.collection == Collection::Artifact)
                        .map(|record| record.decode::<ArtifactDescriptor>().unwrap())
                        .filter(|artifact| artifact.spec.schema == "canonical-coding-pair/1")
                        .map(|artifact| {
                            serde_json::from_slice(&host.read_artifact(artifact.spec.id).unwrap())
                                .unwrap()
                        })
                        .collect();
                    let pair = pairs
                        .iter()
                        .find(|pair| pair["parts"][0]["content"]["id"] == "call-2")
                        .unwrap();
                    assert_eq!(pair["parts"][1]["trust"], "untrusted");
                    assert_eq!(
                        pair["parts"][1]["scope"]["task"],
                        config.root_task.to_string()
                    );
                    for channel in ["stdout", "stderr"] {
                        assert!(pair["sources"]
                            .as_array()
                            .unwrap()
                            .contains(&diagnostics[0][channel]["artifact"]));
                    }
                }
            }
        }
    }
    if mode == "siblings" {
        let last = requests.lock().unwrap().last().unwrap().to_string();
        assert!(
            last.contains("call-stale") && last.contains("isolated response"),
            "unexecuted mixed-call pair survives reassembly: {last}"
        );
        assert_eq!(host.project().unwrap().effects.len(), 3);
    }
    const REFRESHED_GUIDANCE: &str = "Current owner guidance changed after the final answer: verify the configured changed_value check before completion.\n";
    if changed_instructions {
        assert!(!oracle.exists());
        fs::write(workspace.join("AGENTS.md"), REFRESHED_GUIDANCE).unwrap();
    }
    let effects_before_completion = host.project().unwrap().effects;
    let complete = host.complete_coding_turn(thread);
    if owner_recheck {
        use vcp_lifecycle::foundation::verification::{CompletionAttempt, CompletionRejection};
        assert!(complete.is_err());
        let CompletionAttempt::Rejected(failure) = host.try_complete_coding_turn(thread).unwrap()
        else {
            panic!("completion must need current proof");
        };
        if approval_required {
            assert_eq!(failure.kind, CompletionRejection::MissingVerification);
            let _observed = host.verify_for_completion(thread).await;
            let CompletionAttempt::Rejected(held) = host.try_complete_coding_turn(thread).unwrap()
            else {
                panic!("approval cannot complete")
            };
            assert_eq!(held.kind, CompletionRejection::RequiredApproval);
            assert!(host.completion_repair_feedback(thread).is_err());
            let state = host.snapshot().unwrap();
            let questions: Vec<vcp_protocol::command::Approval> = state
                .records
                .values()
                .filter(|row| row.collection == Collection::Approval)
                .map(|row| row.decode().unwrap())
                .collect();
            assert_eq!(questions.len(), 1);
            assert_eq!(questions[0].scope.task, config.root_task);
            assert_eq!(
                questions[0].state,
                vcp_protocol::command::ApprovalState::Pending
            );
            assert!(
                !oracle.exists(),
                "an approval question cannot authorize the check"
            );
            assert_eq!(
                count.load(Ordering::SeqCst),
                expected,
                "approval cannot dispatch a model retry"
            );
            owner.close().await.unwrap();
            test.codex.shutdown_and_wait().await.unwrap();
            return;
        }
        if mode == "failed" {
            assert_eq!(failure.kind, CompletionRejection::FailedChecks);
            for round in 1..=3 {
                let verification = host.verify_for_completion(thread).await.unwrap_or_else(|error| panic!("{backend:?}/{mode}, repair_pass={repair_pass}, round={round}: {error}; task={:?}; transitions={:?}", host.project().unwrap().tasks[&config.root_task], host.snapshot().unwrap().events.iter().rev().filter(|event| event.event.kind == vcp_protocol::event::EventKind::TaskTransition).take(3).map(|event| &event.event.data).collect::<Vec<_>>()));
                if repair_pass && round == 2 {
                    assert!(matches!(
                        host.try_complete_verified(thread, verification.id).unwrap(),
                        CompletionAttempt::Completed(_)
                    ));
                    assert_eq!(
                        host.project().unwrap().tasks[&config.root_task].state,
                        TaskState::Completed
                    );
                    assert_eq!(
                        fs::read_to_string(workspace.join("value.txt"))
                            .unwrap()
                            .trim(),
                        "42"
                    );
                    break;
                }
                assert!(
                    matches!(host.try_complete_verified(thread, verification.id).unwrap(), CompletionAttempt::Rejected(ref failure) if failure.kind == CompletionRejection::FailedChecks)
                );
                let failed_evidence = verification.checks[0].output.to_string();
                let feedback = host.completion_repair_feedback(thread).unwrap();
                if round == 3 {
                    assert!(feedback.is_none());
                    assert_eq!(
                        host.project().unwrap().tasks[&config.root_task].state,
                        TaskState::Paused
                    );
                    break;
                }
                let feedback = feedback.unwrap();
                assert!(
                    feedback.contains("package.json#test") && feedback.contains("evidence"),
                    "{feedback}"
                );
                assert!(
                    host.completion_repair_feedback(thread).is_err(),
                    "same verification cannot dispatch repair twice"
                );
                host.begin_coding_turn(thread, feedback.clone()).unwrap();
                let next_request = count.load(Ordering::SeqCst);
                test.codex
                    .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                        text: feedback,
                        text_elements: vec![],
                    }]))
                    .await
                    .unwrap();
                tokio::time::timeout(Duration::from_secs(30), async {
                    loop {
                        match test.codex.next_event().await.unwrap().msg {
                            EventMsg::Error(error) => loop_errors.push(format!("{error:?}")),
                            EventMsg::TurnComplete(_) => break,
                            _ => {}
                        }
                    }
                })
                .await
                .unwrap();
                assert!(loop_errors.is_empty(), "{backend:?}/{mode}, repair_pass={repair_pass}, round={round}, provider_calls={}: {loop_errors:?}", count.load(Ordering::SeqCst));
                let wire = requests.lock().unwrap()[next_request].to_string();
                assert!(wire.contains("Repair the observed failures") && wire.contains(&failed_evidence), "owner repair feedback and scoped check evidence must reach the exact encoded next request");
            }
            assert_eq!(
                count.load(Ordering::SeqCst),
                expected + 2,
                "only two bounded repair continuations are dispatched"
            );
            let effects_before_resume = host.project().unwrap().effects.len();
            if !repair_pass {
                let paused = host.project().unwrap().tasks[&config.root_task].clone();
                assert!(paused.reason.contains("execution.no_progress"));
                assert!(host
                    .begin_coding_turn(thread, "No implicit resume".into())
                    .is_err());
                host.resume(thread, paused.revision, paused.fingerprint)
                    .unwrap();
                let resumed = host.project().unwrap().tasks[&config.root_task].clone();
                assert_eq!(resumed.state, TaskState::Running);
                assert_eq!(
                    count.load(Ordering::SeqCst),
                    expected + 2,
                    "explicit resume alone does not submit repair"
                );
                assert_eq!(host.project().unwrap().effects.len(), effects_before_resume);
                assert!(!host.has_output_continuation(thread).unwrap());
                host.command(
                    Command::Transition {
                        next: TaskState::Cancelled,
                        reason: "owner cancels after deliberate resume".into(),
                        verification: None,
                    },
                    Some(config.root_task.clone()),
                    resumed.revision,
                )
                .unwrap();
            }
            owner.close().await.unwrap();
            test.codex.shutdown_and_wait().await.unwrap();
            if !repair_pass {
                drop(test);
                drop(host);
                let (reopened, owner) = CanonicalHost::open(config.clone()).unwrap();
                let view = reopened.project().unwrap();
                assert_eq!(view.tasks[&config.root_task].state, TaskState::Cancelled);
                assert_eq!(view.effects.len(), effects_before_resume);
                assert_eq!(count.load(Ordering::SeqCst), expected + 2);
                assert!(reopened.has_output_continuation(thread).is_err());
                let retained = reopened.snapshot().unwrap();
                assert!(retained
                    .records
                    .values()
                    .filter(|row| row.collection == Collection::Verification)
                    .any(|row| row
                        .decode::<Verification>()
                        .unwrap()
                        .checks
                        .iter()
                        .any(|check| matches!(check.outcome, CheckOutcome::Failed { .. }))));
                assert!(retained
                    .records
                    .values()
                    .filter(|row| row.collection == Collection::Artifact)
                    .any(
                        |row| row.decode::<ArtifactDescriptor>().unwrap().spec.schema
                            == "execution-completion-repair/1"
                    ));
                owner.close().await.unwrap();
            }
            return;
        } else if mode == "denied" {
            assert!(
                host.verify_for_completion(thread).await.is_err(),
                "owner scheduling cannot bypass named tool authority"
            );
            assert!(!oracle.exists());
        } else {
            assert_eq!(
                failure.kind,
                if scope_refresh || changed_instructions {
                    CompletionRejection::InstructionScopeRefresh
                } else if mode == "missing" {
                    CompletionRejection::MissingVerification
                } else {
                    CompletionRejection::StaleVerification
                }
            );
            if changed_instructions {
                assert_eq!(host.project().unwrap().effects, effects_before_completion);
                assert!(
                    !oracle.exists(),
                    "typed refresh must not replay any accounted call"
                );
            }
            let focused = host
                .verify_focused(thread, vec!["value.txt".into()], vec![])
                .await
                .unwrap();
            assert!(focused
                .checks
                .iter()
                .all(|check| check.outcome == CheckOutcome::Passed));
            assert!(
                matches!(
                    host.try_complete_verified(thread, focused.id).unwrap(),
                    CompletionAttempt::Rejected(_)
                ),
                "diagnostic verification cannot authorize completion, including full fallback"
            );
            let verified = host.verify_for_completion(thread).await.unwrap();
            assert!(
                verified.outstanding_issues.is_empty(),
                "{:?}",
                verified.outstanding_issues
            );
            assert!(verified
                .checks
                .iter()
                .all(|check| check.outcome == CheckOutcome::Passed));
            if changed_instructions {
                let captured_current_guidance = host
                    .snapshot()
                    .unwrap()
                    .records
                    .values()
                    .filter(|row| row.collection == Collection::Artifact)
                    .map(|row| row.decode::<ArtifactDescriptor>().unwrap())
                    .filter(|artifact| artifact.spec.schema == "verification-source/1")
                    .any(|artifact| {
                        host.read_artifact(artifact.spec.id).unwrap()
                            == REFRESHED_GUIDANCE.as_bytes()
                    });
                assert!(
                    captured_current_guidance,
                    "fresh verification must capture current instructions"
                );
                assert!(oracle.exists());
            }
            assert!(matches!(
                host.try_complete_verified(thread, verified.id).unwrap(),
                CompletionAttempt::Completed(_)
            ));
            assert_eq!(
                host.project().unwrap().tasks[&config.root_task].state,
                TaskState::Completed
            );
        }
        assert_eq!(
            count.load(Ordering::SeqCst),
            expected,
            "verification repair cannot dispatch inference"
        );
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        return;
    }
    if matches!(mode, "pass" | "siblings") {
        complete.unwrap_or_else(|e| panic!("{backend:?}/{mode}: {e}"));
        let state = host.snapshot().unwrap();
        let completed_turn: vcp_domain::task::Turn = state
            .record(Collection::Turn, turn.as_str(), &config.workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(completed_turn.state, vcp_domain::task::TurnState::Completed);
        let completed_task: Task = state
            .record(
                Collection::Task,
                config.root_task.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(
            completed_turn.cause, completed_task.cause,
            "completion is one canonical transaction"
        );
        host.command(
            Command::CreateSession {
                id: SessionId::new(),
                fork_through: Some(turn.clone()),
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
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
        } else if matches!(mode, "missing" | "denied" | "not_run") {
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
