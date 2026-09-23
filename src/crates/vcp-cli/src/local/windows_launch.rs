// SPDX-License-Identifier: Apache-2.0
//! Restricted native bootstrap. No credentials are accepted as launch arguments.
use super::windows_identity::{checked, Executable, HeldProcess};
use std::{
    ffi::OsStr,
    fs::File,
    io,
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle},
    },
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Cryptography::*, SECURITY_ATTRIBUTES},
    System::{Pipes::CreatePipe, Threading::*},
};

pub(super) struct Launched {
    /// Keep this guard through bootstrap and forwarding. Drop kills only this child.
    pub guard: ChildGuard,
    pub process: HeldProcess,
    pub stdin: File,
    pub stdout: File,
    pub stderr: File,
    /// This value names an inherited handle in the CHILD only. Deliver through stdin.
    pub parent_proof_value: u64,
}
impl Launched {
    /// Trusted bootstrap failure cleanup. Does not confer protocol shutdown authority.
    pub fn terminate(&self) -> io::Result<()> {
        self.guard.terminate()
    }
}

/// Owns the creation handle, distinct from query-only reconnect/process handles.
/// Call wait after closing child stdin for orderly shutdown; Drop is the fallback.
pub(super) struct ChildGuard {
    process: OwnedHandle,
    handed_off: bool,
}
impl ChildGuard {
    /// Only after authenticated pipe readiness and explicit server handoff ack.
    /// The server then owns idle shutdown; this closes our creation handle.
    pub fn handoff(mut self) {
        self.handed_off = true;
    }
    pub fn wait(&self, milliseconds: u32) -> io::Result<bool> {
        match unsafe { WaitForSingleObject(self.process.as_raw_handle(), milliseconds) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(io::Error::last_os_error()),
        }
    }
    pub fn exit_code(&self) -> io::Result<u32> {
        if !self.wait(0)? {
            return Err(io::Error::other("local child is still running"));
        }
        let mut code = 0;
        checked(unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &mut code) })?;
        Ok(code)
    }
    pub fn terminate(&self) -> io::Result<()> {
        if self.wait(0)? {
            return Ok(());
        }
        checked(unsafe { TerminateProcess(self.process.as_raw_handle(), 1) })?;
        match unsafe { WaitForSingleObject(self.process.as_raw_handle(), 5000) } {
            WAIT_OBJECT_0 => Ok(()),
            _ => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "bootstrap child stop incomplete",
            )),
        }
    }
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if !self.handed_off {
            let _ = self.terminate();
        }
    }
}

pub(super) fn random_bytes<const N: usize>() -> io::Result<[u8; N]> {
    let length =
        u32::try_from(N).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "random size"))?;
    let mut bytes = [0u8; N];
    if unsafe {
        BCryptGenRandom(
            null_mut(),
            bytes.as_mut_ptr(),
            length,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    } < 0
    {
        return Err(io::Error::other("system randomness unavailable"));
    }
    Ok(bytes)
}

fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut value: Vec<_> = value.encode_wide().collect();
    if value.contains(&0) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "embedded NUL"));
    }
    value.push(0);
    Ok(value)
}

fn pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let security = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    let (mut read, mut write) = (null_mut(), null_mut());
    checked(unsafe { CreatePipe(&mut read, &mut write, &security, 0) })?;
    // SAFETY: CreatePipe returned two newly owned valid handles.
    Ok(unsafe {
        (
            OwnedHandle::from_raw_handle(read),
            OwnedHandle::from_raw_handle(write),
        )
    })
}

struct Attributes {
    storage: Vec<usize>,
}
impl Attributes {
    fn new(handles: &[HANDLE]) -> io::Result<Self> {
        let mut needed = 0;
        unsafe { InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut needed) };
        if needed == 0 || needed > 65536 {
            return Err(io::Error::other("invalid startup attribute size"));
        }
        let mut storage = vec![0usize; needed.div_ceil(size_of::<usize>())];
        checked(unsafe {
            InitializeProcThreadAttributeList(storage.as_mut_ptr().cast(), 1, 0, &mut needed)
        })?;
        let mut attributes = Self { storage };
        checked(unsafe {
            UpdateProcThreadAttribute(
                attributes.raw(),
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr().cast(),
                std::mem::size_of_val(handles),
                null_mut(),
                null(),
            )
        })?;
        Ok(attributes)
    }
    fn raw(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.storage.as_mut_ptr().cast()
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.raw()) };
    }
}

/// Launch only the fixed internal mode of the explicitly trusted executable.
/// All configuration, handle metadata and credentials follow on private stdin.
/// Caller owns bootstrap timeout/failure cleanup and drains stderr concurrently.
pub(super) fn launch(executable: &Executable) -> io::Result<Launched> {
    launch_inner(executable, "local-server", &[])
}

fn launch_inner(
    executable: &Executable,
    fixed_arguments: &str,
    extra: &[BorrowedHandle<'_>],
) -> io::Result<Launched> {
    let (child_in, parent_in) = pipe()?;
    let (parent_out, child_out) = pipe()?;
    let (parent_err, child_err) = pipe()?;
    for handle in [&parent_in, &parent_out, &parent_err] {
        checked(unsafe { SetHandleInformation(handle.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) })?;
    }
    let parent = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            1,
            std::process::id(),
        )
    };
    if parent.is_null() {
        return Err(io::Error::last_os_error());
    }
    let parent = unsafe { OwnedHandle::from_raw_handle(parent) };
    let mut handles = vec![
        child_in.as_raw_handle(),
        child_out.as_raw_handle(),
        child_err.as_raw_handle(),
        parent.as_raw_handle(),
    ];
    handles.extend(extra.iter().map(AsRawHandle::as_raw_handle));
    let mut attributes = Attributes::new(&handles)?;
    let application = wide(executable.path().as_os_str())?;
    // Windows file names cannot contain a quote. Arguments here are private fixed literals.
    let mut command: Vec<u16> = vec![b'"' as u16];
    command.extend_from_slice(&application[..application.len() - 1]);
    command.extend("\" ".encode_utf16());
    command.extend(fixed_arguments.encode_utf16());
    command.push(0);
    let cwd = wide(
        executable
            .path()
            .parent()
            .ok_or_else(|| io::Error::other("image directory unavailable"))?
            .as_os_str(),
    )?;
    // Explicit allowlist: never inherit provider credentials or arbitrary user environment.
    let mut environment = Vec::new();
    for key in ["SystemRoot", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(key) {
            let mut entry = std::ffi::OsString::from(key);
            entry.push("=");
            entry.push(value);
            environment.extend(wide(&entry)?);
        }
    }
    environment.push(0);
    if environment.len() == 1 {
        environment.push(0);
    }
    let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = child_in.as_raw_handle();
    startup.StartupInfo.hStdOutput = child_out.as_raw_handle();
    startup.StartupInfo.hStdError = child_err.as_raw_handle();
    startup.lpAttributeList = attributes.raw();
    let mut information: PROCESS_INFORMATION = unsafe { zeroed() };
    checked(unsafe {
        CreateProcessW(
            application.as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            1,
            CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
            environment.as_ptr().cast(),
            cwd.as_ptr(),
            &startup.StartupInfo,
            &mut information,
        )
    })?;
    let guard = ChildGuard {
        process: unsafe { OwnedHandle::from_raw_handle(information.hProcess) },
        handed_off: false,
    };
    let _thread = unsafe { OwnedHandle::from_raw_handle(information.hThread) };
    let mut process = null_mut();
    checked(unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            guard.process.as_raw_handle(),
            GetCurrentProcess(),
            &mut process,
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            0,
        )
    })?;
    let process = HeldProcess::from_owned(unsafe { OwnedHandle::from_raw_handle(process) });
    let launched = Launched {
        guard,
        process,
        stdin: File::from(parent_in),
        stdout: File::from(parent_out),
        stderr: File::from(parent_err),
        parent_proof_value: parent.as_raw_handle() as usize as u64,
    };
    let validation = launched.process.pin().and_then(|pin| {
        let principal = HeldProcess::current()?.pin()?.principal;
        if pin.image != executable.path()
            || &pin.file != executable.identity()
            || pin.principal != principal
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "launched child identity denied",
            ));
        }
        Ok(())
    });
    if let Err(error) = validation {
        let _ = launched.terminate();
        return Err(error);
    }
    Ok(launched)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, Write};
    use std::os::windows::io::AsHandle;

    #[test]
    #[ignore = "subprocess fixture for exact inherited kernel handles"]
    fn inherited_handle_child() {
        for handle in [
            std::io::stdin().as_raw_handle(),
            std::io::stdout().as_raw_handle(),
            std::io::stderr().as_raw_handle(),
        ] {
            checked(unsafe {
                SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT)
            })
            .unwrap();
        }
        super::super::windows_identity::seal_inherited_stdio().unwrap();
        for handle in [
            std::io::stdin().as_raw_handle(),
            std::io::stdout().as_raw_handle(),
            std::io::stderr().as_raw_handle(),
        ] {
            let mut flags = 0;
            checked(unsafe { GetHandleInformation(handle, &mut flags) }).unwrap();
            assert_eq!(flags & HANDLE_FLAG_INHERIT, 0);
        }
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        let raw: usize = input.trim().parse().unwrap();
        let parent = HeldProcess::inherited(raw as u64).unwrap();
        let mut flags = 0;
        checked(unsafe { GetHandleInformation(raw as HANDLE, &mut flags) }).unwrap();
        assert_eq!(flags & HANDLE_FLAG_INHERIT, 0);
        checked(unsafe { GetHandleInformation(parent.handle().as_raw_handle(), &mut flags) })
            .unwrap();
        assert_eq!(flags & HANDLE_FLAG_INHERIT, 0);
        let parent_pin = parent.pin().unwrap();
        let own = HeldProcess::current().unwrap().pin().unwrap();
        assert_ne!(parent_pin.pid, own.pid);
        assert_eq!(parent_pin.principal, own.principal);
        assert_eq!(parent_pin.image, own.image);
        assert_eq!(parent_pin.file, own.file);
        println!("proof-ready");
        std::io::stdout().flush().unwrap();
        input.clear();
        std::io::stdin().read_line(&mut input).unwrap();
    }

    #[test]
    fn native_launch_inherits_only_listed_kernel_objects() {
        let exe = Executable::open(&std::env::current_exe().unwrap()).unwrap();
        let security = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: null_mut(),
            bInheritHandle: 1,
        };
        let raw = unsafe { CreateEventW(&security, 1, 0, null()) };
        assert!(!raw.is_null());
        let decoy = unsafe { OwnedHandle::from_raw_handle(raw) };
        for include in [false, true] {
            let extra = if include {
                vec![decoy.as_handle()]
            } else {
                vec![]
            };
            let mut child = launch_inner(&exe, "--exact local::windows_launch::tests::inherited_handle_child --ignored --nocapture", &extra).unwrap();
            let outcome = (|| -> io::Result<()> {
                writeln!(child.stdin, "{}", child.parent_proof_value)?;
                child.stdin.flush()?;
                let output = child.stdout.try_clone()?;
                let (send, receive) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let result = io::BufReader::new(output)
                        .lines()
                        .any(|line| line.is_ok_and(|line| line == "proof-ready"));
                    let _ = send.send(result);
                });
                assert!(receive
                    .recv_timeout(std::time::Duration::from_secs(15))
                    .unwrap());
                let mut copied = null_mut();
                let copied_ok = unsafe {
                    DuplicateHandle(
                        child.guard.process.as_raw_handle(),
                        decoy.as_raw_handle(),
                        GetCurrentProcess(),
                        &mut copied,
                        0,
                        0,
                        DUPLICATE_SAME_ACCESS,
                    )
                };
                let same = if copied_ok != 0 {
                    let copied = unsafe { OwnedHandle::from_raw_handle(copied) };
                    unsafe {
                        CompareObjectHandles(decoy.as_raw_handle(), copied.as_raw_handle()) != 0
                    }
                } else {
                    false
                };
                assert_eq!(
                    same, include,
                    "numeric handle reuse is not kernel object inheritance"
                );
                writeln!(child.stdin, "done")?;
                child.stdin.flush()?;
                Ok(())
            })();
            let _ = child.terminate();
            outcome.unwrap();
        }
    }

    #[test]
    fn dropping_child_guard_stops_only_the_launched_process() {
        let exe = Executable::open(&std::env::current_exe().unwrap()).unwrap();
        let child = launch_inner(
            &exe,
            "--exact local::windows_launch::tests::inherited_handle_child --ignored --nocapture",
            &[],
        )
        .unwrap();
        assert!(child.process.is_alive().unwrap());
        drop(child.guard);
        assert!(!child.process.is_alive().unwrap());
        assert!(HeldProcess::current().unwrap().is_alive().unwrap());
    }

    #[test]
    fn explicit_handoff_leaves_child_alive_for_its_new_owner() {
        let exe = Executable::open(&std::env::current_exe().unwrap()).unwrap();
        let child = launch_inner(
            &exe,
            "--exact local::windows_launch::tests::inherited_handle_child --ignored --nocapture",
            &[],
        )
        .unwrap();
        // Test owns a separate cleanup guard so even an assertion failure cannot
        // leave the deliberately detached fixture process behind.
        let cleanup = ChildGuard {
            process: child.guard.process.try_clone().unwrap(),
            handed_off: false,
        };
        child.guard.handoff();
        assert!(child.process.is_alive().unwrap());
        assert!(cleanup.exit_code().is_err());
        cleanup.terminate().unwrap();
        assert!(!child.process.is_alive().unwrap());
        assert_eq!(cleanup.exit_code().unwrap(), 1);
    }
}
