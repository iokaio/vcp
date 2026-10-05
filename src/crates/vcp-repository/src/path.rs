// SPDX-License-Identifier: Apache-2.0
use crate::*;
use std::{
    fs::File,
    io::Read,
    path::{Component, Path},
};

pub fn relative(path: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        let Component::Normal(part) = component else {
            return Err(Error::Scope("relative normal components required".into()));
        };
        let part = part
            .to_str()
            .ok_or(Error::Unsupported("non-Unicode path"))?;
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        let device = matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|s| s.len() == 1 && s.as_bytes()[0].is_ascii_digit());
        if device
            || part
                .chars()
                .any(|c| c.is_control() || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
            || part.ends_with(['.', ' '])
        {
            return Err(Error::Scope(
                "alternate stream or ambiguous component".into(),
            ));
        }
        parts.push(part);
    }
    if parts.is_empty() || parts.len() > 128 {
        return Err(Error::Limit("path components"));
    }
    Ok(parts.join("/"))
}

pub struct HeldPath {
    pub(crate) file: File,
    // Directory handles deny deletion while a path is resolved and read. They
    // do not claim to exclude arbitrary file writers after this guard drops.
    _parents: Vec<File>,
    pub native_identity: String,
}

#[cfg(all(test, windows))]
mod directory_create_tests {
    use super::*;

    #[test]
    fn directory_creation_returns_an_already_pinned_handle() {
        let temp = tempfile::tempdir().unwrap();
        let parent = native::open(temp.path(), true).unwrap();
        let child = native::create_directory(&parent, "new").unwrap();
        assert!(std::fs::rename(temp.path().join("new"), temp.path().join("moved")).is_err());
        assert!(native::create_directory(&parent, "NEW").is_err());
        assert!(native::create_directory(&parent, "../escape").is_err());
        let grandchild = native::create_directory(&child, "nested").unwrap();
        assert!(
            std::fs::rename(temp.path().join("new/nested"), temp.path().join("replaced")).is_err()
        );
        drop(grandchild);
        drop(child);
        std::fs::rename(temp.path().join("new"), temp.path().join("moved")).unwrap();
    }
}

/// Side-effect-free directory dependencies for a file destination. Missing
/// components are explicit mutations, never created while preparing a patch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParentDirectory {
    pub path: String,
    pub native_identity: Option<String>,
}

impl Root {
    pub fn prepare_parents(&self, file: &str) -> Result<Vec<ParentDirectory>> {
        let normalized = relative(Path::new(file))?;
        let mut plan = vec![];
        let mut guards = vec![self.hold(None, true)?];
        let mut path = String::new();
        let parts: Vec<_> = normalized.split('/').collect();
        let mut missing = false;
        for part in &parts[..parts.len() - 1] {
            if !path.is_empty() {
                path.push('/');
            }
            path.push_str(part);
            let native_identity = if missing {
                None
            } else {
                match self.hold(Some(Path::new(&path)), true) {
                    Ok(held) => {
                        let identity = held.native_identity.clone();
                        guards.push(held);
                        Some(identity)
                    }
                    Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                        missing = true;
                        None
                    }
                    Err(error) => return Err(error),
                }
            };
            plan.push(ParentDirectory {
                path: path.clone(),
                native_identity,
            });
        }
        Ok(plan)
    }

    /// Keep each existing ancestor pinned while checking the complete plan.
    /// `created` contains only identities observed by this same broker dispatch.
    pub fn hold_planned_parents(
        &self,
        file: &str,
        plan: &[ParentDirectory],
        created: &std::collections::BTreeMap<String, String>,
    ) -> Result<Vec<HeldPath>> {
        let observed = self.prepare_parents(file)?;
        if observed.len() != plan.len() {
            return Err(Error::Stale);
        }
        let mut held = vec![self.hold(None, true)?];
        for (actual, expected) in observed.iter().zip(plan) {
            if actual.path != expected.path {
                return Err(Error::Stale);
            }
            let identity = expected
                .native_identity
                .as_ref()
                .or_else(|| created.get(&expected.path.to_lowercase()));
            if actual.native_identity.as_ref() != identity {
                return Err(Error::Stale);
            }
            if let Some(identity) = identity {
                let guard = self.hold(Some(Path::new(&expected.path)), true)?;
                if &guard.native_identity != identity {
                    return Err(Error::Stale);
                }
                held.push(guard);
            }
        }
        Ok(held)
    }
}

#[cfg(windows)]
pub(crate) mod native {
    use super::*;
    use std::{
        fs::OpenOptions,
        os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    };
    use windows_sys::Win32::Storage::FileSystem::*;
    pub fn open(path: &Path, directory: bool) -> Result<File> {
        let mut options = OpenOptions::new();
        options
            .access_mode(if directory {
                // Attribute-only opens do not establish the intended sharing
                // conflict for directory rename. Request directory read access
                // so omitting FILE_SHARE_DELETE actually pins the namespace.
                FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES
            } else {
                FILE_GENERIC_READ
            })
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
            );
        let file = options.open(path)?;
        let info = info(&file)?;
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(Error::Link);
        }
        if (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory {
            return Err(Error::Scope("unexpected file/directory kind".into()));
        }
        Ok(file)
    }

    /// FILE_CREATE atomically returns the new directory with delete sharing
    /// denied. No create-then-open interval permits substituting another object.
    pub fn create_directory(parent: &File, name: &str) -> Result<File> {
        use std::os::windows::io::FromRawHandle;
        use windows_sys::{
            Wdk::{
                Foundation::OBJECT_ATTRIBUTES,
                Storage::FileSystem::{
                    NtCreateFile, FILE_CREATE, FILE_DIRECTORY_FILE, FILE_OPEN_REPARSE_POINT,
                    FILE_SYNCHRONOUS_IO_NONALERT,
                },
            },
            Win32::{
                Foundation::{RtlNtStatusToDosError, OBJ_CASE_INSENSITIVE, UNICODE_STRING},
                System::IO::IO_STATUS_BLOCK,
            },
        };
        if relative(Path::new(name))? != name || name.contains('/') {
            return Err(Error::Scope("single directory component required".into()));
        }
        let mut name: Vec<u16> = name.encode_utf16().collect();
        let length =
            u16::try_from(name.len() * 2).map_err(|_| Error::Limit("directory component"))?;
        let mut string = UNICODE_STRING {
            Length: length,
            MaximumLength: length,
            Buffer: name.as_mut_ptr(),
        };
        let attributes = OBJECT_ATTRIBUTES {
            Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
            RootDirectory: parent.as_raw_handle().cast(),
            ObjectName: &mut string,
            Attributes: OBJ_CASE_INSENSITIVE,
            SecurityDescriptor: std::ptr::null_mut(),
            SecurityQualityOfService: std::ptr::null_mut(),
        };
        let mut handle = std::ptr::null_mut();
        let mut status_block: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
        // All pointers live through this synchronous call. FILE_CREATE cannot
        // open an existing entry, and the one-component name is parent-relative.
        let status = unsafe {
            NtCreateFile(
                &mut handle,
                FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
                &attributes,
                &mut status_block,
                std::ptr::null(),
                FILE_ATTRIBUTE_NORMAL,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                FILE_CREATE,
                FILE_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
                std::ptr::null(),
                0,
            )
        };
        if status < 0 {
            return Err(std::io::Error::from_raw_os_error(
                unsafe { RtlNtStatusToDosError(status) } as i32,
            )
            .into());
        }
        // A successful synchronous create transfers this unique owned handle.
        Ok(unsafe { File::from_raw_handle(handle.cast()) })
    }
    pub(crate) fn info(file: &File) -> Result<BY_HANDLE_FILE_INFORMATION> {
        let mut value = std::mem::MaybeUninit::zeroed();
        if unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), value.as_mut_ptr()) }
            == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(unsafe { value.assume_init() })
    }
    pub fn identity(file: &File) -> Result<String> {
        let info = info(file)?;
        Ok(format!(
            "{:08x}:{:08x}{:08x}",
            info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
        ))
    }
    pub fn final_path(file: &File) -> Result<PathBuf> {
        use std::os::windows::ffi::OsStringExt;
        let mut buffer = vec![0u16; 32768];
        let count = unsafe {
            GetFinalPathNameByHandleW(
                file.as_raw_handle().cast(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                0,
            )
        };
        if count == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        if count as usize >= buffer.len() {
            return Err(Error::Limit("native path"));
        }
        Ok(std::ffi::OsString::from_wide(&buffer[..count as usize]).into())
    }
    pub fn local_absolute(path: &Path) -> bool {
        use std::path::Prefix;
        path.is_absolute()
            && matches!(path.components().next(), Some(Component::Prefix(prefix))
            if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)))
    }
}
#[cfg(not(windows))]
mod native {
    use super::*;
    pub fn open(_: &Path, _: bool) -> Result<File> {
        Err(Error::Unsupported("native Windows file identity required"))
    }
    pub fn identity(_: &File) -> Result<String> {
        Err(Error::Unsupported("native Windows file identity required"))
    }
    pub fn final_path(_: &File) -> Result<PathBuf> {
        Err(Error::Unsupported("native Windows file identity required"))
    }
    pub fn local_absolute(_: &Path) -> bool {
        false
    }
}

impl Root {
    /// Root IDs come from canonical registration, never from path/name hashing.
    /// The initial Windows contract rejects reparse components, including links
    /// that happen to point back inside the root; discovery reports that limit.
    pub fn open(identity: RootIdentity, path: &Path) -> Result<Self> {
        if !native::local_absolute(path)
            || identity.repository.is_empty()
            || identity.worktree.is_empty()
        {
            return Err(Error::Scope(
                "explicit local absolute root and stable identities required".into(),
            ));
        }
        let path = std::path::absolute(path)?;
        let mut parents = Vec::new();
        for ancestor in path.ancestors().collect::<Vec<_>>().into_iter().rev() {
            parents.push(native::open(ancestor, true)?);
        }
        let directory_identity =
            native::identity(parents.last().ok_or(Error::Scope("root".into()))?)?;
        let path = native::final_path(parents.last().unwrap())?;
        Ok(Self {
            identity,
            path,
            directory_identity,
        })
    }
    pub fn hold(&self, relative_path: Option<&Path>, directory: bool) -> Result<HeldPath> {
        let mut parents = Vec::new();
        for ancestor in self.path.ancestors().collect::<Vec<_>>().into_iter().rev() {
            parents.push(native::open(ancestor, true)?);
        }
        if native::identity(parents.last().unwrap())? != self.directory_identity {
            return Err(Error::Stale);
        }
        let Some(relative_path) = relative_path else {
            if !directory {
                return Err(Error::Scope("root is a directory".into()));
            }
            let file = parents.pop().unwrap();
            return Ok(HeldPath {
                file,
                _parents: parents,
                native_identity: self.directory_identity.clone(),
            });
        };
        let normalized = relative(relative_path)?;
        let components: Vec<_> = normalized.split('/').collect();
        let mut path = self.path.clone();
        for part in &components[..components.len() - 1] {
            path.push(part);
            parents.push(native::open(&path, true)?);
        }
        path.push(components.last().unwrap());
        let file = native::open(&path, directory)?;
        let native_identity = native::identity(&file)?;
        Ok(HeldPath {
            file,
            _parents: parents,
            native_identity,
        })
    }
    pub fn read(&self, path: &Path, limit: u64) -> Result<Source> {
        if limit == 0 || limit > 64 * 1024 * 1024 {
            return Err(Error::Limit("file bytes"));
        }
        let normalized = relative(path)?;
        let held = self.hold(Some(path), false)?;
        let length = held.file.metadata()?.len();
        if length > limit {
            return Err(Error::Limit("file bytes"));
        }
        let mut bytes = Vec::new();
        (&held.file).take(limit + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > limit {
            return Err(Error::Limit("file bytes"));
        }
        if bytes.len() as u64 != length {
            return Err(Error::Stale);
        }
        Ok(Source {
            version: FileVersion {
                root: self.identity.root.clone(),
                binding: self.identity.binding,
                path: normalized,
                native_identity: held.native_identity.clone(),
                sha256: vcp_protocol::digest_bytes(&bytes),
                bytes: ByteCount::new(length),
            },
            bytes,
        })
    }
    pub fn revalidate(&self, expected: &FileVersion) -> Result<()> {
        if expected.root != self.identity.root || expected.binding != self.identity.binding {
            return Err(Error::Stale);
        }
        let observed = self.version(Path::new(&expected.path), expected.bytes.get().max(1))?;
        if observed != *expected {
            return Err(Error::Stale);
        }
        Ok(())
    }
    /// Streaming identity for bounded native executables. Source capture retains
    /// its separate 64 MiB ceiling; executable bytes never become context text.
    pub fn version(&self, path: &Path, limit: u64) -> Result<FileVersion> {
        if limit == 0 || limit > 256 * 1024 * 1024 {
            return Err(Error::Limit("version bytes"));
        }
        let normalized = relative(path)?;
        let held = self.hold(Some(path), false)?;
        let expected = held.file.metadata()?.len();
        if expected > limit {
            return Err(Error::Limit("version bytes"));
        }
        let (sha256, length) = vcp_protocol::digest_reader((&held.file).take(limit + 1))?;
        if length != expected || length > limit {
            return Err(Error::Stale);
        }
        Ok(FileVersion {
            root: self.identity.root.clone(),
            binding: self.identity.binding,
            path: normalized,
            native_identity: held.native_identity.clone(),
            sha256,
            bytes: ByteCount::new(length),
        })
    }
    /// Keep native deny-write/delete sharing and ancestor guards alive while
    /// verifying executable/source bytes. The caller retains this guard across
    /// dispatch; a path/hash check followed by closing the handle is insufficient.
    pub fn pin_version(&self, expected: &FileVersion) -> Result<HeldPath> {
        if expected.root != self.identity.root || expected.binding != self.identity.binding {
            return Err(Error::Stale);
        }
        let held = self.hold(Some(Path::new(&expected.path)), false)?;
        if held.native_identity != expected.native_identity {
            return Err(Error::Stale);
        }
        self.revalidate(expected)?;
        Ok(held)
    }
}
