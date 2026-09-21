// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
use std::{fs, path::Path};
use vcp_domain::{workspace::Scope, *};
use vcp_repository::{
    instructions::Probe,
    merge::{IntegrationPlan, ProposedChange},
    observation::Manifest,
    Root, RootIdentity,
};
use vcp_tools::{Identity, Request};

fn root(path: &Path) -> Root {
    Root::open(
        RootIdentity {
            workspace: WorkspaceId::parse("integration-tools").unwrap(),
            root: RootId::new(),
            repository: "fixture".into(),
            worktree: "fixture".into(),
            binding: Revision::ZERO,
        },
        path,
    )
    .unwrap()
}
fn identity(root: &Root) -> Identity {
    Identity {
        scope: Scope {
            workspace: root.identity.workspace.clone(),
            session: SessionId::new(),
            task: TaskId::new(),
        },
        actor: ActorId::new(),
        host: HostId::new(),
        binding: root.identity.binding,
        authority: AuthorityRevision::ZERO,
        steering: SteeringRevision::ZERO,
        policy: PolicyRevision::ZERO,
    }
}
fn setup() -> (tempfile::TempDir, Root, IntegrationPlan) {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join(".git")).unwrap();
    fs::write(
        directory.path().join(".git/index"),
        b"synthetic index before",
    )
    .unwrap();
    fs::write(directory.path().join("file.bin"), [0, 128, 13, 10, 255]).unwrap();
    let root = root(directory.path());
    let observed = root.read(Path::new("file.bin"), 1024).unwrap();
    let probe = Probe {
        root: root.identity.root.clone(),
        binding: root.identity.binding,
        path: "file.bin".into(),
        observed: Some(observed.version),
    };
    let scan = root
        .discover(&vcp_repository::discovery::Limits::default())
        .unwrap();
    let manifest = Manifest {
        version: 1,
        identity: root.identity.clone(),
        git: None,
        files: scan
            .sources
            .iter()
            .map(|source| source.version.clone())
            .collect(),
        ignore_dependencies: scan.ignore_dependencies,
        exclusions: scan.exclusions,
        bounded_scan_complete: scan.complete,
    };
    let plan = IntegrationPlan {
        base_fingerprint: "a".repeat(64),
        child_fingerprint: Some("b".repeat(64)),
        parent_fingerprint: Some(vcp_protocol::digest_bytes(
            &vcp_protocol::canonical_bytes(&manifest).unwrap(),
        )),
        parent_index: Some(root.version(Path::new(".git/index"), 1024).unwrap()),
        parent_manifest: Some(manifest),
        parent_probes: vec![probe.clone()],
        changes: vec![ProposedChange {
            path: "file.bin".into(),
            parent: probe,
            before: Some(observed.bytes),
            after: Some(vec![0, 250, 13, 10, 255]),
        }],
        conflicts: vec![],
        rejection: None,
        findings: vec![],
    };
    (directory, root, plan)
}

#[test]
fn owner_preparation_keeps_binary_bytes_and_pins_index_through_native_effect() {
    let (_directory, root, plan) = setup();
    let prepared = vcp_tools::integration::prepare(
        root.clone(),
        root.clone(),
        identity(&root),
        TaskId::new(),
        plan,
    )
    .unwrap();
    assert_eq!(
        fs::read(root.path().join("file.bin")).unwrap(),
        [0, 128, 13, 10, 255]
    );
    assert_eq!(prepared.changes()[0].after, Some(vec![0, 250, 13, 10, 255]));
    let index = prepared.hold_index().unwrap();
    assert!(fs::write(root.path().join(".git/index"), b"concurrent staging").is_err());
    let change = &prepared.changes()[0];
    let target = prepared.root().mutation_target(&change.probes[0]).unwrap();
    let outcome = target.apply(change.after.as_deref(), None);
    assert!(outcome.complete);
    assert_eq!(
        fs::read(root.path().join("file.bin")).unwrap(),
        [0, 250, 13, 10, 255]
    );
    assert_eq!(
        fs::read(root.path().join(".git/index")).unwrap(),
        b"synthetic index before"
    );
    drop(index);
    fs::write(root.path().join(".git/index"), b"later staging").unwrap();
    assert!(prepared.hold_index().is_err());
    assert!(Request::from_call("vcp_patch", r#"{"kind":"child_integration"}"#).is_err());
}

#[test]
fn human_source_index_and_new_file_changes_invalidate_owner_integration_preview() {
    let (_directory, root, plan) = setup();
    let prepared = vcp_tools::integration::prepare(
        root.clone(),
        root.clone(),
        identity(&root),
        TaskId::new(),
        plan.clone(),
    )
    .unwrap();
    fs::write(root.path().join("new-source.rs"), b"human source\n").unwrap();
    assert!(prepared.revalidate().is_err());
    fs::remove_file(root.path().join("new-source.rs")).unwrap();
    prepared.revalidate().unwrap();
    fs::write(root.path().join("file.bin"), b"human edit").unwrap();
    assert!(prepared.revalidate().is_err());
    assert!(vcp_tools::integration::prepare(
        root.clone(),
        root.clone(),
        identity(&root),
        TaskId::new(),
        plan
    )
    .is_err());
    assert_eq!(
        fs::read(root.path().join("file.bin")).unwrap(),
        b"human edit"
    );
    let (_other_directory, other_root, other_plan) = setup();
    fs::write(other_root.path().join(".git/index"), b"new index").unwrap();
    assert!(vcp_tools::integration::prepare(
        other_root.clone(),
        other_root.clone(),
        identity(&other_root),
        TaskId::new(),
        other_plan
    )
    .is_err());
}
