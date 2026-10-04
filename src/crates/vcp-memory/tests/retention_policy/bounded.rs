// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::cell::Cell;
use vcp_store::contract::{Receipt, State, Transaction};

struct Paged<'a> {
    store: &'a Store,
    reads: Cell<usize>,
    fail: bool,
}
impl CanonicalStore for Paged<'_> {
    fn state(&self) -> &State {
        panic!("aging requested complete history")
    }
    fn current(&self) -> vcp_store::CurrentStateView<'_> {
        self.store.current()
    }
    async fn history_event_count(&self) -> vcp_store::Result<u64> {
        self.store.history_event_count().await
    }
    async fn history_events(
        &self,
        after: Option<u64>,
        limit: usize,
    ) -> vcp_store::Result<Vec<vcp_protocol::event::EventEnvelope>> {
        self.reads.set(self.reads.get() + 1);
        if self.fail && after.is_some() {
            return Err(vcp_store::Error::Unavailable("injected aging read"));
        }
        self.store.history_events(after, limit.min(2)).await
    }
    async fn transact(&mut self, _: Transaction) -> vcp_store::Result<Receipt> {
        panic!("aging mutated canonical state")
    }
}

#[tokio::test]
async fn paged_notice_matches_retained_rows_and_rejects_partial_reads() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path(), backend, &[]).await.unwrap();
        let initial = common::initial();
        store.transact(initial.clone()).await.unwrap();
        let events = (0..70)
            .map(|n| {
                let mut event = initial.events[0].clone();
                event.id = EventId::new();
                event.timestamp = Timestamp::new(101 + n);
                event
            })
            .collect();
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.current().watermark,
                mutations: vec![],
                events,
                command: None,
            })
            .await
            .unwrap();
        let access = access();
        let now = Timestamp::new(40 * DAY_MS);
        let mut expected_bytes = store
            .state()
            .events
            .iter()
            .map(|event| serde_json::to_vec(event).unwrap().len() as u64)
            .sum::<u64>();
        for row in store
            .current()
            .records
            .values()
            .filter(|row| row.collection == vcp_store::contract::Collection::Artifact)
        {
            let artifact: vcp_domain::artifact::ArtifactDescriptor = row.decode().unwrap();
            for range in artifact.retained {
                expected_bytes += range.end.get() - range.start.get();
            }
        }
        let reader = Paged {
            store: &store,
            reads: Cell::new(0),
            fail: false,
        };
        let notice = aging(&reader, &access, now).await.unwrap();
        assert_eq!(notice.oldest, Some(Timestamp::new(100)));
        assert_eq!(notice.retained_bytes, expected_bytes);
        assert!(notice.due && !notice.bytes_are_exact);
        assert!(reader.reads.get() > 2);
        let failing = Paged {
            store: &store,
            reads: Cell::new(0),
            fail: true,
        };
        assert!(aging(&failing, &access, now).await.is_err());
        assert_eq!(failing.reads.get(), 2);
        let denied = Paged {
            store: &store,
            reads: Cell::new(0),
            fail: false,
        };
        let mut forbidden = access;
        forbidden.read = false;
        assert!(aging(&denied, &forbidden, now).await.is_err());
        assert_eq!(denied.reads.get(), 0);
        store.close().await.unwrap();
    }
}
