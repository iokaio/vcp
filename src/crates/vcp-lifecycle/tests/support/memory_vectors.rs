// SPDX-License-Identifier: Apache-2.0
//! Register as canonical_host support module with the P5-04 production hook.
use super::*;
use std::sync::atomic::AtomicBool;
use vcp_lifecycle::foundation::memory_vectors::{Request, Status};

async fn vector_fixture(
    backend: BackendKind,
) -> (
    tempfile::TempDir,
    Config,
    CanonicalHost,
    vcp_lifecycle::foundation::CanonicalOwner,
    TestCodex,
    codex_protocol::ThreadId,
) {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    let config = config(&temp.path().join("canonical"), &workspace, backend);
    let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
    host.command(
        Command::CreateTask {
            root: config.root_task.clone(),
            parent: None,
            fork_origin: None,
            objective: Objective {
                text: r#"{"memory_preference":{"key":"output","value":"concise"}}"#.into(),
                constraints: vec![],
                acceptance: vec!["retain preference".into()],
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
        Command::Transition {
            next: TaskState::Running,
            reason: "explicit fixture start".into(),
            verification: None,
        },
        Some(config.root_task.clone()),
        Revision::ZERO,
    )
    .unwrap();
    for _ in 0..8 {
        if host.maintain_memory().unwrap().caught_up {
            break;
        }
    }
    let server = start_mock_server().await;
    let mut registry = ExtensionRegistryBuilder::new();
    registry.turn_start_admission(Arc::new(host.clone()));
    registry.work_admission(Arc::new(host.clone()));
    let starter = host.clone();
    let cwd = workspace;
    let test = test_codex()
        .with_extensions(Arc::new(registry.build()))
        .with_auth(codex_login::CodexAuth::from_api_key(
            "synthetic-memory-fixture",
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
    host.register(
        thread,
        ThreadBinding {
            scope: Scope {
                workspace: config.workspace.clone(),
                session: config.session.clone(),
                task: config.root_task.clone(),
            },
            agent: AgentId::new(),
            role: RequestRole::Main,
        },
    )
    .unwrap();
    (temp, config, host, owner, test, thread)
}
fn request(root: &std::path::Path, assets: std::path::PathBuf, cancelled: bool) -> Request {
    Request {
        assets,
        private_root: root.into(),
        sources: vec![],
        chunker: vcp_memory::search_record::ChunkerSpec::default(),
        cancelled: Arc::new(AtomicBool::new(cancelled)),
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelled_and_paused_local_vectors_never_open_assets_or_create_output() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let (temp, config, host, owner, _test, thread) = vector_fixture(backend).await;
        let private = temp.path().join("not-created");
        let assets = temp.path().join("missing-assets");
        let before = host.snapshot().unwrap();
        assert_eq!(
            host.build_memory_vectors(thread, request(&private, assets.clone(), true))
                .await
                .unwrap()
                .status,
            Status::Cancelled
        );
        assert!(!private.exists());
        assert_eq!(host.snapshot().unwrap(), before);
        let root: Task = before
            .record(
                Collection::Task,
                config.root_task.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        host.command(
            Command::Transition {
                next: TaskState::Paused,
                reason: "explicit vector pause".into(),
                verification: None,
            },
            Some(config.root_task.clone()),
            root.revision,
        )
        .unwrap();
        let before = host.snapshot().unwrap();
        assert!(matches!(
            host.build_memory_vectors(thread, request(&private, assets, false))
                .await
                .unwrap()
                .status,
            Status::Deferred(_)
        ));
        assert!(!private.exists());
        assert_eq!(host.snapshot().unwrap(), before);
        owner.close().await.unwrap();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn missing_local_assets_are_failed_not_remote_or_ready() {
    let (temp, _config, host, owner, _test, thread) = vector_fixture(BackendKind::Files).await;
    let private = temp.path().join("derived");
    std::fs::create_dir(&private).unwrap();
    let outcome = host
        .build_memory_vectors(
            thread,
            request(&private, temp.path().join("missing-assets"), false),
        )
        .await
        .unwrap();
    assert!(matches!(outcome.status, Status::Failed(_)));
    assert!(outcome.built.is_none());
    let resources = outcome.resources.unwrap();
    assert!(resources.samples > 0);
    assert!(resources.observation_retained);
    assert_eq!(resources.sampled_temporary_disk_peak_bytes, 0);
    owner.close().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "explicit native CPU assets gate: VCP_MINILM_ASSETS required"]
async fn admitted_real_local_vectors_retain_resource_observation_and_reopen() {
    let assets =
        std::env::var_os("VCP_MINILM_ASSETS").expect("pinned local asset directory required");
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let (temp, config, host, owner, _test, thread) = vector_fixture(backend).await;
        let private = temp.path().join("derived");
        std::fs::create_dir(&private).unwrap();
        let outcome = host
            .build_memory_vectors(thread, request(&private, assets.clone().into(), false))
            .await
            .unwrap();
        assert_eq!(outcome.status, Status::Ready);
        let built = outcome.built.unwrap();
        let report = outcome.resources.unwrap();
        eprintln!(
            "local_vector_resource_report={}",
            serde_json::to_string(&report).unwrap()
        );
        assert!(report.observation_retained);
        assert!(report.samples >= 4);
        assert!(report.sampled_resident_peak_bytes > 0);
        assert!(report.sampled_private_commit_peak_bytes > 0);
        let reopened = vcp_memory::vector::Component::open(
            &built.directory.join("vectors.json"),
            &built.checksum,
            &config.workspace,
            &vcp_memory::embedding::Specification::qualified(),
            &|| false,
        )
        .unwrap();
        assert_eq!(reopened.rows().len(), built.rows);
        let snapshot = host.snapshot().unwrap();
        let observed: LocalResources = snapshot
            .record(
                Collection::LocalResources,
                report.observation.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(observed.cpu_millis.get(), report.process_cpu_millis);
        assert_eq!(observed.peak_ram.get(), report.sampled_resident_peak_bytes);
        owner.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "explicit in-flight native CPU gate: VCP_MINILM_ASSETS required"]
async fn inflight_local_vectors_cancel_and_pause_without_blocking_owner() {
    use std::sync::atomic::Ordering;
    let assets =
        std::env::var_os("VCP_MINILM_ASSETS").expect("pinned local asset directory required");
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for pause_root in [false, true] {
            let (temp, config, host, owner, _test, thread) = vector_fixture(backend).await;
            let private = temp.path().join("inflight-derived");
            std::fs::create_dir(&private).unwrap();
            let input = request(&private, assets.clone().into(), false);
            let cancelled = input.cancelled.clone();
            let worker_host = host.clone();
            let build =
                tokio::spawn(async move { worker_host.build_memory_vectors(thread, input).await });
            // This is the actual exclusive private directory creation after
            // canonical admission and the process-shared resource permit.
            // No fixture-only production barrier delays model execution.
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                if std::fs::read_dir(&private).unwrap().next().is_some() {
                    break;
                }
                assert!(
                    !build.is_finished(),
                    "work ended before actual admission marker"
                );
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "local admission marker timeout"
                );
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            assert!(
                !build.is_finished(),
                "fixture must interrupt in-flight work"
            );
            let observing = host.clone();
            let snapshot = tokio::time::timeout(
                Duration::from_secs(2),
                tokio::task::spawn_blocking(move || observing.snapshot()),
            )
            .await
            .expect("canonical owner blocked behind local CPU work")
            .unwrap()
            .unwrap();
            let root: Task = snapshot
                .record(
                    Collection::Task,
                    config.root_task.as_str(),
                    &config.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(root.state, TaskState::Running);
            if pause_root {
                let pausing = host.clone();
                let task = config.root_task.clone();
                tokio::time::timeout(
                    Duration::from_secs(2),
                    tokio::task::spawn_blocking(move || {
                        pausing.command(
                            Command::Transition {
                                next: TaskState::Paused,
                                reason: "explicit pause during admitted local vector work".into(),
                                verification: None,
                            },
                            Some(task),
                            root.revision,
                        )
                    }),
                )
                .await
                .expect("pause blocked behind native vector computation")
                .unwrap()
                .unwrap();
            } else {
                cancelled.store(true, Ordering::Release);
            }
            let outcome = tokio::time::timeout(Duration::from_secs(30), build)
                .await
                .expect("nonpreemptible bounded unit failed to finish")
                .unwrap()
                .unwrap();
            assert_eq!(outcome.status, Status::Cancelled);
            assert!(
                outcome.built.is_none(),
                "cancelled work cannot advertise a ready component"
            );
            let report = outcome
                .resources
                .expect("admitted work must retain observed resources");
            eprintln!(
                "inflight_local_vector_resource_report={}",
                serde_json::to_string(&report).unwrap()
            );
            assert!(report.observation_retained);
            assert!(report.samples > 0);
            for directory in std::fs::read_dir(&private).unwrap() {
                assert!(
                    !directory.unwrap().path().join("vectors.json").exists(),
                    "cancellation during model startup must precede vector persistence"
                );
            }
            let current: Task = host
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
            assert_eq!(
                current.state,
                if pause_root {
                    TaskState::Paused
                } else {
                    TaskState::Running
                }
            );
            if !pause_root {
                // Reaching MissingAsset and retaining its observation proves the
                // earlier model/graph permit was released, rather than yielding
                // another Deferred("capacity occupied") response.
                let retried = host
                    .build_memory_vectors(
                        thread,
                        request(&private, temp.path().join("missing-assets"), false),
                    )
                    .await
                    .unwrap();
                assert!(matches!(retried.status, Status::Failed(_)));
                assert!(retried.resources.unwrap().observation_retained);
                assert!(retried.built.is_none());
            }
            owner.close().await.unwrap();
        }
    }
}
