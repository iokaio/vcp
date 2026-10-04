// SPDX-License-Identifier: Apache-2.0
//! Retained production-path diagnostic fixtures, using an isolated provider.
use super::*;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn interrupted_provider_remains_unresolved_without_completed_financial_proof() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        diagnostic_case(backend, Case::InterruptedProvider).await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn archives_context_omission_and_interrupted_provider_evidence() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        diagnostic_case(backend, Case::ContextOmission).await;
        diagnostic_case(backend, Case::InterruptedProvider).await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn archives_sibling_tool_calls_with_exact_diagnostic_identities() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        diagnostic_case(backend, Case::SiblingCalls).await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn archives_interrupted_native_tool_and_preserves_cancelled_reopen() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        diagnostic_case(backend, Case::InterruptedTool).await;
    }
}
#[derive(Clone, Copy)]
enum Case {
    ContextOmission,
    InterruptedProvider,
    SiblingCalls,
    InterruptedTool,
}
async fn diagnostic_case(backend: BackendKind, case: Case) {
    let interrupted = matches!(case, Case::InterruptedProvider);
    let sibling = matches!(case, Case::SiblingCalls);
    let interrupted_tool = matches!(case, Case::InterruptedTool);
    let temporary = tempfile::tempdir().unwrap();
    let workspace = temporary.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    fs::write(workspace.join("value.txt"), "41\n").unwrap();
    let mut endpoint = json!({"data":{"id":"fixture/model","endpoints":[{"tag":"fixture/region","status":0,"context_length":500000,"max_prompt_tokens":400000,"max_completion_tokens":8000,"supported_parameters":["tools","max_tokens"],"pricing":{"prompt":"0","completion":"0","request":"0.0001"}}]}});
    let mut required_parameters = BTreeSet::from(["tools".into(), "max_tokens".into()]);
    if sibling {
        endpoint["data"]["endpoints"][0]["supported_parameters"]
            .as_array_mut()
            .unwrap()
            .push(json!("parallel_tool_calls"));
        required_parameters.insert("parallel_tool_calls".into());
    }
    let raw = serde_json::to_vec(&endpoint).unwrap();
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
            required_parameters,
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
    if interrupted { config.cap.micros = vcp_domain::Limit::Unbounded; }
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
                text: "Collect bounded context and interrupted provider evidence.".into(),
                constraints: vec![],
                acceptance: vec![],
                source: EventId::new(),
                steering: SteeringRevision::ZERO,
            },
            fingerprint: Fingerprint {
                repository: "a".repeat(64),
                buffers: "b".repeat(64),
                environment: "c".repeat(64),
            },
            editing: true,
            required_checks: vec![],
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

    if interrupted_tool {
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
        fs::write(
            workspace.join("interrupted.cjs"),
            br#"const fs=require('node:fs');
fs.appendFileSync('dispatches.txt','started\n');
fs.writeSync(1,'PARTIAL_STDOUT_BEFORE_CANCEL\n');
fs.writeSync(2,'PARTIAL_STDERR_BEFORE_CANCEL\n');
setTimeout(()=>fs.writeFileSync('process-ready','ready'),500);
setInterval(()=>{},1000);
"#,
        )
        .unwrap();
    }

    fs::write(
        workspace.join("large.txt"),
        format!(
            "{}\nOMITTED_MIDDLE_MARKER\n{}\n",
            "x".repeat(60_000),
            "y".repeat(60_000)
        ),
    )
    .unwrap();
    let fixture = if interrupted_tool {
        "interrupted-native-tool"
    } else if sibling {
        "sibling-tool-calls"
    } else if interrupted {
        "interrupted-provider"
    } else {
        "context-omission"
    };
    let server = MockServer::start().await;
    let count = Arc::new(AtomicUsize::new(0));
    let requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let calls = count.clone();
    let captured = requests.clone();
    Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request:&wiremock::Request| {
        let index=calls.fetch_add(1,Ordering::SeqCst);
        captured.lock().unwrap().push(serde_json::from_slice(&request.body).unwrap());
        let events=if interrupted {
            vec![json!({"type":"response.created","response":{"id":"interrupted-fixture-response","status":"in_progress"}}),
            json!({"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"partial-item","call_id":"partial-call","name":"vcp_patch","arguments":"","status":"in_progress"}}),
            json!({"type":"response.function_call_arguments.delta","item_id":"partial-item","delta":"{\"patch\":\"*** Begin Patch\n*** Update File: value.txt\n"})]
        } else {
            let item=if interrupted_tool {json!({"type":"function_call","id":"process-item","call_id":"interrupted-process","name":"vcp_exec","arguments":json!({"profile":"node","arguments":["interrupted.cjs"],"directory":"","timeout_ms":120000,"output_bytes":1048576,"input":null}).to_string(),"status":"completed"})}
            else if index==0 {json!({"type":"function_call","id":"read-item","call_id":"read-large","name":"vcp_read","arguments":json!({"path":"large.txt","max_bytes":200000,"start_line":null,"end_line":null}).to_string(),"status":"completed"})}
            else {json!({"type":"message","id":"omission-final","role":"assistant","status":"completed","content":[{"type":"output_text","text":"The middle marker is unavailable in this bounded request; consult the retained evidence.","annotations":[]}]})};
            let mut items=vec![item];
            if sibling && index==0 {
                items.push(json!({"type":"function_call","id":"sibling-item","call_id":"read-sibling","name":"vcp_read","arguments":json!({"path":"value.txt","max_bytes":1024,"start_line":null,"end_line":null}).to_string(),"status":"completed"}));
            }
            let mut events:Vec<_>=items.iter().enumerate().map(|(position,item)|json!({"type":"response.output_item.done","output_index":position,"item":item})).collect();
            events.push(json!({"type":"response.completed","response":{"id":format!("diagnostic-response-{index}"),"status":"completed","output":items,"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}}));
            events
        };
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
            requirements: vec![],
            rationale: "read-only diagnostic baseline; completion is not attempted".into(),
        },
    )
    .unwrap();
    host.configure_coding(
        session.id,
        CodingConfig {
            canonical_tools: Default::default(),
            operating: "Read requested evidence and report unavailable context honestly.".into(),
            affected_paths: vec!["large.txt".into()],
            max_requests: 4,
            deadline: Timestamp::new(crate::settings::now().get() + 600_000).into(),
        },
    )
    .unwrap();
    host.configure_continuity(session.id, continuity_defaults())
        .unwrap();
    let mut execution = RetainedExecution::claim(&host, &session, &scope).unwrap();
    let LifecycleResult::Submitted(active) =
        execution.start_submission(None).result().await.unwrap()
    else {
        panic!("submission missing")
    };
    if interrupted_tool {
        tokio::time::timeout(Duration::from_secs(30), async {
            while !workspace.join("process-ready").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let task = current(&host, &scope).unwrap();
        let command = host
            .control_envelope(
                CommandId::new(),
                scope.task.clone(),
                task.revision,
                Command::Transition {
                    next: TaskState::Cancelled,
                    reason: "explicit cancellation during native output capture".into(),
                    verification: None,
                },
            )
            .unwrap();
        let receipt = host.stop(command.clone()).unwrap();
        assert_eq!(
            host.stop(command).unwrap(),
            receipt,
            "duplicate cancellation is idempotent"
        );
    }
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let event = execution.next_event().await.unwrap();
            if event_for_turn(&event, &active)
                && matches!(
                    event.msg,
                    codex_protocol::protocol::EventMsg::TurnComplete(_)
                        | codex_protocol::protocol::EventMsg::TurnAborted(_)
                )
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    if !interrupted && !interrupted_tool {
        let task = current(&host, &scope).unwrap();
        host.stop(
            host.control_envelope(
                CommandId::new(),
                scope.task.clone(),
                task.revision,
                Command::Transition {
                    next: TaskState::Paused,
                    reason: "explicit fixture stop after context omission".into(),
                    verification: None,
                },
            )
            .unwrap(),
        )
        .unwrap();
    }
    let state = host.snapshot().unwrap();
    let view = host.project().unwrap();
    assert_eq!(
        view.tasks[&scope.task].state,
        if interrupted_tool {
            TaskState::Cancelled
        } else {
            TaskState::Paused
        }
    );
    assert_eq!(
        fs::read_to_string(workspace.join("value.txt")).unwrap(),
        "41\n"
    );
    let artifacts: Vec<_> = state
        .records
        .values()
        .filter(|row| row.collection == Collection::Artifact)
        .map(|row| row.decode::<ArtifactDescriptor>().unwrap())
        .collect();
    if interrupted_tool {
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(
            fs::read_to_string(workspace.join("dispatches.txt")).unwrap(),
            "started\n"
        );
        assert_eq!(view.effects.len(), 1);
        let effects: Vec<vcp_domain::effect::Effect> = state
            .records
            .values()
            .filter(|row| row.collection == Collection::Effect)
            .map(|row| row.decode().unwrap())
            .collect();
        use vcp_domain::effect::EffectState;
        assert!(matches!(
            effects[0].state,
            EffectState::Failed | EffectState::OutcomeUnknown
        ));
        assert!(effects[0].execution.is_some());
        if effects[0].state == EffectState::OutcomeUnknown {
            assert_eq!(effects[0].exit_code, None);
            assert!(
                !artifacts
                    .iter()
                    .any(|artifact| artifact.spec.schema == "canonical-coding-pair/1"),
                "unobserved outcome has no completed pair"
            );
        } else {
            let outcome = artifacts
                .iter()
                .find(|artifact| artifact.spec.schema == "vcp-process-outcome-v1")
                .unwrap();
            let outcome: Value =
                serde_json::from_slice(&host.read_artifact(outcome.spec.id.clone()).unwrap())
                    .unwrap();
            assert_eq!(outcome["effect"], json!(effects[0].id));
            assert_eq!(outcome["execution"], json!(effects[0].execution));
            assert_eq!(outcome["output_complete"], false);
            assert_eq!(outcome["owned_processes_remaining"], 0);
            assert!(outcome["stop_reason"].is_string());
        }
        for (channel, expected) in [
            (
                vcp_domain::artifact::Channel::Stdout,
                "PARTIAL_STDOUT_BEFORE_CANCEL\n",
            ),
            (
                vcp_domain::artifact::Channel::Stderr,
                "PARTIAL_STDERR_BEFORE_CANCEL\n",
            ),
        ] {
            let output: Vec<_> = artifacts
                .iter()
                .filter(|artifact| artifact.spec.channel == channel)
                .collect();
            assert_eq!(output.len(), 1);
            assert_eq!(output[0].state, vcp_domain::artifact::CaptureState::Aborted);
            assert_eq!(
                host.read_artifact(output[0].spec.id.clone()).unwrap(),
                expected.as_bytes()
            );
        }
    } else if interrupted {
        assert_eq!(count.load(Ordering::SeqCst), 1);
        let outcome = Outcome::read(&host, &scope).unwrap();
        assert!(outcome.conditions.unresolved_effect);
        assert_eq!(outcome.conditions.code(), 7);
        assert!(!artifacts.iter().any(|artifact| artifact.spec.schema == "provider-completed-execution/1"));
        for row in state.records.values().filter(|row| row.collection == Collection::Attempt) {
            assert!(!host.completed_financial_uncertainty(row.decode().unwrap()).unwrap());
        }
        assert!(view.effects.is_empty(), "partial patch cannot execute");
        let partial = artifacts
            .iter()
            .find(|artifact| {
                artifact.spec.channel == vcp_domain::artifact::Channel::Response
                    && artifact.state == vcp_domain::artifact::CaptureState::Aborted
            })
            .unwrap();
        let prefix =
            String::from_utf8(host.read_artifact(partial.spec.id.clone()).unwrap()).unwrap();
        assert!(prefix.contains("response.function_call_arguments.delta"));
        assert!(!prefix.contains("response.completed"));
        assert!(
            view.ledgers[&scope.task].unresolved.get() > 0,
            "missing charge is unknown, never zero"
        );
    } else {
        assert_eq!(count.load(Ordering::SeqCst), 2);
        assert_eq!(view.effects.len(), if sibling { 2 } else { 1 });
        let second = requests.lock().unwrap()[1].to_string();
        assert!(!second.contains("OMITTED_MIDDLE_MARKER"));
        assert!(
            second.contains("omitted_bytes"),
            "bounded context must declare omissions"
        );
        let projection = artifacts
            .iter()
            .find(|a| a.spec.schema == "canonical-compaction-projection/1")
            .unwrap();
        let projection: Value =
            serde_json::from_slice(&host.read_artifact(projection.spec.id.clone()).unwrap())
                .unwrap();
        assert!(projection["gain"].as_u64().unwrap() > 32_000);
        assert!(artifacts
            .iter()
            .filter(|a| a.spec.schema == "canonical-coding-pair/1")
            .any(
                |a| String::from_utf8_lossy(&host.read_artifact(a.spec.id.clone()).unwrap())
                    .contains("OMITTED_MIDDLE_MARKER")
            ));
    }
    let diagnostics = host.execution_diagnostics(scope.clone()).unwrap();
    use vcp_lifecycle::foundation::execution_diagnostics::{Phase, Status};
    if interrupted_tool {
        let calls: Vec<_> = diagnostics
            .observations
            .iter()
            .filter(|span| span.phase == Phase::ToolDispatch)
            .collect();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].call_id.as_deref(), Some("interrupted-process"));
        assert!(
            matches!(
                calls[0].status,
                Status::Interrupted | Status::Failed | Status::Succeeded
            ),
            "dispatch must be terminal; completed wrapper processing does not imply effect success"
        );
        assert!(calls[0].attempt.is_some());
    }
    if sibling {
        let calls: Vec<_> = diagnostics
            .observations
            .iter()
            .filter(|span| span.phase == Phase::ToolDispatch)
            .collect();
        assert_eq!(calls.len(), 2);
        assert_eq!(
            calls
                .iter()
                .filter_map(|span| span.call_id.as_deref())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["read-large", "read-sibling"])
        );
        assert_eq!(calls[0].attempt, calls[1].attempt);
        assert!(calls.iter().all(|span| span.scope == scope
            && span.turn.is_some()
            && span.status == Status::Succeeded));
        for artifact in artifacts
            .iter()
            .filter(|artifact| artifact.spec.schema == "canonical-coding-pair/1")
        {
            let pair: Value =
                serde_json::from_slice(&host.read_artifact(artifact.spec.id.clone()).unwrap())
                    .unwrap();
            let matching = calls
                .iter()
                .find(|span| span.call_id.as_deref() == pair["parts"][0]["content"]["id"].as_str())
                .unwrap();
            assert_eq!(
                matching.attempt.as_ref().unwrap().as_str(),
                pair["attempt"].as_str().unwrap()
            );
        }
    }
    assert!(
        interrupted_tool
            || diagnostics.observations.iter().any(|span| span.phase
                == if interrupted {
                    Phase::ProviderExchange
                } else {
                    Phase::ToolDispatch
                }
                && span.status
                    == if interrupted || interrupted_tool {
                        Status::Interrupted
                    } else {
                        Status::Succeeded
                    }
                && span.attempt.is_some())
    );
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
    bundle["lifecycle_diagnostics"] = serde_json::to_value(diagnostics).unwrap();
    if let Some(directory) = std::env::var_os("VCP_EXECUTION_EVIDENCE_ROOT") {
        export_fixture_evidence(
            &host,
            &state,
            &scope,
            backend,
            &bundle,
            directory.into(),
            fixture,
        );
    }
    drop(execution);
    owner.close().await.unwrap();
    session.thread.shutdown_and_wait().await.unwrap();
    if interrupted_tool {
        drop(session);
        drop(host);
        let (reopened, reopened_owner) = CanonicalHost::open(config).unwrap();
        let reopened_view = reopened.project().unwrap();
        assert_eq!(reopened_view.tasks[&scope.task].state, TaskState::Cancelled);
        assert_eq!(reopened_view.effects.len(), 1);
        let prior_effect = state
            .records
            .values()
            .find(|row| row.collection == Collection::Effect)
            .unwrap();
        let reopened_state = reopened.snapshot().unwrap();
        let prior_effect: vcp_domain::effect::Effect = prior_effect.decode().unwrap();
        let recovered: vcp_domain::effect::Effect = reopened_state
            .record(
                Collection::Effect,
                prior_effect.id.as_str(),
                &scope.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(recovered.id, prior_effect.id);
        assert_eq!(recovered.execution, prior_effect.execution);
        assert_eq!(recovered.state, prior_effect.state);
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "read-only reopen cannot infer again"
        );
        assert_eq!(
            fs::read_to_string(workspace.join("dispatches.txt")).unwrap(),
            "started\n",
            "read-only reopen cannot replay the effect"
        );
        for artifact in artifacts.iter().filter(|artifact| {
            matches!(
                artifact.spec.channel,
                vcp_domain::artifact::Channel::Stdout | vcp_domain::artifact::Channel::Stderr
            )
        }) {
            assert_eq!(
                reopened
                    .read_artifact(artifact.spec.id.clone())
                    .unwrap()
                    .len() as u64,
                artifact.length.get()
            );
        }
        reopened_owner.close().await.unwrap();
    }
}
