// SPDX-License-Identifier: Apache-2.0
mod common;
use vcp_domain::{
    ids::*,
    revision::*,
    task::{Turn, TurnState},
    workspace::Session,
};
use vcp_protocol::{
    command::{
        fork::{self, Acceptance, Format},
        CommandResult,
    },
    event::{EventInput, EventKind},
};
use vcp_store::{artifact::ArtifactWriter, contract::*, BackendKind, Store};

async fn fixture(store: &mut Store) -> Transaction {
    store.transact(common::initial()).await.unwrap();
    let mut writer = store.spool().create(common::spec()).unwrap();
    writer.write_chunk(b"synthetic turn input").unwrap();
    let artifact = writer.finalize().unwrap();
    drop(writer);
    store
        .transact(common::attach(store.state(), artifact.clone(), None))
        .await
        .unwrap();
    let boundary = EventId::new();
    let turn = Turn {
        redaction: None,
        id: TurnId::new(),
        scope: common::task().scope,
        revision: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        state: TurnState::Completed,
        trigger: artifact.spec.id,
        cause: boundary.clone(),
        reason: "imported completed boundary for store contract fixture".into(),
    };
    store.transact(Transaction {
        id:TransactionId::new(),expected_watermark:store.state().watermark,
        mutations:vec![Mutation::Put {expected:None,record:Record::typed(Collection::Turn,turn.id.as_str(),common::workspace().id,Revision::ZERO,&turn).unwrap()}],
        events:vec![EventInput {id:boundary,workspace:common::workspace().id,session:common::session().id,task:Some(common::task().scope.task),actor:ActorId::new(),correlation:CommandId::new(),causation:None,timestamp:Timestamp::new(1),kind:EventKind::TurnTransition,artifacts:vec![],metadata:None,data:serde_json::json!({"schema_version":1,"facts":[{"collection":"turn","id":turn.id,"revision":turn.revision,"value":turn}]})}],command:None,
    }).await.unwrap();
    let accepted = Acceptance {
        document_type: Format::V1,
        schema_version: 1,
        source: turn.scope.clone(),
        through_turn: turn.id,
        through_watermark: store.state().watermark,
        new_session: SessionId::parse("forked-session").unwrap(),
        new_task: TaskId::parse("forked-task").unwrap(),
    };
    let command = CommandId::parse("fork-command").unwrap();
    let source_id = EventId::new();
    let target_id = EventId::new();
    let session = Session {
        id: accepted.new_session.clone(),
        workspace: common::workspace().id,
        revision: Revision::ZERO,
        configuration: common::session().configuration,
        fork_origin: Some(common::session().id),
        fork_through: Some(accepted.through_turn.clone()),
    };
    let mut task = common::task();
    task.scope.session = session.id.clone();
    task.scope.task = accepted.new_task.clone();
    task.root = accepted.new_task.clone();
    task.fork_origin = Some(common::task().scope.task);
    task.cause = target_id.clone();
    task.objectives[0].source = target_id.clone();
    let mut source = EventInput {
        id: source_id.clone(),
        workspace: common::workspace().id,
        session: common::session().id,
        task: None,
        actor: ActorId::new(),
        correlation: command.clone(),
        causation: None,
        timestamp: Timestamp::new(2),
        kind: EventKind::SessionStarted,
        artifacts: vec![],
        metadata: None,
        data: serde_json::to_value(&accepted).unwrap(),
    };
    let target = EventInput {
        id: target_id,
        workspace: source.workspace.clone(),
        session: session.id.clone(),
        task: Some(task.scope.task.clone()),
        actor: source.actor.clone(),
        correlation: fork::target_correlation(&command, &accepted).unwrap(),
        causation: Some(source_id),
        timestamp: source.timestamp,
        kind: EventKind::TaskCreated,
        artifacts: vec![],
        metadata: None,
        data: fork::genesis_facts(&session, &task),
    };
    source.task = None;
    Transaction {
        id: TransactionId::new(),
        expected_watermark: store.state().watermark,
        mutations: vec![
            Mutation::Put {
                expected: None,
                record: Record::typed(
                    Collection::Session,
                    session.id.as_str(),
                    source.workspace.clone(),
                    Revision::ZERO,
                    &session,
                )
                .unwrap(),
            },
            Mutation::Put {
                expected: None,
                record: Record::typed(
                    Collection::Task,
                    task.scope.task.as_str(),
                    source.workspace.clone(),
                    Revision::ZERO,
                    &task,
                )
                .unwrap(),
            },
        ],
        events: vec![source, target],
        command: Some(ReceiptInput {
            command,
            workspace: common::workspace().id,
            session: common::session().id,
            digest: "a".repeat(64),
            result: CommandResult::Accepted {
                revision: Revision::ZERO,
            },
        }),
    }
}

#[tokio::test]
async fn only_exact_paired_genesis_can_cross_receipt_scope_on_both_stores() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        let valid = fixture(&mut store).await;
        let before = store.state().clone();
        for variant in 0..13 {
            let mut bad = valid.clone();
            bad.id = TransactionId::new();
            match variant {
                0 => bad.events[1].correlation = bad.events[0].correlation.clone(),
                1 => bad.events[1].causation = None,
                2 => bad.events[1].actor = ActorId::new(),
                3 => bad.events[1].timestamp = Timestamp::new(3),
                4 => {
                    bad.events[1].data["facts"][1]["value"]["reason"] =
                        serde_json::json!("forged separate fact")
                }
                5 => bad.mutations.push(bad.mutations[0].clone()),
                6 => {
                    if let Mutation::Put { expected, .. } = &mut bad.mutations[0] {
                        *expected = Some(Revision::ZERO);
                    }
                }
                7 => bad.events[0].data["document_type"] = serde_json::json!("vcp-session-fork/2"),
                8 => bad.events[1].kind = EventKind::Commentary,
                9 => bad.events[0].data["schema_version"] = serde_json::json!(2),
                10 => {
                    bad.events.truncate(1);
                } // reserved marker cannot fall back to ordinary single-session acceptance
                11 => bad.events[0].data["through_turn"] = serde_json::json!("missing"),
                _ => bad.events[1].workspace = WorkspaceId::new(),
            }
            assert!(store.transact(bad).await.is_err(), "variant {variant}");
            assert_eq!(store.state(), &before, "variant {variant}");
        }
        let committed = store.transact(valid.clone()).await.unwrap();
        assert_eq!(store.state().records.len(), before.records.len() + 2);
        assert_eq!(store.state().events.len(), before.events.len() + 2);
        let receipt = committed.command.as_ref().unwrap();
        assert_eq!(receipt.first_event, receipt.last_event);
        assert_eq!(
            receipt.first_event,
            before.sequences[&common::session().id].next().unwrap()
        );
        assert_eq!(
            store.state().sequences[&SessionId::parse("forked-session").unwrap()],
            SessionSeq::new(1)
        );
        assert_eq!(store.transact(valid.clone()).await.unwrap(), committed);
        let after = store.state().clone();
        let mut another = valid;
        another.id = TransactionId::new();
        another.expected_watermark = after.watermark;
        another.command.as_mut().unwrap().command = CommandId::new();
        assert!(store.transact(another).await.is_err());
        assert_eq!(store.state(), &after);
        store.close().await.unwrap();
        let store = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(store.state(), &after);
        store.close().await.unwrap();
    }
}
