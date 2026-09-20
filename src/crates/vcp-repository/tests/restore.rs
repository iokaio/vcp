// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
use std::{fs, path::Path};
use vcp_domain::{CommandId, Revision, RootId, WorkspaceId};
use vcp_repository::{
    restore::{Published, Staging},
    Root, RootIdentity,
};
fn root(path: &Path) -> Root {
    Root::open(
        RootIdentity {
            workspace: WorkspaceId::new(),
            root: RootId::new(),
            repository: "fixture".into(),
            worktree: "restore".into(),
            binding: Revision::ZERO,
        },
        path,
    )
    .unwrap()
}
#[test]
fn directory_publication_keeps_native_identity_and_descendant_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let parent = root(temp.path());
    let id = CommandId::new();
    let stage = Staging::create(&parent, id.as_str()).unwrap();
    let identity = stage.native_identity().to_owned();
    assert!(fs::rename(stage.path(), temp.path().join("stolen")).is_err());
    fs::create_dir(stage.path().join("nested")).unwrap();
    {
        let _guard = stage.root().hold(Some(Path::new("nested")), true).unwrap();
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(stage.path().join("nested/source.txt"))
            .unwrap();
        use std::io::Write;
        file.write_all(b"retained dirty source\r\n").unwrap();
        file.sync_all().unwrap();
    }
    let published = stage.publish("destination ü").unwrap();
    assert_eq!(published.native_identity(), identity);
    assert_eq!(
        published
            .root()
            .read(Path::new("nested/source.txt"), 1024)
            .unwrap()
            .bytes,
        b"retained dirty source\r\n"
    );
    assert!(!temp.path().join(id.as_str()).exists());
    assert!(fs::rename(published.path(), temp.path().join("stolen")).is_err());
    drop(published);
    let reopened = Published::reopen(&parent, "destination ü", &identity).unwrap();
    assert_eq!(reopened.native_identity(), identity);
    assert!(Published::reopen(&parent, "destination ü", "different native identity").is_err());
}
#[test]
fn every_destination_collision_preserves_source_and_destination() {
    for kind in ["empty", "nonempty", "file"] {
        let temp = tempfile::tempdir().unwrap();
        let parent = root(temp.path());
        let id = CommandId::new();
        let stage = Staging::create(&parent, id.as_str()).unwrap();
        let identity = stage.native_identity().to_owned();
        fs::write(stage.path().join("source.txt"), b"source").unwrap();
        let destination = temp.path().join("destination");
        match kind {
            "empty" => fs::create_dir(&destination).unwrap(),
            "nonempty" => {
                fs::create_dir(&destination).unwrap();
                fs::write(destination.join("keep.txt"), b"existing").unwrap();
            }
            _ => fs::write(&destination, b"existing file").unwrap(),
        }
        assert!(stage.publish("destination").is_err());
        let recovered = Staging::reopen(&parent, id.as_str(), &identity).unwrap();
        assert_eq!(
            fs::read(recovered.path().join("source.txt")).unwrap(),
            b"source"
        );
        match kind {
            "empty" => assert_eq!(fs::read_dir(destination).unwrap().count(), 0),
            "nonempty" => assert_eq!(fs::read(destination.join("keep.txt")).unwrap(), b"existing"),
            _ => assert_eq!(fs::read(destination).unwrap(), b"existing file"),
        }
        assert!(Staging::create(&parent, id.as_str()).is_err());
    }
}
#[test]
fn replaced_stage_and_junction_parent_are_rejected_without_modifying_them() {
    let temp = tempfile::tempdir().unwrap();
    let parent_path = temp.path().join("parent");
    fs::create_dir(&parent_path).unwrap();
    let parent = root(&parent_path);
    let id = CommandId::new();
    let stage = Staging::create(&parent, id.as_str()).unwrap();
    let expected = stage.native_identity().to_owned();
    drop(stage);
    fs::rename(parent_path.join(id.as_str()), parent_path.join("original")).unwrap();
    fs::create_dir(parent_path.join(id.as_str())).unwrap();
    fs::write(
        parent_path.join(id.as_str()).join("keep.txt"),
        b"replacement",
    )
    .unwrap();
    assert!(Staging::reopen(&parent, id.as_str(), &expected).is_err());
    assert_eq!(
        fs::read(parent_path.join(id.as_str()).join("keep.txt")).unwrap(),
        b"replacement"
    );
    fs::rename(&parent_path, temp.path().join("previous-parent")).unwrap();
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    let status = std::process::Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&parent_path)
        .arg(&outside)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "junction fixture prerequisite failed"
    );
    assert!(Staging::create(&parent, CommandId::new().as_str()).is_err());
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
    fs::remove_dir(&parent_path).unwrap(); // Only the fixture junction, never its target.
}
