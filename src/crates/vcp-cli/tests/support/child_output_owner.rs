// SPDX-License-Identifier: Apache-2.0
//! Connected P7-06 output loss: real registered child loops and an OS pipe.
use super::*;
use std::{process::Stdio, time::Duration};
use vcp_domain::{
    accounting::{Ledger, Money, RequestRole},
    artifact::{ArtifactDescriptor, Channel},
    policy::{Autonomy, Policy},
    task::{Objective, TaskState},
    verification::Fingerprint,
    workspace::{Binding, Scope, Trust},
    *,
};
use vcp_lifecycle::foundation::{CanonicalHost, Config, ThreadBinding};
use vcp_protocol::{command::Command as CanonicalCommand, subscription::EventPage};
use vcp_store::{contract::Collection, BackendKind};

const PUBLIC_MESSAGES: usize = 32;

fn noisy_response() -> String {
    let mut items: Vec<_> = (0..PUBLIC_MESSAGES).map(|index| json!({
        "type":"message","id":format!("progress-{index}"),"role":"assistant",
        "channel":"commentary","status":"completed",
        "content":[{"type":"output_text","text":format!("noisy-child-progress-{index:02}"),"annotations":[]}]
    })).collect();
    items.push(json!({"type":"function_call","id":"read-item","call_id":"read-call","name":"vcp_read",
        "arguments":json!({"path":"value.txt","max_bytes":1024,"start_line":null,"end_line":null}).to_string(),"status":"completed"}));
    let mut events: Vec<_> = items.iter().enumerate().map(|(index,item)|
        json!({"type":"response.output_item.done","output_index":index,"item":item})).collect();
    events.push(json!({"type":"response.completed","response":{"id":"noisy-progress","status":"completed","output":items,
        "usage":{"input_tokens":10,"output_tokens":256,"total_tokens":266,"cost":0.0001}}}));
    events
        .into_iter()
        .map(|event| format!("data: {event}\n\n"))
        .collect()
}

fn transcripts(host: &CanonicalHost, task: &TaskId) -> Vec<String> {
    host.snapshot()
        .unwrap()
        .records
        .values()
        .filter(|row| row.collection == Collection::Artifact)
        .map(|row| row.decode::<ArtifactDescriptor>().unwrap())
        .filter(|artifact| {
            artifact.spec.scope.task == *task && artifact.spec.channel == Channel::ChildTranscript
        })
        .map(|artifact| String::from_utf8(host.read_artifact(artifact.spec.id).unwrap()).unwrap())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn noisy_and_quiet_registered_children_survive_output_loss_and_cursor_recovery() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        output_loss_case(backend).await;
    }
}

async fn output_loss_case(backend: BackendKind) {
    let server = MockServer::start().await;
    let noisy_requests = Arc::new(AtomicUsize::new(0));
    let count = noisy_requests.clone();
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(move |request: &wiremock::Request| {
            let noisy = String::from_utf8_lossy(&request.body).contains("noisy-child-objective");
            let first_noisy = noisy && count.fetch_add(1, Ordering::SeqCst) == 0;
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(if first_noisy {
                    noisy_response()
                } else {
                    response(3, "complete")
                })
                .set_delay(if first_noisy {
                    Duration::ZERO
                } else {
                    Duration::from_secs(30)
                })
        })
        .mount(&server)
        .await;
    let fixture = Fixture::new(&server.uri(), "complete");
    let workspace = fixture.workspace.canonicalize().unwrap();
    let profile = vcp_cli::settings::load(&fixture.profile, &workspace).unwrap();
    let prepared = profile.prepare(Autonomy::Autonomous).unwrap();
    let config = Config {
        canonical_root: fixture.data.join("child-output-canonical"),
        backend,
        workspace: WorkspaceId::new(),
        session: SessionId::new(),
        binding: Binding {
            host: HostId::new(),
            root: workspace.to_string_lossy().into_owned(),
            repository: "child-output-fixture".into(),
            worktree: "parent".into(),
            revision: Revision::ZERO,
        },
        actor: ActorId::new(),
        root_task: TaskId::new(),
        cap: Money {
            currency: "USD".to_owned().try_into().unwrap(),
            micros: Micros::new(1_000_000),
        },
        protected: Micros::ZERO,
        price: prepared.profile.provider.price.clone(),
        input_ceiling: prepared.profile.provider.max_input,
        output_ceiling: prepared.profile.output_ceiling().unwrap(),
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
        CanonicalCommand::SetWorkspaceTrust {
            trust: Trust::Trusted,
        },
        None,
        Revision::ZERO,
    )
    .unwrap();
    host.command(
        CanonicalCommand::SetPolicy {
            policy: Policy {
                workspace: config.workspace.clone(),
                revision: PolicyRevision::ZERO,
                mode: Autonomy::Autonomous,
                denials: vec![],
                workspace_roots: BTreeSet::from(
                    [RootId::parse(config.workspace.as_str()).unwrap()],
                ),
                automatic_effects: prepared.profile.automatic_effects.clone(),
                timeout_ceiling_ms: Units::new(120_000),
                output_ceiling_bytes: ByteCount::new(1024 * 1024),
            },
        },
        None,
        Revision::ZERO,
    )
    .unwrap();
    host.command(
        CanonicalCommand::CreateTask {
            root: config.root_task.clone(),
            parent: None,
            fork_origin: None,
            objective: Objective {
                text: "Observe two bounded reviewers".into(),
                constraints: vec![],
                acceptance: vec!["Retain attributed child evidence".into()],
                source: EventId::new(),
                steering: SteeringRevision::ZERO,
            },
            fingerprint: Fingerprint {
                repository: "a".repeat(64),
                buffers: "b".repeat(64),
                environment: "c".repeat(64),
            },
            editing: false,
            required_checks: vec![],
        },
        Some(config.root_task.clone()),
        Revision::ZERO,
    )
    .unwrap();
    host.command(
        CanonicalCommand::Transition {
            next: TaskState::Running,
            reason: "connected output fixture".into(),
            verification: None,
        },
        Some(config.root_task.clone()),
        Revision::ZERO,
    )
    .unwrap();
    host.initialize_root_budget().unwrap();
    host.configure_provider(prepared.profile.provider.clone(), prepared.raw_catalog)
        .unwrap();
    let credential =
        vcp_engine::capture::ProviderCredential::from_config("synthetic-cli-qualification".into());
    let mut retained = vcp_cli::session::configuration(
        &fixture.data.join("retained"),
        &workspace,
        &credential,
        &prepared.profile.provider.compatibility.model,
    )
    .await
    .unwrap();
    retained.model_provider.base_url = prepared.profile.qualification_endpoint.clone();
    let parent = vcp_cli::session::Session::start(
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
    // A cloned session must not steal events from the supervisor. An invalid
    // scope cannot acquire ownership, and releasing ownership permits handoff.
    let execution = vcp_cli::execution::RetainedExecution::claim(&host, &parent, &scope).unwrap();
    assert!(vcp_cli::execution::RetainedExecution::claim(&host, &parent.clone(), &scope).is_err());
    drop(execution);
    let mut wrong_scope = scope.clone();
    wrong_scope.task = TaskId::new();
    assert!(vcp_cli::execution::RetainedExecution::claim(&host, &parent, &wrong_scope).is_err());
    assert!(vcp_cli::execution::submit(&host, &parent, &wrong_scope)
        .await
        .is_err());
    drop(vcp_cli::execution::RetainedExecution::claim(&host, &parent, &scope).unwrap());
    host.configure_verification(
        parent.id,
        vcp_lifecycle::foundation::verification::VerificationConfig {
            requirements: vec![],
            rationale: "read-only fixture".into(),
        },
    )
    .unwrap();
    host.configure_coding(
        parent.id,
        vcp_lifecycle::foundation::coding::CodingConfig {
            operating: "Inspect assigned source and report public observations.".into(),
            affected_paths: vec!["value.txt".into()],
            max_requests: 8,
            deadline: Timestamp::new(vcp_cli::settings::now().get() + 120_000),
        },
    )
    .unwrap();
    let copies = fixture._temp.path().join("copies");
    fs::create_dir(&copies).unwrap();
    let git = PathBuf::from(std::env::var_os("VCP_TEST_GIT").expect("native Git required"));
    // Non-Git workspace capture selects only the explicitly granted source.
    let mut children = Vec::new();
    for objective in ["quiet-child-objective", "noisy-child-objective"] {
        let spec = fixture._temp.path().join(format!("{objective}.json"));
        fs::write(&spec,serde_json::to_vec(&json!({"version":1,"git":git,"disposable_parent":copies,
            "objective":objective,"acceptance":["Report source observations"],"mode":"read_only","write_paths":[],
            "untracked_inputs":["value.txt"],"allocation_usd":"0.1","seconds":120,"required_checks":[]})).unwrap()).unwrap();
        children.push(
            vcp_cli::delegation::prepare(&host, &parent, &scope, &spec)
                .await
                .unwrap(),
        );
    }
    let cursor = host.subscribe_events(SessionSeq::ZERO, 2).unwrap();
    let EventPage::Events { next_cursor, .. } = host.events(cursor).unwrap() else {
        panic!("initial cursor missing")
    };
    let seen = next_cursor.after;
    // Deliberately stop consuming the small UI queue. Public transcript capture
    // must proceed before lossy notices, without hiding the quiet sibling.
    let (notices, updates) = tokio::sync::mpsc::channel(2);
    let mut pumps = Vec::new();
    for child in &children {
        pumps.push(
            vcp_cli::delegation::run(host.clone(), child, &scope, notices.clone())
                .await
                .unwrap(),
        );
    }
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if server.received_requests().await.unwrap().len() == 3
                && transcripts(&host, &children[1].task).len() == PUBLIC_MESSAGES
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("both children must be active after attributed public-message flood");
    // These are actual active retained child pumps, not just guard fixtures.
    // A cloned child session must not steal their completion/transcript events.
    for child in &children {
        let mut child_scope = scope.clone();
        child_scope.task = child.task.clone();
        assert!(vcp_cli::execution::RetainedExecution::claim(
            &host,
            &child.session.clone(),
            &child_scope
        )
        .is_err());
    }
    assert_eq!(updates.len(), 2, "UI notice queue stays bounded");
    let page = vcp_cli::agents_view::page(
        &host.snapshot().unwrap(),
        &scope,
        vcp_cli::settings::now(),
        0,
    )
    .unwrap();
    assert_eq!(page["total"], 2);
    for item in page["items"].as_array().unwrap() {
        assert_eq!(item["state"], "running");
        assert!(!item["registration"].is_null());
        assert!(!item["last_activity"].is_null());
        assert!(item["reason"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty()));
    }
    assert!(transcripts(&host, &children[0].task).is_empty());

    // The independent consumer closes its OS read handle after one real JSONL
    // frame. Its exit, not an injected status, makes the following write fail.
    let mut consumer = Command::new(std::env::var_os("VCP_TEST_NODE").unwrap())
        .args([
            "-e",
            "process.stdin.once('data',()=>process.exit(0));process.stdin.resume()",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut output =
        vcp_cli::output::OwnedJsonl::new(consumer.stdin.take().unwrap(), owner).unwrap();
    output
        .emit(
            &CommandId::new(),
            Some(&scope),
            vcp_cli::jsonl::Payload::CursorGap {
                reason: "fixture consumer ready",
            },
        )
        .await
        .unwrap();
    let exit = tokio::task::spawn_blocking(move || consumer.wait_with_output().unwrap())
        .await
        .unwrap();
    assert!(exit.status.success());
    assert!(output
        .drain_events(&host, &CommandId::new(), seen)
        .await
        .is_err());
    drop(updates);
    for child in &children {
        assert!(host
            .begin_coding_turn(child.session.id, "must remain fenced".into())
            .is_err());
    }
    for pump in pumps {
        let _ = tokio::time::timeout(Duration::from_secs(5), pump)
            .await
            .expect("owner loss must stop the child pump")
            .unwrap();
    }
    let state = host.snapshot().unwrap();
    for row in state
        .records
        .values()
        .filter(|row| row.collection == Collection::Task)
    {
        assert_eq!(row.value["state"], "paused");
    }
    let ledger: Ledger = state
        .record(
            Collection::Ledger,
            config.root_task.as_str(),
            &config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(ledger.settled, Micros::new(100));
    assert!(ledger.unresolved > Micros::ZERO);
    let attempts: Vec<_> = state
        .records
        .values()
        .filter(|row| row.collection == Collection::Attempt)
        .collect();
    assert_eq!(attempts.len(), 3);
    assert_eq!(
        attempts
            .iter()
            .filter(|row| row.value["phase"] == "reconciliation_pending")
            .count(),
        2
    );
    host.unsubscribe_events(next_cursor.snapshot.clone())
        .unwrap();
    assert!(matches!(
        host.events(next_cursor).unwrap(),
        EventPage::Gap { .. }
    ));
    let child_ids: Vec<_> = children.iter().map(|child| child.task.clone()).collect();
    for child in children {
        child.session.thread.shutdown_and_wait().await.unwrap();
    }
    parent.thread.shutdown_and_wait().await.unwrap();
    drop(parent);
    drop(output);
    drop(host);

    let (recovered, reopened_owner) = CanonicalHost::open(config.clone()).unwrap();
    let mut cursor = recovered.subscribe_events(seen, 2).unwrap();
    let mut event_ids = BTreeSet::new();
    let mut observed_children = BTreeSet::new();
    loop {
        let EventPage::Events {
            events,
            next_cursor,
            at_end,
            ..
        } = recovered.events(cursor).unwrap()
        else {
            panic!("fresh cursor must recover retained history")
        };
        for row in events {
            assert!(event_ids.insert(row.event.id));
            if let Some(task) = row.event.task {
                if child_ids.contains(&task) {
                    observed_children.insert(task);
                }
            }
        }
        cursor = next_cursor;
        if at_end {
            break;
        }
    }
    recovered.unsubscribe_events(cursor.snapshot).unwrap();
    assert_eq!(observed_children, child_ids.iter().cloned().collect());
    let retained = transcripts(&recovered, &child_ids[1]);
    assert_eq!(retained.len(), PUBLIC_MESSAGES);
    for index in 0..PUBLIC_MESSAGES {
        assert!(retained.contains(&format!("noisy-child-progress-{index:02}")));
    }
    let page = vcp_cli::agents_view::page(
        &recovered.snapshot().unwrap(),
        &scope,
        vcp_cli::settings::now(),
        0,
    )
    .unwrap();
    for item in page["items"].as_array().unwrap() {
        assert_eq!(item["state"], "paused");
        assert!(!item["registration"].is_null());
    }
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        3,
        "consumer/cursor recovery never resumes dispatch"
    );
    assert_eq!(
        fs::read_to_string(workspace.join("value.txt")).unwrap(),
        "41\n"
    );
    reopened_owner.close().await.unwrap();
}
