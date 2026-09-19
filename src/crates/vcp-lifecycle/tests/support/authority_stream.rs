// SPDX-License-Identifier: Apache-2.0
use super::*;
use core_test_support::streaming_sse::{start_streaming_sse_server, StreamingSseChunk};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn authority_change_cancels_retained_stream_and_preserves_late_usage_and_paused_child() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let config = config(&temp.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let binding = task(&host, &config, config.root_task.clone(), None);
        let child = task(
            &host,
            &config,
            TaskId::new(),
            Some(config.root_task.clone()),
        );
        host.command(
            Command::Transition {
                next: TaskState::Paused,
                reason: "held child fixture".into(),
                verification: None,
            },
            Some(child.scope.task.clone()),
            Revision::new(1),
        )
        .unwrap();
        host.command(
            Command::SetWorkspaceTrust {
                trust: Trust::Trusted,
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        let (release, gate) = tokio::sync::oneshot::channel();
        let prefix = sse(vec![
            ev_response_created("authority-stream"),
            ev_message_item_added("partial", ""),
            ev_output_text_delta("observed partial output"),
        ]);
        let (server, _) = start_streaming_sse_server(vec![vec![
            StreamingSseChunk {
                gate: None,
                body: prefix.clone(),
            },
            StreamingSseChunk {
                gate: Some(gate),
                body: sse(vec![ev_completed_with_tokens("authority-stream", 7)]),
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
                "synthetic-authority-stream",
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
        let id = host.lifecycle().attach_root(test.codex.clone()).unwrap();
        host.register(id, binding.clone()).unwrap();
        test.codex
            .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                text: "synthetic streaming request".into(),
                text_elements: vec![],
            }]))
            .await
            .unwrap();
        tokio::time::timeout(
            Duration::from_secs(10),
            wait_for_event(&test.codex, |e| {
                matches!(e, EventMsg::AgentMessageContentDelta(_))
            }),
        )
        .await
        .unwrap();
        let command = Command::SetWorkspaceTrust {
            trust: Trust::Untrusted,
        };
        assert!(host
            .command(command.clone(), None, Revision::new(1))
            .unwrap_err()
            .contains("coordinated"));
        assert!(host
            .change_authority(command.clone(), None, Revision::ZERO)
            .is_err());
        assert!(!host.lifecycle().inspect(id).unwrap().local_hold);
        host.change_authority(command, None, Revision::new(1))
            .unwrap()
            .wait()
            .await
            .unwrap();
        let state = host.snapshot().unwrap();
        let workspace_state: Workspace = state
            .record(
                Collection::Workspace,
                config.workspace.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(workspace_state.trust, Trust::Untrusted);
        for task_id in [&config.root_task, &child.scope.task] {
            let task: Task = state
                .record(Collection::Task, task_id.as_str(), &config.workspace)
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(task.state, TaskState::Paused);
        }
        let attempt: Attempt = state
            .records
            .values()
            .find(|r| r.collection == Collection::Attempt)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(attempt.phase, ReservationState::ReconciliationPending);
        let response: ArtifactDescriptor = state
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
            .filter_map(|r| r.decode::<ArtifactDescriptor>().ok())
            .find(|a| a.spec.channel == Channel::Response)
            .unwrap();
        assert_eq!(response.state, CaptureState::Aborted);
        assert_eq!(
            host.read_artifact(response.spec.id).unwrap(),
            prefix.as_bytes()
        );
        let observed = host
            .capture(
                id,
                Channel::Evidence,
                b"synthetic late charge: 120 micros".to_vec(),
            )
            .unwrap();
        host.observe_usage(UsageObservation {
            id: ObservationId::new(),
            scope: binding.scope.clone(),
            attempt: attempt.id,
            provider_request: "authority-stream".into(),
            mode: UsageMode::Cumulative {
                version: Units::new(1),
            },
            amount: Money {
                currency: config.cap.currency.clone(),
                micros: Micros::new(120),
            },
            final_usage: true,
            raw: observed.spec.id,
            correction: None,
        })
        .unwrap();
        assert_eq!(
            host.project().unwrap().ledgers[&config.root_task]
                .settled
                .get(),
            120
        );
        let view = host.lifecycle().inspect(id).unwrap();
        assert!(view.owner_attached && view.local_hold && view.interrupt_complete);
        assert!(host.lifecycle().resume(id, &view.revision).is_err());
        assert!(matches!(
            test.codex
                .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                    text: "must remain paused".into(),
                    text_elements: vec![]
                }]))
                .await
                .unwrap(),
            codex_core::TurnInputSubmission::NotSubmitted { .. }
        ));
        assert_eq!(server.requests().await.len(), 1);
        let _ = release.send(());
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        server.shutdown().await;
    }
}
