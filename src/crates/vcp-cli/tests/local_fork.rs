// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
//! Atomic metadata fork through the compiled authenticated bridge. The recorded
//! completed boundary is seeded with real engine commands and a real capture;
//! no retained provider or executable root is constructed by this test.
#[path = "support/local_fixture.rs"]
mod local_fixture;
use local_fixture::*;
use serde_json::{json, Value};
use vcp_domain::{
    artifact::{ArtifactSpec, Channel},
    ids::*,
    revision::*,
    task::{Objective, Task, TaskState, Turn, TurnState},
    verification::Fingerprint,
    workspace::{Scope, Session},
};
use vcp_engine::{Access, Engine, HostFacts};
use vcp_protocol::{
    command::{Command, CommandEnvelope},
    event::EventKind,
};
use vcp_store::{
    artifact::ArtifactWriter,
    contract::{Collection, State},
    BackendKind, Store,
};

async fn command(
    engine: &mut Engine<Store>,
    fixture: &Fixture,
    task: &TaskId,
    payload: Command,
    expected: u64,
    steering: u64,
) {
    let config = &fixture.config;
    let access = Access {
        actor: config.actor.clone(),
        workspace: config.workspace.clone(),
        session: config.session.clone(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: false,
    };
    let envelope = CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: config.workspace.clone(),
        session: config.session.clone(),
        task: Some(task.clone()),
        caller: config.actor.clone(),
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected: Revision::new(expected),
        steering: SteeringRevision::new(steering),
        payload,
    };
    engine
        .handle(
            envelope,
            &access,
            &HostFacts {
                may_execute: true,
                ..HostFacts::inspect(Timestamp::new(10))
            },
        )
        .await
        .unwrap();
}

async fn completed_boundary(fixture: &Fixture) -> (TaskId, TurnId, State) {
    let mut engine = Engine::new(fixture.reopen().await).unwrap();
    let task = TaskId::parse("fork-source-task").unwrap();
    let objective = |text: &str| Objective {
        text: text.into(),
        constraints: vec!["preserve source files".into()],
        acceptance: vec!["retain evidence".into()],
        source: EventId::new(),
        steering: SteeringRevision::ZERO,
    };
    command(
        &mut engine,
        fixture,
        &task,
        Command::CreateTask {
            root: task.clone(),
            parent: None,
            fork_origin: None,
            objective: objective("objective at completed boundary"),
            fingerprint: Fingerprint {
                repository: "a".repeat(64),
                buffers: "b".repeat(64),
                environment: "c".repeat(64),
            },
            editing: false,
            required_checks: vec![],
        },
        0,
        0,
    )
    .await;
    command(
        &mut engine,
        fixture,
        &task,
        Command::Transition {
            next: TaskState::Running,
            reason: "offline canonical boundary fixture".into(),
            verification: None,
        },
        0,
        0,
    )
    .await;
    let mut writer = engine
        .store()
        .spool()
        .create(ArtifactSpec {
            id: ArtifactId::new(),
            scope: Scope {
                workspace: fixture.config.workspace.clone(),
                session: fixture.config.session.clone(),
                task: task.clone(),
            },
            media_type: "text/plain".into(),
            schema: "coding-turn-input/1".into(),
            source: "offline compiled fork fixture".into(),
            channel: Channel::Evidence,
            retention: "history".into(),
            omissions: vec![],
        })
        .unwrap();
    writer.write_chunk(b"recorded boundary input").unwrap();
    let artifact = writer.finalize().unwrap();
    drop(writer);
    command(
        &mut engine,
        fixture,
        &task,
        Command::AttachArtifact {
            descriptor: artifact.clone(),
        },
        0,
        0,
    )
    .await;
    let turn = TurnId::parse("fork-through-turn").unwrap();
    command(
        &mut engine,
        fixture,
        &task,
        Command::StartTurn {
            id: turn.clone(),
            trigger: artifact.spec.id,
        },
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
        command(
            &mut engine,
            fixture,
            &task,
            Command::AdvanceTurn {
                id: turn.clone(),
                next,
                reason: "recorded offline completion".into(),
            },
            revision as u64,
            0,
        )
        .await;
    }
    command(
        &mut engine,
        fixture,
        &task,
        Command::Steer {
            objective: objective("later source guidance must not be inherited"),
        },
        1,
        0,
    )
    .await;
    command(
        &mut engine,
        fixture,
        &task,
        Command::ObserveFingerprint {
            fingerprint: Fingerprint {
                repository: "d".repeat(64),
                buffers: "e".repeat(64),
                environment: "f".repeat(64),
            },
        },
        2,
        1,
    )
    .await;
    command(
        &mut engine,
        fixture,
        &task,
        Command::Transition {
            next: TaskState::Paused,
            reason: "serve metadata without execution".into(),
            verification: None,
        },
        3,
        1,
    )
    .await;
    let state = engine.store().state().clone();
    engine.into_store().close().await.unwrap();
    (task, turn, state)
}

fn initialize(client: &mut Client) {
    let methods = [
        "session/fork",
        "session/read",
        "task/read",
        "command/read",
        "controller/read",
        "controller/acquire",
    ];
    let response = client.rpc(1, "initialize", json!({"protocol_version":"1.0", "client":{"name":"compiled-fork","version":"1"},"capabilities":methods,"required_capabilities":methods}));
    assert!(response.get("error").is_none(), "{response}");
}
fn acquire(client: &mut Client, fixture: &Fixture, name: &str) {
    let lease = client.rpc(2, "controller/read", json!({"scope":fixture.scope()}));
    assert!(lease.get("error").is_none(), "{lease}");
    accepted(&client.rpc(3, "controller/acquire", json!({"scope":fixture.scope(),"command_id":name,"expected_revision":lease["result"]["value"]["revision"]})));
}
fn read_task(client: &mut Client, fixture: &Fixture, task: &TaskId, request: u64) -> Value {
    let response = client.rpc(
        request,
        "task/read",
        json!({"scope":fixture.scope(),"task":task}),
    );
    assert!(response.get("error").is_none(), "{response}");
    response["result"].clone()
}

fn assert_metadata_only(
    state: &State,
    fixture: &Fixture,
    source: &TaskId,
    through: &TurnId,
    before: &State,
) {
    let target: Task = state
        .record(Collection::Task, "forked-root", &fixture.config.workspace)
        .unwrap()
        .decode()
        .unwrap();
    // Closing/reopening the source owner retains its existing safety behavior:
    // Pending roots become Paused, even though the fork itself never ran them.
    assert_eq!(target.state, TaskState::Paused);
    assert_eq!(target.root, target.scope.task);
    assert_eq!(target.parent, None);
    assert_eq!(target.fork_origin.as_ref(), Some(source));
    assert_eq!(target.steering, SteeringRevision::ZERO);
    assert_eq!(target.objectives.len(), 1);
    assert_eq!(target.objectives[0].text, "objective at completed boundary");
    assert_eq!(target.fingerprint.repository, "a".repeat(64));
    let session: Session = state
        .record(
            Collection::Session,
            "forked-session",
            &fixture.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(session.fork_origin.as_ref(), Some(&fixture.config.session));
    assert_eq!(session.fork_through.as_ref(), Some(through));
    let receipt = state
        .commands
        .values()
        .find(|receipt| receipt.command.as_str() == "compiled-fork-once")
        .unwrap();
    let events: Vec<_> = state
        .events
        .iter()
        .filter(|event| event.watermark == receipt.watermark)
        .collect();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event.session, fixture.config.session);
    assert_eq!(events[0].event.correlation, receipt.command);
    assert_eq!(events[0].sequence, receipt.first_event);
    assert_eq!(receipt.first_event, receipt.last_event);
    assert_eq!(events[1].event.kind, EventKind::TaskCreated);
    assert_eq!(events[1].event.session, session.id);
    assert_ne!(events[1].event.correlation, receipt.command);
    assert_eq!(
        events[1].event.causation.as_ref(),
        Some(&events[0].event.id)
    );
    let facts = events[1].event.data["facts"].as_array().unwrap();
    let genesis: Task = serde_json::from_value(
        facts
            .iter()
            .find(|fact| fact["collection"] == "task")
            .unwrap()["value"]
            .clone(),
    )
    .unwrap();
    assert_eq!(genesis.state, TaskState::Pending);
    assert_eq!(genesis.revision, Revision::ZERO);
    assert_eq!(genesis.cause, events[1].event.id);
    assert_eq!(genesis.objectives[0].source, events[1].event.id);
    assert_eq!(
        state
            .record(Collection::Task, source.as_str(), &fixture.config.workspace)
            .unwrap(),
        before
            .record(Collection::Task, source.as_str(), &fixture.config.workspace)
            .unwrap()
    );
    assert_eq!(
        state
            .records
            .values()
            .filter(|record| record.collection == Collection::Task)
            .count(),
        before
            .records
            .values()
            .filter(|record| record.collection == Collection::Task)
            .count()
            + 1
    );
    assert_eq!(
        state
            .records
            .values()
            .filter(|record| record.collection == Collection::Turn)
            .count(),
        before
            .records
            .values()
            .filter(|record| record.collection == Collection::Turn)
            .count()
    );
    assert!(!state.records.values().any(|record| matches!(
        record.collection,
        Collection::Attempt
            | Collection::Effect
            | Collection::Ledger
            | Collection::Reservation
            | Collection::Approval
    )));
    let boundary: Turn = state
        .record(
            Collection::Turn,
            through.as_str(),
            &fixture.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(boundary.state, TurnState::Completed);
    assert_eq!(
        state
            .commands
            .values()
            .filter(|receipt| receipt.command.as_str() == "compiled-fork-once")
            .count(),
        1
    );
    assert!(state
        .record(Collection::Task, "changed-root", &fixture.config.workspace)
        .is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_session_fork_is_atomic_metadata_and_replays_only_for_current_controller() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let fixture = Fixture::new(backend).await;
        let (source, through, before) = completed_boundary(&fixture).await;
        let request = json!({"scope":fixture.scope(),"mutation":{"command_id":"compiled-fork-once","expected_revision":"0","steering_revision":"0"},"new_session":"forked-session","new_task":"forked-root","through_turn":through});
        let mut client = Client::connect(&fixture, "controller");
        initialize(&mut client);
        assert!(client
            .rpc(4, "session/fork", request.clone())
            .get("error")
            .is_some());
        acquire(&mut client, &fixture, "fork-owner-one");
        let source_view = read_task(&mut client, &fixture, &source, 5);
        let first = client.rpc(6, "session/fork", request.clone());
        accepted(&first);
        assert_eq!(
            client.rpc(7, "session/fork", request.clone())["result"],
            first["result"]
        );
        assert_eq!(read_task(&mut client, &fixture, &source, 8), source_view);
        let mut conflict = request.clone();
        conflict["new_task"] = json!("changed-root");
        assert_eq!(
            client.rpc(9, "session/fork", conflict.clone())["error"]["data"]["details"]["code"],
            "COMMAND_CONFLICT"
        );
        assert_eq!(
            client.rpc(
                10,
                "command/read",
                json!({"scope":fixture.scope(),"command_id":"compiled-fork-once"})
            )["result"],
            first["result"]
        );
        let target_scope = json!({"workspace":fixture.config.workspace,"session":"forked-session"});
        assert!(client
            .rpc(
                11,
                "command/read",
                json!({"scope":target_scope,"command_id":"compiled-fork-once"})
            )
            .get("error")
            .is_some());
        assert!(client
            .rpc(12, "session/read", json!({"scope":fixture.scope()}))
            .get("error")
            .is_none());
        assert!(client.finish().await.0.success());
        let store = fixture.reopen().await;
        assert_metadata_only(store.state(), &fixture, &source, &through, &before);
        let forked = store
            .state()
            .record(Collection::Task, "forked-root", &fixture.config.workspace)
            .unwrap()
            .clone();
        store.close().await.unwrap();

        let mut restarted = Client::connect(&fixture, "controller");
        initialize(&mut restarted);
        assert!(restarted
            .rpc(4, "session/fork", request.clone())
            .get("error")
            .is_some());
        acquire(&mut restarted, &fixture, "fork-owner-two");
        assert_eq!(
            restarted.rpc(5, "session/fork", request.clone())["result"],
            first["result"]
        );
        assert_eq!(
            restarted.rpc(6, "session/fork", conflict)["error"]["data"]["details"]["code"],
            "COMMAND_CONFLICT"
        );
        assert_eq!(read_task(&mut restarted, &fixture, &source, 7), source_view);
        assert!(restarted.finish().await.0.success());
        let store = fixture.reopen().await;
        assert_metadata_only(store.state(), &fixture, &source, &through, &before);
        assert_eq!(
            store
                .state()
                .record(Collection::Task, "forked-root", &fixture.config.workspace)
                .unwrap(),
            &forked
        );
        store.close().await.unwrap();

        let mut observer = Client::connect(&fixture, "observer");
        initialize(&mut observer);
        assert_eq!(
            observer.rpc(
                2,
                "command/read",
                json!({"scope":fixture.scope(),"command_id":"compiled-fork-once"})
            )["result"],
            first["result"]
        );
        assert!(observer
            .rpc(3, "session/fork", request)
            .get("error")
            .is_some());
        assert_eq!(read_task(&mut observer, &fixture, &source, 4), source_view);
        assert!(observer.finish().await.0.success());
    }
}
