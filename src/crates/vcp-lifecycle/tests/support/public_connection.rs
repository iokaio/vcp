// SPDX-License-Identifier: Apache-2.0
use super::*;
use codex_extension_api::HostWorkAdmission;
use vcp_domain::controller::{Lease, Reason};
use vcp_engine::{command_handler::Access, query::Query};

fn access(host: &CanonicalHost, config: &Config, write: bool) -> Access {
    let workspace: Workspace = host
        .snapshot()
        .unwrap()
        .record(
            Collection::Workspace,
            config.workspace.as_str(),
            &config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    Access {
        actor: config.actor.clone(),
        workspace: config.workspace.clone(),
        session: config.session.clone(),
        authority: workspace.authority,
        read: true,
        write,
        bootstrap: false,
    }
}

fn lease(host: &CanonicalHost) -> Lease {
    host.snapshot()
        .unwrap()
        .records
        .values()
        .find(|row| row.value["document_type"] == "vcp_controller_lease_v1")
        .unwrap()
        .decode()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disconnect_releases_controller_without_closing_host_or_resuming_tasks() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let config = config(
            &temporary.path().join("canonical"),
            &workspace.canonicalize().unwrap(),
            backend,
        );
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let binding = task(&host, &config, config.root_task.clone(), None);
        assert!(host
            .public_connection(access(&host, &config, true))
            .is_err());
        host.command(
            Command::Transition {
                next: TaskState::Paused,
                reason: "select public mode".into(),
                verification: None,
            },
            Some(binding.scope.task.clone()),
            Revision::new(1),
        )
        .unwrap();
        let mut controller = host
            .public_connection(access(&host, &config, true))
            .unwrap();
        let observer = host
            .public_connection(access(&host, &config, false))
            .unwrap();
        controller.acquire(CommandId::new(), None).unwrap();
        assert!(controller.controller_token().is_ok());
        assert!(observer.controller_token().is_err());
        let other_observer = host
            .public_connection(access(&host, &config, false))
            .unwrap();
        assert!(other_observer
            .disconnect()
            .unwrap()
            .wait()
            .await
            .unwrap()
            .is_none());
        assert!(controller.controller_token().is_ok());
        let server = start_mock_server().await;
        let retained = test_codex().build_with_auto_env(&server).await.unwrap();
        let thread = host
            .lifecycle()
            .attach_root(retained.codex.clone())
            .unwrap();
        host.register(thread, binding.clone()).unwrap();
        let paused: Task = host
            .snapshot()
            .unwrap()
            .record(
                Collection::Task,
                binding.scope.task.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        host.resume(thread, paused.revision, paused.fingerprint)
            .unwrap();
        controller
            .disconnect()
            .unwrap()
            .wait()
            .await
            .unwrap()
            .unwrap();
        let released = lease(&host);
        assert!(released.holder.is_none());
        assert_eq!(released.reason, Reason::ConnectionLost);
        assert!(observer
            .query(Query::Session {
                session: config.session.clone()
            })
            .is_ok());
        let task: Task = host
            .snapshot()
            .unwrap()
            .record(
                Collection::Task,
                binding.scope.task.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(task.state, TaskState::Paused);
        assert!(host
            .command(
                Command::Transition {
                    next: TaskState::Running,
                    reason: "missing lease".into(),
                    verification: None
                },
                Some(task.scope.task.clone()),
                task.revision
            )
            .is_err());
        let mut replacement = host
            .public_connection(access(&host, &config, true))
            .unwrap();
        replacement
            .acquire(CommandId::new(), Some(released.revision))
            .unwrap();
        assert!(replacement.controller_token().unwrap().generation() > released.generation);
        let task: Task = host
            .snapshot()
            .unwrap()
            .record(
                Collection::Task,
                binding.scope.task.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(task.state, TaskState::Paused);
        assert!(host.lifecycle().inspect(thread).unwrap().local_hold);
        replacement.disconnect().unwrap().wait().await.unwrap();
        observer.disconnect().unwrap().wait().await.unwrap();
        owner.close().await.unwrap();
        retained.codex.shutdown_and_wait().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dropped_disconnect_waiter_keeps_owned_release_running() {
    let temporary = tempfile::tempdir().unwrap();
    let workspace = temporary.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let config = config(
        &temporary.path().join("canonical"),
        &workspace.canonicalize().unwrap(),
        BackendKind::Files,
    );
    let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
    let mut controller = host
        .public_connection(access(&host, &config, true))
        .unwrap();
    controller.acquire(CommandId::new(), None).unwrap();
    host.command(
        Command::SetWorkspaceTrust {
            trust: Trust::Trusted,
        },
        None,
        Revision::ZERO,
    )
    .unwrap();
    assert!(controller.controller_token().is_err());
    drop(controller.disconnect().unwrap());
    tokio::time::timeout(Duration::from_secs(5), async {
        while lease(&host).holder.is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(lease(&host).reason, Reason::ConnectionLost);
    owner.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disconnect_before_root_attachment_drains_startup_and_rejects_late_root() {
    let temporary = tempfile::tempdir().unwrap();
    let workspace = temporary.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    let config = config(
        &temporary.path().join("canonical"),
        &workspace,
        BackendKind::Files,
    );
    let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
    let mut controller = host
        .public_connection(access(&host, &config, true))
        .unwrap();
    controller.acquire(CommandId::new(), None).unwrap();
    host.lifecycle()
        .authorize_startup(&workspace, None)
        .unwrap();
    let startup = host.admit_startup(&workspace, None).unwrap();
    let release = controller.disconnect().unwrap();
    assert!(lease(&host).holder.is_some());
    assert!(host
        .lifecycle()
        .authorize_startup(&workspace, None)
        .is_err());
    assert!(host.admit_startup(&workspace, None).is_err());
    drop(startup);
    release.wait().await.unwrap().unwrap();
    let released = lease(&host);
    let mut replacement = host
        .public_connection(access(&host, &config, true))
        .unwrap();
    replacement
        .acquire(CommandId::new(), Some(released.revision))
        .unwrap();
    assert!(host
        .lifecycle()
        .authorize_startup(&workspace, None)
        .is_err());
    let server = start_mock_server().await;
    let late = test_codex().build_with_auto_env(&server).await.unwrap();
    assert!(host.lifecycle().attach_root(late.codex.clone()).is_err());
    assert!(host.lifecycle().root().unwrap().is_none());
    late.codex.shutdown_and_wait().await.unwrap();
    replacement.disconnect().unwrap().wait().await.unwrap();
    owner.close().await.unwrap();
}
