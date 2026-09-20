// SPDX-License-Identifier: Apache-2.0
//! A descriptor selection is leased through canonical use. Activation cannot
//! race a reader that has selected an older root but has not opened it yet.
use crate::settings::{self, WorkspaceEntry};
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};
use vcp_repository::path::HeldPath;

type Result<T> = std::result::Result<T, String>;
pub struct Lease {
    _directory: HeldPath,
    _file: File,
    data: PathBuf,
    directory: PathBuf,
    exclusive: bool,
    target_parent: Option<HeldPath>,
}
impl Lease {
    pub fn shared(data: &Path, directory: &Path) -> Result<Self> {
        Self::open(data, directory, false)
    }
    pub fn exclusive(data: &Path, directory: &Path) -> Result<Self> {
        Self::open(data, directory, true)
    }
    fn open(data: &Path, directory: &Path, exclusive: bool) -> Result<Self> {
        let relative = directory
            .strip_prefix(data)
            .map_err(|_| "selection outside registry")?;
        let parts: Vec<_> = relative.components().collect();
        if parts.len() != 2
            || parts[0].as_os_str() != "workspaces"
            || !matches!(parts[1], std::path::Component::Normal(_))
        {
            return Err("selection must identify one workspace registry entry".into());
        }
        let root = settings::registry_root(data)?;
        let pin = root
            .hold(Some(relative), true)
            .map_err(|_| "selection directory unavailable or redirected")?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x0020_0000).share_mode(1 | 2);
        }
        let file = options
            .open(directory.join("selection.lock"))
            .map_err(|_| "selection lease unavailable")?;
        let metadata = file
            .metadata()
            .map_err(|_| "selection lease metadata unavailable")?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("selection lease redirected".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("selection lease redirected".into());
            }
        }
        if exclusive {
            file.try_lock()
        } else {
            file.try_lock_shared()
        }
        .map_err(|_| "workspace selection is in use; close the owner/readers before activation")?;
        Ok(Self {
            _directory: pin,
            _file: file,
            data: data.to_owned(),
            directory: directory.to_owned(),
            exclusive,
            target_parent: None,
        })
    }
    pub fn descriptor(&self) -> Result<Option<(WorkspaceEntry, String)>> {
        let path = self.directory.join("workspace.json");
        if !path.exists() {
            return Ok(None);
        }
        let bytes = settings::read_workspace_descriptor(&self.data, &path)?;
        let entry: WorkspaceEntry =
            serde_json::from_slice(&bytes).map_err(|_| "invalid workspace descriptor")?;
        validate_location(&self.directory, &entry)?;
        Ok(Some((entry, vcp_protocol::digest_bytes(&bytes))))
    }
    /// Only a generated child is available for conversion/restore preparation.
    /// The caller's validated import capability must refer to this exact path.
    pub fn target(&mut self, operation: &vcp_domain::CommandId) -> Result<PathBuf> {
        if !self.exclusive {
            return Err("exclusive selection required".into());
        }
        if !operation_id(operation.as_str()) {
            return Err("opaque operation UUID required".into());
        }
        let parent = self.directory.join("canonical-roots");
        if !parent.exists() {
            std::fs::create_dir(&parent).map_err(|_| "owned root directory unavailable")?;
        }
        let root = settings::registry_root(&self.data)?;
        let pin = root
            .hold(
                Some(
                    parent
                        .strip_prefix(&self.data)
                        .map_err(|_| "root outside registry")?,
                ),
                true,
            )
            .map_err(|_| "owned root directory redirected")?;
        self.target_parent = Some(pin);
        Ok(parent.join(operation.as_str()))
    }
    /// Internal final CAS; the activation controller first reopens its opaque
    /// validated candidate and holds its Store owner until this call completes.
    pub(crate) fn publish(&self, expected: Option<&str>, next: &WorkspaceEntry) -> Result<()> {
        if !self.exclusive {
            return Err("exclusive selection required".into());
        }
        validate_location(&self.directory, next)?;
        let current = self.descriptor()?;
        if current.as_ref().map(|(_, digest)| digest.as_str()) != expected {
            return Err("workspace selection changed; prepare a new activation preview".into());
        }
        if current
            .as_ref()
            .is_some_and(|(old, _)| old.config.workspace != next.config.workspace)
        {
            return Err("workspace identity collision".into());
        }
        settings::save(&self.directory.join("workspace.json"), next)
    }
}
pub(crate) fn operation_id(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}
pub fn validate_location(directory: &Path, entry: &WorkspaceEntry) -> Result<()> {
    let accepted = match entry.version {
        1 => entry.config.canonical_root == directory.join("canonical"),
        2 => {
            entry.config.canonical_root.parent()
                == Some(directory.join("canonical-roots").as_path())
                && entry
                    .config
                    .canonical_root
                    .file_name()
                    .and_then(|id| id.to_str())
                    .is_some_and(operation_id)
        }
        _ => false,
    };
    if !accepted {
        return Err("workspace descriptor canonical location mismatch".into());
    }
    Ok(())
}
