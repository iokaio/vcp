// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::cell::Cell;
use vcp_protocol::event::EventEnvelope;
use vcp_store::{
    contract::{CanonicalStore, Receipt, Transaction},
    CurrentState, CurrentStateView,
};

struct Reader {
    current: CurrentState,
    events: Vec<EventEnvelope>,
    exact_reads: Cell<usize>,
    pages: Cell<usize>,
    fail_exact: bool,
    fail_page: bool,
}
impl Reader {
    fn new(store: &Store) -> Self {
        Self {
            current: store.current_state().as_ref().clone(),
            events: store.state().events.to_vec(),
            exact_reads: Cell::new(0),
            pages: Cell::new(0),
            fail_exact: false,
            fail_page: false,
        }
    }
}
impl CanonicalStore for Reader {
    fn state(&self) -> &State {
        panic!("origin navigation materialized State")
    }
    fn current(&self) -> CurrentStateView<'_> {
        (&self.current).into()
    }
    async fn history_event(&self, id: &EventId) -> vcp_store::Result<Option<EventEnvelope>> {
        self.exact_reads.set(self.exact_reads.get() + 1);
        if self.fail_exact {
            return Err(vcp_store::Error::Corruption("missing origin index page"));
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
        if self.fail_page {
            return Err(vcp_store::Error::Corruption(
                "missing retention history page",
            ));
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
        panic!("origin query mutated")
    }
}

async fn query(
    reader: &Reader,
    access: &Access,
    origins: &std::collections::BTreeSet<EventId>,
) -> vcp_memory::Result<serde_json::Value> {
    let result =
        history::origin_links_store_with_check(reader, access, origins, &|| Ok(())).await?;
    Ok(serde_json::to_value(result).unwrap())
}

#[tokio::test]
async fn bounded_origin_navigation_preserves_scopes_absence_and_read_failures() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        propose(
            &mut f.store,
            &f.access,
            f.proposal.clone(),
            Timestamp::new(200),
        )
        .await
        .unwrap();
        let origins = f.proposal.origins.iter().cloned().collect();
        let mut reader = Reader::new(&f.store);
        let expected =
            serde_json::to_value(history::origin_links(&f.store, &f.access, &origins).unwrap())
                .unwrap();
        assert_eq!(
            serde_json::to_value(
                history::origin_links_store_with_check(&f.store, &f.access, &origins, &|| Ok(()),)
                    .await
                    .unwrap()
            )
            .unwrap(),
            expected,
            "real owner exact history adapter"
        );
        assert_eq!(query(&reader, &f.access, &origins).await.unwrap(), expected);
        assert!(reader.exact_reads.get() > 0);
        assert_eq!(reader.pages.get(), 0, "no masks need no scan");
        reader.fail_exact = true;
        assert!(query(&reader, &f.access, &origins).await.is_err());
        let denied = Access {
            workspace: f.access.workspace.clone(),
            actor: f.access.actor.clone(),
            authority: f.access.authority,
            read: true,
            write: false,
            tasks: Some(Default::default()),
        };
        let before = reader.exact_reads.get();
        assert_eq!(
            query(&reader, &denied, &origins).await.unwrap(),
            serde_json::json!([[], false])
        );
        assert_eq!(
            reader.exact_reads.get(),
            before,
            "denied task never reads origin"
        );
        reader.fail_exact = false;
        let at = reader
            .events
            .iter()
            .position(|row| origins.contains(&row.event.id))
            .unwrap();
        let saved = reader.events[at].clone();
        reader.events[at].event.workspace = WorkspaceId::new();
        assert_eq!(
            query(&reader, &f.access, &origins).await.unwrap(),
            serde_json::json!([[], false])
        );
        reader.events.remove(at);
        assert_eq!(
            query(&reader, &f.access, &origins).await.unwrap(),
            expected,
            "ordinary missing origin stays available"
        );
        reader.events.insert(at, saved);
        let checks = Cell::new(0);
        let result = history::origin_links_store_with_check(&reader, &f.access, &origins, &|| {
            checks.set(checks.get() + 1);
            if checks.get() > 2 {
                Err(vcp_memory::Error::Conflict("cancelled"))
            } else {
                Ok(())
            }
        })
        .await;
        assert!(result.is_err());
        f.store.close().await.unwrap();
    }
}

fn mask(reader: &mut Reader, access: &Access, row: &EventEnvelope, name: &str) {
    let mask = vcp_domain::retention::RetentionMask {
        schema_version: 1,
        workspace: access.workspace.clone(),
        session: row.event.session.clone(),
        first: row.sequence,
        last: row.sequence,
        artifacts: vec![],
        deletion: DeletionEpoch::ZERO,
        reason: "typed provenance fixture".into(),
    };
    let record = vcp_store::contract::Record::typed(
        Collection::Tombstone,
        name,
        access.workspace.clone(),
        Revision::ZERO,
        &mask,
    )
    .unwrap();
    reader.current.records.insert(record.key(), record);
}

#[tokio::test]
async fn retention_origin_and_receiptless_correlation_use_one_bounded_scan() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let first = propose(
            &mut f.store,
            &f.access,
            f.proposal.clone(),
            Timestamp::new(200),
        )
        .await
        .unwrap();
        let mut second = different_output(&f.proposal, "second-origin-link");
        second.predecessor = first.result.version;
        second.correction_reason = Some("explicit later source confirmation".into());
        propose(&mut f.store, &f.access, second, Timestamp::new(201))
            .await
            .unwrap();
        let origins: std::collections::BTreeSet<_> = f.proposal.origins.iter().cloned().collect();
        for by_origin in [true, false] {
            let mut reader = Reader::new(&f.store);
            let row = reader
                .events
                .iter()
                .find(|row| {
                    if by_origin {
                        origins.contains(&row.event.id)
                    } else {
                        row.event.correlation == f.proposal.command
                            && row.event.kind == EventKind::MemoryResolved
                    }
                })
                .unwrap()
                .clone();
            mask(&mut reader, &f.access, &row, "mask");
            let result = query(&reader, &f.access, &origins).await.unwrap();
            assert_eq!(result[0].as_array().unwrap().len(), usize::from(!by_origin));
            assert_eq!(result[1], false);
            assert_eq!(reader.pages.get(), reader.events.len().div_ceil(2));
            reader.fail_page = true;
            assert!(query(&reader, &f.access, &origins).await.is_err());
            reader.fail_page = false;
            let key = vcp_store::contract::key(Collection::Tombstone, "mask");
            reader.current.records.get_mut(&key).unwrap().value["schema_version"] =
                serde_json::json!(2);
            let before = reader.pages.get();
            assert!(query(&reader, &f.access, &origins).await.is_err());
            assert_eq!(
                reader.pages.get(),
                before,
                "invalid mask rejected before history scan"
            );
        }
        f.store.close().await.unwrap();
    }
}

#[tokio::test]
async fn redacted_conflict_origins_require_presence_and_current_source_scope() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        propose(
            &mut f.store,
            &f.access,
            f.proposal.clone(),
            Timestamp::new(200),
        )
        .await
        .unwrap();
        let origins = f.proposal.origins.iter().cloned().collect();
        let version = versions(f.store.state(), &f.access.workspace).remove(0);
        let mut redacted =
            vcp_protocol::redaction::version(&version, DeletionEpoch::new(1)).unwrap();
        redacted.id = ClaimVersionId::new();
        redacted.sources = vcp_domain::redaction::Sources {
            origins: vec![EventId::new()],
            ..Default::default()
        };
        let mut reader = Reader::new(&f.store);
        let key = vcp_store::contract::key(Collection::Claim, version.id.as_str());
        reader.current.records.get_mut(&key).unwrap().value["resolution"]["conflicts"] =
            serde_json::json!([redacted.id]);
        let record = vcp_store::contract::Record::typed(
            Collection::Claim,
            redacted.id.as_str(),
            f.access.workspace.clone(),
            redacted.revision,
            &redacted,
        )
        .unwrap();
        reader.current.records.insert(record.key(), record);
        assert_eq!(
            query(&reader, &f.access, &origins).await.unwrap(),
            serde_json::json!([[], false])
        );
        let key = vcp_store::contract::key(Collection::Claim, redacted.id.as_str());
        reader.current.records.get_mut(&key).unwrap().value["sources"]["origins"] =
            serde_json::json!(f.proposal.origins);
        assert_eq!(
            query(&reader, &f.access, &origins).await.unwrap()[0]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let at = reader
            .events
            .iter()
            .position(|row| origins.contains(&row.event.id))
            .unwrap();
        reader.events[at].event.task = Some(TaskId::new());
        let scoped = Access {
            workspace: f.access.workspace.clone(),
            actor: f.access.actor.clone(),
            authority: f.access.authority,
            read: true,
            write: false,
            tasks: Some([f.proposal.scope.task.clone()].into_iter().collect()),
        };
        assert_eq!(
            query(&reader, &scoped, &origins).await.unwrap(),
            serde_json::json!([[], false])
        );
        f.store.close().await.unwrap();
    }
}

#[tokio::test]
async fn early_navigation_limit_does_not_validate_later_malformed_claims() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        propose(
            &mut f.store,
            &f.access,
            f.proposal.clone(),
            Timestamp::new(200),
        )
        .await
        .unwrap();
        let origins = f.proposal.origins.iter().cloned().collect();
        let version = versions(f.store.state(), &f.access.workspace).remove(0);
        let mut reader = Reader::new(&f.store);
        reader.current.records.remove(&vcp_store::contract::key(
            Collection::Claim,
            version.id.as_str(),
        ));
        for index in 0..130 {
            let mut version = version.clone();
            version.id = ClaimVersionId::parse(format!("origin-window-{index:04}")).unwrap();
            let record = vcp_store::contract::Record::typed(
                Collection::Claim,
                version.id.as_str(),
                f.access.workspace.clone(),
                Revision::ZERO,
                &version,
            )
            .unwrap();
            reader.current.records.insert(record.key(), record);
        }
        let mut late = version;
        late.id = ClaimVersionId::parse("zzzz-malformed-later-version").unwrap();
        let mut record = vcp_store::contract::Record::typed(
            Collection::Claim,
            late.id.as_str(),
            f.access.workspace.clone(),
            Revision::ZERO,
            &late,
        )
        .unwrap();
        record.value["memory_seq"] = serde_json::json!("malformed-later-row");
        reader.current.records.insert(record.key(), record);
        // A valid unrelated mask forces dependency discovery and one full scan,
        // but must not move the later decoding error ahead of the link limit.
        let unrelated = reader
            .events
            .iter()
            .find(|row| row.event.workspace != f.access.workspace)
            .unwrap()
            .clone();
        mask(&mut reader, &f.access, &unrelated, "unrelated");
        let result = query(&reader, &f.access, &origins).await.unwrap();
        assert_eq!(result[0].as_array().unwrap().len(), 128);
        assert_eq!(result[1], true);
        assert_eq!(reader.pages.get(), reader.events.len().div_ceil(2));
        f.store.close().await.unwrap();
    }
}
