// SPDX-License-Identifier: Apache-2.0
use super::*;

#[tokio::test]
async fn pinned_history_continues_short_byte_pages_and_excludes_later_append() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temporary = tempfile::tempdir().unwrap();
        let mut fixture = fixture(temporary.path(), backend).await;
        let mut expected = Vec::new();
        for _ in 0..2 {
            let id = EventId::new();
            expected.push(id.clone());
            let transaction = Transaction {
                id: TransactionId::new(),
                expected_watermark: fixture.engine.store().current().watermark,
                mutations: vec![],
                events: vec![EventInput {
                    id,
                    workspace: access().workspace,
                    session: engine_access().session,
                    task: Some(fixture.root.clone()),
                    actor: engine_access().actor,
                    correlation: CommandId::new(),
                    causation: None,
                    timestamp: Timestamp::new(3000),
                    kind: EventKind::Commentary,
                    artifacts: vec![],
                    data: serde_json::json!({"payload":"x".repeat(5 * 1024 * 1024)}),
                    metadata: None,
                }],
                command: None,
            };
            fixture
                .engine
                .store_mut()
                .transact(transaction)
                .await
                .unwrap();
        }
        let mut history = History::default();
        let filter = Filter {
            kind: Some(EventKind::Commentary),
            ..Default::default()
        };
        let cursor = history
            .start(
                fixture.engine.store(),
                &access(),
                filter.clone(),
                128,
                Timestamp::new(3000),
            )
            .await
            .unwrap();
        let later = TaskId::new();
        issue(
            &mut fixture.engine,
            create(&later, None, None),
            Some(later),
            0,
        )
        .await;
        let page = history
            .page(
                fixture.engine.store(),
                &access(),
                &filter,
                &cursor,
                Timestamp::new(3000),
            )
            .await
            .unwrap();
        assert_eq!(
            page.events
                .iter()
                .map(|row| row.event.id.clone())
                .collect::<Vec<_>>(),
            expected
        );
        assert!(page.at_end);
        assert_eq!(page.next_cursor.after, cursor.end);
        assert_eq!(page.snapshot_watermark, cursor.watermark);
        assert!(fixture.engine.store().current().watermark > cursor.watermark);
    }
}
