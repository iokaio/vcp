// SPDX-License-Identifier: Apache-2.0
//! Exercise the production shared event owner against an isolated provider.
use super::*;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use vcp_domain::{
    accounting::{Money, RequestRole},
    artifact::ArtifactDescriptor,
    policy::{Autonomy, EffectClass, Policy},
    task::Objective,
    verification::{CheckOutcome, Fingerprint, Verification},
    workspace::{Binding, Trust, Workspace},
    *,
};
use vcp_lifecycle::foundation::{
    coding::{continuity_defaults, CodingConfig},
    verification::VerificationConfig,
    CanonicalHost, Config, ThreadBinding,
};
use vcp_models::catalog::{Compatibility, Snapshot};
use vcp_protocol::command::Command;
use vcp_store::BackendKind;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

mod diagnostic_cases;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_driver_repairs_failed_checks_and_exports_scoped_evidence() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        repaired(backend, false, false).await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_driver_refreshes_stale_verification_and_exports_evidence() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        repaired(backend, true, false).await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_driver_reports_completed_unknown_cost_without_unknown_tool_effects() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        repaired(backend, false, true).await;
    }
}
async fn repaired(backend: BackendKind, stale_verification: bool, missing_cost: bool) {
    let temporary = tempfile::tempdir().unwrap();
    let workspace = temporary.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    fs::write(workspace.join("value.txt"), "41\n").unwrap();
    fs::write(
        workspace.join("package.json"),
        r#"{"scripts":{"test":"node --test acceptance.cjs"}}"#,
    )
    .unwrap();
    fs::write(workspace.join("acceptance.cjs"), "const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs');test('changed_value',()=>assert.equal(fs.readFileSync('value.txt','utf8').trim(),'42'));\n").unwrap();
    let raw = serde_json::to_vec(&json!({"data":{"id":"fixture/model","endpoints":[{"tag":"fixture/region","status":0,"context_length":500000,"max_prompt_tokens":400000,"max_completion_tokens":8000,"supported_parameters":["tools","max_tokens"],"pricing":{"prompt":"0","completion":"0","request":"0.0001"}}]}})).unwrap();
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
            qualified_reasoning_efforts: BTreeSet::new(),
        },
    )
    .unwrap();
    let mut config = Config {
        canonical_root: temporary.path().join("canonical"),
        backend,
        workspace: WorkspaceId::new(),
        session: SessionId::new(),
        binding: Binding {
            host: HostId::new(),
            root: workspace.to_string_lossy().into_owned(),
            repository: "shared-driver-fixture".into(),
            worktree: "root".into(),
            revision: Revision::ZERO,
        },
        actor: ActorId::new(),
        root_task: TaskId::new(),
        cap: Money {
            currency: snapshot.price.currency.clone(),
            micros: Micros::new(1_000_000),
        }
        .into(),
        protected: Micros::ZERO,
        price: snapshot.price.clone(),
        input_ceiling: Units::new(400_000),
        output_ceiling: Units::new(4096),
        artifact_limit: ByteCount::new(vcp_store::artifact::DEFAULT_ARTIFACT_LIMIT),
        max_transport_retries: 0,
        host_tool_denials: vec![],
    };
    if missing_cost { config.cap.micros = vcp_domain::Limit::Unbounded; }
    let scope = Scope {
        workspace: config.workspace.clone(),
        session: config.session.clone(),
        task: config.root_task.clone(),
    };
    let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
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
    host.command(
        Command::CreateTask {
            root: config.root_task.clone(),
            parent: None,
            fork_origin: None,
            objective: Objective {
                text: "Change value.txt to 42 and pass changed_value.".into(),
                constraints: vec![],
                acceptance: vec!["changed_value passes".into()],
                source: EventId::new(),
                steering: SteeringRevision::ZERO,
            },
            fingerprint: Fingerprint {
                repository: "a".repeat(64),
                buffers: "b".repeat(64),
                environment: "c".repeat(64),
            },
            editing: true,
            required_checks: vec!["package.json#test".into()],
        },
        Some(scope.task.clone()),
        Revision::ZERO,
    )
    .unwrap();
    host.command(
        Command::Transition {
            next: TaskState::Running,
            reason: "explicit scripted qualification".into(),
            verification: None,
        },
        Some(scope.task.clone()),
        Revision::ZERO,
    )
    .unwrap();
    host.initialize_root_budget().unwrap();
    host.configure_canonical_tools(Default::default()).unwrap();
    host.configure_provider(snapshot, raw).unwrap();
    let node = temporary.path().join("node.exe");
    fs::copy(
        std::env::var_os("VCP_TEST_NODE").expect("explicit native Node fixture"),
        &node,
    )
    .unwrap();
    host.configure_process_profile(
        vcp_tools::process::Profile::new(
            "node".into(),
            node,
            vcp_tools::process::Mode::Direct,
            BTreeMap::from([("SystemRoot".into(), std::env::var("SystemRoot").unwrap())]),
            BTreeSet::new(),
            true,
        )
        .unwrap(),
    )
    .unwrap();
    let server = MockServer::start().await;
    let count = Arc::new(AtomicUsize::new(0));
    let requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let calls = count.clone();
    let captured = requests.clone();
    Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request:&wiremock::Request| {
        let index=calls.fetch_add(1,Ordering::SeqCst); captured.lock().unwrap().push(serde_json::from_slice(&request.body).unwrap());
        let repair_index=if stale_verification {4} else {2};
        let item=if index==0 || index==repair_index {json!({"type":"function_call","id":format!("item-{index}"),"call_id":format!("patch-{index}"),"name":"vcp_patch","arguments":json!({"patch":format!("*** Begin Patch\n*** Update File: value.txt\n@@\n-{}\n+{}\n*** End Patch",if index==0{41}else{43},if index==0{43}else{42})}).to_string(),"status":"completed"})}
        else if stale_verification && index==1 {json!({"type":"function_call","id":"verify-item","call_id":"initial-verify","name":"vcp_verify","arguments":json!({"citations":[]}).to_string(),"status":"completed"})}
        else if stale_verification && index==2 {json!({"type":"function_call","id":"read-item","call_id":"stale-read","name":"vcp_read","arguments":json!({"path":"value.txt","max_bytes":1024,"start_line":null,"end_line":null}).to_string(),"status":"completed"})}
        else {json!({"type":"message","id":format!("final-{index}"),"role":"assistant","status":"completed","content":[{"type":"output_text","text":"The change is ready for owner verification.","annotations":[]}]})};
        let cost = if missing_cost { Value::Null } else { json!(0.0001) };
        let events=[json!({"type":"response.output_item.done","output_index":0,"item":item}),json!({"type":"response.completed","response":{"id":format!("driver-response-{index}"),"status":"completed","output":[item],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":cost}}})];
        ResponseTemplate::new(200).insert_header("content-type","text/event-stream").set_body_string(events.into_iter().map(|event|format!("data: {event}\n\n")).collect::<String>())
    }).mount(&server).await;
    let credential =
        vcp_engine::capture::ProviderCredential::from_config("synthetic-cli-qualification".into());
    let mut retained = crate::session::configuration(
        &temporary.path().join("retained"),
        &workspace,
        &credential,
        "fixture/model",
    )
    .await
    .unwrap();
    retained.model_provider.base_url = Some(format!("{}/v1", server.uri()));
    let session = Session::start(
        &host,
        retained,
        ThreadBinding {
            scope: scope.clone(),
            agent: AgentId::new(),
            role: RequestRole::Main,
        },
    )
    .await
    .unwrap();
    host.configure_verification(
        session.id,
        VerificationConfig {
            requirements: vec![vcp_tools::verification::Requirement {
                manifest: "package.json".into(),
                runner: vcp_tools::verification::Runner::Node,
                profile: "node".into(),
                timeout_ms: None,
                expected_tests: vec!["changed_value".into()],
                rationale: "owner acceptance".into(),
            }],
            rationale: "shared driver repair".into(),
        },
    )
    .unwrap();
    host.configure_coding(
        session.id,
        CodingConfig {
            canonical_tools: Default::default(),
            operating: "Perform the accepted edit; only owner checks establish completion.".into(),
            affected_paths: vec!["value.txt".into()],
            max_requests: 8,
            deadline: Timestamp::new(crate::settings::now().get() + 600_000).into(),
        },
    )
    .unwrap();
    host.configure_continuity(session.id, continuity_defaults())
        .unwrap();
    let mut execution = RetainedExecution::claim(&host, &session, &scope).unwrap();
    assert!(RetainedExecution::claim(&host, &session.clone(), &scope).is_err());
    let LifecycleResult::Submitted(mut active) =
        execution.start_submission(None).result().await.unwrap()
    else {
        panic!("initial submission missing")
    };
    let mut stale_observation = None;
    let repairs = tokio::time::timeout(Duration::from_secs(180), async {
        let mut repairs = 0;
        loop {
            let event = execution.next_event().await.unwrap();
            if !event_for_turn(&event, &active) {
                continue;
            }
            if matches!(
                event.msg,
                codex_protocol::protocol::EventMsg::TurnComplete(_)
            ) {
                if stale_verification && repairs == 0 {
                    let snapshot=host.snapshot().unwrap();
                    let prior:Vec<Verification>=snapshot.records.values().filter(|row|row.collection==Collection::Verification).map(|row|row.decode().unwrap()).collect();
                    assert_eq!(prior.len(),1);
                    assert!(prior[0].checks.iter().any(|check|matches!(check.outcome,CheckOutcome::Failed{..})));
                    let CompletionAttempt::Rejected(failure) = host.try_complete_coding_turn(session.id).unwrap() else {
                        panic!("later effect requires current verification");
                    };
                    assert_eq!(failure.kind, CompletionRejection::StaleVerification, "{}", failure.message);
                    let observation = json!({"schema_version":1,"kind":"fixture_observed_verification_refresh",
                        "provenance":"asserted by the local CLI qualification fixture; no execution authority",
                        "scope":scope,"prior_verification":prior[0].id,"rejection":failure.kind,
                        "trigger":"completed_read_after_report","later_call":"stale-read",
                        "verified_source_sha256":vcp_protocol::digest_bytes(b"43\n"),
                        "current_source_sha256":vcp_protocol::digest_bytes(b"43\n")});
                    let captured=host.capture(session.id,vcp_domain::artifact::Channel::Evidence,vcp_protocol::canonical_bytes(&observation).unwrap()).unwrap();
                    stale_observation=Some(captured.spec.id);
                }
                match execution.start_completion().result().await.unwrap() {
                    LifecycleResult::Submitted(turn) => {
                        active = turn;
                        repairs += 1;
                        assert!(repairs <= 1);
                    }
                    LifecycleResult::Completed(Completion::Completed) => break,
                    LifecycleResult::Completed(Completion::Rejected(reason)) => panic!("{reason}"),
                    _ => panic!("unexpected deferred repair"),
                }
            }
        }
        repairs
    })
    .await
    .unwrap();
    assert_eq!(repairs, 1);
    assert_eq!(
        count.load(Ordering::SeqCst),
        if stale_verification { 6 } else { 4 }
    );
    assert_eq!(
        fs::read_to_string(workspace.join("value.txt"))
            .unwrap()
            .trim(),
        "42"
    );
    let state = host.snapshot().unwrap();
    let task: Task = state
        .record(Collection::Task, scope.task.as_str(), &scope.workspace)
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(task.state, TaskState::Completed);
    let outcome = Outcome::read(&host, &scope).unwrap();
    assert_eq!(outcome.task, task);
    assert_eq!(outcome.conditions.code(), 0);
    let cause = state.events.iter().find(|event| event.event.id == task.cause).unwrap();
    let receipt = state.commands.values().find(|receipt| {
        receipt.command == cause.event.correlation && receipt.workspace == scope.workspace
    }).unwrap();
    assert_eq!(&outcome.receipt, receipt, "bounded lookup preserves the task-cause receipt");
    if missing_cost {
        assert!(!outcome.conditions.unresolved_effect);
        let ledger: vcp_domain::accounting::Ledger = state.record(Collection::Ledger, scope.task.as_str(), &scope.workspace).unwrap().decode().unwrap();
        assert_eq!(ledger.settled, Micros::ZERO);
        assert!(ledger.unresolved.known().unwrap() > Micros::ZERO);
        assert_eq!(state.records.values().filter(|row| row.collection == Collection::Artifact).filter_map(|row| row.decode::<ArtifactDescriptor>().ok()).filter(|descriptor| descriptor.spec.schema == "provider-completed-execution/1").count(), count.load(Ordering::SeqCst));
    }
    let reports: Vec<Verification> = state
        .records
        .values()
        .filter(|record| record.collection == Collection::Verification)
        .map(|record| record.decode().unwrap())
        .collect();
    assert_eq!(reports.len(), if stale_verification { 3 } else { 2 });
    assert!(reports.iter().any(|report| report
        .checks
        .iter()
        .any(|check| matches!(check.outcome, CheckOutcome::Failed { .. }))));
    assert!(reports.iter().any(|report| report
        .checks
        .iter()
        .all(|check| check.outcome == CheckOutcome::Passed)));
    let repair_wire = requests.lock().unwrap()[if stale_verification { 4 } else { 2 }].to_string();
    assert!(repair_wire.contains("Repair the observed failures"));
    let saved: Vec<_> = state
        .records
        .values()
        .filter(|record| record.collection == Collection::Artifact)
        .map(|record| record.decode::<ArtifactDescriptor>().unwrap())
        .filter(|artifact| {
            matches!(
                artifact.spec.schema.as_str(),
                "verification-result/1" | "execution-completion-repair/1"
            )
        })
        .collect();
    assert!(!saved.is_empty());
    let diagnostics = host.execution_diagnostics(scope.clone()).unwrap();
    assert!(diagnostics.observations.iter().any(|span| span.phase
        == vcp_lifecycle::foundation::execution_diagnostics::Phase::Verification
        && span.status == vcp_lifecycle::foundation::execution_diagnostics::Status::Failed));
    assert!(diagnostics
        .observations
        .iter()
        .any(|span| span.phase == vcp_lifecycle::foundation::execution_diagnostics::Phase::Repair));
    let workspace_record: Workspace = state
        .record(
            Collection::Workspace,
            scope.workspace.as_str(),
            &scope.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    let mut bundle = crate::inspection_bundle::collect(
        &state,
        &vcp_audit::history::Access {
            workspace: scope.workspace.clone(),
            authority: workspace_record.authority,
            read: true,
            tasks: Some(BTreeSet::from([scope.task.clone()])),
        },
        &scope.task,
    )
    .unwrap();
    bundle["lifecycle_diagnostics"] = serde_json::to_value(&diagnostics).unwrap();
    if missing_cost {
        assert!(bundle.to_string().contains("provider-completed-execution/1"), "inspection retains the reporting proof metadata");
    }
    if let Some(artifact) = stale_observation {
        bundle["fixture_observation_artifacts"] = json!([artifact]);
    }
    if let Some(directory) = std::env::var_os("VCP_EXECUTION_EVIDENCE_ROOT") {
        export_fixture_evidence(
            &host,
            &state,
            &scope,
            backend,
            &bundle,
            directory.into(),
            if stale_verification {
                "shared-driver-stale-verification-refresh"
            } else {
                "shared_driver_repairs_failed_checks_and_exports_scoped_evidence"
            },
        );
    }
    drop(execution);
    eprintln!(
        "EE02 shared repair {backend:?}: before close {:?}",
        host.store_diagnostics()
    );
    owner.close().await.unwrap();
    session.thread.shutdown_and_wait().await.unwrap();
    drop(session);
    drop(host);
    let (reopened, reopened_owner) = CanonicalHost::open(config).unwrap();
    if missing_cost { assert_eq!(Outcome::read(&reopened, &scope).unwrap().conditions.code(), 0); }
    assert!(reopened
        .execution_diagnostics(scope.clone())
        .unwrap()
        .observations
        .is_empty());
    let reopened_state = reopened.snapshot().unwrap();
    let reopened_bundle = crate::inspection_bundle::collect(
        &reopened_state,
        &vcp_audit::history::Access {
            workspace: scope.workspace.clone(),
            authority: workspace_record.authority,
            read: true,
            tasks: Some(BTreeSet::from([scope.task.clone()])),
        },
        &scope.task,
    ).unwrap();
    let retained = reopened_bundle["retained_lifecycle_diagnostics"].as_array().unwrap();
    assert_eq!(retained.len(), 1, "explicit shutdown retains the execution owner's timing window");
    assert_eq!(retained[0]["snapshot"]["owner"], diagnostics.owner);
    assert_eq!(retained[0]["snapshot"]["complete_history"], false);
    assert!(retained[0]["snapshot"]["observations"].as_array().unwrap().iter()
        .any(|span| span["phase"] == "verification" && span["status"] == "failed"));
    for artifact in saved {
        let checkpoint: Value =
            serde_json::from_slice(&reopened.read_artifact(artifact.spec.id).unwrap()).unwrap();
        assert_eq!(
            checkpoint["execution_diagnostics"]["complete_history"],
            false
        );
        assert_eq!(checkpoint["execution_diagnostics"]["schema_version"], 1);
        assert!(!checkpoint["execution_diagnostics"]["observations"]
            .as_array()
            .unwrap()
            .is_empty());
    }
    assert_eq!(
        count.load(Ordering::SeqCst),
        if stale_verification { 6 } else { 4 },
        "read-only reopen cannot dispatch inference"
    );
    reopened_owner.close().await.unwrap();
}

// Optional qualification evidence only: all bytes originate in this synthetic
// fixture and pass the owner's existing artifact access/hash checks. Never use
// this helper to export arbitrary user stores or provider credentials.
fn export_fixture_evidence(
    host: &CanonicalHost,
    state: &vcp_store::contract::State,
    scope: &vcp_domain::workspace::Scope,
    backend: BackendKind,
    bundle: &Value,
    root: std::path::PathBuf,
    fixture: &str,
) {
    use std::io::Write;
    fs::create_dir_all(&root).unwrap();
    let directory = root.join(format!("{backend:?}-{fixture}-{}", EventId::new()));
    fs::create_dir(&directory).unwrap();
    fs::create_dir(directory.join("artifacts")).unwrap();
    let mut total = 0usize;
    let mut write = |name: &str, bytes: &[u8]| {
        total = total.checked_add(bytes.len()).unwrap();
        assert!(total <= 64 * 1024 * 1024, "fixture evidence exceeds 64 MiB");
        assert!(
            !bytes
                .windows(b"synthetic-cli-qualification".len())
                .any(|window| window == b"synthetic-cli-qualification"),
            "credential cannot be exported"
        );
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(name))
            .unwrap();
        output.write_all(bytes).unwrap();
        json!({"path":name,"bytes":bytes.len(),"sha256":vcp_protocol::digest_bytes(bytes)})
    };
    let bundle_entry = write(
        "inspection-bundle.json",
        &serde_json::to_vec_pretty(bundle).unwrap(),
    );
    let mut artifacts = vec![];
    for record in state
        .records
        .values()
        .filter(|record| record.collection == Collection::Artifact)
    {
        let descriptor: ArtifactDescriptor = record.decode().unwrap();
        if descriptor.spec.scope != *scope
            || !matches!(
                descriptor.state,
                vcp_domain::artifact::CaptureState::Complete
                    | vcp_domain::artifact::CaptureState::Aborted
            )
        {
            continue;
        }
        assert!(descriptor.length.get() <= 64 * 1024 * 1024);
        let bytes = host.read_artifact(descriptor.spec.id.clone()).unwrap();
        assert_eq!(vcp_protocol::digest_bytes(&bytes), descriptor.sha256);
        let mut entry = write(&format!("artifacts/{}.bin", descriptor.spec.id), &bytes);
        entry["descriptor"] = serde_json::to_value(descriptor).unwrap();
        artifacts.push(entry);
    }
    let manifest = serde_json::to_vec_pretty(&json!({"schema_version":1,"fixture":fixture,"scope":scope,"backend":format!("{backend:?}"),"bundle":bundle_entry,"artifacts":artifacts,"limitations":"Synthetic provider only. Complete captures and sealed Aborted prefixes from the same task; descriptors retain partial state and omission declarations. Time ordering is not recorded causation."})).unwrap();
    let manifest_entry = write("manifest.json", &manifest);
    write(
        "manifest.sha256",
        format!("{}\n", manifest_entry["sha256"].as_str().unwrap()).as_bytes(),
    );
}
