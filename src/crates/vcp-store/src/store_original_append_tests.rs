// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::contract::{Commit, FORMAT_VERSION};
#[path = "../tests/common/mod.rs"]
mod common;

async fn originals(store: &Store) -> Vec<Vec<u8>> {
    let mut reader = store.core.history().await.unwrap();
    let mut rows = Vec::new();
    while let Some(row) = reader.next_original().await.unwrap() {
        rows.push(row.bytes);
    }
    reader.close().await.unwrap();
    rows
}

#[tokio::test]
async fn original_append_replays_semantics_and_conversion_keeps_exact_native_payloads() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for retained_base in [false, true] {
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path().join("source");
            let (initial_state, initial_commit) = State::default()
                .prepare_reference(&common::initial())
                .unwrap();
            let mut expected = State::default();
            let mut bytes = Vec::new();
            let base_bytes = if retained_base {
                fs::create_dir(&root).unwrap();
                // A retained pre-rewrite source commitment need not equal the
                // rewritten state digest. Conversion must not manufacture it.
                let prior = digest_bytes(b"retained pre-rewrite source");
                let base = crate::replay_base::ReplayBase {
                    version: 1,
                    source_digest: prior.clone(),
                    state: initial_state.clone(),
                    prefixes: vec![crate::replay_base::PrefixCommitment {
                        watermark: initial_state.watermark,
                        digest: prior,
                    }],
                };
                let encoded = canonical_bytes(&base).unwrap();
                immutable_file(&root.join("replay-base.json"), &encoded).unwrap();
                immutable_file(
                    &root.join("replay-base.seal"),
                    digest_bytes(&encoded).as_bytes(),
                )
                .unwrap();
                expected = initial_state.clone();
                Some(encoded)
            } else {
                None
            };
            let mut store = Store::open(&root, backend, &[]).await.unwrap();
            if !retained_base {
                let raw = serde_json::to_vec_pretty(&initial_commit).unwrap();
                assert_ne!(raw, canonical_bytes(&initial_commit).unwrap());
                assert_eq!(
                    store.transact_original(&raw).await.unwrap(),
                    initial_commit.receipt
                );
                expected = initial_state.clone();
                bytes.push(raw);
            }
            for index in 0..3 {
                let mut event = common::initial().events[0].clone();
                event.id = EventId::new();
                event.data["native_iteration"] = index.into();
                let transaction = Transaction {
                    id: TransactionId::new(),
                    expected_watermark: expected.watermark,
                    mutations: vec![],
                    events: vec![event],
                    command: None,
                };
                let (next, commit) = expected.prepare_reference(&transaction).unwrap();
                let raw = serde_json::to_vec_pretty(&commit).unwrap();
                assert_eq!(store.transact_original(&raw).await.unwrap(), commit.receipt);
                assert_eq!(store.transact_original(&raw).await.unwrap(), commit.receipt);
                assert!(store
                    .transact_original(&canonical_bytes(&commit).unwrap())
                    .await
                    .is_err());
                assert!(store.healthy());
                bytes.push(raw);
                expected = next;
            }
            assert_eq!(originals(&store).await, bytes);
            assert_eq!(store.archive_state().await.unwrap(), expected);
            let destination = temporary.path().join("converted");
            let other = if backend == BackendKind::Files {
                BackendKind::Sqlite
            } else {
                BackendKind::Files
            };
            let converted = store.convert(&destination, other, &[]).await.unwrap();
            assert_eq!(originals(&converted).await, bytes);
            assert_eq!(converted.archive_state().await.unwrap(), expected);
            if let Some(base_bytes) = base_bytes {
                assert_eq!(
                    fs::read(destination.join("replay-base.json")).unwrap(),
                    base_bytes
                );
                assert_eq!(
                    fs::read(destination.join("replay-base.seal")).unwrap(),
                    digest_bytes(&base_bytes).as_bytes()
                );
            }
            converted.close().await.unwrap();
            store.close().await.unwrap();
            let reopened = Store::open(&destination, other, &[]).await.unwrap();
            assert_eq!(originals(&reopened).await, bytes);
            assert_eq!(reopened.archive_state().await.unwrap(), expected);
            reopened.close().await.unwrap();
        }
    }
}

#[tokio::test]
async fn original_append_rejects_forged_proofs_and_invalid_transitions_without_publication() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temporary = tempfile::tempdir().unwrap();
        let mut store = Store::open(temporary.path(), backend, &[]).await.unwrap();
        let (_, good) = State::default()
            .prepare_reference(&common::initial())
            .unwrap();
        let identity = store.core.owner.identity().to_owned();
        for variant in 0..4 {
            let mut bad: Commit = good.clone();
            match variant {
                0 => bad.version = FORMAT_VERSION + 1,
                1 => bad.receipt.watermark = Watermark::new(2),
                2 => bad.receipt.digest = "0".repeat(64),
                _ => bad.transaction.events[0].workspace = WorkspaceId::new(),
            }
            assert!(store
                .transact_original(&serde_json::to_vec_pretty(&bad).unwrap())
                .await
                .is_err());
            assert!(store.healthy());
            assert_eq!(store.current().watermark, Watermark::ZERO);
            assert_eq!(store.core.owner.identity(), identity);
            assert!(originals(&store).await.is_empty());
        }
        assert!(store
            .transact_original(&vec![b' '; MAX_COMMIT_BYTES + 1])
            .await
            .is_err());
        let raw = serde_json::to_vec_pretty(&good).unwrap();
        assert_eq!(store.transact_original(&raw).await.unwrap(), good.receipt);
        let mut bad_duplicate = good.clone();
        bad_duplicate.receipt.digest = "0".repeat(64);
        assert!(store
            .transact_original(&canonical_bytes(&bad_duplicate).unwrap())
            .await
            .is_err());
        assert_eq!(originals(&store).await, vec![raw]);
        store.close().await.unwrap();
    }
}
