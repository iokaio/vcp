// SPDX-License-Identifier: Apache-2.0
//! Trusted native worker primitives; no policy is inferred from a path. The
//! broker must commit dispatch before `apply`. Existing-file writes retain an
//! exclusive version-checked handle. They are staged but not crash-atomic;
//! partial outcomes are reported and never rolled back over newer human work.
use crate::{
    instructions::Probe,
    path::{native, HeldPath},
    *,
};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::Path,
};
use windows_sys::Win32::Storage::FileSystem::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Observation {
    pub path: String,
    pub destination: Option<String>,
    pub before: Option<FileVersion>,
    pub intended_sha256: Option<String>,
    pub observed: Option<FileVersion>,
    pub changed: bool,
    pub complete: bool,
    pub error: Option<String>,
    pub staging_path: Option<String>,
}
pub struct Target {
    root: Root,
    path: String,
    expected: Option<FileVersion>,
    file: Option<File>,
    _parent: HeldPath,
}
fn parent(root: &Root, path: &str) -> Result<HeldPath> {
    let p = Path::new(path)
        .parent()
        .ok_or(Error::Scope("file parent".into()))?;
    root.hold(
        if p.as_os_str().is_empty() {
            None
        } else {
            Some(p)
        },
        true,
    )
}
fn validate(path: &str) -> Result<()> {
    if crate::path::relative(Path::new(path))? != path
        || path.split('/').any(|p| p.eq_ignore_ascii_case(".git"))
    {
        return Err(Error::Scope("canonical file path required".into()));
    }
    Ok(())
}
fn observe(root: &Root, path: &str, file: &mut File) -> Result<FileVersion> {
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = vec![];
    (&mut *file).take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 1024 * 1024 {
        return Err(Error::Limit("mutation source bytes"));
    }
    Ok(FileVersion {
        root: root.identity.root.clone(),
        binding: root.identity.binding,
        path: path.into(),
        native_identity: native::identity(file)?,
        sha256: vcp_protocol::digest_bytes(&bytes),
        bytes: ByteCount::new(bytes.len() as u64),
    })
}
fn dispose(file: &File) -> Result<()> {
    let info = FILE_DISPOSITION_INFO { DeleteFile: true };
    let ok = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle().cast(),
            FileDispositionInfo,
            (&info as *const FILE_DISPOSITION_INFO).cast(),
            std::mem::size_of_val(&info) as u32,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
pub(crate) fn rename(file: &File, destination: &Path) -> Result<()> {
    let name: Vec<u16> = destination.as_os_str().encode_wide().collect();
    let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
    let length = (offset + (name.len() + 1) * 2).max(std::mem::size_of::<FILE_RENAME_INFO>());
    // The Win32 FileName field is NUL terminated; FileNameLength excludes that
    // terminator. Allocate it explicitly even when the name fills aligned words.
    let mut storage = vec![0usize; length.div_ceil(std::mem::size_of::<usize>())];
    let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    unsafe {
        (*info).Anonymous.ReplaceIfExists = false;
        (*info).RootDirectory = std::ptr::null_mut();
        (*info).FileNameLength = (name.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            storage.as_mut_ptr().cast::<u8>().add(offset).cast(),
            name.len(),
        );
        if SetFileInformationByHandle(
            file.as_raw_handle().cast(),
            FileRenameInfo,
            info.cast(),
            length as u32,
        ) == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    Ok(())
}
struct Stage {
    file: File,
    keep: bool,
}
impl Drop for Stage {
    fn drop(&mut self) {
        if !self.keep {
            let _ = dispose(&self.file);
        }
    }
}
impl Root {
    /// Acquires a deny-write/delete native handle and compares full identity and
    /// bytes. Acquiring this guard has no file mutation effect.
    pub fn mutation_target(&self, probe: &Probe) -> Result<Target> {
        validate(&probe.path)?;
        if probe.root != self.identity.root || probe.binding != self.identity.binding {
            return Err(Error::Stale);
        }
        let parent = parent(self, &probe.path)?;
        let file = if let Some(expected) = &probe.observed {
            let mut f = OpenOptions::new()
                .access_mode(FILE_GENERIC_READ | FILE_GENERIC_WRITE | DELETE)
                .share_mode(FILE_SHARE_READ)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(self.path.join(&probe.path))?;
            let info = native::info(&f)?;
            if info.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY)
                != 0
                || info.nNumberOfLinks != 1
            {
                return Err(Error::Link);
            }
            if observe(self, &probe.path, &mut f)? != *expected {
                return Err(Error::Stale);
            }
            Some(f)
        } else {
            match std::fs::symlink_metadata(self.path.join(&probe.path)) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
                Ok(_) => return Err(Error::Stale),
            }
            None
        };
        Ok(Target {
            root: self.clone(),
            path: probe.path.clone(),
            expected: probe.observed.clone(),
            file,
            _parent: parent,
        })
    }
}
impl Target {
    /// The caller durably records this operation's intent first. Once a write
    /// starts, an error is an observed partial/unknown outcome, not permission
    /// to retry. No automatic rollback is performed.
    pub fn apply(mut self, after: Option<&[u8]>, destination: Option<&str>) -> Observation {
        let mut observation = Observation {
            path: self.path.clone(),
            destination: destination.map(str::to_owned),
            before: self.expected.clone(),
            intended_sha256: after.map(vcp_protocol::digest_bytes),
            observed: None,
            changed: false,
            complete: false,
            error: None,
            staging_path: None,
        };
        let result = (|| -> Result<()> {
            if after.is_some_and(|b| b.len() > 1024 * 1024) {
                return Err(Error::Limit("candidate bytes"));
            }
            let _destination_parent = destination
                .map(|p| {
                    validate(p)?;
                    parent(&self.root, p)
                })
                .transpose()?;
            if let Some(dest) = destination {
                if dest.eq_ignore_ascii_case(&self.path) {
                    let check = OpenOptions::new()
                        .access_mode(FILE_READ_ATTRIBUTES)
                        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                        .open(self.root.path.join(dest))?;
                    if native::identity(&check)?
                        != self.expected.as_ref().ok_or(Error::Stale)?.native_identity
                    {
                        return Err(Error::Stale);
                    }
                } else {
                    match std::fs::symlink_metadata(self.root.path.join(dest)) {
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                        Err(e) => return Err(e.into()),
                        Ok(_) => return Err(Error::Stale),
                    }
                }
            }
            match after {
                Some(bytes) => {
                    let directory = self
                        .root
                        .path
                        .join(&self.path)
                        .parent()
                        .unwrap()
                        .to_path_buf();
                    let stage_path =
                        directory.join(format!(".vcp-stage-{}", vcp_domain::ArtifactId::new()));
                    observation.staging_path = Some(
                        stage_path
                            .strip_prefix(self.root.path())
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                    let file = OpenOptions::new()
                        .write(true)
                        .access_mode(FILE_GENERIC_READ | FILE_GENERIC_WRITE | DELETE)
                        .share_mode(0)
                        .create_new(true)
                        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                        .open(&stage_path)?;
                    let mut stage = Stage { file, keep: false };
                    stage.file.write_all(bytes)?;
                    stage.file.sync_all()?;
                    stage.file.seek(SeekFrom::Start(0))?;
                    if let Some(file) = &mut self.file {
                        // Handle identity and sharing remain held across this write.
                        file.seek(SeekFrom::Start(0))?;
                        observation.changed = true;
                        std::io::copy(&mut stage.file, file)?;
                        file.set_len(bytes.len() as u64)?;
                        file.sync_all()?;
                        if let Some(dest) = destination {
                            rename(file, &self.root.path.join(dest))?;
                        }
                        dispose(&stage.file)?;
                        stage.keep = true;
                    } else {
                        if destination.is_some() {
                            return Err(Error::Scope("rename needs existing source".into()));
                        }
                        rename(&stage.file, &self.root.path.join(&self.path))?;
                        observation.changed = true;
                        stage.keep = true;
                        self.file = Some(stage.file.try_clone()?);
                    }
                    let path = destination.unwrap_or(&self.path);
                    let actual = native::final_path(self.file.as_ref().unwrap())?;
                    if actual != self.root.path.join(path) {
                        return Err(Error::Scope(format!(
                            "native target path differs after mutation: {}",
                            actual.display()
                        )));
                    }
                    observation.observed =
                        Some(observe(&self.root, path, self.file.as_mut().unwrap())?);
                    if observation.observed.as_ref().unwrap().sha256
                        != observation.intended_sha256.as_ref().unwrap().as_str()
                    {
                        return Err(Error::Stale);
                    }
                }
                None => {
                    if destination.is_some() {
                        return Err(Error::Scope("delete cannot rename".into()));
                    }
                    let file = self.file.take().ok_or(Error::Stale)?;
                    dispose(&file)?;
                    observation.changed = true;
                    drop(file);
                    match std::fs::symlink_metadata(self.root.path.join(&self.path)) {
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                        Err(e) => return Err(e.into()),
                        Ok(_) => return Err(Error::Stale),
                    }
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => observation.complete = true,
            Err(e) => {
                observation.error = Some(e.to_string());
                if let Some(file) = &mut self.file {
                    observation.observed = observe(&self.root, &self.path, file).ok();
                }
            }
        }
        observation
    }
}
