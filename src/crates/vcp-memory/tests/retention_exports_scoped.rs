// SPDX-License-Identifier: Apache-2.0
use vcp_audit::{history, session_export};
use vcp_domain::{artifact::*, task::*, verification::Fingerprint, workspace::*, *};
use vcp_engine::{public_export::*, Access, Engine, HostFacts};
use vcp_protocol::{command::*, methods};
use vcp_store::{artifact::ArtifactWriter, BackendKind, Store};

fn id(value: &str) -> methods::Id {
    value.to_owned().try_into().unwrap()
}
fn access() -> Access {
    Access {
        actor: ActorId::parse("owner").unwrap(),
        workspace: WorkspaceId::parse("workspace").unwrap(),
        session: SessionId::parse("session").unwrap(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: true,
    }
}
fn disclosure() -> ExportDisclosure {
    ExportDisclosure {
        policy: PolicyRevision::ZERO,
        history: true,
        artifacts: true,
    }
}
fn reader() -> history::Access {
    history::Access {
        workspace: access().workspace,
        authority: AuthorityRevision::ZERO,
        read: true,
        tasks: None,
    }
}
fn request(
    command: &str,
    task: Option<&TaskId>,
    capture: methods::CaptureScope,
) -> methods::SessionExport {
    methods::SessionExport {
        scope: methods::Scope {
            workspace: id("workspace"),
            session: id("session"),
        },
        mutation: methods::Mutation {
            command_id: id(command),
            expected_revision: 0.into(),
            steering_revision: 0.into(),
        },
        task: task.map(|t| id(t.as_str())),
        capture,
    }
}
async fn issue(engine: &mut Engine<Store>, payload: Command, task: Option<TaskId>, expected: u64) {
    let command = CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: access().workspace,
        session: access().session,
        task,
        caller: access().actor,
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected: Revision::new(expected),
        steering: SteeringRevision::ZERO,
        payload,
    };
    engine
        .handle(command, &access(), &HostFacts::inspect(Timestamp::new(1)))
        .await
        .unwrap();
}
async fn fixture(
    path: &std::path::Path,
    backend: BackendKind,
) -> (
    Engine<Store>,
    TaskId,
    TaskId,
    ControllerId,
    vcp_engine::controller::ControllerToken,
) {
    let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
    issue(
        &mut engine,
        Command::Initialize {
            binding: Binding {
                host: HostId::new(),
                root: "C:/export-fixture".into(),
                repository: "fixture".into(),
                worktree: "main".into(),
                revision: Revision::ZERO,
            },
        },
        None,
        0,
    )
    .await;
    let root = TaskId::parse("root").unwrap();
    let other = TaskId::parse("other").unwrap();
    for task in [&root, &other] {
        issue(
            &mut engine,
            Command::CreateTask {
                root: task.clone(),
                parent: None,
                fork_origin: None,
                objective: Objective {
                    text: format!("private-objective-{task}"),
                    constraints: vec![],
                    acceptance: vec!["inspect".into()],
                    source: EventId::new(),
                    steering: SteeringRevision::ZERO,
                },
                fingerprint: Fingerprint {
                    repository: "a".repeat(64),
                    buffers: "b".repeat(64),
                    environment: "c".repeat(64),
                },
                editing: false,
                required_checks: vec![],
            },
            Some(task.clone()),
            0,
        )
        .await;
        let mut writer = engine
            .store()
            .spool()
            .create(ArtifactSpec {
                id: ArtifactId::new(),
                scope: Scope {
                    workspace: access().workspace,
                    session: access().session,
                    task: task.clone(),
                },
                media_type: "application/octet-stream".into(),
                schema: "synthetic-evidence/1".into(),
                source: "fixture".into(),
                channel: Channel::Evidence,
                retention: "history".into(),
                omissions: vec![Omission::AuthenticationHeaders],
            })
            .unwrap();
        writer
            .write_chunk(format!("arbitrary-source-bytes-{task}").as_bytes())
            .unwrap();
        let descriptor = writer.finalize().unwrap();
        issue(
            &mut engine,
            Command::AttachArtifact { descriptor },
            Some(task.clone()),
            0,
        )
        .await;
    }
    let connection = ControllerId::new();
    engine
        .acquire_controller(
            &access(),
            &connection,
            CommandId::new(),
            None,
            Timestamp::new(2),
        )
        .await
        .unwrap();
    let token = engine.controller_token(&access(), &connection).unwrap();
    (engine, root, other, connection, token)
}
fn prepare(
    engine: &Engine<Store>,
    request: methods::SessionExport,
    root: &TaskId,
    connection: &ControllerId,
    token: &vcp_engine::controller::ControllerToken,
) -> PreparedPublicExport {
    match engine
        .prepare_public_export(request, &access(), connection, token, &disclosure(), root)
        .unwrap()
    {
        PublicExportAdmission::Ready(value) => value,
        _ => panic!("expected fresh export"),
    }
}

#[tokio::test]
async fn scoped_source_purge_denies_foreign_task_export_copy_instead_of_dropping_dependency() {
    use std::collections::BTreeSet;
    use vcp_domain::retention_selector::{Criterion, Selector, Tree};
    use vcp_memory::{retention::Action, retention_public};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut engine, root, other, connection, token) = fixture(temp.path(), backend).await;
        let prepared = prepare(
            &engine,
            request(
                "foreign-copy",
                None,
                methods::CaptureScope::VisibleHistoryAndArtifacts,
            ),
            &other,
            &connection,
            &token,
        );
        let rendered = session_export::render(
            engine.store(),
            &reader(),
            prepared.sources(),
            prepared.capture(),
        )
        .unwrap();
        engine
            .commit_public_export(
                prepared,
                &access(),
                &disclosure(),
                rendered,
                Timestamp::new(3),
            )
            .await
            .unwrap();
        for task in [&root, &other] {
            issue(
                &mut engine,
                Command::Transition {
                    next: TaskState::Cancelled,
                    reason: "settled fixture".into(),
                    verification: None,
                },
                Some(task.clone()),
                0,
            )
            .await;
        }
        let store = engine.into_store();
        let before = store.state().clone();
        let auth = vcp_memory::access::Access {
            workspace: access().workspace,
            actor: access().actor,
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            tasks: Some(BTreeSet::from([root.clone()])),
        };
        let selector = Selector {
            schema_version: 1,
            tree: Tree::All(vec![
                Tree::Match(Criterion::Task(root.clone())),
                Tree::Match(Criterion::Event("artifact_attached".into())),
            ]),
        };
        let result = retention_public::preview(
            &store,
            &auth,
            Scope {
                workspace: auth.workspace.clone(),
                session: access().session,
                task: root,
            },
            selector,
            Action::Purge,
            Timestamp::new(10),
        );
        assert!(
            matches!(result, Err(vcp_memory::Error::Access)),
            "foreign copy must not be silently filtered"
        );
        assert_eq!(store.state(), &before);
        store.close().await.unwrap();
    }
}
