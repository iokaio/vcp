// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{collections::BTreeSet, fs};
use vcp_domain::{agents::ChildMode, policy::*};
use vcp_lifecycle::foundation::{DelegationRequest, HelperTemplate};
use vcp_repository::{worktree::Snapshotter, Root, RootIdentity};

fn policy(host: &CanonicalHost, config: &Config, mode: Autonomy, include_root: bool) {
    let mut policy =
        vcp_engine::policy::current(&host.snapshot().unwrap(), &config.workspace).unwrap();
    let expected = Revision::new(policy.revision.get());
    policy.revision = policy.revision.next().unwrap();
    policy.mode = mode;
    policy.workspace_roots = if include_root {
        BTreeSet::from([RootId::parse(config.workspace.as_str()).unwrap()])
    } else {
        BTreeSet::new()
    };
    policy.automatic_effects = if mode == Autonomy::Workspace {
        BTreeSet::new()
    } else {
        BTreeSet::from([EffectClass::Read, EffectClass::Write])
    };
    host.command(Command::SetPolicy { policy }, None, expected)
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn child_cleanup_retains_durable_intent_and_results_and_reconciles_only_original_root() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let copies = temp.path().join("children");
        fs::create_dir(&workspace).unwrap();
        fs::create_dir(&copies).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        fs::write(workspace.join("file.txt"), b"preserved parent\n").unwrap();
        let config = config(&temp.path().join("canonical"), &workspace, backend);
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
                    mode: Autonomy::Autonomous,
                    denials: vec![],
                    workspace_roots: BTreeSet::from([
                        RootId::parse(config.workspace.as_str()).unwrap()
                    ]),
                    automatic_effects: BTreeSet::from([EffectClass::Read, EffectClass::Write]),
                    timeout_ceiling_ms: Units::new(30000),
                    output_ceiling_bytes: ByteCount::new(1024 * 1024),
                },
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        host.initialize_root_budget().unwrap();
        let (snapshot, raw) = provider_snapshot();
        host.configure_provider(snapshot, raw).unwrap();
        let server = start_mock_server().await;
        let starter = host.clone();
        let cwd = workspace.clone();
        let test = test_codex()
            .with_auth(codex_login::CodexAuth::from_api_key(
                "synthetic-cleanup-fixture",
            ))
            .with_allowed_tools(AllowedTools(vec![]))
            .with_config(move |c| {
                c.cwd = cwd.clone().try_into().unwrap();
                c.model = Some("gpt-5.1".into());
                configure_fixture_provider(c);
                starter
                    .lifecycle()
                    .authorize_startup(c.cwd.as_path(), None)
                    .unwrap();
            })
            .build_with_auto_env(&server)
            .await
            .unwrap();
        let parent = host.lifecycle().attach_root(test.codex.clone()).unwrap();
        host.register(parent, binding.clone()).unwrap();
        let disposable = Root::open(
            RootIdentity {
                workspace: config.workspace.clone(),
                root: RootId::new(),
                repository: config.binding.repository.clone(),
                worktree: "cleanup-copies".into(),
                binding: config.binding.revision,
            },
            &copies,
        )
        .unwrap();
        let snapshotter = Snapshotter::new(
            std::env::var_os("VCP_TEST_GIT").unwrap().into(),
            ["SystemRoot", "WINDIR", "PATH", "TEMP", "TMP"]
                .into_iter()
                .filter_map(|k| std::env::var_os(k).map(|v| (k.into(), v)))
                .collect(),
            Duration::from_secs(20),
            8 * 1024 * 1024,
        )
        .unwrap();
        let child = host
            .delegate_child(
                parent,
                DelegationRequest {
                    role: "review".into(),
                    read_paths: BTreeSet::from([String::new()]),
                    helper: Some(HelperTemplate {
                        name: "review".into(),
                        revision: 1,
                    }),
                    objective: "Review retained source".into(),
                    acceptance: vec!["Report source evidence".into()],
                    mode: ChildMode::ReadOnly,
                    write_paths: BTreeSet::new(),
                    untracked_inputs: BTreeSet::from(["file.txt".into()]),
                    allocation: Micros::new(400),
                    deadline: Timestamp::new(u64::MAX),
                    required_checks: vec![],
                },
                &snapshotter,
                &disposable,
            )
            .await
            .unwrap();
        assert!(host
            .prepare_child_cleanup(parent, child.clone(), &snapshotter, false)
            .await
            .is_err());
        let child_task: Task = host
            .snapshot()
            .unwrap()
            .record(Collection::Task, child.as_str(), &config.workspace)
            .unwrap()
            .decode()
            .unwrap();
        host.stop(
            host.control_envelope(
                CommandId::new(),
                child.clone(),
                child_task.revision,
                Command::Transition {
                    next: TaskState::Cancelled,
                    reason: "reject unused helper".into(),
                    verification: None,
                },
            )
            .unwrap(),
        )
        .unwrap();
        let path = copies.join(child.as_str());
        for (mode, include_root) in [
            (Autonomy::Ask, true),
            (Autonomy::Autonomous, false),
            (Autonomy::Plan, true),
        ] {
            policy(&host, &config, mode, include_root);
            assert!(host
                .prepare_child_cleanup(parent, child.clone(), &snapshotter, false)
                .await
                .is_err());
            assert!(vcp_engine::agents::graph(
                &host.snapshot().unwrap(),
                &binding.scope,
                &config.root_task
            )
            .unwrap()
            .unwrap()
            .cleanup
            .is_empty());
            assert!(path.exists());
        }
        policy(&host, &config, Autonomy::Workspace, true);
        fs::write(path.join("file.txt"), b"rejected child result\n").unwrap();
        assert!(host
            .prepare_child_cleanup(parent, child.clone(), &snapshotter, false)
            .await
            .is_err());
        let stale_authority = host
            .prepare_child_cleanup(parent, child.clone(), &snapshotter, true)
            .await
            .unwrap();
        policy(&host, &config, Autonomy::Ask, true);
        assert!(host.apply_child_cleanup(stale_authority).is_err());
        assert!(vcp_engine::agents::graph(
            &host.snapshot().unwrap(),
            &binding.scope,
            &config.root_task
        )
        .unwrap()
        .unwrap()
        .cleanup
        .is_empty());
        policy(&host, &config, Autonomy::Workspace, true);
        let preview = host
            .prepare_child_cleanup(parent, child.clone(), &snapshotter, true)
            .await
            .unwrap();
        fs::write(path.join("file.txt"), b"late human edit\n").unwrap();
        assert!(host.apply_child_cleanup(preview).is_err());
        let state = host.snapshot().unwrap();
        let graph = vcp_engine::agents::graph(&state, &binding.scope, &config.root_task)
            .unwrap()
            .unwrap();
        let cleanup = graph.cleanup[&child].clone();
        assert!(cleanup.receipt.is_none());
        assert_eq!(cleanup.diagnostics.len(), 1);
        assert!(!cleanup.diagnostics[0].reason.is_empty());
        assert_eq!(
            fs::read(path.join("file.txt")).unwrap(),
            b"late human edit\n"
        );
        let graph_record = state
            .record(
                Collection::Projection,
                &vcp_domain::agents::graph_id(&config.root_task),
                &config.workspace,
            )
            .unwrap();
        let refs = graph_record.required_references().unwrap();
        for id in [&cleanup.intent, &cleanup.retained_result] {
            assert!(refs.contains(&vcp_store::contract::key(Collection::Artifact, id.as_str())));
        }
        assert!(refs.contains(&vcp_store::contract::key(
            Collection::Artifact,
            cleanup.diagnostics[0].artifact.as_str()
        )));
        assert!(host
            .prepare_child_cleanup(parent, child.clone(), &snapshotter, true)
            .await
            .is_err());
        assert!(host.reconcile_child_cleanup(parent, child.clone()).is_err());
        fs::write(path.join("file.txt"), b"rejected child result\n").unwrap();
        #[cfg(feature = "qualification")]
        {
            assert!(host
                .qualification_cleanup_before_claim(parent, child.clone(), || policy(
                    &host,
                    &config,
                    Autonomy::Ask,
                    true
                ))
                .is_err());
            assert!(path.join("file.txt").exists());
            policy(&host, &config, Autonomy::Workspace, true);
        }
        let receipt = host.reconcile_child_cleanup(parent, child.clone()).unwrap();
        assert!(receipt.removed);
        assert!(!path.exists());
        assert_eq!(
            fs::read(workspace.join("file.txt")).unwrap(),
            b"preserved parent\n"
        );
        let state = host.snapshot().unwrap();
        let graph = vcp_engine::agents::graph(&state, &binding.scope, &config.root_task)
            .unwrap()
            .unwrap();
        assert!(graph.cleanup[&child].receipt.is_some());
        assert!(host
            .prepare_child_start(parent, child.clone(), &snapshotter)
            .await
            .is_err());
        fs::create_dir(&path).unwrap();
        fs::write(path.join("unrelated.txt"), b"new human root").unwrap();
        host.reconcile_child_cleanup(parent, child).unwrap();
        assert_eq!(
            fs::read(path.join("unrelated.txt")).unwrap(),
            b"new human root"
        );
        assert!(server.received_requests().await.unwrap().is_empty());
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        drop(test);
        drop(host);
        let (reopened, owner) = CanonicalHost::open(config.clone()).unwrap();
        let restored = reopened.snapshot().unwrap();
        let graph = vcp_engine::agents::graph(&restored, &binding.scope, &config.root_task)
            .unwrap()
            .unwrap();
        let retained = graph.cleanup.values().next().unwrap();
        assert!(retained.receipt.is_some());
        assert!(!retained.diagnostics.is_empty());
        assert_eq!(retained.intent, cleanup.intent);
        let result: vcp_repository::dirty_snapshot::WorkspaceSnapshot = serde_json::from_slice(
            &reopened
                .read_artifact(retained.retained_result.clone())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            result
                .files
                .iter()
                .find(|f| f.path == "file.txt")
                .unwrap()
                .working
                .as_deref(),
            Some(b"rejected child result\n".as_slice())
        );
        assert_eq!(
            fs::read(path.join("unrelated.txt")).unwrap(),
            b"new human root"
        );
        assert!(server.received_requests().await.unwrap().is_empty());
        owner.close().await.unwrap();
    }
}
