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
use vcp_tools::{
    process::{Mode, Profile},
    Request,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn host_tool_denials_survive_user_policy_and_grants_without_native_dispatch() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        fs::write(workspace.join("file.txt"), "original\n").unwrap();
        let mut config = config(&temp.path().join("canonical"), &workspace, backend);
        let root = RootId::parse(config.workspace.as_str()).unwrap();
        config.host_tool_denials = vec![
            Denial {
                id: "host-protected-file".into(),
                origin: RuleOrigin::Host,
                reason: "synthetic protected source".into(),
                effects: BTreeSet::from([EffectClass::Write]),
                tool: Some("vcp_patch".into()),
                roots: BTreeSet::from([root.clone()]),
                paths: vec!["file.txt".into()],
            },
            Denial {
                id: "host-opaque-ceiling".into(),
                origin: RuleOrigin::Host,
                reason: "synthetic installation ceiling".into(),
                effects: BTreeSet::from([EffectClass::Install]),
                tool: Some("vcp_exec".into()),
                roots: BTreeSet::from([RootId::parse("other-root").unwrap()]),
                paths: vec!["restricted".into()],
            },
            Denial {
                id: "host-search-read".into(),
                origin: RuleOrigin::Host,
                reason: "synthetic scoped read ceiling".into(),
                effects: BTreeSet::from([EffectClass::Read]),
                tool: Some("vcp_search".into()),
                roots: BTreeSet::from([root.clone()]),
                paths: vec!["file.txt".into()],
            },
        ];
        let mut invalid = config.clone();
        invalid.host_tool_denials[0].origin = RuleOrigin::User;
        assert!(CanonicalHost::open(invalid)
            .err()
            .unwrap()
            .contains("host denial origin"));
        assert!(
            !config.canonical_root.exists(),
            "invalid host configuration must fail before storage opens"
        );
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
                "synthetic-host-tool-fixture",
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
        let id = host.lifecycle().attach_root(test.codex.clone()).unwrap();
        host.register(id, binding.clone()).unwrap();
        host.command(
            Command::SetWorkspaceTrust {
                trust: Trust::Trusted,
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        let mut policy = Policy {
            workspace: config.workspace.clone(),
            revision: PolicyRevision::ZERO,
            mode: Autonomy::Autonomous,
            denials: vec![],
            workspace_roots: BTreeSet::from([root]),
            automatic_effects: BTreeSet::from([
                EffectClass::Read,
                EffectClass::Write,
                EffectClass::Execute,
                EffectClass::Network,
                EffectClass::Install,
                EffectClass::Publish,
                EffectClass::Opaque,
            ]),
            timeout_ceiling_ms: Units::new(30_000),
            output_ceiling_bytes: ByteCount::new(1024 * 1024),
        };
        host.command(
            Command::SetPolicy {
                policy: policy.clone(),
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        let read = host
            .prepare_tool(
                id,
                Request::Read {
                    path: "file.txt".into(),
                    max_bytes: 1024,
                    start_line: None,
                    end_line: None,
                },
            )
            .unwrap();
        assert_eq!(
            host.dispatch_tool(read).unwrap().result["text"],
            "original\n"
        );
        let patch = || Request::Patch {
            patch:
                "*** Begin Patch\n*** Update File: file.txt\n@@\n-original\n+changed\n*** End Patch"
                    .into(),
        };
        let denied = host.prepare_tool(id, patch()).unwrap();
        let digest = denied.digest().to_owned();
        assert!(
            matches!(&denied.decision, vcp_policy::Decision::Deny { origin, .. } if origin == "host-protected-file")
        );
        assert!(host.dispatch_tool(denied).is_err());
        let state = host.snapshot().unwrap();
        let current: Workspace = state
            .record(
                Collection::Workspace,
                config.workspace.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let current_policy = vcp_engine::policy::current(&state, &config.workspace).unwrap();
        host.command(
            Command::SetGrant {
                grant: Grant {
                    id: GrantId::new(),
                    actor: config.actor.clone(),
                    scope: GrantScope::Task {
                        scope: binding.scope.clone(),
                    },
                    host: current.binding.host,
                    binding: current.binding.revision,
                    authority: current.authority,
                    policy: current_policy.revision,
                    expires_at: Timestamp::new(u64::MAX),
                    target: GrantTarget::Exact { digest },
                    origin: RuleOrigin::User,
                    reason: "synthetic explicit scoped user grant".into(),
                    revoked: false,
                    revision: Revision::ZERO,
                    approval: None,
                },
            },
            Some(binding.scope.task.clone()),
            Revision::ZERO,
        )
        .unwrap();
        let denied = host.prepare_tool(id, patch()).unwrap();
        assert!(
            matches!(&denied.decision, vcp_policy::Decision::Deny { origin, .. } if origin == "host-protected-file")
        );
        assert!(host.dispatch_tool(denied).is_err());
        policy.revision = PolicyRevision::new(1);
        host.command(
            Command::SetPolicy {
                policy: policy.clone(),
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        let denied = host.prepare_tool(id, patch()).unwrap();
        assert!(
            matches!(&denied.decision, vcp_policy::Decision::Deny { origin, .. } if origin == "host-protected-file")
        );
        assert!(host.dispatch_tool(denied).is_err());
        assert_eq!(
            fs::read_to_string(workspace.join("file.txt")).unwrap(),
            "original\n"
        );
        // The host search ceiling is resolved without opening a missing source.
        fs::rename(workspace.join("file.txt"), workspace.join("hidden.txt")).unwrap();
        assert!(host
            .prepare_tool(
                id,
                Request::Search {
                    query: "original".into(),
                    max_hits: 10,
                    mode: None,
                    path_pattern: None,
                    max_files: None,
                    max_scan_bytes: None,
                }
            )
            .err()
            .unwrap()
            .contains("trusted read denial"));
        fs::rename(workspace.join("hidden.txt"), workspace.join("file.txt")).unwrap();
        let executable = std::path::Path::new(env!("CARGO_BIN_EXE_vcp-process-fixture"));
        let profile = Profile::new(
            "fixture".into(),
            executable.into(),
            Mode::Direct,
            BTreeMap::from([("SystemRoot".into(), std::env::var("SystemRoot").unwrap())]),
            BTreeSet::new(),
            true,
        )
        .unwrap();
        host.configure_process_profile(profile).unwrap();
        let marker = workspace.join("not-dispatched");
        let denied = host
            .prepare_process(
                id,
                vcp_tools::process::Request {
                    profile: "fixture".into(),
                    arguments: vec!["write".into(), marker.to_str().unwrap().into()],
                    directory: String::new(),
                    timeout_ms: 10_000,
                    output_bytes: 1024 * 1024,
                    input: None,
                },
            )
            .unwrap();
        assert!(
            matches!(&denied.decision, vcp_policy::Decision::Deny { origin, .. } if origin == "host-opaque-ceiling")
        );
        let effect = denied.effect().clone();
        assert!(host.dispatch_process(denied).is_err());
        assert!(!marker.exists());
        let state = host.snapshot().unwrap();
        let effect: Effect = state
            .record(Collection::Effect, effect.as_str(), &config.workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(effect.state, EffectState::Cancelled);
        assert!(effect.execution.is_none());
        let plans: Vec<_> = state
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
            .map(|r| r.decode::<ArtifactDescriptor>().unwrap())
            .filter(|d| d.spec.schema == "vcp-prepared-tool-v2")
            .collect();
        assert!(!plans.is_empty());
        for plan in plans {
            let bytes = host.read_artifact(plan.spec.id).unwrap();
            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                value["host_tool_denials"],
                serde_json::to_value(&config.host_tool_denials).unwrap()
            );
        }
        assert!(server.received_requests().await.unwrap().is_empty());
        owner.close().await.unwrap();
    }
}
