// SPDX-License-Identifier: Apache-2.0
use vcp_audit::{history, session_export};
use vcp_domain::{artifact::*, task::*, verification::Fingerprint, workspace::*, *};
use vcp_engine::{public_export::*, Access, Engine, HostFacts};
use vcp_protocol::{command::*, methods};
use vcp_store::contract::{key, Collection};
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
async fn source_purge_physically_removes_both_export_copies_after_source_revision_changes() {
    use vcp_domain::retention_selector::{Criterion, Selector, Tree};
    use vcp_memory::retention::{self, Action, Target};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut engine, root, other, connection, token) = fixture(temp.path(), backend).await;
        let prepared = prepare(
            &engine,
            request(
                "derived-copy",
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
        let payload: serde_json::Value = serde_json::from_slice(&rendered.payload).unwrap();
        assert!(payload["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["bytes"] == serde_json::json!(b"arbitrary-source-bytes-root".to_vec())));
        let outcome = engine
            .commit_public_export(
                prepared,
                &access(),
                &disclosure(),
                rendered,
                Timestamp::new(3),
            )
            .await
            .unwrap();
        let outputs = [
            outcome.view.artifact.clone(),
            outcome.view.visibility_manifest.clone(),
        ];
        let original_outputs: Vec<ArtifactDescriptor> = outputs
            .iter()
            .map(|id| {
                engine
                    .store()
                    .state()
                    .record(Collection::Artifact, id.as_str(), &access().workspace)
                    .unwrap()
                    .decode()
                    .unwrap()
            })
            .collect();
        for original in &original_outputs {
            engine
                .store()
                .spool()
                .read(original, &mut Vec::new())
                .unwrap();
        }
        // Cancelling changes a captured task. Retention must use immutable lineage,
        // not validate_current(), which would reject this changed source.
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
        let mut store = engine.into_store();
        let auth = vcp_memory::access::Access {
            workspace: access().workspace,
            actor: access().actor,
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            tasks: None,
        };
        // Select only the root's original ArtifactAttached, not its Task record or
        // the export's attachment. The export lives on the other task.
        let selector = Selector {
            schema_version: 1,
            tree: Tree::All(vec![
                Tree::Match(Criterion::Task(root.clone())),
                Tree::Match(Criterion::Event("artifact_attached".into())),
            ]),
        };
        let preview =
            retention::preview(&store, &auth, selector, Action::Purge, Timestamp::new(10)).unwrap();
        for output in &outputs {
            assert!(preview
                .dependent
                .contains(&Target::Record(key(Collection::Artifact, output.as_str()))));
        }
        assert!(preview.protected.is_empty(), "{:?}", preview.protected);
        let receipt = retention::apply(&mut store, &auth, &preview, Timestamp::new(11))
            .await
            .unwrap();
        let receipt = retention::cleanup(&mut store, &auth, &receipt.id, Timestamp::new(12))
            .await
            .unwrap();
        assert!(receipt.rewrite_complete && receipt.local_cleanup_complete);
        for original in &original_outputs {
            assert!(
                store.spool().read(original, &mut Vec::new()).is_err(),
                "complete original descriptor must no longer read copied bytes"
            );
        }
        for output in &outputs {
            let descriptor: ArtifactDescriptor = store
                .state()
                .record(Collection::Artifact, output.as_str(), &auth.workspace)
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(descriptor.state, CaptureState::Purged);
            assert!(store.spool().read(&descriptor, &mut Vec::new()).is_err());
        }
        let other: Task = store
            .state()
            .record(Collection::Task, other.as_str(), &auth.workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert!(
            other.redaction.is_none(),
            "copy removal must not purge unrelated owning task"
        );
        store.close().await.unwrap();
    }
}
