// SPDX-License-Identifier: Apache-2.0
use super::tests::{access, init, request, send, server, setup};
use super::*;
use serde_json::json;
use vcp_domain::{
    effect::{Effect, EffectState},
    revision::*,
    task::*,
    verification::Fingerprint,
};
use vcp_protocol::{
    command::{Approval, ApprovalState, Command, CommandEnvelope},
    event::{EventEnvelope, EventInput, EventKind},
};
use vcp_store::{
    contract::{Collection, Record, State},
    BackendKind, Store,
};

async fn create_task(engine: &mut Engine<Store>) -> Task {
    let grant = access();
    let task = TaskId::parse("task").unwrap();
    engine
        .handle(
            CommandEnvelope {
                version: 1,
                id: CommandId::new(),
                workspace: grant.workspace.clone(),
                session: grant.session.clone(),
                task: Some(task.clone()),
                caller: grant.actor.clone(),
                controller: engine.controller().clone(),
                owner_epoch: engine.owner_epoch(),
                expected: Revision::ZERO,
                steering: SteeringRevision::ZERO,
                payload: Command::CreateTask {
                    root: task.clone(),
                    parent: None,
                    fork_origin: None,
                    objective: Objective {
                        text: "inspect public task".into(),
                        constraints: vec![],
                        acceptance: vec![],
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
            },
            &grant,
            &HostFacts::inspect(Timestamp::new(1)),
        )
        .await
        .unwrap();
    match engine.query(&grant, &Query::Task { task }).unwrap() {
        QueryResult::Task { task, .. } => task,
        _ => unreachable!(),
    }
}
fn insert<T: Serialize>(
    state: &mut State,
    collection: Collection,
    name: &str,
    task: &Task,
    revision: Revision,
    value: &T,
) {
    let record = Record::typed(
        collection,
        name,
        task.scope.workspace.clone(),
        revision,
        value,
    )
    .unwrap();
    state.records.insert(record.key(), record);
}
fn effect(task: &Task, state: EffectState) -> Effect {
    Effect {
        id: ToolRunId::parse("effect").unwrap(),
        scope: task.scope.clone(),
        revision: Revision::ZERO,
        steering: task.steering,
        state,
        operation_digest: "a".repeat(64),
        execution: None,
        exit_code: None,
        observed_changes: vec![],
        cause: EventId::new(),
        reason: "observed effect".into(),
        redaction: None,
    }
}
fn approval(task: &Task, name: &str) -> Approval {
    Approval {
        id: ApprovalId::parse(name).unwrap(),
        scope: task.scope.clone(),
        effect: ToolRunId::parse("effect").unwrap(),
        effect_revision: Revision::ZERO,
        steering: task.steering,
        operation_digest: "a".repeat(64),
        actor: ActorId::parse("owner").unwrap(),
        policy: PolicyRevision::ZERO,
        expires_at: Timestamp::new(2),
        state: ApprovalState::Pending,
        revision: Revision::new(u64::MAX),
        controller: None,
        owner_epoch: None,
        authority: None,
        binding: None,
    }
}
fn put_turn(state: &mut State, task: &Task, name: &str, sequence: u64) -> Turn {
    let cause = EventId::new();
    let turn = Turn {
        id: TurnId::parse(name).unwrap(),
        scope: task.scope.clone(),
        revision: Revision::ZERO,
        steering: task.steering,
        state: TurnState::Queued,
        trigger: ArtifactId::new(),
        cause: cause.clone(),
        reason: "queued".into(),
        redaction: None,
    };
    insert(state, Collection::Turn, name, task, turn.revision, &turn);
    state.events.push(EventEnvelope {
        version: 1,
        sequence: SessionSeq::new(sequence),
        watermark: Watermark::new(sequence),
        redaction: None,
        event: EventInput {
            id: cause,
            workspace: task.scope.workspace.clone(),
            session: task.scope.session.clone(),
            task: Some(task.scope.task.clone()),
            actor: access().actor,
            correlation: CommandId::new(),
            causation: None,
            timestamp: Timestamp::new(1),
            kind: EventKind::TurnTransition,
            artifacts: vec![],
            metadata: None,
            data: json!({"schema_version":1,"facts":[{"collection":"turn","id":name,"revision":"0","value":turn}]}),
        },
    });
    turn
}

#[tokio::test]
async fn observer_task_read_is_scoped_and_does_not_write_either_store() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let mut engine = setup(temp.path(), backend).await;
        create_task(&mut engine).await;
        let before = engine.store().state().clone();
        let mut observer = access();
        observer.write = false;
        observer.bootstrap = false;
        let mut rpc = RpcSession::new(server()).unwrap();
        send(&mut rpc, &mut engine, &observer, init())
            .await
            .unwrap();
        let read = request(
            1,
            "task/read",
            json!({"scope":{"workspace":"workspace","session":"session"},"task":"task"}),
        );
        let response = send(&mut rpc, &mut engine, &observer, read.clone())
            .await
            .unwrap();
        assert_eq!(response["result"]["value"]["task"], "task");
        assert_eq!(response["result"]["value"]["effects"], "known");
        assert_eq!(response["result"]["value"]["pending_inputs"], json!([]));
        let mut wrong = read.clone();
        wrong["params"]["scope"]["session"] = json!("other");
        assert_eq!(
            send(&mut rpc, &mut engine, &observer, wrong).await.unwrap()["error"]["data"]
                ["details"]["code"],
            "POLICY_DENIED"
        );
        observer.read = false;
        assert!(send(&mut rpc, &mut engine, &observer, read)
            .await
            .unwrap()
            .get("error")
            .is_some());
        assert_eq!(&before, engine.store().state());
        engine.into_store().close().await.unwrap();
    }
}

// Pure projection fixtures deliberately cover retained historical combinations;
// admission/store validity and authorization are separately exercised above.
#[tokio::test]
async fn pending_inputs_and_effect_uncertainty_are_real_scoped_and_bounded() {
    let temp = tempfile::tempdir().unwrap();
    let mut engine = setup(temp.path(), BackendKind::Files).await;
    let mut task = create_task(&mut engine).await;
    task.revision = Revision::new(u64::MAX);
    task.steering = SteeringRevision::new(9_007_199_254_740_993);
    let mut state = engine.store().state().clone();
    let pending = approval(&task, "pending");
    insert(
        &mut state,
        Collection::Approval,
        "pending",
        &task,
        pending.revision,
        &pending,
    );
    let mut foreign = approval(&task, "foreign");
    foreign.scope.session = SessionId::parse("other").unwrap();
    insert(
        &mut state,
        Collection::Approval,
        "foreign",
        &task,
        foreign.revision,
        &foreign,
    );
    let mut stale = approval(&task, "stale");
    stale.steering = SteeringRevision::ZERO;
    insert(
        &mut state,
        Collection::Approval,
        "stale",
        &task,
        stale.revision,
        &stale,
    );
    let mut done = approval(&task, "done");
    done.state = ApprovalState::Allowed;
    insert(
        &mut state,
        Collection::Approval,
        "done",
        &task,
        done.revision,
        &done,
    );
    let mut outcome = effect(&task, EffectState::OutcomeUnknown);
    outcome.steering = SteeringRevision::ZERO;
    insert(
        &mut state,
        Collection::Effect,
        "effect",
        &task,
        outcome.revision,
        &outcome,
    );
    // A later known effect must not overwrite another effect's uncertainty.
    let mut known = effect(&task, EffectState::Succeeded);
    known.id = ToolRunId::parse("z-known").unwrap();
    insert(
        &mut state,
        Collection::Effect,
        "z-known",
        &task,
        known.revision,
        &known,
    );
    // An unrelated session's unknown effect must not contaminate this task.
    let mut foreign_effect = effect(&task, EffectState::OutcomeUnknown);
    foreign_effect.id = ToolRunId::parse("z-foreign").unwrap();
    foreign_effect.scope.session = SessionId::parse("other").unwrap();
    insert(
        &mut state,
        Collection::Effect,
        "z-foreign",
        &task,
        foreign_effect.revision,
        &foreign_effect,
    );
    let view = serde_json::to_value(task_view(&state, task.clone()).unwrap()).unwrap();
    assert_eq!(view["revision"], u64::MAX.to_string());
    assert_eq!(view["steering_revision"], "9007199254740993");
    assert_eq!(view["effects"], "unknown");
    assert_eq!(view["pending_inputs"].as_array().unwrap().len(), 1);
    assert_eq!(view["pending_inputs"][0]["id"], "pending");
    assert_eq!(view["pending_inputs"][0]["revision"], u64::MAX.to_string());
    for (effect_state, changed, expected) in [
        (EffectState::DispatchRecorded, false, "unknown"),
        (EffectState::Running, false, "pending"),
        (EffectState::Failed, true, "partial"),
        (EffectState::Cancelled, true, "partial"),
        (EffectState::Succeeded, true, "known"),
    ] {
        outcome.state = effect_state;
        outcome.observed_changes = if changed {
            vec![ArtifactId::new()]
        } else {
            vec![]
        };
        insert(
            &mut state,
            Collection::Effect,
            "effect",
            &task,
            outcome.revision,
            &outcome,
        );
        assert_eq!(
            serde_json::to_value(task_view(&state, task.clone()).unwrap()).unwrap()["effects"],
            expected
        );
    }
    for n in 0..128 {
        let name = format!("input-{n}");
        let pending = approval(&task, &name);
        insert(
            &mut state,
            Collection::Approval,
            &name,
            &task,
            pending.revision,
            &pending,
        );
    }
    assert_eq!(
        serde_json::to_value(task_view(&state, task).unwrap_err()).unwrap()["data"]["details"]
            ["code"],
        "RESOURCE_LIMIT"
    );
    engine.into_store().close().await.unwrap();
}

#[tokio::test]
async fn current_turn_uses_creation_history_and_missing_evidence_is_null() {
    let temp = tempfile::tempdir().unwrap();
    let mut engine = setup(temp.path(), BackendKind::Files).await;
    let task = create_task(&mut engine).await;
    let mut state = engine.store().state().clone();
    let mut old = put_turn(&mut state, &task, "z-old", 10);
    let current = put_turn(&mut state, &task, "a-current", 20);
    old.revision = Revision::new(99);
    old.state = TurnState::Paused;
    insert(
        &mut state,
        Collection::Turn,
        "z-old",
        &task,
        old.revision,
        &old,
    );
    assert_eq!(
        task_view(&state, task.clone())
            .unwrap()
            .turn
            .unwrap()
            .as_str(),
        current.id.as_str()
    );
    state.events.pop();
    assert!(task_view(&state, task.clone()).unwrap().turn.is_none());
    let mut state = engine.store().state().clone();
    put_turn(&mut state, &task, "old-steering", 10);
    let mut steered = task.clone();
    steered.steering = SteeringRevision::new(1);
    assert!(task_view(&state, steered).unwrap().turn.is_none());
    let mut excessive = task;
    excessive.reason = "r".repeat(4097);
    assert!(task_view(&state, excessive).is_err());
    engine.into_store().close().await.unwrap();
}
