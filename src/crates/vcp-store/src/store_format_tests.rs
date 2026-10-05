// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{contract::CanonicalStore, legacy_store_fixture::LegacyFixture as Store, Barrier};
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

#[cfg(windows)]
#[test]
fn marker_replacement_preserves_shared_readers_and_never_removes_old_bytes() {
    use std::io::Read;
    let temp = tempfile::tempdir().unwrap();
    for iteration in 0..32 {
        let target = temp.path().join(format!("marker-{iteration}.json"));
        let source = temp.path().join(format!("replacement-{iteration}.pending"));
        let prior = b"{\"version\":1}";
        let next = b"{\"version\":3}";
        immutable_file(&target, prior).unwrap();
        let mut reader = std::fs::File::open(&target).unwrap();
        crate::private_paths::write_private(&source, next).unwrap();
        replace(&source, &target).unwrap();
        assert!(!source.exists());
        assert_eq!(std::fs::read(&target).unwrap(), next);
        let mut retained = Vec::new();
        reader.read_to_end(&mut retained).unwrap();
        assert_eq!(retained, prior);
    }
}

#[cfg(windows)]
#[test]
fn replacement_retries_only_transient_io_with_exact_identity_and_a_finite_bound() {
    use std::cell::Cell;
    for corrupt in [None, Some("pending"), Some("selected")] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("pending");
        let target = temp.path().join("format.json");
        std::fs::write(&source, b"next").unwrap();
        std::fs::write(&target, b"prior").unwrap();
        let checks = Cell::new(0);
        let calls = Cell::new(0);
        let verify = || {
            checks.set(checks.get() + 1);
            if std::fs::read(&source)? != b"next" || std::fs::read(&target)? != b"prior" {
                return Err(Error::Corruption("fixture identity changed"));
            }
            Ok(false)
        };
        let result = replace_checked(&source, &target, &verify, &mut |source, target| {
            calls.set(calls.get() + 1);
            if calls.get() == 1 {
                if let Some(part) = corrupt {
                    std::fs::write(if part == "pending" { source } else { target }, b"changed")?;
                }
                return Err(std::io::Error::from_raw_os_error(5).into());
            }
            replace(source, target)
        });
        assert_eq!(checks.get(), 2);
        if corrupt.is_none() {
            result.unwrap();
            assert_eq!(calls.get(), 2);
            assert_eq!(std::fs::read(&target).unwrap(), b"next");
        } else {
            assert!(matches!(
                result,
                Err(Error::Corruption("fixture identity changed"))
            ));
            assert_eq!(calls.get(), 1);
            assert_eq!(
                std::fs::read(&target).unwrap(),
                if corrupt == Some("selected") {
                    &b"changed"[..]
                } else {
                    &b"prior"[..]
                }
            );
        }
    }
    let calls = Cell::new(0);
    let path = Path::new("unused");
    let result = replace_checked(path, path, &|| Ok(false), &mut |_, _| {
        calls.set(calls.get() + 1);
        Err(std::io::Error::from_raw_os_error(32).into())
    });
    assert!(matches!(result, Err(Error::Io(e)) if e.raw_os_error() == Some(32)));
    assert_eq!(calls.get(), 8);
    calls.set(0);
    assert!(replace_checked(path, path, &|| Ok(false), &mut |_, _| {
        calls.set(calls.get() + 1);
        Err(std::io::Error::from_raw_os_error(112).into())
    })
    .is_err());
    assert_eq!(calls.get(), 1);
    assert!(replace_checked(path, path, &|| Ok(true), &mut |_, _| {
        panic!("already verified new marker must not be replaced")
    })
    .is_ok());
}
