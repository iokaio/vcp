// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{cell::Cell, sync::Arc};
use vcp_audit::history_query::{self, Query};
use vcp_domain::retention_selector::{Criterion, Selector, Tree};
use vcp_store::{CurrentState, CurrentStateView};

struct Paged {
    current: Arc<CurrentState>,
    events: Vec<EventEnvelope>,
    reads: Cell<usize>,
    mode: u8,
}
impl vcp_store::contract::reference::ReferenceStore for Paged {
    fn state(&self) -> &State {
        panic!("live audit must not materialize State")
    }
    fn current(&self) -> CurrentStateView<'_> {
        self.current.as_ref().into()
    }
    async fn history_event_count(&self) -> vcp_store::Result<u64> {
        self.reads.set(self.reads.get() + 1);
        Ok(self.events.len() as u64)
    }
    async fn history_event_at(&self, ordinal: u64) -> vcp_store::Result<Option<EventEnvelope>> {
        self.reads.set(self.reads.get() + 1);
        if self.mode == 3 {
            return Ok(None);
        }
        Ok(self.events.get(ordinal as usize).cloned())
    }
    async fn history_event(&self, id: &EventId) -> vcp_store::Result<Option<EventEnvelope>> {
        self.reads.set(self.reads.get() + 1);
        if self.mode == 1 {
            return Err(vcp_store::Error::Corruption(
                "injected exact origin failure",
            ));
        }
        if self.mode == 3 {
            return Ok(None);
        }
        if self.mode == 5 {
            let mut event = self.events[0].clone();
            event.event.id = EventId::new();
            return Ok(Some(event));
        }
        Ok(self.events.iter().find(|e| &e.event.id == id).cloned())
    }
    async fn history_events(
        &self,
        after: Option<u64>,
        limit: usize,
    ) -> vcp_store::Result<Vec<EventEnvelope>> {
        self.reads.set(self.reads.get() + 1);
        assert!((1..=4096).contains(&limit));
        if self.mode == 1 {
            return Err(vcp_store::Error::Corruption(
                "injected authenticated page failure",
            ));
        }
        if self.mode == 2 {
            return Ok(Vec::new());
        }
        let first = after.map_or(0, |value| value + 1) as usize;
        if self.mode == 4 && first > 0 {
            return Err(vcp_store::Error::Corruption(
                "injected interior page failure",
            ));
        }
        // Deliberately short nonempty pages model a byte boundary, not EOF.
        Ok(self
            .events
            .iter()
            .skip(first)
            .take(limit.min(2))
            .cloned()
            .collect())
    }
    async fn transact(&mut self, _: Transaction) -> vcp_store::Result<Receipt> {
        panic!("audit must not mutate")
    }
}

#[tokio::test]
async fn inspection_ingestion_findings_require_exact_retained_origin() {
    use vcp_audit::inspection::{self, InspectionQuery, View};
    use vcp_domain::ingestion::*;
    let temporary = tempfile::tempdir().unwrap();
    let fixture = fixture(temporary.path(), BackendKind::Files).await;
    let mut state = fixture.engine.store().archive_state().await.unwrap();
    let event = state
        .events
        .iter()
        .find(|e| e.event.task.as_ref() == Some(&fixture.root))
        .unwrap();
    let mut job = Job {
        document_type: DocumentType::Job,
        schema_version: 1,
        id: CommandId::new(),
        scope: Scope {
            workspace: access().workspace,
            session: event.event.session.clone(),
            task: fixture.root.clone(),
        },
        root: fixture.root.clone(),
        revision: Revision::ZERO,
        cursor: CommandId::new(),
        extractor: ExtractorSpec {
            name: "fixture".into(),
            version: 1,
            event_kinds: vec!["task_created".into()],
        },
        origin: event.event.id.clone(),
        origin_watermark: event.watermark,
        state: JobState::Completed,
        attempts: Units::new(1),
        max_attempts: Units::new(1),
        lease: None,
        not_before: Timestamp::ZERO,
        last_failure: None,
        results: Vec::new(),
        finding: Some("observed-finding".into()),
    };
    job.id = CommandId::parse(vcp_protocol::digest_bytes(
        &vcp_protocol::canonical_bytes(&("ingestion-job/1", &job.cursor, &job.origin)).unwrap(),
    ))
    .unwrap();
    job.validate().unwrap();
    let row = Record::typed(
        Collection::Claim,
        job.id.as_str(),
        access().workspace,
        Revision::ZERO,
        &job,
    )
    .unwrap();
    state
        .records
        .insert(key(Collection::Claim, job.id.as_str()), row.clone());
    let mut current = fixture.engine.store().current_state();
    Arc::make_mut(&mut current)
        .records
        .insert(key(Collection::Claim, job.id.as_str()), row);
    let mut reader = Paged {
        current,
        events: state.events.to_vec(),
        reads: Cell::new(0),
        mode: 0,
    };
    let request = InspectionQuery {
        id: fixture.root.to_string(),
        view: View::Memory,
        limit: 128,
        cursor: None,
        range: None,
    };
    let expected = inspection::records(&state, &access(), &request).unwrap();
    assert_eq!(
        inspection::records_store(&reader, &access(), &request)
            .await
            .unwrap(),
        expected
    );
    assert!(serde_json::to_string(&expected)
        .unwrap()
        .contains("observed-finding"));
    reader.mode = 3;
    let missing = inspection::records_store(&reader, &access(), &request)
        .await
        .unwrap();
    assert!(!serde_json::to_string(&missing)
        .unwrap()
        .contains("observed-finding"));
    for mode in [1, 5] {
        reader.mode = mode;
        assert!(inspection::records_store(&reader, &access(), &request)
            .await
            .is_err());
    }
}

#[tokio::test]
async fn live_inspection_matches_archive_cursor_scope_and_short_pages() {
    use vcp_audit::inspection::{self, InspectionQuery, View};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temporary = tempfile::tempdir().unwrap();
        let fixture = fixture(temporary.path(), backend).await;
        let state = &fixture.engine.store().archive_state().await.unwrap();
        let mut reader = Paged {
            current: fixture.engine.store().current_state(),
            events: state.events.to_vec(),
            reads: Cell::new(0),
            mode: 0,
        };
        let mut access = access();
        access.tasks = Some([fixture.root.clone()].into());
        for view in [
            View::Chain,
            View::Context,
            View::Prompts,
            View::Outputs,
            View::Routing,
            View::Policy,
            View::Tools,
            View::Costs,
            View::Verification,
            View::Memory,
        ] {
            let mut request = InspectionQuery {
                id: fixture.root.to_string(),
                view,
                limit: 2,
                cursor: None,
                range: None,
            };
            loop {
                let expected = inspection::records(state, &access, &request).unwrap();
                let actual = inspection::records_store(&reader, &access, &request)
                    .await
                    .unwrap();
                assert_eq!(
                    serde_json::to_value(&actual).unwrap(),
                    serde_json::to_value(&expected).unwrap()
                );
                request.cursor = actual.next_cursor;
                if request.cursor.is_none() {
                    break;
                }
            }
        }
        // Begin after all current records, forcing actual global ordinal reads.
        let mut request = InspectionQuery {
            id: fixture.root.to_string(),
            view: View::Chain,
            limit: 128,
            cursor: None,
            range: None,
        };
        let initial = inspection::records_store(&reader, &access, &request)
            .await
            .unwrap();
        assert!(initial
            .items
            .iter()
            .any(|item| item["reference"].as_str().unwrap().starts_with("z:event:")));
        for mode in [1, 2, 4] {
            reader.mode = mode;
            assert!(inspection::records_store(&reader, &access, &request)
                .await
                .is_err());
        }
        reader.mode = 0;
        request.limit = 1;
        request.cursor = inspection::records_store(&reader, &access, &request)
            .await
            .unwrap()
            .next_cursor;
        request.cursor.as_mut().unwrap().watermark = Watermark::ZERO;
        assert!(matches!(
            inspection::records_store(&reader, &access, &request).await,
            Err(Error::Restart(_))
        ));
        access.read = false;
        reader.reads.set(0);
        assert!(matches!(
            inspection::records_store(&reader, &access, &request).await,
            Err(Error::Access)
        ));
        assert_eq!(reader.reads.get(), 0);
    }
}
fn query() -> Query {
    Query {
        selector: Selector {
            schema_version: 1,
            tree: Tree::Match(Criterion::Workspace(access().workspace)),
        },
        text: None,
        limit: 2,
        cursor: None,
        artifact: None,
        expand_compacted: false,
    }
}
fn same(a: &history_query::Page, b: &history_query::Page) {
    assert_eq!(
        serde_json::to_value(a).unwrap(),
        serde_json::to_value(b).unwrap()
    );
}

#[tokio::test]
async fn live_pages_match_archive_with_short_reads_append_scope_and_failures() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temporary = tempfile::tempdir().unwrap();
        let mut fixture = fixture(temporary.path(), backend).await;
        let mut request = query();
        let first = history_query::query(
            &fixture.engine.store().archive_state().await.unwrap(),
            &access(),
            &request,
        )
        .unwrap();
        same(
            &first,
            &history_query::query_store(fixture.engine.store(), &access(), &request)
                .await
                .unwrap(),
        );
        for _ in 0..70 {
            let id = TaskId::new();
            issue(&mut fixture.engine, create(&id, None, None), Some(id), 0).await;
        }
        let state = &fixture.engine.store().archive_state().await.unwrap();
        let mut reader = Paged {
            current: fixture.engine.store().current_state(),
            events: state.events.to_vec(),
            reads: Cell::new(0),
            mode: 0,
        };
        request.cursor = first.next_cursor;
        assert!(request.cursor.is_some());
        while request.cursor.is_some() {
            let expected = history_query::query(state, &access(), &request).unwrap();
            assert!(expected.newer_events > 0);
            same(
                &expected,
                &history_query::query_store(&reader, &access(), &request)
                    .await
                    .unwrap(),
            );
            request.cursor = expected.next_cursor;
        }
        request.cursor = None;
        let mut scan = request.clone();
        scan.limit = 128;
        scan.text = Some("no matching retained event text".into());
        let session = SessionId::parse("session").unwrap();
        let page = history_query::query_store_session(&reader, &access(), &scan, &session)
            .await
            .unwrap();
        assert_eq!(page.next_cursor.as_ref().unwrap().after, 64);
        same(
            &history_query::query_session(state, &access(), &scan, &session).unwrap(),
            &page,
        );
        for session in [SessionId::parse("session").unwrap(), SessionId::new()] {
            same(
                &history_query::query_session(state, &access(), &request, &session).unwrap(),
                &history_query::query_store_session(&reader, &access(), &request, &session)
                    .await
                    .unwrap(),
            );
        }
        request.artifact = Some(fixture.request.spec.id.clone());
        same(
            &history_query::query(state, &access(), &request).unwrap(),
            &history_query::query_store(&reader, &access(), &request)
                .await
                .unwrap(),
        );
        request.artifact = None;
        request.limit = 128;
        let mut changed = scan.clone();
        changed.cursor = Some(page.next_cursor.unwrap());
        changed.cursor.as_mut().unwrap().boundary_digest = "0".repeat(64);
        assert!(matches!(
            history_query::query_store_session(&reader, &access(), &changed, &session).await,
            Err(Error::Restart(_))
        ));
        for mode in [1, 2, 3, 4] {
            reader.mode = mode;
            assert!(history_query::query_store(&reader, &access(), &request)
                .await
                .is_err());
        }
        reader.mode = 0;
        let checks = Cell::new(0);
        assert!(history_query::query_store_session_with_check(
            &reader,
            &access(),
            &request,
            &session,
            &|| {
                checks.set(checks.get() + 1);
                if checks.get() > 5 {
                    Err(Error::Restart("cancelled by caller"))
                } else {
                    Ok(())
                }
            }
        )
        .await
        .is_err());
        assert!(checks.get() > 5);
        let mut denied = access();
        denied.read = false;
        reader.reads.set(0);
        assert!(matches!(
            history_query::query_store(&reader, &denied, &request).await,
            Err(Error::Access)
        ));
        assert_eq!(reader.reads.get(), 0, "access precedes historical I/O");
    }
}

struct Resident(State);
impl vcp_store::contract::reference::ReferenceStore for Resident {
    fn state(&self) -> &State {
        &self.0
    }
    async fn transact(&mut self, _: Transaction) -> vcp_store::Result<Receipt> {
        panic!("read only")
    }
}
#[tokio::test]
async fn ordinal_reader_bounds_bytes_and_preserves_exact_global_positions() {
    let temporary = tempfile::tempdir().unwrap();
    let fixture = fixture(temporary.path(), BackendKind::Files).await;
    let mut state = fixture.engine.store().archive_state().await.unwrap();
    let mut event = state.events[0].clone();
    event.event.data = serde_json::json!({"large": "x".repeat(6 * 1024 * 1024)});
    state.events = (0..3)
        .map(|_| {
            let mut row = event.clone();
            row.event.id = EventId::new();
            row
        })
        .collect();
    let reader = Resident(state);
    assert_eq!(reader.history_event_count().await.unwrap(), 3);
    let first = reader.history_events(None, 4096).await.unwrap();
    assert_eq!(first.len(), 2);
    assert!(
        first
            .iter()
            .map(|row| vcp_protocol::canonical_bytes(row).unwrap().len())
            .sum::<usize>()
            <= MAX_COMMIT_BYTES
    );
    assert_eq!(
        reader.history_events(Some(1), 4096).await.unwrap(),
        reader.0.events[2..]
    );
    assert!(reader.history_events(Some(2), 1).await.unwrap().is_empty());
    assert!(reader.history_events(Some(3), 1).await.is_err());
    assert!(reader.history_events(Some(u64::MAX), 1).await.is_err());
    assert!(reader.history_events(None, 0).await.is_err());
    assert!(reader.history_events(None, 4097).await.is_err());
    assert_eq!(
        reader.history_event_at(1).await.unwrap().as_ref(),
        Some(&reader.0.events[1])
    );
    assert!(reader.history_event_at(3).await.unwrap().is_none());
}
