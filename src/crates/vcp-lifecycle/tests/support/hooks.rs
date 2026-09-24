// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};
use vcp_domain::policy::*;
use vcp_extensions::hooks::{input::HookLimits, planner, receipt::HookStatus, registry::*};
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
    pub(super) async fn new(backend: BackendKind) -> Self {
        Self::with_coding(backend, false).await
    }
    async fn with_coding(backend: BackendKind, coding: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace ü");
        fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        fs::create_dir(workspace.join("hooks-workdir")).unwrap();
        let tools = temp.path().join("tools");
        fs::create_dir(&tools).unwrap();
        let executable = tools.join("fixture.exe");
        fs::copy(env!("CARGO_BIN_EXE_vcp-hook-fixture"), &executable).unwrap();
        fs::write(workspace.join("input.txt"), b"authorized input").unwrap();
        let config = config(&temp.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let binding = task(&host, &config, config.root_task.clone(), None);
        if coding {
            let (snapshot, raw) = provider_snapshot();
            host.configure_provider(snapshot, raw).unwrap();
        }
        let server = start_mock_server().await;
        let mut registry = ExtensionRegistryBuilder::new();
        registry.turn_start_admission(Arc::new(host.clone()));
        registry.work_admission(Arc::new(host.clone()));
        if coding {
            registry.tool_contributor(Arc::new(host.clone()));
        }
        let starter = host.clone();
        let cwd = workspace.clone();
        let test = test_codex()
            .with_extensions(Arc::new(registry.build()))
            .with_auth(codex_login::CodexAuth::from_api_key(
                "synthetic-duplex-fixture",
            ))
            .with_allowed_tools(if coding {
                vcp_lifecycle::foundation::coding::allowed_tools()
            } else {
                AllowedTools(vec![])
            })
            .with_config(move |c| {
                c.cwd = cwd.try_into().unwrap();
                if coding {
                    configure_provider_fixture(c);
                } else {
                    configure_fixture_provider(c);
                }
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
        if coding {
            host.configure_verification(
                thread,
                vcp_lifecycle::foundation::verification::VerificationConfig {
                    requirements: vec![],
                    rationale:
                        "Retain the owner baseline for hook context and compaction qualification"
                            .into(),
                },
            )
            .unwrap();
        }
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
    pub(super) async fn close(self) {
        self.owner.close().await.unwrap();
        self.test.codex.shutdown_and_wait().await.unwrap();
    }
}
fn definition(id: &str, event: HookEvent, marker: &std::path::Path, mode: &str) -> HookDefinition {
    HookDefinition {
        id: id.into(),
        version: 1,
        source_hash: "a".repeat(64),
        event,
        priority: 0,
        before: vec![],
        after: vec![],
        command: HookCommand {
            profile: "fixture".into(),
            arguments: vec![mode.into(), marker.to_string_lossy().into_owned()],
            working_directory: "hooks-workdir".into(),
        },
        effect_scope: HookEffectScope::BrokerProfile,
        timeout_ms: 10_000,
        max_output_bytes: 64 * 1024,
        failure_policy: FailurePolicy::Block,
    }
}
fn input(f: &Fixture, event: HookEvent, event_id: &str) -> vcp_extensions::hooks::input::HookInput {
    f.host
        .hook_input(
            f.thread,
            event,
            event_id.into(),
            event_id.into(),
            0,
            vec![],
            serde_json::json!({}),
        )
        .unwrap()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_registered_raw_gate_rejects_changed_resources_without_rewrite() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend).await;
        fs::write(f.workspace.join("observed.txt"), "original source").unwrap();
        let marker = f.workspace.join("resource-change-marker");
        let hook = definition(
            "source-change",
            HookEvent::BeforeToolAuthorization,
            &marker,
            "edit_source",
        );
        f.host.configure_hooks(f.thread, vec![hook]).unwrap();
        let request = vcp_tools::Request::Read {
            path: "observed.txt".into(),
            max_bytes: 1024,
            start_line: None,
            end_line: None,
        };
        let arguments = serde_json::json!({"path":"observed.txt","max_bytes":1024,"start_line":null,"end_line":null});
        let result = f
            .host
            .prepare_gated_tool(f.thread, request, arguments, "source-change-event".into())
            .await;
        match result {
            Err(error) => assert!(error.contains("requires refreshed hook input"), "{error}"),
            Ok(_) => panic!("stale validation must not authorize changed source resources"),
        }
        assert_eq!(
            fs::read_to_string(f.workspace.join("observed.txt")).unwrap(),
            "changed by hook"
        );
        assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
        f.close().await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_registered_raw_gate_rewrites_before_final_tool_preparation() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend).await;
        fs::write(f.workspace.join("rewritten.txt"), "rewritten target").unwrap();
        let marker = f.workspace.join("raw-gate-marker");
        let hook = definition(
            "raw-rewrite",
            HookEvent::BeforeToolAuthorization,
            &marker,
            "rewrite_read",
        );
        f.host
            .configure_hooks(f.thread, vec![hook.clone()])
            .unwrap();
        assert!(f.host.configure_hooks(f.thread, vec![hook]).is_err());
        let request = vcp_tools::Request::Read {
            path: "input.txt".into(),
            max_bytes: 1024,
            start_line: None,
            end_line: None,
        };
        let arguments = serde_json::json!({"path":"input.txt","max_bytes":1024,"start_line":null,"end_line":null});
        let (proposal, hooks) = f
            .host
            .prepare_gated_tool(f.thread, request, arguments, "raw-read-event".into())
            .await
            .unwrap();
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].receipt.status, HookStatus::Validated);
        let rewrite = hooks[0]
            .receipt
            .output
            .as_ref()
            .unwrap()
            .rewrite
            .as_ref()
            .unwrap();
        assert_ne!(proposal.digest(), rewrite.original_digest);
        let outcome = f.host.schedule_tool(proposal).await.unwrap();
        assert_eq!(outcome.result["text"], "rewritten target");
        assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
        f.close().await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_hooks_order_each_event_and_reject_duplicate_delivery() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend).await;
        for (index, event) in [
            HookEvent::SessionStart,
            HookEvent::TaskStart,
            HookEvent::BeforeContextAssembly,
            HookEvent::BeforeToolAuthorization,
            HookEvent::AfterToolCompletion,
            HookEvent::BeforeCompaction,
            HookEvent::AfterVerification,
            HookEvent::TaskCompletion,
        ]
        .into_iter()
        .enumerate()
        {
            let marker = f.workspace.join(format!("event-{index}"));
            let a = definition("alpha", event, &marker, "valid");
            let mut z = definition("zeta", event, &marker, "valid");
            z.before.push("alpha".into());
            let definitions = vec![a, z];
            let input = input(&f, event, &format!("event-{index}"));
            let plans = planner::plan(&definitions, &input, HookLimits::default()).unwrap();
            let outcomes = f
                .host
                .run_hooks(f.thread, &definitions, input)
                .await
                .unwrap();
            assert_eq!(outcomes.len(), 2);
            assert!(outcomes
                .iter()
                .all(|o| o.receipt.status == HookStatus::Validated));
            assert_eq!(fs::read_to_string(&marker).unwrap(), "zeta\nalpha\n");
            for plan in plans {
                let receipt = f
                    .host
                    .inspect_hook(f.thread, plan.identity.clone())
                    .unwrap()
                    .unwrap();
                assert!(receipt.receipt.is_some());
                assert!(f.host.prepare_hook(f.thread, plan).is_err());
            }
            assert_eq!(fs::read_to_string(&marker).unwrap().lines().count(), 2);
        }
        f.close().await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_hooks_enforce_timeout_output_validation_and_environment_scope() {
    struct Guard(Option<std::ffi::OsString>);
    impl Drop for Guard {
        fn drop(&mut self) {
            match &self.0 {
                Some(v) => std::env::set_var("VCP_HOOK_SECRET_CANARY", v),
                None => std::env::remove_var("VCP_HOOK_SECRET_CANARY"),
            }
        }
    }
    let _guard = Guard(std::env::var_os("VCP_HOOK_SECRET_CANARY"));
    std::env::set_var("VCP_HOOK_SECRET_CANARY", "synthetic-secret");
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend).await;
        for mode in ["timeout", "malformed", "overflow", "environment"] {
            let marker = f.workspace.join(mode);
            let mut def = definition(mode, HookEvent::TaskStart, &marker, mode);
            if mode == "timeout" {
                def.timeout_ms = 500;
            }
            let out = f
                .host
                .run_hooks(f.thread, &[def], input(&f, HookEvent::TaskStart, mode))
                .await;
            if mode == "overflow" && out.is_err() {
                assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
                continue;
            }
            let out = out.unwrap();
            assert_eq!(out.len(), 1);
            if mode == "environment" {
                assert_eq!(out[0].receipt.status, HookStatus::Validated);
                let findings = &out[0].receipt.output.as_ref().unwrap().findings;
                assert!(findings.contains(&"ambient=false".into()));
                assert!(findings.contains(&"ci=canonical-duplex-fixture".into()));
            } else {
                assert_eq!(out[0].receipt.status, HookStatus::Blocked);
            }
            assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
        }
        f.close().await;
    }
}
fn current_task(f: &Fixture) -> Task {
    f.host
        .snapshot()
        .unwrap()
        .record(
            Collection::Task,
            f.config.root_task.as_str(),
            &f.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap()
}
fn approve(f: &Fixture, question: ApprovalId, digest: &str) {
    let current = current_task(f);
    f.host
        .command(
            Command::Decide {
                id: question,
                operation_digest: digest.into(),
                effect_revision: Revision::new(1),
                allow: true,
            },
            Some(f.config.root_task.clone()),
            Revision::ZERO,
        )
        .unwrap();
    f.host
        .resume(f.thread, current.revision, current.fingerprint)
        .unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_hook_rewrite_after_approval_requires_fresh_authorization() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let mut f = Fixture::new(backend).await;
        fs::create_dir(f.workspace.join("changed-path")).unwrap();
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
        let original = f
            .host
            .prepare_process(
                f.thread,
                Request {
                    profile: "fixture".into(),
                    arguments: vec![
                        "valid".into(),
                        f.workspace
                            .join("original-marker")
                            .to_string_lossy()
                            .into_owned(),
                    ],
                    directory: String::new(),
                    timeout_ms: 10000,
                    output_bytes: 65536,
                    input: None,
                },
            )
            .unwrap();
        approve(&f, original.question.clone().unwrap(), original.digest());
        let digest = original.digest().to_owned();
        let event = HookEvent::BeforeToolAuthorization;
        let mut input = input(&f, event, "rewrite");
        input.payload = serde_json::json!({"operation_digest":digest,"rewrite":{"original_digest":digest,"tool":"vcp_exec","arguments":{"profile":"fixture","arguments":["valid",f.workspace.join("rewritten-marker")],"directory":"changed-path","timeout_ms":10000,"output_bytes":65536}}});
        let plan = planner::plan(
            &[definition(
                "rewrite",
                event,
                &f.workspace.join("hook-marker"),
                "rewrite",
            )],
            &input,
            HookLimits::default(),
        )
        .unwrap()
        .remove(0);
        let hook = f.host.prepare_hook(f.thread, plan).unwrap();
        // Approval digest is stored canonically on its effect.
        let effect: vcp_domain::effect::Effect = f
            .host
            .snapshot()
            .unwrap()
            .record(
                Collection::Effect,
                hook.effect().as_str(),
                &f.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        approve(
            &f,
            hook.question().unwrap().clone(),
            &effect.operation_digest,
        );
        let outcome = f.host.dispatch_hook(hook).await.unwrap();
        assert_eq!(outcome.receipt.status, HookStatus::Validated);
        let replacement = f
            .host
            .rewrite_hook_process(f.thread, original, &outcome)
            .unwrap();
        assert!(
            replacement.question.is_some(),
            "approval of old arguments must not transfer"
        );
        assert_ne!(replacement.digest(), digest);
        assert!(f.host.dispatch_process(replacement).is_err());
        assert!(!f.workspace.join("original-marker").exists());
        assert!(!f.workspace.join("rewritten-marker").exists());
        assert_eq!(
            fs::read_to_string(f.workspace.join("hook-marker"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        f.close().await;
    }
}
#[cfg(feature = "qualification")]
fn record(path: &std::path::Path, value: &impl serde::Serialize) {
    use std::io::Write;
    let temporary = path.with_extension("tmp");
    let mut file = fs::File::create(&temporary).unwrap();
    file.write_all(&serde_json::to_vec(value).unwrap()).unwrap();
    file.sync_all().unwrap();
    drop(file);
    fs::rename(temporary, path).unwrap();
}
#[cfg(feature = "qualification")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "supervisor launches this independently killed owner"]
async fn native_hook_kill_owner() {
    let root = std::path::PathBuf::from(std::env::var_os("VCP_HOOK_KILL_ROOT").unwrap());
    let phase = std::env::var("VCP_HOOK_KILL_PHASE").unwrap();
    let backend = if std::env::var("VCP_HOOK_KILL_BACKEND").unwrap() == "files" {
        BackendKind::Files
    } else {
        BackendKind::Sqlite
    };
    let mut f = Fixture::new(backend).await;
    f._temp.disable_cleanup(true);
    let mode = if phase == "effect" {
        "timeout"
    } else {
        "valid"
    };
    let marker = f.workspace.join("kill-marker");
    let mut def = definition("kill", HookEvent::TaskStart, &marker, mode);
    def.timeout_ms = 60000;
    let plan = planner::plan(
        &[def],
        &input(&f, HookEvent::TaskStart, "kill"),
        HookLimits::default(),
    )
    .unwrap()
    .remove(0);
    let hook = f.host.prepare_hook(f.thread, plan.clone()).unwrap();
    let effect = hook.effect().clone();
    record(&root.join("config.json"), &f.config);
    let arrived = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let dispatch = if phase == "intent" {
        None
    } else {
        let hook = hook.qualification_block_before_receipt(arrived.clone(), release);
        let host = f.host.clone();
        Some(tokio::spawn(async move { host.dispatch_hook(hook).await }))
    };
    if phase == "receipt" {
        tokio::time::timeout(Duration::from_secs(15), arrived.notified())
            .await
            .unwrap();
    }
    if phase == "effect" {
        tokio::time::timeout(Duration::from_secs(15), async {
            while !marker.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
    record(
        &root.join("barrier.json"),
        &serde_json::json!({"plan":plan,"effect":effect,"marker":marker,"fixture_root":f._temp.path()}),
    );
    tokio::time::sleep(Duration::from_secs(60)).await;
    drop(dispatch);
    panic!("supervisor did not kill hook owner before deadline");
}
#[cfg(feature = "qualification")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_hooks_owner_kill_at_intent_effect_and_receipt_never_replays() {
    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for backend in ["files", "sqlite"] {
        for phase in ["intent", "effect", "receipt"] {
            let root = tempfile::tempdir().unwrap();
            let mut child = ChildGuard(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "hooks::native_hook_kill_owner",
                        "--ignored",
                        "--nocapture",
                    ])
                    .env("VCP_HOOK_KILL_ROOT", root.path())
                    .env("VCP_HOOK_KILL_PHASE", phase)
                    .env("VCP_HOOK_KILL_BACKEND", backend)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::inherit())
                    .spawn()
                    .unwrap(),
            );
            tokio::time::timeout(Duration::from_secs(45), async {
                while !root.path().join("barrier.json").exists() {
                    assert!(
                        child.0.try_wait().unwrap().is_none(),
                        "owner exited before barrier"
                    );
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            })
            .await
            .unwrap();
            let barrier: serde_json::Value =
                serde_json::from_slice(&fs::read(root.path().join("barrier.json")).unwrap())
                    .unwrap();
            let config: Config =
                serde_json::from_slice(&fs::read(root.path().join("config.json")).unwrap())
                    .unwrap();
            let marker = std::path::PathBuf::from(barrier["marker"].as_str().unwrap());
            let written = if phase == "intent" {
                assert!(!marker.exists());
                vec![]
            } else {
                let b = fs::read(&marker).unwrap();
                assert_eq!(String::from_utf8(b.clone()).unwrap().lines().count(), 1);
                b
            };
            child.0.kill().unwrap();
            assert!(!child.0.wait().unwrap().success());
            if phase == "effect" {
                use std::os::windows::fs::OpenOptionsExt;
                tokio::time::timeout(Duration::from_secs(10), async {
                    loop {
                        if fs::OpenOptions::new()
                            .write(true)
                            .share_mode(0)
                            .open(marker.with_extension("lock"))
                            .is_ok()
                        {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(25)).await;
                    }
                })
                .await
                .expect("hook child retained its native lock after owner kill");
            }
            for _ in 0..2 {
                let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
                host.reconcile_effects().unwrap();
                let state = host.snapshot().unwrap();
                let result_id = format!(
                    "hook-result-{}",
                    barrier["plan"]["identity"].as_str().unwrap()
                );
                assert!(!state
                    .records
                    .contains_key(&vcp_store::contract::key(Collection::Artifact, &result_id)));
                let effect: vcp_domain::effect::Effect = state
                    .record(
                        Collection::Effect,
                        barrier["effect"].as_str().unwrap(),
                        &config.workspace,
                    )
                    .unwrap()
                    .decode()
                    .unwrap();
                if phase == "effect" {
                    assert_eq!(
                        effect.state,
                        vcp_domain::effect::EffectState::OutcomeUnknown
                    );
                }
                owner.close().await.unwrap();
                drop(host);
                if phase == "intent" {
                    assert!(!marker.exists());
                } else {
                    assert_eq!(fs::read(&marker).unwrap(), written);
                }
            }
            // Preserve killed-owner evidence in the same manner as existing native qualification.
            eprintln!(
                "hook kill evidence {backend}/{phase}: {}",
                barrier["fixture_root"]
            );
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_hook_stale_steering_and_pause_prevent_effect_dispatch() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for steer in [false, true] {
            let mut f = Fixture::new(backend).await;
            let marker = f.workspace.join("stale-marker");
            let event = HookEvent::TaskStart;
            let input = input(&f, event, "stale");
            let plan = planner::plan(
                &[definition("stale", event, &marker, "valid")],
                &input,
                HookLimits::default(),
            )
            .unwrap()
            .remove(0);
            let proposal = f.host.prepare_hook(f.thread, plan).unwrap();
            let task = current_task(&f);
            let (command, target, revision) = if steer {
                let mut objective = task.objectives.last().unwrap().clone();
                objective.text = "new steering invalidates prepared hook".into();
                (
                    Command::Steer { objective },
                    Some(f.config.root_task.clone()),
                    task.revision,
                )
            } else {
                f.policy.revision = PolicyRevision::new(1);
                f.policy.mode = Autonomy::Plan;
                (
                    Command::SetPolicy {
                        policy: f.policy.clone(),
                    },
                    None,
                    Revision::ZERO,
                )
            };
            f.host
                .change_authority(command, target, revision)
                .unwrap()
                .wait()
                .await
                .unwrap();
            assert!(f.host.dispatch_hook(proposal).await.is_err());
            assert!(!marker.exists());
            assert!(f
                .host
                .hook_input(
                    f.thread,
                    event,
                    "paused".into(),
                    "paused".into(),
                    0,
                    vec![],
                    serde_json::json!({})
                )
                .is_err());
            f.close().await;
        }
    }
}
#[cfg(feature = "qualification")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_hook_late_rewrite_after_steering_is_durable_but_cannot_authorize() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend).await;
        let marker = f.workspace.join("late-hook-marker");
        let rewritten_marker = f.workspace.join("late-rewritten-marker");
        let original = f
            .host
            .prepare_process(
                f.thread,
                Request {
                    profile: "fixture".into(),
                    arguments: vec![
                        "valid".into(),
                        f.workspace
                            .join("late-original-marker")
                            .to_string_lossy()
                            .into_owned(),
                    ],
                    directory: String::new(),
                    timeout_ms: 10000,
                    output_bytes: 65536,
                    input: None,
                },
            )
            .unwrap();
        let mut input = input(&f, HookEvent::BeforeToolAuthorization, "late-rewrite");
        input.payload = serde_json::json!({"operation_digest":original.digest(),"rewrite":{
            "original_digest":original.digest(),"tool":"vcp_exec","arguments":{
                "profile":"fixture","arguments":["valid",rewritten_marker],"directory":"hooks-workdir","timeout_ms":10000,"output_bytes":65536
            }
        }});
        let plan = planner::plan(
            &[definition(
                "late-rewrite",
                HookEvent::BeforeToolAuthorization,
                &marker,
                "rewrite",
            )],
            &input,
            HookLimits::default(),
        )
        .unwrap()
        .remove(0);
        let identity = plan.identity.clone();
        let arrived = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let hook = f
            .host
            .prepare_hook(f.thread, plan)
            .unwrap()
            .qualification_block_before_receipt(arrived.clone(), release.clone());
        let host = f.host.clone();
        let dispatched = tokio::spawn(async move { host.dispatch_hook(hook).await });
        tokio::time::timeout(Duration::from_secs(15), arrived.notified())
            .await
            .unwrap();
        assert!(f
            .host
            .inspect_hook(f.thread, identity.clone())
            .unwrap()
            .unwrap()
            .receipt
            .is_none());
        let task = current_task(&f);
        let mut objective = task.objectives.last().unwrap().clone();
        objective.text = "steering changed after external hook returned".into();
        f.host
            .change_authority(
                Command::Steer { objective },
                Some(f.config.root_task.clone()),
                task.revision,
            )
            .unwrap()
            .wait()
            .await
            .unwrap();
        release.notify_one();
        let outcome = tokio::time::timeout(Duration::from_secs(15), dispatched)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(outcome.receipt.status, HookStatus::Blocked);
        assert!(outcome
            .receipt
            .reason
            .as_deref()
            .unwrap()
            .contains("late hook output requires revalidation"));
        let durable = f
            .host
            .inspect_hook(f.thread, identity)
            .unwrap()
            .unwrap()
            .receipt
            .unwrap();
        assert_eq!(durable, outcome.receipt);
        assert!(f
            .host
            .rewrite_hook_process(f.thread, original, &outcome)
            .is_err());
        assert!(!rewritten_marker.exists());
        assert!(!f.workspace.join("late-original-marker").exists());
        assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
        f.close().await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_hook_pending_batch_resumes_after_approval_without_replaying() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let mut f = Fixture::new(backend).await;
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
        let marker = f.workspace.join("pending-marker");
        let definitions = [definition(
            "pending",
            HookEvent::TaskStart,
            &marker,
            "valid",
        )];
        let input = input(&f, HookEvent::TaskStart, "pending");
        let plan = planner::plan(&definitions, &input, HookLimits::default())
            .unwrap()
            .remove(0);
        let error = f
            .host
            .run_hooks(f.thread, &definitions, input.clone())
            .await
            .unwrap_err();
        assert!(error.contains("requires current authorization"), "{error}");
        assert!(!marker.exists());
        let pending = f
            .host
            .inspect_hook(f.thread, plan.identity.clone())
            .unwrap()
            .unwrap();
        assert!(pending.receipt.is_none());
        let approval = f
            .host
            .snapshot()
            .unwrap()
            .records
            .values()
            .filter(|r| r.collection == Collection::Approval)
            .map(|r| r.decode::<vcp_protocol::command::Approval>().unwrap())
            .find(|a| a.operation_digest == pending.effect.operation_digest)
            .unwrap();
        approve(&f, approval.id, &pending.effect.operation_digest);
        let first = f
            .host
            .run_hooks(f.thread, &definitions, input.clone())
            .await
            .unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].receipt.status, HookStatus::Validated);
        assert!(!first[0].duplicate);
        let second = f
            .host
            .run_hooks(f.thread, &definitions, input)
            .await
            .unwrap();
        assert_eq!(second.len(), 1);
        assert!(second[0].duplicate);
        assert_eq!(second[0].receipt, first[0].receipt);
        assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
        f.close().await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_hook_cached_result_rejects_changed_pinned_input_or_executable() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for change in ["pinned-input", "executable"] {
            let f = Fixture::new(backend).await;
            let marker = f.workspace.join("source-fence-marker");
            let definitions = [definition(
                "source-fence",
                HookEvent::TaskStart,
                &marker,
                "valid",
            )];
            let input = input(&f, HookEvent::TaskStart, "source-fence");
            let first = f
                .host
                .run_hooks(f.thread, &definitions, input.clone())
                .await
                .unwrap();
            assert_eq!(first[0].receipt.status, HookStatus::Validated);
            let duplicate = f
                .host
                .run_hooks(f.thread, &definitions, input.clone())
                .await
                .unwrap();
            assert!(duplicate[0].duplicate);
            if change == "pinned-input" {
                fs::write(
                    f.workspace.join("input.txt"),
                    b"changed pinned hook script input",
                )
                .unwrap();
            } else {
                let executable = f._temp.path().join("tools").join("fixture.exe");
                let mut changed = fs::read(&executable).unwrap();
                // Preserve a valid native executable while changing its exact identity.
                changed.extend_from_slice(b"changed-hook-executable-source");
                fs::write(executable, changed).unwrap();
            }
            let error = f
                .host
                .run_hooks(f.thread, &definitions, input)
                .await
                .unwrap_err();
            assert!(
                error.contains("changed; result requires revalidation"),
                "{change}: {error}"
            );
            let durable = f
                .host
                .inspect_hook(f.thread, first[0].receipt.identity.clone())
                .unwrap()
                .unwrap();
            assert_eq!(durable.receipt, Some(first[0].receipt.clone()));
            assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
            f.close().await;
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_hook_cached_result_rejects_changed_policy_without_new_steering() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let mut f = Fixture::new(backend).await;
        let marker = f.workspace.join("policy-fence-marker");
        let definitions = [definition(
            "policy-fence",
            HookEvent::TaskStart,
            &marker,
            "valid",
        )];
        let input = input(&f, HookEvent::TaskStart, "policy-fence");
        let first = f
            .host
            .run_hooks(f.thread, &definitions, input.clone())
            .await
            .unwrap();
        assert_eq!(first[0].receipt.status, HookStatus::Validated);
        let before = current_task(&f);
        f.policy.revision = PolicyRevision::new(1);
        f.host
            .command(
                Command::SetPolicy {
                    policy: f.policy.clone(),
                },
                None,
                Revision::ZERO,
            )
            .unwrap();
        assert_eq!(current_task(&f).steering, before.steering);
        // Prove the input is otherwise current and the changed authority fence is what rejects reuse.
        f.host
            .hook_input(
                f.thread,
                HookEvent::TaskStart,
                "fresh-after-policy".into(),
                "fresh-after-policy".into(),
                0,
                vec![],
                serde_json::json!({}),
            )
            .unwrap();
        let error = f
            .host
            .run_hooks(f.thread, &definitions, input)
            .await
            .unwrap_err();
        assert!(
            error.contains("changed; result requires revalidation"),
            "{error}"
        );
        let durable = f
            .host
            .inspect_hook(f.thread, first[0].receipt.identity.clone())
            .unwrap()
            .unwrap();
        assert_eq!(durable.receipt, Some(first[0].receipt.clone()));
        assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
        f.close().await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_hook_context_precedes_retained_model_request_and_transport_retry() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };
    use vcp_lifecycle::foundation::coding::CodingConfig;
    use wiremock::{
        matchers::{method, path},
        Mock, ResponseTemplate,
    };
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::with_coding(backend, true).await;
        let marker = f.workspace.join("model-context-marker");
        let compaction_marker = f.workspace.join("compaction-marker");
        let session_marker = f.workspace.join("session-marker");
        let task_marker = f.workspace.join("task-marker");
        let completion_marker = f.workspace.join("completion-marker");
        f.host
            .configure_hooks(
                f.thread,
                vec![
                    definition(
                        "model-context",
                        HookEvent::BeforeContextAssembly,
                        &marker,
                        "context",
                    ),
                    definition(
                        "model-compaction",
                        HookEvent::BeforeCompaction,
                        &compaction_marker,
                        "valid",
                    ),
                    definition("session", HookEvent::SessionStart, &session_marker, "valid"),
                    definition("task", HookEvent::TaskStart, &task_marker, "valid"),
                    definition(
                        "completion",
                        HookEvent::TaskCompletion,
                        &completion_marker,
                        "valid",
                    ),
                ],
            )
            .unwrap();
        f.host.start_lifecycle_hooks(f.thread).await.unwrap();
        f.host.start_lifecycle_hooks(f.thread).await.unwrap();
        assert_eq!(
            fs::read_to_string(&session_marker).unwrap().lines().count(),
            1
        );
        assert_eq!(fs::read_to_string(&task_marker).unwrap().lines().count(), 1);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        f.host
            .configure_coding(
                f.thread,
                CodingConfig {
                    operating: "Observe attributed hook evidence and respond without tools.".into(),
                    affected_paths: vec!["input.txt".into()],
                    max_requests: 3,
                    deadline: Timestamp::new(now + 300000),
                },
            )
            .unwrap();
        f.host
            .configure_continuity(
                f.thread,
                vcp_context::compaction::Config {
                    keep_recent_pairs: 1,
                    preview_bytes: 64,
                    minimum_gain_bytes: 256,
                },
            )
            .unwrap();
        let observed = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let requests = observed.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let requests_seen = calls.clone();
        let gate_marker = marker.clone();
        Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request:&wiremock::Request| {
            // This assertion occurs at the actual transport boundary, before any response.
            assert_eq!(fs::read_to_string(&gate_marker).unwrap().lines().count(),1);
            let body:serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            requests.lock().unwrap().push(body);
            if requests_seen.fetch_add(1,Ordering::SeqCst)==0 { return ResponseTemplate::new(503); }
            ResponseTemplate::new(200).insert_header("content-type","text/event-stream").set_body_string(sse(vec![
                ev_assistant_message("hook-observed","Attributed hook evidence was observed."),
                serde_json::json!({"type":"response.completed","response":{"id":"hook-model-boundary","status":"completed","output":[],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}}),
            ]))
        }).mount(&f._server).await;
        f.test
            .codex
            .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                text: "Observe hook context.".into(),
                text_elements: vec![],
            }]))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(120), async {
            loop {
                let event = f.test.codex.next_event().await.unwrap();
                if matches!(event.msg, EventMsg::TurnComplete(_)) {
                    break;
                }
            }
        })
        .await
        .unwrap();
        let requests = observed.lock().unwrap().clone();
        assert_eq!(requests.len(), 2, "one retry of the retained request");
        for request in requests {
            let context = request["input"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|item| item["type"] == "message")
                .filter_map(|item| {
                    serde_json::from_str::<serde_json::Value>(item["content"][0]["text"].as_str()?)
                        .ok()
                })
                .find(|part| part.to_string().contains("NATIVE_HOOK_CONTEXT_PROVENANCE"))
                .expect("hook context must reach the model as attributed structured evidence");
            let serialized = context.to_string();
            assert!(serialized.contains("untrusted"), "{serialized}");
            assert!(serialized.contains("hook-result-"), "{serialized}");
        }
        assert_eq!(fs::read_to_string(&marker).unwrap().lines().count(), 1);
        assert!(
            !compaction_marker.exists(),
            "empty history has no eligible compaction, even with continuity enabled"
        );
        f.host.complete_lifecycle_hooks(f.thread).await.unwrap();
        f.host.complete_lifecycle_hooks(f.thread).await.unwrap();
        assert_eq!(
            fs::read_to_string(completion_marker)
                .unwrap()
                .lines()
                .count(),
            1
        );
        f.close().await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_hook_optional_failure_warns_only_without_malformed_output() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend).await;
        for (mode, expected) in [
            ("malformed_failure", HookStatus::Blocked),
            ("notification_failure", HookStatus::Warning),
        ] {
            let marker = f.workspace.join(mode);
            let mut definition = definition(mode, HookEvent::TaskStart, &marker, mode);
            definition.failure_policy = FailurePolicy::Warn;
            let outcomes = f
                .host
                .run_hooks(
                    f.thread,
                    &[definition],
                    input(&f, HookEvent::TaskStart, mode),
                )
                .await
                .unwrap();
            assert_eq!(outcomes.len(), 1);
            assert_eq!(outcomes[0].receipt.status, expected);
            assert!(outcomes[0].receipt.output.is_none());
            if mode == "malformed_failure" {
                assert!(outcomes[0]
                    .receipt
                    .reason
                    .as_deref()
                    .unwrap()
                    .contains("invalid hook output"));
            }
            assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
        }
        f.close().await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_hook_compaction_fires_only_for_eligible_retained_history() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use vcp_lifecycle::foundation::coding::CodingConfig;
    use wiremock::{
        matchers::{method, path},
        Mock, ResponseTemplate,
    };
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::with_coding(backend, true).await;
        // This fixture qualifies hook timing, not the separate context-fit gate.
        let (snapshot, raw) = provider_snapshot();
        let mut endpoint: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        endpoint["data"]["endpoints"][0]["context_length"] = serde_json::json!(128_000);
        endpoint["data"]["endpoints"][0]["max_prompt_tokens"] = serde_json::json!(96_000);
        let raw = serde_json::to_vec(&endpoint).unwrap();
        let snapshot = vcp_models::catalog::Snapshot::from_endpoints(
            &raw,
            snapshot.observed_at,
            snapshot.valid_until,
            snapshot.compatibility,
        )
        .unwrap();
        f.host.configure_provider(snapshot, raw).unwrap();
        fs::write(
            f.workspace.join("history.txt"),
            "historical source material\n".repeat(200),
        )
        .unwrap();
        let marker = f.workspace.join("eligible-compaction-marker");
        f.host
            .configure_hooks(
                f.thread,
                vec![definition(
                    "eligible-compaction",
                    HookEvent::BeforeCompaction,
                    &marker,
                    "valid",
                )],
            )
            .unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        f.host
            .configure_coding(
                f.thread,
                CodingConfig {
                    operating: "Read history twice, then summarize.".into(),
                    affected_paths: vec!["input.txt".into()],
                    max_requests: 4,
                    deadline: Timestamp::new(now + 300000),
                },
            )
            .unwrap();
        f.host
            .configure_continuity(
                f.thread,
                vcp_context::compaction::Config {
                    keep_recent_pairs: 1,
                    preview_bytes: 64,
                    minimum_gain_bytes: 256,
                },
            )
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let gate_marker = marker.clone();
        Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |_:&wiremock::Request| {
            let n=observed.fetch_add(1,Ordering::SeqCst);
            if n < 2 { assert!(!gate_marker.exists(),"zero/one history pair must not compact"); }
            else { assert_eq!(fs::read_to_string(&gate_marker).unwrap().lines().count(),1,"eligible historical pair gates before model transport"); }
            let mut events=vec![];
            let mut output=vec![];
            if n < 2 {
                let item=serde_json::json!({"type":"function_call","id":format!("hook-read-{n}"),"call_id":format!("hook-read-{n}"),"name":"vcp_read","arguments":serde_json::json!({"path":"history.txt","max_bytes":40000,"start_line":null,"end_line":null}).to_string(),"status":"completed"});
                events.push(serde_json::json!({"type":"response.output_item.done","output_index":0,"item":item}));
                output.push(item);
            } else {events.push(ev_assistant_message("compaction-observed","Observed compacted historical source."));}
            events.push(serde_json::json!({"type":"response.completed","response":{"id":format!("hook-compaction-{n}"),"status":"completed","output":output,"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}}));
            ResponseTemplate::new(200).insert_header("content-type","text/event-stream").set_body_string(sse(events))
        }).mount(&f._server).await;
        f.test
            .codex
            .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                text: "Read history twice.".into(),
                text_elements: vec![],
            }]))
            .await
            .unwrap();
        let mut diagnostics = Vec::new();
        tokio::time::timeout(Duration::from_secs(120), async {
            loop {
                let event = f.test.codex.next_event().await.unwrap();
                let diagnostic = format!("{:?}", event.msg);
                diagnostics.push(diagnostic.chars().take(1200).collect::<String>());
                if matches!(event.msg, EventMsg::TurnComplete(_)) {
                    break;
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 3, "{backend:?}: retained compaction transport refused; hook marker exists={}; events={diagnostics:#?}", marker.exists());
        assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
        f.close().await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_hook_owner_approval_resume_reuses_pending_hook_for_same_user_bytes() {
    use codex_extension_api::{HostModelPurpose, HostWorkAdmission};
    use vcp_lifecycle::foundation::coding::CodingConfig;
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let mut f = Fixture::with_coding(backend, true).await;
        let marker = f.workspace.join("owner-resume-marker");
        f.host
            .configure_hooks(
                f.thread,
                vec![definition(
                    "owner-resume",
                    HookEvent::BeforeContextAssembly,
                    &marker,
                    "context",
                )],
            )
            .unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        f.host
            .configure_coding(
                f.thread,
                CodingConfig {
                    operating: "Preserve pending hook across explicit resume.".into(),
                    affected_paths: vec!["input.txt".into()],
                    max_requests: 3,
                    deadline: Timestamp::new(now + 300000),
                },
            )
            .unwrap();
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
        let first_turn = f
            .host
            .begin_coding_turn(f.thread, "same owner user bytes".into())
            .unwrap();
        let error = f
            .host
            .prepare_model(f.thread, HostModelPurpose::Turn)
            .await
            .unwrap_err();
        assert!(error.contains("requires current authorization"), "{error}");
        assert!(!marker.exists());
        let state = f.host.snapshot().unwrap();
        let approval = state
            .records
            .values()
            .filter(|r| r.collection == Collection::Approval)
            .map(|r| r.decode::<vcp_protocol::command::Approval>().unwrap())
            .next()
            .unwrap();
        let original_effect = approval.effect.clone();
        pause_coding_owner(&f).await;
        approve(&f, approval.id, &approval.operation_digest);
        let second_turn = f
            .host
            .begin_coding_turn(f.thread, "same owner user bytes".into())
            .unwrap();
        assert_ne!(first_turn, second_turn);
        f.host
            .prepare_model(f.thread, HostModelPurpose::Turn)
            .await
            .unwrap();
        assert_eq!(fs::read_to_string(&marker).unwrap().lines().count(), 1);
        let state = f.host.snapshot().unwrap();
        let effects: Vec<_> = state
            .records
            .values()
            .filter(|r| r.collection == Collection::Effect)
            .map(|r| r.decode::<vcp_domain::effect::Effect>().unwrap())
            .collect();
        assert_eq!(
            effects.len(),
            1,
            "resume must retain the original hook effect identity"
        );
        assert_eq!(effects[0].id, original_effect);
        f.host
            .prepare_model(f.thread, HostModelPurpose::Turn)
            .await
            .unwrap();
        assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
        f.close().await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_hook_later_batch_member_invalidates_earlier_rewrite_source() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let mut f = Fixture::new(backend).await;
        fs::write(f.workspace.join("observed.txt"), "original request").unwrap();
        fs::write(f.workspace.join("rewritten.txt"), "rewritten request").unwrap();
        f.policy.revision = PolicyRevision::new(1);
        f.policy
            .workspace_roots
            .insert(RootId::parse("exec-unpinned").unwrap());
        f.host
            .command(
                Command::SetPolicy {
                    policy: f.policy.clone(),
                },
                None,
                Revision::ZERO,
            )
            .unwrap();
        f.host
            .configure_process_profile(
                Profile::new(
                    "unpinned".into(),
                    f._temp.path().join("tools").join("fixture.exe"),
                    Mode::Direct,
                    BTreeMap::from([("SystemRoot".into(), std::env::var("SystemRoot").unwrap())]),
                    BTreeSet::new(),
                    true,
                )
                .unwrap(),
            )
            .unwrap();
        let mut first = definition(
            "rewrite-first",
            HookEvent::BeforeToolAuthorization,
            &f.workspace.join("rewrite-first-marker"),
            "rewrite_read",
        );
        first.before.push("mutate-later".into());
        let mut second = definition(
            "mutate-later",
            HookEvent::BeforeToolAuthorization,
            &f.workspace.join("mutate-later-marker"),
            "edit_pinned",
        );
        second.command.profile = "unpinned".into();
        f.host
            .configure_hooks(f.thread, vec![first, second])
            .unwrap();
        let request=vcp_tools::Request::from_call("vcp_read",&serde_json::json!({"path":"observed.txt","max_bytes":1024,"start_line":null,"end_line":null}).to_string()).unwrap();
        let error = f
            .host
            .prepare_gated_tool(f.thread, request, serde_json::json!({"path":"observed.txt","max_bytes":1024,"start_line":null,"end_line":null}), "batch-source-fence".into())
            .await
            .err()
            .unwrap();
        assert!(error.contains("requires revalidation"), "{error}");
        assert_eq!(
            fs::read_to_string(f.workspace.join("input.txt")).unwrap(),
            "source changed by later hook"
        );
        for name in ["rewrite-first-marker", "mutate-later-marker"] {
            assert_eq!(
                fs::read_to_string(f.workspace.join(name))
                    .unwrap()
                    .lines()
                    .count(),
                1
            );
        }
        let effects = f
            .host
            .snapshot()
            .unwrap()
            .records
            .values()
            .filter(|r| r.collection == Collection::Effect)
            .count();
        assert_eq!(
            effects, 2,
            "only hook effects may be proposed before stale rewrite rejection"
        );
        f.close().await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_hook_model_admission_rejects_omitted_and_stale_async_gate() {
    use codex_extension_api::{HostModelPurpose, HostWorkAdmission};
    use vcp_lifecycle::foundation::coding::CodingConfig;
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for stale in [false, true] {
            let f = Fixture::with_coding(backend, true).await;
            let marker = f.workspace.join("model-token-marker");
            f.host
                .configure_hooks(
                    f.thread,
                    vec![definition(
                        "model-token",
                        HookEvent::BeforeContextAssembly,
                        &marker,
                        "context",
                    )],
                )
                .unwrap();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            f.host
                .configure_coding(
                    f.thread,
                    CodingConfig {
                        operating: "Require current asynchronous hook gate.".into(),
                        affected_paths: vec!["input.txt".into()],
                        max_requests: 3,
                        deadline: Timestamp::new(now + 300000),
                    },
                )
                .unwrap();
            f.host
                .begin_coding_turn(f.thread, "original input".into())
                .unwrap();
            if stale {
                f.host
                    .prepare_model(f.thread, HostModelPurpose::Turn)
                    .await
                    .unwrap();
                assert_eq!(fs::read_to_string(&marker).unwrap().lines().count(), 1);
                pause_coding_owner(&f).await;
                let paused = current_task(&f);
                f.host
                    .resume(f.thread, paused.revision, paused.fingerprint)
                    .unwrap();
                // Changing the actual user bytes between async preparation and
                // synchronous admission must invalidate the armed boundary.
                f.host
                    .begin_coding_turn(f.thread, "different user input after gate".into())
                    .unwrap();
            }
            let mut body = serde_json::json!({"model":"gpt-5.1"});
            let error = f
                .host
                .admit_model(f.thread, &mut body, HostModelPurpose::Turn)
                .err()
                .expect("model admission must reject missing/stale hook preparation");
            assert!(error.contains("hook context"), "{error}");
            assert!(f._server.received_requests().await.unwrap().is_empty());
            if stale {
                assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
            } else {
                assert!(!marker.exists());
            }
            assert_eq!(
                f.host
                    .snapshot()
                    .unwrap()
                    .records
                    .values()
                    .filter(|r| r.collection == Collection::Attempt)
                    .count(),
                0,
                "rejected gate must not reserve a provider attempt"
            );
            f.close().await;
        }
    }
}

async fn pause_coding_owner(f: &Fixture) {
    let current = current_task(f);
    let command = f
        .host
        .control_envelope(
            CommandId::new(),
            f.config.root_task.clone(),
            current.revision,
            Command::Transition {
                next: TaskState::Paused,
                reason: "owner stops the interrupted coding turn before explicit resubmission"
                    .into(),
                verification: None,
            },
        )
        .unwrap();
    f.host.stop(command).unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while !f
            .host
            .lifecycle()
            .inspect(f.thread)
            .unwrap()
            .interrupt_complete
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    // The trusted owner first releases the drained retained hold. Canonical
    // task resumption remains a separate current-revision operation below.
    let retained = f.host.lifecycle().inspect(f.thread).unwrap();
    f.host
        .lifecycle()
        .resume(f.thread, &retained.revision)
        .unwrap();
}
