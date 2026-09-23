// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    public::PublicError,
    query::{Query, QueryError, QueryResult},
    Access, Engine, HostFacts,
};
use vcp_domain::{
    artifact::{ArtifactSpec, Channel},
    task::Objective,
    verification::Fingerprint,
    workspace::Binding,
};
use vcp_protocol::{
    command::{Command, CommandReceipt},
    methods::{self, Call},
};
use vcp_store::{artifact::ArtifactWriter, BackendKind, Store};
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
fn id(value: &str) -> methods::Id {
    value.to_owned().try_into().unwrap()
}
fn scope() -> methods::Scope {
    methods::Scope {
        workspace: id("workspace"),
        session: id("session"),
    }
}
fn mutation(command: &str, expected: u64, steering: u64) -> methods::Mutation {
    methods::Mutation {
        command_id: id(command),
        expected_revision: expected.into(),
        steering_revision: steering.into(),
    }
}
fn facts() -> HostFacts {
    HostFacts {
        may_execute: true,
        ..HostFacts::inspect(Timestamp::new(10))
    }
}
fn objective(text: &str) -> Objective {
    Objective {
        text: text.into(),
        constraints: vec!["preserve unrelated files".into()],
        acceptance: vec!["cite observed evidence".into()],
        source: EventId::new(),
        steering: SteeringRevision::ZERO,
    }
}
async fn internal(
    engine: &mut Engine<Store>,
    payload: Command,
    task: Option<TaskId>,
    expected: u64,
    steering: u64,
) -> CommandReceipt {
    let access = access();
    let envelope = CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: access.workspace.clone(),
        session: access.session.clone(),
        task,
        caller: access.actor.clone(),
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected: Revision::new(expected),
        steering: SteeringRevision::new(steering),
        payload,
    };
    engine.handle(envelope, &access, &facts()).await.unwrap()
}
async fn setup(path: &std::path::Path, backend: BackendKind) -> Engine<Store> {
    let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
    internal(
        &mut engine,
        Command::Initialize {
            binding: Binding {
                host: HostId::parse("host").unwrap(),
                root: "C:/public-fixture".into(),
                repository: "fixture".into(),
                worktree: "main".into(),
                revision: Revision::ZERO,
            },
        },
        None,
        0,
        0,
    )
    .await;
    engine
}
async fn task(engine: &mut Engine<Store>) -> TaskId {
    let task = TaskId::parse("task").unwrap();
    internal(
        engine,
        Command::CreateTask {
            root: task.clone(),
            parent: None,
            fork_origin: None,
            objective: objective("original"),
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
        0,
    )
    .await;
    task
}

async fn fixture(path: &std::path::Path, backend: BackendKind) -> (Engine<Store>, TurnId) {
    let mut engine = setup(path, backend).await;
    let task = task(&mut engine).await;
    internal(
        &mut engine,
        Command::Transition {
            next: TaskState::Running,
            reason: "explicit synthetic run".into(),
            verification: None,
        },
        Some(task.clone()),
        0,
        0,
    )
    .await;
    let mut capture = engine
        .store()
        .spool()
        .create(ArtifactSpec {
            id: ArtifactId::new(),
            scope: Scope {
                workspace: access().workspace,
                session: access().session,
                task: task.clone(),
            },
            media_type: "text/plain".into(),
            schema: "coding-turn-input/1".into(),
            source: "synthetic fixture".into(),
            channel: Channel::Evidence,
            retention: "history".into(),
            omissions: vec![],
        })
        .unwrap();
    capture.write_chunk(b"original input").unwrap();
    let artifact = capture.finalize().unwrap();
    drop(capture);
    internal(
        &mut engine,
        Command::AttachArtifact {
            descriptor: artifact.clone(),
        },
        Some(task.clone()),
        0,
        0,
    )
    .await;
    let turn = TurnId::parse("through-turn").unwrap();
    internal(
        &mut engine,
        Command::StartTurn {
            id: turn.clone(),
            trigger: artifact.spec.id,
        },
        Some(task.clone()),
        1,
        0,
    )
    .await;
    for (revision, next) in [
        TurnState::AssemblingContext,
        TurnState::ReservingBudget,
        TurnState::RequestingModel,
        TurnState::ProcessingResponse,
        TurnState::Verifying,
        TurnState::Completed,
    ]
    .into_iter()
    .enumerate()
    {
        internal(
            &mut engine,
            Command::AdvanceTurn {
                id: turn.clone(),
                next,
                reason: "synthetic canonical stage".into(),
            },
            Some(task.clone()),
            revision as u64,
            0,
        )
        .await;
    }
    // Later guidance and environment observations must not rewrite the boundary.
    internal(
        &mut engine,
        Command::Steer {
            objective: objective("later objective"),
        },
        Some(task.clone()),
        1,
        0,
    )
    .await;
    internal(
        &mut engine,
        Command::ObserveFingerprint {
            fingerprint: Fingerprint {
                repository: "d".repeat(64),
                buffers: "e".repeat(64),
                environment: "f".repeat(64),
            },
        },
        Some(task),
        2,
        1,
    )
    .await;
    (engine, turn)
}
fn call(command: &str) -> Call {
    Call::SessionFork(methods::SessionFork {
        scope: scope(),
        mutation: mutation(command, 0, 0),
        new_session: id("forked-session"),
        new_task: id("forked-task"),
        through_turn: id("through-turn"),
    })
}

#[tokio::test]
async fn atomic_public_fork_replays_after_restart_and_preserves_historical_metadata_only() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let (engine, through) = fixture(temp.path(), backend).await;
        let before = engine.store().state().clone();
        let shared = std::sync::Arc::new(tokio::sync::Mutex::new(engine));
        let invoke = |shared: std::sync::Arc<tokio::sync::Mutex<Engine<Store>>>| async move {
            let mut engine = shared.lock().await;
            engine
                .handle_public(call("fork-once"), &access(), &facts())
                .await
        };
        let (first, second) = tokio::join!(invoke(shared.clone()), invoke(shared.clone()));
        let receipt = first.unwrap();
        assert_eq!(second.unwrap(), receipt);
        let mut engine = std::sync::Arc::try_unwrap(shared)
            .ok()
            .unwrap()
            .into_inner();
        let after = engine.store().state();
        assert_eq!(after.records.len(), before.records.len() + 2);
        assert_eq!(after.events.len(), before.events.len() + 2);
        assert_eq!(after.commands.len(), before.commands.len() + 1);
        let task: Task = after
            .record(Collection::Task, "forked-task", &access().workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(task.state, TaskState::Pending);
        assert_eq!(task.root, task.scope.task);
        assert!(task.parent.is_none());
        assert_eq!(task.fork_origin, Some(TaskId::parse("task").unwrap()));
        assert_eq!(
            task.objectives,
            vec![Objective {
                source: task.cause.clone(),
                ..objective("original")
            }]
        );
        assert_eq!(task.fingerprint.repository, "a".repeat(64));
        assert_eq!(task.revision, Revision::ZERO);
        assert_eq!(task.steering, SteeringRevision::ZERO);
        let session: Session = after
            .record(Collection::Session, "forked-session", &access().workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(session.fork_through, Some(through));
        assert_eq!(session.fork_origin, Some(access().session));
        let events = &after.events[before.events.len()..];
        assert_eq!(events[0].event.session, access().session);
        assert_eq!(events[0].event.correlation, receipt.command);
        assert_eq!(events[1].event.session, session.id);
        assert_ne!(events[1].event.correlation, receipt.command);
        assert_eq!(events[1].event.causation, Some(events[0].event.id.clone()));
        assert_eq!(events[1].event.id, task.cause);
        assert_eq!(events[1].sequence, SessionSeq::new(1));
        assert_eq!(receipt.first_event, events[0].sequence);
        assert_eq!(receipt.last_event, events[0].sequence);
        for (key, record) in &before.records {
            assert_eq!(after.records.get(key), Some(record));
        }
        assert!(matches!(
            engine.query(
                &access(),
                &Query::Command {
                    command: receipt.command.clone()
                }
            ),
            Ok(QueryResult::Command { .. })
        ));
        let mut target_access = access();
        target_access.session = session.id;
        assert_eq!(
            engine.query(
                &target_access,
                &Query::Command {
                    command: receipt.command.clone()
                }
            ),
            Err(QueryError::Unavailable)
        );
        let snapshot = engine.store().state().clone();
        assert_eq!(
            engine
                .handle_public(call("fork-once"), &access(), &facts())
                .await
                .unwrap(),
            receipt
        );
        let mut changed = call("fork-once");
        if let Call::SessionFork(p) = &mut changed {
            p.new_task = id("changed");
        }
        assert_eq!(
            engine.handle_public(changed, &access(), &facts()).await,
            Err(PublicError::CommandConflict)
        );
        assert_eq!(engine.store().state(), &snapshot);
        engine.into_store().close().await.unwrap();
        let mut engine =
            Engine::new(Store::open(temp.path(), backend, &[]).await.unwrap()).unwrap();
        assert_eq!(
            engine
                .handle_public(call("fork-once"), &access(), &facts())
                .await
                .unwrap(),
            receipt
        );
        let mut denied = access();
        denied.write = false;
        assert_eq!(
            engine
                .handle_public(call("fork-once"), &denied, &facts())
                .await,
            Err(PublicError::Access)
        );
        assert_eq!(engine.store().state(), &snapshot);
        engine.into_store().close().await.unwrap();
    }
}

#[tokio::test]
async fn rejected_boundaries_and_target_collisions_never_leave_a_partial_fork() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let (mut engine, through) = fixture(temp.path(), backend).await;
        let before = engine.store().state().clone();
        for variant in 0..5 {
            let mut request = call(&format!("rejected-{variant}"));
            if let Call::SessionFork(p) = &mut request {
                match variant {
                    0 => p.new_task = id("task"),
                    1 => p.new_session = id("session"),
                    2 => p.through_turn = id("absent"),
                    3 => p.mutation.expected_revision = 1.into(),
                    _ => p.mutation.steering_revision = 1.into(),
                }
            }
            assert!(engine
                .handle_public(request, &access(), &facts())
                .await
                .is_err());
            assert_eq!(engine.store().state(), &before);
        }
        let selected = source(&before, &access().workspace, &access().session, &through).unwrap();
        for variant in 0..7 {
            let mut altered = before.clone();
            match variant {
                0 => {
                    altered
                        .events
                        .retain(|event| event.event.id != selected.turn.cause);
                }
                1 => {
                    altered
                        .events
                        .iter_mut()
                        .find(|event| event.event.id == selected.turn.cause)
                        .unwrap()
                        .event
                        .session = SessionId::parse("foreign").unwrap();
                }
                2 => {
                    altered
                        .events
                        .iter_mut()
                        .find(|event| event.event.id == selected.turn.cause)
                        .unwrap()
                        .event
                        .data = serde_json::json!({"schema_version":1,"facts":[]});
                }
                3 => {
                    for event in &mut altered.events {
                        if matches!(
                            event.event.kind,
                            EventKind::TaskCreated
                                | EventKind::TaskTransition
                                | EventKind::ObjectiveChanged
                                | EventKind::FingerprintObserved
                        ) {
                            event.event.data = serde_json::json!({"schema_version":1,"facts":[]});
                        }
                    }
                }
                4 => {
                    let mask = RetentionMask {
                        schema_version: 1,
                        workspace: access().workspace,
                        session: access().session,
                        first: SessionSeq::ZERO,
                        last: SessionSeq::new(u64::MAX),
                        artifacts: vec![],
                        deletion: DeletionEpoch::ZERO,
                        reason: "logical suppression".into(),
                    };
                    let record = Record::typed(
                        Collection::Tombstone,
                        "mask",
                        access().workspace,
                        Revision::ZERO,
                        &mask,
                    )
                    .unwrap();
                    altered.records.insert(record.key(), record);
                }
                5 => {
                    altered
                        .records
                        .get_mut(&key(Collection::Turn, through.as_str()))
                        .unwrap()
                        .value["redaction"] =
                        serde_json::json!({"deletion":"1","original_digest":"a".repeat(64)});
                }
                _ => {
                    let row = altered
                        .records
                        .get_mut(&key(Collection::Artifact, selected.turn.trigger.as_str()))
                        .unwrap();
                    let mut descriptor: ArtifactDescriptor = row.decode().unwrap();
                    descriptor.state = CaptureState::Purged;
                    descriptor.retained.clear();
                    row.value = serde_json::to_value(descriptor).unwrap();
                }
            }
            assert!(
                matches!(
                    source(&altered, &access().workspace, &access().session, &through),
                    Err(Error::Unavailable)
                ),
                "variant {variant}"
            );
        }
        engine.into_store().close().await.unwrap();
    }
}
