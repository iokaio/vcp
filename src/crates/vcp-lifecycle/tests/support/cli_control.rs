// SPDX-License-Identifier: Apache-2.0
use super::*;
use core_test_support::streaming_sse::{start_streaming_sse_server, StreamingSseChunk};

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
            host.command(Command::SetWorkspaceTrust { trust: Trust::Trusted }, None, Revision::ZERO).unwrap();
            let (release, gate) = tokio::sync::oneshot::channel();
            let (server, _) = start_streaming_sse_server(vec![vec![
                StreamingSseChunk { gate: None, body: sse(vec![ev_response_created("cli"), ev_message_item_added("partial", ""), ev_output_text_delta("still running")]) },
                StreamingSseChunk { gate: Some(gate), body: sse(vec![ev_completed_with_tokens("cli", 7)]) },
            ]]).await;
            let mut registry = ExtensionRegistryBuilder::new();
            registry.turn_start_admission(Arc::new(host.clone()));
            registry.work_admission(Arc::new(host.clone()));
            let starter = host.clone();
            let cwd = workspace.clone();
            let model = config.price.model.clone();
            let test = test_codex()
                .with_extensions(Arc::new(registry.build()))
                .with_auth(codex_login::CodexAuth::from_api_key("synthetic-cli-control"))
                .with_allowed_tools(AllowedTools(vec![]))
                .with_config(move |c| {
                    c.cwd = cwd.try_into().unwrap();
                    c.model = Some(model.clone());
                    configure_fixture_provider(c);
                    starter.lifecycle().authorize_startup(c.cwd.as_path(), None).unwrap();
                })
                .build_with_streaming_server(&server).await.unwrap();
            let thread = host.lifecycle().attach_root(test.codex.clone()).unwrap();
            host.register(thread, binding).unwrap();
            test.codex.start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                text: "stream until owner control".into(), text_elements: vec![],
            }])).await.unwrap();
            tokio::time::timeout(Duration::from_secs(15), wait_for_event(&test.codex,
                |e| matches!(e, EventMsg::AgentMessageContentDelta(_)))).await.unwrap();
            let command = host.control_envelope(CommandId::new(), config.root_task.clone(), Revision::new(1),
                Command::Transition { next, reason: "explicit CLI control".into(), verification: None }).unwrap();
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
            let task: Task = host.snapshot().unwrap().record(Collection::Task, config.root_task.as_str(), &config.workspace).unwrap().decode().unwrap();
            assert_eq!(task.state, next);
            if next == TaskState::Paused {
                let repeat = host.control_envelope(CommandId::new(), config.root_task.clone(), task.revision,
                    Command::Transition { next, reason: "repeat CLI pause".into(), verification: None }).unwrap();
                host.stop(repeat).unwrap();
                let after: Task = host.snapshot().unwrap().record(Collection::Task, config.root_task.as_str(), &config.workspace).unwrap().decode().unwrap();
                assert_eq!(task, after);
            }
            tokio::time::timeout(Duration::from_secs(15), async {
                while !host.lifecycle().inspect(thread).unwrap().interrupt_complete {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }).await.unwrap();
            assert!(matches!(test.codex.start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                text: "must not dispatch".into(), text_elements: vec![],
            }])).await.unwrap(), codex_core::TurnInputSubmission::NotSubmitted { .. }));
            assert_eq!(server.requests().await.len(), 1);
            let _ = release.send(());
            owner.close().await.unwrap();
            test.codex.shutdown_and_wait().await.unwrap();
            server.shutdown().await;
        }
    }
}
