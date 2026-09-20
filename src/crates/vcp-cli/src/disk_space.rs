// SPDX-License-Identifier: Apache-2.0
//! Point-in-time local capacity evidence, never a disk reservation or an
//! assertion that encrypted archive sizes predict native backend expansion.
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct Observation {
    pub directory: PathBuf,
    pub available_bytes: u64,
    pub total_bytes: u64,
    pub required_minimum_bytes: u64,
    pub minimum_fits: bool,
    pub reserved: bool,
    pub sizing: &'static str,
}

#[cfg(windows)]
pub fn observe(path: &Path, required_minimum_bytes: u64) -> Result<Observation, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    // The same no-reparse root used by local registry operations pins the
    // existing directory throughout the volume query. Callers choose an
    // existing parent when the future destination has not been created.
    let root = crate::settings::registry_root(path)?;
    let pin = root.hold(None, true).map_err(|e| e.to_string())?;
    let directory = root.path().to_owned();
    let mut wide: Vec<u16> = directory.as_os_str().encode_wide().collect();
    if wide.contains(&0) {
        return Err("capacity directory contains a null character".into());
    }
    wide.push(0);
    let mut available = 0;
    let mut total = 0;
    // SAFETY: the directory is a live null-terminated UTF-16 buffer; outputs
    // are valid u64 pointers for the duration of this synchronous system call.
    let result = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available,
            &mut total,
            std::ptr::null_mut(),
        )
    };
    if result == 0 {
        return Err(format!(
            "local free-space query failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    if root
        .hold(None, true)
        .map_err(|e| e.to_string())?
        .native_identity
        != pin.native_identity
    {
        return Err("capacity directory identity changed".into());
    }
    Ok(Observation {
        directory,
        available_bytes: available,
        total_bytes: total,
        required_minimum_bytes,
        minimum_fits: available >= required_minimum_bytes,
        reserved: false,
        sizing: "lower_bound_excludes_backend_overhead",
    })
}

#[cfg(not(windows))]
pub fn observe(_: &Path, _: u64) -> Result<Observation, String> {
    Err("native Windows capacity qualification required".into())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn reports_native_capacity_without_promising_reservation() {
        let directory = tempfile::tempdir().unwrap();
        let result = observe(directory.path(), u64::MAX).unwrap();
        assert!(result.total_bytes > 0);
        assert!(result.available_bytes <= result.total_bytes);
        assert!(!result.minimum_fits);
        assert!(!result.reserved);
        assert!(observe(&directory.path().join("missing"), 0).is_err());
    }
}
