// SPDX-License-Identifier: Apache-2.0
use super::*;
use codex_extension_api::{
    HostModelPurpose, HostResponseCapture, HostWorkAdmission, HostWorkKind, HostWorkPermit,
    ToolName,
};
use core_test_support::streaming_sse::{start_streaming_sse_server, StreamingSseChunk};
use std::sync::Mutex;

struct CaptureGate {
    host: CanonicalHost,
    captures: Arc<Mutex<Vec<HostResponseCapture>>>,
}
impl std::fmt::Debug for CaptureGate {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output.debug_struct("CaptureGate").finish_non_exhaustive()
    }
}
impl HostWorkAdmission for CaptureGate {
    fn requires_completed_response(&self) -> bool {
        self.host.requires_completed_response()
    }
    fn admit_startup(
        &self,
        workspace: &std::path::Path,
        resumed: Option<codex_protocol::ThreadId>,
    ) -> Result<Box<dyn Send>, String> {
        self.host.admit_startup(workspace, resumed)
    }
    fn admit(
        &self,
        thread: codex_protocol::ThreadId,
        kind: HostWorkKind,
        label: &str,
    ) -> Result<Box<dyn HostWorkPermit>, String> {
        self.host.admit(thread, kind, label)
    }
    fn admit_model(
        &self,
        thread: codex_protocol::ThreadId,
        body: &mut serde_json::Value,
        purpose: HostModelPurpose,
    ) -> Result<Box<dyn HostWorkPermit>, String> {
        let permit = self.host.admit_model(thread, body, purpose)?;
        self.captures.lock().unwrap().push(
            permit
                .response_capture()
                .ok_or("response capture missing")?,
        );
        self.captures.lock().unwrap().push(
            permit
                .response_error_capture()
                .ok_or("error capture missing")?,
        );
        Ok(permit)
    }
    fn admit_tool(
        &self,
        thread: codex_protocol::ThreadId,
        call_id: &str,
        name: &ToolName,
    ) -> Result<Box<dyn HostWorkPermit>, String> {
        self.host.admit_tool(thread, call_id, name)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cli_control_authenticates_before_stopping_and_retries_without_another_effect() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for next in [TaskState::Paused, TaskState::Cancelled] {
            let temp = tempfile::tempdir().unwrap();
            let workspace = temp.path().join("workspace");
            std::fs::create_dir(&workspace).unwrap();
            let workspace = workspace.canonicalize().unwrap();
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
            let (release, gate) = tokio::sync::oneshot::channel();
            let (server, _) = start_streaming_sse_server(vec![
                vec![
                    StreamingSseChunk {
                        gate: None,
                        body: sse(vec![
                            ev_response_created("cli"),
                            ev_message_item_added("partial", ""),
                            ev_output_text_delta("still running"),
                        ]),
                    },
                    StreamingSseChunk {
                        gate: Some(gate),
                        body: sse(vec![ev_completed_with_tokens("cli", 7)]),
                    },
                ],
                vec![StreamingSseChunk {
                    gate: None,
                    body: sse(vec![
                        ev_assistant_message("resumed", "resumed safely"),
                        ev_completed_with_tokens("resumed", 7),
                    ]),
                }],
            ])
            .await;
            let captures = Arc::new(Mutex::new(Vec::new()));
            let mut registry = ExtensionRegistryBuilder::new();
            registry.turn_start_admission(Arc::new(host.clone()));
            registry.work_admission(Arc::new(CaptureGate {
                host: host.clone(),
                captures: captures.clone(),
            }));
            let starter = host.clone();
            let cwd = workspace.clone();
            let model = config.price.model.clone();
            let test = test_codex()
                .with_extensions(Arc::new(registry.build()))
                .with_auth(codex_login::CodexAuth::from_api_key(
                    "synthetic-cli-control",
                ))
                .with_allowed_tools(AllowedTools(vec![]))
                .with_config(move |c| {
                    c.cwd = cwd.try_into().unwrap();
                    c.model = Some(model.clone());
                    configure_fixture_provider(c);
                    starter
                        .lifecycle()
                        .authorize_startup(c.cwd.as_path(), None)
                        .unwrap();
                })
                .build_with_streaming_server(&server)
                .await
                .unwrap();
            let thread = host.lifecycle().attach_root(test.codex.clone()).unwrap();
            host.register(thread, binding).unwrap();
            test.codex
                .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                    text: "stream until owner control".into(),
                    text_elements: vec![],
                }]))
                .await
                .unwrap();
            tokio::time::timeout(
                Duration::from_secs(15),
                wait_for_event(&test.codex, |e| {
                    matches!(e, EventMsg::AgentMessageContentDelta(_))
                }),
            )
            .await
            .unwrap();
            let command = host
                .control_envelope(
                    CommandId::new(),
                    config.root_task.clone(),
                    Revision::new(1),
                    Command::Transition {
                        next,
                        reason: "explicit CLI control".into(),
                        verification: None,
                    },
                )
                .unwrap();
            for variant in 0..5 {
                let mut invalid = command.clone();
                match variant {
                    0 => invalid.controller = ControllerId::new(),
                    1 => invalid.owner_epoch = OwnerEpoch::ZERO,
                    2 => invalid.workspace = WorkspaceId::new(),
                    3 => invalid.caller = ActorId::new(),
                    _ => invalid.expected = Revision::ZERO,
                }
                assert!(host.stop(invalid).is_err());
                assert!(!host.lifecycle().inspect(thread).unwrap().local_hold);
            }
            let receipt = host.stop(command.clone()).unwrap();
            assert_eq!(host.stop(command).unwrap(), receipt);
            assert!(host.lifecycle().inspect(thread).unwrap().local_hold);
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
            assert_eq!(task.state, next);
            if next == TaskState::Paused {
                let repeat = host
                    .control_envelope(
                        CommandId::new(),
                        config.root_task.clone(),
                        task.revision,
                        Command::Transition {
                            next,
                            reason: "repeat CLI pause".into(),
                            verification: None,
                        },
                    )
                    .unwrap();
                host.stop(repeat).unwrap();
                let after: Task = host
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
                assert_eq!(task, after);
            }
            tokio::time::timeout(Duration::from_secs(15), async {
                while !host.lifecycle().inspect(thread).unwrap().interrupt_complete {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            assert!(matches!(
                test.codex
                    .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                        text: "must not dispatch".into(),
                        text_elements: vec![],
                    }]))
                    .await
                    .unwrap(),
                codex_core::TurnInputSubmission::NotSubmitted { .. }
            ));
            assert_eq!(server.requests().await.len(), 1);
            // The cancelled response producer has stopped even though no final
            // provider usage arrived. Its money remains reserved as uncertain;
            // that liability must not strand the retained pause forever.
            let retained = host.lifecycle().inspect(thread).unwrap();
            assert_eq!(retained.unresolved_work, 0);
            let state = host.snapshot().unwrap();
            let before = vcp_budget::ledger(&state, &task.scope).unwrap();
            assert!(before.unresolved.get() > 0);
            assert_eq!(before.settled, Micros::ZERO);
            let attempts: Vec<Attempt> = state
                .records
                .values()
                .filter(|row| row.collection == Collection::Attempt)
                .map(|row| row.decode().unwrap())
                .collect();
            assert_eq!(attempts.len(), 1);
            assert_eq!(attempts[0].phase, ReservationState::ReconciliationPending);
            let before_late = host.snapshot().unwrap();
            for capture in captures.lock().unwrap().drain(..) {
                capture(b"late bytes after retained cancellation").unwrap();
            }
            assert_eq!(host.snapshot().unwrap(), before_late);
            if next == TaskState::Paused {
                host.lifecycle().resume(thread, &retained.revision).unwrap();
                assert!(!host.lifecycle().inspect(thread).unwrap().local_hold);
                // Releasing the retained hold cannot bypass canonical pause.
                assert!(matches!(
                    test.codex
                        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                            text: "canonical pause still fences admission".into(),
                            text_elements: vec![],
                        }]))
                        .await
                        .unwrap(),
                    codex_core::TurnInputSubmission::NotSubmitted { .. }
                ));
                let after = host.snapshot().unwrap();
                assert_eq!(vcp_budget::ledger(&after, &task.scope).unwrap(), before);
                assert_eq!(server.requests().await.len(), 1);
                host.resume(thread, task.revision, task.fingerprint.clone())
                    .unwrap();
                turn(&test).await;
                assert_eq!(server.requests().await.len(), 2);
            }
            let _ = release.send(());
            owner.close().await.unwrap();
            test.codex.shutdown_and_wait().await.unwrap();
            server.shutdown().await;
        }
    }
}
