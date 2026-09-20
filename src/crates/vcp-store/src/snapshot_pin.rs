// SPDX-License-Identifier: Apache-2.0
//! Physical-root snapshot lease, including snapshots with no artifact payloads.
//! Never unlink this lock: old snapshot handles must fence a reopened owner.
use crate::{artifact::reject_link, Error, Result};
use std::{
    fs::{File, OpenOptions, TryLockError},
    path::Path,
};

fn open(root: &Path) -> Result<File> {
    let path = root.join("root-snapshot.lock");
    match std::fs::symlink_metadata(&path) {
        Ok(_) => reject_link(&path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Open the reparse point itself rather than following an exchanged path.
        // Shared lock holders can coexist, but nobody can replace their inode.
        options.custom_flags(0x0020_0000).share_mode(0x1 | 0x2);
    }
    let file = options.open(&path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(Error::Corruption("snapshot lock is not a regular file"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(Error::Corruption("snapshot lock reparse point"));
        }
    }
    reject_link(&path)?;
    Ok(file)
}
pub(crate) fn acquire(root: &Path) -> Result<File> {
    let file = open(root)?;
    match file.try_lock_shared() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(Error::Conflict("snapshot cleanup in progress")),
        Err(TryLockError::Error(error)) => Err(error.into()),
    }
}
pub(crate) fn cleanup(root: &Path) -> Result<Option<File>> {
    let file = open(root)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(error.into()),
    }
}
