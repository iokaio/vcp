// SPDX-License-Identifier: Apache-2.0
//! Native layout selection. Publishing layout 3 replaces the old marker so an
//! older executable rejects it before it can append a legacy frame.
use crate::{
    artifact::{immutable_file, read_bounded, sync_directory},
    backend::current_publication::open::Opened,
    canonical_lock::CanonicalLock,
    private_paths::Directory,
    BackendKind, Error, Result,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use vcp_protocol::{canonical_bytes, digest_bytes};

const MAX_MARKER: usize = 4096;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Format {
    version: u32,
    backend: BackendKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    origin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    legacy: Option<String>,
}

pub(crate) enum Selection {
    Legacy(Legacy),
    Current { origin: String },
}
pub(crate) struct Legacy {
    bytes: Vec<u8>,
    backend: BackendKind,
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn legacy_name(digest: &str) -> String {
    format!("format-legacy-{digest}.json")
}
fn decode_legacy(root: &Path, kind: BackendKind, bytes: Vec<u8>) -> Result<Legacy> {
    let format: Format = serde_json::from_slice(&bytes)?;
    let expected = if root.join("replay-base.json").exists() {
        2
    } else {
        1
    };
    if format.version != expected
        || format.backend != kind
        || format.origin.is_some()
        || format.legacy.is_some()
    {
        return Err(Error::Incompatible);
    }
    Ok(Legacy {
        bytes,
        backend: kind,
    })
}
pub(crate) fn read(lock: &CanonicalLock, kind: BackendKind) -> Result<Selection> {
    lock.verify()?;
    let bytes = read_bounded(&lock.root().join("format.json"), MAX_MARKER)?;
    let format: Format = serde_json::from_slice(&bytes)?;
    if format.version != 3 {
        return Ok(Selection::Legacy(decode_legacy(lock.root(), kind, bytes)?));
    }
    if format.backend != kind || canonical_bytes(&format)? != bytes {
        return Err(Error::Incompatible);
    }
    let origin = format
        .origin
        .as_deref()
        .ok_or(Error::Corruption("format origin missing"))?;
    let legacy = format
        .legacy
        .as_deref()
        .ok_or(Error::Corruption("format legacy marker missing"))?;
    if !valid_digest(origin) || !valid_digest(legacy) {
        return Err(Error::Corruption("format commitment"));
    }
    let original = read_bounded(&lock.root().join(legacy_name(legacy)), MAX_MARKER)?;
    if digest_bytes(&original) != legacy {
        return Err(Error::Corruption("format original marker differs"));
    }
    decode_legacy(lock.root(), kind, original)?;
    Ok(Selection::Current {
        origin: origin.to_owned(),
    })
}

/// The owner is already admitted by full native replay. Its origin commitment
/// cannot be supplied independently from the actual held canonical lock.
pub(crate) fn publish(owner: &Opened, legacy: &Legacy) -> Result<()> {
    publish_observed(owner, legacy, &|_| Ok(()))
}
pub(crate) fn publish_observed(
    owner: &Opened,
    legacy: &Legacy,
    observe: &impl Fn(crate::Barrier) -> Result<()>,
) -> Result<()> {
    owner.ensure_healthy()?;
    let lock = owner.canonical_lock();
    let _root = Directory::locked_root(lock)?;
    if owner.kind() != legacy.backend || !valid_digest(&owner.origin_digest) {
        return Err(Error::Corruption("format owner differs"));
    }
    let digest = digest_bytes(&legacy.bytes);
    let bytes = canonical_bytes(&Format {
        version: 3,
        backend: legacy.backend,
        origin: Some(owner.origin_digest.clone()),
        legacy: Some(digest.clone()),
    })?;
    let target = lock.root().join("format.json");
    lock.verify_child(&target)?;
    let prior = read_bounded(&target, MAX_MARKER)?;
    if prior == bytes {
        read(lock, owner.kind())?;
        return Ok(());
    }
    if prior != legacy.bytes {
        return Err(Error::Corruption("format changed before publication"));
    }
    let backup = lock.root().join(legacy_name(&digest));
    lock.verify_child(&backup)?;
    if backup.exists() {
        if read_bounded(&backup, MAX_MARKER)? != legacy.bytes {
            return Err(Error::Corruption("format backup collision"));
        }
    } else {
        immutable_file(&backup, &legacy.bytes)?;
    }
    let temporary = lock.root().join(format!(
        "format-current-{}.pending",
        vcp_domain::TransactionId::new()
    ));
    lock.verify_child(&temporary)?;
    crate::private_paths::write_private(&temporary, &bytes)?;
    observe(crate::Barrier::BeforeActivation)?;
    // Recheck the selection after all potentially fallible staging callbacks.
    lock.verify()?;
    if read_bounded(&target, MAX_MARKER)? != legacy.bytes {
        return Err(Error::Corruption("format changed during publication"));
    }
    replace(&temporary, &target)?;
    sync_directory(lock.root())?;
    observe(crate::Barrier::AfterActivation)?;
    Ok(())
}

fn replace(source: &Path, target: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: buffers remain live; the held canonical lock and pinned
        // directory restrict both checked paths to this selected physical root.
        if unsafe {
            MoveFileExW(
                source.as_ptr(),
                target.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(source, target)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "store_format_tests.rs"]
mod tests;
