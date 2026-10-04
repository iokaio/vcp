// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::cell::Cell;
use vcp_domain::artifact::{ArtifactSpec, Channel};
use vcp_protocol::event::EventEnvelope;
use vcp_store::artifact::ArtifactWriter;

struct CurrentOwner {
    store: Store,
    mode: u8,
    reads: Cell<usize>,
}
impl CanonicalStore for CurrentOwner {
    fn state(&self) -> &State {
        panic!("ordinary command read historical State")
    }
    fn current(&self) -> vcp_store::CurrentStateView<'_> {
        self.store.current()
    }
    async fn command_receipt(
        &self,
        workspace: &WorkspaceId,
        command: &CommandId,
        digest: &str,
    ) -> vcp_store::Result<Option<CommandReceipt>> {
        self.store.command_receipt(workspace, command, digest).await
    }
    async fn history_event(&self, id: &EventId) -> vcp_store::Result<Option<EventEnvelope>> {
        self.reads.set(self.reads.get() + 1);
        match self.mode {
            1 => return Ok(None),
            2 => return Err(vcp_store::Error::Corruption("unavailable exact event page")),
            _ => {}
        }
        let mut event = self.store.history_event(id).await?;
        if self.mode == 3 {
            event.as_mut().unwrap().event.session = SessionId::new();
        }
        Ok(event)
    }
    async fn transact(&mut self, transaction: Transaction) -> vcp_store::Result<Receipt> {
        self.store.transact(transaction).await
    }
}

#[tokio::test]
async fn commands_use_current_rows_and_exact_fallible_session_boundary_history() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path(), backend, &[]).await.unwrap();
        let mut engine = Engine::new(CurrentOwner {
            store,
            mode: 0,
            reads: Cell::new(0),
        })
        .unwrap();
        let mut facts = HostFacts::inspect(Timestamp::new(100));
        facts.may_execute = true;
        engine
            .handle(
                command(
                    &engine,
                    Command::Initialize { binding: binding() },
                    None,
                    Revision::ZERO,
                ),
                &access(),
                &facts,
            )
            .await
            .unwrap();
        let task = TaskId::new();
        engine
            .handle(
                command(&engine, creation(&task), Some(task.clone()), Revision::ZERO),
                &access(),
                &facts,
            )
            .await
            .unwrap();
        engine
            .handle(
                command(
                    &engine,
                    Command::Transition {
                        next: TaskState::Running,
                        reason: "fixture".into(),
                        verification: None,
                    },
                    Some(task.clone()),
                    Revision::ZERO,
                ),
                &access(),
                &facts,
            )
            .await
            .unwrap();
        let mut writer = engine
            .store()
            .store
            .spool()
            .create(ArtifactSpec {
                id: ArtifactId::new(),
                scope: Scope {
                    workspace: access().workspace,
                    session: access().session,
                    task: task.clone(),
                },
                media_type: "text/plain".into(),
                schema: "turn-trigger/1".into(),
                source: "fixture".into(),
                channel: Channel::Evidence,
                retention: "history".into(),
                omissions: vec![],
            })
            .unwrap();
        writer.write_chunk(b"current owner turn").unwrap();
        let descriptor = writer.finalize().unwrap();
        drop(writer);
        let trigger = descriptor.spec.id.clone();
        engine
            .handle(
                command(
                    &engine,
                    Command::AttachArtifact { descriptor },
                    Some(task.clone()),
                    Revision::ZERO,
                ),
                &access(),
                &facts,
            )
            .await
            .unwrap();
        let turn = TurnId::new();
        engine
            .handle(
                command(
                    &engine,
                    Command::StartTurn {
                        id: turn.clone(),
                        trigger,
                    },
                    Some(task.clone()),
                    Revision::new(1),
                ),
                &access(),
                &facts,
            )
            .await
            .unwrap();
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
            engine
                .handle(
                    command(
                        &engine,
                        Command::AdvanceTurn {
                            id: turn.clone(),
                            next,
                            reason: "observed fixture".into(),
                        },
                        Some(task.clone()),
                        Revision::new(revision as u64),
                    ),
                    &access(),
                    &facts,
                )
                .await
                .unwrap();
        }
        assert_eq!(
            engine.store().reads.get(),
            0,
            "ordinary planning requires no event history"
        );
        let request = command(
            &engine,
            Command::CreateSession {
                id: SessionId::new(),
                fork_through: Some(turn),
            },
            None,
            Revision::ZERO,
        );
        let before = engine.store().current().watermark;
        for mode in [1, 2, 3] {
            engine.store_mut().mode = mode;
            assert!(engine
                .handle(request.clone(), &access(), &facts)
                .await
                .is_err());
            assert_eq!(engine.store().current().watermark, before);
        }
        engine.store_mut().mode = 0;
        let receipt = engine
            .handle(request.clone(), &access(), &facts)
            .await
            .unwrap();
        let reads = engine.store().reads.get();
        engine.store_mut().mode = 2;
        assert_eq!(
            engine.handle(request, &access(), &facts).await.unwrap(),
            receipt
        );
        assert_eq!(
            engine.store().reads.get(),
            reads,
            "exact retry precedes optional historical boundary proof"
        );
        engine.into_store().store.close().await.unwrap();
    }
}
