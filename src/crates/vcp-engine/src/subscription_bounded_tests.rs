// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::cell::Cell;
use vcp_protocol::event::EventEnvelope;
use vcp_store::{
    contract::{Receipt, State, Transaction, MAX_COMMIT_BYTES},
    BackendKind,
};

struct Reader {
    source: State,
    reads: Cell<usize>,
    fail: bool,
}
impl CanonicalStore for Reader {
    fn state(&self) -> &State {
        panic!("subscription requested resident history")
    }
    fn current(&self) -> vcp_store::CurrentStateView<'_> {
        (&self.source).into()
    }
    async fn history_event_count(&self) -> vcp_store::Result<u64> {
        Ok(self.source.events.len() as u64)
    }
    async fn history_events(
        &self,
        after: Option<u64>,
        limit: usize,
    ) -> vcp_store::Result<Vec<EventEnvelope>> {
        self.reads.set(self.reads.get() + 1);
        if self.fail && after.is_some() {
            return Err(vcp_store::Error::Corruption("injected subscriber page"));
        }
        Ok(self
            .source
            .events
            .iter()
            .skip(after.map_or(0, |n| n as usize + 1))
            .take(limit.min(2))
            .cloned()
            .collect())
    }
    async fn transact(&mut self, _: Transaction) -> vcp_store::Result<Receipt> {
        panic!("subscriber wrote canonical records")
    }
}

#[tokio::test]
async fn subscriptions_stream_short_pages_and_fail_on_interior_reads_without_advancing_cursor() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut original, access) = projection_tests::fixture(temp.path(), backend).await;
        let start = original.store().current().sequences[&access.session];
        let ids = projection_tests::append(&mut original, &access, 6).await;
        let state = original.store().state().clone();
        original.into_store().close().await.unwrap();
        let mut engine = Engine::new(Reader {
            source: state,
            reads: Cell::new(0),
            fail: false,
        })
        .unwrap();
        let cursor = engine
            .subscribe(&access, start, 128, Timestamp::new(2))
            .unwrap();
        let EventPage::Events { events, at_end, .. } = engine
            .events(&access, &cursor, Timestamp::new(3))
            .await
            .unwrap()
        else {
            panic!()
        };
        assert!(at_end);
        assert_eq!(
            events
                .iter()
                .map(|row| row.event.id.clone())
                .collect::<Vec<_>>(),
            ids
        );
        let ProjectedEvents::Page { events, at_end, .. } = engine
            .projected_events(&access, &cursor, Timestamp::new(3), 256 * 1024)
            .await
            .unwrap()
        else {
            panic!()
        };
        assert!(at_end);
        assert_eq!(
            events.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            ids.iter().map(|id| id.as_str()).collect::<Vec<_>>()
        );
        assert!(!serde_json::to_string(&events)
            .unwrap()
            .contains("secret_internal_fact"));
        engine.store_mut().fail = true;
        assert!(engine
            .events(&access, &cursor, Timestamp::new(3))
            .await
            .is_err());
        assert!(engine
            .projected_events(&access, &cursor, Timestamp::new(3), 256 * 1024)
            .await
            .is_err());
        assert_eq!(
            vcp_protocol::canonical_bytes(&engine.subscriptions[&cursor.snapshot]).unwrap(),
            vcp_protocol::canonical_bytes(&cursor).unwrap()
        );
        let reads = engine.store().reads.get();
        let mut denied = access.clone();
        denied.read = false;
        assert!(engine
            .events(&denied, &cursor, Timestamp::new(3))
            .await
            .is_err());
        assert_eq!(engine.store().reads.get(), reads);

        engine.store_mut().fail = false;
        for row in engine
            .store_mut()
            .source
            .events
            .iter_mut()
            .filter(|row| row.sequence > start)
            .take(3)
        {
            row.event.data =
                serde_json::json!({"schema_version":1,"payload":"x".repeat(6 * 1024 * 1024)});
        }
        let EventPage::Events {
            events,
            next_cursor,
            at_end,
            ..
        } = engine
            .events(&access, &cursor, Timestamp::new(3))
            .await
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(
            events.len(),
            2,
            "raw result stops at a byte boundary before its row limit"
        );
        assert!(!at_end);
        assert!(
            events
                .iter()
                .map(|row| vcp_protocol::canonical_bytes(row).unwrap().len())
                .sum::<usize>()
                <= MAX_COMMIT_BYTES
        );
        let EventPage::Events {
            events: rest,
            at_end,
            ..
        } = engine
            .events(&access, &next_cursor, Timestamp::new(3))
            .await
            .unwrap()
        else {
            panic!()
        };
        assert!(at_end);
        assert_eq!(
            events
                .iter()
                .chain(&rest)
                .map(|row| row.event.id.clone())
                .collect::<Vec<_>>(),
            ids
        );
    }
}
