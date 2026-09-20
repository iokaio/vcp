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
use vcp_lifecycle::foundation::{DuplexIoLimits, DuplexProcess};
use vcp_tools::process::{Mode, Profile, Request};

struct Fixture {
    _temp: tempfile::TempDir,
    workspace: std::path::PathBuf,
    host: CanonicalHost,
    owner: vcp_lifecycle::foundation::CanonicalOwner,
    test: TestCodex,
    _server: wiremock::MockServer,
    thread: codex_protocol::ThreadId,
    config: Config,
    policy: Policy,
}
impl Fixture {
    async fn new(backend: BackendKind) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace ü");
        fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let tools = temp.path().join("tools");
        fs::create_dir(&tools).unwrap();
        let executable = tools.join("fixture.exe");
        fs::copy(env!("CARGO_BIN_EXE_vcp-process-fixture"), &executable).unwrap();
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
    fn request(&self) -> Request {
        Request {
            profile: "fixture".into(),
            arguments: vec![
                "duplex-echo".into(),
                self.workspace.to_string_lossy().into_owned(),
            ],
            directory: String::new(),
            timeout_ms: 15_000,
            output_bytes: 1024 * 1024,
            input: None,
        }
    }
    fn connect(&self) -> DuplexProcess {
        let ticket = self
            .host
            .prepare_process(self.thread, self.request())
            .unwrap();
        self.host.dispatch_duplex_process(ticket, limits()).unwrap()
    }
    async fn close(self) {
        let Self {
            _temp,
            owner,
            test,
            host,
            ..
        } = self;
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        drop(test);
        drop(host);
        drop(_temp);
    }
}
fn limits() -> DuplexIoLimits {
    DuplexIoLimits {
        frame_bytes: 1024,
        queued_frames: 8,
        input_bytes: 4096,
        max_messages: 2,
        stderr_bytes: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_duplex_captures_input_and_holds_exclusive_claim_until_receipt() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let mut f = Fixture::new(backend).await;
        f.policy.revision = PolicyRevision::new(1);
        f.policy.denials.push(Denial {
            id: "deny-duplex-startup".into(),
            origin: RuleOrigin::User,
            reason: "explicit startup denial".into(),
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
        let mut denied_request = f.request();
        denied_request.arguments[0] = "duplex-silent".into();
        let denied = f.host.prepare_process(f.thread, denied_request).unwrap();
        assert!(matches!(denied.decision, vcp_policy::Decision::Deny { .. }));
        let denied_effect = denied.effect().clone();
        assert!(f.host.dispatch_duplex_process(denied, limits()).is_err());
        assert!(
            !f.workspace.join("duplex-ready").exists(),
            "denied startup must not execute its native marker"
        );
        assert!(f
            .host
            .snapshot()
            .unwrap()
            .record(
                Collection::Effect,
                denied_effect.as_str(),
                &f.config.workspace
            )
            .unwrap()
            .decode::<Effect>()
            .unwrap()
            .execution
            .is_none());
        f.policy.denials.clear();
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
        let stale = f.host.prepare_process(f.thread, f.request()).unwrap();
        fs::write(f.workspace.join("input.txt"), b"changed input").unwrap();
        assert!(f.host.dispatch_duplex_process(stale, limits()).is_err());
        let mut connection = f.connect();
        let effect = connection.effect().clone();
        assert!(connection.qualification_write_line(b"").await.is_err());
        assert!(connection
            .qualification_write_line(b"not\na frame")
            .await
            .is_err());
        connection
            .qualification_write_line("hello ü".as_bytes())
            .await
            .unwrap();
        let line = connection.read_line().await.unwrap().unwrap();
        let value: serde_json::Value = serde_json::from_slice(&line).unwrap();
        assert_eq!(value["line"], "hello ü");
        assert!(
            value["public"].is_null(),
            "unconfigured public environment must not leak through"
        );
        assert_eq!(value["path_inherited"], false);
        assert_eq!(value["ci"], "canonical-duplex-fixture");
        let competing = f.host.prepare_process(f.thread, f.request()).unwrap();
        assert!(
            f.host.dispatch_process(competing).is_err(),
            "live arbitrary process must retain exclusive claim"
        );
        connection
            .qualification_write_line(b"second")
            .await
            .unwrap();
        assert!(connection.read_line().await.unwrap().is_some());
        assert!(connection.qualification_write_line(b"third").await.is_err());
        connection.close_stdin();
        let outcome = connection.wait().await.unwrap();
        assert_eq!(outcome.exit_code, Some(0));
        assert_eq!(
            fs::read_to_string(f.workspace.join("duplex-input")).unwrap(),
            "hello ü\nsecond\n",
            "input limits must prevent actual third-frame delivery"
        );
        let snapshot = f.host.snapshot().unwrap();
        let durable: Effect = snapshot
            .record(Collection::Effect, effect.as_str(), &f.config.workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(durable.state, EffectState::Succeeded);
        let inputs: Vec<ArtifactDescriptor> = snapshot
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
            .map(|r| r.decode::<ArtifactDescriptor>().unwrap())
            .filter(|a| a.spec.schema == "vcp-duplex-input-v1")
            .collect();
        assert_eq!(inputs.len(), 2);
        assert!(inputs
            .iter()
            .all(|a| durable.observed_changes.contains(&a.spec.id)));
        assert!(snapshot
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
            .map(|r| r.decode::<ArtifactDescriptor>().unwrap())
            .any(|a| a.spec.schema == "vcp-duplex-lifetime-v1"));
        f.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_duplex_revocation_blocks_queued_output_and_future_input() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let mut f = Fixture::new(backend).await;
        let mut connection = f.connect();
        connection
            .qualification_write_line(b"before-revocation")
            .await
            .unwrap();
        f.policy.revision = PolicyRevision::new(1);
        f.policy.denials.push(Denial {
            id: "revoke-duplex".into(),
            origin: RuleOrigin::User,
            reason: "explicit test revocation".into(),
            effects: BTreeSet::from([EffectClass::Execute]),
            tool: None,
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
                Revision::new(0),
            )
            .unwrap();
        assert!(connection
            .qualification_write_line(b"after-revocation")
            .await
            .is_err());
        assert!(
            connection.read_line().await.is_err(),
            "queued external bytes must respect revocation"
        );
        let _ = connection.wait().await;
        tokio::time::timeout(Duration::from_secs(10), change.wait())
            .await
            .unwrap()
            .unwrap();
        let delivered = fs::read_to_string(f.workspace.join("duplex-input")).unwrap_or_default();
        assert!(
            !delivered.contains("after-revocation"),
            "authority fence must prevent actual native delivery"
        );
        let inputs = f
            .host
            .snapshot()
            .unwrap()
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
            .filter(|r| {
                r.decode::<ArtifactDescriptor>().unwrap().spec.schema == "vcp-duplex-input-v1"
            })
            .count();
        assert_eq!(inputs, 1, "held owner cannot capture or send another frame");
        f.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_duplex_drop_retains_unknown_process_intent_after_reopen() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend).await;
        let mut connection = f.connect();
        let effect = connection.effect().clone();
        connection
            .qualification_write_line(b"observed-before-drop")
            .await
            .unwrap();
        assert!(connection.read_line().await.unwrap().is_some());
        drop(connection);
        let durable: Effect = f
            .host
            .snapshot()
            .unwrap()
            .record(Collection::Effect, effect.as_str(), &f.config.workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(durable.state, EffectState::OutcomeUnknown);
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
        let store = vcp_store::Store::open(&config.canonical_root, backend, &[workspace])
            .await
            .unwrap();
        let durable: Effect = store
            .state()
            .record(Collection::Effect, effect.as_str(), &config.workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(durable.state, EffectState::OutcomeUnknown);
        assert_eq!(
            store
                .state()
                .records
                .values()
                .filter(|r| r.collection == Collection::Effect)
                .count(),
            1,
            "reopen must not replay the connection"
        );
        store.close().await.unwrap();
        drop(_temp);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_duplex_invalid_scheduled_limits_cancel_unsent_ticket() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend).await;
        let ticket = f.host.prepare_process(f.thread, f.request()).unwrap();
        let effect = ticket.effect().clone();
        let mut invalid = limits();
        invalid.frame_bytes = 0;
        assert!(f
            .host
            .schedule_duplex_process(ticket, invalid)
            .await
            .is_err());
        let state = f.host.snapshot().unwrap();
        let effect: Effect = state
            .record(Collection::Effect, effect.as_str(), &f.config.workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(effect.state, EffectState::Cancelled);
        assert!(effect.execution.is_none());
        assert!(!f.workspace.join("duplex-input").exists());
        f.close().await;
    }
}
