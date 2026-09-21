// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
use std::{fs, path::Path};
use vcp_domain::{workspace::Scope, *};
use vcp_repository::{Root, RootIdentity};
use vcp_tools::*;
fn root(path: &Path) -> Root {
    Root::open(
        RootIdentity {
            workspace: WorkspaceId::parse("workspace").unwrap(),
            root: RootId::parse("root").unwrap(),
            repository: "synthetic".into(),
            worktree: "synthetic".into(),
            binding: Revision::ZERO,
        },
        path,
    )
    .unwrap()
}
fn identity() -> Identity {
    Identity {
        scope: Scope {
            workspace: WorkspaceId::parse("workspace").unwrap(),
            session: SessionId::new(),
            task: TaskId::new(),
        },
        actor: ActorId::new(),
        host: HostId::new(),
        binding: Revision::ZERO,
        authority: AuthorityRevision::ZERO,
        steering: SteeringRevision::ZERO,
        policy: PolicyRevision::ZERO,
    }
}
#[test]
fn verification_discovers_real_targets_and_rejects_shell_hooks_and_wrong_projects() {
    use vcp_tools::verification::*;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().canonicalize().unwrap();
    let root = root(&path);
    fs::create_dir(path.join("project")).unwrap();
    fs::write(
        path.join("project/acceptance.cjs"),
        b"// synthetic target\n",
    )
    .unwrap();
    let requirement = Requirement {
        timeout_ms: None,
        manifest: "project/package.json".into(),
        runner: Runner::Node,
        profile: "node".into(),
        expected_tests: vec!["acceptance".into()],
        rationale: "explicit expected project target".into(),
    };
    let observe = || {
        let scan = root
            .discover(&vcp_repository::discovery::Limits::default())
            .unwrap();
        let manifest = vcp_repository::observation::Manifest {
            version: 1,
            identity: root.identity.clone(),
            git: None,
            files: scan.sources.iter().map(|s| s.version.clone()).collect(),
            ignore_dependencies: scan.ignore_dependencies,
            exclusions: scan.exclusions,
            bounded_scan_complete: scan.complete,
        };
        vcp_repository::observation::Observation {
            digest: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&manifest).unwrap()),
            manifest,
            sources: scan.sources,
            git: None,
        }
    };
    for (script, valid) in [
        ("node --test acceptance.cjs", true),
        ("node --test", false),
        ("node --test missing.cjs", false),
        ("node --test ../acceptance.cjs", false),
        ("node --test acceptance.cjs && echo success", false),
        ("node --test acceptance.cjs acceptance.cjs", false),
        ("node --test *.cjs", false),
    ] {
        fs::write(
            path.join("project/package.json"),
            serde_json::to_vec(&serde_json::json!({"scripts":{"test":script}})).unwrap(),
        )
        .unwrap();
        let plans = discover(&observe(), std::slice::from_ref(&requirement)).unwrap();
        assert_eq!(plans[0].not_run.is_none(), valid, "{script}");
        assert_eq!(plans[0].request.directory, "project");
    }
    fs::write(
        path.join("project/package.json"),
        br#"{"scripts":{"test":"node --test acceptance.cjs","pretest":"echo setup"}}"#,
    )
    .unwrap();
    assert!(
        discover(&observe(), std::slice::from_ref(&requirement)).unwrap()[0]
            .not_run
            .as_ref()
            .unwrap()
            .contains("hooks")
    );
    fs::remove_file(path.join("project/package.json")).unwrap();
    assert!(
        discover(&observe(), std::slice::from_ref(&requirement)).unwrap()[0]
            .not_run
            .is_some()
    );
    let mut incomplete = observe();
    incomplete.manifest.bounded_scan_complete = false;
    assert!(
        discover(&incomplete, std::slice::from_ref(&requirement)).unwrap()[0]
            .not_run
            .as_ref()
            .unwrap()
            .contains("incomplete")
    );
    fs::write(
        path.join("project/Cargo.toml"),
        b"[package]\nname='synthetic'\nversion='0.1.0'\n",
    )
    .unwrap();
    let cargo = Requirement {
        timeout_ms: None,
        manifest: "project/Cargo.toml".into(),
        runner: Runner::Cargo,
        profile: "cargo".into(),
        ..requirement
    };
    let plans = discover(&observe(), &[cargo]).unwrap();
    assert!(plans[0].not_run.is_none());
    assert_eq!(
        &plans[0].request.arguments[..5],
        [
            "test",
            "--locked",
            "--offline",
            "--manifest-path",
            "Cargo.toml"
        ]
    );
}
fn prepare_at(root: &Root, request: Request) -> vcp_tools::Result<Prepared> {
    prepare(
        root.clone(),
        identity(),
        request,
        ByteCount::new(1024 * 1024),
    )
}
#[test]
fn multi_file_preparation_preserves_crlf_and_utf16_without_effects() {
    let temp = tempfile::tempdir().unwrap();
    let r = root(temp.path());
    fs::write(temp.path().join("one.txt"), b"before\r\nkeep\r\n").unwrap();
    let mut utf16 = vec![0xff, 0xfe];
    for c in "old\r\n".encode_utf16() {
        utf16.extend(c.to_le_bytes());
    }
    fs::write(temp.path().join("two.txt"), &utf16).unwrap();
    let text="*** Begin Patch\n*** Update File: one.txt\n@@\n-before\n+after\n keep\n*** Update File: two.txt\n@@\n-old\n+new\n*** Add File: three.txt\n+created\n*** End Patch";
    let p = prepare_at(&r, Request::Patch { patch: text.into() }).unwrap();
    assert_eq!(p.changes().len(), 3);
    assert_eq!(p.changes()[0].after.as_ref().unwrap(), b"after\r\nkeep\r\n");
    let mut expected = vec![0xff, 0xfe];
    for c in "new\r\n".encode_utf16() {
        expected.extend(c.to_le_bytes());
    }
    assert_eq!(p.changes()[1].after.as_ref().unwrap(), &expected);
    assert_eq!(
        fs::read(temp.path().join("one.txt")).unwrap(),
        b"before\r\nkeep\r\n"
    );
    assert_eq!(fs::read(temp.path().join("two.txt")).unwrap(), utf16);
    assert!(!temp.path().join("three.txt").exists());
    p.revalidate().unwrap();
    fs::write(temp.path().join("one.txt"), b"human").unwrap();
    assert!(p.revalidate().is_err());
}
#[test]
fn ambiguous_fuzzy_overlapping_and_traversal_patches_leave_all_bytes_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let r = root(temp.path());
    fs::write(temp.path().join("file"), b"same\nsame\n").unwrap();
    for text in [
        "*** Begin Patch\n*** Update File: file\n@@\n-same\n+changed\n*** End Patch",
        "*** Begin Patch\n*** Update File: file\n@@\n- same\n+changed\n*** End Patch",
        "*** Begin Patch\n*** Add File: ../escape\n+bad\n*** End Patch",
        "*** Begin Patch\n*** Delete File: file\n*** Add File: FILE\n+bad\n*** End Patch",
    ] {
        assert!(prepare_at(&r, Request::Patch { patch: text.into() }).is_err());
        assert_eq!(fs::read(temp.path().join("file")).unwrap(), b"same\nsame\n");
    }
}
#[test]
fn reads_lists_and_search_have_visible_bounds_and_source_fences() {
    let temp = tempfile::tempdir().unwrap();
    let r = root(temp.path());
    fs::write(temp.path().join("file"), b"needle\nneedle\n").unwrap();
    let read = prepare_at(
        &r,
        Request::Read {
            path: "file".into(),
            max_bytes: 100,
            start_line: None,
            end_line: None,
        },
    )
    .unwrap();
    assert_eq!(read.proposed_result()["text"], "needle\nneedle\n");
    assert!(prepare_at(
        &r,
        Request::Read {
            path: "file".into(),
            max_bytes: 1,
            start_line: None,
            end_line: None,
        }
    )
    .is_err());
    let list = prepare_at(
        &r,
        Request::List {
            path: "".into(),
            max_entries: 1,
        },
    )
    .unwrap();
    list.revalidate().unwrap();
    let search = prepare_at(
        &r,
        Request::Search {
            query: "needle".into(),
            max_hits: 1,
            mode: None,
            path_pattern: None,
            max_files: None,
            max_scan_bytes: None,
        },
    )
    .unwrap();
    assert_eq!(
        search.proposed_result()["matches"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(search.proposed_result()["complete"], false);
    fs::write(temp.path().join("new"), b"human").unwrap();
    assert!(list.revalidate().is_err());
    fs::write(temp.path().join("file"), b"changed").unwrap();
    assert!(read.revalidate().is_err());
    assert!(search.revalidate().is_err());
}
#[test]
fn absent_destination_is_bound_and_git_metadata_and_devices_are_rejected() {
    assert!(Request::from_call(
        "vcp_read",
        r#"{"path":"file","max_bytes":10,"tool":"patch"}"#
    )
    .is_err());
    assert!(
        Request::from_call("vcp_read", r#"{"path":"file","max_bytes":10,"extra":true}"#).is_err()
    );
    assert!(Request::from_call("foreign", r#"{}"#).is_err());
    assert!(Request::from_call("vcp_read", r#"{"path":"file","max_bytes":10}"#).is_ok());
    let temp = tempfile::tempdir().unwrap();
    let r = root(temp.path());
    let p = prepare_at(
        &r,
        Request::Patch {
            patch: "*** Begin Patch\n*** Add File: new.txt\n+new\n*** End Patch".into(),
        },
    )
    .unwrap();
    fs::write(temp.path().join("new.txt"), b"human").unwrap();
    assert!(p.revalidate().is_err());
    for path in ["NUL.txt", "sub/COM1", ".git/config", "../other"] {
        assert!(prepare_at(
            &r,
            Request::Read {
                path: path.into(),
                max_bytes: 10,
                start_line: None,
                end_line: None,
            }
        )
        .is_err());
    }
    assert_eq!(fs::read(temp.path().join("new.txt")).unwrap(), b"human");
}

fn navigation(
    root: &Root,
    name: &str,
    arguments: serde_json::Value,
) -> vcp_tools::Result<Prepared> {
    prepare(
        root.clone(),
        identity(),
        Request::from_call(name, &arguments.to_string())?,
        ByteCount::new(8 * 1024 * 1024),
    )
}

#[test]
fn regex_search_matches_independent_full_scan_and_preserves_literal_default() {
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("outside.rs"), "alpha\n").unwrap();
    fs::create_dir(temp.path().join("src")).unwrap();
    fs::write(temp.path().join(".gitignore"), "ignored.rs\n").unwrap();
    fs::write(temp.path().join("ignored.rs"), "alpha\n").unwrap();
    let fixtures = [
        ("src/a.rs", "alpha\r\nbeta\r\nαβ\r\nx alpha\r\n"),
        ("src/b.rs", "beta\nalpha|beta\n"),
        ("notes.txt", "alpha\n"),
    ];
    for (path, content) in fixtures {
        fs::write(temp.path().join(path), content).unwrap();
    }
    let root = root(temp.path());
    for (query, case) in [("^(alpha|beta)$", 0), (r"^\p{Greek}+$", 1), ("absent", 2)] {
        let prepared = navigation(&root, "vcp_search", serde_json::json!({"query":query,"max_hits":100,"mode":"regex","path_pattern":"^src/.*\\.rs$"})).unwrap();
        let expected: Vec<_> = fixtures
            .iter()
            .filter(|(path, _)| path.starts_with("src/"))
            .flat_map(|(path, content)| {
                content
                    .lines()
                    .enumerate()
                    .filter(move |(_, line)| match case {
                        0 => *line == "alpha" || *line == "beta",
                        1 => *line == "αβ",
                        _ => false,
                    })
                    .map(move |(i, line)| (path.to_string(), i + 1, line.to_string()))
            })
            .collect();
        let actual: Vec<_> = prepared.proposed_result()["matches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|hit| {
                (
                    hit["path"].as_str().unwrap().to_owned(),
                    hit["line"].as_u64().unwrap() as usize,
                    hit["text"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        assert_eq!(actual, expected);
        assert_eq!(prepared.proposed_result()["complete"], true);
        prepared.revalidate().unwrap();
    }
    let literal = navigation(
        &root,
        "vcp_search",
        serde_json::json!({"query":"alpha|beta","max_hits":100}),
    )
    .unwrap();
    assert_eq!(
        literal.proposed_result()["matches"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(literal.proposed_result()["exclusions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["path"] == "ignored.rs"));
}

#[test]
fn search_invalid_patterns_limits_and_cancellation_are_explicit() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("a.txt"), "hit\nhit\nhit\n").unwrap();
    fs::write(temp.path().join("b.txt"), "hit\n").unwrap();
    let root = root(temp.path());
    for extra in [
        serde_json::json!({"mode":"regex","query":"["}),
        serde_json::json!({"path_pattern":"["}),
        serde_json::json!({"max_scan_bytes":0}),
        serde_json::json!({"max_files":10001}),
    ] {
        let mut args = serde_json::json!({"query":"hit","max_hits":10});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert!(navigation(&root, "vcp_search", args).is_err());
    }
    for extra in [
        serde_json::json!({"max_hits":1}),
        serde_json::json!({"max_scan_bytes":1}),
        serde_json::json!({"max_files":1}),
    ] {
        let mut args = serde_json::json!({"query":"hit","max_hits":10});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let prepared = navigation(&root, "vcp_search", args).unwrap();
        assert_eq!(prepared.proposed_result()["complete"], false);
    }
    let polls = std::cell::Cell::new(0);
    let cancelled = || {
        polls.set(polls.get() + 1);
        polls.get() > 4
    };
    assert!(prepare_cancellable(
        root,
        identity(),
        Request::from_call("vcp_search", r#"{"query":"hit","max_hits":10}"#).unwrap(),
        ByteCount::new(1024 * 1024),
        &cancelled
    )
    .is_err());
    assert!(polls.get() > 4);
}

#[test]
fn ranged_reads_preserve_crlf_unicode_versions_and_explicit_continuations() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("file.txt"), "first\r\nαβ\r\nlast").unwrap();
    let root = root(temp.path());
    let first = navigation(
        &root,
        "vcp_read",
        serde_json::json!({"path":"file.txt","max_bytes":7,"start_line":1,"end_line":3}),
    )
    .unwrap();
    assert_eq!(first.proposed_result()["text"], "first\r\n");
    assert_eq!(first.proposed_result()["next_line"], 2);
    assert_eq!(first.proposed_result()["complete"], false);
    let middle = navigation(
        &root,
        "vcp_read",
        serde_json::json!({"path":"file.txt","max_bytes":100,"start_line":2,"end_line":2}),
    )
    .unwrap();
    assert_eq!(middle.proposed_result()["text"], "αβ\r\n");
    assert_eq!(
        middle.proposed_result()["returned_range"],
        serde_json::json!({"start_line":2,"end_line":2})
    );
    assert_eq!(
        first.proposed_result()["version"],
        middle.proposed_result()["version"]
    );
    let last = navigation(
        &root,
        "vcp_read",
        serde_json::json!({"path":"file.txt","max_bytes":100,"start_line":3}),
    )
    .unwrap();
    assert_eq!(last.proposed_result()["text"], "last");
    assert!(last.proposed_result()["next_line"].is_null());
    assert_eq!(last.proposed_result()["complete"], false);
    for bounds in [
        serde_json::json!({"start_line":0}),
        serde_json::json!({"start_line":4}),
        serde_json::json!({"start_line":3,"end_line":2}),
        serde_json::json!({"start_line":2,"max_bytes":1}),
    ] {
        let mut args = serde_json::json!({"path":"file.txt","max_bytes":100});
        args.as_object_mut()
            .unwrap()
            .extend(bounds.as_object().unwrap().clone());
        assert!(navigation(&root, "vcp_read", args).is_err());
    }
    fs::write(temp.path().join("file.txt"), "first\r\nchanged\r\nlast").unwrap();
    assert!(first.revalidate().is_err());
    let changed = navigation(
        &root,
        "vcp_read",
        serde_json::json!({"path":"file.txt","max_bytes":100,"start_line":2}),
    )
    .unwrap();
    assert_ne!(
        first.proposed_result()["version"],
        changed.proposed_result()["version"]
    );
}

#[test]
fn ranged_reads_do_not_relax_legacy_size_encoding_or_scope_errors() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("large.txt"), "short\n".repeat(200_000)).unwrap();
    fs::write(temp.path().join("binary.txt"), [0xff]).unwrap();
    fs::write(temp.path().join("empty.txt"), "").unwrap();
    let root = root(temp.path());
    assert!(navigation(
        &root,
        "vcp_read",
        serde_json::json!({"path":"large.txt","max_bytes":1048576})
    )
    .is_err());
    let range = navigation(
        &root,
        "vcp_read",
        serde_json::json!({"path":"large.txt","max_bytes":6,"start_line":200000}),
    )
    .unwrap();
    assert_eq!(range.proposed_result()["text"], "short\n");
    for path in ["binary.txt", "../outside.txt"] {
        for bounds in [serde_json::json!({}), serde_json::json!({"start_line":1})] {
            let mut args = serde_json::json!({"path":path,"max_bytes":100});
            args.as_object_mut()
                .unwrap()
                .extend(bounds.as_object().unwrap().clone());
            assert!(navigation(&root, "vcp_read", args).is_err());
        }
    }
    let empty = navigation(
        &root,
        "vcp_read",
        serde_json::json!({"path":"empty.txt","max_bytes":100,"start_line":1}),
    )
    .unwrap();
    assert_eq!(empty.proposed_result()["complete"], true);
    assert!(empty.proposed_result()["returned_range"].is_null());
}

#[test]
fn search_revalidation_observes_cancellation_after_entry_without_releasing_stale_results() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("source.txt"), "hit\n".repeat(100)).unwrap();
    let root = root(temp.path());
    let prepared = navigation(
        &root,
        "vcp_search",
        serde_json::json!({"query":"hit","max_hits":100}),
    )
    .unwrap();
    let polls = std::cell::Cell::new(0);
    let cancelled = || {
        polls.set(polls.get() + 1);
        polls.get() > 8
    };
    let error = prepared.revalidate_cancellable(&cancelled).unwrap_err();
    assert!(error.to_string().contains("cancelled"), "{error}");
    assert!(polls.get() > 8);
    prepared.revalidate_cancellable(&|| false).unwrap();
    fs::write(temp.path().join("source.txt"), "changed\n").unwrap();
    assert!(prepared.revalidate_cancellable(&|| false).is_err());
}

#[test]
fn path_filters_apply_before_byte_capture_and_revalidate_ignore_changes() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("a-large.txt"), "irrelevant".repeat(1000)).unwrap();
    fs::write(temp.path().join("wanted.rs"), "hit\n").unwrap();
    let root = root(temp.path());
    let prepared = navigation(&root, "vcp_search", serde_json::json!({"query":"^hit$","mode":"regex","path_pattern":"^wanted[.]rs$","max_hits":1,"max_scan_bytes":4})).unwrap();
    assert_eq!(prepared.proposed_result()["complete"], true);
    assert_eq!(
        prepared.proposed_result()["matches"][0]["path"],
        "wanted.rs"
    );
    assert!(prepared.proposed_result()["exclusions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["path"] == "a-large.txt" && item["reason"] == "path_filter"));
    prepared.revalidate().unwrap();
    fs::write(temp.path().join(".gitignore"), "wanted.rs\n").unwrap();
    assert!(prepared.revalidate().is_err());
    assert!(navigation(
        &root,
        "vcp_search",
        serde_json::json!({"query":"a".repeat(4097),"max_hits":1})
    )
    .is_err());
    assert!(navigation(
        &root,
        "vcp_search",
        serde_json::json!({"query":r"(?:\w{1000}){1000}","mode":"regex","max_hits":1})
    )
    .is_err());
}
