// SPDX-License-Identifier: Apache-2.0
//! Public controls retain the connection while owned interruption drains.
use super::*;
use codex_extension_api::{HostWorkAdmission, TurnStartAdmission};
use core_test_support::streaming_sse::{start_streaming_sse_server, StreamingSseChunk};
use vcp_engine::{rpc::RpcHost, Access};
use vcp_protocol::methods::{self, Call, ResultValue};

fn id(value: &str) -> methods::Id {
    value.to_owned().try_into().unwrap()
}
fn scope(config: &Config) -> methods::Scope {
    methods::Scope {
        workspace: id(config.workspace.as_str()),
        session: id(config.session.as_str()),
    }
}
fn selected(host: &CanonicalHost, config: &Config, task: &TaskId) -> Task {
    host.snapshot()
        .unwrap()
        .record(Collection::Task, task.as_str(), &config.workspace)
        .unwrap()
        .decode()
        .unwrap()
}
fn params(
    call: &mut Call,
) -> (
    &mut methods::Scope,
    &mut methods::Mutation,
    &mut methods::Id,
) {
    match call {
        Call::TaskCancel(p) => (&mut p.scope, &mut p.mutation, &mut p.task),
        Call::TurnPause(p) | Call::TurnCancel(p) => (&mut p.scope, &mut p.mutation, &mut p.task),
        _ => unreachable!("fixture only creates task/turn controls"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn public_task_control_seals_unattached_root_startup_and_keeps_connection_alive() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for cancel in [false, true] {
            let temporary = tempfile::tempdir().unwrap();
            let workspace = temporary.path().join("workspace");
            std::fs::create_dir(&workspace).unwrap();
            let workspace = workspace.canonicalize().unwrap();
            let config = config(&temporary.path().join("canonical"), &workspace, backend);
            let (seed, seed_owner) = CanonicalHost::open(config.clone()).unwrap();
            let binding = task(&seed, &config, config.root_task.clone(), None);
            let seed_thread = codex_protocol::ThreadId::new();
            seed.register(seed_thread, binding).unwrap();
            let trigger = seed
                .capture(
                    seed_thread,
                    Channel::Evidence,
                    b"queued public control startup".to_vec(),
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
            let token = controller.controller_token().unwrap();
            let before = selected(&host, &config, &config.root_task);
            assert_eq!(before.state, TaskState::Paused);
            let turn = TurnId::parse("queued-control-turn").unwrap();
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
            let mutation = methods::Mutation {
                command_id: id("unattached-control"),
                expected_revision: before.revision.get().into(),
                steering_revision: before.steering.get().into(),
            };
            let call = if cancel {
                Call::TaskCancel(methods::TaskCancel {
                    scope: scope(&config),
                    mutation,
                    task: id(config.root_task.as_str()),
                    reason: "cancel before root attaches".into(),
                })
            } else {
                Call::TurnPause(methods::TurnControl {
                    scope: scope(&config),
                    mutation,
                    task: id(config.root_task.as_str()),
                    turn: id(turn.as_str()),
                    reason: "pause before root attaches".into(),
                })
            };
            // Acceptance must not wait for an admitted constructor to finish;
            // interruption/draining remains owned after the RPC has returned.
            let receipt = tokio::time::timeout(
                Duration::from_secs(2),
                controller.call(call.clone(), &current),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(
                host.lifecycle().authorize_startup(&workspace, None),
                Err(vcp_lifecycle::Error::Busy)
            );
            assert!(host.admit_startup(&workspace, None).is_err());
            let accepted = host.snapshot().unwrap();
            assert_eq!(
                controller.call(call.clone(), &current).await.unwrap(),
                receipt
            );
            assert_eq!(host.snapshot().unwrap(), accepted);
            assert_eq!(
                controller.controller_token().unwrap().generation(),
                token.generation()
            );
            drop(startup);
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    match host.lifecycle().authorize_startup(&workspace, None) {
                        Err(vcp_lifecycle::Error::Held) => break,
                        Err(vcp_lifecycle::Error::Busy) => {
                            tokio::time::sleep(Duration::from_millis(10)).await
                        }
                        result => panic!("startup fence changed unexpectedly: {result:?}"),
                    }
                }
            })
            .await
            .unwrap();
            assert_eq!(
                selected(&host, &config, &config.root_task).state,
                if cancel {
                    TaskState::Cancelled
                } else {
                    TaskState::Paused
                }
            );
            assert_eq!(controller.call(call, &current).await.unwrap(), receipt);
            assert_eq!(host.snapshot().unwrap(), accepted);
            let current_token = controller.controller_token().unwrap();
            assert_eq!(current_token.generation(), token.generation());
            assert_eq!(current_token.revision(), token.revision());
            let observed = controller
                .call(
                    Call::TaskRead(methods::TaskRead {
                        scope: scope(&config),
                        task: id(config.root_task.as_str()),
                    }),
                    &current,
                )
                .await
                .unwrap();
            assert!(matches!(observed, ResultValue::Task(_)));
            let server = start_mock_server().await;
            let late = test_codex().build_with_auto_env(&server).await.unwrap();
            assert!(host.lifecycle().attach_root(late.codex.clone()).is_err());
            assert!(host.lifecycle().root().unwrap().is_none());
            late.codex.shutdown_and_wait().await.unwrap();
            controller.disconnect().unwrap().wait().await.unwrap();
            owner.close().await.unwrap();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn public_task_controls_pause_or_cancel_active_descendants_without_closing_the_connection() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for operation in ["task/cancel", "turn/pause", "turn/cancel"] {
            let temporary = tempfile::tempdir().unwrap();
            let workspace = temporary.path().join("workspace");
            std::fs::create_dir(&workspace).unwrap();
            let workspace = workspace.canonicalize().unwrap();
            let config = config(&temporary.path().join("canonical"), &workspace, backend);
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
            let mut readonly = current.clone();
            readonly.write = false;
            let mut controller = host.public_connection(current.clone()).unwrap();
            let mut observer = host.public_connection(readonly.clone()).unwrap();
            controller.acquire(CommandId::new(), None).unwrap();
            let token = controller.controller_token().unwrap();
            let root_binding = task(&host, &config, config.root_task.clone(), None);
            let child_id = TaskId::parse("public-control-child").unwrap();
            let child_binding = task(
                &host,
                &config,
                child_id.clone(),
                Some(config.root_task.clone()),
            );
            let (release_root, root_gate) = tokio::sync::oneshot::channel();
            let (release_child, child_gate) = tokio::sync::oneshot::channel();
            let responses = [("root", root_gate), ("child", child_gate)]
                .into_iter()
                .map(|(name, gate)| {
                    vec![
                        StreamingSseChunk {
                            gate: None,
                            body: sse(vec![
                                ev_response_created(name),
                                ev_message_item_added(name, ""),
                                ev_output_text_delta("active before public control"),
                            ]),
                        },
                        StreamingSseChunk {
                            gate: Some(gate),
                            body: sse(vec![ev_completed_with_tokens(name, 7)]),
                        },
                    ]
                })
                .collect();
            let (server, _) = start_streaming_sse_server(responses).await;
            let mut retained = Vec::new();
            for (index, binding) in [root_binding, child_binding].into_iter().enumerate() {
                let mut registry = ExtensionRegistryBuilder::new();
                registry.turn_start_admission(Arc::new(host.clone()));
                registry.work_admission(Arc::new(host.clone()));
                let starter = host.clone();
                let cwd = workspace.clone();
                let model = config.price.model.clone();
                let test = test_codex()
                    .with_extensions(Arc::new(registry.build()))
                    .with_auth(codex_login::CodexAuth::from_api_key(
                        "synthetic-public-task-control",
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
                let thread = if index == 0 {
                    host.lifecycle().attach_root(test.codex.clone()).unwrap()
                } else {
                    let root = host.lifecycle().root().unwrap().unwrap();
                    host.lifecycle()
                        .attach_child(
                            root,
                            &host.lifecycle().inspect(root).unwrap().revision,
                            test.codex.clone(),
                        )
                        .unwrap()
                };
                host.register(thread, binding).unwrap();
                if index == 0 {
                    let trigger = host
                        .capture(thread, Channel::Evidence, b"current public turn".to_vec())
                        .unwrap();
                    host.command(
                        Command::StartTurn {
                            id: TurnId::parse("public-current-turn").unwrap(),
                            trigger: trigger.spec.id,
                        },
                        Some(config.root_task.clone()),
                        selected(&host, &config, &config.root_task).revision,
                    )
                    .unwrap();
                }
                test.codex
                    .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                        text: "remain active until explicit public control".into(),
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
                retained.push((thread, test));
            }
            assert_eq!(server.requests().await.len(), 2);
            let root = retained[0].0;
            let child = retained[1].0;
            let before = selected(&host, &config, &config.root_task);
            let read = Call::TaskRead(methods::TaskRead {
                scope: scope(&config),
                task: id(config.root_task.as_str()),
            });
            let ResultValue::Task(view) = observer.call(read.clone(), &readonly).await.unwrap()
            else {
                panic!("task projection required");
            };
            assert_eq!(view.state, methods::TaskStatus::Running);
            assert_eq!(view.task.as_str(), config.root_task.as_str());
            let mut foreign_read = read.clone();
            if let Call::TaskRead(p) = &mut foreign_read {
                p.scope.session = id("other-session");
            }
            assert!(observer.call(foreign_read, &readonly).await.is_err());
            let mutation = methods::Mutation {
                command_id: id("public-control"),
                expected_revision: before.revision.get().into(),
                steering_revision: before.steering.get().into(),
            };
            let control = methods::TurnControl {
                scope: scope(&config),
                mutation: mutation.clone(),
                task: id(config.root_task.as_str()),
                turn: id("public-current-turn"),
                reason: "explicit connected control".into(),
            };
            let call = match operation {
                "task/cancel" => Call::TaskCancel(methods::TaskCancel {
                    scope: scope(&config),
                    mutation,
                    task: id(config.root_task.as_str()),
                    reason: "explicit connected control".into(),
                }),
                "turn/pause" => Call::TurnPause(control),
                _ => Call::TurnCancel(control),
            };
            assert!(observer.call(call.clone(), &readonly).await.is_err());
            for invalid in 0..3 {
                let mut rejected = call.clone();
                let (scope, mutation, task) = params(&mut rejected);
                match invalid {
                    0 => mutation.expected_revision = u64::MAX.into(),
                    1 => scope.session = id("other-session"),
                    _ => *task = id("missing-task"),
                }
                assert!(controller.call(rejected, &current).await.is_err());
                assert!(!host.lifecycle().inspect(root).unwrap().local_hold);
                assert!(!host.lifecycle().inspect(child).unwrap().inherited_hold);
            }
            if operation != "task/cancel" {
                let mut wrong_turn = call.clone();
                match &mut wrong_turn {
                    Call::TurnPause(p) | Call::TurnCancel(p) => p.turn = id("not-current-turn"),
                    _ => unreachable!(),
                }
                assert!(controller.call(wrong_turn, &current).await.is_err());
                assert!(!host.lifecycle().inspect(root).unwrap().local_hold);
            }
            let drain = host.admit_turn_start_for_thread(root).unwrap();
            // Acceptance is durable before interruption completes. Dropping the
            // completed request await does not cancel the host-owned hold waiter.
            let receipt = controller.call(call.clone(), &current).await.unwrap();
            let held = host.lifecycle().inspect(root).unwrap();
            assert!(held.local_hold);
            assert!(!held.interrupt_complete);
            assert!(host.lifecycle().inspect(child).unwrap().inherited_hold);
            assert_eq!(
                controller.call(call.clone(), &current).await.unwrap(),
                receipt
            );
            let mut conflict = call.clone();
            match &mut conflict {
                Call::TaskCancel(p) => p.reason.push_str(" changed"),
                Call::TurnPause(p) | Call::TurnCancel(p) => p.reason.push_str(" changed"),
                _ => unreachable!(),
            }
            assert!(controller.call(conflict, &current).await.is_err());
            assert_eq!(
                format!("{:?}", held.revision),
                format!("{:?}", host.lifecycle().inspect(root).unwrap().revision)
            );
            drop(drain);
            tokio::time::timeout(Duration::from_secs(10), async {
                while !host.lifecycle().inspect(root).unwrap().interrupt_complete
                    || !host.lifecycle().inspect(child).unwrap().interrupt_complete
                {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            let after = selected(&host, &config, &config.root_task);
            assert_eq!(
                after.state,
                if operation == "turn/pause" {
                    TaskState::Paused
                } else {
                    TaskState::Cancelled
                }
            );
            if operation != "turn/pause" {
                assert_eq!(
                    selected(&host, &config, &child_id).state,
                    TaskState::Cancelled
                );
            }
            let ResultValue::Task(view) = observer.call(read, &readonly).await.unwrap() else {
                panic!("connected task read required");
            };
            assert_eq!(
                view.state,
                if operation == "turn/pause" {
                    methods::TaskStatus::Paused
                } else {
                    methods::TaskStatus::Cancelled
                }
            );
            let current_token = controller.controller_token().unwrap();
            assert_eq!(current_token.generation(), token.generation());
            assert_eq!(current_token.revision(), token.revision());
            let observed = observer
                .call(
                    Call::CommandRead(methods::CommandRead {
                        scope: scope(&config),
                        command_id: id("public-control"),
                    }),
                    &readonly,
                )
                .await
                .unwrap();
            assert_eq!(observed, receipt);
            assert_eq!(controller.call(call, &current).await.unwrap(), receipt);
            for (_, test) in &retained {
                assert!(matches!(
                    test.codex
                        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                            text: "connection does not resume".into(),
                            text_elements: vec![]
                        }]))
                        .await
                        .unwrap(),
                    codex_core::TurnInputSubmission::NotSubmitted { .. }
                ));
            }
            assert_eq!(server.requests().await.len(), 2);
            let state = host.snapshot().unwrap();
            assert_eq!(
                state
                    .records
                    .values()
                    .filter(|row| row.collection == Collection::Attempt)
                    .count(),
                2
            );
            assert_eq!(
                state
                    .commands
                    .values()
                    .filter(|receipt| receipt.command.as_str() == "public-control")
                    .count(),
                1
            );
            assert!(state
                .records
                .values()
                .filter(|row| row.collection == Collection::Attempt)
                .all(|row| row.decode::<Attempt>().unwrap().phase
                    == ReservationState::ReconciliationPending));
            assert!(
                vcp_budget::ledger(&state, &after.scope)
                    .unwrap()
                    .unresolved
                    .get()
                    > 0
            );
            let _ = release_root.send(());
            let _ = release_child.send(());
            controller.disconnect().unwrap().wait().await.unwrap();
            observer.disconnect().unwrap().wait().await.unwrap();
            owner.close().await.unwrap();
            for (_, test) in retained {
                test.codex.shutdown_and_wait().await.unwrap();
            }
            server.shutdown().await;
        }
    }
}
