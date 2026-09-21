// SPDX-License-Identifier: Apache-2.0
//! Explicit disposable workspace materialization. The owner must persist the
//! registration before calling this boundary. Failures retain the exact path
//! for reconciliation; this module never guesses or recursively deletes paths.
use crate::{dirty_snapshot::WorkspaceSnapshot, *};
use std::{collections::BTreeMap, ffi::OsString, path::Path, time::Duration};
use vcp_domain::TaskId;

pub struct Snapshotter {
    pub(crate) git: git::Git,
    executable: PathBuf,
    environment: BTreeMap<OsString, OsString>,
    timeout: Duration,
    output_limit: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceRegistration {
    pub owner: TaskId,
    pub source: RootIdentity,
    pub child: RootIdentity,
    pub absolute_path: PathBuf,
    pub snapshot: String,
    pub git_metadata_owner: Option<RegisteredRoot>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisteredRoot {
    pub identity: RootIdentity,
    pub absolute_path: PathBuf,
}
pub struct MaterializedWorkspace {
    pub root: Root,
    pub registration: WorkspaceRegistration,
}
fn git_path(path: &Path) -> Result<String> {
    // Git's worktree writer interprets verbatim DOS paths as UNC names. Roots
    // already qualify local disk paths; preserve that resolved identity while
    // supplying Git its ordinary DOS spelling and explicit long-path support.
    let path = path
        .to_str()
        .ok_or(Error::Unsupported("Git path encoding"))?;
    Ok(path.strip_prefix(r"\\?\").unwrap_or(path).to_owned())
}
fn metadata_path(path: &Path) -> Result<String> {
    let normalized = git_path(path)?.replace('\\', "/");
    let bytes = normalized.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' || bytes[2] != b'/' {
        return Err(Error::Scope(
            "Git metadata must use a registered local disk path".into(),
        ));
    }
    Ok(normalized)
}
impl Snapshotter {
    pub fn new(
        executable: PathBuf,
        environment: BTreeMap<OsString, OsString>,
        timeout: Duration,
        output_limit: usize,
    ) -> Result<Self> {
        let git = git::Git::new(
            executable.clone(),
            environment.clone(),
            timeout,
            output_limit,
        )?;
        Ok(Self {
            git,
            executable,
            environment,
            timeout,
            output_limit,
        })
    }
    /// `disposable_parent` is an independently registered host-owned directory.
    /// The destination must not exist and must be its direct child. Replays
    /// reconcile `verify` instead of treating an existing directory as new.
    pub async fn materialize(
        &self,
        source: &Root,
        disposable_parent: &Root,
        snapshot: &WorkspaceSnapshot,
        registration: &WorkspaceRegistration,
    ) -> Result<MaterializedWorkspace> {
        self.materialize_with_metadata(source, source, disposable_parent, snapshot, registration)
            .await
    }
    /// Nested Git children retain the original, explicitly registered common
    /// metadata owner; a .git pointer never grants access to another root.
    pub async fn materialize_with_metadata(
        &self,
        source: &Root,
        metadata_owner: &Root,
        disposable_parent: &Root,
        snapshot: &WorkspaceSnapshot,
        registration: &WorkspaceRegistration,
    ) -> Result<MaterializedWorkspace> {
        snapshot.validate()?;
        if registration.source != source.identity
            || snapshot.source != source.identity
            || registration.snapshot != snapshot.fingerprint
            || registration.child.workspace != source.identity.workspace
            || disposable_parent.identity.workspace != source.identity.workspace
            || registration.child.repository != source.identity.repository
            || registration.child.root == source.identity.root
            || registration.child.root == disposable_parent.identity.root
            || registration.child.worktree == source.identity.worktree
            || !registration.absolute_path.is_absolute()
        {
            return Err(Error::Scope(
                "workspace registration does not bind snapshot and child".into(),
            ));
        }
        let parent_path = registration
            .absolute_path
            .parent()
            .ok_or(Error::Scope("disposable parent".into()))?;
        if std::fs::canonicalize(parent_path)? != std::fs::canonicalize(disposable_parent.path())?
            || disposable_parent.path().starts_with(source.path())
        {
            return Err(Error::Scope(
                "disposable child must be outside source workspace".into(),
            ));
        }
        let name = registration
            .absolute_path
            .file_name()
            .ok_or(Error::Scope("disposable name".into()))?;
        path::relative(Path::new(name))?;
        let _parent = disposable_parent.hold(None, true)?;
        let _source = source.hold(None, true)?;
        tokio::task::yield_now().await;
        let Some(base_commit) = snapshot.base_commit.as_deref() else {
            if registration.git_metadata_owner.is_some() {
                return Err(Error::Scope(
                    "non-Git copy cannot claim Git metadata".into(),
                ));
            }
            if crate::dirty_snapshot::has_git_metadata(source)? {
                return Err(Error::Stale);
            }
            std::fs::create_dir(&registration.absolute_path)?;
            let root = Root::open(registration.child.clone(), &registration.absolute_path)?;
            self.write_new(
                &root,
                ".vcp-child-owner",
                &vcp_protocol::canonical_bytes(registration)?,
            )?;
            for file in &snapshot.files {
                tokio::task::yield_now().await;
                if let Some(bytes) = &file.working {
                    self.write_new(&root, &file.path, bytes)?;
                }
            }
            self.verify(source, &root, snapshot, registration).await?;
            return Ok(MaterializedWorkspace {
                root,
                registration: registration.clone(),
            });
        };
        self.check_metadata_registration(metadata_owner, registration)?;
        let _configuration = metadata_owner.hold(Some(Path::new(".git/config")), false)?;
        // Revalidate the local Git execution configuration and source metadata
        // boundary; the captured base can intentionally predate current HEAD.
        self.git.observe(metadata_owner).await?;
        let _source_metadata = if source.identity != metadata_owner.identity {
            self.pin_metadata(metadata_owner, source)?
        } else {
            vec![]
        };
        self.run(
            source,
            &["cat-file", "-e", &format!("{base_commit}^{{commit}}")],
            None,
        )
        .await?;
        let _git_directory = metadata_owner.hold(Some(Path::new(".git")), true)?;
        match std::fs::create_dir(metadata_owner.path().join(".git/worktrees")) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error.into()),
        }
        // Reject a metadata junction before worktree add can write through it.
        let _worktrees = metadata_owner.hold(Some(Path::new(".git/worktrees")), true)?;
        std::fs::create_dir(&registration.absolute_path)?;
        let root = Root::open(registration.child.clone(), &registration.absolute_path)?;
        let _child = root.hold(None, true)?;
        let target = git_path(root.path())?;
        self.run(
            source,
            &[
                "worktree",
                "add",
                "--detach",
                "--no-checkout",
                "--",
                &target,
                base_commit,
            ],
            None,
        )
        .await?;
        // A held .git link prevents later namespace swaps during index writes.
        let _metadata = self.pin_metadata(metadata_owner, &root)?;
        self.write_new(
            &root,
            ".vcp-child-owner",
            &vcp_protocol::canonical_bytes(registration)?,
        )?;
        self.run(&root, &["read-tree", "--empty"], None).await?;
        for file in &snapshot.files {
            tokio::task::yield_now().await;
            if let (Some(bytes), Some(mode)) = (&file.index, &file.mode) {
                let object = self
                    .run(
                        &root,
                        &["hash-object", "--stdin", "--no-filters"],
                        Some(bytes),
                    )
                    .await?;
                let object = std::str::from_utf8(&object)
                    .map_err(|_| Error::Git("object identity".into()))?
                    .trim();
                if !matches!(object.len(), 40 | 64)
                    || !object.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err(Error::Git("object identity".into()));
                }
                // Captured index objects must still exist. Do not write into
                // the parent's shared object database to repair a missing base.
                self.run(&root, &["cat-file", "-e", object], None).await?;
                self.run(
                    &root,
                    &[
                        "update-index",
                        "--add",
                        "--cacheinfo",
                        mode,
                        object,
                        &file.path,
                    ],
                    None,
                )
                .await?;
            }
            if let Some(bytes) = &file.working {
                self.write_new(&root, &file.path, bytes)?;
            }
        }
        self.verify_with_metadata(source, metadata_owner, &root, snapshot, registration)
            .await?;
        Ok(MaterializedWorkspace {
            root,
            registration: registration.clone(),
        })
    }
    /// Verifies observed files and the separate index before a persisted
    /// assignment may dispatch. It does not resume or launch any work.
    pub async fn verify(
        &self,
        source: &Root,
        root: &Root,
        snapshot: &WorkspaceSnapshot,
        registration: &WorkspaceRegistration,
    ) -> Result<()> {
        self.verify_with_metadata(source, source, root, snapshot, registration)
            .await
    }
    pub async fn verify_with_metadata(
        &self,
        source: &Root,
        metadata_owner: &Root,
        root: &Root,
        snapshot: &WorkspaceSnapshot,
        registration: &WorkspaceRegistration,
    ) -> Result<()> {
        snapshot.validate()?;
        if source.identity != snapshot.source
            || root.identity != registration.child
            || registration.source != snapshot.source
            || registration.snapshot != snapshot.fingerprint
            || registration.git_metadata_owner.is_some() != snapshot.base_commit.is_some()
            || std::fs::canonicalize(&registration.absolute_path)?
                != std::fs::canonicalize(root.path())?
        {
            return Err(Error::Scope("child registration".into()));
        }
        let _marker = root.hold(Some(Path::new(".vcp-child-owner")), false)?;
        let marker = root.read(Path::new(".vcp-child-owner"), 64 * 1024)?;
        if marker.bytes != vcp_protocol::canonical_bytes(registration)? {
            return Err(Error::Scope("disposable ownership marker".into()));
        }
        self.verify_inventory(root, snapshot)?;
        let Some(base_commit) = snapshot.base_commit.as_deref() else {
            for file in &snapshot.files {
                if Some(root.read(Path::new(&file.path), 64 * 1024 * 1024)?.bytes) != file.working {
                    return Err(Error::Stale);
                }
            }
            return Ok(());
        };
        self.check_metadata_registration(metadata_owner, registration)?;
        let _configuration = metadata_owner.hold(Some(Path::new(".git/config")), false)?;
        self.git.observe(metadata_owner).await?;
        let _metadata = self.pin_metadata(metadata_owner, root)?;
        let head = self
            .run(root, &["rev-parse", "--verify", "HEAD"], None)
            .await?;
        if std::str::from_utf8(&head)
            .map_err(|_| Error::Git("HEAD encoding".into()))?
            .trim()
            != base_commit
        {
            return Err(Error::Stale);
        }
        let index = self.run(root, &["ls-files", "--stage", "-z"], None).await?;
        let mut actual = BTreeMap::new();
        for row in std::str::from_utf8(&index)
            .map_err(|_| Error::Git("index encoding".into()))?
            .split_terminator('\0')
        {
            let (header, path) = row
                .split_once('\t')
                .ok_or(Error::Git("index entry".into()))?;
            let fields: Vec<_> = header.split(' ').collect();
            if fields.len() != 3 || fields[2] != "0" {
                return Err(Error::Stale);
            }
            actual.insert(
                path.to_owned(),
                (fields[0].to_owned(), fields[1].to_owned()),
            );
        }
        if actual.len()
            != snapshot
                .files
                .iter()
                .filter(|file| file.index.is_some())
                .count()
        {
            return Err(Error::Stale);
        }
        for file in &snapshot.files {
            if let Some(bytes) = &file.index {
                let (mode, object) = actual.get(&file.path).ok_or(Error::Stale)?;
                if file.mode.as_ref() != Some(mode)
                    || &self.run(root, &["cat-file", "blob", object], None).await? != bytes
                {
                    return Err(Error::Stale);
                }
            }
            let observed = match root.read(Path::new(&file.path), 64 * 1024 * 1024) {
                Ok(source) => Some(source.bytes),
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error),
            };
            if observed != file.working {
                return Err(Error::Stale);
            }
        }
        Ok(())
    }
    fn verify_inventory(&self, root: &Root, snapshot: &WorkspaceSnapshot) -> Result<()> {
        use std::collections::BTreeSet;
        let mut expected: BTreeSet<String> = snapshot
            .files
            .iter()
            .filter(|file| file.working.is_some())
            .map(|file| file.path.clone())
            .collect();
        expected.insert(".vcp-child-owner".into());
        if snapshot.base_commit.is_some() {
            expected.insert(".git".into());
        }
        let mut pending = vec![PathBuf::new()];
        let mut observed = BTreeSet::new();
        let mut entries = 0usize;
        while let Some(directory) = pending.pop() {
            let _directory = root.hold(
                if directory.as_os_str().is_empty() {
                    None
                } else {
                    Some(&directory)
                },
                true,
            )?;
            for entry in std::fs::read_dir(root.path().join(&directory))? {
                entries += 1;
                if entries > 30_000 {
                    return Err(Error::Limit("child inventory"));
                }
                let entry = entry?;
                let path = directory.join(entry.file_name());
                let name = path::relative(&path)?;
                if entry.file_type()?.is_dir() {
                    let _held = root.hold(Some(&path), true)?;
                    if !expected
                        .iter()
                        .any(|file| file.starts_with(&format!("{name}/")))
                    {
                        return Err(Error::Stale);
                    }
                    pending.push(path);
                } else {
                    let _held = root.hold(Some(&path), false)?;
                    if !expected.contains(&name) {
                        return Err(Error::Stale);
                    }
                    observed.insert(name);
                }
            }
        }
        if observed != expected {
            return Err(Error::Stale);
        }
        Ok(())
    }
    pub(crate) fn check_metadata_registration(
        &self,
        owner: &Root,
        registration: &WorkspaceRegistration,
    ) -> Result<()> {
        let declared = registration
            .git_metadata_owner
            .as_ref()
            .ok_or(Error::Scope(
                "registered Git metadata owner required".into(),
            ))?;
        if declared.identity != owner.identity
            || owner.identity.workspace != registration.source.workspace
            || !declared.absolute_path.is_absolute()
            || std::fs::canonicalize(&declared.absolute_path)?
                != std::fs::canonicalize(owner.path())?
        {
            return Err(Error::Scope(
                "Git metadata owner differs from registration".into(),
            ));
        }
        Ok(())
    }
    pub(crate) fn pin_metadata(&self, source: &Root, child: &Root) -> Result<Vec<path::HeldPath>> {
        let mut guards = vec![child.hold(Some(Path::new(".git")), false)?];
        let pointer = child.read(Path::new(".git"), 32 * 1024)?;
        let pointer = std::str::from_utf8(&pointer.bytes)
            .map_err(|_| Error::Unsupported("worktree metadata encoding"))?
            .trim()
            .strip_prefix("gitdir: ")
            .ok_or(Error::Scope("worktree metadata pointer".into()))?;
        // Validate spelling inside the registered local root before resolving
        // anything. An untrusted UNC pointer must not trigger network lookup.
        let pointer = metadata_path(Path::new(pointer))?;
        let prefix = format!("{}/.git/worktrees/", metadata_path(source.path())?);
        if !pointer
            .get(..prefix.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(&prefix))
        {
            return Err(Error::Scope(
                "worktree metadata outside registered source".into(),
            ));
        }
        let name = &pointer[prefix.len()..];
        if name.contains('/') {
            return Err(Error::Scope(
                "worktree metadata must be an immediate child".into(),
            ));
        }
        path::relative(Path::new(name))?;
        let relative = Path::new(".git/worktrees").join(name);
        guards.push(source.hold(Some(&relative), true)?);
        for name in ["commondir", "gitdir"] {
            guards.push(source.hold(Some(&relative.join(name)), false)?);
        }
        let common = source.read(&relative.join("commondir"), 1024)?;
        if std::str::from_utf8(&common.bytes)
            .map_err(|_| Error::Unsupported("common directory encoding"))?
            .trim()
            != "../.."
        {
            return Err(Error::Scope("external common Git metadata".into()));
        }
        let backlink = source.read(&relative.join("gitdir"), 32 * 1024)?;
        let backlink = std::str::from_utf8(&backlink.bytes)
            .map_err(|_| Error::Unsupported("Git backlink encoding"))?
            .trim();
        if !metadata_path(Path::new(backlink))?
            .eq_ignore_ascii_case(&metadata_path(&child.path().join(".git"))?)
        {
            return Err(Error::Scope("worktree metadata backlink".into()));
        }
        Ok(guards)
    }
    fn write_new(&self, root: &Root, name: &str, bytes: &[u8]) -> Result<()> {
        use std::io::Write;
        let name = path::relative(Path::new(name))?;
        let path = Path::new(&name);
        let mut guards = vec![root.hold(None, true)?];
        if let Some(parent) = path.parent() {
            let mut current = PathBuf::new();
            for component in parent.components() {
                current.push(component);
                match std::fs::create_dir(root.path().join(&current)) {
                    Ok(()) => (),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
                    Err(error) => return Err(error.into()),
                }
                guards.push(root.hold(Some(&current), true)?);
            }
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.path().join(path))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    }
    pub(crate) async fn run(
        &self,
        root: &Root,
        args: &[&str],
        input: Option<&[u8]>,
    ) -> Result<Vec<u8>> {
        let _root = root.hold(None, true)?;
        #[cfg(not(windows))]
        {
            let _ = (args, input);
            Err(Error::Unsupported("native Windows contained Git required"))
        }
        #[cfg(windows)]
        {
            use std::process::Stdio;
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let job = codex_utils_pty::JobObject::create_without_breakaway()?;
            let mut command = tokio::process::Command::new(&self.executable);
            command
                .current_dir(root.path())
                .env_clear()
                .envs(&self.environment)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "NUL")
                .env("GIT_OPTIONAL_LOCKS", "0")
                .env("GIT_TERMINAL_PROMPT", "0")
                .env("GIT_ATTR_NOSYSTEM", "1")
                .env("GIT_NO_REPLACE_OBJECTS", "1")
                .env("GIT_NO_LAZY_FETCH", "1")
                .arg(format!("--work-tree={}", git_path(root.path())?))
                .args([
                    "-c",
                    "core.fsmonitor=false",
                    "-c",
                    "core.untrackedCache=false",
                    "-c",
                    "core.hooksPath=NUL",
                    "-c",
                    "core.attributesFile=NUL",
                    "-c",
                    "diff.external=",
                    "-c",
                    "protocol.allow=never",
                    "-c",
                    "core.autocrlf=false",
                    "-c",
                    "core.safecrlf=false",
                    "-c",
                    "core.longpaths=true",
                ])
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            let mut child = job.spawn_contained(&mut command)?;
            let mut stdin = child.stdin.take().ok_or(Error::Unsupported("Git stdin"))?;
            let mut stdout = child
                .stdout
                .take()
                .ok_or(Error::Unsupported("Git stdout"))?
                .take(self.output_limit as u64 + 1);
            let mut stderr = child
                .stderr
                .take()
                .ok_or(Error::Unsupported("Git stderr"))?
                .take(65_537);
            let result = tokio::time::timeout(self.timeout, async {
                let write = async {
                    if let Some(bytes) = input {
                        stdin.write_all(bytes).await?;
                    }
                    drop(stdin);
                    std::io::Result::Ok(())
                };
                let mut out = Vec::new();
                let mut err = Vec::new();
                let (written, read, errors, status) = tokio::join!(
                    write,
                    stdout.read_to_end(&mut out),
                    stderr.read_to_end(&mut err),
                    child.wait()
                );
                written?;
                read?;
                errors?;
                if out.len() > self.output_limit || err.len() > 65_536 {
                    return Err(Error::Limit("Git output"));
                }
                if !status?.success() {
                    return Err(Error::Git(format!(
                        "child workspace {} failed",
                        args.first().unwrap_or(&"operation")
                    )));
                }
                Ok(out)
            })
            .await;
            let _ = job.terminate();
            result.map_err(|_| Error::Limit("Git deadline"))?
        }
    }
}
