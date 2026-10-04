// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::cell::Cell;
use vcp_protocol::{command::CommandReceipt, event::EventEnvelope};
use vcp_store::contract::{Receipt, Transaction};

struct Reader {
    store: Store,
    mode: u8,
    pages: Cell<usize>,
}
impl CanonicalStore for Reader {
    fn state(&self) -> &State {
        panic!("receipt projection requested resident State")
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
    async fn scoped_command_receipt(
        &self,
        workspace: &WorkspaceId,
        session: &SessionId,
        command: &CommandId,
    ) -> vcp_store::Result<Option<CommandReceipt>> {
        self.store
            .scoped_command_receipt(workspace, session, command)
            .await
    }
    async fn history_event_count(&self) -> vcp_store::Result<u64> {
        self.store.history_event_count().await
    }
    async fn history_events(
        &self,
        after: Option<u64>,
        limit: usize,
    ) -> vcp_store::Result<Vec<EventEnvelope>> {
        self.pages.set(self.pages.get() + 1);
        if after.is_some() {
            if self.mode == 1 {
                return Err(vcp_store::Error::Corruption(
                    "injected acceptance history fault",
                ));
            }
            if self.mode == 2 {
                return Ok(vec![]);
            }
        }
        let mut rows = self.store.history_events(after, limit.min(2)).await?;
        if self.mode == 3 {
            for row in &mut rows {
                if row.event.kind == EventKind::TurnTransition {
                    row.event.data["facts"] = serde_json::json!([]);
                }
            }
        }
        Ok(rows)
    }
    async fn transact(&mut self, _: Transaction) -> vcp_store::Result<Receipt> {
        panic!("receipt inspection wrote canonical state")
    }
}

#[tokio::test]
async fn receipt_projection_uses_bounded_history_and_rejects_faults_or_missing_turn_proof() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut engine, access, connection, token) = fixture(temp.path(), backend).await;
        let prepared = prepare(&engine, &access, &connection, &token).await;
        let trigger = trigger(&engine);
        let PublicStartOutcome::Accepted(receipt) = engine
            .commit_public_start(prepared, &access, &facts(), &trigger, Timestamp::new(3))
            .await
            .unwrap()
        else {
            panic!()
        };
        let expected = serde_json::to_value(
            crate::rpc::acceptance(&engine, &access, &receipt)
                .await
                .unwrap(),
        )
        .unwrap();
        let mut engine = Engine::new(Reader {
            store: engine.into_store(),
            mode: 0,
            pages: Cell::new(0),
        })
        .unwrap();
        assert_eq!(
            serde_json::to_value(
                crate::rpc::acceptance(&engine, &access, &receipt)
                    .await
                    .unwrap()
            )
            .unwrap(),
            expected
        );
        assert!(
            engine.store().pages.get() > 2,
            "receipt proof must continue through short pages"
        );
        for mode in [1, 2, 3] {
            engine.store_mut().mode = mode;
            assert!(
                crate::rpc::acceptance(&engine, &access, &receipt)
                    .await
                    .is_err(),
                "mode {mode}"
            );
        }
        let pages = engine.store().pages.get();
        let mut denied = access.clone();
        denied.read = false;
        assert!(crate::rpc::acceptance(&engine, &denied, &receipt)
            .await
            .is_err());
        assert_eq!(engine.store().pages.get(), pages);
        engine.store_mut().mode = 0;
        assert_eq!(
            serde_json::to_value(
                crate::rpc::acceptance(&engine, &access, &receipt)
                    .await
                    .unwrap()
            )
            .unwrap(),
            expected
        );
        engine.into_store().store.close().await.unwrap();
    }
}
