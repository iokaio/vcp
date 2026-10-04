// SPDX-License-Identifier: Apache-2.0
mod common;
use vcp_store::{contract::CanonicalStore, BackendKind, Store};

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
        let state = store.state().clone();
        assert_eq!(store.transact(initial.clone()).await.unwrap(), receipt);
        let mut conflicting = initial;
        conflicting.events[0].data = serde_json::json!({"different":true});
        assert!(store.transact(conflicting).await.is_err());
        assert_eq!(store.state(), &state);
        let observed = store.diagnostics();
        assert_eq!(observed.preparation.completed, 3);
        assert_eq!(observed.preparation.failed, 1);
        assert_eq!(observed.validation.completed, 1);
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
        assert_eq!(reopened.state(), &state);
        let observed = reopened.diagnostics();
        assert_eq!(observed.replayed_commits, 1);
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
    let watermark = store.state().watermark.get();
    store.close().await.unwrap();
    let path = root.join(format!("checkpoint-{watermark:020}.json"));
    let marker = root.join(format!("checkpoint-{watermark:020}.active"));
    let mut state: vcp_store::contract::State =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    state.records.get_mut("task:task").unwrap().value["reason"] =
        serde_json::json!("a forged valid-looking final state");
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
            "checkpoint differs from canonical history"
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
                    expected_watermark: store.state().watermark,
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
        let state = store.state().clone();
        store.close().await.unwrap();
        let reopened = Store::open(&root, backend, &[]).await.unwrap();
        assert_eq!(reopened.state(), &state);
        let diagnostics = reopened.diagnostics();
        assert_eq!(diagnostics.replayed_commits, 65);
        assert_eq!(diagnostics.validation.completed, 65);
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
