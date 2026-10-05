// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::cell::Cell;
use vcp_protocol::event::EventEnvelope;
use vcp_store::contract::{reference::ReferenceStore, Receipt, State};

struct Bounded {
    state: State,
    events: Vec<EventEnvelope>,
    watermark: Cell<Watermark>,
    unavailable: Option<u64>,
    empty: bool,
}
impl ReferenceStore for Bounded {
    fn state(&self) -> &State {
        panic!("history payload materialization is forbidden")
    }
    fn current(&self) -> vcp_store::CurrentStateView<'_> {
        let mut view = vcp_store::CurrentStateView::from(&self.state);
        view.watermark = self.watermark.get();
        view
    }
    async fn history_event_count(&self) -> vcp_store::Result<u64> {
        Ok(self.events.len() as u64)
    }
    async fn history_events(
        &self,
        after: Option<u64>,
        limit: usize,
    ) -> vcp_store::Result<Vec<EventEnvelope>> {
        assert!(limit > 0 && limit <= 256);
        let at = after.map_or(0, |after| after + 1);
        if self.unavailable == Some(at) {
            return if self.empty {
                Ok(vec![])
            } else {
                Err(vcp_store::Error::Corruption("injected history failure"))
            };
        }
        Ok(self
            .events
            .iter()
            .skip(at as usize)
            .take(1)
            .cloned()
            .collect())
    }
    async fn transact(&mut self, _: Transaction) -> vcp_store::Result<Receipt> {
        panic!("read-only")
    }
}
fn fixture() -> Bounded {
    Bounded {
        state: State::default(),
        watermark: Cell::new(Watermark::new(1)),
        unavailable: None,
        empty: false,
        events: (0..3)
            .map(|index| EventEnvelope {
                version: 1,
                sequence: SessionSeq::new(index + 1),
                watermark: Watermark::new(1),
                redaction: None,
                event: EventInput {
                    id: EventId::new(),
                    workspace: WorkspaceId::new(),
                    session: SessionId::new(),
                    task: None,
                    actor: ActorId::new(),
                    correlation: CommandId::new(),
                    causation: None,
                    timestamp: Timestamp::new(index),
                    kind: EventKind::Diagnostic,
                    artifacts: vec![],
                    data: serde_json::json!({}),
                    metadata: None,
                },
            })
            .collect(),
    }
}
#[tokio::test]
async fn short_pages_preserve_order_without_resident_history() {
    let store = fixture();
    let mut pages = HistoryPages::new(&store).await.unwrap();
    let mut actual = vec![];
    while let Some(page) = pages.next(&store).await.unwrap() {
        actual.extend(page);
    }
    assert_eq!(actual, store.events);
}
#[tokio::test]
async fn missing_pages_errors_and_changed_cuts_never_become_eof() {
    for empty in [false, true] {
        let mut store = fixture();
        store.unavailable = Some(1);
        store.empty = empty;
        let mut pages = HistoryPages::new(&store).await.unwrap();
        assert!(pages.next(&store).await.unwrap().is_some());
        assert!(pages.next(&store).await.is_err());
    }
    let store = fixture();
    let mut pages = HistoryPages::new(&store).await.unwrap();
    store.watermark.set(Watermark::new(2));
    assert!(pages.next(&store).await.is_err());
    let mut store = fixture();
    store.events[0].watermark = Watermark::new(2);
    assert!(HistoryPages::new(&store)
        .await
        .unwrap()
        .next(&store)
        .await
        .is_err());
}
