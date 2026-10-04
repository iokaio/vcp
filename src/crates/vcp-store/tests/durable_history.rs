// SPDX-License-Identifier: Apache-2.0
mod common;
use sqlx::{Connection, SqliteConnection};
use std::io::{Read, Seek, SeekFrom, Write};
use vcp_domain::{EventId, TransactionId, Watermark};
use vcp_protocol::{canonical_bytes, digest_bytes};
use vcp_store::{
    contract::{CanonicalStore, Commit, Transaction},
    BackendKind, Store,
};

fn append_event(store: &Store, initial: &Transaction) -> Transaction {
    let mut event = initial.events[0].clone();
    event.id = EventId::new();
    Transaction {
        id: TransactionId::new(),
        expected_watermark: store.state().watermark,
        mutations: vec![],
        events: vec![event],
        command: None,
    }
}
async fn original_history(root: &std::path::Path, backend: BackendKind) -> Vec<Vec<u8>> {
    match backend {
        BackendKind::Files => vec![std::fs::read(root.join("canonical.frames")).unwrap()],
        BackendKind::Sqlite => {
            let options = sqlx::sqlite::SqliteConnectOptions::new()
                .filename(root.join("canonical.sqlite"))
                .read_only(true);
            let mut db = SqliteConnection::connect_with(&options).await.unwrap();
            let rows = sqlx::query_scalar("SELECT payload FROM commits ORDER BY watermark")
                .fetch_all(&mut db)
                .await
                .unwrap();
            db.close().await.unwrap();
            rows
        }
    }
}

#[tokio::test]
async fn every_durable_cut_matches_memory_reference_and_reading_never_moves_append_cursor() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("canonical");
        let mut store = Store::open(&root, backend, &[]).await.unwrap();
        let initial = common::initial();
        let mut expected = vec![digest_bytes(&canonical_bytes(store.state()).unwrap())];
        store.transact(initial.clone()).await.unwrap();
        expected.push(digest_bytes(&canonical_bytes(store.state()).unwrap()));
        for _ in 0..8 {
            store
                .transact(append_event(&store, &initial))
                .await
                .unwrap();
            expected.push(digest_bytes(&canonical_bytes(store.state()).unwrap()));
        }
        let original = original_history(&root, backend).await;
        for (index, digest) in expected.iter().enumerate() {
            assert_eq!(
                &store
                    .prefix_digest(Watermark::new(index as u64))
                    .await
                    .unwrap(),
                digest
            );
        }
        assert_eq!(original_history(&root, backend).await, original);
        // A partial read must not move the writer back into old journal bytes.
        assert_eq!(
            store.prefix_digest(Watermark::new(1)).await.unwrap(),
            expected[1]
        );
        store
            .transact(append_event(&store, &initial))
            .await
            .unwrap();
        expected.push(digest_bytes(&canonical_bytes(store.state()).unwrap()));
        let final_state = store.state().clone();
        store.close().await.unwrap();
        let store = Store::open(&root, backend, &[]).await.unwrap();
        assert_eq!(store.state(), &final_state);
        for (index, digest) in expected.iter().enumerate() {
            assert_eq!(
                &store
                    .prefix_digest(Watermark::new(index as u64))
                    .await
                    .unwrap(),
                digest
            );
        }
        let original = original_history(&root, backend).await;
        let other = match backend {
            BackendKind::Files => BackendKind::Sqlite,
            BackendKind::Sqlite => BackendKind::Files,
        };
        let converted = store
            .convert(&temporary.path().join("converted"), other, &[])
            .await
            .unwrap();
        assert_eq!(
            canonical_bytes(converted.state()).unwrap(),
            canonical_bytes(&final_state).unwrap()
        );
        assert_eq!(original_history(&root, backend).await, original);
        converted.close().await.unwrap();
        store.close().await.unwrap();
    }
}

#[tokio::test]
async fn durable_reads_reject_resealed_transaction_bytes_that_differ_from_live_owner() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("canonical");
        let mut store = Store::open(&root, backend, &[]).await.unwrap();
        let initial = common::initial();
        store.transact(initial.clone()).await.unwrap();
        store
            .transact(append_event(&store, &initial))
            .await
            .unwrap();
        match backend {
            BackendKind::Sqlite => {
                let options = sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(root.join("canonical.sqlite"));
                let mut db = SqliteConnection::connect_with(&options).await.unwrap();
                let bytes: Vec<u8> =
                    sqlx::query_scalar("SELECT payload FROM commits WHERE watermark=1")
                        .fetch_one(&mut db)
                        .await
                        .unwrap();
                let bytes = forged(&bytes);
                sqlx::query("UPDATE commits SET payload=?,digest=? WHERE watermark=1")
                    .bind(&bytes)
                    .bind(digest_bytes(&bytes))
                    .execute(&mut db)
                    .await
                    .unwrap();
                db.close().await.unwrap();
            }
            BackendKind::Files => {
                let mut file = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(root.join("canonical.frames"))
                    .unwrap();
                let mut header = [0u8; 80];
                file.read_exact(&mut header).unwrap();
                let size = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
                let mut bytes = vec![0; size];
                file.read_exact(&mut bytes).unwrap();
                let forged = forged(&bytes);
                assert_eq!(forged.len(), bytes.len());
                let hash = digest_bytes(&[header.as_slice(), forged.as_slice()].concat());
                file.seek(SeekFrom::Start(80)).unwrap();
                file.write_all(&forged).unwrap();
                file.write_all(hash.as_bytes()).unwrap();
                file.sync_all().unwrap();
            }
        }
        assert!(matches!(
            store.prefix_digest(Watermark::new(1)).await,
            Err(vcp_store::Error::Corruption(
                "retained commit differs from validated owner"
            ))
        ));
        assert!(store
            .convert(&temporary.path().join("refused"), backend, &[])
            .await
            .is_err());
        store.close().await.unwrap();
    }
}
fn forged(bytes: &[u8]) -> Vec<u8> {
    let mut commit: Commit = serde_json::from_slice(bytes).unwrap();
    commit.transaction.events[0].data["state"] = serde_json::json!("PENDING");
    commit.receipt.digest = digest_bytes(&canonical_bytes(&commit.transaction).unwrap());
    canonical_bytes(&commit).unwrap()
}
