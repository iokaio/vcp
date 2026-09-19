// SPDX-License-Identifier: Apache-2.0
//! Test-only CreateProcess wrapper: a real console created hidden from its first
//! frame, with owned handles so failures cannot leave a fixture running.
use std::{ffi::c_void, os::windows::ffi::OsStrExt, path::Path};
#[repr(C)]
struct Startup {
    size: u32,
    reserved: *mut u16,
    desktop: *mut u16,
    title: *mut u16,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    columns: u32,
    rows: u32,
    fill: u32,
    flags: u32,
    show: u16,
    reserved_count: u16,
    reserved_bytes: *mut u8,
    stdin: isize,
    stdout: isize,
    stderr: isize,
}
#[repr(C)]
struct Information {
    process: isize,
    thread: isize,
    pid: u32,
    thread_id: u32,
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateProcessW(
        application: *const u16,
        command: *mut u16,
        process_attributes: *mut c_void,
        thread_attributes: *mut c_void,
        inherit: i32,
        flags: u32,
        environment: *mut c_void,
        directory: *const u16,
        startup: *mut Startup,
        information: *mut Information,
    ) -> i32;
    fn WaitForSingleObject(handle: isize, millis: u32) -> u32;
    fn TerminateProcess(handle: isize, code: u32) -> i32;
    fn CloseHandle(handle: isize) -> i32;
}
pub struct Console(isize);
impl Console {
    pub fn spawn(executable: &Path, root: &Path, backend: &str) -> std::io::Result<Self> {
        // Windows filenames cannot contain quotation marks; the test argument
        // is a generated temp root with no trailing separator.
        let mut command: Vec<u16> = std::ffi::OsStr::new(&format!(
            "\"{}\" \"{}\" {}",
            executable.display(),
            root.display(),
            backend
        ))
        .encode_wide()
        .chain(Some(0))
        .collect();
        let application: Vec<u16> = executable
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        // SAFETY: both structs are Win32 POD; zero means omitted optional fields.
        let mut startup: Startup = unsafe { std::mem::zeroed() };
        let mut information: Information = unsafe { std::mem::zeroed() };
        startup.size = std::mem::size_of::<Startup>() as u32;
        startup.flags = 1; // STARTF_USESHOWWINDOW
        startup.show = 0; // SW_HIDE
        let ok = unsafe {
            CreateProcessW(
                application.as_ptr(),
                command.as_mut_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
                0x00000010,
                std::ptr::null_mut(),
                std::ptr::null(),
                &mut startup,
                &mut information,
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        unsafe {
            CloseHandle(information.thread);
        }
        Ok(Self(information.process))
    }
    pub fn finished(&self) -> std::io::Result<bool> {
        match unsafe { WaitForSingleObject(self.0, 0) } {
            0 => Ok(true),
            258 => Ok(false),
            _ => Err(std::io::Error::last_os_error()),
        }
    }
    pub fn kill(&self) -> std::io::Result<()> {
        if unsafe { TerminateProcess(self.0, 137) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
}
impl Drop for Console {
    fn drop(&mut self) {
        unsafe {
            if WaitForSingleObject(self.0, 0) == 258 {
                TerminateProcess(self.0, 137);
                WaitForSingleObject(self.0, 5000);
            }
            CloseHandle(self.0);
        }
    }
}
