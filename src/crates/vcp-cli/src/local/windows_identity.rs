// SPDX-License-Identifier: Apache-2.0
//! Windows identities are read from held kernel handles, never wire claims.
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs::{File, OpenOptions},
    io,
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStringExt,
        fs::OpenOptionsExt,
        io::{AsHandle, AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
    ptr::null_mut,
};
use windows_sys::Win32::{
    Foundation::*,
    Security::*,
    Storage::FileSystem::*,
    System::{Pipes::*, Threading::*},
};

pub(super) fn checked(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
fn denied() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "local peer identity denied",
    )
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FileIdentity {
    pub volume: u32,
    pub index: String,
}

/// Holds the image against replacement/write until launch and authentication finish.
pub(super) struct Executable {
    _file: File,
    path: PathBuf,
    identity: FileIdentity,
}
impl Executable {
    pub fn open(path: &Path) -> io::Result<Self> {
        if !path.is_absolute() {
            return Err(denied());
        }
        let path = path.canonicalize()?;
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&path)?;
        let mut information = unsafe { zeroed() };
        // SAFETY: valid owned file and correctly sized output.
        checked(unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) })?;
        if information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
            return Err(denied());
        }
        let identity = FileIdentity {
            volume: information.dwVolumeSerialNumber,
            index: ((u64::from(information.nFileIndexHigh) << 32)
                | u64::from(information.nFileIndexLow))
            .to_string(),
        };
        Ok(Self {
            _file: file,
            path,
            identity,
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn identity(&self) -> &FileIdentity {
        &self.identity
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Principal {
    pub sid: Vec<u8>,
    pub session: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProcessPin {
    pub pid: u32,
    /// Decimal string avoids JavaScript's lossy integer range.
    pub created: String,
    pub principal: Principal,
    pub image: PathBuf,
    pub file: FileIdentity,
}

pub(super) struct HeldProcess(OwnedHandle);
impl HeldProcess {
    pub fn from_owned(handle: OwnedHandle) -> Self {
        Self(handle)
    }
    pub fn open(pid: u32) -> io::Result<Self> {
        // SAFETY: returns a newly owned real process handle.
        let handle =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, 0, pid) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(unsafe { OwnedHandle::from_raw_handle(handle) }))
    }
    pub fn current() -> io::Result<Self> {
        Self::open(std::process::id())
    }
    /// Duplicate an inherited parent proof with query/synchronize rights only.
    /// The number is untrusted until the resulting process pin is validated.
    pub fn inherited(raw: u64) -> io::Result<Self> {
        let value = usize::try_from(raw).map_err(|_| denied())?;
        let handle = value as HANDLE;
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Err(denied());
        }
        let mut copy = null_mut();
        checked(unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                handle,
                GetCurrentProcess(),
                &mut copy,
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                0,
            )
        })?;
        let process = Self(unsafe { OwnedHandle::from_raw_handle(copy) });
        // Reject other kernel object types before exposing the result.
        if unsafe { GetProcessId(process.0.as_raw_handle()) } == 0 {
            return Err(denied());
        }
        // Keep ownership of the original with its creator, but prevent any
        // later child launch from inheriting this validated process object.
        checked(unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) })?;
        Ok(process)
    }
    pub fn handle(&self) -> &OwnedHandle {
        &self.0
    }
    pub fn is_alive(&self) -> io::Result<bool> {
        match unsafe { WaitForSingleObject(self.0.as_raw_handle(), 0) } {
            WAIT_TIMEOUT => Ok(true),
            WAIT_OBJECT_0 => Ok(false),
            _ => Err(io::Error::last_os_error()),
        }
    }
    pub fn pin(&self) -> io::Result<ProcessPin> {
        if !self.is_alive()? {
            return Err(denied());
        }
        let handle = self.0.as_raw_handle();
        let pid = unsafe { GetProcessId(handle) };
        if pid == 0 {
            return Err(io::Error::last_os_error());
        }
        let (mut created, mut exited, mut kernel, mut user) =
            unsafe { (zeroed(), zeroed(), zeroed(), zeroed()) };
        checked(unsafe {
            GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user)
        })?;
        let mut image = vec![0u16; 32768];
        let mut length = image.len() as u32;
        checked(unsafe { QueryFullProcessImageNameW(handle, 0, image.as_mut_ptr(), &mut length) })?;
        let executable =
            Executable::open(Path::new(&OsString::from_wide(&image[..length as usize])))?;
        let mut token = null_mut();
        checked(unsafe { OpenProcessToken(handle, TOKEN_QUERY, &mut token) })?;
        let token = unsafe { OwnedHandle::from_raw_handle(token) };
        let principal = token_principal(&token)?;
        if !self.is_alive()? {
            return Err(denied());
        }
        Ok(ProcessPin {
            pid,
            created: ((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
                .to_string(),
            principal,
            image: executable.path.clone(),
            file: executable.identity.clone(),
        })
    }
    pub fn validate(&self, expected: &ProcessPin) -> io::Result<()> {
        if &self.pin()? != expected {
            return Err(denied());
        }
        Ok(())
    }
}

/// Dedicated helper startup only, before pumps or any execution child exists.
/// Standard streams remain open; future children cannot inherit their handles.
pub(super) fn seal_inherited_stdio() -> io::Result<()> {
    for handle in [
        std::io::stdin().as_raw_handle(),
        std::io::stdout().as_raw_handle(),
        std::io::stderr().as_raw_handle(),
    ] {
        checked(unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) })?;
    }
    Ok(())
}

fn token_principal(token: &OwnedHandle) -> io::Result<Principal> {
    let mut needed = 0;
    unsafe { GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut needed) };
    if needed < size_of::<TOKEN_USER>() as u32 || needed > 65536 {
        return Err(denied());
    }
    // usize storage supplies TOKEN_USER alignment; the SID is copied before freeing it.
    let mut storage = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
    checked(unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            storage.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    })?;
    let user = unsafe { &*storage.as_ptr().cast::<TOKEN_USER>() };
    if unsafe { IsValidSid(user.User.Sid) } == 0 {
        return Err(denied());
    }
    let length = unsafe { GetLengthSid(user.User.Sid) };
    if length == 0 || length > 1024 {
        return Err(denied());
    }
    let mut sid = vec![0u8; length as usize];
    checked(unsafe { CopySid(length, sid.as_mut_ptr().cast(), user.User.Sid) })?;
    let mut session = 0u32;
    checked(unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenSessionId,
            (&mut session as *mut u32).cast(),
            size_of::<u32>() as u32,
            &mut needed,
        )
    })?;
    Ok(Principal { sid, session })
}

struct Revert;
impl Drop for Revert {
    fn drop(&mut self) {
        // Continuing on this thread under a client token would cross the trust boundary.
        if unsafe { RevertToSelf() } == 0 {
            std::process::abort();
        }
    }
}

pub(super) struct AuthenticatedPeer {
    pub process: HeldProcess,
    pub pin: ProcessPin,
}

/// Call synchronously immediately after the bounded bootstrap read, before any
/// await. Reverts on every return/unwind. Caller still checks principal/role policy.
pub(super) fn authenticate_pipe_client(pipe: &impl AsHandle) -> io::Result<AuthenticatedPeer> {
    let raw = pipe.as_handle().as_raw_handle();
    let mut pid = 0;
    checked(unsafe { GetNamedPipeClientProcessId(raw, &mut pid) })?;
    let process = HeldProcess::open(pid)?;
    checked(unsafe { ImpersonateNamedPipeClient(raw) })?;
    let revert = Revert;
    let mut token = null_mut();
    checked(unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut token) })?;
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    let principal = token_principal(&token)?;
    drop(revert);
    let pin = process.pin()?;
    if pin.principal != principal {
        return Err(denied());
    }
    Ok(AuthenticatedPeer { process, pin })
}

/// expected comes ONLY from trusted controlled launch, never endpoint discovery.
/// Keep the returned process alive in the connection object, and recheck at auth completion.
pub(super) fn verify_pipe_server(
    pipe: &impl AsHandle,
    expected: &ProcessPin,
) -> io::Result<HeldProcess> {
    let process = HeldProcess::open(expected.pid)?;
    process.validate(expected)?;
    let mut pid = 0;
    checked(unsafe { GetNamedPipeServerProcessId(pipe.as_handle().as_raw_handle(), &mut pid) })?;
    if pid != expected.pid || !process.is_alive()? {
        return Err(denied());
    }
    Ok(process)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn held_process_rejects_changed_pin_fields() {
        let process = HeldProcess::current().unwrap();
        let pin = process.pin().unwrap();
        process.validate(&pin).unwrap();
        let mut wrong = pin.clone();
        wrong.created.push('0');
        assert!(process.validate(&wrong).is_err());
        let mut wrong = pin.clone();
        wrong.principal.session ^= 1;
        assert!(process.validate(&wrong).is_err());
        let mut wrong = pin.clone();
        wrong.file.index.push('0');
        assert!(process.validate(&wrong).is_err());
        let mut wrong = pin.clone();
        wrong.principal.sid.push(0);
        assert!(process.validate(&wrong).is_err());
        let mut wrong = pin.clone();
        wrong.image.push("other");
        assert!(process.validate(&wrong).is_err());
        let mut wrong = pin;
        wrong.pid ^= 1;
        assert!(process.validate(&wrong).is_err());
    }
    #[test]
    fn inherited_proof_requires_a_real_process_handle() {
        assert!(HeldProcess::inherited(0).is_err());
        assert!(HeldProcess::inherited(u64::MAX).is_err());
        let process = HeldProcess::current().unwrap();
        let copied =
            HeldProcess::inherited(process.handle().as_raw_handle() as usize as u64).unwrap();
        copied.validate(&process.pin().unwrap()).unwrap();
        let file = tempfile::tempfile().unwrap();
        assert!(HeldProcess::inherited(file.as_raw_handle() as usize as u64).is_err());
    }
    #[test]
    fn held_executable_denies_writes_and_relative_paths() {
        assert!(Executable::open(Path::new("vcp.exe")).is_err());
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("image.exe");
        std::fs::write(&path, b"fixture").unwrap();
        let held = Executable::open(&path).unwrap();
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        assert!(std::fs::remove_file(&path).is_err());
        drop(held);
        std::fs::remove_file(&path).unwrap();
    }
}
