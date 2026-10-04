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
        let forbidden = root.join("history-pages");
        fs::create_dir_all(&forbidden).unwrap();
        // Existing Store admission permits a sibling spool here. Deriving the
        // canonical history child must independently preserve the protected path.
        let store = Store::open(&root, kind, &[forbidden.clone()])
            .await
            .unwrap();
        assert!(Directory::canonical_child(&store, "history-pages").is_err());
        assert_eq!(fs::read_dir(forbidden).unwrap().count(), 0);
        store.close().await.unwrap();
    }
}

#[tokio::test]
async fn canonical_child_does_not_treat_existing_file_as_directory() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("canonical");
    let store = Store::open(&root, BackendKind::Files, &[]).await.unwrap();
    let path = root.join("history-pages");
    fs::write(&path, b"preserve existing user bytes").unwrap();
    assert!(Directory::canonical_child(&store, "history-pages").is_err());
    assert_eq!(fs::read(path).unwrap(), b"preserve existing user bytes");
    store.close().await.unwrap();
}
