// SPDX-License-Identifier: Apache-2.0
//! Native cleanup recovery after the disposable root is removed but before
//! the canonical receipt publication.
use super::*;
use std::{
    collections::BTreeSet, fs, path::PathBuf, process::Stdio, sync::mpsc::sync_channel,
    time::Instant,
};
use vcp_domain::{agents::ChildMode, policy::*};
use vcp_lifecycle::foundation::{DelegationRequest, HelperTemplate};
use vcp_repository::{worktree::Snapshotter, Root, RootIdentity};

fn allow_workspace_cleanup(host: &CanonicalHost, config: &Config) {
    let mut policy =
        vcp_engine::policy::current(&host.snapshot().unwrap(), &config.workspace).unwrap();
    let expected = Revision::new(policy.revision.get());
    policy.revision = policy.revision.next().unwrap();
    policy.mode = Autonomy::Workspace;
    policy.workspace_roots = BTreeSet::from([RootId::parse(config.workspace.as_str()).unwrap()]);
    policy.automatic_effects = BTreeSet::new();
    host.command(Command::SetPolicy { policy }, None, expected)
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn child_cleanup_receipt_hook_error_retains_intent_and_reconciles_both_stores() {
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
                "synthetic-cleanup-receipt-fault",
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
                    reason: "cleanup receipt fault fixture".into(),
                    verification: None,
                },
            )
            .unwrap(),
        )
        .unwrap();
        allow_workspace_cleanup(&host, &config);
        let preview = host
            .prepare_child_cleanup(parent, child.clone(), &snapshotter, false)
            .await
            .unwrap();
        let path = copies.join(child.as_str());
        let (arrived_tx, arrived_rx) = sync_channel(0);
        let (release_tx, release_rx) = sync_channel(0);
        let barrier_waiter = std::thread::spawn(move || {
            if arrived_rx.recv_timeout(Duration::from_secs(5)).is_ok() {
                let _ = release_tx.send(());
            }
        });
        let error = host
            .qualification_apply_child_cleanup(preview, move || {
                arrived_tx
                    .send(())
                    .map_err(|_| "receipt publication barrier observer exited".to_owned())?;
                release_rx
                    .recv_timeout(Duration::from_secs(5))
                    .map_err(|_| "receipt publication barrier timed out".to_owned())?;
                Err("deterministic process interruption before receipt publication".into())
            })
            .unwrap_err();
        barrier_waiter.join().unwrap();
        assert!(error.contains("receipt publication interrupted"), "{error}");
        assert!(
            !path.exists(),
            "native removal must precede the interruption"
        );
        let state = host.snapshot().unwrap();
        let graph = vcp_engine::agents::graph(&state, &binding.scope, &config.root_task)
            .unwrap()
            .unwrap();
        let cleanup = graph.cleanup.get(&child).unwrap();
        assert!(cleanup.receipt.is_none());
        let intent = cleanup.intent.clone();

        owner.close().await.unwrap();
        drop(host);
        let (reopened, reopened_owner) = CanonicalHost::open(config.clone()).unwrap();
        let (snapshot, raw) = provider_snapshot();
        reopened.configure_provider(snapshot, raw).unwrap();
        let reopened_parent = reopened
            .lifecycle()
            .attach_root(test.codex.clone())
            .unwrap();
        reopened.register(reopened_parent, binding.clone()).unwrap();
        resume_parent(&reopened, reopened_parent, &config);
        let receipt = reopened
            .reconcile_child_cleanup(reopened_parent, child.clone())
            .unwrap();
        assert!(receipt.removed);
        assert!(!path.exists());
        let state = reopened.snapshot().unwrap();
        let graph = vcp_engine::agents::graph(&state, &binding.scope, &config.root_task)
            .unwrap()
            .unwrap();
        let cleanup = graph.cleanup.get(&child).unwrap();
        assert_eq!(cleanup.intent, intent);
        assert!(cleanup.receipt.is_some());
        reopened_owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        drop(test);
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}

fn retained_parent_binding(config: &Config) -> ThreadBinding {
    ThreadBinding {
        scope: Scope {
            workspace: config.workspace.clone(),
            session: config.session.clone(),
            task: config.root_task.clone(),
        },
        agent: AgentId::new(),
        role: RequestRole::Main,
    }
}

fn resume_parent(host: &CanonicalHost, parent: codex_protocol::ThreadId, config: &Config) {
    let task: Task = host
        .snapshot()
        .unwrap()
        .record(
            Collection::Task,
            config.root_task.as_str(),
            &config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    if task.state != TaskState::Running {
        host.resume(parent, task.revision, task.fingerprint)
            .unwrap();
    }
}

fn cleanup_backend(name: &str) -> BackendKind {
    match name {
        "files" => BackendKind::Files,
        "sqlite" => BackendKind::Sqlite,
        other => panic!("unknown cleanup child backend: {other}"),
    }
}

#[test]
#[ignore = "supervisor launches this test as a real child process"]
fn cleanup_receipt_fault_process_child() {
    let root = PathBuf::from(
        std::env::var_os("VCP_CLEANUP_RECEIPT_CHILD_ROOT")
            .expect("cleanup child root environment is required"),
    );
    let backend = cleanup_backend(
        &std::env::var("VCP_CLEANUP_RECEIPT_CHILD_BACKEND")
            .expect("cleanup child backend environment is required"),
    );
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(run_cleanup_receipt_fault_child(root, backend));
}

async fn run_cleanup_receipt_fault_child(root: PathBuf, backend: BackendKind) {
    let workspace = root.join("workspace").canonicalize().unwrap();
    let copies = root.join("children");
    let config = config(&root.join("canonical"), &workspace, backend);
    let (host, _owner) = CanonicalHost::open(config.clone()).unwrap();
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
                workspace_roots: BTreeSet::from(
                    [RootId::parse(config.workspace.as_str()).unwrap()],
                ),
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
            "synthetic-cleanup-receipt-process-fault",
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
                reason: "cleanup receipt real process fault fixture".into(),
                verification: None,
            },
        )
        .unwrap(),
    )
    .unwrap();
    allow_workspace_cleanup(&host, &config);
    let preview = host
        .prepare_child_cleanup(parent, child.clone(), &snapshotter, false)
        .await
        .unwrap();
    let barrier = root.join("cleanup-receipt-barrier.json");
    host.qualification_apply_child_cleanup(preview, move || {
        let payload = serde_json::json!({
            "phase": "after_native_removal_before_receipt_publication",
            "child": child.as_str(),
        });
        let bytes = serde_json::to_vec(&payload).unwrap();
        let temporary = barrier.with_extension("json.tmp");
        let mut file = fs::File::create(&temporary).unwrap();
        std::io::Write::write_all(&mut file, &bytes).unwrap();
        file.sync_all().unwrap();
        fs::rename(temporary, barrier).unwrap();
        loop {
            std::thread::sleep(Duration::from_millis(100));
        }
    })
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn child_cleanup_receipt_publication_process_kill_reopens_and_reconciles_both_stores() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let copies = temp.path().join("children");
        fs::create_dir(&workspace).unwrap();
        fs::create_dir(&copies).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        fs::write(workspace.join("file.txt"), b"preserved parent\n").unwrap();
        let config = config(&temp.path().join("canonical"), &workspace, backend);
        let child_backend = if backend == BackendKind::Files {
            "files"
        } else {
            "sqlite"
        };
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "child_cleanup_receipt_fault::cleanup_receipt_fault_process_child",
                "--ignored",
                "--nocapture",
            ])
            .env("VCP_CLEANUP_RECEIPT_CHILD_ROOT", temp.path())
            .env("VCP_CLEANUP_RECEIPT_CHILD_BACKEND", child_backend)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let barrier = temp.path().join("cleanup-receipt-barrier.json");
        let deadline = Instant::now() + Duration::from_secs(60);
        while !barrier.exists() {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("cleanup child exited before post-removal barrier: {status}");
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("cleanup receipt publication barrier timed out");
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let marker: serde_json::Value =
            serde_json::from_slice(&fs::read(&barrier).unwrap()).unwrap();
        assert_eq!(
            marker["phase"],
            "after_native_removal_before_receipt_publication"
        );
        let child_id = TaskId::parse(marker["child"].as_str().unwrap()).unwrap();
        assert!(!copies.join(child_id.as_str()).exists());
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success());

        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let (snapshot, raw) = provider_snapshot();
        host.configure_provider(snapshot, raw).unwrap();
        let binding = retained_parent_binding(&config);
        let server = start_mock_server().await;
        let starter = host.clone();
        let cwd = workspace.clone();
        let test = test_codex()
            .with_auth(codex_login::CodexAuth::from_api_key(
                "synthetic-cleanup-receipt-reopen",
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
        host.register(parent, binding).unwrap();
        resume_parent(&host, parent, &config);
        let receipt = host
            .reconcile_child_cleanup(parent, child_id.clone())
            .unwrap();
        assert!(receipt.removed);
        assert!(!copies.join(child_id.as_str()).exists());
        let state = host.snapshot().unwrap();
        let graph = vcp_engine::agents::graph(
            &state,
            &retained_parent_binding(&config).scope,
            &config.root_task,
        )
        .unwrap()
        .unwrap();
        assert!(graph.cleanup[&child_id].receipt.is_some());
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        drop(test);
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}
