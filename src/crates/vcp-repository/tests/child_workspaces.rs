// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use vcp_domain::{Revision, RootId, TaskId, WorkspaceId};
use vcp_repository::{
    dirty_snapshot::CapturePolicy,
    worktree::{Snapshotter, WorkspaceRegistration},
    *,
};

fn root(path: &Path, worktree: &str) -> Root {
    Root::open(
        RootIdentity {
            workspace: WorkspaceId::parse("child-fixture").unwrap(),
            root: RootId::new(),
            repository: "fixture-repo".into(),
            worktree: worktree.into(),
            binding: Revision::ZERO,
        },
        path,
    )
    .unwrap()
}
fn executable() -> PathBuf {
    std::env::var_os("VCP_TEST_GIT")
        .map(PathBuf::from)
        .expect("VCP_TEST_GIT must identify native Git")
}
fn git(path: &Path, args: &[&str]) -> Vec<u8> {
    let output = Command::new(executable())
        .current_dir(path)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
fn snapshotter() -> Snapshotter {
    let environment: BTreeMap<OsString, OsString> = ["SystemRoot", "WINDIR", "PATH", "TEMP", "TMP"]
        .into_iter()
        .filter_map(|name| std::env::var_os(name).map(|value| (name.into(), value)))
        .collect();
    Snapshotter::new(
        executable(),
        environment,
        Duration::from_secs(20),
        8 * 1024 * 1024,
    )
    .unwrap()
}
fn initialize(path: &Path) {
    git(path, &["init", "-q"]);
    git(path, &["config", "user.name", "Fixture"]);
    git(path, &["config", "user.email", "fixture@example.invalid"]);
    git(path, &["config", "core.autocrlf", "false"]);
    fs::write(path.join("tracked.txt"), b"base\n").unwrap();
    fs::write(path.join("removed.txt"), b"deleted in worktree\n").unwrap();
    fs::write(path.join("staged-delete.txt"), b"deleted in index\n").unwrap();
    fs::write(path.join(".gitignore"), b"ignored.txt\n").unwrap();
    git(path, &["add", "."]);
    git(path, &["commit", "-qm", "base"]);
}
fn registration(
    source: &Root,
    parent: &Root,
    snapshot: &dirty_snapshot::WorkspaceSnapshot,
    name: &str,
) -> WorkspaceRegistration {
    WorkspaceRegistration {
        owner: TaskId::new(),
        source: source.identity.clone(),
        child: RootIdentity {
            root: RootId::new(),
            worktree: name.into(),
            ..source.identity.clone()
        },
        absolute_path: parent.path().join(name),
        snapshot: snapshot.fingerprint.clone(),
        git_metadata_owner: snapshot
            .base_commit
            .as_ref()
            .map(|_| worktree::RegisteredRoot {
                identity: source.identity.clone(),
                absolute_path: source.path().to_path_buf(),
            }),
    }
}

struct CleanupFixture {
    _temp: tempfile::TempDir,
    source: Root,
    parent: Root,
    child: Root,
    registration: WorkspaceRegistration,
    service: Snapshotter,
    intent: cleanup::CleanupIntent,
}
impl CleanupFixture {
    async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("source");
        let parent_path = temp.path().join("disposables");
        fs::create_dir(&source_path).unwrap();
        fs::create_dir(&parent_path).unwrap();
        fs::create_dir(source_path.join("nested")).unwrap();
        fs::write(source_path.join("nested/file.txt"), b"retained result").unwrap();
        let source = root(&source_path, "source");
        let parent = root(&parent_path, "disposables");
        let service = snapshotter();
        let snapshot = service
            .capture(
                &source,
                &CapturePolicy {
                    untracked: ["nested/file.txt".into()].into(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let registration = registration(&source, &parent, &snapshot, "owned-child");
        let child = service
            .materialize(&source, &parent, &snapshot, &registration)
            .await
            .unwrap()
            .root;
        assert!(child
            .path()
            .starts_with(fs::canonicalize(temp.path()).unwrap()));
        let native = child.hold(None, true).unwrap().native_identity;
        let intent = service
            .prepare_cleanup(&parent, &child, None, &registration, &native)
            .unwrap();
        Self {
            _temp: temp,
            source,
            parent,
            child,
            registration,
            service,
            intent,
        }
    }
}

#[tokio::test]
async fn cleanup_exact_owned_root_preserves_parent_and_reconciles_removed_root() {
    let fixture = CleanupFixture::new().await;
    fs::write(fixture.parent.path().join("unrelated.txt"), b"human").unwrap();
    let receipt = cleanup::remove_cleanup(&fixture.parent, None, &fixture.intent).unwrap();
    assert!(receipt.removed);
    assert!(!fixture.child.path().exists());
    assert_eq!(
        fs::read(fixture.parent.path().join("unrelated.txt")).unwrap(),
        b"human"
    );
    assert_eq!(
        fs::read(fixture.source.path().join("nested/file.txt")).unwrap(),
        b"retained result"
    );
    assert_eq!(
        cleanup::remove_cleanup(&fixture.parent, None, &fixture.intent).unwrap(),
        receipt
    );
}

#[tokio::test]
async fn cleanup_admission_gate_stops_between_native_removals_and_reuses_same_intent() {
    let fixture = CleanupFixture::new().await;
    let calls = std::cell::Cell::new(0usize);
    let result = cleanup::remove_cleanup_with_gate(&fixture.parent, None, &fixture.intent, || {
        calls.set(calls.get() + 1);
        if calls.get() >= 3 {
            Err(vcp_repository::Error::Scope("owner paused".into()))
        } else {
            Ok(())
        }
    });
    assert!(result.is_err());
    assert!(!fixture.child.path().join("nested/file.txt").exists());
    assert!(fixture.child.path().join("nested").exists());
    assert!(fixture.child.path().join(".vcp-child-owner").exists());
    cleanup::remove_cleanup(&fixture.parent, None, &fixture.intent).unwrap();
    assert!(!fixture.child.path().exists());
}

#[tokio::test]
async fn cleanup_rejects_changed_unlisted_and_replaced_entries_without_removing_other_files() {
    for change in ["changed", "unlisted", "replaced-marker", "replaced-file"] {
        let fixture = CleanupFixture::new().await;
        let child = fixture.child.path();
        match change {
            "changed" => fs::write(child.join("nested/file.txt"), b"new human edit").unwrap(),
            "unlisted" => fs::write(child.join("human.txt"), b"new human file").unwrap(),
            "replaced-marker" => fs::write(child.join(".vcp-child-owner"), b"other owner").unwrap(),
            "replaced-file" => {
                fs::rename(
                    child.join("nested/file.txt"),
                    fixture.parent.path().join("retained-original"),
                )
                .unwrap();
                fs::write(child.join("nested/file.txt"), b"retained result").unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            cleanup::remove_cleanup(&fixture.parent, None, &fixture.intent).is_err(),
            "{change}"
        );
        assert!(child.join("nested/file.txt").exists(), "{change}");
        assert!(child.join(".vcp-child-owner").exists(), "{change}");
    }
}

#[tokio::test]
async fn cleanup_rejects_moved_replaced_root_even_with_identical_marker() {
    let fixture = CleanupFixture::new().await;
    let moved = fixture.parent.path().join("moved-child");
    fs::rename(fixture.child.path(), &moved).unwrap();
    fs::create_dir(fixture.child.path()).unwrap();
    fs::copy(
        moved.join(".vcp-child-owner"),
        fixture.child.path().join(".vcp-child-owner"),
    )
    .unwrap();
    assert!(matches!(
        cleanup::remove_cleanup(&fixture.parent, None, &fixture.intent),
        Err(Error::Stale)
    ));
    let replacement = Root::open(fixture.child.identity.clone(), fixture.child.path()).unwrap();
    assert!(fixture
        .service
        .prepare_cleanup(
            &fixture.parent,
            &replacement,
            None,
            &fixture.registration,
            &fixture.intent.native_identity
        )
        .is_err());
    assert!(moved.join("nested/file.txt").exists());
    assert!(fixture.child.path().join(".vcp-child-owner").exists());
}

#[tokio::test]
async fn cleanup_locked_native_file_blocks_before_any_removal_then_retries() {
    let fixture = CleanupFixture::new().await;
    let locked = fixture
        .child
        .hold(Some(Path::new("nested/file.txt")), false)
        .unwrap();
    assert!(cleanup::remove_cleanup(&fixture.parent, None, &fixture.intent).is_err());
    assert!(fixture.child.path().join(".vcp-child-owner").exists());
    assert!(fixture.child.path().join("nested/file.txt").exists());
    drop(locked);
    cleanup::remove_cleanup(&fixture.parent, None, &fixture.intent).unwrap();
}

#[tokio::test]
async fn cleanup_partial_removal_and_final_marker_gap_reconcile_same_intent() {
    let fixture = CleanupFixture::new().await;
    // Simulate process exit after one exact entry, before canonical receipt.
    fs::remove_file(fixture.child.path().join("nested/file.txt")).unwrap();
    cleanup::remove_cleanup(&fixture.parent, None, &fixture.intent).unwrap();
    let fixture = CleanupFixture::new().await;
    fs::remove_file(fixture.child.path().join("nested/file.txt")).unwrap();
    fs::remove_dir(fixture.child.path().join("nested")).unwrap();
    fs::remove_file(fixture.child.path().join(".vcp-child-owner")).unwrap();
    cleanup::remove_cleanup(&fixture.parent, None, &fixture.intent).unwrap();
    assert!(!fixture.child.path().exists());
}

#[tokio::test]
async fn cleanup_native_partial_failure_retains_intent_and_marker_for_retry() {
    let fixture = CleanupFixture::new().await;
    let marker = fixture.child.path().join(".vcp-child-owner");
    let original = fs::metadata(&marker).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&marker, readonly).unwrap();
    // Actual native deletion proceeds through the content, then fails at the
    // protected marker. The error must not discard the committed intent.
    assert!(cleanup::remove_cleanup(&fixture.parent, None, &fixture.intent).is_err());
    assert!(!fixture.child.path().join("nested/file.txt").exists());
    assert!(marker.exists());
    fs::set_permissions(&marker, original).unwrap();
    cleanup::remove_cleanup(&fixture.parent, None, &fixture.intent).unwrap();
    assert!(!fixture.child.path().exists());
}

#[tokio::test]
async fn cleanup_refuses_wrong_parent_and_junction_escape() {
    let fixture = CleanupFixture::new().await;
    assert!(cleanup::remove_cleanup(&fixture.source, None, &fixture.intent).is_err());
    let link = fixture.child.path().join("escape");
    let result = Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&link)
        .arg(fixture.source.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(cleanup::remove_cleanup(&fixture.parent, None, &fixture.intent).is_err());
    assert!(fixture.source.path().join("nested/file.txt").exists());
    // Remove only the freshly created junction, not its destination.
    fs::remove_dir(&link).unwrap();
    cleanup::remove_cleanup(&fixture.parent, None, &fixture.intent).unwrap();
}

#[tokio::test]
async fn cleanup_git_child_preserves_registered_common_metadata_and_history() {
    let temp = tempfile::tempdir().unwrap();
    let source_path = temp.path().join("source");
    let parent_path = temp.path().join("children");
    fs::create_dir(&source_path).unwrap();
    fs::create_dir(&parent_path).unwrap();
    initialize(&source_path);
    let source = root(&source_path, "source");
    let parent = root(&parent_path, "children");
    let service = snapshotter();
    let snapshot = service
        .capture(&source, &CapturePolicy::default())
        .await
        .unwrap();
    let registration = registration(&source, &parent, &snapshot, "child");
    let child = service
        .materialize(&source, &parent, &snapshot, &registration)
        .await
        .unwrap()
        .root;
    assert!(child
        .path()
        .starts_with(fs::canonicalize(temp.path()).unwrap()));
    let index = fs::read(source.path().join(".git/index")).unwrap();
    let native = child.hold(None, true).unwrap().native_identity;
    assert!(service
        .prepare_cleanup(&parent, &child, None, &registration, &native)
        .is_err());
    let intent = service
        .prepare_cleanup(&parent, &child, Some(&source), &registration, &native)
        .unwrap();
    assert!(cleanup::remove_cleanup(&parent, None, &intent).is_err());
    let receipt = cleanup::remove_cleanup(&parent, Some(&source), &intent).unwrap();
    assert!(receipt.git_metadata_retained);
    assert!(!child.path().exists());
    assert_eq!(fs::read(source.path().join(".git/index")).unwrap(), index);
    assert!(source.path().join(".git/worktrees/child/HEAD").exists());
    assert!(source.path().join("tracked.txt").exists());
}

#[tokio::test]
async fn materialization_preserves_dirty_index_working_binary_deletions_and_scoped_untracked() {
    let temp = tempfile::tempdir().unwrap();
    let source_path = temp.path().join("parent");
    let children = temp.path().join("children");
    fs::create_dir(&source_path).unwrap();
    fs::create_dir(&children).unwrap();
    initialize(&source_path);
    fs::write(source_path.join("tracked.txt"), b"staged\n").unwrap();
    git(&source_path, &["add", "tracked.txt"]);
    git(&source_path, &["update-index", "--chmod=+x", "tracked.txt"]);
    fs::write(source_path.join("tracked.txt"), b"working\0binary\r\n").unwrap();
    fs::remove_file(source_path.join("removed.txt")).unwrap();
    git(&source_path, &["rm", "-q", "staged-delete.txt"]);
    fs::write(source_path.join("input.bin"), [0, 255, 0, 128]).unwrap();
    fs::write(source_path.join("ignored.txt"), b"ignored").unwrap();
    fs::write(source_path.join(".env"), b"not a real secret").unwrap();
    fs::write(source_path.join("unrelated.txt"), b"unrelated").unwrap();
    let source = root(&source_path, "parent");
    let parent = root(&children, "disposables");
    let before_index = fs::read(source_path.join(".git/index")).unwrap();
    let before_status = git(
        &source_path,
        &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
    );
    let service = snapshotter();
    let policy = CapturePolicy {
        untracked: ["input.bin".into(), ".env".into()].into(),
        required: ["tracked.txt".into(), "input.bin".into()].into(),
        ..Default::default()
    };
    let snapshot = service.capture(&source, &policy).await.unwrap();
    assert!(snapshot.excluded.contains(&".env".into()));
    assert!(snapshot.excluded.contains(&"unrelated.txt".into()));
    let registration = registration(&source, &parent, &snapshot, "child");
    let materialized = service
        .materialize(&source, &parent, &snapshot, &registration)
        .await
        .unwrap();
    let child = materialized.root.path();
    assert_eq!(
        fs::read(child.join("tracked.txt")).unwrap(),
        b"working\0binary\r\n"
    );
    assert_eq!(git(child, &["show", ":tracked.txt"]), b"staged\n");
    assert_eq!(
        git(child, &["ls-files", "--stage", "-z"]),
        git(&source_path, &["ls-files", "--stage", "-z"])
    );
    assert_eq!(fs::read(child.join("input.bin")).unwrap(), [0, 255, 0, 128]);
    assert!(!child.join("removed.txt").exists());
    assert!(!child.join("staged-delete.txt").exists());
    assert!(!child.join("ignored.txt").exists());
    assert!(!child.join(".env").exists());
    assert!(!child.join("unrelated.txt").exists());
    assert_eq!(
        fs::read(source_path.join(".git/index")).unwrap(),
        before_index
    );
    assert_eq!(
        git(
            &source_path,
            &["status", "--porcelain=v2", "-z", "--untracked-files=all"]
        ),
        before_status
    );
    assert_eq!(
        fs::read(source_path.join("tracked.txt")).unwrap(),
        b"working\0binary\r\n"
    );
    assert!(service
        .materialize(&source, &parent, &snapshot, &registration)
        .await
        .is_err());
    service
        .verify(&source, &materialized.root, &snapshot, &registration)
        .await
        .unwrap();
    for extra in ["unexpected.txt", "ignored.txt", ".env"] {
        fs::write(child.join(extra), b"unrecorded input").unwrap();
        assert!(matches!(
            service
                .verify(&source, &materialized.root, &snapshot, &registration)
                .await,
            Err(Error::Stale)
        ));
        fs::remove_file(child.join(extra)).unwrap();
    }
    fs::write(child.join("tracked.txt"), b"child-only edit").unwrap();
    assert!(matches!(
        service
            .verify(&source, &materialized.root, &snapshot, &registration)
            .await,
        Err(Error::Stale)
    ));
    assert_eq!(
        fs::read(source_path.join("tracked.txt")).unwrap(),
        b"working\0binary\r\n"
    );
}

#[tokio::test]
async fn metadata_junction_is_rejected_before_any_external_write() {
    let temp = tempfile::tempdir().unwrap();
    let source_path = temp.path().join("parent");
    let children = temp.path().join("children");
    let external = temp.path().join("external");
    for directory in [&source_path, &children, &external] {
        fs::create_dir(directory).unwrap();
    }
    initialize(&source_path);
    fs::write(external.join("sentinel"), b"preserve").unwrap();
    let junction = source_path.join(".git/worktrees");
    let status = Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(
            junction
                .to_string_lossy()
                .trim_start_matches(r"\\?\")
                .replace('/', "\\"),
        )
        .arg(
            external
                .to_string_lossy()
                .trim_start_matches(r"\\?\")
                .replace('/', "\\"),
        )
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "junction fixture unavailable: {}",
        String::from_utf8_lossy(&status.stderr)
    );
    let source = root(&source_path, "parent");
    let parent = root(&children, "children");
    let service = snapshotter();
    let snapshot = service
        .capture(&source, &CapturePolicy::default())
        .await
        .unwrap();
    let registration = registration(&source, &parent, &snapshot, "child");
    assert!(matches!(
        service
            .materialize(&source, &parent, &snapshot, &registration)
            .await,
        Err(Error::Link)
    ));
    assert_eq!(fs::read(external.join("sentinel")).unwrap(), b"preserve");
    assert_eq!(fs::read_dir(&external).unwrap().count(), 1);
    assert!(!registration.absolute_path.exists());
    fs::remove_dir(junction).unwrap();
}

#[tokio::test]
async fn missing_ignored_sensitive_and_unborn_inputs_are_explicit_setup_constraints() {
    let temp = tempfile::tempdir().unwrap();
    let source = root(temp.path(), "parent");
    let service = snapshotter();
    assert!(service
        .capture(&source, &CapturePolicy::default())
        .await
        .unwrap()
        .base_commit
        .is_none());
    git(temp.path(), &["init", "-q"]);
    assert!(matches!(
        service.capture(&source, &CapturePolicy::default()).await,
        Err(Error::Unsupported(_))
    ));
    initialize(temp.path());
    fs::write(temp.path().join("ignored.txt"), b"omitted").unwrap();
    for required in ["missing.txt", "ignored.txt", ".env"] {
        let policy = CapturePolicy {
            untracked: [required.into()].into(),
            required: [required.into()].into(),
            ..Default::default()
        };
        assert!(matches!(
            service.capture(&source, &policy).await,
            Err(Error::Scope(_))
        ));
    }
    fs::write(temp.path().join(".env"), b"synthetic sensitive input").unwrap();
    git(temp.path(), &["add", ".env"]);
    assert!(matches!(
        service.capture(&source, &CapturePolicy::default()).await,
        Err(Error::Unsupported(_))
    ));
}

#[tokio::test]
async fn non_git_copy_is_scoped_binary_safe_isolated_and_verifiable() {
    let temp = tempfile::tempdir().unwrap();
    let source_path = temp.path().join("parent");
    let children = temp.path().join("children");
    fs::create_dir(&source_path).unwrap();
    fs::create_dir(&children).unwrap();
    fs::create_dir(source_path.join("nested")).unwrap();
    fs::write(source_path.join("nested/input.bin"), [0, 255, 13, 10]).unwrap();
    fs::write(source_path.join(".gitignore"), b"ignored.txt\n").unwrap();
    for name in ["ignored.txt", ".env", "unrelated.txt"] {
        fs::write(source_path.join(name), b"excluded input").unwrap();
    }
    let source = root(&source_path, "parent");
    let parent = root(&children, "children");
    let service = snapshotter();
    let policy = CapturePolicy {
        untracked: [
            "nested/input.bin".into(),
            "ignored.txt".into(),
            ".env".into(),
        ]
        .into(),
        required: ["nested/input.bin".into()].into(),
        ..Default::default()
    };
    let snapshot = service.capture(&source, &policy).await.unwrap();
    assert!(snapshot.base_commit.is_none());
    assert_eq!(snapshot.files.len(), 1);
    assert_eq!(snapshot.excluded, vec![".env", "ignored.txt"]);
    let registration = registration(&source, &parent, &snapshot, "child");
    let child = service
        .materialize(&source, &parent, &snapshot, &registration)
        .await
        .unwrap();
    assert_eq!(
        fs::read(child.root.path().join("nested/input.bin")).unwrap(),
        [0, 255, 13, 10]
    );
    for name in [".git", "ignored.txt", ".env", "unrelated.txt"] {
        assert!(!child.root.path().join(name).exists());
    }
    service
        .verify(&source, &child.root, &snapshot, &registration)
        .await
        .unwrap();
    fs::write(child.root.path().join("nested/input.bin"), b"child edit").unwrap();
    assert_eq!(
        fs::read(source_path.join("nested/input.bin")).unwrap(),
        [0, 255, 13, 10]
    );
    assert!(matches!(
        service
            .verify(&source, &child.root, &snapshot, &registration)
            .await,
        Err(Error::Stale)
    ));
    let nested_snapshot = service
        .capture_registered(&child.root, &source, &registration, &policy)
        .await
        .unwrap();
    let nested_registration = WorkspaceRegistration {
        owner: TaskId::new(),
        source: child.root.identity.clone(),
        child: RootIdentity {
            root: RootId::new(),
            worktree: "nested-copy".into(),
            ..child.root.identity.clone()
        },
        absolute_path: parent.path().join("nested-copy"),
        snapshot: nested_snapshot.fingerprint.clone(),
        git_metadata_owner: None,
    };
    let nested = service
        .materialize_with_metadata(
            &child.root,
            &source,
            &parent,
            &nested_snapshot,
            &nested_registration,
        )
        .await
        .unwrap();
    assert_eq!(
        fs::read(nested.root.path().join("nested/input.bin")).unwrap(),
        b"child edit"
    );
    let required_ignored = CapturePolicy {
        required: ["ignored.txt".into()].into(),
        ..policy
    };
    assert!(matches!(
        service.capture(&source, &required_ignored).await,
        Err(Error::Scope(_))
    ));
}

#[tokio::test]
async fn nested_registered_worktree_preserves_child_staging_and_rejects_unrelated_metadata_owner() {
    let temp = tempfile::tempdir().unwrap();
    let source_path = temp.path().join("parent");
    let children = temp.path().join("children");
    fs::create_dir(&source_path).unwrap();
    fs::create_dir(&children).unwrap();
    initialize(&source_path);
    let source = root(&source_path, "parent");
    let parent = root(&children, "children");
    let service = snapshotter();
    let initial = service
        .capture(&source, &CapturePolicy::default())
        .await
        .unwrap();
    let child_registration = registration(&source, &parent, &initial, "child");
    let child = service
        .materialize(&source, &parent, &initial, &child_registration)
        .await
        .unwrap();
    fs::write(child.root.path().join("tracked.txt"), b"child staged\n").unwrap();
    git(child.root.path(), &["add", "tracked.txt"]);
    fs::write(child.root.path().join("tracked.txt"), b"child working\n").unwrap();
    fs::write(child.root.path().join("input.txt"), b"nested input\n").unwrap();
    let before_index = git(child.root.path(), &["ls-files", "--stage", "-z"]);
    let policy = CapturePolicy {
        untracked: ["input.txt".into()].into(),
        ..Default::default()
    };
    assert!(service
        .capture_registered(&child.root, &parent, &child_registration, &policy)
        .await
        .is_err());
    let snapshot = service
        .capture_registered(&child.root, &source, &child_registration, &policy)
        .await
        .unwrap();
    let mut nested_registration = registration(&child.root, &parent, &snapshot, "grandchild");
    nested_registration.git_metadata_owner = child_registration.git_metadata_owner.clone();
    let nested = service
        .materialize_with_metadata(
            &child.root,
            &source,
            &parent,
            &snapshot,
            &nested_registration,
        )
        .await
        .unwrap();
    assert_eq!(
        git(nested.root.path(), &["show", ":tracked.txt"]),
        b"child staged\n"
    );
    assert_eq!(
        fs::read(nested.root.path().join("tracked.txt")).unwrap(),
        b"child working\n"
    );
    assert_eq!(
        fs::read(nested.root.path().join("input.txt")).unwrap(),
        b"nested input\n"
    );
    assert_eq!(
        git(child.root.path(), &["ls-files", "--stage", "-z"]),
        before_index
    );
    assert_eq!(
        fs::read(source.path().join("tracked.txt")).unwrap(),
        b"base\n"
    );
    service
        .verify_with_metadata(
            &child.root,
            &source,
            &nested.root,
            &snapshot,
            &nested_registration,
        )
        .await
        .unwrap();
    fs::write(
        child.root.path().join(".git"),
        b"gitdir: \\\\untrusted.invalid\\share\\metadata\n",
    )
    .unwrap();
    assert!(matches!(
        service
            .capture_registered(&child.root, &source, &child_registration, &policy)
            .await,
        Err(Error::Scope(_))
    ));
}

#[tokio::test]
async fn dropping_copy_materialization_stops_between_files_and_keeps_recovery_path() {
    use std::{future::Future, task::Poll};
    let temp = tempfile::tempdir().unwrap();
    let source_path = temp.path().join("source");
    let children = temp.path().join("children");
    fs::create_dir(&source_path).unwrap();
    fs::create_dir(&children).unwrap();
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(source_path.join(name), name.as_bytes()).unwrap();
    }
    let source = root(&source_path, "source");
    let parent = root(&children, "children");
    let service = snapshotter();
    let policy = CapturePolicy {
        untracked: ["a.txt".into(), "b.txt".into(), "c.txt".into()].into(),
        ..Default::default()
    };
    let snapshot = service.capture(&source, &policy).await.unwrap();
    let registration = registration(&source, &parent, &snapshot, "child");
    let mut pending = Box::pin(service.materialize(&source, &parent, &snapshot, &registration));
    for _ in 0..3 {
        std::future::poll_fn(|context| {
            assert!(pending.as_mut().poll(context).is_pending());
            Poll::Ready(())
        })
        .await;
    }
    drop(pending);
    assert!(registration
        .absolute_path
        .join(".vcp-child-owner")
        .is_file());
    assert_eq!(
        fs::read(registration.absolute_path.join("a.txt")).unwrap(),
        b"a.txt"
    );
    assert!(!registration.absolute_path.join("b.txt").exists());
    assert!(!registration.absolute_path.join("c.txt").exists());
    for name in ["a.txt", "b.txt", "c.txt"] {
        assert_eq!(fs::read(source_path.join(name)).unwrap(), name.as_bytes());
    }
    let child = Root::open(registration.child.clone(), &registration.absolute_path).unwrap();
    assert!(service
        .verify(&source, &child, &snapshot, &registration)
        .await
        .is_err());
    assert!(service
        .materialize(&source, &parent, &snapshot, &registration)
        .await
        .is_err());
}

#[tokio::test]
async fn hooks_are_suppressed_and_registration_cannot_reuse_parent_or_other_workspace() {
    let temp = tempfile::tempdir().unwrap();
    let source_path = temp.path().join("parent");
    let children = temp.path().join("children");
    fs::create_dir(&source_path).unwrap();
    fs::create_dir(&children).unwrap();
    initialize(&source_path);
    fs::write(
        source_path.join(".git/hooks/post-checkout"),
        b"#!/bin/sh\necho hook-ran > hook-marker\n",
    )
    .unwrap();
    let source = root(&source_path, "parent");
    let parent = root(&children, "children");
    let service = snapshotter();
    let snapshot = service
        .capture(&source, &CapturePolicy::default())
        .await
        .unwrap();
    let mut invalid = registration(&source, &parent, &snapshot, "child");
    invalid.absolute_path = source.path().to_path_buf();
    assert!(service
        .materialize(&source, &parent, &snapshot, &invalid)
        .await
        .is_err());
    let registration = registration(&source, &parent, &snapshot, "child");
    let child = service
        .materialize(&source, &parent, &snapshot, &registration)
        .await
        .unwrap();
    assert!(!source.path().join("hook-marker").exists());
    assert!(!child.root.path().join("hook-marker").exists());
    fs::write(
        child.root.path().join(".vcp-child-owner"),
        b"different owner",
    )
    .unwrap();
    assert!(service
        .verify(&source, &child.root, &snapshot, &registration)
        .await
        .is_err());
}
