// SPDX-License-Identifier: Apache-2.0
//! Real retained provider interruption with a deterministic admitted-start drain gate.
use super::*;
use codex_extension_api::TurnStartAdmission;
use core_test_support::streaming_sse::{start_streaming_sse_server, StreamingSseChunk};
use std::{future::Future, task::Poll};
use vcp_engine::{rpc::RpcHost, Access};
use vcp_protocol::methods::{self, Call};

fn id(value: &str) -> methods::Id {
    value.to_owned().try_into().unwrap()
}

fn selected(host: &CanonicalHost, config: &Config) -> Task {
    host.snapshot()
        .unwrap()
        .record(
            Collection::Task,
            config.root_task.as_str(),
            &config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn public_rpc_active_stream_drain_survives_waiter_loss_and_rejects_owner_loss_or_capture_fault(
) {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for scenario in [
            "dropped-waiter",
            "connection-lost",
            "capture-fault",
            "signal-before-disconnect",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let workspace = temp.path().join("workspace");
            std::fs::create_dir(&workspace).unwrap();
            let workspace = workspace.canonicalize().unwrap();
            let mut config = config(&temp.path().join("canonical"), &workspace, backend);
            config.artifact_limit = ByteCount::new(65_536);
            let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
            host.command(
                Command::SetWorkspaceTrust {
                    trust: Trust::Trusted,
                },
                None,
                Revision::ZERO,
            )
            .unwrap();
            let current = Access {
                actor: config.actor.clone(),
                workspace: config.workspace.clone(),
                session: config.session.clone(),
                authority: AuthorityRevision::new(1),
                read: true,
                write: true,
                bootstrap: false,
            };
            let mut controller = host.public_connection(current.clone()).unwrap();
            controller.acquire(CommandId::new(), None).unwrap();
            let binding = task(&host, &config, config.root_task.clone(), None);
            let (release, gate) = tokio::sync::oneshot::channel();
            let prefix = sse(vec![
                ev_response_created("public-drain"),
                ev_message_item_added("partial", ""),
                ev_output_text_delta("observed public drain prefix"),
            ]);
            let (server, _) = start_streaming_sse_server(vec![vec![
                StreamingSseChunk {
                    gate: None,
                    body: prefix.clone(),
                },
                StreamingSseChunk {
                    gate: Some(gate),
                    body: sse(vec![ev_completed_with_tokens("public-drain", 7)]),
                },
            ]])
            .await;
            let mut registry = ExtensionRegistryBuilder::new();
            registry.turn_start_admission(Arc::new(host.clone()));
            registry.work_admission(Arc::new(host.clone()));
            let starter = host.clone();
            let cwd = workspace.clone();
            let model = config.price.model.clone();
            let test = test_codex()
                .with_extensions(Arc::new(registry.build()))
                .with_auth(codex_login::CodexAuth::from_api_key(
                    "synthetic-public-drain",
                ))
                .with_allowed_tools(AllowedTools(vec![]))
                .with_config(move |config| {
                    config.cwd = cwd.try_into().unwrap();
                    config.model = Some(model.clone());
                    configure_fixture_provider(config);
                    starter
                        .lifecycle()
                        .authorize_startup(config.cwd.as_path(), None)
                        .unwrap();
                })
                .build_with_streaming_server(&server)
                .await
                .unwrap();
            let thread = host.lifecycle().attach_root(test.codex.clone()).unwrap();
            host.register(thread, binding).unwrap();
            let trigger = host
                .capture(thread, Channel::Evidence, b"public drain trigger".to_vec())
                .unwrap();
            let turn = TurnId::parse("public-drain-turn").unwrap();
            host.command(
                Command::StartTurn {
                    id: turn.clone(),
                    trigger: trigger.spec.id,
                },
                Some(config.root_task.clone()),
                Revision::new(1),
            )
            .unwrap();
            test.codex
                .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                    text: "synthetic gated public drain".into(),
                    text_elements: vec![],
                }]))
                .await
                .unwrap();
            tokio::time::timeout(
                Duration::from_secs(10),
                wait_for_event(&test.codex, |event| {
                    matches!(event, EventMsg::AgentMessageContentDelta(_))
                }),
            )
            .await
            .unwrap();
            assert_eq!(server.requests().await.len(), 1);
            // This is a real owner admission permit, not a production test hook:
            // interruption cannot finish until the admitted start relinquishes it.
            let drain_gate = host.admit_turn_start_for_thread(thread).unwrap();
            let output = if scenario == "capture-fault" {
                Some(host.open_output(thread, Channel::Stdout).unwrap())
            } else {
                None
            };
            let before = selected(&host, &config);
            let command = CommandId::parse("public-gated-steer").unwrap();
            let call = Call::TurnSteer(methods::TurnSteer {
                scope: methods::Scope {
                    workspace: id(config.workspace.as_str()),
                    session: id(config.session.as_str()),
                },
                mutation: methods::Mutation {
                    command_id: id(command.as_str()),
                    expected_revision: before.revision.get().into(),
                    steering_revision: before.steering.get().into(),
                },
                task: id(config.root_task.as_str()),
                turn: id(turn.as_str()),
                objective: "new objective after owned drain".into(),
                constraints: vec![],
                acceptance: vec![],
            });
            let signal = controller.loss_signal();
            let mut pending = Box::pin(controller.call(call.clone(), &current));
            std::future::poll_fn(|context| match pending.as_mut().poll(context) {
                Poll::Pending => Poll::Ready(()),
                Poll::Ready(result) => panic!("steering finished before gate release: {result:?}"),
            })
            .await;
            assert!(host.lifecycle().inspect(thread).unwrap().local_hold);
            assert_eq!(selected(&host, &config).steering, before.steering);
            assert!(!host
                .snapshot()
                .unwrap()
                .commands
                .values()
                .any(|receipt| receipt.command == command));
            if scenario == "capture-fault" {
                let output = output.unwrap();
                // Exactly fill one physical artifact, then cross its capacity.
                for chunk in vec![b'x'; 65_536].chunks(vcp_store::artifact::CHUNK_BYTES) {
                    output.write(chunk).unwrap();
                }
                assert!(output.write(b"overflow").is_err());
                drop(drain_gate);
                assert!(tokio::time::timeout(Duration::from_secs(10), pending)
                    .await
                    .unwrap()
                    .is_err());
                drop(output);
                controller.disconnect().unwrap().wait().await.unwrap();
            } else if scenario == "signal-before-disconnect" {
                // The blocking pipe pump can detect loss before the async
                // dispatcher gets scheduled to drop this request/connection.
                signal.invalidate();
                drop(drain_gate);
                assert!(tokio::time::timeout(Duration::from_secs(10), pending)
                    .await
                    .unwrap()
                    .is_err());
                assert!(!host
                    .snapshot()
                    .unwrap()
                    .commands
                    .values()
                    .any(|receipt| receipt.command == command));
                controller.disconnect().unwrap().wait().await.unwrap();
            } else {
                // Cancelling the JSON-RPC receiver must not cancel host-owned completion.
                drop(pending);
                if scenario == "connection-lost" {
                    let disconnect = controller.disconnect().unwrap();
                    drop(drain_gate);
                    tokio::time::timeout(Duration::from_secs(10), disconnect.wait())
                        .await
                        .unwrap()
                        .unwrap();
                } else {
                    drop(drain_gate);
                    tokio::time::timeout(Duration::from_secs(10), async {
                        while !host
                            .snapshot()
                            .unwrap()
                            .commands
                            .values()
                            .any(|receipt| receipt.command == command)
                        {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                    })
                    .await
                    .unwrap();
                    // Reconciliation/retry returns the single durable result without re-holding.
                    controller.call(call, &current).await.unwrap();
                    controller.disconnect().unwrap().wait().await.unwrap();
                }
            }
            let state = host.snapshot().unwrap();
            let task = selected(&host, &config);
            let intent: ArtifactDescriptor = state
                .records
                .values()
                .filter(|record| record.collection == Collection::Artifact)
                .filter_map(|record| record.decode::<ArtifactDescriptor>().ok())
                .find(|artifact| artifact.spec.schema == "vcp-public-authority-intent/1")
                .expect("original identity must survive request loss as durable intent");
            let intent: serde_json::Value =
                serde_json::from_slice(&host.read_artifact(intent.spec.id).unwrap()).unwrap();
            assert_eq!(intent["command"], command.as_str());
            assert_eq!(intent["state"], "stopping; command not applied");
            assert_eq!(task.state, TaskState::Paused);
            let receipts = state
                .commands
                .values()
                .filter(|receipt| receipt.command == command)
                .count();
            if scenario == "dropped-waiter" {
                assert_eq!(receipts, 1);
                assert_eq!(task.steering, before.steering.next().unwrap());
            } else {
                assert_eq!(receipts, 0, "{scenario} must not commit public steering");
                assert_eq!(task.steering, before.steering);
            }
            let attempt: Attempt = state
                .records
                .values()
                .find(|record| record.collection == Collection::Attempt)
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(attempt.phase, ReservationState::ReconciliationPending);
            let response: ArtifactDescriptor = state
                .records
                .values()
                .filter(|record| record.collection == Collection::Artifact)
                .filter_map(|record| record.decode::<ArtifactDescriptor>().ok())
                .find(|artifact| artifact.spec.channel == Channel::Response)
                .unwrap();
            assert_eq!(response.state, CaptureState::Aborted);
            assert_eq!(
                host.read_artifact(response.spec.id).unwrap(),
                prefix.as_bytes()
            );
            assert_eq!(server.requests().await.len(), 1);
            let _ = release.send(());
            owner.close().await.unwrap();
            test.codex.shutdown_and_wait().await.unwrap();
            server.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn public_rpc_without_root_drains_admitted_startup_before_steering() {
    use codex_extension_api::HostWorkAdmission;
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let config = config(&temp.path().join("canonical"), &workspace, backend);
        // Retain an actual trigger in a previous host, then reopen with no live
        // bindings or retained root. The new server must still drain startup.
        let (seed, seed_owner) = CanonicalHost::open(config.clone()).unwrap();
        let binding = task(&seed, &config, config.root_task.clone(), None);
        let seed_thread = codex_protocol::ThreadId::new();
        seed.register(seed_thread, binding).unwrap();
        let trigger = seed
            .capture(
                seed_thread,
                Channel::Evidence,
                b"queued startup steering".to_vec(),
            )
            .unwrap();
        seed_owner.close().await.unwrap();
        drop(seed);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let current = Access {
            actor: config.actor.clone(),
            workspace: config.workspace.clone(),
            session: config.session.clone(),
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            bootstrap: false,
        };
        let mut controller = host.public_connection(current.clone()).unwrap();
        controller.acquire(CommandId::new(), None).unwrap();
        let before = selected(&host, &config);
        assert_eq!(before.state, TaskState::Paused);
        let turn = TurnId::parse("queued-no-root").unwrap();
        host.command(
            Command::StartTurn {
                id: turn.clone(),
                trigger: trigger.spec.id,
            },
            Some(config.root_task.clone()),
            before.revision,
        )
        .unwrap();
        let queued: Turn = host
            .snapshot()
            .unwrap()
            .record(Collection::Turn, turn.as_str(), &config.workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(queued.state, TurnState::Queued);
        assert!(host.lifecycle().root().unwrap().is_none());
        host.lifecycle()
            .authorize_startup(&workspace, None)
            .unwrap();
        let startup = host.admit_startup(&workspace, None).unwrap();
        let command = CommandId::parse("steer-no-root").unwrap();
        let call = Call::TurnSteer(methods::TurnSteer {
            scope: methods::Scope {
                workspace: id(config.workspace.as_str()),
                session: id(config.session.as_str()),
            },
            mutation: methods::Mutation {
                command_id: id(command.as_str()),
                expected_revision: before.revision.get().into(),
                steering_revision: before.steering.get().into(),
            },
            task: id(config.root_task.as_str()),
            turn: id(turn.as_str()),
            objective: "steering after startup drain".into(),
            constraints: vec![],
            acceptance: vec![],
        });
        let mut pending = Box::pin(controller.call(call.clone(), &current));
        std::future::poll_fn(|context| match pending.as_mut().poll(context) {
            Poll::Pending => Poll::Ready(()),
            Poll::Ready(result) => panic!("steering completed before startup drain: {result:?}"),
        })
        .await;
        drop(pending);
        assert_eq!(selected(&host, &config).steering, before.steering);
        assert!(!host
            .snapshot()
            .unwrap()
            .commands
            .values()
            .any(|receipt| receipt.command == command));
        assert!(host
            .lifecycle()
            .authorize_startup(&workspace, None)
            .is_err());
        assert!(host.admit_startup(&workspace, None).is_err());
        drop(startup);
        tokio::time::timeout(Duration::from_secs(10), async {
            while !host
                .snapshot()
                .unwrap()
                .commands
                .values()
                .any(|receipt| receipt.command == command)
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let after = selected(&host, &config);
        assert_eq!(after.state, TaskState::Paused);
        assert_eq!(after.steering, before.steering.next().unwrap());
        controller.call(call, &current).await.unwrap();
        assert_eq!(
            host.snapshot()
                .unwrap()
                .commands
                .values()
                .filter(|receipt| receipt.command == command)
                .count(),
            1
        );
        assert!(host.lifecycle().root().unwrap().is_none());
        assert!(host
            .lifecycle()
            .authorize_startup(&workspace, None)
            .is_err());
        assert!(host.admit_startup(&workspace, None).is_err());
        controller.disconnect().unwrap().wait().await.unwrap();
        owner.close().await.unwrap();
    }
}
