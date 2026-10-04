// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_engine::{Access, Engine, HostFacts};
use vcp_protocol::command::CommandEnvelope;
use vcp_store::Store;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn selected_owned_handoff_preserves_lock_configuration_and_runtime_independence() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for rewritten in [false, true] {
            let temporary = tempfile::tempdir().unwrap();
            let workspace = temporary.path().join("workspace");
            std::fs::create_dir(&workspace).unwrap();
            let workspace = workspace.canonicalize().unwrap();
            let mut config = config(&temporary.path().join("canonical"), &workspace, backend);
            config.artifact_limit = ByteCount::new(vcp_store::artifact::DEFAULT_ARTIFACT_LIMIT / 2);
            let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
            host.command(
                Command::CreateTask {
                    root: config.root_task.clone(),
                    parent: None,
                    fork_origin: None,
                    objective: Objective {
                        text: "Resume through the same store owner".into(),
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
                    editing: false,
                    required_checks: vec![],
                },
                Some(config.root_task.clone()),
                Revision::ZERO,
            )
            .unwrap();
            owner.close().await.unwrap();
            drop(host);

            let open = |config: Config| {
                std::thread::spawn(move || {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();
                    runtime
                        .block_on(Store::open_with_artifact_limit(
                            &config.canonical_root,
                            config.backend,
                            &[],
                            config.artifact_limit.get(),
                        ))
                        .unwrap()
                    // The originating runtime is dropped before the Store moves.
                })
                .join()
                .unwrap()
            };
            let mut store = open(config.clone());
            if rewritten {
                store
                    .rewrite_base(store.state().clone(), &[])
                    .await
                    .unwrap();
                assert_ne!(store.root(), store.canonical_anchor());
            }
            let before = store.state().clone();
            let selected: Task = before
                .record(
                    Collection::Task,
                    config.root_task.as_str(),
                    &config.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            store.close().await.unwrap();

            for mismatch in [
                "revision",
                "session",
                "task",
                "workspace",
                "root",
                "backend",
                "artifact_limit",
            ] {
                let store = open(config.clone());
                let mut wrong = config.clone();
                let mut expected = selected.revision;
                match mismatch {
                    "revision" => expected = expected.next().unwrap(),
                    "session" => wrong.session = SessionId::new(),
                    "task" => wrong.root_task = TaskId::new(),
                    "workspace" => wrong.workspace = WorkspaceId::new(),
                    "root" => wrong.canonical_root = workspace.clone(),
                    "backend" => {
                        wrong.backend = if backend == BackendKind::Files {
                            BackendKind::Sqlite
                        } else {
                            BackendKind::Files
                        }
                    }
                    "artifact_limit" => {
                        wrong.artifact_limit = ByteCount::new(config.artifact_limit.get() / 2)
                    }
                    _ => unreachable!(),
                }
                assert!(
                    CanonicalHost::open_owned_selected(wrong, store, expected).is_err(),
                    "{backend:?}/{rewritten}/{mismatch}"
                );
                let reopened = open(config.clone());
                assert_eq!(
                    reopened.state(),
                    &before,
                    "invalid selection must not recover or mutate"
                );
                reopened.close().await.unwrap();
            }

            let store = open(config.clone());
            let initial_diagnostics = store.diagnostics().clone();
            assert!(Store::open(&config.canonical_root, backend, &[])
                .await
                .is_err());
            let (host, owner) =
                CanonicalHost::open_owned_selected(config.clone(), store, selected.revision)
                    .unwrap();
            let current = host.snapshot().unwrap();
            let recovered: Task = current
                .record(
                    Collection::Task,
                    config.root_task.as_str(),
                    &config.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(recovered.state, TaskState::Paused);
            assert_eq!(
                current, before,
                "read-only handoff of paused work does not mutate"
            );
            host.command(
                Command::CreateSession {
                    id: SessionId::new(),
                    fork_through: None,
                },
                None,
                Revision::ZERO,
            )
            .unwrap();
            let current = host.snapshot().unwrap();
            assert!(
                current.watermark > before.watermark,
                "normal commands append using the transferred connection"
            );
            for collection in [Collection::Attempt, Collection::Effect] {
                assert_eq!(
                    current
                        .records
                        .values()
                        .filter(|row| row.collection == collection)
                        .count(),
                    before
                        .records
                        .values()
                        .filter(|row| row.collection == collection)
                        .count(),
                    "handoff never dispatches"
                );
            }
            let diagnostics = host.store_diagnostics().unwrap();
            assert_eq!(diagnostics.open, initial_diagnostics.open);
            assert_eq!(diagnostics.replay, initial_diagnostics.replay);
            assert_eq!(
                diagnostics.replayed_commits,
                initial_diagnostics.replayed_commits
            );
            assert!(CanonicalHost::open(config.clone()).is_err());
            assert!(Store::open(&config.canonical_root, backend, &[])
                .await
                .is_err());
            owner.close().await.unwrap();
            drop(host);
            let final_store = open(config.clone());
            assert_eq!(final_store.state(), &current);
            final_store.close().await.unwrap();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn selected_reopen_checks_revision_before_recovery_and_retains_store_lock() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let config = config(&temporary.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        owner.close().await.unwrap();
        drop(host);

        // Persist pending work through the canonical engine without a live host
        // lease. A valid reopen must recover it to paused; a stale selection must
        // fail before that recovery can change either the task or journal.
        let store = Store::open(&config.canonical_root, backend, &[])
            .await
            .unwrap();
        let mut engine = Engine::new(store).unwrap();
        let current: Workspace = engine
            .store()
            .state()
            .record(
                Collection::Workspace,
                config.workspace.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let access = Access {
            actor: config.actor.clone(),
            workspace: config.workspace.clone(),
            session: config.session.clone(),
            authority: current.authority,
            read: true,
            write: true,
            bootstrap: true,
        };
        let command = CommandEnvelope {
            version: 1,
            id: CommandId::new(),
            workspace: config.workspace.clone(),
            session: config.session.clone(),
            task: Some(config.root_task.clone()),
            caller: config.actor.clone(),
            controller: engine.controller().clone(),
            owner_epoch: engine.owner_epoch(),
            expected: Revision::ZERO,
            steering: SteeringRevision::ZERO,
            payload: Command::CreateTask {
                root: config.root_task.clone(),
                parent: None,
                fork_origin: None,
                objective: Objective {
                    text: "Retain pending work until explicit continuation".into(),
                    constraints: vec![],
                    acceptance: vec!["selection checked before recovery".into()],
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
        };
        engine
            .handle(command, &access, &HostFacts::inspect(Timestamp::new(1)))
            .await
            .unwrap();
        let before = engine.store().state().clone();
        let selected: Task = before
            .record(
                Collection::Task,
                config.root_task.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(selected.state, TaskState::Pending);
        engine.into_store().close().await.unwrap();

        let error = match CanonicalHost::open_selected(
            config.clone(),
            Some(selected.revision.next().unwrap()),
        ) {
            Err(error) => error,
            Ok(_) => panic!("stale selection unexpectedly acquired an owner"),
        };
        assert!(error.contains("task changed since selection"), "{error}");
        let store = Store::open(&config.canonical_root, backend, &[])
            .await
            .unwrap();
        assert_eq!(store.state().watermark, before.watermark);
        assert_eq!(
            store
                .state()
                .record(
                    Collection::Task,
                    config.root_task.as_str(),
                    &config.workspace
                )
                .unwrap(),
            before
                .record(
                    Collection::Task,
                    config.root_task.as_str(),
                    &config.workspace
                )
                .unwrap()
        );
        store.close().await.unwrap();

        let (host, owner) =
            CanonicalHost::open_selected(config.clone(), Some(selected.revision)).unwrap();
        let recovered = host.snapshot().unwrap();
        let task: Task = recovered
            .record(
                Collection::Task,
                config.root_task.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(task.state, TaskState::Paused);
        assert!(recovered.watermark > before.watermark);
        assert!(Store::open(&config.canonical_root, backend, &[])
            .await
            .is_err());
        owner.close().await.unwrap();
        drop(host);
        Store::open(&config.canonical_root, backend, &[])
            .await
            .unwrap()
            .close()
            .await
            .unwrap();
    }
}
