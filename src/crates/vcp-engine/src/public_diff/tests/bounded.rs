// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::cell::Cell;
use vcp_store::contract::{Receipt, State};

struct Reader {
    source: State,
    reads: Cell<usize>,
    fail_after: Option<usize>,
    empty_after: Option<usize>,
}
impl vcp_store::contract::reference::ReferenceStore for Reader {
    fn state(&self) -> &State {
        panic!("diff provenance requested resident history")
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
        let start = after.map_or(0, |ordinal| ordinal as usize + 1);
        self.reads.set(self.reads.get() + 1);
        if self.fail_after.is_some_and(|cut| start >= cut) {
            return Err(vcp_store::Error::Corruption("injected diff page"));
        }
        if self.empty_after.is_some_and(|cut| start >= cut) {
            return Ok(Vec::new());
        }
        Ok(self
            .source
            .events
            .iter()
            .skip(start)
            .take(limit.min(1))
            .cloned()
            .collect())
    }
    async fn transact(&mut self, _: Transaction) -> vcp_store::Result<Receipt> {
        panic!("diff reader wrote canonical state")
    }
}

#[tokio::test]
async fn proposal_proof_visits_full_short_page_history_and_rejects_later_faults() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (engine, _, _) = fixture(temp.path(), backend, 0).await;
        let effect: Effect = engine
            .store()
            .current()
            .record(Collection::Effect, "change", &access().workspace)
            .unwrap()
            .decode()
            .unwrap();
        let expected = provenance::read(engine.store(), &effect, &[])
            .await
            .unwrap();
        let mut source = engine.store().archive_state().await.unwrap();
        engine.into_store().close().await.unwrap();
        let anchor = source
            .events
            .iter()
            .find(|row| row.event.id == expected.0.cause)
            .unwrap()
            .clone();
        // A nonmatching final row proves traversal cannot stop at the proposal.
        let mut tail = anchor.clone();
        tail.event.kind = EventKind::Diagnostic;
        tail.event.id = EventId::new();
        source.events.push(tail);
        let end = source.events.len();
        let mut reader = Reader {
            source,
            reads: Cell::new(0),
            fail_after: None,
            empty_after: None,
        };
        let actual = provenance::read(&reader, &effect, &[]).await.unwrap();
        assert_eq!(
            vcp_protocol::canonical_bytes(&actual).unwrap(),
            vcp_protocol::canonical_bytes(&expected).unwrap()
        );
        assert_eq!(reader.reads.get(), end);
        reader.fail_after = Some(end - 1);
        assert_eq!(
            provenance::read(&reader, &effect, &[]).await,
            Err(QueryError::InvalidData)
        );
        reader.fail_after = None;
        reader.empty_after = Some(end - 1);
        assert_eq!(
            provenance::read(&reader, &effect, &[]).await,
            Err(QueryError::InvalidData)
        );
        reader.empty_after = None;
        // A duplicate valid proof at a later ordinal remains unavailable.
        reader.source.events.push(anchor.clone());
        assert_eq!(
            provenance::read(&reader, &effect, &[]).await,
            Err(QueryError::Unavailable)
        );
        reader.source.events.pop();
        let mut malformed = anchor;
        let facts = malformed.event.data["facts"].as_array_mut().unwrap();
        let fact = facts
            .iter_mut()
            .find(|fact| {
                fact["collection"] == "effect" && fact["id"] == "change" && fact["revision"] == "1"
            })
            .unwrap();
        fact["value"] = serde_json::Value::Null;
        reader.source.events.push(malformed);
        assert_eq!(
            provenance::read(&reader, &effect, &[]).await,
            Err(QueryError::InvalidData)
        );
    }
}
