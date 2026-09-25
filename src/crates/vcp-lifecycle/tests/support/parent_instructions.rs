// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{
    collections::BTreeSet,
    fs,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};
use vcp_domain::policy::*;
use vcp_lifecycle::foundation::{
    coding::{allowed_tools, CodingConfig},
    verification::VerificationConfig,
};
use vcp_repository::{Root, RootIdentity};
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parent_instruction_grants_preserve_scope_and_fence_edits_and_completion() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for mode in [
            "granted",
            "ungranted",
            "edited",
            "created",
            "denied",
            "escape",
        ] {
            run(backend, mode).await;
        }
    }
}

async fn run(backend: BackendKind, mode: &'static str) {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("parent");
    let workspace = parent.join("workspace");
    fs::create_dir_all(&workspace).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    if mode != "created" {
        fs::write(parent.join("AGENTS.md"), "Explicit parent guidance").unwrap();
    }
    fs::write(
        parent.join("private.txt"),
        "Outside ordinary workspace access",
    )
    .unwrap();
    fs::write(workspace.join("AGENTS.md"), "Workspace guidance").unwrap();
    fs::write(workspace.join("file.txt"), "Observed workspace source").unwrap();
    let config = config(&temp.path().join("canonical"), &workspace, backend);
    let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
    let binding = task(&host, &config, config.root_task.clone(), None);
    let parent_id = RootId::new();
    let parent_root = Root::open(
        RootIdentity {
            workspace: config.workspace.clone(),
            root: parent_id.clone(),
            repository: config.binding.repository.clone(),
            worktree: config.binding.worktree.clone(),
            binding: config.binding.revision,
        },
        &parent.canonicalize().unwrap(),
    )
    .unwrap();
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
                denials: if mode == "denied" {
                    vec![Denial {
                        id: "parent-read-denial".into(),
                        origin: RuleOrigin::User,
                        reason: "Parent guidance denied".into(),
                        effects: BTreeSet::from([EffectClass::Read]),
                        tool: Some("vcp_patch".into()),
                        roots: BTreeSet::from([parent_id.clone()]),
                        paths: vec![],
                    }]
                } else {
                    vec![]
                },
                workspace_roots: BTreeSet::from(
                    [RootId::parse(config.workspace.as_str()).unwrap()],
                ),
                automatic_effects: BTreeSet::from([EffectClass::Read]),
                timeout_ceiling_ms: Units::new(30_000),
                output_ceiling_bytes: ByteCount::new(1024 * 1024),
            },
        },
        None,
        Revision::ZERO,
    )
    .unwrap();
    let (snapshot, raw) = provider_snapshot();
    host.configure_provider(snapshot, raw).unwrap();
    let server = start_mock_server().await;
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    let bodies = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let received = bodies.clone();
    let changed_parent = parent.clone();
    Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request: &wiremock::Request| {
        let n = calls.fetch_add(1, Ordering::SeqCst);
        received.lock().unwrap().push(serde_json::from_slice(&request.body).unwrap());
        let mut events = vec![];
        let mut output = vec![];
        if n == 0 {
            if matches!(mode, "edited" | "created") {
                fs::write(changed_parent.join("AGENTS.md"), "New parent guidance after request").unwrap();
            }
            let item = serde_json::json!({"type":"function_call","id":"item-read","call_id":"call-read","name":"vcp_read",
                "arguments":serde_json::json!({"path":if mode=="escape" {"../private.txt"} else {"file.txt"},"max_bytes":1024,"start_line":null,"end_line":null}).to_string(),"status":"completed"});
            events.push(serde_json::json!({"type":"response.output_item.done","output_index":0,"item":item}));
            output.push(item);
        } else { events.push(ev_assistant_message("done", "Observed current context.")); }
        events.push(serde_json::json!({"type":"response.completed","response":{"id":format!("parent-{n}"),"status":"completed","output":output,
            "usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}}));
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
            "synthetic-parent-instructions",
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
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    host.configure_coding(
        thread,
        CodingConfig {
            canonical_tools: Default::default(),
            operating: "Use only current scoped guidance".into(),
            affected_paths: vec!["file.txt".into()],
            max_requests: 3,
            deadline: Timestamp::new(now + 300_000),
        },
    )
    .unwrap();
    // A same-workspace identity alone cannot grant an arbitrary directory.
    let unrelated = Root::open(
        RootIdentity {
            root: RootId::new(),
            ..parent_root.identity.clone()
        },
        &workspace,
    )
    .unwrap();
    assert!(host
        .configure_instruction_roots(thread, vec![unrelated])
        .is_err());
    let mut foreign = parent_root.clone();
    foreign.identity.workspace = WorkspaceId::new();
    assert!(host
        .configure_instruction_roots(thread, vec![foreign])
        .is_err());
    assert!(host
        .configure_instruction_roots(thread, vec![parent_root.clone(); 2])
        .is_err());
    if mode == "denied" {
        assert!(host
            .configure_instruction_roots(thread, vec![parent_root])
            .is_err());
        assert_eq!(count.load(Ordering::SeqCst), 0);
    } else {
        if mode != "ungranted" {
            host.configure_instruction_roots(thread, vec![parent_root])
                .unwrap();
            assert!(host.configure_instruction_roots(thread, vec![]).is_err());
        }
        host.configure_verification(
            thread,
            VerificationConfig {
                requirements: vec![],
                rationale: "Observe instruction applicability".into(),
            },
        )
        .unwrap();
        assert!(
            host.configure_instruction_roots(thread, vec![]).is_err(),
            "baseline freezes read grants"
        );
        test.codex
            .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                text: "Inspect the workspace source".into(),
                text_elements: vec![],
            }]))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(180), async {
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
        let requests = bodies.lock().unwrap().clone();
        let first = requests.first().unwrap().to_string();
        assert_eq!(
            first.contains("Explicit parent guidance"),
            !matches!(mode, "ungranted" | "created")
        );
        assert!(!first.contains("Outside ordinary workspace access"));
        assert!(first.contains("Workspace guidance"));
        let state = host.snapshot().unwrap();
        let effects: Vec<vcp_domain::effect::Effect> = state
            .records
            .values()
            .filter(|r| r.collection == Collection::Effect)
            .map(|r| r.decode().unwrap())
            .collect();
        if matches!(mode, "edited" | "created" | "escape") {
            assert!(
                effects.is_empty(),
                "{backend:?}/{mode}: no stale or outside-root read effect"
            );
            assert!(requests
                .iter()
                .all(|r| !r.to_string().contains("Outside ordinary workspace access")));
        } else {
            assert_eq!(count.load(Ordering::SeqCst), 2, "{backend:?}/{mode}");
            assert_eq!(effects.len(), 1);
            let citation = host
                .capture(
                    thread,
                    Channel::Evidence,
                    b"Observed workspace source".to_vec(),
                )
                .unwrap()
                .spec
                .id;
            let verification = host.verify(thread, vec![citation]).await.unwrap();
            assert!(
                verification.outstanding_issues.is_empty(),
                "{verification:?}"
            );
            if mode == "granted" {
                fs::write(parent.join("AGENTS.md"), "Changed after verification").unwrap();
                assert!(host.complete_verified(thread, verification.id).is_err());
            } else {
                fs::write(parent.join("AGENTS.md"), "Still outside the granted scope").unwrap();
                host.complete_verified(thread, verification.id).unwrap();
            }
        }
    }
    assert_eq!(
        fs::read(parent.join("private.txt")).unwrap(),
        b"Outside ordinary workspace access"
    );
    owner.close().await.unwrap();
    test.codex.shutdown_and_wait().await.unwrap();
}
