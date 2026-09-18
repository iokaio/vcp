// SPDX-License-Identifier: Apache-2.0
use codex_core::{
    CodexThread, NotSubmittedReason, StartThreadOptions, TurnInputRequest, TurnInputSubmission,
    TurnStartOptions,
};
use codex_extension_api::{ExtensionRegistry, ExtensionRegistryBuilder, TurnStartAdmission};
use codex_protocol::{
    protocol::{EventMsg, SessionSource, SubAgentSource},
    user_input::UserInput,
    ThreadId,
};
use core_test_support::{
    responses,
    streaming_sse::{start_streaming_sse_server, StreamingSseChunk},
    test_codex::{test_codex, TestCodex},
    wait_for_event,
};
use std::{sync::Arc, time::Duration};
use tokio::{sync::oneshot, time::timeout};
use vcp_lifecycle::{Error, Lifecycle};

fn extensions(host: &Lifecycle) -> Arc<ExtensionRegistry<codex_core::config::Config>> {
    let mut builder = ExtensionRegistryBuilder::new();
    builder.turn_start_admission(Arc::new(host.clone()));
    Arc::new(builder.build())
}

fn input(text: &str) -> TurnInputRequest {
    TurnInputRequest::user_input(vec![UserInput::Text {
        text: text.into(),
        text_elements: vec![],
    }])
}

fn delegated(text: &str) -> TurnInputRequest {
    input(text).on_start(TurnStartOptions {
        parent_turn_id: Some("host-parent".into()),
        ..Default::default()
    })
}

async fn child(test: &TestCodex) -> Arc<CodexThread> {
    test.thread_manager
        .start_thread(StartThreadOptions {
            session_source: Some(SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                parent_thread_id: test.session_configured.thread_id,
                depth: 1,
                agent_path: None,
                agent_nickname: None,
                agent_role: None,
            })),
            environments: Some(test.codex.environment_selections().await),
            ..StartThreadOptions::new(test.config.clone())
        })
        .await
        .unwrap()
        .thread
}

async fn complete(thread: &CodexThread, request: TurnInputRequest) {
    assert!(matches!(
        thread.start_or_steer_turn(request).await.unwrap(),
        TurnInputSubmission::Started { .. }
    ));
    timeout(
        Duration::from_secs(10),
        wait_for_event(thread, |e| matches!(e, EventMsg::TurnComplete(_))),
    )
    .await
    .unwrap();
}

async fn denied(thread: &CodexThread, request: TurnInputRequest) {
    assert_eq!(
        thread.start_or_steer_turn(request).await.unwrap(),
        TurnInputSubmission::NotSubmitted {
            reason: NotSubmittedReason::ServerDraining
        }
    );
}

async fn hold(host: &Lifecycle, id: ThreadId) {
    timeout(
        Duration::from_secs(10),
        host.hold(id, &host.inspect(id).unwrap().revision)
            .unwrap()
            .wait(),
    )
    .await
    .unwrap()
    .unwrap();
}

#[tokio::test]
async fn independent_child_hold_survives_parent_readmission() {
    let server = responses::start_mock_server().await;
    let observed = responses::mount_sse_sequence(
        &server,
        [
            "parent",
            "sibling",
            "parent-again",
            "sibling-again",
            "child",
        ]
        .into_iter()
        .map(responses::sse_completed)
        .collect(),
    )
    .await;
    let (host, _owner) = Lifecycle::new(Duration::from_secs(5));
    let test = test_codex()
        .with_extensions(extensions(&host))
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    let child = child(&test).await;
    let child_id = host
        .attach_child(root, &host.inspect(root).unwrap().revision, child.clone())
        .unwrap();
    let sibling = self::child(&test).await;
    let sibling_id = host
        .attach_child(root, &host.inspect(root).unwrap().revision, sibling.clone())
        .unwrap();
    hold(&host, child_id).await;
    denied(&child, delegated("independently held")).await;
    complete(&test.codex, input("parent remains active")).await;
    complete(&sibling, delegated("sibling remains active")).await;
    hold(&host, root).await;
    assert!(host.inspect(sibling_id).unwrap().inherited_hold);
    assert_eq!(
        host.resume(child_id, &host.inspect(child_id).unwrap().revision),
        Err(Error::Held)
    );
    denied(&sibling, delegated("inherited hold")).await;
    host.resume(root, &host.inspect(root).unwrap().revision)
        .unwrap();
    assert!(host.inspect(child_id).unwrap().local_hold);
    assert!(!host.inspect(child_id).unwrap().inherited_hold);
    denied(&child, delegated("still independently held")).await;
    complete(&test.codex, input("root readmitted")).await;
    complete(&sibling, delegated("sibling readmitted")).await;
    host.resume(child_id, &host.inspect(child_id).unwrap().revision)
        .unwrap();
    complete(&child, delegated("child explicitly readmitted")).await;
    assert_eq!(observed.requests().len(), 5);
    assert!(!observed
        .requests()
        .last()
        .unwrap()
        .body_contains_text("independently held"));
}

#[tokio::test]
async fn sealing_interrupts_both_streams_and_keeps_controllers_inspectable() {
    let (root_release, root_gate) = oneshot::channel();
    let (child_release, child_gate) = oneshot::channel();
    let pending = |id: &str, gate| {
        vec![
            StreamingSseChunk {
                gate: None,
                body: responses::sse(vec![responses::ev_response_created(id)]),
            },
            StreamingSseChunk {
                gate: Some(gate),
                body: responses::sse(vec![responses::ev_completed(id)]),
            },
        ]
    };
    let (server, _) = start_streaming_sse_server(vec![
        pending("root", root_gate),
        pending("child", child_gate),
        vec![StreamingSseChunk {
            gate: None,
            body: responses::sse_completed("root-resumed"),
        }],
        vec![StreamingSseChunk {
            gate: None,
            body: responses::sse_completed("child-resumed"),
        }],
    ])
    .await;
    let (host, _owner) = Lifecycle::new(Duration::from_secs(5));
    let test = test_codex()
        .with_extensions(extensions(&host))
        .with_model("gpt-5.4")
        .build_with_streaming_server(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    let child = child(&test).await;
    host.attach_child(root, &host.inspect(root).unwrap().revision, child.clone())
        .unwrap();
    test.codex
        .start_or_steer_turn(input("root active"))
        .await
        .unwrap();
    timeout(Duration::from_secs(10), server.wait_for_request_count(1))
        .await
        .unwrap();
    child
        .start_or_steer_turn(delegated("child active"))
        .await
        .unwrap();
    timeout(Duration::from_secs(10), server.wait_for_request_count(2))
        .await
        .unwrap();
    hold(&host, root).await;
    for thread in [&test.codex, &child] {
        timeout(
            Duration::from_secs(10),
            wait_for_event(thread, |e| matches!(e, EventMsg::TurnAborted(_))),
        )
        .await
        .unwrap();
        assert!(!thread.config_snapshot().await.model.is_empty());
    }
    denied(&test.codex, input("root rejected")).await;
    denied(&child, delegated("child rejected")).await;
    assert_eq!(server.requests().await.len(), 2);
    let _ = root_release.send(());
    let _ = child_release.send(());
    host.resume(root, &host.inspect(root).unwrap().revision)
        .unwrap();
    complete(&test.codex, input("root explicitly resumed")).await;
    complete(&child, delegated("child explicitly resumed")).await;
    assert_eq!(server.requests().await.len(), 4);
    child.shutdown_and_wait().await.unwrap();
    test.codex.shutdown_and_wait().await.unwrap();
    server.shutdown().await;
}

#[tokio::test]
async fn cancelled_waiter_preserves_seal_and_drains_issued_permits() {
    let server = responses::start_mock_server().await;
    let (host, _owner) = Lifecycle::new(Duration::from_secs(5));
    let test = test_codex()
        .with_extensions(extensions(&host))
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    let permit = host.admit_turn_start_for_thread(root).unwrap();
    let waiter = host
        .hold(root, &host.inspect(root).unwrap().revision)
        .unwrap();
    assert!(host.admit_continuation_start_for_thread(root).is_none());
    assert_eq!(
        host.resume(root, &host.inspect(root).unwrap().revision),
        Err(Error::Busy)
    );
    assert!(!host.inspect(root).unwrap().interrupt_complete);
    drop(waiter);
    drop(permit);
    timeout(Duration::from_secs(10), async {
        while !host.inspect(root).unwrap().interrupt_complete {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(host.inspect(root).unwrap().local_hold);
    host.resume(root, &host.inspect(root).unwrap().revision)
        .unwrap();
    assert!(host.admit_turn_start_for_thread(root).is_some());
}

#[tokio::test]
async fn timed_out_drain_requires_successful_interruption_before_resume() {
    let server = responses::start_mock_server().await;
    let (host, _owner) = Lifecycle::new(Duration::from_millis(50));
    let test = test_codex()
        .with_extensions(extensions(&host))
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    let permit = host.admit_continuation_start_for_thread(root).unwrap();
    assert_eq!(
        host.hold(root, &host.inspect(root).unwrap().revision)
            .unwrap()
            .wait()
            .await,
        Err(Error::DrainTimeout)
    );
    assert_eq!(
        host.resume(root, &host.inspect(root).unwrap().revision),
        Err(Error::IncompleteInterruption)
    );
    assert_eq!(
        host.inspect(root).unwrap().interruption_error,
        Some(Error::DrainTimeout)
    );
    assert!(host.admit_turn_start_for_thread(root).is_none());
    drop(permit);
    hold(&host, root).await;
    host.resume(root, &host.inspect(root).unwrap().revision)
        .unwrap();
}

#[tokio::test]
async fn stale_foreign_and_unregistered_scope_cannot_gain_authority() {
    let server = responses::start_mock_server().await;
    let (host, _owner) = Lifecycle::new(Duration::from_secs(5));
    let test = test_codex()
        .with_extensions(extensions(&host))
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    let original = host.inspect(root).unwrap().revision;
    let unregistered = child(&test).await;
    denied(&unregistered, delegated("unknown scope")).await;
    hold(&host, root).await;
    assert_eq!(host.resume(root, &original), Err(Error::StaleRevision));
    assert_eq!(
        host.attach_child(
            root,
            &host.inspect(root).unwrap().revision,
            unregistered.clone()
        ),
        Err(Error::Held)
    );
    let (foreign, _foreign_owner) = Lifecycle::new(Duration::from_secs(5));
    let other = test_codex()
        .with_extensions(extensions(&foreign))
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let other_id = foreign.attach_root(other.codex.clone()).unwrap();
    assert_eq!(
        host.resume(root, &foreign.inspect(other_id).unwrap().revision),
        Err(Error::StaleRevision)
    );
    host.resume(root, &host.inspect(root).unwrap().revision)
        .unwrap();
    let before_registration = host.inspect(root).unwrap().revision;
    host.attach_child(root, &before_registration, unregistered.clone())
        .unwrap();
    assert!(matches!(
        host.hold(root, &before_registration),
        Err(Error::StaleRevision)
    ));
    assert_eq!(
        host.attach_child(root, &host.inspect(root).unwrap().revision, unregistered),
        Err(Error::DuplicateThread)
    );
    assert!(host.admit_turn_start().is_none());
    assert!(host.admit_continuation_start().is_none());
}

#[tokio::test]
async fn owner_loss_interrupts_active_work_and_cannot_be_resumed() {
    let (release, gate) = oneshot::channel();
    let (server, _) = start_streaming_sse_server(vec![vec![
        StreamingSseChunk {
            gate: None,
            body: responses::sse(vec![responses::ev_response_created("active")]),
        },
        StreamingSseChunk {
            gate: Some(gate),
            body: responses::sse(vec![responses::ev_completed("active")]),
        },
    ]])
    .await;
    let (host, owner) = Lifecycle::new(Duration::from_secs(5));
    let test = test_codex()
        .with_extensions(extensions(&host))
        .with_model("gpt-5.4")
        .build_with_streaming_server(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    test.codex
        .start_or_steer_turn(input("active before owner loss"))
        .await
        .unwrap();
    timeout(Duration::from_secs(10), server.wait_for_request_count(1))
        .await
        .unwrap();
    drop(owner);
    assert!(!host.inspect(root).unwrap().owner_attached);
    assert!(host.admit_turn_start_for_thread(root).is_none());
    assert!(host.admit_continuation_start_for_thread(root).is_none());
    timeout(
        Duration::from_secs(10),
        wait_for_event(&test.codex, |e| matches!(e, EventMsg::TurnAborted(_))),
    )
    .await
    .unwrap();
    assert_eq!(
        host.resume(root, &host.inspect(root).unwrap().revision),
        Err(Error::OwnerLost)
    );
    denied(&test.codex, input("no implicit reconnect")).await;
    assert_eq!(server.requests().await.len(), 1);
    let _ = release.send(());
    test.codex.shutdown_and_wait().await.unwrap();
    server.shutdown().await;
}

#[tokio::test]
async fn released_controller_is_not_retained_or_reported_as_interrupted() {
    let server = responses::start_mock_server().await;
    let (host, _owner) = Lifecycle::new(Duration::from_secs(5));
    let test = test_codex()
        .with_extensions(extensions(&host))
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    let weak = Arc::downgrade(&test.codex);
    test.codex.shutdown_and_wait().await.unwrap();
    drop(test.thread_manager.remove_thread(&root).await);
    drop(test);
    timeout(Duration::from_secs(10), async {
        while weak.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        host.hold(root, &host.inspect(root).unwrap().revision)
            .unwrap()
            .wait()
            .await,
        Err(Error::InterruptFailed)
    );
    let view = host.inspect(root).unwrap();
    assert!(view.local_hold);
    assert!(!view.interrupt_complete);
    assert_eq!(view.interruption_error, Some(Error::InterruptFailed));
    assert_eq!(
        host.resume(root, &view.revision),
        Err(Error::IncompleteInterruption)
    );
}
