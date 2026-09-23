// SPDX-License-Identifier: Apache-2.0
use super::*;
use codex_extension_api::TurnStartAdmission;
use std::{future::Future, task::Poll};
use vcp_engine::{rpc::RpcHost, Access};
use vcp_protocol::{
    memory_retention as wire,
    methods::{self, Call, ResultValue},
};

fn id(s: &str) -> methods::Id {
    s.to_owned().try_into().unwrap()
}
fn current(f: &super::mcp::Fixture) -> Task {
    f.host
        .snapshot()
        .unwrap()
        .record(
            Collection::Task,
            f.config.root_task.as_str(),
            &f.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn public_retention_owned_mcp_drain_survives_abandoned_waiter_and_connection_loss() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for lose_connection in [false, true] {
            let f = super::mcp::Fixture::new(backend, "normal").await;
            let task = current(&f);
            f.host
                .stop(
                    f.host
                        .control_envelope(
                            CommandId::new(),
                            task.scope.task,
                            task.revision,
                            Command::Transition {
                                next: TaskState::Paused,
                                reason: "public retention fixture".into(),
                                verification: None,
                            },
                        )
                        .unwrap(),
                )
                .unwrap();
            tokio::time::timeout(Duration::from_secs(10), async {
                while !f
                    .host
                    .lifecycle()
                    .inspect(f.thread)
                    .unwrap()
                    .interrupt_complete
                {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            let workspace: Workspace = f
                .host
                .snapshot()
                .unwrap()
                .record(
                    Collection::Workspace,
                    f.config.workspace.as_str(),
                    &f.config.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            let access = Access {
                actor: f.config.actor.clone(),
                workspace: f.config.workspace.clone(),
                session: f.config.session.clone(),
                authority: workspace.authority,
                read: true,
                write: true,
                bootstrap: false,
            };
            let mut controller = f.host.public_connection(access.clone()).unwrap();
            controller.acquire(CommandId::new(), None).unwrap();
            let scope = methods::Scope {
                workspace: id(f.config.workspace.as_str()),
                session: id(f.config.session.as_str()),
            };
            let task = current(&f);
            controller
                .resume_canonical(
                    methods::SessionResume {
                        scope: scope.clone(),
                        mutation: methods::Mutation {
                            command_id: id("retention-initial-resume"),
                            expected_revision: task.revision.get().into(),
                            steering_revision: task.steering.get().into(),
                        },
                        task: id(f.config.root_task.as_str()),
                    },
                    &access,
                )
                .unwrap();
            assert_eq!(f.list().await["connected"], true);
            assert!(f.host.mcp_connections_present());
            let ResultValue::RetentionPreview(preview) = controller
                .call(
                    Call::MemoryForgetPreview(wire::PreviewRequest {
                        scope: scope.clone(),
                        task: id(f.config.root_task.as_str()),
                        selector: serde_json::from_value(
                            serde_json::to_value(vcp_domain::retention_selector::Selector {
                                schema_version: 1,
                                tree: vcp_domain::retention_selector::Tree::Match(
                                    vcp_domain::retention_selector::Criterion::Task(
                                        f.config.root_task.clone(),
                                    ),
                                ),
                            })
                            .unwrap(),
                        )
                        .unwrap(),
                        action: wire::Action::Exclude,
                        limit: 128,
                    }),
                    &access,
                )
                .await
                .unwrap()
            else {
                panic!("preview")
            };
            let task = current(&f);
            let command = CommandId::parse("retention-owned-forget").unwrap();
            let call = Call::MemoryForget(methods::MemoryForget {
                scope: scope.clone(),
                mutation: methods::Mutation {
                    command_id: id(command.as_str()),
                    expected_revision: task.revision.get().into(),
                    steering_revision: task.steering.get().into(),
                },
                task: id(f.config.root_task.as_str()),
                preview: preview.preview,
                preview_digest: preview.digest.as_str().into(),
            });
            let gate = f.host.admit_turn_start_for_thread(f.thread).unwrap();
            let mut pending = Box::pin(controller.call(call.clone(), &access));
            std::future::poll_fn(|cx| match pending.as_mut().poll(cx) {
                Poll::Pending => Poll::Ready(()),
                Poll::Ready(r) => panic!("completed before gate: {r:?}"),
            })
            .await;
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    if f.host.snapshot().unwrap().commands.contains_key(
                        &vcp_store::contract::command_key(&f.config.workspace, &command),
                    ) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            assert!(f.host.lifecycle().inspect(f.thread).unwrap().local_hold);
            assert_eq!(current(&f).state, TaskState::Paused);
            drop(pending);
            let mut controller = Some(controller);
            let disconnect = if lose_connection {
                Some(controller.take().unwrap().disconnect().unwrap())
            } else {
                None
            };
            drop(gate);
            if let Some(waiter) = disconnect {
                waiter.wait().await.unwrap();
            }
            tokio::time::timeout(Duration::from_secs(10), async {
                while !f
                    .host
                    .lifecycle()
                    .inspect(f.thread)
                    .unwrap()
                    .interrupt_complete
                {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            let mut replacement = f.host.public_connection(access.clone()).unwrap();
            if lose_connection {
                let ResultValue::Controller(view) = replacement
                    .call(
                        Call::ControllerRead(methods::ControllerRead {
                            scope: scope.clone(),
                        }),
                        &access,
                    )
                    .await
                    .unwrap()
                else {
                    panic!("controller view")
                };
                replacement
                    .acquire(
                        CommandId::new(),
                        view.revision
                            .map(|value| Revision::new(value.as_str().parse().unwrap())),
                    )
                    .unwrap();
            }
            let recipient = if lose_connection {
                &mut replacement
            } else {
                controller.as_mut().unwrap()
            };
            let ResultValue::Forgotten(result) =
                recipient.call(call.clone(), &access).await.unwrap()
            else {
                panic!("forget result")
            };
            assert_eq!(result.acceptance.command_id, id(command.as_str()));
            assert!(!result.job.cleanup_required);
            assert!(!f.host.mcp_connections_present());
            assert_eq!(current(&f).state, TaskState::Paused);
            assert!(!f.workspace.join("writes.jsonl").exists());
            let before = f.host.snapshot().unwrap();
            recipient.call(call, &access).await.unwrap();
            let after = f.host.snapshot().unwrap();
            assert_eq!(before.watermark, after.watermark);
            if lose_connection {
                replacement.disconnect().unwrap().wait().await.unwrap();
            } else {
                controller
                    .take()
                    .unwrap()
                    .disconnect()
                    .unwrap()
                    .wait()
                    .await
                    .unwrap();
            }
            f.close().await;
        }
    }
}
