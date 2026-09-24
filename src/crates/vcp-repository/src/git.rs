// SPDX-License-Identifier: Apache-2.0
use crate::*;
use std::{
    collections::BTreeMap, ffi::OsString, io::Read, path::Path, process::Stdio, time::Duration,
};
use tokio::io::AsyncReadExt;

pub struct Git {
    executable: PathBuf,
    _executable_pin: Option<path::HeldPath>,
    environment: BTreeMap<OsString, OsString>,
    timeout: Duration,
    output_limit: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub path: String,
    pub original: Option<String>,
    pub index: char,
    pub worktree: char,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub identity: RootIdentity,
    pub head: Option<String>,
    pub changes: Vec<Change>,
    pub index_sha256: String,
    pub status_sha256: String,
    pub staged_diff_sha256: String,
    pub unstaged_diff_sha256: String,
    pub digest: String,
}
pub struct Observation {
    pub snapshot: Snapshot,
    pub status: Vec<u8>,
    pub index: Vec<u8>,
    pub staged_diff: Vec<u8>,
    pub unstaged_diff: Vec<u8>,
}
impl Git {
    pub fn new(
        executable: PathBuf,
        environment: BTreeMap<OsString, OsString>,
        timeout: Duration,
        output_limit: usize,
    ) -> Result<Self> {
        if !executable.is_absolute()
            || !executable.is_file()
            || timeout.is_zero()
            || timeout > Duration::from_secs(60)
            || output_limit == 0
            || output_limit > 8 * 1024 * 1024
        {
            return Err(Error::Limit("Git configuration"));
        }
        if environment.keys().any(|key| {
            !key.to_str().is_some_and(|key| {
                ["path", "systemroot", "windir", "temp", "tmp"]
                    .contains(&key.to_ascii_lowercase().as_str())
            })
        }) {
            return Err(Error::Scope(
                "Git environment must be explicit and credential-free".into(),
            ));
        }
        Ok(Self {
            executable,
            _executable_pin: None,
            environment,
            timeout,
            output_limit,
        })
    }
    /// Bind an explicitly selected native executable to this capability's full
    /// lifetime, including detached observations after its original loader exits.
    #[cfg(windows)]
    pub fn with_executable_pin(mut self, pin: path::HeldPath) -> Result<Self> {
        let current = path::native::open(&self.executable, false)?;
        if path::native::identity(&current)? != pin.native_identity
            || path::native::final_path(&current)? != path::native::final_path(&pin.file)?
            || path::native::info(&pin.file)?.nNumberOfLinks != 1
        {
            return Err(Error::Stale);
        }
        self.executable = path::native::final_path(&pin.file)?;
        self._executable_pin = Some(pin);
        Ok(self)
    }
    async fn run(&self, root: &Root, args: &[&str]) -> Result<(bool, Vec<u8>)> {
        let _held = root.hold(None, true)?;
        #[cfg(not(windows))]
        return Err(Error::Unsupported(
            "native contained Git observation required",
        ));
        #[cfg(windows)]
        {
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
                .args([
                    "--no-optional-locks",
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
                    "status.relativePaths=false",
                    "-c",
                    "protocol.allow=never",
                ])
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            let mut child = job.spawn_contained(&mut command)?;
            let mut stdout = child
                .stdout
                .take()
                .unwrap()
                .take(self.output_limit as u64 + 1);
            let mut stderr = child.stderr.take().unwrap().take(65_537);
            let result = tokio::time::timeout(self.timeout, async {
                let mut out = Vec::new();
                let mut err = Vec::new();
                let (out_result, err_result, status) = tokio::join!(
                    stdout.read_to_end(&mut out),
                    stderr.read_to_end(&mut err),
                    child.wait()
                );
                out_result?;
                err_result?;
                if out.len() > self.output_limit || err.len() > 65_536 {
                    return Err(Error::Limit("Git output"));
                }
                Ok((status?.success(), out))
            })
            .await;
            // Also terminates any helper left alive after the main command.
            let _ = job.terminate();
            result.map_err(|_| Error::Limit("Git deadline"))?
        }
    }
    async fn required(&self, root: &Root, args: &[&str]) -> Result<Vec<u8>> {
        let (success, bytes) = self.run(root, args).await?;
        if !success {
            return Err(Error::Git(format!(
                "read-only {} failed",
                args.first().unwrap_or(&"command")
            )));
        }
        Ok(bytes)
    }
    pub async fn observe(&self, root: &Root) -> Result<Observation> {
        // Hold the local configuration while interpreting it and invoking Git.
        // Linked-worktree and alternate-object roots require a separately
        // registered metadata scope; this first reader rejects those layouts.
        let _config = root.hold(Some(Path::new(".git/config")), false)?;
        let mut config = Vec::new();
        (&_config.file).take(65_537).read_to_end(&mut config)?;
        if config.len() > 65_536 {
            return Err(Error::Limit("Git configuration bytes"));
        }
        let config = std::str::from_utf8(&config)
            .map_err(|_| Error::Unsupported("Git configuration encoding"))?;
        for line in config.trim_start_matches('\u{feff}').lines() {
            if let Some(section) = line.trim_start().strip_prefix('[') {
                let name = section
                    .trim_start()
                    .split(|c: char| c.is_ascii_whitespace() || c == ']' || c == '.')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if matches!(
                    name.as_str(),
                    "include" | "includeif" | "filter" | "extensions"
                ) {
                    return Err(Error::Unsupported("Git includes, executable filters or extension metadata need a qualified host scope"));
                }
            }
        }
        for metadata in [".git/commondir", ".git/objects/info/alternates"] {
            match root.hold(Some(Path::new(metadata)), false) {
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => (),
                _ => {
                    return Err(Error::Unsupported(
                        "external Git metadata root requires explicit scope",
                    ))
                }
            }
        }
        let top = self
            .required(root, &["rev-parse", "--show-toplevel"])
            .await?;
        let top = std::str::from_utf8(&top)
            .map_err(|_| Error::Unsupported("Git root encoding"))?
            .trim();
        if std::fs::canonicalize(top)? != std::fs::canonicalize(root.path())? {
            return Err(Error::Scope(
                "registered root must be the Git worktree root".into(),
            ));
        }
        let status_args = [
            "status",
            "--porcelain=v2",
            "-z",
            "--untracked-files=all",
            "--ignored=no",
        ];
        let index = self.required(root, &["ls-files", "--stage", "-z"]).await?;
        let mut tracked = Vec::new();
        let index_text = std::str::from_utf8(&index)
            .map_err(|_| Error::Unsupported("Git index path encoding"))?;
        for (position, entry) in index_text.split_terminator('\0').enumerate() {
            if position == 10_000 {
                return Err(Error::Limit("tracked file observations"));
            }
            let (_, path) = entry
                .split_once('\t')
                .ok_or(Error::Git("invalid index entry".into()))?;
            match root.hold(Some(Path::new(path)), false) {
                Ok(held) => tracked.push(held),
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error),
            }
        }
        let status = self.required(root, &status_args).await?;
        let (has_head, head) = self.run(root, &["rev-parse", "--verify", "HEAD"]).await?;
        let head = if has_head {
            let value = std::str::from_utf8(&head)
                .map_err(|_| Error::Git("HEAD encoding".into()))?
                .trim();
            if !matches!(value.len(), 40 | 64)
                || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(Error::Git("HEAD identity".into()));
            }
            Some(value.into())
        } else {
            None
        };
        let staged_diff = self
            .required(
                root,
                &[
                    "diff",
                    "--cached",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--binary",
                    "--",
                ],
            )
            .await?;
        let unstaged_diff = self
            .required(
                root,
                &["diff", "--no-ext-diff", "--no-textconv", "--binary", "--"],
            )
            .await?;
        let (final_has_head, final_head) =
            self.run(root, &["rev-parse", "--verify", "HEAD"]).await?;
        if self.required(root, &status_args).await? != status
            || self.required(root, &["ls-files", "--stage", "-z"]).await? != index
            || final_has_head != has_head
            || (has_head
                && std::str::from_utf8(&final_head)
                    .map_err(|_| Error::Git("HEAD encoding".into()))?
                    .trim()
                    != head.as_deref().unwrap())
            || self
                .required(
                    root,
                    &[
                        "diff",
                        "--cached",
                        "--no-ext-diff",
                        "--no-textconv",
                        "--binary",
                        "--",
                    ],
                )
                .await?
                != staged_diff
            || self
                .required(
                    root,
                    &["diff", "--no-ext-diff", "--no-textconv", "--binary", "--"],
                )
                .await?
                != unstaged_diff
        {
            return Err(Error::Stale);
        }
        let mut snapshot = Snapshot {
            identity: root.identity.clone(),
            head,
            changes: parse_status(&status)?,
            index_sha256: vcp_protocol::digest_bytes(&index),
            status_sha256: vcp_protocol::digest_bytes(&status),
            staged_diff_sha256: vcp_protocol::digest_bytes(&staged_diff),
            unstaged_diff_sha256: vcp_protocol::digest_bytes(&unstaged_diff),
            digest: String::new(),
        };
        snapshot.digest = vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&snapshot)?);
        Ok(Observation {
            snapshot,
            status,
            index,
            staged_diff,
            unstaged_diff,
        })
    }
}
pub fn parse_status(bytes: &[u8]) -> Result<Vec<Change>> {
    if !bytes.is_empty() && bytes.last() != Some(&0) {
        return Err(Error::Git("truncated status".into()));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| Error::Unsupported("Git path encoding"))?;
    let mut records = text.split_terminator('\0');
    let mut result = Vec::new();
    while let Some(record) = records.next() {
        let (path, original, index, worktree) = if let Some(path) = record.strip_prefix("? ") {
            (path, None, '?', '?')
        } else {
            let count = match record.as_bytes().first() {
                Some(b'1') => 9,
                Some(b'2') => 10,
                Some(b'u') => 11,
                _ => return Err(Error::Git("unknown status record".into())),
            };
            let fields: Vec<_> = record.splitn(count, ' ').collect();
            if fields.len() != count || fields[1].len() != 2 {
                return Err(Error::Git("invalid status fields".into()));
            }
            let original = if count == 10 {
                Some(
                    records
                        .next()
                        .ok_or(Error::Git("rename source missing".into()))?,
                )
            } else {
                None
            };
            (
                fields[count - 1],
                original,
                fields[1].as_bytes()[0] as char,
                fields[1].as_bytes()[1] as char,
            )
        };
        let path = path::relative(Path::new(path))?;
        let original = original
            .map(|path| path::relative(Path::new(path)))
            .transpose()?;
        result.push(Change {
            path,
            original,
            index,
            worktree,
        });
    }
    result.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}

#[cfg(all(test, windows))]
mod pin_tests {
    use super::*;
    fn root(path: &Path) -> Root {
        Root::open(
            RootIdentity {
                workspace: WorkspaceId::parse("pin-workspace").unwrap(),
                root: RootId::parse("pin-root").unwrap(),
                repository: "fixture".into(),
                worktree: "fixture".into(),
                binding: Revision::ZERO,
            },
            path,
        )
        .unwrap()
    }
    #[test]
    fn executable_pin_follows_git_lifetime_and_rejects_other_identity_or_alias() {
        let temporary = tempfile::tempdir().unwrap();
        let directory = temporary.path().canonicalize().unwrap();
        let executable = directory.join("git.exe");
        let other = directory.join("other.exe");
        std::fs::write(&executable, b"not executed").unwrap();
        std::fs::write(&other, b"not executed").unwrap();
        let root = root(&directory);
        let git = Git::new(
            executable.clone(),
            BTreeMap::new(),
            Duration::from_secs(1),
            1024,
        )
        .unwrap();
        assert!(git
            .with_executable_pin(root.hold(Some(Path::new("other.exe")), false).unwrap())
            .is_err());
        let git = Git::new(
            executable.clone(),
            BTreeMap::new(),
            Duration::from_secs(1),
            1024,
        )
        .unwrap()
        .with_executable_pin(root.hold(Some(Path::new("git.exe")), false).unwrap())
        .unwrap();
        let owner = std::sync::Arc::new(git);
        let background = owner.clone();
        drop(owner);
        assert!(std::fs::write(&executable, b"replacement").is_err());
        assert!(std::fs::rename(&executable, directory.join("moved.exe")).is_err());
        drop(background);
        std::fs::write(&executable, b"replacement").unwrap();
        let alias = directory.join("alias.exe");
        std::fs::hard_link(&executable, &alias).unwrap();
        let git = Git::new(
            executable.clone(),
            BTreeMap::new(),
            Duration::from_secs(1),
            1024,
        )
        .unwrap();
        assert!(git
            .with_executable_pin(root.hold(Some(Path::new("git.exe")), false).unwrap())
            .is_err());
    }
}
