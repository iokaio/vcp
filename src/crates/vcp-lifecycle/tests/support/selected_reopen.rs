// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_engine::{Access, Engine, HostFacts};
use vcp_protocol::command::CommandEnvelope;
use vcp_store::Store;

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
