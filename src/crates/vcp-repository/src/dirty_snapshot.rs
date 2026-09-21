// SPDX-License-Identifier: Apache-2.0
//! Bounded, replayable dirty workspace inputs. Snapshot bytes belong in a
//! scoped artifact; the graph stores only its identity and fingerprint.
use crate::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Clone, Debug)]
pub struct CapturePolicy {
    /// Exact untracked paths authorized by the assignment, never a glob.
    pub untracked: BTreeSet<String>,
    pub required: BTreeSet<String>,
    /// Explicit additional sensitive path prefixes supplied by the host.
    pub excluded: BTreeSet<String>,
    pub file_bytes: u64,
    pub total_bytes: u64,
    pub attempts: usize,
}
impl Default for CapturePolicy {
    fn default() -> Self {
        Self {
            untracked: BTreeSet::new(),
            required: BTreeSet::new(),
            excluded: BTreeSet::new(),
            file_bytes: 8 * 1024 * 1024,
            total_bytes: 32 * 1024 * 1024,
            attempts: 3,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotFile {
    pub path: String,
    /// None is an authorized untracked file, or a staged deletion that still
    /// exists in the working directory. Only regular Git modes are supported.
    pub index: Option<Vec<u8>>,
    pub mode: Option<String>,
    /// None records a working-tree deletion without deleting the index entry.
    pub working: Option<Vec<u8>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceSnapshot {
    pub source: RootIdentity,
    /// None identifies an explicitly scoped non-Git isolated-copy snapshot.
    pub base_commit: Option<String>,
    pub files: Vec<SnapshotFile>,
    pub excluded: Vec<String>,
    pub fingerprint: String,
}
pub(crate) fn sensitive(path: &str) -> bool {
    path.split('/').any(|part| {
        let part = part.to_ascii_lowercase();
        matches!(
            part.as_str(),
            ".git"
                | ".vcp"
                | ".vcp-child-owner"
                | ".env"
                | ".ssh"
                | ".aws"
                | ".azure"
                | ".npmrc"
                | ".netrc"
                | "id_rsa"
                | "id_ed25519"
                | "credentials"
                | "credentials.json"
        ) || part.starts_with(".env.")
            || part.ends_with(".pem")
            || part.ends_with(".pfx")
            || part.ends_with(".key")
    })
}
fn excluded(path: &str, policy: &CapturePolicy) -> bool {
    sensitive(path)
        || policy.excluded.iter().any(|prefix| {
            path.eq_ignore_ascii_case(prefix)
                || path
                    .to_ascii_lowercase()
                    .starts_with(&format!("{}/", prefix.to_ascii_lowercase()))
        })
}
pub(crate) fn has_git_metadata(root: &Root) -> Result<bool> {
    match std::fs::symlink_metadata(root.path().join(".git")) {
        Ok(metadata) => {
            let _held = root.hold(Some(Path::new(".git")), metadata.is_dir())?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
impl WorkspaceSnapshot {
    pub fn validate(&self) -> Result<()> {
        if self.base_commit.as_ref().is_some_and(|base| {
            !matches!(base.len(), 40 | 64) || !base.bytes().all(|b| b.is_ascii_hexdigit())
        }) || self.files.len() > 10_000
        {
            return Err(Error::Scope("invalid snapshot base or size".into()));
        }
        let mut paths = BTreeSet::new();
        let mut total = 0usize;
        for file in &self.files {
            if path::relative(Path::new(&file.path))? != file.path
                || sensitive(&file.path)
                || !paths.insert(file.path.to_lowercase())
                || file.index.is_some() != file.mode.is_some()
                || (self.base_commit.is_none() && (file.index.is_some() || file.working.is_none()))
                || file
                    .mode
                    .as_deref()
                    .is_some_and(|mode| !matches!(mode, "100644" | "100755"))
            {
                return Err(Error::Scope("invalid snapshot file".into()));
            }
            for bytes in file.index.iter().chain(file.working.iter()) {
                total = total
                    .checked_add(bytes.len())
                    .ok_or(Error::Limit("snapshot bytes"))?;
                if bytes.len() > 64 * 1024 * 1024 || total > 64 * 1024 * 1024 {
                    return Err(Error::Limit("snapshot bytes"));
                }
            }
        }
        if self.digest()? != self.fingerprint {
            return Err(Error::Stale);
        }
        Ok(())
    }
    pub(crate) fn digest(&self) -> Result<String> {
        let mut value = self.clone();
        value.fingerprint.clear();
        Ok(vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(
            &value,
        )?))
    }
}

impl crate::worktree::Snapshotter {
    pub async fn capture(&self, root: &Root, policy: &CapturePolicy) -> Result<WorkspaceSnapshot> {
        self.capture_scoped(root, root, policy).await
    }
    pub async fn capture_registered(
        &self,
        root: &Root,
        metadata_owner: &Root,
        registration: &crate::worktree::WorkspaceRegistration,
        policy: &CapturePolicy,
    ) -> Result<WorkspaceSnapshot> {
        let _marker = root.hold(Some(Path::new(".vcp-child-owner")), false)?;
        if root.identity != registration.child
            || std::fs::canonicalize(root.path())?
                != std::fs::canonicalize(&registration.absolute_path)?
            || root.read(Path::new(".vcp-child-owner"), 64 * 1024)?.bytes
                != vcp_protocol::canonical_bytes(registration)?
        {
            return Err(Error::Scope("registered parent worktree identity".into()));
        }
        if registration.git_metadata_owner.is_none() {
            if has_git_metadata(root)? {
                return Err(Error::Stale);
            }
            return self.capture(root, policy).await;
        }
        self.check_metadata_registration(metadata_owner, registration)?;
        let _configuration = metadata_owner.hold(Some(Path::new(".git/config")), false)?;
        self.git.observe(metadata_owner).await?;
        let _metadata = self.pin_metadata(metadata_owner, root)?;
        self.capture_scoped(root, metadata_owner, policy).await
    }
    async fn capture_scoped(
        &self,
        root: &Root,
        metadata_owner: &Root,
        policy: &CapturePolicy,
    ) -> Result<WorkspaceSnapshot> {
        if policy.attempts == 0
            || policy.attempts > 5
            || policy.file_bytes == 0
            || policy.file_bytes > 64 * 1024 * 1024
            || policy.total_bytes == 0
            || policy.total_bytes > 64 * 1024 * 1024
        {
            return Err(Error::Limit("snapshot policy"));
        }
        for path in policy
            .untracked
            .iter()
            .chain(&policy.required)
            .chain(&policy.excluded)
        {
            crate::path::relative(Path::new(path))?;
        }
        if !has_git_metadata(root)? {
            for _ in 0..policy.attempts {
                match self.capture_plain_once(root, policy, || {}) {
                    Err(Error::Stale) => continue,
                    result => return result,
                }
            }
            return Err(Error::Stale);
        }
        if !root.path().join(".git").is_dir() && root.identity == metadata_owner.identity {
            return Err(Error::Unsupported(
                "child snapshots require a qualified ordinary Git root",
            ));
        }
        for _ in 0..policy.attempts {
            match self.capture_once(root, metadata_owner, policy, || {}).await {
                Err(Error::Stale) => continue,
                result => return result,
            }
        }
        Err(Error::Stale)
    }
    async fn capture_once(
        &self,
        root: &Root,
        metadata_owner: &Root,
        policy: &CapturePolicy,
        after_collection: impl FnOnce(),
    ) -> Result<WorkspaceSnapshot> {
        let _configuration = metadata_owner.hold(Some(Path::new(".git/config")), false)?;
        let before = self.observe_scoped(root, metadata_owner).await?;
        // Sparse/assume-unchanged and intent-to-add entries carry index
        // semantics beyond content and mode. Fail explicitly until those
        // formats are qualified instead of changing their staged meaning.
        let flags = self.run(root, &["ls-files", "-v", "-z"], None).await?;
        if flags
            .split(|byte| *byte == 0)
            .filter(|entry| !entry.is_empty())
            .any(|entry| !entry.starts_with(b"H "))
        {
            return Err(Error::Unsupported(
                "special Git index flags require qualification",
            ));
        }
        let visible_intent = self
            .run(
                root,
                &[
                    "diff",
                    "--cached",
                    "--ita-visible-in-index",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--binary",
                    "--",
                ],
                None,
            )
            .await?;
        if visible_intent != before.staged_diff {
            return Err(Error::Unsupported(
                "intent-to-add index entries require qualification",
            ));
        }
        let base_commit = before.snapshot.head.clone().ok_or(Error::Unsupported(
            "child snapshot requires an existing base commit",
        ))?;
        let mut entries = BTreeMap::new();
        for entry in std::str::from_utf8(&before.index)
            .map_err(|_| Error::Unsupported("index encoding"))?
            .split_terminator('\0')
        {
            let (header, name) = entry
                .split_once('\t')
                .ok_or(Error::Git("index entry".into()))?;
            let fields: Vec<_> = header.split(' ').collect();
            if fields.len() != 3 || fields[2] != "0" || !matches!(fields[0], "100644" | "100755") {
                return Err(Error::Unsupported(
                    "conflicted, symlink or submodule child input",
                ));
            }
            let name = path::relative(Path::new(name))?;
            if excluded(&name, policy) {
                return Err(Error::Unsupported(
                    "tracked sensitive input requires an explicitly sanitized base",
                ));
            }
            entries.insert(name, (fields[0].to_owned(), fields[1].to_owned()));
        }
        let mut omitted = Vec::new();
        let available_untracked: BTreeSet<_> = before
            .snapshot
            .changes
            .iter()
            .filter(|change| change.index == '?')
            .map(|change| change.path.clone())
            .collect();
        let mut names: BTreeSet<_> = entries.keys().cloned().collect();
        for name in &available_untracked {
            if policy.untracked.contains(name) && !excluded(name, policy) {
                names.insert(name.clone());
            } else {
                omitted.push(name.clone());
            }
        }
        for name in &policy.untracked {
            if !names.contains(name) {
                omitted.push(name.clone());
            }
        }
        if policy.required.iter().any(|name| !names.contains(name)) {
            return Err(Error::Scope(
                "required child input is missing, ignored or excluded".into(),
            ));
        }
        if names.len() > 10_000 {
            return Err(Error::Limit("snapshot files"));
        }
        let mut files = Vec::new();
        let mut total = 0u64;
        for name in names {
            let (index, mode) = if let Some((mode, object)) = entries.get(&name) {
                let bytes = self.run(root, &["cat-file", "blob", object], None).await?;
                (Some(bytes), Some(mode.clone()))
            } else {
                (None, None)
            };
            let working = match root.read(Path::new(&name), policy.file_bytes) {
                Ok(source) => Some(source.bytes),
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error),
            };
            if policy.required.contains(&name) && working.is_none() {
                return Err(Error::Scope("required child input is deleted".into()));
            }
            for bytes in index.iter().chain(working.iter()) {
                total = total
                    .checked_add(bytes.len() as u64)
                    .ok_or(Error::Limit("snapshot bytes"))?;
                if bytes.len() as u64 > policy.file_bytes || total > policy.total_bytes {
                    return Err(Error::Limit("snapshot bytes"));
                }
            }
            files.push(SnapshotFile {
                path: name,
                index,
                mode,
                working,
            });
        }
        // Deterministic seam for qualifying concurrent human file changes.
        // Production capture supplies no scheduling authority.
        after_collection();
        let after = self.observe_scoped(root, metadata_owner).await?;
        if before.snapshot != after.snapshot {
            return Err(Error::Stale);
        }
        // Status alone does not fingerprint an untracked file, nor guarantee
        // unchanged tracked bytes when Git's stat cache is stale.
        for file in &files {
            let current = match root.read(Path::new(&file.path), policy.file_bytes) {
                Ok(source) => Some(source.bytes),
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error),
            };
            if current != file.working {
                return Err(Error::Stale);
            }
        }
        omitted.sort();
        omitted.dedup();
        let mut snapshot = WorkspaceSnapshot {
            source: root.identity.clone(),
            base_commit: Some(base_commit),
            files,
            excluded: omitted,
            fingerprint: String::new(),
        };
        snapshot.fingerprint = snapshot.digest()?;
        snapshot.validate()?;
        Ok(snapshot)
    }
    async fn observe_scoped(&self, root: &Root, metadata_owner: &Root) -> Result<git::Observation> {
        if root.identity == metadata_owner.identity {
            return self.git.observe(root).await;
        }
        let index = self.run(root, &["ls-files", "--stage", "-z"], None).await?;
        let status = self
            .run(
                root,
                &[
                    "status",
                    "--porcelain=v2",
                    "-z",
                    "--untracked-files=all",
                    "--ignored=no",
                ],
                None,
            )
            .await?;
        let head = self
            .run(root, &["rev-parse", "--verify", "HEAD"], None)
            .await?;
        let head = std::str::from_utf8(&head)
            .map_err(|_| Error::Unsupported("HEAD encoding"))?
            .trim()
            .to_owned();
        let staged_diff = self
            .run(
                root,
                &[
                    "diff",
                    "--cached",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--binary",
                    "--",
                ],
                None,
            )
            .await?;
        let unstaged_diff = self
            .run(
                root,
                &["diff", "--no-ext-diff", "--no-textconv", "--binary", "--"],
                None,
            )
            .await?;
        let mut snapshot = git::Snapshot {
            identity: root.identity.clone(),
            head: Some(head),
            changes: git::parse_status(&status)?,
            index_sha256: vcp_protocol::digest_bytes(&index),
            status_sha256: vcp_protocol::digest_bytes(&status),
            staged_diff_sha256: vcp_protocol::digest_bytes(&staged_diff),
            unstaged_diff_sha256: vcp_protocol::digest_bytes(&unstaged_diff),
            digest: String::new(),
        };
        snapshot.digest = vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&snapshot)?);
        Ok(git::Observation {
            snapshot,
            status,
            index,
            staged_diff,
            unstaged_diff,
        })
    }
    fn plain_inputs(
        &self,
        root: &Root,
        policy: &CapturePolicy,
    ) -> Result<(Vec<SnapshotFile>, Vec<String>, String)> {
        use ignore::gitignore::GitignoreBuilder;
        let mut files = Vec::new();
        let mut omitted = Vec::new();
        let mut rules = BTreeMap::new();
        let mut total = 0u64;
        if policy.untracked.len() > 10_000 {
            return Err(Error::Limit("snapshot files"));
        }
        if policy
            .required
            .iter()
            .any(|path| !policy.untracked.contains(path))
        {
            return Err(Error::Scope(
                "required non-Git input is outside explicit input set".into(),
            ));
        }
        for name in &policy.untracked {
            if excluded(name, policy) {
                if policy.required.contains(name) {
                    return Err(Error::Scope("required child input is excluded".into()));
                }
                omitted.push(name.clone());
                continue;
            }
            let mut ignored = false;
            let relative = Path::new(name);
            let mut directories: Vec<_> = relative
                .parent()
                .into_iter()
                .flat_map(|parent| parent.ancestors())
                .collect();
            directories.reverse();
            for directory in directories {
                let ignore_path = directory.join(".gitignore");
                let source = match root.read(&ignore_path, 64 * 1024) {
                    Ok(source) => Some(source),
                    Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => return Err(error),
                };
                rules.insert(
                    ignore_path.to_string_lossy().into_owned(),
                    source.as_ref().map(|source| source.version.sha256.clone()),
                );
                if let Some(source) = source {
                    let text = std::str::from_utf8(&source.bytes)
                        .map_err(|_| Error::Unsupported("non-UTF8 ignore rules"))?;
                    let mut builder = GitignoreBuilder::new(root.path().join(directory));
                    for line in text.lines() {
                        builder
                            .add_line(None, line)
                            .map_err(|_| Error::Unsupported("invalid ignore pattern"))?;
                    }
                    let matcher = builder
                        .build()
                        .map_err(|_| Error::Unsupported("invalid ignore rules"))?;
                    let matched =
                        matcher.matched_path_or_any_parents(root.path().join(relative), false);
                    if matched.is_ignore() {
                        ignored = true;
                    }
                    // A nested exception cannot re-include an ignored ancestor.
                    if ignored {
                        break;
                    }
                }
            }
            if excluded(name, policy) || ignored {
                if policy.required.contains(name) {
                    return Err(Error::Scope(
                        "required child input is ignored or excluded".into(),
                    ));
                }
                omitted.push(name.clone());
                continue;
            }
            let bytes = match root.read(relative, policy.file_bytes) {
                Ok(source) => source.bytes,
                Err(Error::Io(error))
                    if error.kind() == std::io::ErrorKind::NotFound
                        && !policy.required.contains(name) =>
                {
                    omitted.push(name.clone());
                    continue;
                }
                Err(error) => return Err(error),
            };
            total = total
                .checked_add(bytes.len() as u64)
                .ok_or(Error::Limit("snapshot bytes"))?;
            if total > policy.total_bytes {
                return Err(Error::Limit("snapshot bytes"));
            }
            files.push(SnapshotFile {
                path: name.clone(),
                index: None,
                mode: None,
                working: Some(bytes),
            });
        }
        Ok((
            files,
            omitted,
            vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&rules)?),
        ))
    }
    fn capture_plain_once(
        &self,
        root: &Root,
        policy: &CapturePolicy,
        after_collection: impl FnOnce(),
    ) -> Result<WorkspaceSnapshot> {
        let _root = root.hold(None, true)?;
        let before = self.plain_inputs(root, policy)?;
        after_collection();
        let after = self.plain_inputs(root, policy)?;
        if before != after || has_git_metadata(root)? {
            return Err(Error::Stale);
        }
        let mut snapshot = WorkspaceSnapshot {
            source: root.identity.clone(),
            base_commit: None,
            files: before.0,
            excluded: before.1,
            fingerprint: String::new(),
        };
        snapshot.fingerprint = snapshot.digest()?;
        snapshot.validate()?;
        Ok(snapshot)
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::{fs, process::Command, time::Duration};
    #[tokio::test]
    async fn changes_during_collection_do_not_produce_a_stable_snapshot() {
        let executable =
            PathBuf::from(std::env::var_os("VCP_TEST_GIT").expect("native Git fixture"));
        let temp = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let result = Command::new(&executable)
                .current_dir(temp.path())
                .args(args)
                .output()
                .unwrap();
            assert!(result.status.success());
        };
        git(&["init", "-q"]);
        git(&["config", "user.name", "Fixture"]);
        git(&["config", "user.email", "fixture@example.invalid"]);
        fs::write(temp.path().join("tracked.txt"), b"base\n").unwrap();
        git(&["add", "tracked.txt"]);
        git(&["commit", "-qm", "base"]);
        fs::write(temp.path().join("input.txt"), b"before\n").unwrap();
        let root = Root::open(
            RootIdentity {
                workspace: WorkspaceId::new(),
                root: RootId::new(),
                repository: "fixture".into(),
                worktree: "parent".into(),
                binding: Revision::ZERO,
            },
            temp.path(),
        )
        .unwrap();
        let environment = ["SystemRoot", "WINDIR", "PATH", "TEMP", "TMP"]
            .into_iter()
            .filter_map(|name| std::env::var_os(name).map(|value| (name.into(), value)))
            .collect();
        let service = crate::worktree::Snapshotter::new(
            executable,
            environment,
            Duration::from_secs(20),
            8 * 1024 * 1024,
        )
        .unwrap();
        let policy = CapturePolicy {
            untracked: ["input.txt".into()].into(),
            ..Default::default()
        };
        for name in ["tracked.txt", "input.txt"] {
            let result = service
                .capture_once(&root, &root, &policy, || {
                    fs::write(temp.path().join(name), b"human change during capture\n").unwrap()
                })
                .await;
            assert!(matches!(result, Err(Error::Stale)));
            assert_eq!(
                fs::read(temp.path().join(name)).unwrap(),
                b"human change during capture\n"
            );
        }
        service.capture(&root, &policy).await.unwrap();
        let plain = tempfile::tempdir().unwrap();
        fs::write(plain.path().join("input.txt"), b"copy before").unwrap();
        let plain_root = Root::open(
            RootIdentity {
                root: RootId::new(),
                worktree: "plain".into(),
                ..root.identity.clone()
            },
            plain.path(),
        )
        .unwrap();
        let changed = service.capture_plain_once(&plain_root, &policy, || {
            fs::write(plain.path().join("input.txt"), b"copy changed").unwrap()
        });
        assert!(matches!(changed, Err(Error::Stale)));
        let changed_rules = service.capture_plain_once(&plain_root, &policy, || {
            fs::write(plain.path().join(".gitignore"), b"input.txt\n").unwrap()
        });
        assert!(matches!(changed_rules, Err(Error::Stale)));
    }
}
