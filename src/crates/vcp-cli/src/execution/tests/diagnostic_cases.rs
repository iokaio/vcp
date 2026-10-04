// SPDX-License-Identifier: Apache-2.0
//! Retained production-path diagnostic fixtures, using an isolated provider.
use super::*;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn archives_context_omission_and_interrupted_provider_evidence() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        diagnostic_case(backend, false).await;
        diagnostic_case(backend, true).await;
    }
}
async fn diagnostic_case(backend: BackendKind, interrupted: bool) {
    let temporary = tempfile::tempdir().unwrap();
    let workspace = temporary.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    fs::write(workspace.join("value.txt"), "41\n").unwrap();
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
    let config = Config {
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

    fs::write(
        workspace.join("large.txt"),
        format!(
            "{}\nOMITTED_MIDDLE_MARKER\n{}\n",
            "x".repeat(60_000),
            "y".repeat(60_000)
        ),
    )
    .unwrap();
    let fixture = if interrupted {
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
            let item=if index==0 {json!({"type":"function_call","id":"read-item","call_id":"read-large","name":"vcp_read","arguments":json!({"path":"large.txt","max_bytes":200000,"start_line":null,"end_line":null}).to_string(),"status":"completed"})}
            else {json!({"type":"message","id":"omission-final","role":"assistant","status":"completed","content":[{"type":"output_text","text":"The middle marker is unavailable in this bounded request; consult the retained evidence.","annotations":[]}]})};
            vec![json!({"type":"response.output_item.done","output_index":0,"item":item}),json!({"type":"response.completed","response":{"id":format!("diagnostic-response-{index}"),"status":"completed","output":[item],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}})]
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
    if !interrupted {
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
    assert_eq!(view.tasks[&scope.task].state, TaskState::Paused);
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
    if interrupted {
        assert_eq!(count.load(Ordering::SeqCst), 1);
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
        assert_eq!(view.effects.len(), 1);
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
    assert!(diagnostics.observations.iter().any(|span| span.phase
        == if interrupted {
            Phase::ProviderExchange
        } else {
            Phase::ToolDispatch
        }
        && span.status
            == if interrupted {
                Status::Interrupted
            } else {
                Status::Succeeded
            }
        && span.attempt.is_some()));
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
}
