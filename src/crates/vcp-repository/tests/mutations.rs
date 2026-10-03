// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
use std::{fs, path::Path};
use vcp_domain::*;
use vcp_repository::{instructions::Probe, *};
fn root(path: &Path) -> Root {
    Root::open(
        RootIdentity {
            workspace: WorkspaceId::new(),
            root: RootId::new(),
            repository: "synthetic".into(),
            worktree: "synthetic".into(),
            binding: Revision::ZERO,
        },
        path,
    )
    .unwrap()
}
fn probe(r: &Root, p: &str) -> Probe {
    Probe {
        root: r.identity.root.clone(),
        binding: r.identity.binding,
        path: p.into(),
        observed: match r.read(Path::new(p), 1024 * 1024) {
            Ok(s) => Some(s.version),
            Err(vcp_repository::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => panic!("{e}"),
        },
    }
}
#[test]
fn planned_nested_creation_reuses_only_same_dispatch_directory_identities() {
    let temp = tempfile::tempdir().unwrap();
    let r = root(temp.path());
    fs::create_dir(temp.path().join("src")).unwrap();
    let first = probe(&r, "src/Data/Models/one.cs");
    let second = probe(&r, "src/data/models/two.cs");
    let second_plan = r.prepare_parents(&second.path).unwrap();
    let plan = r.prepare_parents(&first.path).unwrap();
    assert!(!temp.path().join("src/Data").exists());
    let target = r
        .mutation_target_with_parents(&first, &plan, &Default::default())
        .unwrap();
    assert!(!temp.path().join("src/Data").exists());
    assert!(fs::rename(temp.path().join("src"), temp.path().join("moved")).is_err());
    let result = target.apply(Some(b"first\n"), None);
    assert!(result.complete, "{result:?}");
    assert_eq!(result.created_directories.len(), 2);
    assert!(r
        .mutation_target_with_parents(&second, &second_plan, &Default::default())
        .is_err());
    let created = result
        .created_directories
        .iter()
        .map(|d| (d.path.to_lowercase(), d.native_identity.clone().unwrap()))
        .collect();
    let target = r
        .mutation_target_with_parents(&second, &second_plan, &created)
        .unwrap();
    let second_result = target.apply(Some(b"second\n"), None);
    assert!(second_result.complete, "{second_result:?}");
    assert!(second_result.created_directories.is_empty());
    assert_eq!(
        fs::read(temp.path().join(&second.path)).unwrap(),
        b"second\n"
    );
    fs::rename(
        temp.path().join("src/Data/Models"),
        temp.path().join("src/Data/replaced"),
    )
    .unwrap();
    fs::create_dir(temp.path().join("src/Data/Models")).unwrap();
    let third = probe(&r, "src/Data/Models/three.cs");
    assert!(r
        .mutation_target_with_parents(&third, &plan, &created)
        .is_err());
}

#[test]
fn nested_creation_retains_partial_directories_and_never_overwrites_new_entries() {
    let temp = tempfile::tempdir().unwrap();
    let r = root(temp.path());
    let path = format!("created/{}/file", "x".repeat(256));
    let input = Probe {
        root: r.identity.root.clone(),
        binding: r.identity.binding,
        path: path.clone(),
        observed: None,
    };
    let plan = r.prepare_parents(&path).unwrap();
    let target = r
        .mutation_target_with_parents(&input, &plan, &Default::default())
        .unwrap();
    let result = target.apply(Some(b"never written"), None);
    assert!(!result.complete);
    assert!(result.changed);
    assert_eq!(result.created_directories.len(), 1);
    assert_eq!(result.created_directories[0].path, "created");
    assert!(temp.path().join("created").is_dir());
    // A deliberate retry observes the surviving directory as an existing
    // dependency rather than replaying its earlier creation.
    let retry = probe(&r, "created/retry.cs");
    let retry_plan = r.prepare_parents(&retry.path).unwrap();
    let retry = r
        .mutation_target_with_parents(&retry, &retry_plan, &Default::default())
        .unwrap()
        .apply(Some(b"recovered"), None);
    assert!(retry.complete, "{retry:?}");
    assert!(retry.created_directories.is_empty());
    assert_eq!(
        fs::read(temp.path().join("created/retry.cs")).unwrap(),
        b"recovered"
    );
    let input = probe(&r, "new/child");
    let plan = r.prepare_parents(&input.path).unwrap();
    let target = r
        .mutation_target_with_parents(&input, &plan, &Default::default())
        .unwrap();
    fs::write(temp.path().join("new"), b"human").unwrap();
    let result = target.apply(Some(b"never written"), None);
    assert!(!result.complete);
    assert!(!result.changed);
    assert_eq!(fs::read(temp.path().join("new")).unwrap(), b"human");
}

#[test]
fn native_staging_guards_creation_replacement_rename_and_delete() {
    let temp = tempfile::tempdir().unwrap();
    let r = root(temp.path());
    let target = r.mutation_target(&probe(&r, "space ü.txt")).unwrap();
    let created = target.apply(Some(b"before\r\n"), None);
    assert!(created.complete, "{created:?}");
    assert_eq!(
        fs::read(temp.path().join("space ü.txt")).unwrap(),
        b"before\r\n"
    );
    let before = probe(&r, "space ü.txt");
    let held = r.mutation_target(&before).unwrap();
    assert!(fs::write(temp.path().join("space ü.txt"), b"racing editor").is_err());
    assert!(fs::rename(temp.path().join("space ü.txt"), temp.path().join("stolen")).is_err());
    let changed = held.apply(Some(b"after\r\n"), None);
    assert!(changed.complete, "{changed:?}");
    let second = r
        .mutation_target(&probe(&r, "new.txt"))
        .unwrap()
        .apply(Some(b"created\n"), None);
    assert!(second.complete, "{second:?}");
    assert_eq!(fs::read(temp.path().join("new.txt")).unwrap(), b"created\n");
    fs::remove_file(temp.path().join("new.txt")).unwrap();
    assert_eq!(
        changed.observed.as_ref().unwrap().native_identity,
        before.observed.unwrap().native_identity
    );
    let renamed = r
        .mutation_target(&probe(&r, "space ü.txt"))
        .unwrap()
        .apply(Some(b"renamed\r\n"), Some("renamed.txt"));
    assert!(renamed.complete, "{renamed:?}");
    assert!(!temp.path().join("space ü.txt").exists());
    assert_eq!(
        fs::read(temp.path().join("renamed.txt")).unwrap(),
        b"renamed\r\n"
    );
    let case = r
        .mutation_target(&probe(&r, "renamed.txt"))
        .unwrap()
        .apply(Some(b"renamed\r\n"), Some("RENAMED.txt"));
    assert!(case.complete, "{case:?}");
    assert_eq!(
        fs::read_dir(temp.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .file_name(),
        "RENAMED.txt"
    );
    let deleted = r
        .mutation_target(&probe(&r, "renamed.txt"))
        .unwrap()
        .apply(None, None);
    assert!(deleted.complete, "{deleted:?}");
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    // Exercise every UTF-16 tail alignment; no accidental allocation padding
    // may become a terminator or part of the destination name.
    for length in 1..17 {
        let name = format!("{}.txt", "n".repeat(length));
        let created = r
            .mutation_target(&probe(&r, &name))
            .unwrap()
            .apply(Some(b"exact-name"), None);
        assert!(created.complete, "{created:?}");
        assert_eq!(fs::read(temp.path().join(&name)).unwrap(), b"exact-name");
        assert!(
            r.mutation_target(&probe(&r, &name))
                .unwrap()
                .apply(None, None)
                .complete
        );
    }
}
#[test]
fn stale_sources_new_destinations_and_hard_links_preserve_human_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let r = root(temp.path());
    fs::write(temp.path().join("source"), b"first").unwrap();
    let old = probe(&r, "source");
    fs::write(temp.path().join("source"), b"human").unwrap();
    assert!(r.mutation_target(&old).is_err());
    let missing = probe(&r, "new");
    let target = r.mutation_target(&missing).unwrap();
    fs::write(temp.path().join("new"), b"human-created").unwrap();
    let failed = target.apply(Some(b"candidate"), None);
    assert!(!failed.complete);
    assert!(!failed.changed);
    assert_eq!(fs::read(temp.path().join("new")).unwrap(), b"human-created");
    fs::hard_link(temp.path().join("source"), temp.path().join("alias")).unwrap();
    assert!(r.mutation_target(&probe(&r, "source")).is_err());
    assert_eq!(fs::read(temp.path().join("alias")).unwrap(), b"human");
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 3);
}

#[test]
fn native_parent_guards_long_paths_and_junction_rejection_keep_effects_contained() {
    let temp = tempfile::tempdir().unwrap();
    let inside = temp.path().join("inside");
    let outside = temp.path().join("outside");
    fs::create_dir(&inside).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("marker"), b"outside-human").unwrap();
    let r = root(&inside);
    fs::create_dir(inside.join("empty-parent")).unwrap();
    let missing = r
        .mutation_target(&probe(&r, "empty-parent/new.txt"))
        .unwrap();
    assert!(fs::rename(inside.join("empty-parent"), inside.join("moved-parent")).is_err());
    assert!(missing.apply(Some(b"guarded creation"), None).complete);
    let junction = inside.join("escape");
    let nested_probe = probe(&r, "escape/new/marker");
    let nested_plan = r.prepare_parents(&nested_probe.path).unwrap();
    let cmd = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .unwrap()
        .join("System32")
        .join("cmd.exe");
    let linked = std::process::Command::new(cmd)
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&junction)
        .arg(&outside)
        .output()
        .unwrap();
    assert!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let attempt = Probe {
        root: r.identity.root.clone(),
        binding: r.identity.binding,
        path: "escape/marker".into(),
        observed: None,
    };
    assert!(r.mutation_target(&attempt).is_err());
    assert!(r
        .mutation_target_with_parents(&nested_probe, &nested_plan, &Default::default())
        .is_err());
    assert!(!outside.join("new").exists());
    assert_eq!(fs::read(outside.join("marker")).unwrap(), b"outside-human");
    let relative = (0..7)
        .map(|i| format!("segment-{i}-{}", "x".repeat(35)))
        .collect::<Vec<_>>()
        .join("/");
    fs::create_dir_all(r.path().join(&relative)).unwrap();
    let path = format!("{relative}/file.txt");
    let target = r.mutation_target(&probe(&r, &path)).unwrap();
    assert!(fs::rename(
        inside.join(relative.split('/').next().unwrap()),
        inside.join("replaced")
    )
    .is_err());
    let observation = target.apply(Some(b"bounded-long-path"), None);
    assert!(observation.complete, "{observation:?}");
    assert_eq!(fs::read(r.path().join(path)).unwrap(), b"bounded-long-path");
}
