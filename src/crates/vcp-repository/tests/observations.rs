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
use vcp_domain::{Revision, RootId, WorkspaceId};
use vcp_repository::{
    discovery::{ExclusionReason, Limits},
    git::Git,
    *,
};

fn root(path: &Path) -> Root {
    Root::open(
        RootIdentity {
            workspace: WorkspaceId::parse("fixture-workspace").unwrap(),
            root: RootId::new(),
            repository: "fixture-repository".into(),
            worktree: "fixture-worktree".into(),
            binding: Revision::ZERO,
        },
        path,
    )
    .unwrap()
}
fn write(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}
fn git_exe() -> PathBuf {
    std::env::var_os("VCP_TEST_GIT")
        .map(PathBuf::from)
        .expect("runner must supply explicit VCP_TEST_GIT")
}
fn git(root: &Path, args: &[&str]) {
    assert!(Command::new(git_exe())
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
        .status
        .success());
}
fn observer() -> Git {
    let environment: BTreeMap<OsString, OsString> = ["SystemRoot", "WINDIR", "PATH", "TEMP", "TMP"]
        .into_iter()
        .filter_map(|name| std::env::var_os(name).map(|value| (name.into(), value)))
        .collect();
    Git::new(
        git_exe(),
        environment,
        Duration::from_secs(10),
        2 * 1024 * 1024,
    )
    .unwrap()
}

#[test]
fn native_identity_sharing_guards_and_stale_versions_preserve_human_edits() {
    let temp = tempfile::tempdir().unwrap();
    let root = root(temp.path());
    write(temp.path().join("space ü/file.txt"), b"first\r\n");
    let source = root.read(Path::new("space ü/file.txt"), 1024).unwrap();
    let held = root
        .hold(Some(Path::new("space ü/file.txt")), false)
        .unwrap();
    assert!(fs::write(temp.path().join("space ü/file.txt"), "blocked").is_err());
    assert!(fs::rename(temp.path().join("space ü"), temp.path().join("renamed")).is_err());
    drop(held);
    fs::write(temp.path().join("space ü/file.txt"), "human edit\r\n").unwrap();
    assert!(matches!(
        root.revalidate(&source.version),
        Err(Error::Stale) | Err(Error::Limit(_))
    ));
    assert_eq!(
        fs::read(temp.path().join("space ü/file.txt")).unwrap(),
        b"human edit\r\n"
    );
    for path in ["../escape", "file.txt:stream", "C:\\outside", "ambiguous."] {
        assert!(root.read(Path::new(path), 1024).is_err());
    }
    let mut rebound = root.identity.clone();
    rebound.binding = Revision::new(1);
    let moved = Root::open(rebound, temp.path()).unwrap();
    assert_eq!(moved.identity.root, root.identity.root);
    assert!(matches!(
        moved.revalidate(&source.version),
        Err(Error::Stale)
    ));
}

#[test]
fn native_junction_escape_is_reported_without_reading_outside_content() {
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write(outside.path().join("marker.txt"), "outside marker");
    let link = temp.path().join("escape");
    let result = Command::new("cmd.exe")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(outside.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "junction fixture prerequisite failed"
    );
    let root = root(temp.path());
    assert!(matches!(
        root.read(Path::new("escape/marker.txt"), 1024),
        Err(Error::Link)
    ));
    let observed = root.discover(&Limits::default()).unwrap();
    assert!(observed.sources.is_empty());
    assert!(observed
        .exclusions
        .iter()
        .any(|row| row.path == "escape" && row.reason == ExclusionReason::Reparse));
    assert_eq!(
        fs::read(outside.path().join("marker.txt")).unwrap(),
        b"outside marker"
    );
    // Remove only the distinctly created junction, never its target contents.
    fs::remove_dir(&link).unwrap();
    assert!(outside.path().join("marker.txt").exists());
}

#[test]
fn discovery_keeps_explicit_ignored_generated_binary_and_capacity_boundaries() {
    let temp = tempfile::tempdir().unwrap();
    let root = root(temp.path());
    write(
        temp.path().join(".gitignore"),
        "ignored/\n*.log\n!keep.log\n",
    );
    write(temp.path().join("ignored/private.txt"), "do not select");
    write(temp.path().join("keep.log"), "kept");
    write(temp.path().join("skip.log"), "ignored");
    write(temp.path().join("target/generated.rs"), "generated");
    write(temp.path().join("binary.dat"), [0, 255]);
    write(temp.path().join("large.txt"), vec![b'x'; 2048]);
    write(temp.path().join("deep/a/b/c/file.rs"), "deep");
    let observed = root
        .discover(&Limits {
            file_bytes: 1024,
            depth: 2,
            ..Limits::default()
        })
        .unwrap();
    assert!(!observed.complete);
    assert!(observed
        .sources
        .iter()
        .any(|row| row.version.path == "keep.log"));
    assert!(!observed
        .sources
        .iter()
        .any(|row| row.version.path.starts_with("ignored/")));
    for reason in [
        ExclusionReason::Ignored,
        ExclusionReason::Generated,
        ExclusionReason::Binary,
        ExclusionReason::Oversize,
        ExclusionReason::Depth,
    ] {
        assert!(
            observed.exclusions.iter().any(|row| row.reason == reason),
            "{reason:?}"
        );
    }
    let bounded = root
        .discover(&Limits {
            entries: 2,
            ..Limits::default()
        })
        .unwrap();
    assert!(!bounded.complete);
    assert!(bounded
        .exclusions
        .iter()
        .any(|row| row.reason == ExclusionReason::Entries));
}

#[test]
fn scoped_agents_and_missing_probes_keep_sibling_and_parent_authority_separate() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("repo")).unwrap();
    let parent = root(temp.path());
    let root = root(&temp.path().join("repo"));
    write(temp.path().join("AGENTS.md"), "explicit parent convention");
    write(root.path().join("AGENTS.md"), "general");
    write(root.path().join("a/AGENTS.md"), "A only");
    write(root.path().join("b/file.rs"), "b");
    write(root.path().join("a/file.rs"), "a");
    write(root.path().join("CLAUDE.md"), "must not load");
    let files = vec![PathBuf::from("a/file.rs"), PathBuf::from("b/file.rs")];
    let local = root.instructions(&files, &[], 4096).unwrap();
    assert_eq!(local.documents.len(), 2);
    let scoped = root.instructions(&files, &[parent.clone()], 4096).unwrap();
    assert_eq!(scoped.documents.len(), 3);
    let nested = scoped
        .documents
        .iter()
        .find(|row| row.source.version.path == "a/AGENTS.md")
        .unwrap();
    assert_eq!(nested.applies_to, vec!["a/file.rs"]);
    assert_eq!(
        scoped
            .documents
            .iter()
            .filter(|row| row.parent_read_only)
            .count(),
        1
    );
    scoped.revalidate(&[parent.clone(), root.clone()]).unwrap();
    write(root.path().join("b/AGENTS.md"), "new scoped constraint");
    assert!(matches!(
        scoped.revalidate(&[parent, root]),
        Err(Error::Stale)
    ));
}

#[tokio::test]
async fn actual_git_observation_retains_staged_unstaged_untracked_bytes_without_index_mutation() {
    let temp = tempfile::tempdir().unwrap();
    git(temp.path(), &["init", "--quiet"]);
    write(temp.path().join("file.txt"), "staged\r\n");
    git(temp.path(), &["add", "file.txt"]);
    write(temp.path().join("file.txt"), "unstaged\r\n");
    write(temp.path().join("new file.txt"), "untracked");
    let before = fs::read(temp.path().join(".git/index")).unwrap();
    let observed = observer().observe(&root(temp.path())).await.unwrap();
    assert_eq!(fs::read(temp.path().join(".git/index")).unwrap(), before);
    assert_eq!(
        fs::read(temp.path().join("file.txt")).unwrap(),
        b"unstaged\r\n"
    );
    assert!(observed
        .snapshot
        .changes
        .iter()
        .any(|row| row.path == "file.txt" && row.index == 'A' && row.worktree == 'M'));
    assert!(observed
        .snapshot
        .changes
        .iter()
        .any(|row| row.path == "new file.txt" && row.index == '?'));
    assert!(String::from_utf8_lossy(&observed.staged_diff).contains("+staged"));
    assert!(String::from_utf8_lossy(&observed.unstaged_diff).contains("+unstaged"));
    assert!(observed.snapshot.head.is_none());
}

#[tokio::test]
async fn git_configuration_cannot_launch_an_untrusted_filter_during_observation() {
    let temp = tempfile::tempdir().unwrap();
    git(temp.path(), &["init", "--quiet"]);
    write(temp.path().join("file.txt"), "first");
    git(temp.path(), &["add", "file.txt"]);
    write(temp.path().join("file.txt"), "changed");
    write(temp.path().join(".gitattributes"), "*.txt filter=attack\n");
    git(
        temp.path(),
        &[
            "config",
            "filter.attack.clean",
            "cmd /c echo executed > untrusted.marker",
        ],
    );
    assert!(matches!(
        observer().observe(&root(temp.path())).await,
        Err(Error::Unsupported(_))
    ));
    assert!(!temp.path().join("untrusted.marker").exists());
}

#[tokio::test]
async fn workspace_manifest_versions_untracked_bytes_and_new_ignore_rules() {
    let temp = tempfile::tempdir().unwrap();
    git(temp.path(), &["init", "--quiet"]);
    write(temp.path().join("new.txt"), "first untracked bytes");
    let root = root(temp.path());
    let git = observer();
    let first = root.observe(Some(&git), &Limits::default()).await.unwrap();
    write(temp.path().join("new.txt"), "second untracked bytes");
    let second = root.observe(Some(&git), &Limits::default()).await.unwrap();
    assert_eq!(
        first.manifest.git.as_ref().unwrap().status_sha256,
        second.manifest.git.as_ref().unwrap().status_sha256
    );
    assert_ne!(first.digest, second.digest);
    assert!(first
        .manifest
        .revalidate_selected(&root, &first.manifest.files)
        .is_err());
    second
        .manifest
        .revalidate_selected(&root, &second.manifest.files)
        .unwrap();
    write(temp.path().join(".gitignore"), "new.txt\n");
    assert!(second
        .manifest
        .revalidate_selected(&root, &second.manifest.files)
        .is_err());
}
