// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    history_index::{io, Pages},
    BackendKind, Store,
};

#[tokio::test]
async fn canonical_child_requires_actual_locked_owner_fixed_name_and_forbidden_scope() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir(&workspace).unwrap();
        let root = temp.path().join("canonical");
        let store = Store::open(&root, kind, &[workspace.clone()])
            .await
            .unwrap();
        for bad in ["", "..", "../other", "history/pages", "C:", "history.pages"] {
            assert!(Directory::canonical_child(&store, bad).is_err());
        }
        let directory = Directory::canonical_child(&store, "history-pages").unwrap();
        assert_eq!(
            directory.path,
            root.canonicalize().unwrap().join("history-pages")
        );
        let bytes = b"native immutable page";
        let digest = vcp_protocol::digest_bytes(bytes);
        let mut pages = io::Files::new(&directory);
        pages.write(&digest, bytes).await.unwrap();
        assert_eq!(pages.read(&digest, bytes.len()).await.unwrap(), bytes);
        assert!(Store::open(&root, kind, &[workspace]).await.is_err());
        assert!(store
            .validate_canonical_child(&temp.path().join("outside"))
            .is_err());
        store.close().await.unwrap();

        let root = temp.path().join("protected-parent");
        let forbidden = root.join("protected-child");
        fs::create_dir_all(&forbidden).unwrap();
        // Existing Store admission permits a sibling spool here. Deriving the
        // canonical history child must independently preserve the protected path.
        let store = Store::open(&root, kind, &[forbidden.clone()])
            .await
            .unwrap();
        assert!(Directory::canonical_child(&store, "protected-child").is_err());
        assert_eq!(fs::read_dir(forbidden).unwrap().count(), 0);
        store.close().await.unwrap();
    }
}

#[tokio::test]
async fn canonical_child_does_not_treat_existing_file_as_directory() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("canonical");
    let store = Store::open(&root, BackendKind::Files, &[]).await.unwrap();
    let path = root.join("non-directory-child");
    fs::write(&path, b"preserve existing user bytes").unwrap();
    assert!(Directory::canonical_child(&store, "non-directory-child").is_err());
    assert_eq!(fs::read(path).unwrap(), b"preserve existing user bytes");
    store.close().await.unwrap();
}

#[tokio::test]
async fn admitted_existing_children_are_read_only_and_preserve_source_exclusions() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("canonical"), BackendKind::Files, &[])
        .await
        .unwrap();
    let root = Directory::canonical_root(&store).unwrap();
    let spool = root.existing_child("spool").unwrap();
    for name in ["", ".", "..", "../outside", "spool/child", "C:", "missing"] {
        assert!(spool.existing_child(name).is_err());
    }
    assert!(!spool.path.join("missing").exists());
    let artifact = spool.path.join("artifact-1");
    fs::create_dir(&artifact).unwrap();
    assert!(spool
        .existing_child_excluding("artifact-1", &[artifact.clone()])
        .is_err());
    assert!(spool
        .existing_child_excluding("artifact-1", &[root.path.clone()])
        .is_err());
    assert_eq!(
        spool
            .existing_child_excluding("artifact-1", &[])
            .unwrap()
            .path,
        artifact
    );
    fs::write(spool.path.join("file"), b"retained").unwrap();
    assert!(spool.existing_child("file").is_err());
    assert_eq!(fs::read(spool.path.join("file")).unwrap(), b"retained");
    store.close().await.unwrap();
}
