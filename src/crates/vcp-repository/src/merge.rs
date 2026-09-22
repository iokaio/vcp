// SPDX-License-Identifier: Apache-2.0
//! Read-only child result validation and three-way integration preparation.
//! Proposed bytes are not authority. The parent broker must revalidate every
//! probe and the index precondition, apply through its ordinary effect path,
//! and verify the resulting parent state before accepting completion.
pub use crate::review_findings::ReviewFinding;
use crate::{
    dirty_snapshot::{CapturePolicy, SnapshotFile, WorkspaceSnapshot},
    instructions::Probe,
    worktree::{Snapshotter, WorkspaceRegistration},
    *,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use vcp_domain::agents::{ChildMode, ChildSpec};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildPacket {
    pub base_fingerprint: String,
    pub result_fingerprint: String,
    pub changed_paths: BTreeSet<String>,
    /// Untrusted findings remain separately inspectable when the patch fails.
    pub findings: Vec<ReviewFinding>,
}
impl ChildPacket {
    pub fn validate_findings(&self) -> Result<()> {
        if self.findings.len() > 64 || self.changed_paths.len() > 128 {
            return Err(Error::Limit("child result packet"));
        }
        for finding in &self.findings {
            finding.validate()?;
        }
        Ok(())
    }
}
pub struct Inputs<'a> {
    pub parent: &'a Root,
    pub child: &'a Root,
    pub metadata_owner: &'a Root,
    pub parent_registration: Option<&'a WorkspaceRegistration>,
    pub registration: &'a WorkspaceRegistration,
    pub base: &'a WorkspaceSnapshot,
    pub assignment: &'a ChildSpec,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedChange {
    pub path: String,
    pub parent: Probe,
    pub before: Option<Vec<u8>>,
    pub after: Option<Vec<u8>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    ConcurrentContent,
    AddDelete,
    BinaryContent,
    IndexOnly,
    ModeChange,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conflict {
    pub path: String,
    pub kind: ConflictKind,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationPlan {
    pub base_fingerprint: String,
    pub child_fingerprint: Option<String>,
    pub parent_fingerprint: Option<String>,
    /// This is an observed read dependency, never a proposed index mutation.
    pub parent_index: Option<FileVersion>,
    pub parent_manifest: Option<observation::Manifest>,
    pub parent_probes: Vec<Probe>,
    pub changes: Vec<ProposedChange>,
    pub conflicts: Vec<Conflict>,
    pub rejection: Option<String>,
    pub findings: Vec<ReviewFinding>,
}
impl IntegrationPlan {
    pub fn ready(&self) -> bool {
        self.rejection.is_none()
            && self.conflicts.is_empty()
            && self.parent_fingerprint.is_some()
            && self.parent_manifest.is_some()
    }
    fn reject(mut self, reason: impl Into<String>) -> Self {
        self.rejection = Some(reason.into());
        self.changes.clear();
        self
    }
}
fn files(snapshot: &WorkspaceSnapshot) -> BTreeMap<&str, &SnapshotFile> {
    snapshot
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect()
}
fn changed(base: &WorkspaceSnapshot, result: &WorkspaceSnapshot) -> BTreeSet<String> {
    let before = files(base);
    let after = files(result);
    before
        .keys()
        .chain(after.keys())
        .filter(|name| before.get(**name) != after.get(**name))
        .map(|name| (*name).to_owned())
        .collect()
}
fn permitted(inputs: &Inputs<'_>, name: &str) -> bool {
    inputs.assignment.paths.iter().any(|scope| {
        scope.write
            && (scope.root == inputs.parent.identity.root
                || scope.root == inputs.metadata_owner.identity.root)
            && vcp_domain::agents::prefix(&scope.path, name)
    })
}
/// Full no-follow inventory is necessary: ignored or out-of-scope new files
/// must not disappear merely because ordinary source discovery excludes them.
fn inventory(root: &Root, git: bool) -> Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    let mut pending = vec![PathBuf::new()];
    let mut count = 0;
    while let Some(directory) = pending.pop() {
        let _held = root.hold(
            if directory.as_os_str().is_empty() {
                None
            } else {
                Some(&directory)
            },
            true,
        )?;
        for entry in std::fs::read_dir(root.path().join(&directory))? {
            count += 1;
            if count > 30_000 {
                return Err(Error::Limit("child result inventory"));
            }
            let entry = entry?;
            let relative = directory.join(entry.file_name());
            let name = path::relative(&relative)?;
            if name == ".vcp-child-owner" || (git && name == ".git") {
                continue;
            }
            if entry.file_type()?.is_dir() {
                let _held = root.hold(Some(&relative), true)?;
                pending.push(relative);
            } else {
                let _held = root.hold(Some(&relative), false)?;
                names.insert(name);
            }
        }
    }
    Ok(names)
}
fn policy(names: BTreeSet<String>) -> CapturePolicy {
    CapturePolicy {
        untracked: names,
        ..Default::default()
    }
}
fn validate_inputs(inputs: &Inputs<'_>) -> Result<()> {
    inputs.base.validate()?;
    inputs
        .assignment
        .validate()
        .map_err(|_| Error::Scope("invalid child assignment".into()))?;
    let registration_digest =
        vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(inputs.registration)?);
    let snapshot_digest = vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(inputs.base)?);
    if inputs.registration.source != inputs.parent.identity
        || inputs.registration.child != inputs.child.identity
        || inputs.base.source != inputs.parent.identity
        || inputs.registration.snapshot != inputs.base.fingerprint
        || inputs.assignment.snapshot_digest != snapshot_digest
        || inputs.assignment.registration_digest.as_deref() != Some(&registration_digest)
        || inputs.assignment.isolated_root.as_ref() != Some(&inputs.child.identity.root)
    {
        return Err(Error::Scope(
            "result does not bind the canonical assignment, base and registered worktree".into(),
        ));
    }
    Ok(())
}
/// Trusted observation for constructing a result claim. The controller must
/// capture these bytes itself; a model-supplied digest is not an observation.
pub async fn observe_result(
    snapshotter: &Snapshotter,
    inputs: &Inputs<'_>,
) -> Result<WorkspaceSnapshot> {
    validate_inputs(inputs)?;
    let names = inventory(inputs.child, inputs.base.base_commit.is_some())?;
    // Never capture the contents of a newly introduced sensitive file. Its
    // presence is a validation error with findings still retained by prepare.
    if names.iter().any(|name| dirty_snapshot::sensitive(name)) {
        return Err(Error::Scope("sensitive child result input".into()));
    }
    let observed = snapshotter
        .capture_registered(
            inputs.child,
            inputs.metadata_owner,
            inputs.registration,
            &policy(names.clone()),
        )
        .await?;
    let observed_names: BTreeSet<_> = observed
        .files
        .iter()
        .filter(|file| file.working.is_some())
        .map(|file| file.path.clone())
        .collect();
    if observed_names != names {
        return Err(Error::Scope(
            "ignored or excluded child result file requires explicit resolution".into(),
        ));
    }
    if inventory(inputs.child, inputs.base.base_commit.is_some())? != names {
        return Err(Error::Stale);
    }
    Ok(observed)
}
/// Build a claim from a fresh owner observation, with no invented findings or
/// verification claims. Preparation observes again before admitting any edit.
pub async fn observe_packet(snapshotter: &Snapshotter, inputs: &Inputs<'_>) -> Result<ChildPacket> {
    let result = observe_result(snapshotter, inputs).await?;
    Ok(ChildPacket {
        base_fingerprint: inputs.base.fingerprint.clone(),
        result_fingerprint: result.fingerprint.clone(),
        changed_paths: changed(inputs.base, &result),
        findings: Vec::new(),
    })
}
/// Recheck the complete qualified source inventory as well as selected binary
/// and absence probes. A new unrelated source file invalidates the preview.
pub fn revalidate_parent(
    root: &Root,
    manifest: &observation::Manifest,
    probes: &[Probe],
) -> Result<()> {
    if manifest.identity != root.identity || manifest.git.is_some() {
        return Err(Error::Scope("integration parent manifest".into()));
    }
    let scan = root.discover(&discovery::Limits::default())?;
    let versions: Vec<_> = scan
        .sources
        .iter()
        .map(|source| source.version.clone())
        .collect();
    if versions != manifest.files
        || scan.ignore_dependencies != manifest.ignore_dependencies
        || scan.exclusions != manifest.exclusions
        || scan.complete != manifest.bounded_scan_complete
    {
        return Err(Error::Stale);
    }
    instructions::revalidate_probes(probes, std::slice::from_ref(root))
}
fn working<'a>(file: Option<&&'a SnapshotFile>) -> Option<&'a [u8]> {
    file.and_then(|file| (*file).working.as_deref())
}
fn merge_bytes(
    base: Option<&[u8]>,
    child: Option<&[u8]>,
    parent: Option<&[u8]>,
) -> std::result::Result<Option<Vec<u8>>, ConflictKind> {
    if child == base || child == parent {
        return Ok(parent.map(Vec::from));
    }
    if parent == base {
        return Ok(child.map(Vec::from));
    }
    let (Some(base), Some(child), Some(parent)) = (base, child, parent) else {
        return Err(ConflictKind::AddDelete);
    };
    if [base, child, parent]
        .iter()
        .any(|bytes| bytes.contains(&0) || std::str::from_utf8(bytes).is_err())
    {
        return Err(ConflictKind::BinaryContent);
    }
    diffy::merge_bytes(base, parent, child)
        .map(Some)
        .map_err(|_| ConflictKind::ConcurrentContent)
}
fn parent_index(inputs: &Inputs<'_>) -> Result<Option<FileVersion>> {
    if inputs.base.base_commit.is_none() {
        return Ok(None);
    }
    let owner = inputs.metadata_owner;
    if inputs.parent.identity == owner.identity {
        return owner
            .version(Path::new(".git/index"), 64 * 1024 * 1024)
            .map(Some);
    }
    let pointer = inputs.parent.read(Path::new(".git"), 32 * 1024)?;
    let pointer = std::str::from_utf8(&pointer.bytes)
        .map_err(|_| Error::Unsupported("index metadata encoding"))?
        .trim()
        .strip_prefix("gitdir: ")
        .ok_or(Error::Scope("index metadata pointer".into()))?;
    let directory = std::fs::canonicalize(pointer)?;
    let expected = std::fs::canonicalize(owner.path().join(".git/worktrees"))?;
    if directory.parent() != Some(expected.as_path()) {
        return Err(Error::Scope("index outside registered metadata".into()));
    }
    let name = directory
        .file_name()
        .ok_or(Error::Scope("index metadata name".into()))?;
    owner
        .version(
            &Path::new(".git/worktrees").join(name).join("index"),
            64 * 1024 * 1024,
        )
        .map(Some)
}
pub async fn prepare(
    snapshotter: &Snapshotter,
    inputs: Inputs<'_>,
    packet: &ChildPacket,
) -> Result<IntegrationPlan> {
    packet.validate_findings()?;
    let mut plan = IntegrationPlan {
        base_fingerprint: inputs.base.fingerprint.clone(),
        child_fingerprint: None,
        parent_fingerprint: None,
        parent_index: None,
        parent_manifest: None,
        parent_probes: vec![],
        changes: vec![],
        conflicts: vec![],
        rejection: None,
        findings: packet.findings.clone(),
    };
    if let Err(error) = validate_inputs(&inputs) {
        return Ok(plan.reject(error.to_string()));
    }
    if packet.base_fingerprint != inputs.base.fingerprint {
        return Ok(plan.reject("stale child base fingerprint"));
    }
    for finding in &packet.findings {
        if let ReviewFinding::Structured(finding) = finding {
            if !finding.matches_revision(&packet.base_fingerprint, &packet.result_fingerprint) {
                return Ok(plan.reject("review finding describes a different examined revision"));
            }
            if finding.examined_paths.iter().any(|path| {
                !inputs.assignment.paths.iter().any(|scope| {
                    (scope.root == inputs.parent.identity.root
                        || scope.root == inputs.metadata_owner.identity.root)
                        && vcp_domain::agents::prefix(&scope.path, path)
                })
            }) {
                return Ok(plan.reject("review finding exceeds assignment read scope"));
            }
        }
    }
    let result = match observe_result(snapshotter, &inputs).await {
        Ok(result) => result,
        Err(error) => return Ok(plan.reject(error.to_string())),
    };
    plan.child_fingerprint = Some(result.fingerprint.clone());
    let changes = changed(inputs.base, &result);
    if packet.result_fingerprint != result.fingerprint || packet.changed_paths != changes {
        return Ok(plan.reject("child packet differs from observed result"));
    }
    if !changes.is_empty()
        && (inputs.assignment.mode == ChildMode::ReadOnly
            || changes.iter().any(|name| !permitted(&inputs, name)))
    {
        return Ok(plan.reject("child changes exceed assignment write scope"));
    }
    if changes.len() > 64 {
        return Ok(plan.reject("integration file count exceeds prepared effect limit"));
    }
    let _parent_metadata = if inputs.base.base_commit.is_some()
        && inputs.parent.identity != inputs.metadata_owner.identity
    {
        snapshotter.pin_metadata(inputs.metadata_owner, inputs.parent)?
    } else {
        vec![]
    };
    plan.parent_index = parent_index(&inputs)?;
    let parent_observation = inputs
        .parent
        .observe(None, &discovery::Limits::default())
        .await?;
    let selected: BTreeSet<String> = inputs
        .base
        .files
        .iter()
        .chain(&result.files)
        .map(|file| file.path.clone())
        .collect();
    let current = match inputs.parent_registration {
        Some(registration) => {
            snapshotter
                .capture_registered(
                    inputs.parent,
                    inputs.metadata_owner,
                    registration,
                    &policy(selected),
                )
                .await?
        }
        None => {
            snapshotter
                .capture(inputs.parent, &policy(selected))
                .await?
        }
    };
    plan.parent_fingerprint = Some(vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(
        &(&current.fingerprint, &parent_observation.manifest),
    )?));
    let all_names: BTreeSet<_> = current
        .files
        .iter()
        .chain(&inputs.base.files)
        .chain(&result.files)
        .map(|file| file.path.clone())
        .collect();
    let current_files = files(&current);
    for name in all_names {
        let actual = match inputs.parent.read(Path::new(&name), 64 * 1024 * 1024) {
            Ok(source) => Some(source),
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        if actual.as_ref().map(|source| source.bytes.as_slice())
            != working(current_files.get(name.as_str()))
        {
            return Err(Error::Stale);
        }
        plan.parent_probes.push(Probe {
            root: inputs.parent.identity.root.clone(),
            binding: inputs.parent.identity.binding,
            path: name,
            observed: actual.map(|source| source.version),
        });
    }
    plan.parent_manifest = Some(parent_observation.manifest);
    let base_files = files(inputs.base);
    let child_files = files(&result);
    let parent_files = files(&current);
    let mut total = 0usize;
    for name in changes {
        let base = base_files.get(name.as_str());
        let child = child_files.get(name.as_str());
        let parent = parent_files.get(name.as_str());
        if [working(base), working(child), working(parent)]
            .into_iter()
            .flatten()
            .any(|bytes| {
                bytes.len() > 1024 * 1024
                    || bytes.iter().filter(|byte| **byte == b'\n').count() > 20_000
            })
        {
            return Ok(plan.reject("integration merge input exceeds bounded file or line limit"));
        }
        let base_mode = base.and_then(|file| file.mode.as_deref());
        let child_mode = child.and_then(|file| file.mode.as_deref());
        if base_mode.is_some() && child_mode.is_some() && base_mode != child_mode {
            plan.conflicts.push(Conflict {
                path: name,
                kind: ConflictKind::ModeChange,
            });
            continue;
        }
        if working(base) == working(child) {
            plan.conflicts.push(Conflict {
                path: name,
                kind: ConflictKind::IndexOnly,
            });
            continue;
        }
        let after = match merge_bytes(working(base), working(child), working(parent)) {
            Ok(after) => after,
            Err(kind) => {
                plan.conflicts.push(Conflict { path: name, kind });
                continue;
            }
        };
        if after.as_deref() == working(parent) {
            continue;
        }
        let observed = match inputs.parent.read(Path::new(&name), 1024 * 1024) {
            Ok(observed) => Some(observed),
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        if observed.as_ref().map(|source| source.bytes.as_slice()) != working(parent) {
            return Err(Error::Stale);
        }
        total += after.as_ref().map_or(0, Vec::len);
        if total > 4 * 1024 * 1024
            || after
                .as_ref()
                .is_some_and(|bytes| bytes.len() > 1024 * 1024)
        {
            return Err(Error::Limit("integration bytes"));
        }
        plan.changes.push(ProposedChange {
            path: name.clone(),
            parent: Probe {
                root: inputs.parent.identity.root.clone(),
                binding: inputs.parent.identity.binding,
                path: name,
                observed: observed.as_ref().map(|source| source.version.clone()),
            },
            before: observed.map(|source| source.bytes),
            after,
        });
    }
    for change in &plan.changes {
        instructions::revalidate_probes(
            std::slice::from_ref(&change.parent),
            std::slice::from_ref(inputs.parent),
        )?;
    }
    if let Some(index) = &plan.parent_index {
        inputs.metadata_owner.revalidate(index)?;
    }
    revalidate_parent(
        inputs.parent,
        plan.parent_manifest.as_ref().ok_or(Error::Stale)?,
        &plan.parent_probes,
    )?;
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn three_way_bytes_preserve_independent_parent_edits_and_surface_conflicts() {
        assert_eq!(
            merge_bytes(Some(b"a\nb\nc\n"), Some(b"A\nb\nc\n"), Some(b"a\nb\nC\n")).unwrap(),
            Some(b"A\nb\nC\n".to_vec())
        );
        assert_eq!(
            merge_bytes(Some(b"base\n"), Some(b"child\n"), Some(b"parent\n")),
            Err(ConflictKind::ConcurrentContent)
        );
        assert_eq!(
            merge_bytes(None, Some(b"child"), Some(b"parent")),
            Err(ConflictKind::AddDelete)
        );
        assert_eq!(
            merge_bytes(Some(b"base"), None, Some(b"parent")),
            Err(ConflictKind::AddDelete)
        );
        assert_eq!(
            merge_bytes(Some(b"a\0"), Some(b"b\0"), Some(b"c\0")),
            Err(ConflictKind::BinaryContent)
        );
        assert_eq!(
            merge_bytes(Some(b"a\0"), Some(b"b\0"), Some(b"a\0")).unwrap(),
            Some(b"b\0".to_vec())
        );
        assert_eq!(
            merge_bytes(
                Some(b"base"),
                Some(b"already applied"),
                Some(b"already applied")
            )
            .unwrap(),
            Some(b"already applied".to_vec())
        );
    }
}
