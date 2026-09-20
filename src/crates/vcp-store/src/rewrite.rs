// SPDX-License-Identifier: Apache-2.0
//! Sealed activation into an owned child. Old content is pending cleanup, never
//! silently deleted by activation. Local owner/path trust remains required.
use crate::{
    artifact::{read_bounded, reject_link},
    BackendKind, Error, Result,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};
use vcp_domain::{TransactionId, Watermark};
use vcp_protocol::{canonical_bytes, digest_bytes};

pub(crate) const MAX_REWRITES: usize = 256;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RewriteReceipt {
    pub version: u32,
    pub generation: u64,
    pub root: TransactionId,
    pub backend: BackendKind,
    pub watermark: Watermark,
    pub source_digest: String,
    pub state_digest: String,
    pub previous: String,
    /// Owned roots remain explicit local retention obligations after activation.
    pub pending_roots: Vec<String>,
}
pub(crate) fn receipts(root: &Path) -> Result<Vec<RewriteReceipt>> {
    let mut paths = fs::read_dir(root)?
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .filter(|e| {
            e.file_name().to_string_lossy().starts_with("rewrite-")
                && e.path().extension().is_some_and(|e| e == "json")
        })
        .map(|e| e.path())
        .collect::<Vec<_>>();
    paths.sort();
    if paths.len() > MAX_REWRITES {
        return Err(Error::Limit("rewrite activation count"));
    }
    let mut chain = "0".repeat(64);
    let mut result = Vec::<RewriteReceipt>::new();
    let mut expected_pending = vec!["anchor".to_owned()];
    let mut root_ids = std::collections::BTreeSet::new();
    for (i, path) in paths.iter().enumerate() {
        let bytes = read_bounded(path, 64 * 1024)?;
        let value: RewriteReceipt = serde_json::from_slice(&bytes)?;
        if value.version != 1
            || value.generation != i as u64
            || value.previous != chain
            || path.file_name().and_then(|n| n.to_str())
                != Some(format!("rewrite-{i:020}.json").as_str())
            || canonical_bytes(&value)? != bytes
        {
            return Err(Error::Corruption("rewrite activation chain"));
        }
        let hash = |s: &str| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        };
        if !hash(&value.source_digest)
            || !hash(&value.state_digest)
            || value.pending_roots != expected_pending
            || !root_ids.insert(value.root.clone())
            || result.last().is_some_and(|previous| {
                previous.watermark > value.watermark || previous.backend != value.backend
            })
        {
            return Err(Error::Corruption("rewrite receipt identity"));
        }
        expected_pending.push(value.root.to_string());
        chain = digest_bytes(&bytes);
        result.push(value);
    }
    Ok(result)
}
pub(crate) fn resolve(root: &Path) -> Result<Option<(PathBuf, RewriteReceipt)>> {
    let Some(receipt) = receipts(root)?.pop() else {
        return Ok(None);
    };
    // A rewrite child is a leaf; arbitrary nested activation is not accepted.
    if root
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|n| n == ".replay-roots")
    {
        return Err(Error::Corruption("nested rewrite anchor"));
    }
    let children = root.join(".replay-roots");
    reject_link(&children)?;
    let path = children.join(receipt.root.as_str());
    reject_link(&path)?;
    if !path.is_dir() || path.canonicalize()?.parent() != Some(children.canonicalize()?.as_path()) {
        return Err(Error::Access);
    }
    Ok(Some((path, receipt)))
}
/// Enumerates only recognized content-bearing files in one retired physical
/// root. Routing records, root locks and contained replacement roots are kept.
/// Unknown entries fail closed. This is an inventory, never deletion authority.
fn partial_stem(name: &str) -> Option<&str> {
    let (stem, nonce) = name.strip_suffix(".partial")?.rsplit_once('.')?;
    if nonce.len() != 36
        || !nonce.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        return None;
    }
    Some(stem)
}
pub fn owned_files(root: &Path) -> Result<Vec<PathBuf>> {
    reject_link(root)?;
    let mut files = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        reject_link(&path)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type()?.is_dir() {
            if matches!(name.as_str(), ".replay-roots" | "search-generations") {
                continue;
            }
            if name != "spool" {
                return Err(Error::Conflict("unrecognized retired root directory"));
            }
            for object in fs::read_dir(&path)? {
                let object = object?;
                reject_link(&object.path())?;
                if !object.file_type()?.is_dir() {
                    return Err(Error::Corruption("retired spool object"));
                }
                vcp_domain::ArtifactId::parse(object.file_name().to_string_lossy().into_owned())?;
                for payload in fs::read_dir(object.path())? {
                    let payload = payload?;
                    reject_link(&payload.path())?;
                    let name = payload.file_name().to_string_lossy().into_owned();
                    let chunk = name
                        .strip_suffix(".chunk")
                        .and_then(|s| s.split_once('-'))
                        .is_some_and(|(n, h)| {
                            n.len() == 20
                                && n.bytes().all(|b| b.is_ascii_digit())
                                && h.len() == 64
                                && h.bytes()
                                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                        });
                    let partial = partial_stem(&name).is_some_and(|stem| {
                        matches!(stem, "spec" | "seal")
                            || stem.split_once('-').is_some_and(|(n, h)| {
                                n.len() == 20
                                    && n.bytes().all(|b| b.is_ascii_digit())
                                    && h.len() == 64
                                    && h.bytes().all(|b| b.is_ascii_hexdigit())
                            })
                    });
                    if !payload.file_type()?.is_file()
                        || !(partial
                            || chunk
                            || matches!(
                                name.as_str(),
                                "spec.json" | "seal.json" | "owner.lock" | "snapshot.lock"
                            ))
                    {
                        return Err(Error::Conflict("unrecognized retired spool file"));
                    }
                    if !matches!(name.as_str(), "owner.lock" | "snapshot.lock") {
                        files.push(payload.path());
                    }
                    if files.len() > 100_000 {
                        return Err(Error::Limit("retired file inventory"));
                    }
                }
            }
        } else if entry.file_type()?.is_file() {
            if matches!(
                name.as_str(),
                "owner.lock" | "root-snapshot.lock" | "retired.json" | "private-rewrite.json"
            ) || (name.starts_with("rewrite-") && name.ends_with(".json"))
            {
                continue;
            }
            let recognized = matches!(
                name.as_str(),
                "format.json"
                    | "canonical.sqlite"
                    | "canonical.sqlite-wal"
                    | "canonical.sqlite-shm"
                    | "canonical.frames"
                    | "replay-base.json"
                    | "replay-base.seal"
                    | "conversion.json"
            ) || (name.starts_with("checkpoint-")
                && (name.ends_with(".json") || name.ends_with(".active")))
                || (name.starts_with("commit-") && name.ends_with(".tip"))
                || (name.starts_with("torn-tail-") && name.ends_with(".bin"));
            let partial = partial_stem(&name).is_some_and(|stem| {
                matches!(
                    stem,
                    "format" | "replay-base" | "conversion" | "retired" | "private-rewrite"
                ) || stem.starts_with("checkpoint-")
                    || stem.starts_with("commit-")
            });
            if !recognized && !partial {
                return Err(Error::Conflict("unrecognized retired root file"));
            }
            files.push(path);
        } else {
            return Err(Error::Access);
        }
        if files.len() > 100_000 {
            return Err(Error::Limit("retired file inventory"));
        }
    }
    files.sort();
    Ok(files)
}

/// Outcomes contain only root identities and reason categories, never payloads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Cleanup {
    pub completed: Vec<String>,
    pub pinned: Vec<String>,
    pub failed: Vec<String>,
    pub removed_bytes: u64,
}
/// Only roots named by the sealed activation chain are eligible. Retained lock
/// inodes keep late readers from acquiring an obsolete lock after cleanup.
fn pin_directory(path: &Path) -> Result<File> {
    reject_link(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .share_mode(0x1 | 0x2);
    }
    let handle = options.open(path)?;
    if !handle.metadata()?.is_dir() {
        return Err(Error::Access);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if handle.metadata()?.file_attributes() & 0x400 != 0 {
            return Err(Error::Access);
        }
    }
    Ok(handle)
}
pub(crate) fn cleanup(
    anchor: &Path,
    active: &Path,
    barrier: &dyn Fn(crate::Barrier),
) -> Result<Cleanup> {
    let _anchor = pin_directory(anchor)?;

    let Some(latest) = receipts(anchor)?.pop() else {
        return Ok(Cleanup::default());
    };
    let _children = pin_directory(&anchor.join(".replay-roots"))?;
    let mut result = Cleanup::default();
    let mut pending: BTreeSet<String> = latest.pending_roots.iter().cloned().collect();
    for entry in fs::read_dir(anchor.join(".replay-roots"))? {
        let entry = entry?;
        reject_link(&entry.path())?;
        if !entry.file_type()?.is_dir() {
            return Err(Error::Conflict("unexpected rewrite child"));
        }
        let identity = entry.file_name().to_string_lossy().into_owned();
        let id = vcp_domain::TransactionId::parse(identity.clone())?;
        if entry.path().canonicalize()? == active.canonicalize()? {
            continue;
        }
        if !pending.contains(&identity) {
            let marker = entry.path().join("private-rewrite.json");
            if !marker.exists() {
                result.failed.push(identity);
                continue;
            }
            let proof: serde_json::Value = serde_json::from_slice(&read_bounded(&marker, 4096)?)?;
            if proof["version"] != 1
                || proof["root"] != id.as_str()
                || !proof["source"]
                    .as_str()
                    .is_some_and(vcp_domain::accounting::valid_hash)
            {
                return Err(Error::Corruption("rewrite staging ownership"));
            }
            pending.insert(identity);
        }
        if pending.len() > MAX_REWRITES * 2 {
            return Err(Error::Limit("retired rewrite roots"));
        }
    }
    for identity in &pending {
        let root = if identity == "anchor" {
            anchor.to_path_buf()
        } else {
            anchor
                .join(".replay-roots")
                .join(vcp_domain::TransactionId::parse(identity.clone())?.as_str())
        };
        let _root = pin_directory(&root)?;
        if root.canonicalize()? == active.canonicalize()? {
            return Err(Error::Access);
        }
        let _orphan_owner = if !latest.pending_roots.contains(identity) {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(root.join("owner.lock"))?;
            match file.try_lock() {
                Ok(()) => Some(file),
                Err(std::fs::TryLockError::WouldBlock) => {
                    result.pinned.push(identity.clone());
                    continue;
                }
                Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
            }
        } else {
            None
        };
        let Some(_pin) = crate::store::snapshot_pin::cleanup(&root)? else {
            result.pinned.push(identity.clone());
            continue;
        };
        let files = owned_files(&root)?;
        let directories: BTreeSet<_> = files
            .iter()
            .filter_map(|file| {
                let parent = file.parent()?;
                (parent.parent() == Some(root.join("spool").as_path()))
                    .then(|| parent.to_path_buf())
            })
            .collect();
        let mut leases = Vec::new();
        let mut directory_pins = Vec::new();
        if root.join("spool").exists() {
            directory_pins.push(pin_directory(&root.join("spool"))?);
        }
        let mut held = false;
        for directory in directories {
            directory_pins.push(pin_directory(&directory)?);
            for name in ["owner.lock", "snapshot.lock"] {
                let path = directory.join(name);
                reject_link(&path)?;
                let lease = OpenOptions::new().read(true).write(true).open(path)?;
                match lease.try_lock() {
                    Ok(()) => leases.push(lease),
                    Err(std::fs::TryLockError::WouldBlock) => {
                        held = true;
                        break;
                    }
                    Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
                }
            }
            if held {
                break;
            }
        }
        if held {
            result.pinned.push(identity.clone());
            continue;
        }
        // Explicitly prevent a direct retired-child reopen from resurrecting or
        // initializing a root after its format/journal files have disappeared.
        let marker = root.join("retired.json");
        if !marker.exists() {
            crate::artifact::immutable_file(
                &marker,
                &canonical_bytes(
                    &serde_json::json!({"version":1,"activation":latest.root,"retired":identity}),
                )?,
            )?;
        }
        let mut failed = false;
        for file in files {
            reject_link(&file)?;
            let size = fs::metadata(&file)?.len();
            barrier(crate::Barrier::BeforeCleanupFile);
            if fs::remove_file(&file).is_err() {
                failed = true;
                break;
            }
            result.removed_bytes = result.removed_bytes.saturating_add(size);
            barrier(crate::Barrier::AfterCleanupFile);
        }
        if failed {
            result.failed.push(identity.clone());
        } else {
            result.completed.push(identity.clone());
        }
    }
    Ok(result)
}
