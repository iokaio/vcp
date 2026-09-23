// SPDX-License-Identifier: Apache-2.0
//! Public resume must retain exact MCP idle-process approval evidence.
use super::mcp::Fixture;
use super::*;
use vcp_engine::{rpc::RpcHost, Access};
use vcp_lifecycle::foundation::PublicConnection;
use vcp_protocol::methods;

fn task(f: &Fixture) -> Task {
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
fn request(f: &Fixture, command: &str) -> methods::SessionResume {
    let task = task(f);
    methods::SessionResume {
        scope: methods::Scope {
            workspace: f.config.workspace.to_string().try_into().unwrap(),
            session: f.config.session.to_string().try_into().unwrap(),
        },
        mutation: methods::Mutation {
            command_id: command.to_owned().try_into().unwrap(),
            expected_revision: task.revision.get().into(),
            steering_revision: task.steering.get().into(),
        },
        task: task.scope.task.to_string().try_into().unwrap(),
    }
}
fn allow(f: &Fixture, question: &serde_json::Value) {
    f.host
        .command(
            Command::Decide {
                id: ApprovalId::parse(question["question"].as_str().unwrap()).unwrap(),
                operation_digest: question["decision"]["digest"].as_str().unwrap().into(),
                effect_revision: Revision::new(1),
                allow: true,
            },
            Some(f.config.root_task.clone()),
            Revision::ZERO,
        )
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn public_resume_mcp_preserves_exact_approved_process_and_does_not_repeat_submission() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let mut f = Fixture::new(backend, "normal").await;
        let initial = task(&f);
        f.host
            .stop(
                f.host
                    .control_envelope(
                        CommandId::new(),
                        initial.scope.task,
                        initial.revision,
                        Command::Transition {
                            next: TaskState::Paused,
                            reason: "select public owner before MCP startup".into(),
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
        f.policy.revision = PolicyRevision::new(1);
        f.policy.mode = vcp_domain::policy::Autonomy::Ask;
        f.host
            .command(
                Command::SetPolicy {
                    policy: f.policy.clone(),
                },
                None,
                Revision::ZERO,
            )
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
        let mut connection: PublicConnection = f.host.public_connection(access.clone()).unwrap();
        connection.acquire(CommandId::new(), None).unwrap();
        connection
            .resume_canonical(request(&f, "public-mcp-initial"), &access)
            .unwrap();
        assert!(!f.workspace.join("started").exists());
        let startup = f.list().await;
        assert_eq!(startup["decision"]["kind"], "question");
        let denied = request(&f, "public-mcp-startup");
        assert!(connection
            .resume_canonical(denied.clone(), &access)
            .is_err());
        assert!(!f.workspace.join("started").exists());
        allow(&f, &startup);
        let accepted = connection
            .resume_canonical(denied.clone(), &access)
            .unwrap();
        assert_eq!(
            connection.resume_canonical(denied, &access).unwrap(),
            accepted
        );
        assert!(!f.workspace.join("started").exists());
        let catalog = f.list().await;
        assert_eq!(catalog["connected"], true);
        let call = f.call(
            &catalog,
            "write_marker",
            serde_json::json!({"value":"public-once"}),
        );
        let question = f.host.mcp_control(f.thread, call.clone()).await.unwrap();
        assert_eq!(question["decision"]["kind"], "question");
        let resume = request(&f, "public-mcp-call");
        assert!(connection
            .resume_canonical(resume.clone(), &access)
            .is_err());
        allow(&f, &question);
        let receipt = connection
            .resume_canonical(resume.clone(), &access)
            .unwrap();
        assert_eq!(
            connection
                .resume_canonical(resume.clone(), &access)
                .unwrap(),
            receipt
        );
        assert!(
            !f.workspace.join("writes.jsonl").exists(),
            "canonical acceptance never submits the tool call"
        );
        let result = f.host.mcp_control(f.thread, call).await.unwrap();
        assert_eq!(result["outcome"], "succeeded");
        // The pending MCP proof is consumed now. Replay must return before proof
        // lookup, and may never dispatch the already-completed effect again.
        assert_eq!(
            connection.resume_canonical(resume, &access).unwrap(),
            receipt
        );
        assert_eq!(
            std::fs::read_to_string(f.workspace.join("writes.jsonl"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        if backend == BackendKind::Sqlite {
            let token = connection.controller_token().unwrap();
            connection
                .call(
                    methods::Call::ControllerRelease(methods::ControllerRelease {
                        scope: request(&f, "unused").scope,
                        command_id: "public-mcp-release".to_owned().try_into().unwrap(),
                        expected_revision: token.revision().get().into(),
                        generation: token.generation().get().into(),
                    }),
                    &access,
                )
                .await
                .unwrap();
            assert!(!f.host.mcp_connections_present());
        }
        connection.disconnect().unwrap().wait().await.unwrap();
        assert!(!f.host.mcp_connections_present());
        f.close().await;
    }
}
