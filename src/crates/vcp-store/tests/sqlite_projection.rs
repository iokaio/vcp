// SPDX-License-Identifier: Apache-2.0
mod common;
use sqlx::{Connection, SqliteConnection};
use vcp_domain::{CommandId, EventId, TransactionId};
use vcp_store::{
    contract::{CanonicalStore, Transaction},
    BackendKind, Store,
};

#[tokio::test]
async fn paged_sqlite_history_checks_every_row_and_rejects_projection_corruption() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("canonical");
    let mut store = Store::open(&root, BackendKind::Sqlite, &[]).await.unwrap();
    let initial = common::initial();
    store.transact(initial.clone()).await.unwrap();
    // Cross the fixed page boundary for both independently ordered tables.
    for index in 0..258 {
        let command = CommandId::parse(format!("command-{index:04}")).unwrap();
        let mut event = initial.events[0].clone();
        event.id = EventId::parse(format!("event-{index:04}")).unwrap();
        event.correlation = command.clone();
        let mut receipt = initial.command.clone().unwrap();
        receipt.command = command;
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![],
                events: vec![event],
                command: Some(receipt),
            })
            .await
            .unwrap();
    }
    let expected = store.state().clone();
    store.close().await.unwrap();
    let reopened = Store::open(&root, BackendKind::Sqlite, &[]).await.unwrap();
    assert_eq!(reopened.state(), &expected);
    reopened.close().await.unwrap();

    let database = root.join("canonical.sqlite");
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(&database);
    let mut db = SqliteConnection::connect_with(&options).await.unwrap();
    sqlx::query("CREATE TABLE expected_events AS SELECT * FROM events")
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE expected_commands AS SELECT * FROM commands")
        .execute(&mut db)
        .await
        .unwrap();
    db.close().await.unwrap();
    for mutation in [
        "UPDATE events SET payload=x'7b7d' WHERE id='event-0257'",
        "UPDATE commands SET payload=x'7b7d' WHERE id='command-0257'",
        "UPDATE events SET id='unexpected-event' WHERE id='event-0257'",
        "UPDATE commands SET id='unexpected-command' WHERE id='command-0257'",
        "DELETE FROM events WHERE id='event-0257'",
        "DELETE FROM commands WHERE id='command-0257'",
    ] {
        let mut db = SqliteConnection::connect_with(&options).await.unwrap();
        sqlx::query(mutation).execute(&mut db).await.unwrap();
        db.close().await.unwrap();
        let (opened, diagnostics) =
            Store::open_with_diagnostics(&root, BackendKind::Sqlite, &[]).await;
        assert!(opened.is_err(), "accepted mutation: {mutation}");
        assert_eq!(
            diagnostics.materialized_verification.failed, 1,
            "{mutation}"
        );
        // Restore only the test's rows through SQLite, including its WAL.
        let mut db = SqliteConnection::connect_with(&options).await.unwrap();
        for restore in [
            "DELETE FROM events",
            "INSERT INTO events SELECT * FROM expected_events",
            "DELETE FROM commands",
            "INSERT INTO commands SELECT * FROM expected_commands",
        ] {
            sqlx::query(restore).execute(&mut db).await.unwrap();
        }
        db.close().await.unwrap();
    }
}
