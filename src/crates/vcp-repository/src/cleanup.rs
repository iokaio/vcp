// SPDX-License-Identifier: Apache-2.0
//! Exact disposable-root removal, not lifecycle authorization. The host must
//! retain results, exclude live/recovery references, and durably commit the
//! returned intent before removal. It must retain that intent until canonical
//! acknowledgement, including after partial failure or process exit.
//!
//! Common Git metadata is deliberately retained: it can contain child history
//! and objects still needed by siblings. This boundary never prunes it.
use crate::{path::native, worktree::WorkspaceRegistration, *};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::Path,
};
use windows_sys::Win32::Storage::FileSystem::*;

const MAX_ENTRIES: usize = 30_000;
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupEntry {
    pub path: String,
    pub native_identity: String,
    pub directory: bool,
    pub sha256: Option<String>,
    pub bytes: u64,
}

/// Trusted canonical state, never a child-supplied deletion manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupIntent {
    pub registration: WorkspaceRegistration,
    pub absolute_path: PathBuf,
    pub native_identity: String,
    pub disposable_parent: worktree::RegisteredRoot,
    pub parent_native_identity: String,
    pub metadata_native_identity: Option<String>,
    pub entries: Vec<CleanupEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupReceipt {
    pub absolute_path: PathBuf,
    pub native_identity: String,
    /// True also when a committed intent is reconciled after root removal.
    pub removed: bool,
    pub git_metadata_retained: bool,
}

struct PinnedEntry {
    observed: CleanupEntry,
    file: File,
}

// DELETE access and denying delete-sharing let removal address the observed
// file object, without reopening a raceable pathname. Files also deny writers.
fn open_delete(path: &Path, directory: bool) -> Result<File> {
    let file = OpenOptions::new()
        .access_mode(
            DELETE
                | if directory {
                    FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES
                } else {
                    FILE_GENERIC_READ
                },
        )
        .share_mode(if directory {
            FILE_SHARE_READ | FILE_SHARE_WRITE
        } else {
            FILE_SHARE_READ
        })
        .custom_flags(
            FILE_FLAG_OPEN_REPARSE_POINT
                | if directory {
                    FILE_FLAG_BACKUP_SEMANTICS
                } else {
                    0
                },
        )
        .open(path)?;
    let info = native::info(&file)?;
    if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(Error::Link);
    }
    if (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory {
        return Err(Error::Stale);
    }
    Ok(file)
}

fn inventory(root: &Path) -> Result<Vec<PinnedEntry>> {
    let mut pending = vec![PathBuf::new()];
    let mut entries = Vec::new();
    let mut total_bytes = 0;
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(root.join(&directory))? {
            if entries.len() >= MAX_ENTRIES {
                return Err(Error::Limit("cleanup inventory"));
            }
            let entry = entry?;
            let path = directory.join(entry.file_name());
            let name = path::relative(&path)?;
            let directory = entry.file_type()?.is_dir();
            let file = open_delete(&root.join(&path), directory)?;
            let bytes = if directory { 0 } else { file.metadata()?.len() };
            if bytes > MAX_FILE_BYTES {
                return Err(Error::Limit("cleanup file bytes"));
            }
            total_bytes += bytes;
            if total_bytes > MAX_TOTAL_BYTES {
                return Err(Error::Limit("cleanup total bytes"));
            }
            let sha256 = if directory {
                pending.push(path);
                None
            } else {
                let (digest, length) =
                    vcp_protocol::digest_reader((&file).take(MAX_FILE_BYTES + 1))?;
                if length != bytes {
                    return Err(Error::Stale);
                }
                Some(digest)
            };
            entries.push(PinnedEntry {
                observed: CleanupEntry {
                    path: name,
                    native_identity: native::identity(&file)?,
                    directory,
                    sha256,
                    bytes,
                },
                file,
            });
        }
    }
    entries.sort_by(|a, b| a.observed.path.cmp(&b.observed.path));
    Ok(entries)
}

fn marker(entries: &mut [PinnedEntry], registration: &WorkspaceRegistration) -> Result<()> {
    let marker = entries
        .iter_mut()
        .find(|e| e.observed.path == ".vcp-child-owner")
        .ok_or(Error::Scope("disposable ownership marker missing".into()))?;
    if marker.observed.directory || marker.observed.bytes > 64 * 1024 {
        return Err(Error::Scope("disposable ownership marker".into()));
    }
    marker.file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    (&marker.file).take(64 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes != vcp_protocol::canonical_bytes(registration)? {
        return Err(Error::Scope("disposable ownership marker".into()));
    }
    Ok(())
}

impl worktree::Snapshotter {
    /// Prepare only after canonical retention and reference eligibility checks.
    /// `expected_native_identity` is the persisted materialization receipt; a
    /// freshly opened replacement directory must not become the trusted root.
    pub fn prepare_cleanup(
        &self,
        parent: &Root,
        child: &Root,
        metadata_owner: Option<&Root>,
        registration: &WorkspaceRegistration,
        expected_native_identity: &str,
    ) -> Result<CleanupIntent> {
        let parent_guard = parent.hold(None, true)?;
        if child.identity != registration.child
            || child.directory_identity != expected_native_identity
            || child.identity.root == parent.identity.root
            || child.identity.root == registration.source.root
            || parent.identity.workspace != child.identity.workspace
            || child.identity.workspace != registration.source.workspace
            || child.identity.repository != registration.source.repository
            || child.identity.worktree == registration.source.worktree
            || child.path().parent() != Some(parent.path())
            || !registration.absolute_path.is_absolute()
        {
            return Err(Error::Scope("registered disposable root required".into()));
        }
        let declared = Root::open(registration.child.clone(), &registration.absolute_path)?;
        if declared.path() != child.path()
            || declared.directory_identity != expected_native_identity
        {
            return Err(Error::Stale);
        }
        let metadata_native_identity = match (&registration.git_metadata_owner, metadata_owner) {
            (Some(_), Some(owner)) => {
                self.check_metadata_registration(owner, registration)?;
                let _metadata = self.pin_metadata(owner, child)?;
                Some(owner.hold(None, true)?.native_identity)
            }
            (None, None) => None,
            _ => {
                return Err(Error::Scope(
                    "registered Git metadata owner required".into(),
                ))
            }
        };
        let root = open_delete(child.path(), true)?;
        if native::identity(&root)? != expected_native_identity {
            return Err(Error::Stale);
        }
        let mut entries = inventory(child.path())?;
        marker(&mut entries, registration)?;
        if registration.git_metadata_owner.is_none()
            && entries
                .iter()
                .any(|e| e.observed.path.eq_ignore_ascii_case(".git"))
        {
            return Err(Error::Scope("unregistered Git metadata".into()));
        }
        Ok(CleanupIntent {
            registration: registration.clone(),
            absolute_path: child.path().to_path_buf(),
            native_identity: expected_native_identity.into(),
            disposable_parent: worktree::RegisteredRoot {
                identity: parent.identity.clone(),
                absolute_path: parent.path().to_path_buf(),
            },
            parent_native_identity: parent_guard.native_identity,
            metadata_native_identity,
            entries: entries.into_iter().map(|e| e.observed).collect(),
        })
    }
}

fn dispose(file: &File) -> Result<()> {
    let info = FILE_DISPOSITION_INFO { DeleteFile: true };
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle().cast(),
            FileDispositionInfo,
            (&info as *const FILE_DISPOSITION_INFO).cast(),
            std::mem::size_of_val(&info) as u32,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

/// Execute/reconcile a durably committed intent under the canonical host's
/// exclusive cleanup lease. Missing original entries are allowed after partial
/// removal; substituted, changed, or newly added entries always block retry.
/// Errors may follow partial removal: preserve the same intent and diagnose.
pub fn remove_cleanup(
    parent: &Root,
    metadata_owner: Option<&Root>,
    intent: &CleanupIntent,
) -> Result<CleanupReceipt> {
    remove_cleanup_with_gate(parent, metadata_owner, intent, || Ok(()))
}

/// The host retains its admission guard across each native deletion. A pause
/// between entries returns an error without replacing the committed intent.
pub fn remove_cleanup_with_gate<G>(
    parent: &Root,
    metadata_owner: Option<&Root>,
    intent: &CleanupIntent,
    gate: impl Fn() -> Result<G>,
) -> Result<CleanupReceipt> {
    drop(gate()?);
    let parent_guard = parent.hold(None, true)?;
    if parent.identity != intent.disposable_parent.identity
        || parent.path() != intent.disposable_parent.absolute_path
        || parent_guard.native_identity != intent.parent_native_identity
        || intent.absolute_path.parent() != Some(parent.path())
        || intent.registration.child.root == parent.identity.root
        || intent.registration.child.root == intent.registration.source.root
        || intent.entries.len() > MAX_ENTRIES
    {
        return Err(Error::Scope("cleanup parent binding".into()));
    }
    path::relative(Path::new(intent.absolute_path.file_name().ok_or(
        Error::Scope("cleanup root must be an immediate named child".into()),
    )?))?;
    let _metadata = match (
        &intent.registration.git_metadata_owner,
        metadata_owner,
        &intent.metadata_native_identity,
    ) {
        (Some(expected), Some(owner), Some(native_identity)) => {
            let held = owner.hold(None, true)?;
            let declared = Root::open(expected.identity.clone(), &expected.absolute_path)?;
            if owner.identity != expected.identity
                || owner.path() != declared.path()
                || declared.directory_identity != *native_identity
                || &held.native_identity != native_identity
                || owner.path().starts_with(&intent.absolute_path)
            {
                return Err(Error::Stale);
            }
            Some(held)
        }
        (None, None, None) => None,
        _ => return Err(Error::Scope("cleanup Git metadata binding".into())),
    };
    let receipt = CleanupReceipt {
        absolute_path: intent.absolute_path.clone(),
        native_identity: intent.native_identity.clone(),
        removed: true,
        git_metadata_retained: intent.metadata_native_identity.is_some(),
    };
    let root = match open_delete(&intent.absolute_path, true) {
        Ok(root) => root,
        Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => return Ok(receipt),
        Err(e) => return Err(e),
    };
    if native::identity(&root)? != intent.native_identity {
        return Err(Error::Stale);
    }
    let expected: BTreeMap<_, _> = intent
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e))
        .collect();
    if expected.len() != intent.entries.len() || !expected.contains_key(".vcp-child-owner") {
        return Err(Error::Scope("cleanup inventory binding".into()));
    }
    let mut entries = inventory(&intent.absolute_path)?;
    for entry in &entries {
        if expected.get(entry.observed.path.as_str()).copied() != Some(&entry.observed) {
            return Err(Error::Stale);
        }
    }
    // The marker is last. Its absence is only reconcilable for an empty root
    // after a previously committed intent removed it but not the directory.
    if !entries.is_empty() {
        marker(&mut entries, &intent.registration)?;
    }
    entries.sort_by(|a, b| {
        let key = |entry: &PinnedEntry| {
            (
                entry.observed.path != ".vcp-child-owner",
                entry.observed.path.matches('/').count(),
            )
        };
        key(b).cmp(&key(a))
    });
    for entry in entries {
        let _admission = gate()?;
        dispose(&entry.file)?;
        drop(entry);
    }
    // A concurrent new entry causes directory removal to fail, never recursive
    // discovery/deletion outside the committed inventory.
    let _admission = gate()?;
    dispose(&root)?;
    drop(root);
    Ok(receipt)
}
