// SPDX-License-Identifier: Apache-2.0
use super::*;

struct Paged<'a> {
    store: &'a mut Store,
    mode: u8,
    writes: usize,
}
impl CanonicalStore for Paged<'_> {
    fn state(&self) -> &State {
        panic!("live projector must not read full State")
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
    ) -> vcp_store::Result<Vec<EventEnvelope>> {
        if self.mode == 1 || self.mode == 3 && after.is_some() {
            return Err(vcp_store::Error::Corruption(
                "injected projector history failure",
            ));
        }
        if self.mode == 2 {
            return Ok(Vec::new());
        }
        self.store.history_events(after, limit.min(2)).await
    }
    async fn transact(&mut self, transaction: Transaction) -> vcp_store::Result<Receipt> {
        self.writes += 1;
        self.store.transact(transaction).await
    }
}

#[tokio::test]
async fn bounded_projector_matches_archive_and_never_publishes_failed_reads() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temporary = tempfile::tempdir().unwrap();
        let mut fixture = fixture(temporary.path(), backend).await;
        let workspace = access().workspace;
        for version in [1, 1, 2] {
            let expected =
                projection::prepare_activation(fixture.engine.store().state(), &workspace, version)
                    .unwrap()
                    .1;
            let mut reader = Paged {
                store: fixture.engine.store_mut(),
                mode: 0,
                writes: 0,
            };
            let actual = projection::publish(&mut reader, &workspace, version)
                .await
                .unwrap();
            assert_eq!(actual, expected);
            let writes = reader.writes;
            assert_eq!(
                projection::publish(&mut reader, &workspace, version)
                    .await
                    .unwrap(),
                actual
            );
            assert_eq!(
                reader.writes, writes,
                "no-new-event shortcut must not republish"
            );
            // Both fresh-version rebuild and same-version absence proof fail closed.
            for mode in [1, 2, 3] {
                reader.mode = mode;
                for requested in [version, 3 - version] {
                    assert!(projection::publish(&mut reader, &workspace, requested)
                        .await
                        .is_err());
                    assert_eq!(reader.writes, writes);
                }
            }
            drop(reader);
            let id = TaskId::new();
            issue(&mut fixture.engine, create(&id, None, None), Some(id), 0).await;
        }
    }
}
