// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use vcp_domain::{
    agents::{ChildMode, ChildPath, ChildSpec},
    policy::EffectClass,
    *,
};
use vcp_repository::{
    dirty_snapshot::{CapturePolicy, WorkspaceSnapshot},
    merge::{self, ChildPacket, ConflictKind, Inputs},
    worktree::{RegisteredRoot, Snapshotter, WorkspaceRegistration},
    *,
};

fn executable() -> PathBuf {
    std::env::var_os("VCP_TEST_GIT")
        .map(PathBuf::from)
        .expect("native Git fixture")
}
fn git(path: &Path, args: &[&str]) {
    let output = Command::new(executable())
        .current_dir(path)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn root(path: &Path, name: &str) -> Root {
    Root::open(
        RootIdentity {
            workspace: WorkspaceId::parse("integration-fixture").unwrap(),
            root: RootId::new(),
            repository: "fixture".into(),
            worktree: name.into(),
            binding: Revision::ZERO,
        },
        path,
    )
    .unwrap()
}
struct Fixture {
    _directory: tempfile::TempDir,
    source: Root,
    child: Root,
    service: Snapshotter,
    base: WorkspaceSnapshot,
    registration: WorkspaceRegistration,
    assignment: ChildSpec,
}
impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let source_path = directory.path().join("source");
        let children = directory.path().join("children");
        fs::create_dir(&source_path).unwrap();
        fs::create_dir(&children).unwrap();
        git(&source_path, &["init", "-q"]);
        git(&source_path, &["config", "user.name", "Fixture"]);
        git(
            &source_path,
            &["config", "user.email", "fixture@example.invalid"],
        );
        git(&source_path, &["config", "core.autocrlf", "false"]);
        fs::write(
            source_path.join("file.txt"),
            b"staged\ntwo\nthree\nfour\nfive\n",
        )
        .unwrap();
        git(&source_path, &["add", "."]);
        git(&source_path, &["commit", "-qm", "base"]);
        fs::write(
            source_path.join("file.txt"),
            b"one\ntwo\nthree\nfour\nfive\n",
        )
        .unwrap();
        let source = root(&source_path, "source");
        let disposable = root(&children, "children");
        let environment: BTreeMap<OsString, OsString> =
            ["SystemRoot", "WINDIR", "PATH", "TEMP", "TMP"]
                .into_iter()
                .filter_map(|name| std::env::var_os(name).map(|value| (name.into(), value)))
                .collect();
        let service = Snapshotter::new(
            executable(),
            environment,
            Duration::from_secs(20),
            8 * 1024 * 1024,
        )
        .unwrap();
        let base = service
            .capture(&source, &CapturePolicy::default())
            .await
            .unwrap();
        let registration = WorkspaceRegistration {
            owner: TaskId::new(),
            source: source.identity.clone(),
            child: RootIdentity {
                root: RootId::new(),
                worktree: "child".into(),
                ..source.identity.clone()
            },
            absolute_path: disposable.path().join("child"),
            snapshot: base.fingerprint.clone(),
            git_metadata_owner: Some(RegisteredRoot {
                identity: source.identity.clone(),
                absolute_path: source.path().into(),
            }),
        };
        let child = service
            .materialize(&source, &disposable, &base, &registration)
            .await
            .unwrap()
            .root;
        let assignment = ChildSpec {
            parent: TaskId::new(),
            actor: ActorId::new(),
            parent_steering: SteeringRevision::ZERO,
            dependencies: BTreeSet::new(),
            mode: ChildMode::IsolatedWrite,
            role: "bounded edit".into(),
            model_policy: "baseline".into(),
            paths: vec![ChildPath {
                root: source.identity.root.clone(),
                path: "file.txt".into(),
                write: true,
            }],
            effects: [EffectClass::Read, EffectClass::Write].into(),
            authority: AuthorityRevision::ZERO,
            policy: PolicyRevision::ZERO,
            binding: Revision::ZERO,
            grants: BTreeMap::new(),
            allocation: Micros::new(1000),
            deadline: Timestamp::new(1000),
            snapshot: ArtifactId::new(),
            snapshot_digest: vcp_protocol::digest_bytes(
                &vcp_protocol::canonical_bytes(&base).unwrap(),
            ),
            registration: Some(ArtifactId::new()),
            registration_digest: Some(vcp_protocol::digest_bytes(
                &vcp_protocol::canonical_bytes(&registration).unwrap(),
            )),
            isolated_root: Some(child.identity.root.clone()),
        };
        Self {
            _directory: directory,
            source,
            child,
            service,
            base,
            registration,
            assignment,
        }
    }
    fn inputs(&self) -> Inputs<'_> {
        Inputs {
            parent: &self.source,
            child: &self.child,
            metadata_owner: &self.source,
            parent_registration: None,
            registration: &self.registration,
            base: &self.base,
            assignment: &self.assignment,
        }
    }
    async fn packet(&self, paths: &[&str]) -> ChildPacket {
        let result = merge::observe_result(&self.service, &self.inputs())
            .await
            .unwrap();
        ChildPacket {
            base_fingerprint: self.base.fingerprint.clone(),
            result_fingerprint: result.fingerprint,
            changed_paths: paths.iter().map(|name| (*name).into()).collect(),
            findings: vec!["Useful review finding remains unverified.".into()],
        }
    }
}

#[tokio::test]
async fn actual_three_way_plan_preserves_parent_index_and_has_fresh_native_preconditions() {
    let fixture = Fixture::new().await;
    fs::write(
        fixture.child.path().join("file.txt"),
        b"CHILD\ntwo\nthree\nfour\nfive\n",
    )
    .unwrap();
    fs::write(
        fixture.source.path().join("file.txt"),
        b"one\ntwo\nthree\nfour\nPARENT\n",
    )
    .unwrap();
    let index = fs::read(fixture.source.path().join(".git/index")).unwrap();
    let packet = fixture.packet(&["file.txt"]).await;
    let plan = merge::prepare(&fixture.service, fixture.inputs(), &packet)
        .await
        .unwrap();
    assert!(plan.ready(), "{:?}", plan.rejection);
    assert_eq!(plan.changes.len(), 1);
    assert_eq!(
        plan.changes[0].after.as_deref(),
        Some(b"CHILD\ntwo\nthree\nfour\nPARENT\n".as_slice())
    );
    assert_eq!(
        fs::read(fixture.source.path().join("file.txt")).unwrap(),
        b"one\ntwo\nthree\nfour\nPARENT\n"
    );
    assert_eq!(
        fs::read(fixture.source.path().join(".git/index")).unwrap(),
        index
    );
    assert_eq!(
        plan.parent_index.as_ref().unwrap().sha256,
        vcp_protocol::digest_bytes(&index)
    );
    fs::write(
        fixture.source.path().join("unrelated.rs"),
        b"human adds another source file\n",
    )
    .unwrap();
    assert!(merge::revalidate_parent(
        &fixture.source,
        plan.parent_manifest.as_ref().unwrap(),
        &plan.parent_probes
    )
    .is_err());
    fs::remove_file(fixture.source.path().join("unrelated.rs")).unwrap();
    merge::revalidate_parent(
        &fixture.source,
        plan.parent_manifest.as_ref().unwrap(),
        &plan.parent_probes,
    )
    .unwrap();
    fs::write(
        fixture.source.path().join("file.txt"),
        b"human typing after preview\n",
    )
    .unwrap();
    assert!(instructions::revalidate_probes(
        &[plan.changes[0].parent.clone()],
        &[fixture.source.clone()]
    )
    .is_err());
    git(fixture.source.path(), &["add", "file.txt"]);
    assert!(fixture
        .source
        .revalidate(plan.parent_index.as_ref().unwrap())
        .is_err());
}

#[tokio::test]
async fn malformed_stale_and_out_of_scope_packets_retain_findings_without_proposed_writes() {
    let fixture = Fixture::new().await;
    fs::write(fixture.child.path().join("file.txt"), b"child\n").unwrap();
    let packet = fixture.packet(&["file.txt"]).await;
    let mut stale = packet.clone();
    stale.base_fingerprint = "0".repeat(64);
    let plan = merge::prepare(&fixture.service, fixture.inputs(), &stale)
        .await
        .unwrap();
    assert!(!plan.ready());
    assert!(plan.rejection.is_some());
    assert!(plan.changes.is_empty());
    assert_eq!(plan.findings, packet.findings);
    let mut malformed = packet.clone();
    malformed.changed_paths.clear();
    assert!(
        merge::prepare(&fixture.service, fixture.inputs(), &malformed)
            .await
            .unwrap()
            .rejection
            .is_some()
    );
    fs::write(
        fixture.child.path().join("outside.txt"),
        b"unauthorized change",
    )
    .unwrap();
    let outside = fixture.packet(&["file.txt", "outside.txt"]).await;
    let plan = merge::prepare(&fixture.service, fixture.inputs(), &outside)
        .await
        .unwrap();
    assert!(plan.rejection.as_deref().unwrap().contains("write scope"));
    assert!(plan.changes.is_empty());
    fs::write(
        fixture.child.path().join(".env"),
        b"synthetic sensitive input",
    )
    .unwrap();
    let plan = merge::prepare(&fixture.service, fixture.inputs(), &outside)
        .await
        .unwrap();
    assert!(plan.rejection.as_deref().unwrap().contains("sensitive"));
    assert_eq!(plan.findings, packet.findings);
}

#[tokio::test]
async fn content_and_delete_conflicts_leave_actual_parent_bytes_untouched() {
    let fixture = Fixture::new().await;
    fs::write(fixture.child.path().join("file.txt"), b"child\n").unwrap();
    fs::write(fixture.source.path().join("file.txt"), b"human\n").unwrap();
    let packet = fixture.packet(&["file.txt"]).await;
    let plan = merge::prepare(&fixture.service, fixture.inputs(), &packet)
        .await
        .unwrap();
    assert!(!plan.ready());
    assert_eq!(plan.conflicts[0].kind, ConflictKind::ConcurrentContent);
    fs::remove_file(fixture.child.path().join("file.txt")).unwrap();
    let packet = fixture.packet(&["file.txt"]).await;
    let plan = merge::prepare(&fixture.service, fixture.inputs(), &packet)
        .await
        .unwrap();
    assert_eq!(plan.conflicts[0].kind, ConflictKind::AddDelete);
    assert_eq!(
        fs::read(fixture.source.path().join("file.txt")).unwrap(),
        b"human\n"
    );
}

#[tokio::test]
async fn read_only_findings_are_retained_but_observed_writes_are_rejected() {
    let mut fixture = Fixture::new().await;
    fixture.assignment.mode = ChildMode::ReadOnly;
    fixture.assignment.paths[0].write = false;
    fixture.assignment.effects = [EffectClass::Read].into();
    let packet = fixture.packet(&[]).await;
    let plan = merge::prepare(&fixture.service, fixture.inputs(), &packet)
        .await
        .unwrap();
    assert!(plan.ready());
    assert!(plan.changes.is_empty());
    assert_eq!(plan.findings, packet.findings);
    fs::write(fixture.child.path().join("file.txt"), b"unrequested write").unwrap();
    let packet = fixture.packet(&["file.txt"]).await;
    let plan = merge::prepare(&fixture.service, fixture.inputs(), &packet)
        .await
        .unwrap();
    assert!(plan.rejection.is_some());
    assert!(plan.changes.is_empty());
    assert_eq!(plan.findings, packet.findings);
}
