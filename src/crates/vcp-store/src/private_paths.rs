// SPDX-License-Identifier: Apache-2.0
//! Narrow native path capabilities. No universal sync-root discovery is claimed.
use crate::{Error, Result};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub(crate) fn redirected(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}

pub(crate) struct Directory {
    pub(crate) path: PathBuf,
    // Deny replacement of the directory and its ancestors for the capability's
    // entire lifetime. Child content may still change; file handles fence it.
    _ancestors: Vec<File>,
}
impl Directory {
    pub(crate) fn open(path: &Path, forbidden: &[PathBuf]) -> Result<Self> {
        Self::open_policy(path, forbidden, false)
    }
    pub(crate) fn open_cloud(path: &Path, forbidden: &[PathBuf]) -> Result<Self> {
        Self::open_policy(path, forbidden, true)
    }
    fn open_policy(path: &Path, forbidden: &[PathBuf], cloud: bool) -> Result<Self> {
        if !path.is_absolute() || forbidden.is_empty() || forbidden.len() > 128 {
            return Err(Error::Access);
        }
        let directory = Self::hold(path, cloud)?;
        for root in forbidden {
            let root = root.canonicalize()?;
            if directory.path.starts_with(&root) || root.starts_with(&directory.path) {
                return Err(Error::Access);
            }
        }
        Ok(directory)
    }
    fn hold(path: &Path, cloud: bool) -> Result<Self> {
        if !path.is_absolute() {
            return Err(Error::Access);
        }
        let mut held = Vec::new();
        let ancestors = path.ancestors().collect::<Vec<_>>();
        for ancestor in ancestors.into_iter().rev() {
            let metadata = fs::symlink_metadata(ancestor)?;
            if !metadata.is_dir()
                || metadata.file_type().is_symlink()
                || (!cloud && redirected(&metadata))
            {
                return Err(Error::Access);
            }
            let mut options = OpenOptions::new();
            options.read(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options
                    .custom_flags(0x0200_0000 | 0x0020_0000)
                    .share_mode(1 | 2);
            }
            let handle = options.open(ancestor)?;
            if !handle.metadata()?.is_dir() || !allowed_handle(&handle, cloud)? {
                return Err(Error::Access);
            }
            held.push(handle);
        }
        let path = path.canonicalize()?;
        Ok(Self {
            path,
            _ancestors: held,
        })
    }
}

/// Only Cloud Files tags are eligible at the public ciphertext boundary.
/// Their name-surrogate bit is clear; junctions, symlinks and unknown providers
/// remain rejected. The tag is queried from the actual no-follow handle.
pub(crate) fn allowed_handle(file: &File, cloud: bool) -> Result<bool> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            FileAttributeTagInfo, GetFileInformationByHandleEx, FILE_ATTRIBUTE_TAG_INFO,
        };
        #[link(name = "ntdll")]
        unsafe extern "system" {
            fn RtlSetThreadPlaceholderCompatibilityMode(mode: i8) -> i8;
        }
        // Windows can disguise cloud tags. Expose only for this synchronous
        // query, then restore the thread mode; never change process policy.
        // SAFETY: documented scalar API, available on Windows 10 1709+.
        let previous = unsafe { RtlSetThreadPlaceholderCompatibilityMode(2) };
        if previous < 0 {
            return Err(Error::Unavailable("cloud placeholder metadata unavailable"));
        }
        let mut info = FILE_ATTRIBUTE_TAG_INFO {
            FileAttributes: 0,
            ReparseTag: 0,
        };
        // SAFETY: live file handle and correctly sized writable native DTO.
        let queried = unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle(),
                FileAttributeTagInfo,
                (&mut info as *mut FILE_ATTRIBUTE_TAG_INFO).cast(),
                std::mem::size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
            )
        };
        let error = std::io::Error::last_os_error();
        // SAFETY: validated prior mode, restored on this same native thread.
        if unsafe { RtlSetThreadPlaceholderCompatibilityMode(previous) } < 0 {
            return Err(Error::Unavailable(
                "cloud placeholder metadata mode restore failed",
            ));
        }
        if queried == 0 {
            return Err(error.into());
        }
        Ok(info.FileAttributes & 0x400 == 0 || cloud && cloud_tag(info.ReparseTag))
    }
    #[cfg(not(windows))]
    {
        let _ = cloud;
        Ok(!redirected(&file.metadata()?))
    }
}
#[cfg(windows)]
fn cloud_tag(tag: u32) -> bool {
    tag & !0x0000_F000 == 0x9000_001A
}

/// Read selected public ciphertext while its directory ancestry and exact file
/// remain held. Hydration may fail; that error never authorizes partial bytes.
pub(crate) fn read_public_ciphertext(path: &Path, limit: usize) -> Result<Vec<u8>> {
    use std::io::Read;
    let absolute = std::path::absolute(path)?;
    let _parent = Directory::hold(absolute.parent().ok_or(Error::Access)?, true)?;
    if fs::symlink_metadata(&absolute)?.file_type().is_symlink() {
        return Err(Error::Access);
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1).custom_flags(0x0020_0000);
    }
    let mut file = options.open(&absolute)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || !allowed_handle(&file, true)? {
        return Err(Error::Access);
    }
    if metadata.len() > limit as u64 {
        return Err(Error::Limit("snapshot ciphertext input"));
    }
    let mut bytes = Vec::new();
    (&mut file).take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit || bytes.len() as u64 != metadata.len() || !allowed_handle(&file, true)?
    {
        return Err(Error::Corruption(
            "snapshot ciphertext changed or incomplete",
        ));
    }
    Ok(bytes)
}

/// Errors expose no protected file contents. An unsuccessful owner-created
/// write is marked for deletion using its owned handle before it closes.
pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = private_file(path)?;
    let result = file.write_all(bytes).and_then(|()| file.sync_all());
    if result.is_err() {
        remove_owned(&file, path).map_err(|_| {
            Error::Unavailable("secret write failed; owned recovery cleanup pending")
        })?;
    }
    drop(file);
    result.map_err(|_| Error::Unavailable("secret write failed"))
}

/// Mark only the already-owned file for deletion, without a path-based race.
#[cfg(windows)]
pub(crate) fn remove_owned(file: &File, _path: &Path) -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FileDispositionInfo, SetFileInformationByHandle, FILE_DISPOSITION_INFO,
    };
    let mut disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: caller owns a live DELETE-capable file handle; Windows copies the
    // fixed-size descriptor and never receives a path controlled by an archive.
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfo,
            (&mut disposition as *mut FILE_DISPOSITION_INFO).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    } == 0
    {
        return Err(Error::Unavailable("owned ciphertext cleanup pending"));
    }
    Ok(())
}
#[cfg(not(windows))]
pub(crate) fn remove_owned(_file: &File, path: &Path) -> Result<()> {
    Ok(fs::remove_file(path)?)
}

#[cfg(not(windows))]
fn private_file(path: &Path) -> Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    Ok(OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?)
}
#[cfg(windows)]
fn private_file(path: &Path) -> Result<File> {
    use std::os::windows::{ffi::OsStrExt, io::FromRawHandle};
    use windows_sys::Win32::{
        Foundation::{GENERIC_WRITE, INVALID_HANDLE_VALUE},
        Security::SECURITY_ATTRIBUTES,
        Storage::FileSystem::{
            CreateFileW, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_OPEN_REPARSE_POINT,
        },
    };
    let security = Security::current_user()?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: security.0,
        bInheritHandle: 0,
    };
    // Rust's Windows canonicalization supplies the extended-length prefix.
    // The parent is already retained by the caller's directory capability;
    // canonicalizing only that existing parent also preserves CREATE_NEW for
    // the final component without imposing Win32's legacy MAX_PATH limit.
    let native = path
        .parent()
        .ok_or(Error::Access)?
        .canonicalize()?
        .join(path.file_name().ok_or(Error::Access)?);
    let path: Vec<u16> = native.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: buffers/descriptors live through CreateFileW; its returned handle
    // becomes owned by File exactly once. CREATE_NEW never overwrites a secret.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_WRITE | 0x0001_0000,
            0,
            &attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(Error::Unavailable(
            "private recovery file could not be created",
        ));
    }
    Ok(unsafe { File::from_raw_handle(handle) })
}

#[cfg(windows)]
struct Security(*mut std::ffi::c_void);
#[cfg(windows)]
impl Drop for Security {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::LocalFree(self.0);
        }
    }
}
#[cfg(windows)]
impl Security {
    // Same current-user protected-DACL construction as the CLI's owner pipe.
    fn current_user() -> Result<Self> {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, LocalFree},
            Security::{
                Authorization::{
                    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
                },
                GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER,
            },
            System::Threading::{GetCurrentProcess, OpenProcessToken},
        };
        // SAFETY: native token buffer is aligned and bounded; every native
        // allocation/handle is freed on the success and failure paths.
        unsafe {
            let mut token = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err(Error::Unavailable("current user token unavailable"));
            }
            let mut size = 0;
            GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut size);
            if size == 0 || size > 65536 {
                CloseHandle(token);
                return Err(Error::Access);
            }
            let mut storage = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
            let ok = GetTokenInformation(
                token,
                TokenUser,
                storage.as_mut_ptr().cast(),
                size,
                &mut size,
            );
            CloseHandle(token);
            if ok == 0 {
                return Err(Error::Unavailable("current user SID unavailable"));
            }
            let user = &*storage.as_ptr().cast::<TOKEN_USER>();
            let mut text = std::ptr::null_mut();
            if ConvertSidToStringSidW(user.User.Sid, &mut text) == 0 {
                return Err(Error::Access);
            }
            let mut len = 0;
            while *text.add(len) != 0 {
                len += 1;
            }
            let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
            LocalFree(text.cast());
            let sddl: Vec<u16> = format!("D:P(A;;GA;;;{sid})")
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let mut descriptor = std::ptr::null_mut();
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                std::ptr::null_mut(),
            ) == 0
            {
                return Err(Error::Unavailable("private recovery ACL unavailable"));
            }
            Ok(Self(descriptor))
        }
    }
}

#[cfg(all(test, windows))]
mod cloud_tests {
    use super::*;
    #[test]
    fn public_ciphertext_rejects_junction_ancestors_and_leaf() {
        let temp = tempfile::tempdir().unwrap();
        let actual = temp.path().join("actual");
        let junction = temp.path().join("junction");
        let outside = temp.path().join("private");
        fs::create_dir(&actual).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(actual.join("object.age"), b"ciphertext").unwrap();
        let output = std::process::Command::new("cmd.exe")
            .args(["/D", "/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&actual)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "native junction fixture creation failed"
        );
        assert!(Directory::open_cloud(&junction, &[outside]).is_err());
        assert!(read_public_ciphertext(&junction.join("object.age"), 64).is_err());
        assert!(read_public_ciphertext(&junction, 64).is_err());
        // Remove the junction itself, never recursively traverse its target.
        fs::remove_dir(&junction).unwrap();
        assert_eq!(fs::read(actual.join("object.age")).unwrap(), b"ciphertext");
    }
    #[test]
    fn only_documented_cloud_tags_are_public_not_private() {
        for variant in 0..16 {
            assert!(cloud_tag(0x9000_001A | variant << 12));
        }
        for tag in [
            0,
            0xA000_0003,
            0xA000_000C,
            0x8000_0021,
            0x9000_001C,
            0x9001_001A,
            0xB000_001A,
        ] {
            assert!(!cloud_tag(tag));
        }
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("ciphertext.age");
        fs::write(&path, b"bounded ordinary ciphertext").unwrap();
        assert_eq!(
            read_public_ciphertext(&path, 64).unwrap(),
            b"bounded ordinary ciphertext"
        );
        assert!(read_public_ciphertext(&path, 1).is_err());
    }
    #[test]
    #[ignore = "read-only qualification against explicitly selected real OneDrive root"]
    fn actual_cloud_directory_opens_only_as_public_capability() {
        let root = PathBuf::from(std::env::var_os("VCP_TEST_CLOUD_ROOT").unwrap());
        let private = tempfile::tempdir().unwrap();
        let held = Directory::open_cloud(&root, &[private.path().to_path_buf()]).unwrap();
        assert_eq!(held.path, root.canonicalize().unwrap());
        assert!(Directory::open(&root, &[private.path().to_path_buf()]).is_err());
        assert!(Directory::open_cloud(&root, &[root.clone()]).is_err());
    }
}
