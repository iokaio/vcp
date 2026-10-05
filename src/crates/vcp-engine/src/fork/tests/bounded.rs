// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::cell::Cell;
use vcp_protocol::event::EventEnvelope;
use vcp_store::{
    contract::{CanonicalStore, Receipt},
    CurrentState, CurrentStateView,
};

struct Reader {
    current: CurrentState,
    events: Vec<EventEnvelope>,
    fail_exact: bool,
    fail_page: Option<usize>,
    pages: Cell<usize>,
}
impl Reader {
    fn new(store: &Store, state: &State) -> Self {
        let mut current = store.current_state().as_ref().clone();
        current.records = state.records.clone();
        current.sequences = state.sequences.clone();
        current.watermark = state.watermark;
        Self {
            current,
            events: state.events.to_vec(),
            fail_exact: false,
            fail_page: None,
            pages: Cell::new(0),
        }
    }
}
impl vcp_store::contract::reference::ReferenceStore for Reader {
    fn state(&self) -> &State {
        panic!("fork source materialized full State")
    }
    fn current(&self) -> CurrentStateView<'_> {
        (&self.current).into()
    }
    async fn history_event(&self, id: &EventId) -> vcp_store::Result<Option<EventEnvelope>> {
        if self.fail_exact {
            return Err(vcp_store::Error::Corruption("missing fork boundary page"));
        }
        Ok(self.events.iter().find(|row| &row.event.id == id).cloned())
    }
    async fn history_event_count(&self) -> vcp_store::Result<u64> {
        Ok(self.events.len() as u64)
    }
    async fn history_events(
        &self,
        after: Option<u64>,
        limit: usize,
    ) -> vcp_store::Result<Vec<EventEnvelope>> {
        self.pages.set(self.pages.get() + 1);
        if self.fail_page == Some(self.pages.get()) {
            return Err(vcp_store::Error::Corruption("missing fork source page"));
        }
        Ok(self
            .events
            .iter()
            .skip(after.map_or(0, |n| n + 1) as usize)
            .take(limit.min(2))
            .cloned()
            .collect())
    }
    async fn transact(&mut self, _: Transaction) -> vcp_store::Result<Receipt> {
        panic!("fork source query mutated")
    }
}
fn comparable(value: Result<Source>) -> std::result::Result<serde_json::Value, String> {
    value
        .map(|source| {
            serde_json::to_value((
                source.session,
                source.turn,
                source.historical_task,
                source.through_watermark,
            ))
            .unwrap()
        })
        .map_err(|error| error.to_string())
}

struct BoundedOwner(Store);
impl vcp_store::contract::reference::ReferenceStore for BoundedOwner {
    fn state(&self) -> &State {
        panic!("public fork bypassed bounded history")
    }
    fn current(&self) -> CurrentStateView<'_> {
        self.0.current()
    }
    async fn history_event(&self, id: &EventId) -> vcp_store::Result<Option<EventEnvelope>> {
        self.0.history_event(id).await
    }
    async fn history_event_count(&self) -> vcp_store::Result<u64> {
        self.0.history_event_count().await
    }
    async fn history_events(
        &self,
        after: Option<u64>,
        limit: usize,
    ) -> vcp_store::Result<Vec<EventEnvelope>> {
        self.0.history_events(after, limit.min(2)).await
    }
    async fn command_receipt(
        &self,
        workspace: &WorkspaceId,
        command: &CommandId,
        digest: &str,
    ) -> vcp_store::Result<Option<CommandReceipt>> {
        self.0.command_receipt(workspace, command, digest).await
    }
    async fn transact(&mut self, transaction: Transaction) -> vcp_store::Result<Receipt> {
        self.0.transact(transaction).await
    }
}

#[tokio::test]
async fn bounded_fork_proof_matches_original_and_rejects_interior_gaps() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (engine, through) = fixture(temp.path(), backend).await;
        let original = engine.store().archive_state().await.unwrap();
        let selected =
            source_reference(&original, &access().workspace, &access().session, &through).unwrap();
        for variant in 0..14 {
            let mut state = original.clone();
            let cause = state
                .events
                .iter()
                .position(|row| row.event.id == selected.turn.cause)
                .unwrap();
            let snapshot = state
                .events
                .iter()
                .position(|row| {
                    row.event.workspace == access().workspace
                        && row.event.task.as_ref() == Some(&selected.turn.scope.task)
                        && row.event.kind == EventKind::TaskCreated
                })
                .unwrap();
            match variant {
                0 => {}
                1 => {
                    state.events.remove(cause);
                }
                2 => state.events[cause].event.session = SessionId::new(),
                3 => state.events[cause].event.data["facts"] = serde_json::json!([]),
                4 => state.events[snapshot].event.data["schema_version"] = serde_json::json!(2),
                5 => {
                    state.events[snapshot].event.data["facts"] =
                        serde_json::json!({"invalid":"array required"})
                }
                6 => {
                    state.events[snapshot].redaction =
                        Some(vcp_domain::redaction::ContentRedaction {
                            deletion: DeletionEpoch::new(1),
                            original_digest: "a".repeat(64),
                        })
                }
                7 => {
                    let fact = state.events[cause].event.data["facts"][0].clone();
                    state.events[cause].event.data["facts"]
                        .as_array_mut()
                        .unwrap()
                        .push(fact);
                }
                8 => {
                    state
                        .records
                        .get_mut(&key(Collection::Task, selected.turn.scope.task.as_str()))
                        .unwrap()
                        .value["redaction"] =
                        serde_json::json!({"deletion":"1","original_digest":"a".repeat(64)})
                }
                _ => {
                    let row = &state.events[snapshot];
                    let mut mask = RetentionMask {
                        schema_version: 1,
                        workspace: access().workspace,
                        session: access().session,
                        first: row.sequence,
                        last: row.sequence,
                        artifacts: vec![],
                        deletion: DeletionEpoch::ZERO,
                        reason: "fork fixture".into(),
                    };
                    match variant {
                        9 => {}
                        10 => mask.session = SessionId::new(),
                        11 => mask.artifacts.push(selected.turn.trigger.clone()),
                        12 => {}
                        _ => mask.deletion = DeletionEpoch::new(1),
                    }
                    let mut record = Record::typed(
                        Collection::Tombstone,
                        "fork-mask",
                        access().workspace,
                        Revision::ZERO,
                        &mask,
                    )
                    .unwrap();
                    if variant == 12 {
                        record.value["schema_version"] = serde_json::json!(2);
                    }
                    state.records.insert(record.key(), record);
                }
            }
            let expected = comparable(source_reference(
                &state,
                &access().workspace,
                &access().session,
                &through,
            ));
            assert_eq!(
                comparable(source(
                    &state,
                    &access().workspace,
                    &access().session,
                    &through
                )),
                expected,
                "archive variant {variant}"
            );
            let reader = Reader::new(engine.store(), &state);
            assert_eq!(
                comparable(
                    source_store(&reader, &access().workspace, &access().session, &through).await
                ),
                expected,
                "live variant {variant}"
            );
            if variant == 0 {
                assert_eq!(
                    reader.pages.get(),
                    reader.events.len().div_ceil(2),
                    "full ordered stream across short pages"
                );
                assert_eq!(
                    comparable(
                        source_store(
                            engine.store(),
                            &access().workspace,
                            &access().session,
                            &through
                        )
                        .await
                    ),
                    expected
                );
            }
        }
        for failed_page in [None, Some(1), Some(2)] {
            let mut reader = Reader::new(engine.store(), &original);
            reader.fail_exact = failed_page.is_none();
            reader.fail_page = failed_page;
            assert!(matches!(
                source_store(&reader, &access().workspace, &access().session, &through).await,
                Err(Error::Unavailable)
            ));
        }
        let mut owner = Engine::new(BoundedOwner(engine.into_store())).unwrap();
        let request = call("bounded-public-fork");
        let result = owner
            .handle_public(request.clone(), &access(), &facts())
            .await
            .unwrap();
        let watermark = owner.store().current().watermark;
        assert_eq!(
            owner
                .handle_public(request, &access(), &facts())
                .await
                .unwrap(),
            result
        );
        assert_eq!(owner.store().current().watermark, watermark);
        let forked: Task = owner
            .store()
            .current()
            .record(Collection::Task, "forked-task", &access().workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(forked.state, TaskState::Pending);
        assert_eq!(forked.fingerprint, selected.historical_task.fingerprint);
        owner.into_store().0.close().await.unwrap();
    }
}
