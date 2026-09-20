// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};
use vcp_domain::{
    effect::{Effect, EffectState},
    policy::*,
};
use vcp_lifecycle::foundation::mcp::{Registration, Request as McpRequest};
use vcp_tools::process::{Mode, Profile, Request};

pub(super) struct Fixture {
    pub(super) _temp: tempfile::TempDir,
    pub(super) workspace: std::path::PathBuf,
    pub(super) host: CanonicalHost,
    pub(super) owner: vcp_lifecycle::foundation::CanonicalOwner,
    pub(super) test: TestCodex,
    _server: wiremock::MockServer,
    pub(super) thread: codex_protocol::ThreadId,
    pub(super) config: Config,
    pub(super) policy: Policy,
}
impl Fixture {
    pub(super) async fn new(backend: BackendKind, scenario: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace ü");
        fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let tools = temp.path().join("tools");
        fs::create_dir(&tools).unwrap();
        let executable = tools.join("fixture.exe");
        fs::copy(env!("CARGO_BIN_EXE_vcp-mcp-fixture"), &executable).unwrap();
        fs::write(workspace.join("input.txt"), b"authorized input").unwrap();
        let config = config(&temp.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let binding = task(&host, &config, config.root_task.clone(), None);
        let server = start_mock_server().await;
        let mut registry = ExtensionRegistryBuilder::new();
        registry.turn_start_admission(Arc::new(host.clone()));
        registry.work_admission(Arc::new(host.clone()));
        let starter = host.clone();
        let cwd = workspace.clone();
        let test = test_codex()
            .with_extensions(Arc::new(registry.build()))
            .with_auth(codex_login::CodexAuth::from_api_key(
                "synthetic-duplex-fixture",
            ))
            .with_allowed_tools(AllowedTools(vec![]))
            .with_config(move |c| {
                c.cwd = cwd.try_into().unwrap();
                configure_fixture_provider(c);
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
        host.command(
            Command::SetWorkspaceTrust {
                trust: Trust::Trusted,
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        let policy = Policy {
            workspace: config.workspace.clone(),
            revision: PolicyRevision::ZERO,
            mode: Autonomy::Autonomous,
            denials: vec![],
            workspace_roots: BTreeSet::from([
                RootId::parse(config.workspace.as_str()).unwrap(),
                RootId::parse("exec-fixture").unwrap(),
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
            output_ceiling_bytes: ByteCount::new(8 * 1024 * 1024),
        };
        host.command(
            Command::SetPolicy {
                policy: policy.clone(),
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        host.configure_process_profile(
            Profile::new(
                "fixture".into(),
                executable,
                Mode::Direct,
                BTreeMap::from([
                    ("SystemRoot".into(), std::env::var("SystemRoot").unwrap()),
                    ("CI".into(), "canonical-duplex-fixture".into()),
                ]),
                BTreeSet::new(),
                true,
            )
            .unwrap()
            .with_inputs(vec!["input.txt".into()])
            .unwrap(),
        )
        .unwrap();
        host.configure_mcp(Registration {
            name: "fixture".into(),
            process: Request {
                profile: "fixture".into(),
                arguments: vec![workspace.to_string_lossy().into_owned(), scenario.into()],
                directory: String::new(),
                timeout_ms: 30_000,
                output_bytes: 1024 * 1024,
                input: None,
            },
            allowed_resources: if scenario.starts_with("content-")
                && scenario != "content-prompts-only"
            {
                BTreeSet::from([
                    "fixture://public/document".into(),
                    "file:///C:/vcp-untrusted.txt".into(),
                    "https://127.0.0.1:1/untrusted".into(),
                ])
            } else {
                Default::default()
            },
            allowed_prompts: if scenario.starts_with("content-")
                && scenario != "content-resources-only"
            {
                BTreeSet::from(["review".into(), "explain".into()])
            } else {
                Default::default()
            },
            allowed_tools: if matches!(scenario, "content-resources-only" | "content-prompts-only")
            {
                BTreeSet::new()
            } else {
                BTreeSet::from(["echo".into(), "read_marker".into(), "write_marker".into()])
            },
            limits: vcp_extensions::mcp::registration::Limits {
                frame_bytes: 64 * 1024,
                total_discovery_bytes: 1024 * 1024,
                tools: 16,
                pages: 8,
                timeout_ms: 10_000,
                stderr_bytes: 64 * 1024,
            },
        })
        .unwrap();
        Self {
            _temp: temp,
            workspace,
            host,
            owner,
            test,
            _server: server,
            thread,
            config,
            policy,
        }
    }
    async fn list(&self) -> serde_json::Value {
        self.host
            .mcp_control(
                self.thread,
                McpRequest::List {
                    server: "fixture".into(),
                },
            )
            .await
            .unwrap()
    }
    fn call(&self, list: &serde_json::Value, tool: &str, args: serde_json::Value) -> McpRequest {
        let metadata = list["catalog"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["tool"] == tool)
            .unwrap();
        McpRequest::Call {
            server: "fixture".into(),
            tool: tool.into(),
            identity_digest: metadata["identity_digest"].as_str().unwrap().into(),
            arguments_json: args.to_string(),
        }
    }
    pub(super) async fn close(self) {
        self.host.disconnect_mcp().await.unwrap();
        let Self {
            _temp,
            host,
            owner,
            test,
            ..
        } = self;
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        drop(test);
        drop(host);
        drop(_temp);
    }
    pub(super) fn resume_approved(&self) {
        let state = self.host.snapshot().unwrap();
        let task: Task = state
            .record(
                vcp_store::contract::Collection::Task,
                self.config.root_task.as_str(),
                &self.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        self.host
            .resume(self.thread, task.revision, task.fingerprint)
            .unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_mcp_discovery_exact_call_and_reconnect_identity() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend, "normal").await;
        let list = f.list().await;
        assert_eq!(list["connected"], true);
        assert_eq!(list["catalog"]["tools"].as_array().unwrap().len(), 3);
        let request = f.call(&list, "write_marker", serde_json::json!({"value":"one"}));
        let ticket = f
            .host
            .prepare_mcp_call(f.thread, request.clone())
            .await
            .unwrap();
        let effect = ticket.effect().clone();
        let outcome = f.host.dispatch_mcp_call(ticket).await.unwrap();
        assert_eq!(outcome["outcome"], "succeeded", "{outcome}");
        assert_eq!(
            fs::read_to_string(f.workspace.join("value.txt")).unwrap(),
            "one"
        );
        assert_eq!(
            fs::read_to_string(f.workspace.join("writes.jsonl"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        let state = f.host.snapshot().unwrap();
        let effect: Effect = state
            .record(
                vcp_store::contract::Collection::Effect,
                effect.as_str(),
                &f.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(effect.state, EffectState::Succeeded);
        assert!(!effect.observed_changes.is_empty());
        let invalid = f.call(&list, "echo", serde_json::json!({"unexpected":"reject"}));
        assert!(f.host.prepare_mcp_call(f.thread, invalid).await.is_err());
        f.host
            .mcp_control(
                f.thread,
                McpRequest::Disconnect {
                    server: "fixture".into(),
                },
            )
            .await
            .unwrap();
        let fresh = f.list().await;
        assert_ne!(
            list["catalog"]["tools"][0]["identity_digest"],
            fresh["catalog"]["tools"][0]["identity_digest"]
        );
        assert!(f.host.prepare_mcp_call(f.thread, request).await.is_err());
        f.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_mcp_denial_and_revocation_send_no_tool_payload() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let mut f = Fixture::new(backend, "normal").await;
        let list = f.list().await;
        let request = f.call(&list, "write_marker", serde_json::json!({"value":"never"}));
        let ticket = f.host.prepare_mcp_call(f.thread, request).await.unwrap();
        f.policy.revision = PolicyRevision::new(1);
        f.policy.denials.push(Denial {
            id: "deny-mcp".into(),
            origin: RuleOrigin::User,
            reason: "revoke call authority".into(),
            effects: BTreeSet::from([EffectClass::Opaque]),
            tool: Some("vcp_mcp".into()),
            roots: BTreeSet::new(),
            paths: vec![],
        });
        let change = f
            .host
            .change_authority(
                Command::SetPolicy {
                    policy: f.policy.clone(),
                },
                None,
                Revision::ZERO,
            )
            .unwrap();
        assert!(f.host.dispatch_mcp_call(ticket).await.is_err());
        assert!(!f.workspace.join("writes.jsonl").exists());
        assert!(!fs::read_to_string(f.workspace.join("requests.jsonl"))
            .unwrap()
            .contains("tools/call"));
        f.host.disconnect_mcp().await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), change.wait())
            .await
            .unwrap()
            .unwrap();
        f.close().await;
    }
    let mut f = Fixture::new(BackendKind::Sqlite, "normal").await;
    f.policy.revision = PolicyRevision::new(1);
    f.policy.denials.push(Denial {
        id: "deny-startup".into(),
        origin: RuleOrigin::User,
        reason: "no daemon".into(),
        effects: BTreeSet::from([EffectClass::Execute]),
        tool: None,
        roots: BTreeSet::new(),
        paths: vec![],
    });
    f.host
        .command(
            Command::SetPolicy {
                policy: f.policy.clone(),
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
    let denied = f.list().await;
    assert_eq!(denied["connected"], false);
    assert!(!f.workspace.join("started").exists());
    f.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_mcp_lost_response_retains_unknown_without_replay() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend, "write-then-exit").await;
        let list = f.list().await;
        let request = f.call(&list, "write_marker", serde_json::json!({"value":"once"}));
        let outcome = f.host.mcp_control(f.thread, request).await.unwrap();
        assert_eq!(outcome["outcome"], "unknown", "{outcome}");
        assert_eq!(
            fs::read_to_string(f.workspace.join("writes.jsonl"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        let id = ToolRunId::parse(outcome["effect"].as_str().unwrap()).unwrap();
        let state = f.host.snapshot().unwrap();
        let effect: Effect = state
            .record(
                vcp_store::contract::Collection::Effect,
                id.as_str(),
                &f.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(effect.state, EffectState::OutcomeUnknown);
        assert!(
            f.host
                .mcp_control(
                    f.thread,
                    McpRequest::List {
                        server: "fixture".into()
                    }
                )
                .await
                .is_err()
                || !f.host.mcp_connections_present()
        );
        assert_eq!(
            fs::read_to_string(f.workspace.join("writes.jsonl"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        f.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_mcp_explicit_approvals_reuse_exact_startup_and_call() {
    let mut f = Fixture::new(BackendKind::Sqlite, "normal").await;
    f.policy.revision = PolicyRevision::new(1);
    f.policy.mode = Autonomy::Ask;
    f.host
        .command(
            Command::SetPolicy {
                policy: f.policy.clone(),
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
    let startup = f.list().await;
    assert_eq!(startup["decision"]["kind"], "question");
    assert!(!f.workspace.join("started").exists());
    f.host
        .command(
            Command::Decide {
                id: ApprovalId::parse(startup["question"].as_str().unwrap()).unwrap(),
                operation_digest: startup["decision"]["digest"].as_str().unwrap().into(),
                effect_revision: Revision::new(1),
                allow: true,
            },
            Some(f.config.root_task.clone()),
            Revision::ZERO,
        )
        .unwrap();
    f.resume_approved();
    let list = f.list().await;
    let request = f.call(
        &list,
        "write_marker",
        serde_json::json!({"value":"approved"}),
    );
    // An unrelated canonical operation is never covered by the idle daemon's
    // ephemeral resume observation, even if it shares the same task.
    let unrelated = ToolRunId::new();
    let unrelated_execution = ExecutionId::new();
    let state = f.host.snapshot().unwrap();
    let task: Task = state
        .record(
            vcp_store::contract::Collection::Task,
            f.config.root_task.as_str(),
            &f.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    f.host
        .command(
            Command::ProposeEffect {
                id: unrelated.clone(),
                operation_digest: "a".repeat(64),
            },
            Some(f.config.root_task.clone()),
            task.revision,
        )
        .unwrap();
    for (index, next) in [
        EffectState::Validated,
        EffectState::Authorized,
        EffectState::DispatchRecorded,
        EffectState::Running,
    ]
    .into_iter()
    .enumerate()
    {
        f.host
            .command(
                Command::AdvanceEffect {
                    id: unrelated.clone(),
                    next,
                    reason: "synthetic independent effect observation".into(),
                    execution: if index >= 2 {
                        Some(unrelated_execution.clone())
                    } else {
                        None
                    },
                    exit_code: None,
                    observed_changes: vec![],
                },
                Some(f.config.root_task.clone()),
                Revision::new(index as u64),
            )
            .unwrap();
    }
    let question = f.host.mcp_control(f.thread, request.clone()).await.unwrap();
    assert_eq!(question["decision"]["kind"], "question");
    assert!(!f.workspace.join("writes.jsonl").exists());
    f.host
        .command(
            Command::Decide {
                id: ApprovalId::parse(question["question"].as_str().unwrap()).unwrap(),
                operation_digest: question["decision"]["digest"].as_str().unwrap().into(),
                effect_revision: Revision::new(1),
                allow: true,
            },
            Some(f.config.root_task.clone()),
            Revision::ZERO,
        )
        .unwrap();
    let state = f.host.snapshot().unwrap();
    let waiting: Task = state
        .record(
            vcp_store::contract::Collection::Task,
            f.config.root_task.as_str(),
            &f.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    assert!(f
        .host
        .resume(f.thread, waiting.revision, waiting.fingerprint.clone())
        .is_err());
    f.host
        .command(
            Command::AdvanceEffect {
                id: unrelated.clone(),
                next: EffectState::OutcomeUnknown,
                reason: "unrelated outcome missing".into(),
                execution: Some(unrelated_execution.clone()),
                exit_code: None,
                observed_changes: vec![],
            },
            Some(f.config.root_task.clone()),
            Revision::new(4),
        )
        .unwrap();
    assert!(f
        .host
        .resume(f.thread, waiting.revision, waiting.fingerprint)
        .is_err());
    f.host
        .command(
            Command::AdvanceEffect {
                id: unrelated,
                next: EffectState::Failed,
                reason: "synthetic independent fixture reconciliation".into(),
                execution: Some(unrelated_execution),
                exit_code: None,
                observed_changes: vec![],
            },
            Some(f.config.root_task.clone()),
            Revision::new(5),
        )
        .unwrap();
    f.resume_approved();
    let outcome = f.host.mcp_control(f.thread, request).await.unwrap();
    assert_eq!(outcome["outcome"], "succeeded", "{outcome}");
    assert_eq!(outcome["effect"], question["effect"]);
    assert_eq!(
        fs::read_to_string(f.workspace.join("writes.jsonl"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    f.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_mcp_cancel_after_marker_reopens_unknown_without_replay() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend, "write-then-block").await;
        let list = f.list().await;
        let request = f.call(&list, "write_marker", serde_json::json!({"value":"once"}));
        let ticket = f.host.prepare_mcp_call(f.thread, request).await.unwrap();
        let id = ticket.effect().clone();
        let host = f.host.clone();
        let waiter = tokio::spawn(async move { host.dispatch_mcp_call(ticket).await });
        tokio::time::timeout(Duration::from_secs(5), async {
            while !f.workspace.join("after-effect").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let state = f.host.snapshot().unwrap();
        let task: Task = state
            .record(
                vcp_store::contract::Collection::Task,
                f.config.root_task.as_str(),
                &f.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert!(
            f.host
                .resume(f.thread, task.revision, task.fingerprint)
                .is_err(),
            "busy calls cannot supply idle resume proof"
        );
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
        assert_eq!(
            fs::read_to_string(f.workspace.join("writes.jsonl"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        let Fixture {
            _temp,
            workspace,
            host,
            owner,
            test,
            config,
            ..
        } = f;
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        drop(test);
        drop(host);
        let store = vcp_store::Store::open(
            &config.canonical_root,
            backend,
            std::slice::from_ref(&workspace),
        )
        .await
        .unwrap();
        let effect: Effect = store
            .state()
            .record(
                vcp_store::contract::Collection::Effect,
                id.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(effect.state, EffectState::OutcomeUnknown);
        assert_eq!(
            fs::read_to_string(workspace.join("writes.jsonl"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        store.close().await.unwrap();
        drop(_temp);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_mcp_callbacks_and_invalid_peers_are_bounded() {
    for scenario in [
        "callback",
        "malformed",
        "oversized",
        "version-mismatch",
        "duplicate-tools",
    ] {
        let f = Fixture::new(BackendKind::Sqlite, scenario).await;
        let result = f
            .host
            .mcp_control(
                f.thread,
                McpRequest::List {
                    server: "fixture".into(),
                },
            )
            .await;
        if scenario == "callback" {
            let list = result.unwrap();
            let request = f.call(
                &list,
                "echo",
                serde_json::json!({"text":"no model callback"}),
            );
            let result = f.host.mcp_control(f.thread, request).await.unwrap();
            assert_eq!(result["outcome"], "succeeded", "{result}");
            assert!(f.workspace.join("callback-handled").exists());
        } else {
            assert!(result.is_err(), "{scenario}: {result:?}");
        }
        assert!(!f.workspace.join("writes.jsonl").exists());
        f.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_mcp_retention_between_source_fence_and_pipe_write_sends_nothing() {
    use vcp_domain::retention_selector::{Criterion, Selector, Tree};
    use vcp_lifecycle::foundation::history_retention::Request as Retention;
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend, "normal").await;
        let list = f.list().await;
        let request = f.call(
            &list,
            "write_marker",
            serde_json::json!({"value":"pruned source must not leave"}),
        );
        let ticket = f.host.prepare_mcp_call(f.thread, request).await.unwrap();
        let id = ticket.effect().clone();
        let (arrived, release) = f
            .host
            .qualification_block_next_mcp_write(f.thread, "fixture")
            .await
            .unwrap();
        let host = f.host.clone();
        let waiter = tokio::spawn(async move { host.dispatch_mcp_call(ticket).await });
        tokio::time::timeout(Duration::from_secs(5), arrived.notified())
            .await
            .unwrap();
        let preview = f
            .host
            .history_retention(Retention::Preview {
                selector: Selector {
                    schema_version: 1,
                    tree: Tree::Match(Criterion::Task(f.config.root_task.clone())),
                },
                action: vcp_memory::retention::Action::Compact,
            })
            .unwrap();
        let before = f.host.snapshot().unwrap();
        let before: Workspace = before
            .record(
                vcp_store::contract::Collection::Workspace,
                f.config.workspace.as_str(),
                &f.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let applied = f.host.history_retention(Retention::Apply {
            preview: preview["id"].as_str().unwrap().into(),
        });
        release.notify_one();
        assert!(applied.is_ok(), "{applied:?}");
        let outcome = tokio::time::timeout(Duration::from_secs(10), waiter)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(outcome["outcome"], "unknown", "{outcome}");
        let state = f.host.snapshot().unwrap();
        let after: Workspace = state
            .record(
                vcp_store::contract::Collection::Workspace,
                f.config.workspace.as_str(),
                &f.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert!(after.deletion > before.deletion);
        let effect: Effect = state
            .record(
                vcp_store::contract::Collection::Effect,
                id.as_str(),
                &f.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(effect.state, EffectState::OutcomeUnknown);
        assert!(!f.workspace.join("writes.jsonl").exists());
        assert!(!fs::read_to_string(f.workspace.join("requests.jsonl"))
            .unwrap()
            .contains("tools/call"));
        f.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_mcp_selected_model_source_deleted_after_prepare_is_never_sent() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend, "normal").await;
        let list = f.list().await;
        fs::write(f.workspace.join("selected.txt"), "selected private source").unwrap();
        let root = vcp_repository::Root::open(
            vcp_repository::RootIdentity {
                workspace: f.config.workspace.clone(),
                root: RootId::parse(f.config.workspace.as_str()).unwrap(),
                repository: f.config.binding.repository.clone(),
                worktree: f.config.binding.worktree.clone(),
                binding: f.config.binding.revision,
            },
            &f.workspace,
        )
        .unwrap();
        let source = root
            .read(std::path::Path::new("selected.txt"), 1024)
            .unwrap();
        let (snapshot, _) = provider_snapshot();
        let sealed = sealed_provider_context(&f.host, f.thread, &snapshot, Some(&source));
        let request = f.call(
            &list,
            "write_marker",
            serde_json::json!({"value":"selected private source"}),
        );
        let ticket = f
            .host
            .qualification_prepare_mcp_context_call(f.thread, request, sealed, vec![root])
            .await
            .unwrap();
        fs::remove_file(f.workspace.join("selected.txt")).unwrap();
        assert!(f.host.dispatch_mcp_call(ticket).await.is_err());
        assert!(!f.workspace.join("writes.jsonl").exists());
        assert!(!fs::read_to_string(f.workspace.join("requests.jsonl"))
            .unwrap()
            .contains("tools/call"));
        f.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_mcp_second_server_conflict_cancels_unsent_startup_without_waiting() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend, "normal").await;
        let second = f.workspace.join("second-server");
        fs::create_dir(&second).unwrap();
        f.host
            .configure_mcp(Registration {
                name: "second".into(),
                process: Request {
                    profile: "fixture".into(),
                    arguments: vec![second.to_string_lossy().into_owned(), "normal".into()],
                    directory: String::new(),
                    timeout_ms: 30_000,
                    output_bytes: 1024 * 1024,
                    input: None,
                },
                allowed_resources: Default::default(),
                allowed_prompts: Default::default(),
                allowed_tools: BTreeSet::from(["echo".into()]),
                limits: vcp_extensions::mcp::registration::Limits {
                    frame_bytes: 64 * 1024,
                    total_discovery_bytes: 1024 * 1024,
                    tools: 16,
                    pages: 8,
                    timeout_ms: 10_000,
                    stderr_bytes: 64 * 1024,
                },
            })
            .unwrap();
        let list = f.list().await;
        let before = f.host.snapshot().unwrap();
        let conflict = tokio::time::timeout(
            Duration::from_secs(3),
            f.host.mcp_control(
                f.thread,
                McpRequest::List {
                    server: "second".into(),
                },
            ),
        )
        .await
        .expect("second startup must not wait behind the retained lifetime lease");
        assert!(conflict.is_err());
        assert!(!second.join("started").exists());
        let after = f.host.snapshot().unwrap();
        let added = after
            .records
            .iter()
            .filter(|(key, row)| {
                row.collection == vcp_store::contract::Collection::Effect
                    && !before.records.contains_key(*key)
            })
            .map(|(_, row)| row.decode::<Effect>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].state, EffectState::Cancelled);
        assert!(added[0].execution.is_none());
        let result = f
            .host
            .mcp_control(
                f.thread,
                f.call(
                    &list,
                    "echo",
                    serde_json::json!({"text":"first remains usable"}),
                ),
            )
            .await
            .unwrap();
        assert_eq!(result["outcome"], "succeeded");
        f.host
            .mcp_control(
                f.thread,
                McpRequest::Disconnect {
                    server: "fixture".into(),
                },
            )
            .await
            .unwrap();
        let connected = f
            .host
            .mcp_control(
                f.thread,
                McpRequest::List {
                    server: "second".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(connected["connected"], true);
        f.host.disconnect_mcp().await.unwrap();
        assert!(f
            .host
            .snapshot()
            .unwrap()
            .records
            .values()
            .filter(|row| row.collection == vcp_store::contract::Collection::Effect)
            .all(|row| matches!(
                row.decode::<Effect>().unwrap().state,
                EffectState::Succeeded | EffectState::Failed | EffectState::Cancelled
            )));
        f.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_mcp_disconnect_cancels_pending_startup_and_call_approvals() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let mut f = Fixture::new(backend, "normal").await;
        f.policy.revision = PolicyRevision::new(1);
        f.policy.mode = Autonomy::Ask;
        f.host
            .command(
                Command::SetPolicy {
                    policy: f.policy.clone(),
                },
                None,
                Revision::ZERO,
            )
            .unwrap();
        let startup = f.list().await;
        f.host
            .mcp_control(
                f.thread,
                McpRequest::Disconnect {
                    server: "fixture".into(),
                },
            )
            .await
            .unwrap();
        assert_cancelled_question(&f, &startup);
        assert!(!f.workspace.join("started").exists());
        f.resume_approved();
        let startup = f.list().await;
        f.host
            .command(
                Command::Decide {
                    id: ApprovalId::parse(startup["question"].as_str().unwrap()).unwrap(),
                    operation_digest: startup["decision"]["digest"].as_str().unwrap().into(),
                    effect_revision: Revision::new(1),
                    allow: true,
                },
                Some(f.config.root_task.clone()),
                Revision::ZERO,
            )
            .unwrap();
        f.resume_approved();
        let list = f.list().await;
        let pending = f
            .host
            .mcp_control(
                f.thread,
                f.call(
                    &list,
                    "write_marker",
                    serde_json::json!({"value":"not approved"}),
                ),
            )
            .await
            .unwrap();
        assert_eq!(pending["decision"]["kind"], "question");
        f.host
            .mcp_control(
                f.thread,
                McpRequest::Disconnect {
                    server: "fixture".into(),
                },
            )
            .await
            .unwrap();
        assert_cancelled_question(&f, &pending);
        assert!(!f.workspace.join("writes.jsonl").exists());
        f.close().await;
    }
}
fn assert_cancelled_question(f: &Fixture, value: &serde_json::Value) {
    let state = f.host.snapshot().unwrap();
    let effect: Effect = state
        .record(
            vcp_store::contract::Collection::Effect,
            value["effect"].as_str().unwrap(),
            &f.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(effect.state, EffectState::Cancelled);
    assert!(effect.execution.is_none());
    let approval: vcp_protocol::command::Approval = state
        .record(
            vcp_store::contract::Collection::Approval,
            value["question"].as_str().unwrap(),
            &f.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(
        approval.state,
        vcp_protocol::command::ApprovalState::Pending
    );
    assert_ne!(approval.effect_revision, effect.revision);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_mcp_owner_shutdown_cancels_unstarted_approval() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let mut f = Fixture::new(backend, "normal").await;
        f.policy.revision = PolicyRevision::new(1);
        f.policy.mode = Autonomy::Ask;
        f.host
            .command(
                Command::SetPolicy {
                    policy: f.policy.clone(),
                },
                None,
                Revision::ZERO,
            )
            .unwrap();
        let pending = f.list().await;
        assert!(!f.workspace.join("started").exists());
        let Fixture {
            host,
            owner,
            test,
            config,
            _temp,
            ..
        } = f;
        owner.close().await.unwrap();
        let state = host.snapshot().unwrap();
        let effect: Effect = state
            .record(
                vcp_store::contract::Collection::Effect,
                pending["effect"].as_str().unwrap(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(effect.state, EffectState::Cancelled);
        assert!(effect.execution.is_none());
        let approval: vcp_protocol::command::Approval = state
            .record(
                vcp_store::contract::Collection::Approval,
                pending["question"].as_str().unwrap(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(
            approval.state,
            vcp_protocol::command::ApprovalState::Pending
        );
        assert_ne!(approval.effect_revision, effect.revision);
        test.codex.shutdown_and_wait().await.unwrap();
        drop(test);
        drop(host);
        drop(_temp);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_mcp_stale_question_cleanup_preserves_historical_decision() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let mut f = Fixture::new(backend, "normal").await;
        f.policy.revision = PolicyRevision::new(1);
        f.policy.mode = Autonomy::Ask;
        f.host
            .command(
                Command::SetPolicy {
                    policy: f.policy.clone(),
                },
                None,
                Revision::ZERO,
            )
            .unwrap();
        let pending = f.list().await;
        f.policy.revision = PolicyRevision::new(2);
        f.host
            .command(
                Command::SetPolicy {
                    policy: f.policy.clone(),
                },
                None,
                Revision::new(1),
            )
            .unwrap();
        f.host
            .mcp_control(
                f.thread,
                McpRequest::Disconnect {
                    server: "fixture".into(),
                },
            )
            .await
            .unwrap();
        assert_cancelled_question(&f, &pending);
        assert!(!f.workspace.join("started").exists());
        // The original question is retained, but its exact effect revision can
        // no longer authorize anything after cancellation.
        assert!(f
            .host
            .command(
                Command::Decide {
                    id: ApprovalId::parse(pending["question"].as_str().unwrap()).unwrap(),
                    operation_digest: pending["decision"]["digest"].as_str().unwrap().into(),
                    effect_revision: Revision::new(1),
                    allow: true,
                },
                Some(f.config.root_task.clone()),
                Revision::ZERO
            )
            .is_err());
        assert_cancelled_question(&f, &pending);
        f.close().await;
    }
}
