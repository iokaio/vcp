// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{contract::CanonicalStore, Barrier, Store};
#[path = "../tests/common/mod.rs"]
mod common;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyFormat {
    version: u32,
    backend: BackendKind,
}

#[tokio::test]
async fn format_publication_preserves_legacy_bytes_and_resolves_both_interruption_boundaries() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        for stop_after in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("canonical");
            let mut source = Store::open(&root, kind, &[]).await.unwrap();
            source.transact(common::initial()).await.unwrap();
            let reference = source.state().clone();
            let original_marker = std::fs::read(root.join("format.json")).unwrap();
            let staged = source.stage_current().await.unwrap();
            let origin = staged.origin_digest().to_owned();
            drop(staged);
            source.close().await.unwrap();
            let lock = CanonicalLock::acquire(&root, &[]).unwrap();
            let Selection::Legacy(legacy) = read(&lock, kind).unwrap() else {
                panic!("legacy")
            };
            let owner = Opened::open(lock, kind, &origin, &mut crate::StoreDiagnostics::new(kind))
                .await
                .unwrap();
            let stopped = publish_observed(&owner, &legacy, &|barrier| {
                if matches!(
                    (stop_after, barrier),
                    (false, Barrier::BeforeActivation) | (true, Barrier::AfterActivation)
                ) {
                    return Err(Error::Unavailable("injected format interruption"));
                }
                Ok(())
            });
            assert!(stopped.is_err());
            let backup = root.join(legacy_name(&digest_bytes(&original_marker)));
            assert_eq!(std::fs::read(&backup).unwrap(), original_marker);
            match read(owner.canonical_lock(), kind).unwrap() {
                Selection::Legacy(_) => assert!(!stop_after),
                Selection::Current { origin: actual } => {
                    assert!(stop_after);
                    assert_eq!(actual, origin);
                }
            }
            publish(&owner, &legacy).unwrap();
            publish(&owner, &legacy).unwrap();
            assert_eq!(owner.archive_state().await.unwrap(), reference);
            owner.close().await.unwrap();
            // The former Store open behavior rejects the new identifier; it
            // cannot silently append a layout-1 frame to migrated history.
            let old = serde_json::from_slice::<LegacyFormat>(
                &std::fs::read(root.join("format.json")).unwrap(),
            );
            assert!(
                !old.is_ok_and(|marker| matches!(marker.version, 1 | 2) && marker.backend == kind)
            );
            let lock = CanonicalLock::acquire(&root, &[]).unwrap();
            let Selection::Current { origin: actual } = read(&lock, kind).unwrap() else {
                panic!("current")
            };
            let reopened =
                Opened::open(lock, kind, &actual, &mut crate::StoreDiagnostics::new(kind))
                    .await
                    .unwrap();
            assert_eq!(reopened.archive_state().await.unwrap(), reference);
            reopened.close().await.unwrap();
            std::fs::write(&backup, b"{}").unwrap();
            let lock = CanonicalLock::acquire(&root, &[]).unwrap();
            assert!(read(&lock, kind).is_err());
        }
    }
}

#[tokio::test]
async fn format_publication_rejects_changed_selection_without_replacing_it() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        let mut source = Store::open(&root, kind, &[]).await.unwrap();
        source.transact(common::initial()).await.unwrap();
        let staged = source.stage_current().await.unwrap();
        let origin = staged.origin_digest().to_owned();
        drop(staged);
        source.close().await.unwrap();
        let lock = CanonicalLock::acquire(&root, &[]).unwrap();
        let Selection::Legacy(legacy) = read(&lock, kind).unwrap() else {
            panic!("legacy")
        };
        let owner = Opened::open(lock, kind, &origin, &mut crate::StoreDiagnostics::new(kind))
            .await
            .unwrap();
        let replacement = b"{\"version\":99}";
        std::fs::write(root.join("format.json"), replacement).unwrap();
        assert!(publish(&owner, &legacy).is_err());
        assert_eq!(
            std::fs::read(root.join("format.json")).unwrap(),
            replacement
        );
        owner.close().await.unwrap();
    }
}
