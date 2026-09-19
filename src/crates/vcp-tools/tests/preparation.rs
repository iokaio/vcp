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
        },
    )
    .unwrap();
    assert_eq!(read.proposed_result()["text"], "needle\nneedle\n");
    assert!(prepare_at(
        &r,
        Request::Read {
            path: "file".into(),
            max_bytes: 1
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
                max_bytes: 10
            }
        )
        .is_err());
    }
    assert_eq!(fs::read(temp.path().join("new.txt")).unwrap(), b"human");
}
