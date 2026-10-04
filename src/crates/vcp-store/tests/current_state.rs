// SPDX-License-Identifier: Apache-2.0
mod common;
use std::sync::Arc;
use vcp_domain::{EventId, TransactionId, WorkspaceId};
use vcp_store::{
    contract::{CanonicalStore, Collection, Transaction},
    BackendKind, Error, Store,
};

#[test]
#[ignore = "opt-in synthetic snapshot clone measurement, not a timing gate"]
fn benchmark_shared_snapshot_clone() {
    use std::{hint::black_box, time::Instant};
    use vcp_store::contract::State;
    let initial = common::initial();
    let (state, _) = State::default().prepare(&initial).unwrap();
    let events = (0..32)
        .map(|_| {
            let mut event = initial.events[0].clone();
            event.id = EventId::new();
            event.data = serde_json::json!({"synthetic_history": "x".repeat(32 * 1024)});
            event
        })
        .collect();
    let (state, _) = state
        .prepare(&Transaction {
            id: TransactionId::new(),
            expected_watermark: state.watermark,
            mutations: vec![],
            events,
            command: None,
        })
        .unwrap();
    let deep_clone = || State {
        watermark: state.watermark,
        records: (*state.records).clone().into(),
        events: (*state.events).clone().into(),
        commands: (*state.commands).clone().into(),
        transactions: (*state.transactions).clone().into(),
        sequences: state.sequences.clone(),
    };
    assert_eq!(deep_clone(), state.clone());
    let started = Instant::now();
    for _ in 0..1000 {
        black_box(deep_clone());
    }
    let deep_micros = started.elapsed().as_micros();
    let started = Instant::now();
    for _ in 0..1000 {
        black_box(state.clone());
    }
    let shared_micros = started.elapsed().as_micros();
    println!("synthetic_clone iterations=1000 records={} events={} bytes={} deep_micros={} shared_micros={}", state.records.len(), state.events.len(), vcp_protocol::canonical_bytes(&state).unwrap().len(), deep_micros, shared_micros);
}

#[tokio::test]
async fn current_snapshots_share_reads_and_remain_stable_across_rejections_and_commits() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temporary = tempfile::tempdir().unwrap();
        let mut store = Store::open(temporary.path(), backend, &[]).await.unwrap();
        let initial = common::initial();
        store.transact(initial.clone()).await.unwrap();
        let old = store.current_state();
        let historical_snapshot = store.state().clone();
        assert!(std::ptr::eq(
            historical_snapshot.events.as_ptr(),
            store.state().events.as_ptr(),
        ));
        let historical_snapshot_bytes =
            vcp_protocol::canonical_bytes(&historical_snapshot).unwrap();
        assert!(std::ptr::eq(
            old.record(Collection::Task, "task", &common::workspace().id)
                .unwrap(),
            store
                .state()
                .record(Collection::Task, "task", &common::workspace().id)
                .unwrap(),
        ));
        assert!(Arc::ptr_eq(&old, &store.current_state()));
        assert_eq!(
            old.records_in(Collection::Session, &common::workspace().id)
                .count(),
            1
        );
        assert_eq!(
            old.records_in(Collection::Session, &WorkspaceId::new())
                .count(),
            0
        );
        assert_eq!(
            old.records_in(Collection::Artifact, &common::workspace().id)
                .count(),
            0
        );
        store.transact(initial.clone()).await.unwrap();
        assert!(Arc::ptr_eq(&old, &store.current_state()));
        let mut invalid = initial.clone();
        invalid.events[0].data = serde_json::json!({"different": true});
        assert!(store.transact(invalid).await.is_err());
        assert!(Arc::ptr_eq(&old, &store.current_state()));
        assert!(matches!(
            old.record(Collection::Task, "task", &WorkspaceId::new()),
            Err(Error::Access)
        ));

        let mut event = initial.events[0].clone();
        event.id = EventId::new();
        event.data = serde_json::json!({"history_only": "retained-payload".repeat(4096)});
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![],
                events: vec![event],
                command: None,
            })
            .await
            .unwrap();
        let current = store.current_state();
        assert_eq!(
            vcp_protocol::canonical_bytes(&historical_snapshot).unwrap(),
            historical_snapshot_bytes,
        );
        assert_eq!(
            historical_snapshot.events.len() + 1,
            store.state().events.len()
        );
        assert!(!Arc::ptr_eq(&old, &current));
        assert_eq!(old.watermark.get(), 1);
        assert_eq!(current.watermark.get(), 2);
        assert_eq!(old.records, current.records);
        let projected = vcp_protocol::canonical_bytes(&*current).unwrap();
        let historical = vcp_protocol::canonical_bytes(store.state()).unwrap();
        let unwrapped_shape = serde_json::json!({
            "watermark": store.state().watermark,
            "records": &*store.state().records,
            "events": &*store.state().events,
            "commands": &*store.state().commands,
            "transactions": &*store.state().transactions,
            "sequences": store.state().sequences,
        });
        assert_eq!(
            historical,
            vcp_protocol::canonical_bytes(&unwrapped_shape).unwrap()
        );
        let decoded: vcp_store::contract::State = serde_json::from_slice(&historical).unwrap();
        assert_eq!(&decoded, store.state());
        assert!(projected.len() * 10 < historical.len());
        let json = serde_json::to_value(&*current).unwrap();
        assert!(json.get("events").is_none());
        assert!(json.get("commands").is_none());
        assert!(json.get("transactions").is_none());
        assert_ne!(
            current.projection_digest().unwrap(),
            vcp_protocol::digest_bytes(&historical)
        );
        let mut task = common::task();
        task.revision = task.revision.next().unwrap();
        task.reason = "a committed update".into();
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![vcp_store::contract::Mutation::Put {
                    expected: Some(common::task().revision),
                    record: vcp_store::contract::Record::typed(
                        Collection::Task,
                        "task",
                        common::workspace().id,
                        task.revision,
                        &task,
                    )
                    .unwrap(),
                }],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        let updated = store.current_state();
        let previous_record = current
            .record(Collection::Task, "task", &common::workspace().id)
            .unwrap();
        let updated_record = updated
            .record(Collection::Task, "task", &common::workspace().id)
            .unwrap();
        assert_ne!(
            previous_record.value["reason"],
            updated_record.value["reason"]
        );
        assert!(!std::ptr::eq(previous_record, updated_record));
        assert_eq!(vcp_protocol::canonical_bytes(&*current).unwrap(), projected);
        store.close().await.unwrap();
        // A retained reader remains a fixed snapshot, not a capability to mutate.
        assert_eq!(current.watermark.get(), 2);
    }
}
