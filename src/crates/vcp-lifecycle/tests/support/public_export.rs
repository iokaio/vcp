// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_engine::{rpc::RpcHost, Access};
use vcp_protocol::methods::{self, Call};

fn id(value: &str) -> methods::Id {
    value.to_owned().try_into().unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn public_export_capture_failure_stops_admission_until_recovery() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let mut config = config(&temporary.path().join("canonical"), &workspace, backend);
        config.artifact_limit = ByteCount::new(1);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let access = Access {
            actor: config.actor.clone(),
            workspace: config.workspace.clone(),
            session: config.session.clone(),
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            bootstrap: false,
        };
        let mut controller = host.public_connection(access.clone()).unwrap();
        controller.acquire(CommandId::new(), None).unwrap();
        let binding = task(&host, &config, config.root_task.clone(), None);
        let current: Task = host
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
        let command = CommandId::new();
        let request = methods::SessionExport {
            scope: methods::Scope {
                workspace: id(config.workspace.as_str()),
                session: id(config.session.as_str()),
            },
            mutation: methods::Mutation {
                command_id: id(command.as_str()),
                expected_revision: current.revision.get().into(),
                steering_revision: current.steering.get().into(),
            },
            task: Some(id(config.root_task.as_str())),
            capture: methods::CaptureScope::VisibleHistory,
        };
        let error = controller
            .call(Call::SessionExport(request), &access)
            .await
            .unwrap_err();
        assert_eq!(error.data.unwrap().details["code"], "STORE_UNAVAILABLE");
        let state = host.snapshot().unwrap();
        assert!(!state
            .commands
            .contains_key(&vcp_store::contract::command_key(
                &config.workspace,
                &command
            )));
        let paused: Task = state
            .record(
                Collection::Task,
                config.root_task.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(paused.state, TaskState::Paused);
        assert!(!state
            .records
            .values()
            .any(|r| r.collection == Collection::Artifact));
        let pending = host.unfinished_captures().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].length, ByteCount::ZERO);
        assert_eq!(
            pending[0].spec.schema,
            vcp_store::export_contract::PAYLOAD_SCHEMA
        );
        let registration = host
            .register(codex_protocol::ThreadId::new(), binding.clone())
            .unwrap_err();
        assert!(
            registration.contains("capture/admission fenced"),
            "{registration}"
        );
        assert!(host
            .lifecycle()
            .authorize_startup(&workspace, None)
            .is_err());
        controller.disconnect().unwrap().wait().await.unwrap();
        owner.close().await.unwrap();
        drop(host);
        let (reopened, owner) = CanonicalHost::open(config.clone()).unwrap();
        assert!(reopened
            .register(codex_protocol::ThreadId::new(), binding)
            .is_err());
        assert!(!reopened.snapshot().unwrap().commands.contains_key(
            &vcp_store::contract::command_key(&config.workspace, &command)
        ));
        owner.close().await.unwrap();
    }
}
