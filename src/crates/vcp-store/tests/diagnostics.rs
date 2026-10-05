// SPDX-License-Identifier: Apache-2.0
mod common;
use vcp_store::{contract::CanonicalStore, BackendKind, Store};

#[tokio::test]
async fn physical_history_reads_are_aggregated_by_replay_phase_on_both_backends() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("canonical");
        let mut store = Store::open(&root, backend, &[]).await.unwrap();
        let initial = common::initial();
        store.transact(initial.clone()).await.unwrap();
        for index in 1..=3 {
            let mut event = initial.events[0].clone();
            event.id = vcp_domain::EventId::parse(format!("read-observation-{index}")).unwrap();
            store
                .transact(vcp_store::contract::Transaction {
                    id: vcp_domain::TransactionId::parse(format!("read-observation-{index}"))
                        .unwrap(),
                    expected_watermark: store.current().watermark,
                    mutations: vec![],
                    events: vec![event],
                    command: None,
                })
                .await
                .unwrap();
        }
        let expected = store.archive_state().await.unwrap();
        let before_rejection = store.diagnostics().clone();
        assert!(store
            .transact(vcp_store::contract::Transaction {
                id: vcp_domain::TransactionId::parse("duplicate-event-rejected").unwrap(),
                expected_watermark: store.current().watermark,
                mutations: vec![],
                events: vec![initial.events[0].clone()],
                command: None,
            })
            .await
            .is_err());
        let rejected = store.diagnostics();
        assert_eq!(rejected.validation_phases.events.failed, 1);
        assert!(
            rejected.validation_history_reads.events.physical_started
                > before_rejection
                    .validation_history_reads
                    .events
                    .physical_started
        );
        assert_eq!(rejected.history_reads.physical_failed, 0);
        assert_eq!(
            rejected.validation_phases.redaction,
            before_rejection.validation_phases.redaction
        );
        let mut session = common::session();
        session.id = vcp_domain::SessionId::parse("second-session").unwrap();
        let mut task: vcp_domain::task::Task = expected
            .record(
                vcp_store::contract::Collection::Task,
                "task",
                &common::workspace().id,
            )
            .unwrap()
            .decode()
            .unwrap();
        let previous = task.revision;
        task.revision = previous.next().unwrap();
        task.scope.session = session.id.clone();
        let changed_scope = vcp_store::contract::Transaction {
            id: vcp_domain::TransactionId::parse("changed-old-event-scope").unwrap(),
            expected_watermark: store.current().watermark,
            mutations: vec![
                vcp_store::contract::Mutation::Put {
                    expected: None,
                    record: vcp_store::contract::Record::typed(
                        vcp_store::contract::Collection::Session,
                        session.id.to_string(),
                        session.workspace.clone(),
                        session.revision,
                        &session,
                    )
                    .unwrap(),
                },
                vcp_store::contract::Mutation::Put {
                    expected: Some(previous),
                    record: vcp_store::contract::Record::typed(
                        vcp_store::contract::Collection::Task,
                        "task",
                        task.scope.workspace.clone(),
                        task.revision,
                        &task,
                    )
                    .unwrap(),
                },
            ],
            events: vec![],
            command: None,
        };
        assert!(matches!(
            store.transact(changed_scope).await,
            Err(vcp_store::Error::Access)
        ));
        assert_eq!(
            store
                .diagnostics()
                .event_validation_work
                .dependency_fallbacks,
            1
        );
        assert_eq!(store.diagnostics().event_validation_work.full_passes, 1);
        assert_eq!(store.archive_state().await.unwrap(), expected);
        store.close().await.unwrap();
        let reopened = Store::open(&root, backend, &[]).await.unwrap();
        assert_eq!(reopened.archive_state().await.unwrap(), expected);
        let observed = reopened.diagnostics();
        let total = observed.history_reads;
        assert!(total.physical_started > 0);
        assert_eq!(total.physical_started, total.physical_completed);
        assert_eq!(total.physical_failed, 0);
        assert!(total.physical_bytes > 0);
        let events = observed.validation_history_reads.events;
        let redaction = observed.validation_history_reads.redaction;
        assert!(redaction.physical_started > 0);
        assert!(redaction.physical_bytes > 0);
        assert!(redaction.index_misses > 0);
        assert!(events.physical_started <= total.physical_started);
        assert_eq!(observed.validation_phases.events.completed, 4);
        assert_eq!(observed.validation_input_events, 10);
        assert_eq!(observed.event_validation_work.rows_examined, 4);
        assert_eq!(observed.event_validation_work.prefix_reuses, 4);
        assert_eq!(observed.event_validation_work.full_passes, 0);
        let json = serde_json::to_value(observed).unwrap();
        assert!(json.get("history_reads").is_some());
        assert!(json.get("validation_history_reads").is_some());
        reopened.close().await.unwrap();
    }
}

#[tokio::test]
async fn diagnostics_explain_replay_and_failed_or_duplicate_work_without_changing_state() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("canonical");
        let mut store = Store::open(&root, backend, &[]).await.unwrap();
        assert_eq!(store.diagnostics().open.completed, 1);
        assert_eq!(store.diagnostics().replayed_commits, 0);
        let initial = common::initial();
        let receipt = store.transact(initial.clone()).await.unwrap();
        let state = (&store.archive_state().await.unwrap()).clone();
        assert_eq!(store.transact(initial.clone()).await.unwrap(), receipt);
        let mut conflicting = initial;
        conflicting.events[0].data = serde_json::json!({"different":true});
        assert!(store.transact(conflicting).await.is_err());
        assert_eq!((&store.archive_state().await.unwrap()), &state);
        let observed = store.diagnostics();
        assert_eq!(observed.preparation.completed, 3);
        assert_eq!(observed.preparation.failed, 1);
        assert_eq!(observed.validation.completed, 1);
        for phase in [
            &observed.validation_phases.capacity,
            &observed.validation_phases.records,
            &observed.validation_phases.events,
            &observed.validation_phases.redaction,
            &observed.validation_phases.accounting,
            &observed.validation_phases.ingestion,
            &observed.validation_phases.search,
            &observed.validation_phases.agents,
        ] {
            assert_eq!(phase.completed, 1);
            assert_eq!(phase.failed, 0);
            assert!(phase.elapsed_micros <= observed.validation.elapsed_micros);
        }
        assert_eq!(
            observed.validation_input_records,
            state.records.len() as u64
        );
        assert_eq!(observed.validation_input_events, state.events.len() as u64);
        assert_eq!(observed.append.completed, 1);
        assert_eq!(observed.duplicate_transactions, 1);
        assert_eq!(observed.current_watermark, state.watermark.get());
        let serialized = serde_json::to_value(observed).unwrap();
        assert_eq!(serialized["schema_version"], 1);
        assert!(!serialized.to_string().contains("synthetic"));
        store.checkpoint().unwrap();
        assert_eq!(store.diagnostics().checkpoint.completed, 1);
        store.close().await.unwrap();

        let reopened = Store::open(&root, backend, &[]).await.unwrap();
        assert_eq!((&reopened.archive_state().await.unwrap()), &state);
        let observed = reopened.diagnostics();
        assert_eq!(observed.replayed_commits, 1);
        assert_eq!(observed.validation_phases.records.completed, 1);
        assert_eq!(observed.validation_phases.events.completed, 1);
        assert!(observed.replay_payload_bytes > 0);
        assert_eq!(observed.append.completed, 0);
        assert_eq!(observed.preparation.completed, 0);
        match backend {
            BackendKind::Sqlite => {
                assert_eq!(observed.materialized_verification.completed, 1);
                assert_eq!(observed.materialized_records, state.records.len() as u64);
                assert_eq!(observed.validation.completed, 1);
                assert_eq!(observed.checkpoint_replayed_commits, 0);
            }
            BackendKind::Files => {
                assert_eq!(observed.checkpoint_verification.completed, 1);
                assert_eq!(observed.checkpoint_replayed_commits, 0);
                assert_eq!(observed.checkpoint_state_comparisons, 1);
                assert_eq!(observed.validation.completed, 1);
                assert_eq!(observed.materialized_verification.completed, 0);
            }
        }
        reopened.close().await.unwrap();
    }
}

#[tokio::test]
async fn failed_open_keeps_diagnostics_and_original_ownership_error() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("canonical");
        let store = Store::open(&root, backend, &[]).await.unwrap();
        let (result, diagnostics) = Store::open_with_diagnostics(&root, backend, &[]).await;
        assert!(matches!(
            result,
            Err(vcp_store::Error::Conflict(
                "canonical root already has an owner"
            ))
        ));
        assert_eq!(diagnostics.open.completed, 1);
        assert_eq!(diagnostics.open.failed, 1);
        assert_eq!(diagnostics.backend_open.completed, 0);
        assert_eq!(diagnostics.replayed_commits, 0);
        store.close().await.unwrap();
    }
}

#[tokio::test]
async fn resealed_checkpoint_with_wrong_state_is_rejected_during_single_replay() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("canonical");
    let mut store = Store::open(&root, BackendKind::Files, &[]).await.unwrap();
    store.transact(common::initial()).await.unwrap();
    store.checkpoint().unwrap();
    let watermark = store.current().watermark.get();
    store.close().await.unwrap();
    let path = root.join(format!("history-checkpoint-{watermark:020}.json"));
    let marker = root.join(format!("history-checkpoint-{watermark:020}.active"));
    let mut state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    state["current"] = serde_json::json!("f".repeat(64));
    let bytes = vcp_protocol::canonical_bytes(&state).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let mut seal: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&marker).unwrap()).unwrap();
    seal["sha256"] = serde_json::json!(vcp_protocol::digest_bytes(&bytes));
    std::fs::write(&marker, vcp_protocol::canonical_bytes(&seal).unwrap()).unwrap();
    let (result, diagnostics) = Store::open_with_diagnostics(&root, BackendKind::Files, &[]).await;
    assert!(matches!(
        result,
        Err(vcp_store::Error::Corruption(
            "current checkpoint differs from canonical history"
        ))
    ));
    assert_eq!(diagnostics.open.failed, 1);
    assert_eq!(diagnostics.replay.failed, 1);
    assert_eq!(diagnostics.checkpoint_verification.failed, 1);
    assert_eq!(diagnostics.checkpoint_replayed_commits, 0);
    assert_eq!(diagnostics.validation.completed, 1);
}

#[tokio::test]
async fn prefix_checkpoint_and_growing_history_need_one_validation_per_commit() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("canonical");
        let mut store = Store::open(&root, backend, &[]).await.unwrap();
        let initial = common::initial();
        store.transact(initial.clone()).await.unwrap();
        for index in 1..=64 {
            let mut event = initial.events[0].clone();
            event.id = vcp_domain::EventId::parse(format!("event-{index}")).unwrap();
            store
                .transact(vcp_store::contract::Transaction {
                    id: vcp_domain::TransactionId::parse(format!("transaction-{index}")).unwrap(),
                    expected_watermark: store.current().watermark,
                    mutations: vec![],
                    events: vec![event],
                    command: None,
                })
                .await
                .unwrap();
            if index == 32 {
                store.checkpoint().unwrap();
            }
        }
        let state = (&store.archive_state().await.unwrap()).clone();
        store.close().await.unwrap();
        let reopened = Store::open(&root, backend, &[]).await.unwrap();
        assert_eq!((&reopened.archive_state().await.unwrap()), &state);
        let diagnostics = reopened.diagnostics();
        assert_eq!(diagnostics.replayed_commits, 65);
        assert_eq!(diagnostics.validation.completed, 65);
        assert_eq!(diagnostics.state_size_full_scans, 1);
        assert_eq!(diagnostics.state_size_delta_updates, 65);
        assert_eq!(diagnostics.checkpoint_replayed_commits, 0);
        assert_eq!(
            diagnostics.checkpoint_state_comparisons,
            u64::from(backend == BackendKind::Files)
        );
        println!(
            "store_scaling_sample={}",
            serde_json::to_string(diagnostics).unwrap()
        );
        reopened.close().await.unwrap();
    }
}
