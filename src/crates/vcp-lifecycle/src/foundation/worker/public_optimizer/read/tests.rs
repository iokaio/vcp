// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_domain::{
    workspace::{Binding, Session, Trust},
    *,
};
use vcp_protocol::event::{EventInput, EventKind};
use vcp_store::{
    contract::{CanonicalStore, Mutation, Record, Transaction},
    BackendKind,
};

async fn setup(path: &std::path::Path, backend: BackendKind) -> (Store, Access, Workspace) {
    let mut store = Store::open(path, backend, &[]).await.unwrap();
    let workspace = Workspace {
        id: WorkspaceId::new(),
        binding: Binding {
            host: HostId::new(),
            root: "C:/synthetic".into(),
            repository: "repo".into(),
            worktree: "main".into(),
            revision: Revision::ZERO,
        },
        trust: Trust::Trusted,
        revision: Revision::ZERO,
        authority: AuthorityRevision::ZERO,
        deletion: DeletionEpoch::ZERO,
    };
    let session = Session {
        id: SessionId::new(),
        workspace: workspace.id.clone(),
        revision: Revision::ZERO,
        configuration: Revision::ZERO,
        fork_origin: None,
        fork_through: None,
    };
    let access = Access {
        workspace: workspace.id.clone(),
        session: session.id.clone(),
        actor: ActorId::new(),
        authority: workspace.authority,
        read: true,
        write: true,
        bootstrap: false,
    };
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: Watermark::ZERO,
            mutations: vec![
                Mutation::Put {
                    record: Record::typed(
                        Collection::Workspace,
                        workspace.id.as_str(),
                        workspace.id.clone(),
                        Revision::ZERO,
                        &workspace,
                    )
                    .unwrap(),
                    expected: None,
                },
                Mutation::Put {
                    record: Record::typed(
                        Collection::Session,
                        session.id.as_str(),
                        workspace.id.clone(),
                        Revision::ZERO,
                        &session,
                    )
                    .unwrap(),
                    expected: None,
                },
            ],
            events: vec![EventInput {
                id: EventId::new(),
                workspace: workspace.id.clone(),
                session: session.id,
                task: None,
                actor: access.actor.clone(),
                correlation: CommandId::new(),
                causation: None,
                timestamp: Timestamp::new(1),
                kind: EventKind::SessionStarted,
                artifacts: vec![],
                data: serde_json::json!({}),
                metadata: None,
            }],
            command: None,
        })
        .await
        .unwrap();
    (store, access, workspace)
}
async fn put(store: &mut Store, record: Record, expected: Revision) {
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put {
                record,
                expected: Some(expected),
            }],
            events: vec![],
            command: None,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn hostile_report_rows_page_without_skips_and_retention_change_clears_access() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access, mut workspace) = setup(temp.path(), backend).await;
        let governed = memory_access(&store, &access, true, true, &|| Ok(())).unwrap();
        let capture = service::Command {
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            actor: access.actor.clone(),
            id: CommandId::parse("hostile-report").unwrap(),
            digest: "a".repeat(64),
            expected_revision: Revision::ZERO,
            expected_binding_revision: Revision::ZERO,
        };
        let captured = service::capture(
            &mut store,
            &governed,
            &capture,
            service::Coverage::Workspace,
            routing_state::HistoryWindow {
                from: None,
                until: Timestamp::new(100),
            },
            Timestamp::new(2),
            &|| Ok(()),
        )
        .await
        .unwrap();
        let service::Outcome::Report { report, .. } = captured.outcome else {
            panic!("captured report")
        };
        // Test-only canonical fixture enrichment. Production capture never accepts
        // caller report rows. Keep the genuine binding and governed read gate.
        let mut record = store
            .state()
            .record(Collection::Projection, &report, &access.workspace)
            .unwrap()
            .clone();
        let hostile = "\u{0001}".repeat(512);
        let mut cohorts = serde_json::Map::new();
        for n in 0..40u64 {
            let key=serde_json::to_string(&serde_json::json!({"provider":format!("{n:02}{hostile}"),"model":hostile,"catalog":hostile,"routing_policy":hostile,"authority_policy":"0","task_class":hostile,"size":"unknown","role":"main"})).unwrap();
            cohorts.insert(key, serde_json::json!(n + 1));
        }
        record.value["cohorts"] = serde_json::Value::Object(cohorts);
        record.value["uncertainty"] = serde_json::json!(["文".repeat(1000)]);
        record.revision = Revision::new(1);
        put(&mut store, record, Revision::ZERO).await;
        let mut request = wire::ReportRead {
            scope: methods::Scope {
                workspace: id(access.workspace.as_str()).unwrap(),
                session: id(access.session.as_str()).unwrap(),
            },
            report: id(capture.id.as_str()).unwrap(),
            section: wire::ReportSection::Cohorts,
            limit: 32,
            cursor: None,
        };
        let state = store.state().clone();
        let first = read(&store, &access, &request, true, &|| Ok(())).unwrap();
        assert!(!first.complete);
        assert!(first.rows.len() < 32);
        assert!(!first.rows.is_empty());
        let first_cursor = first.next_cursor.clone();
        let mut counts = Vec::new();
        let mut pages = 0;
        loop {
            let page = read(&store, &access, &request, true, &|| Ok(())).unwrap();
            assert!(serde_json::to_vec(&page).unwrap().len() <= wire::MAX_PAGE_BYTES);
            for row in page.rows {
                let wire::ReportRow::Cohort { value } = row else {
                    panic!("cohort")
                };
                assert!(value.provider.truncated);
                counts.push(counter(&value.count).unwrap());
            }
            pages += 1;
            if page.complete {
                break;
            }
            request.cursor = page.next_cursor;
            assert!(pages < 41);
        }
        counts.sort();
        assert_eq!(counts, (1..=40).collect::<Vec<_>>());
        assert_eq!(store.state(), &state);
        request.cursor = first_cursor;
        let mut foreign = request.clone();
        foreign.scope.session = id("foreign").unwrap();
        assert!(read(&store, &access, &foreign, true, &|| Ok(())).is_err());
        let mut other_actor = access.clone();
        other_actor.actor = ActorId::new();
        assert!(read(&store, &other_actor, &request, true, &|| Ok(())).is_err());
        assert!(read(&store, &access, &request, false, &|| Ok(())).is_err());
        assert!(read(&store, &access, &request, true, &|| Err(failure(
            Code::Cancelled
        )))
        .is_err());
        let mut changed = request.clone();
        changed.section = wire::ReportSection::Uncertainty;
        assert!(read(&store, &access, &changed, true, &|| Ok(())).is_err());
        changed.cursor = None;
        let page = read(&store, &access, &changed, true, &|| Ok(())).unwrap();
        let wire::ReportRow::Uncertainty { text } = &page.rows[0] else {
            panic!("uncertainty")
        };
        assert!(text.truncated);
        assert!(text.text.len() <= 512);
        // A changed canonical deletion epoch invalidates the original report,
        // including a fresh first page, rather than replaying the saved payload.
        workspace.deletion = DeletionEpoch::new(1);
        workspace.revision = Revision::new(1);
        put(
            &mut store,
            Record::typed(
                Collection::Workspace,
                workspace.id.as_str(),
                workspace.id.clone(),
                workspace.revision,
                &workspace,
            )
            .unwrap(),
            Revision::ZERO,
        )
        .await;
        assert!(read(&store, &access, &request, true, &|| Ok(())).is_err());
        request.cursor = None;
        assert!(read(&store, &access, &request, true, &|| Ok(())).is_err());
    }
}
