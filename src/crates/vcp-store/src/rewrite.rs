// SPDX-License-Identifier: Apache-2.0
//! Sealed activation into an owned child. Old content is pending cleanup, never
//! silently deleted by activation. Local owner/path trust remains required.
use crate::{
    artifact::{read_bounded, reject_link},
    BackendKind, Error, Result,
};
use serde::{Deserialize, Serialize};
use std::{
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
pub(crate) fn pin(root: &Path) -> Result<File> {
    let path = root.join("root-snapshot.lock");
    if path.exists() {
        reject_link(&path)?;
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock_shared()
        .map_err(|_| Error::Conflict("root cleanup in progress"))?;
    Ok(file)
}
/// Enumerates only recognized content-bearing files in one retired physical
/// root. Routing records, root locks and contained replacement roots are kept.
/// Unknown entries fail closed. This is an inventory, never deletion authority.
pub fn owned_files(root: &Path) -> Result<Vec<PathBuf>> {
    reject_link(root)?;
    let mut files = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        reject_link(&path)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type()?.is_dir() {
            if name == ".replay-roots" {
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
                    if !payload.file_type()?.is_file()
                        || !(chunk
                            || matches!(
                                name.as_str(),
                                "spec.json" | "seal.json" | "owner.lock" | "snapshot.lock"
                            ))
                    {
                        return Err(Error::Conflict("unrecognized retired spool file"));
                    }
                    files.push(payload.path());
                    if files.len() > 100_000 {
                        return Err(Error::Limit("retired file inventory"));
                    }
                }
            }
        } else if entry.file_type()?.is_file() {
            if matches!(name.as_str(), "owner.lock" | "root-snapshot.lock")
                || (name.starts_with("rewrite-") && name.ends_with(".json"))
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
            if !recognized {
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
