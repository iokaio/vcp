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
        if !path.is_absolute() || forbidden.is_empty() || forbidden.len() > 128 {
            return Err(Error::Access);
        }
        let mut held = Vec::new();
        let ancestors = path.ancestors().collect::<Vec<_>>();
        for ancestor in ancestors.into_iter().rev() {
            let metadata = fs::symlink_metadata(ancestor)?;
            if !metadata.is_dir() || redirected(&metadata) {
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
            if redirected(&handle.metadata()?) {
                return Err(Error::Access);
            }
            held.push(handle);
        }
        let path = path.canonicalize()?;
        for root in forbidden {
            let root = root.canonicalize()?;
            if path.starts_with(&root) || root.starts_with(&path) {
                return Err(Error::Access);
            }
        }
        Ok(Self {
            path,
            _ancestors: held,
        })
    }
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
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
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
