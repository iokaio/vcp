// SPDX-License-Identifier: Apache-2.0
mod common;
use vcp_domain::{CommandId, EventId, SessionId, TransactionId, Watermark, WorkspaceId};
use vcp_store::{
    contract::{CanonicalStore, Transaction},
    BackendKind, Store,
};

#[tokio::test]
async fn exact_transaction_receipt_survives_reopen_without_repeating_mutation() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temporary = tempfile::tempdir().unwrap();
        let mut store = Store::open(temporary.path(), backend, &[]).await.unwrap();
        let transaction = common::initial();
        let receipt = store.transact(transaction.clone()).await.unwrap();
        assert_eq!(
            store.transaction_receipt(&transaction.id).await.unwrap(),
            Some(receipt.clone())
        );
        assert_eq!(
            store
                .transaction_receipt(&TransactionId::new())
                .await
                .unwrap(),
            None
        );
        store.close().await.unwrap();
        let mut reopened = Store::open(temporary.path(), backend, &[]).await.unwrap();
        assert_eq!(
            reopened.transaction_receipt(&transaction.id).await.unwrap(),
            Some(receipt.clone())
        );
        let watermark = reopened.current().watermark;
        assert_eq!(reopened.transact(transaction).await.unwrap(), receipt);
        assert_eq!(reopened.current().watermark, watermark);
        reopened.close().await.unwrap();
    }
}

#[tokio::test]
async fn bounded_history_preserves_transactions_scope_and_receipt_cursor_fences() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temporary = tempfile::tempdir().unwrap();
        let mut store = Store::open(temporary.path(), backend, &[]).await.unwrap();
        let initial = common::initial();
        store.transact(initial.clone()).await.unwrap();
        let scope = common::task().scope;
        for index in 0..3 {
            let command = CommandId::parse(format!("history-{index}")).unwrap();
            let mut receipt = initial.command.clone().unwrap();
            receipt.command = command.clone();
            let events = (0..3)
                .map(|_| {
                    let mut event = initial.events[0].clone();
                    event.id = EventId::new();
                    event.correlation = command.clone();
                    event
                })
                .collect();
            store
                .transact(Transaction {
                    id: TransactionId::new(),
                    expected_watermark: store.current().watermark,
                    mutations: vec![],
                    events,
                    command: Some(receipt),
                })
                .await
                .unwrap();
        }
        let mut after = Watermark::ZERO;
        let mut seen = Vec::new();
        loop {
            let page = store.event_history_page(&scope, after, 1).await.unwrap();
            assert!(page.scanned <= 3);
            assert_eq!(page.events.len(), page.scanned);
            assert_eq!(page.source_watermark, store.current().watermark);
            seen.extend(page.events.iter().map(|event| event.event.id.clone()));
            after = page.cutoff.unwrap_or(after);
            if !page.has_more {
                break;
            }
        }
        assert_eq!(
            seen,
            (&store.archive_state().await.unwrap())
                .events
                .iter()
                .map(|event| event.event.id.clone())
                .collect::<Vec<_>>()
        );
        let empty = store.event_history_page(&scope, after, 1).await.unwrap();
        assert!(empty.events.is_empty());
        assert_eq!(empty.cutoff, None);
        let mut other = scope.clone();
        other.workspace = WorkspaceId::new();
        let foreign = store
            .event_history_page(&other, Watermark::ZERO, 1)
            .await
            .unwrap();
        assert!(foreign.events.is_empty());
        assert_eq!(foreign.scanned, 1);
        assert_eq!(foreign.cutoff, Some(Watermark::new(1)));
        other = scope.clone();
        other.session = SessionId::new();
        assert!(store
            .event_history_page(&other, Watermark::ZERO, 1)
            .await
            .unwrap()
            .events
            .is_empty());
        assert!(store
            .event_history_page(&scope, Watermark::ZERO, 0)
            .await
            .is_err());
        assert!(store
            .event_history_page(&scope, store.current().watermark.next().unwrap(), 1)
            .await
            .is_err());

        let watermark = store.current().watermark;
        let mut unordered = (&store.archive_state().await.unwrap()).clone();
        unordered.events[2].watermark = Watermark::ZERO;
        assert!(matches!(
            unordered.validate(),
            Err(vcp_store::Error::Corruption("event identity"))
        ));
        for receipt in (&store.archive_state().await.unwrap()).commands.values() {
            let selected = store
                .receipt_events(&scope.workspace, &scope.session, receipt)
                .await
                .unwrap()
                .collect::<Vec<_>>();
            let archived = store.archive_state().await.unwrap();
            let expected = archived
                .events
                .iter()
                .filter(|event| {
                    event.watermark == receipt.watermark
                        && event.event.workspace == scope.workspace
                        && event.event.session == scope.session
                })
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(selected, expected);
            assert!(store
                .receipt_events(&scope.workspace, &SessionId::new(), receipt)
                .await
                .unwrap()
                .next()
                .is_none());
            let mut forged = receipt.clone();
            forged.watermark = Watermark::ZERO;
            assert!(store
                .receipt_events(&scope.workspace, &scope.session, &forged)
                .await
                .is_err());
        }
        let mut after = None;
        let mut seen = Vec::new();
        loop {
            let page = store
                .command_history_page(&scope.workspace, watermark, after.as_ref(), 2)
                .await
                .unwrap();
            assert!(page.receipts.len() <= 2);
            seen.extend(page.receipts.iter().map(|receipt| receipt.command.clone()));
            after = page.next;
            if after.is_none() {
                break;
            }
        }
        assert_eq!(
            seen,
            (&store.archive_state().await.unwrap())
                .commands
                .values()
                .map(|receipt| receipt.command.clone())
                .collect::<Vec<_>>()
        );
        assert!(store
            .command_history_page(&WorkspaceId::new(), watermark, None, 2)
            .await
            .unwrap()
            .receipts
            .is_empty());
        assert!(store
            .command_history_page(&scope.workspace, watermark, None, 4097)
            .await
            .is_err());
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: watermark,
                mutations: vec![],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        assert!(store
            .command_history_page(&scope.workspace, watermark, None, 2)
            .await
            .is_err());
        let expected = (&store.archive_state().await.unwrap()).clone();
        store.close().await.unwrap();
        let reopened = Store::open(temporary.path(), backend, &[]).await.unwrap();
        let page = reopened
            .event_history_page(&scope, Watermark::ZERO, 4096)
            .await
            .unwrap();
        assert_eq!(page.events.len(), expected.events.len());
        reopened.close().await.unwrap();
    }
}

#[tokio::test]
async fn event_page_refuses_oversized_transaction_instead_of_returning_partial_evidence() {
    let temporary = tempfile::tempdir().unwrap();
    let mut store = Store::open(temporary.path(), BackendKind::Files, &[])
        .await
        .unwrap();
    let initial = common::initial();
    store.transact(initial.clone()).await.unwrap();
    let before = store.current().watermark;
    let events = (0..4097)
        .map(|_| {
            let mut event = initial.events[0].clone();
            event.id = EventId::new();
            event
        })
        .collect();
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: before,
            mutations: vec![],
            events,
            command: None,
        })
        .await
        .unwrap();
    assert!(matches!(
        store
            .event_history_page(&common::task().scope, before, 128)
            .await,
        Err(vcp_store::Error::Limit(
            "history event transaction exceeds bounded page"
        ))
    ));
    assert_eq!((&store.archive_state().await.unwrap()).events.len(), 4098);
    store.close().await.unwrap();
}
