// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    backend::current_publication::open::Opened, canonical_lock::CanonicalLock,
    durable_owner::Outcome,
};
#[path = "../tests/common/mod.rs"]
mod common;

#[tokio::test]
async fn native_factory_replays_without_resident_store_then_appends_and_reopens_exact_history() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        let mut store = Store::open(&root, kind, &[]).await.unwrap();
        store.transact(common::initial()).await.unwrap();
        let expected = store.state().clone();
        let staged = store.stage_current().await.unwrap();
        let digest = staged.origin_digest.clone();
        let identity = staged.owner.identity().to_owned();
        drop(staged);
        store.close().await.unwrap();
        let lock = CanonicalLock::acquire(&root, &[]).unwrap();
        let mut opened = Opened::open(lock, kind, &digest, &mut crate::StoreDiagnostics::new(kind))
            .await
            .unwrap();
        assert_eq!(opened.owner.identity(), identity);
        assert!(Store::open(&root, kind, &[]).await.is_err());
        let mut tx = common::initial();
        tx.id = TransactionId::new();
        tx.expected_watermark = expected.watermark;
        tx.mutations.clear();
        tx.events[0].id = EventId::new();
        tx.events[0].correlation = CommandId::new();
        tx.command.as_mut().unwrap().command = tx.events[0].correlation.clone();
        tx.command.as_mut().unwrap().digest = "f".repeat(64);
        let expected = expected.prepare_reference(&tx).unwrap().0;
        let original;
        let next = match &mut opened.backend {
            Backend::Files(journal) => {
                let mut pages = io::Files::new(opened.pages.as_ref().unwrap());
                let Outcome::Prepared(prepared) =
                    opened.owner.prepare(&mut pages, &tx).await.unwrap()
                else {
                    panic!("new commit")
                };
                original = serde_json::to_vec_pretty(prepared.commit()).unwrap();
                let (next, publication) = crate::history_publication::stage(
                    &mut pages,
                    &opened.owner,
                    &prepared,
                    &original,
                )
                .await
                .unwrap();
                journal
                    .append_current(&original, expected.watermark, &publication, &|_| {})
                    .unwrap();
                next
            }
            Backend::Sqlite(db) => {
                let Outcome::Prepared(prepared) = opened
                    .owner
                    .prepare(&mut io::Sqlite::new(db), &tx)
                    .await
                    .unwrap()
                else {
                    panic!("new commit")
                };
                original = serde_json::to_vec_pretty(prepared.commit()).unwrap();
                current_publication::append_sqlite(db, &opened.owner, &prepared, &original, &|_| {})
                    .await
                    .unwrap()
            }
        };
        let identity = next.identity().to_owned();
        opened.close().await.unwrap();
        let lock = CanonicalLock::acquire(&root, &[]).unwrap();
        let mut reopened =
            Opened::open(lock, kind, &digest, &mut crate::StoreDiagnostics::new(kind))
                .await
                .unwrap();
        assert_eq!(reopened.owner.identity(), identity);
        match &mut reopened.backend {
            Backend::Files(_) => {
                let mut pages = io::Files::new(reopened.pages.as_ref().unwrap());
                assert_eq!(
                    reopened.owner.archive_state(&mut pages).await.unwrap(),
                    expected
                );
                assert_eq!(
                    reopened
                        .owner
                        .originals()
                        .get(&mut pages, expected.watermark)
                        .await
                        .unwrap()
                        .unwrap()
                        .bytes,
                    original
                );
            }
            Backend::Sqlite(db) => {
                let mut pages = io::Sqlite::new(db);
                assert_eq!(
                    reopened.owner.archive_state(&mut pages).await.unwrap(),
                    expected
                );
                assert_eq!(
                    reopened
                        .owner
                        .originals()
                        .get(&mut pages, expected.watermark)
                        .await
                        .unwrap()
                        .unwrap()
                        .bytes,
                    original
                );
            }
        }
        reopened.close().await.unwrap();
    }
}

#[tokio::test]
async fn native_factory_never_creates_missing_data_or_trusts_a_missing_origin() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        let mut store = Store::open(&root, kind, &[]).await.unwrap();
        store.transact(common::initial()).await.unwrap();
        let staged = store.stage_current().await.unwrap();
        let digest = staged.origin_digest.clone();
        drop(staged);
        store.close().await.unwrap();
        let lock = CanonicalLock::acquire(&root, &[]).unwrap();
        assert!(Opened::open(
            lock,
            kind,
            &"f".repeat(64),
            &mut crate::StoreDiagnostics::new(kind)
        )
        .await
        .is_err());
        let data = root.join(if kind == BackendKind::Files {
            "canonical.frames"
        } else {
            "canonical.sqlite"
        });
        let saved = root.join("owned-fixture-saved-data");
        fs::rename(&data, &saved).unwrap();
        let lock = CanonicalLock::acquire(&root, &[]).unwrap();
        assert!(
            Opened::open(lock, kind, &digest, &mut crate::StoreDiagnostics::new(kind))
                .await
                .is_err()
        );
        assert!(!data.exists());
        fs::rename(&saved, &data).unwrap();
        let lock = CanonicalLock::acquire(&root, &[]).unwrap();
        let reopened = Opened::open(lock, kind, &digest, &mut crate::StoreDiagnostics::new(kind))
            .await
            .unwrap();
        reopened.close().await.unwrap();
    }
}
