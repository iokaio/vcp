// SPDX-License-Identifier: Apache-2.0
use super::*;
struct PagedTask<'a> {
    source: &'a State,
    fail: bool,
    reads: std::cell::Cell<usize>,
}
impl vcp_store::contract::reference::ReferenceStore for PagedTask<'_> {
    fn state(&self) -> &State {
        panic!("live task projection must not request resident history")
    }
    fn current(&self) -> vcp_store::CurrentStateView<'_> {
        self.source.into()
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
            return Err(vcp_store::Error::Corruption(
                "injected interior projection page",
            ));
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
    async fn transact(
        &mut self,
        _: vcp_store::contract::Transaction,
    ) -> vcp_store::Result<vcp_store::contract::Receipt> {
        panic!("read-only projection dispatched a transaction")
    }
}

#[tokio::test]
async fn task_projection_streams_full_chronology_and_does_not_hide_read_failure_as_null() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut engine = setup(temp.path(), backend).await;
        let task = create_task(&mut engine).await;
        let mut state = engine.store().archive_state().await.unwrap();
        put_turn(&mut state, &task, "old", 10);
        put_turn(&mut state, &task, "new", 20);
        state.watermark = Watermark::new(20);
        let reader = PagedTask {
            source: &state,
            fail: false,
            reads: Default::default(),
        };
        assert_eq!(
            task_view_store(&reader, task.clone()).await.unwrap(),
            task_view(&state, task.clone()).unwrap()
        );
        assert_eq!(reader.reads.get(), state.events.len().div_ceil(2));
        let fault = PagedTask {
            source: &state,
            fail: true,
            reads: Default::default(),
        };
        assert!(task_view_store(&fault, task.clone()).await.is_err());
        state.events.pop();
        let missing = PagedTask {
            source: &state,
            fail: false,
            reads: Default::default(),
        };
        assert!(task_view_store(&missing, task)
            .await
            .unwrap()
            .turn
            .is_none());
        engine.into_store().close().await.unwrap();
    }
}
