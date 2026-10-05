// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::artifact::ArtifactWriter;
#[path = "../tests/common/mod.rs"]
mod common;

#[tokio::test]
async fn real_snapshot_outlives_store_keeps_exact_cut_and_blocks_cleanup_until_closed() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        let workspace = temp.path().join("workspace");
        fs::create_dir(&workspace).unwrap();
        let mut store = Store::open(&root, kind, &[workspace.clone()])
            .await
            .unwrap();
        store.transact(common::initial()).await.unwrap();
        let mut writer = store.spool().create(common::spec()).unwrap();
        writer
            .write_chunk(b"retained binary evidence\0\xff")
            .unwrap();
        let artifact = writer.finalize().unwrap();
        drop(writer);
        store
            .transact(common::attach(store.state(), artifact.clone(), None))
            .await
            .unwrap();
        let expected = store.state().clone();
        let staged = store.stage_current().await.unwrap();
        let mut snapshot = staged.snapshot().await.unwrap();
        assert!(!snapshot.pages.get_mut().connected());
        drop(staged);
        store.close().await.unwrap();
        assert_eq!(snapshot._artifacts.len(), 1);
        assert!(!snapshot
            .spool
            .collect_unreferenced(&artifact.spec.id)
            .unwrap());
        assert_eq!(snapshot.current().watermark, expected.watermark);
        assert!(!snapshot.pages.get_mut().connected());
        assert_eq!(
            snapshot.history_event_count().await.unwrap(),
            expected.events.len() as u64
        );
        assert_eq!(
            snapshot.history_events(None, 4096).await.unwrap(),
            *expected.events
        );
        assert!(snapshot_pin::cleanup(&root).unwrap().is_none());
        let mut store = Store::open(&root, kind, &[workspace.clone()])
            .await
            .unwrap();
        let mut tx = common::initial();
        tx.id = TransactionId::parse("after-pinned-cut").unwrap();
        tx.expected_watermark = expected.watermark;
        tx.mutations.clear();
        tx.events[0].id = EventId::parse("after-pinned-event").unwrap();
        tx.events[0].correlation = CommandId::parse("after-pinned-command").unwrap();
        tx.command.as_mut().unwrap().command = tx.events[0].correlation.clone();
        tx.command.as_mut().unwrap().digest = "e".repeat(64);
        store.transact(tx).await.unwrap();
        assert_eq!(
            snapshot.history_event_count().await.unwrap(),
            expected.events.len() as u64
        );
        assert_eq!(
            snapshot.history_events(None, 4096).await.unwrap(),
            *expected.events
        );
        assert!(store.current_state().watermark > snapshot.current().watermark);
        assert!(store.try_snapshot_cleanup_guard().unwrap().is_none());
        let path = temp.path().join("archive-pages");
        fs::create_dir(&path).unwrap();
        let directory = Directory::open(&path, &[workspace]).unwrap();
        let mut destination = io::Files::new(&directory);
        let archive = snapshot
            .capture(
                &mut destination,
                &common::workspace().id,
                &Inputs::default(),
                &|| Ok(()),
            )
            .await
            .unwrap();
        assert!(!archive.root().unwrap().is_empty());
        snapshot
            .owner
            .semantic()
            .catalog()
            .verify_replayed_state(snapshot.pages.get_mut(), &expected)
            .await
            .unwrap();
        assert_eq!(
            snapshot
                .owner
                .archive_state(snapshot.pages.get_mut())
                .await
                .unwrap(),
            expected
        );
        assert!(matches!(
            snapshot
                .pages
                .get_mut()
                .write(&"0".repeat(64), b"no write authority")
                .await,
            Err(Error::Access)
        ));
        snapshot.close().await.unwrap();
        let artifact_pin = OpenOptions::new()
            .read(true)
            .write(true)
            .open(
                store
                    .spool()
                    .root()
                    .join(artifact.spec.id.as_str())
                    .join("snapshot.lock"),
            )
            .unwrap();
        artifact_pin.try_lock().unwrap();
        drop(artifact_pin);
        assert!(store.try_snapshot_cleanup_guard().unwrap().is_some());
        store.close().await.unwrap();
    }
}
