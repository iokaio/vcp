// SPDX-License-Identifier: Apache-2.0
//! Test-only hidden process startup with containment before its first instruction.
use codex_utils_pty::JobObject;
use std::{ffi::c_void, os::windows::io::AsRawHandle, time::Duration};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
}
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtResumeProcess(process: *mut c_void) -> i32;
}

/// JobObject::spawn_contained replaces creation_flags with CREATE_SUSPENDED.
/// Preserve CREATE_NO_WINDOW explicitly, with no uncontained fallback.
pub async fn spawn(
    job: &JobObject,
    command: &mut tokio::process::Command,
) -> Result<tokio::process::Child, String> {
    command
        .creation_flags(0x0800_0000 | 0x0000_0004)
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let handle = child
        .raw_handle()
        .ok_or("hidden child process handle unavailable")?;
    // The process is suspended; assignment precedes the first instruction and
    // therefore every descendant. The job is noninheritable and caller-owned.
    let assigned = unsafe { AssignProcessToJobObject(job.as_raw_handle(), handle) } != 0;
    if !assigned || unsafe { NtResumeProcess(handle) } < 0 {
        let _ = child.start_kill();
        let reaped = tokio::time::timeout(Duration::from_secs(10), child.wait()).await;
        return Err(if matches!(reaped, Ok(Ok(_))) {
            "hidden process containment or resume refused"
        } else {
            "hidden process startup failed and reap did not complete"
        }
        .into());
    }
    Ok(child)
}
