// SPDX-License-Identifier: Apache-2.0
use super::*;
use codex_extension_api::TurnStartAdmission;
use std::{future::Future, task::Poll};
use vcp_engine::{rpc::RpcHost, Access};
use vcp_protocol::methods::{self, Call, ResultValue};

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
fn id(value: &str) -> methods::Id {
    value.to_owned().try_into().unwrap()
}
fn scope(config: &Config) -> methods::Scope {
    methods::Scope {
        workspace: id(config.workspace.as_str()),
        session: id(config.session.as_str()),
    }
}
fn acquire(config: &Config, command: &str, revision: Option<u64>) -> Call {
    Call::ControllerAcquire(methods::ControllerAcquire {
        scope: scope(config),
        command_id: id(command),
        expected_revision: revision.map(Into::into),
    })
}
fn release(config: &Config, command: &str, revision: u64, generation: u64) -> Call {
    Call::ControllerRelease(methods::ControllerRelease {
        scope: scope(config),
        command_id: id(command),
        expected_revision: revision.into(),
        generation: generation.into(),
    })
}
async fn view(
    connection: &mut vcp_lifecycle::foundation::PublicConnection,
    config: &Config,
    access: &Access,
) -> methods::ControllerView {
    match connection
        .call(
            Call::ControllerRead(methods::ControllerRead {
                scope: scope(config),
            }),
            access,
        )
        .await
        .unwrap()
    {
        ResultValue::Controller(view) => view,
        result => panic!("unexpected view {result:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn public_controller_release_is_owned_replayable_and_keeps_observers_usable() {
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
        host.command(
            Command::Transition {
                next: TaskState::Paused,
                reason: "select public owner".into(),
                verification: None,
            },
            Some(binding.scope.task.clone()),
            Revision::new(1),
        )
        .unwrap();
        let current = access(&host, &config, true);
        let readonly = access(&host, &config, false);
        let mut controller = host.public_connection(current.clone()).unwrap();
        let mut observer = host.public_connection(readonly.clone()).unwrap();
        assert_eq!(
            view(&mut observer, &config, &readonly).await.ownership,
            methods::ControllerOwnership::Unclaimed
        );
        assert!(observer
            .call(acquire(&config, "observer", None), &readonly)
            .await
            .is_err());
        let first = acquire(&config, "first", None);
        let accepted = controller.call(first.clone(), &current).await.unwrap();
        assert_eq!(
            controller.call(first.clone(), &current).await.unwrap(),
            accepted
        );
        assert_eq!(
            view(&mut controller, &config, &current).await.ownership,
            methods::ControllerOwnership::ThisConnection
        );
        assert_eq!(
            view(&mut observer, &config, &readonly).await.ownership,
            methods::ControllerOwnership::OtherConnection
        );
        let server = start_mock_server().await;
        let retained = test_codex().build_with_auto_env(&server).await.unwrap();
        let thread = host
            .lifecycle()
            .attach_root(retained.codex.clone())
            .unwrap();
        let relinquish = release(&config, "release", 0, 1);
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
        let extra_observer = host.public_connection(readonly.clone()).unwrap();
        let before_observer_loss =
            format!("{:?}", host.lifecycle().inspect(thread).unwrap().revision);
        extra_observer.loss_signal().invalidate();
        assert!(!host.lifecycle().inspect(thread).unwrap().local_hold);
        assert_eq!(
            before_observer_loss,
            format!("{:?}", host.lifecycle().inspect(thread).unwrap().revision)
        );
        assert!(controller.controller_token().is_ok());
        extra_observer.disconnect().unwrap().wait().await.unwrap();
        let receipt = controller.call(relinquish.clone(), &current).await.unwrap();
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
        assert_eq!(paused.state, TaskState::Paused);
        let held = format!("{:?}", host.lifecycle().inspect(thread).unwrap().revision);
        assert_eq!(
            controller.call(relinquish.clone(), &current).await.unwrap(),
            receipt
        );
        assert_eq!(
            held,
            format!("{:?}", host.lifecycle().inspect(thread).unwrap().revision)
        );
        assert!(controller
            .call(release(&config, "release", 0, 2), &current)
            .await
            .is_err());
        assert_eq!(
            view(&mut controller, &config, &current).await.ownership,
            methods::ControllerOwnership::Released
        );
        assert!(controller.controller_token().is_err());
        assert_eq!(controller.call(first, &current).await.unwrap(), accepted);
        assert!(
            controller.controller_token().is_err(),
            "historical acquisition cannot restore a token"
        );
        assert!(observer.call(relinquish, &readonly).await.is_err());
        let stale_signal = controller.loss_signal();
        let mut replacement = host.public_connection(current.clone()).unwrap();
        replacement
            .call(acquire(&config, "second", Some(1)), &current)
            .await
            .unwrap();
        assert_eq!(
            replacement.controller_token().unwrap().generation(),
            Revision::new(2)
        );
        assert!(
            host.lifecycle().inspect(thread).unwrap().local_hold,
            "acquisition never resumes"
        );
        let before_stale_loss = format!("{:?}", host.lifecycle().inspect(thread).unwrap().revision);
        stale_signal.invalidate();
        stale_signal.invalidate();
        assert_eq!(
            before_stale_loss,
            format!("{:?}", host.lifecycle().inspect(thread).unwrap().revision)
        );
        assert!(replacement.controller_token().is_ok());
        controller.disconnect().unwrap().wait().await.unwrap();
        replacement.disconnect().unwrap().wait().await.unwrap();
        observer.disconnect().unwrap().wait().await.unwrap();
        owner.close().await.unwrap();
        retained.codex.shutdown_and_wait().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn public_controller_old_acquire_does_not_refresh_authority_and_recovery_is_explicit() {
    let temporary = tempfile::tempdir().unwrap();
    let workspace = temporary.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let config = config(
        &temporary.path().join("canonical"),
        &workspace.canonicalize().unwrap(),
        BackendKind::Files,
    );
    let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
    let current = access(&host, &config, true);
    let mut controller = host.public_connection(current.clone()).unwrap();
    let first = acquire(&config, "original", None);
    controller.call(first.clone(), &current).await.unwrap();
    host.command(
        Command::SetWorkspaceTrust {
            trust: Trust::Trusted,
        },
        None,
        Revision::ZERO,
    )
    .unwrap();
    let refreshed = access(&host, &config, true);
    assert!(controller.call(first.clone(), &current).await.is_err());
    controller.call(first, &refreshed).await.unwrap();
    assert!(controller.controller_token().is_err());
    assert!(controller
        .call(release(&config, "stale-token", 0, 1), &refreshed)
        .await
        .is_err());
    let recover = Call::ControllerRecover(methods::ControllerRecover {
        scope: scope(&config),
        command_id: id("recover"),
        expected_revision: 0.into(),
        generation: 1.into(),
    });
    assert!(controller.call(recover.clone(), &refreshed).await.is_err());
    owner.close().await.unwrap();
    drop(controller);
    drop(host);
    let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
    let current = access(&host, &config, true);
    let mut controller = host.public_connection(current.clone()).unwrap();
    assert_eq!(
        view(&mut controller, &config, &current).await.ownership,
        methods::ControllerOwnership::PreviousProcess
    );
    assert!(controller
        .call(acquire(&config, "too-early", Some(0)), &current)
        .await
        .is_err());
    let recovered = controller.call(recover.clone(), &current).await.unwrap();
    assert_eq!(
        controller.call(recover.clone(), &current).await.unwrap(),
        recovered
    );
    assert_eq!(
        view(&mut controller, &config, &current).await.ownership,
        methods::ControllerOwnership::Released
    );
    assert!(controller.controller_token().is_err());
    controller
        .call(acquire(&config, "after-recovery", Some(1)), &current)
        .await
        .unwrap();
    let acquired = controller.controller_token().unwrap();
    let watermark = host.snapshot().unwrap().watermark;
    assert_eq!(controller.call(recover, &current).await.unwrap(), recovered);
    let after_replay = controller.controller_token().unwrap();
    assert_eq!(acquired.generation(), after_replay.generation());
    assert_eq!(acquired.revision(), after_replay.revision());
    assert_eq!(host.snapshot().unwrap().watermark, watermark);
    controller.disconnect().unwrap().wait().await.unwrap();
    owner.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn public_controller_release_survives_lost_reply_and_concurrent_disconnect() {
    let temporary = tempfile::tempdir().unwrap();
    let workspace = temporary.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let config = config(
        &temporary.path().join("canonical"),
        &workspace.canonicalize().unwrap(),
        BackendKind::Files,
    );
    let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
    let current = access(&host, &config, true);
    let mut controller = host.public_connection(current.clone()).unwrap();
    controller
        .call(acquire(&config, "acquire", None), &current)
        .await
        .unwrap();
    let server = start_mock_server().await;
    let retained = test_codex().build_with_auto_env(&server).await.unwrap();
    let thread = host
        .lifecycle()
        .attach_root(retained.codex.clone())
        .unwrap();
    let permit = host
        .lifecycle()
        .admit_turn_start_for_thread(thread)
        .unwrap();
    let mut pending = Box::pin(controller.call(release(&config, "owned-release", 0, 1), &current));
    std::future::poll_fn(|context| match pending.as_mut().poll(context) {
        Poll::Pending => Poll::Ready(()),
        Poll::Ready(result) => panic!("release finished before owned drain: {result:?}"),
    })
    .await;
    assert!(host.lifecycle().inspect(thread).unwrap().local_hold);
    drop(pending);
    let disconnect = controller.disconnect().unwrap();
    drop(permit);
    disconnect.wait().await.unwrap();
    let lease: vcp_domain::controller::Lease = host
        .snapshot()
        .unwrap()
        .records
        .values()
        .find(|row| row.value["document_type"] == "vcp_controller_lease_v1")
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(lease.reason, vcp_domain::controller::Reason::Released);
    assert_eq!(lease.revision, Revision::new(1));
    let mut observer = host
        .public_connection(access(&host, &config, false))
        .unwrap();
    assert_eq!(
        view(&mut observer, &config, &access(&host, &config, false))
            .await
            .ownership,
        methods::ControllerOwnership::Released
    );
    observer.disconnect().unwrap().wait().await.unwrap();
    owner.close().await.unwrap();
    retained.codex.shutdown_and_wait().await.unwrap();
}
