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
    replace_checked(
        &temporary,
        &target,
        &|| {
            lock.verify()?;
            let selected = read_bounded(&target, MAX_MARKER)?;
            if selected == bytes {
                read(lock, owner.kind())?;
                return Ok(true);
            }
            if selected != legacy.bytes {
                return Err(Error::Corruption("format changed during replacement"));
            }
            if read_bounded(&temporary, MAX_MARKER)? != bytes {
                return Err(Error::Corruption("format pending bytes changed"));
            }
            Ok(false)
        },
        &mut replace,
    )?;
    sync_directory(lock.root())?;
    observe(crate::Barrier::AfterActivation)?;
    Ok(())
}

/// Windows can briefly deny replacement while native readers finish opening or
/// closing the marker. Every retry requires the same held owner and exact old
/// and pending bytes; a changed selection or any other failure is not retried.
fn replace_checked(
    source: &Path,
    target: &Path,
    verify: &impl Fn() -> Result<bool>,
    operation: &mut impl FnMut(&Path, &Path) -> Result<()>,
) -> Result<()> {
    let attempts = if cfg!(windows) { 8 } else { 1 };
    for attempt in 0..attempts {
        let result = match verify() {
            Ok(true) => return Ok(()),
            Ok(false) => operation(source, target),
            Err(error) => Err(error),
        };
        match result {
            Ok(()) => return Ok(()),
            Err(Error::Io(error))
                if cfg!(windows)
                    && matches!(error.raw_os_error(), Some(5 | 32 | 33))
                    && attempt + 1 < attempts =>
            {
                std::thread::sleep(std::time::Duration::from_millis(10 << attempt.min(4)));
            }
            Err(error) => return Err(error),
        }
    }
    Err(Error::Unavailable("format replacement attempts exhausted"))
}
fn replace(source: &Path, target: &Path) -> Result<()> {
    #[cfg(windows)]
    let _target = {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::{
            Foundation::GENERIC_READ,
            Storage::FileSystem::{DELETE, FILE_FLAG_OPEN_REPARSE_POINT, SYNCHRONIZE},
        };
        // Admit the selected target's read/delete access and retain its native
        // handle across replacement. Opening only the source intermittently
        // fails on Windows even though both marker byte checks succeed.
        let file = std::fs::OpenOptions::new()
            .access_mode(GENERIC_READ | DELETE | SYNCHRONIZE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(target)?;
        if !crate::private_paths::allowed_handle(&file, false)? {
            return Err(Error::Access);
        }
        file
    };
    // Source bytes are synced; both names stay under the pinned canonical root.
    // The standard native rename keeps namespace replacement atomic, including
    // Windows FileRenameInfoEx handling for shared readers. No delete-then-create.
    std::fs::rename(source, target)?;
    Ok(())
}

#[cfg(test)]
#[path = "store_format_tests.rs"]
mod tests;
