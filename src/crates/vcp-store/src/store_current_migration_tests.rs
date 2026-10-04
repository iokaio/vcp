// SPDX-License-Identifier: Apache-2.0
use super::*;
#[path = "../tests/common/mod.rs"]
mod common;

#[tokio::test]
async fn migration_staging_borrows_actual_store_owner_and_keeps_legacy_format_reopenable() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        let mut store = Store::open(&root, kind, &[]).await.unwrap();
        store.transact(common::initial()).await.unwrap();
        let mut expected = store.state().clone();
        let format = fs::read(root.join("format.json")).unwrap();
        let before = store.diagnostics().replayed_commits;
        {
            let staged = store.stage_current().await.unwrap();
            assert_eq!(
                staged.owner().semantic().current(),
                &crate::CurrentState::from_state(&expected)
            );
            assert_eq!(staged.origin_digest().len(), 64);
            assert!(matches!(
                Store::open(&root, kind, &[]).await,
                Err(Error::Conflict("canonical root already has an owner"))
            ));
            assert!(snapshot_pin::cleanup(staged.source.root())
                .unwrap()
                .is_none());
            assert_eq!(fs::read(root.join("format.json")).unwrap(), format);
        }
        assert_eq!(store.state(), &expected);
        assert_eq!(
            store.diagnostics().replayed_commits,
            before,
            "qualification replay must not pretend to be a fresh owner open"
        );
        assert!(store.try_snapshot_cleanup_guard().unwrap().is_some());
        // Successful replay must restore the actual writer to its final native
        // extent, not merely leave a State that can be read before close.
        let mut transaction = common::initial();
        transaction.id = TransactionId::parse("after-staging").unwrap();
        transaction.expected_watermark = expected.watermark;
        transaction.mutations.clear();
        transaction.events[0].id = EventId::parse("after-staging-event").unwrap();
        transaction.events[0].correlation = CommandId::parse("after-staging-command").unwrap();
        transaction.command.as_mut().unwrap().command = transaction.events[0].correlation.clone();
        transaction.command.as_mut().unwrap().digest = "e".repeat(64);
        let (next, commit) = expected.prepare_reference(&transaction).unwrap();
        assert_eq!(store.transact(transaction).await.unwrap(), commit.receipt);
        assert_eq!(
            &next.events[..expected.events.len()],
            expected.events.as_slice()
        );
        for (id, receipt) in expected.transactions.iter() {
            assert_eq!(next.transactions.get(id), Some(receipt));
        }
        expected = next;
        assert_eq!(fs::read(root.join("format.json")).unwrap(), format);
        store.close().await.unwrap();
        let store = Store::open(&root, kind, &[]).await.unwrap();
        assert_eq!(store.state(), &expected);
        store.close().await.unwrap();
    }
}

#[tokio::test]
async fn staging_rejects_protected_child_and_failed_read_without_changing_canonical_format() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("canonical");
    let protected = root.join("history-pages");
    fs::create_dir_all(&protected).unwrap();
    let mut store = Store::open(&root, BackendKind::Files, &[protected.clone()])
        .await
        .unwrap();
    store.transact(common::initial()).await.unwrap();
    let format = fs::read(root.join("format.json")).unwrap();
    assert!(matches!(store.stage_current().await, Err(Error::Access)));
    assert_eq!(fs::read_dir(protected).unwrap().count(), 0);
    assert_eq!(fs::read(root.join("format.json")).unwrap(), format);
    store.close().await.unwrap();
    let mut store = Store::open(&root, BackendKind::Files, &[]).await.unwrap();
    let mut staged = store.stage_current().await.unwrap();
    let path = staged
        .directory
        .as_ref()
        .unwrap()
        .path
        .join(format!("{}.json", staged.origin_digest()));
    let bytes = fs::read(&path).unwrap();
    fs::write(&path, b"corrupt descriptor").unwrap();
    assert!(staged.verify().await.is_err());
    assert_eq!(fs::read(root.join("format.json")).unwrap(), format);
    fs::write(path, bytes).unwrap();
    assert!(matches!(staged.verify().await, Err(Error::Unavailable(_))));
    drop(staged);
    assert!(!store.healthy());
    assert!(matches!(
        store.transact(common::initial()).await,
        Err(Error::Unavailable(_))
    ));
    store.close().await.unwrap();
    let store = Store::open(&root, BackendKind::Files, &[]).await.unwrap();
    assert!(store.healthy());
    store.close().await.unwrap();
}
