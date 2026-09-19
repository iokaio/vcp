// VCP modification: private lifecycle admission, recovery receipts and native stop observation.
use filedescriptor::OwnedHandle;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::os::windows::io::FromRawHandle;
use std::os::windows::io::RawHandle;
use std::sync::Mutex;
use tokio::process::Child;
use tokio::process::Command;
use winapi::shared::ntdef::NT_SUCCESS;
use winapi::shared::ntdef::NTSTATUS;
use winapi::um::jobapi2::AssignProcessToJobObject;
use winapi::um::jobapi2::CreateJobObjectW;
// VCP modification: observe kernel-confirmed process-tree quiescence.
use winapi::um::jobapi2::QueryInformationJobObject;
use winapi::um::jobapi2::SetInformationJobObject;
use winapi::um::jobapi2::TerminateJobObject;
use winapi::um::processthreadsapi::OpenProcess;
use winapi::um::processthreadsapi::TerminateProcess;
use winapi::um::winbase::CREATE_SUSPENDED;
use winapi::um::winnt::HANDLE;
use winapi::um::winnt::JOB_OBJECT_LIMIT_BREAKAWAY_OK;
use winapi::um::winnt::JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
use winapi::um::winnt::JOBOBJECT_EXTENDED_LIMIT_INFORMATION;
use winapi::um::winnt::JobObjectExtendedLimitInformation;
use winapi::um::winnt::PROCESS_SET_QUOTA;
use winapi::um::winnt::PROCESS_SUSPEND_RESUME;
use winapi::um::winnt::PROCESS_TERMINATE;

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtResumeProcess(process_handle: HANDLE) -> NTSTATUS;
}

/// Owns a Windows Job Object used to terminate a spawned process tree.
#[derive(Debug)]
pub struct JobObject {
    handle: OwnedHandle,
    // A mutex makes the state check, Job Object API call, and state update
    // atomic with respect to concurrent preserve and terminate requests.
    preserve_descendants: Mutex<bool>,
}

impl JobObject {
    /// Queries the kernel's live membership count; a successful terminate call
    /// alone does not prove all members have stopped.
    pub fn active_process_count(&self) -> io::Result<u32> {
        let mut accounting: winapi::um::winnt::JOBOBJECT_BASIC_ACCOUNTING_INFORMATION =
            unsafe { std::mem::zeroed() };
        let result = unsafe {
            QueryInformationJobObject(
                self.handle.as_raw_handle().cast(),
                winapi::um::winnt::JobObjectBasicAccountingInformation,
                std::ptr::addr_of_mut!(accounting).cast(),
                std::mem::size_of_val(&accounting) as u32,
                std::ptr::null_mut(),
            )
        };
        if result == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(accounting.ActiveProcesses)
        }
    }

    /// Creates a Job Object configured to terminate all members when its last handle closes.
    pub fn create() -> io::Result<Self> {
        let handle = unsafe { CreateJobObjectW(std::ptr::null_mut(), std::ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(handle.cast()) };

        Self::set_limit_flags(
            &handle,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK,
        )?;

        Ok(Self {
            handle,
            preserve_descendants: Mutex::new(false),
        })
    }

    /// Creates a Job Object whose owned descendants cannot explicitly break away.
    pub fn create_without_breakaway() -> io::Result<Self> {
        let job = Self::create()?;
        Self::set_limit_flags(&job.handle, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE)?;
        Ok(job)
    }

    /// VCP: bound simultaneous members, including the root, before any launch.
    pub fn create_with_process_limit(count: u32) -> io::Result<Self> {
        if count == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "zero process limit"));
        }
        let job = Self::create_without_breakaway()?;
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | winapi::um::winnt::JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
        limits.BasicLimitInformation.ActiveProcessLimit = count;
        let configured = unsafe {
            SetInformationJobObject(
                job.handle.as_raw_handle().cast(),
                JobObjectExtendedLimitInformation,
                std::ptr::addr_of_mut!(limits).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        };
        if configured == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }

    /// Captures an owned process handle before its numeric identifier can be reused.
    pub fn open_process_handle(process_id: u32) -> io::Result<std::os::windows::io::OwnedHandle> {
        let handle = unsafe {
            OpenProcess(PROCESS_TERMINATE, /*bInheritHandle*/ 0, process_id)
        };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }

        Ok(unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(handle.cast()) })
    }

    /// Terminates the exact process identified by a previously captured handle.
    pub fn terminate_process_handle(handle: &std::os::windows::io::OwnedHandle) -> io::Result<()> {
        let terminated = unsafe {
            TerminateProcess(handle.as_raw_handle().cast(), /*uExitCode*/ 1)
        };
        if terminated == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn set_limit_flags(handle: &OwnedHandle, flags: u32) -> io::Result<()> {
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = flags;
        let configured = unsafe {
            SetInformationJobObject(
                handle.as_raw_handle().cast(),
                JobObjectExtendedLimitInformation,
                std::ptr::addr_of_mut!(limits).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Assigns a running process to this job.
    ///
    /// Assignment is not retroactive: descendants created before this call
    /// completes are not guaranteed to become members of the job.
    pub(crate) fn assign_process(&self, process_handle: RawHandle) -> io::Result<()> {
        let assigned = unsafe {
            AssignProcessToJobObject(self.handle.as_raw_handle().cast(), process_handle.cast())
        };
        if assigned == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    /// Prevents a child from running before it can be assigned to this job.
    pub fn prepare_suspended_spawn(&self, command: &mut Command) {
        command.creation_flags(CREATE_SUSPENDED).kill_on_drop(true);
    }

    /// Assigns and resumes a suspended child, returning whether assignment succeeded.
    ///
    /// Nested jobs can reject assignment. Such a child is resumed without
    /// containment so callers can preserve their existing compatibility fallback.
    pub fn assign_and_resume_process(&self, process_id: u32) -> io::Result<bool> {
        let process = unsafe {
            OpenProcess(
                PROCESS_SET_QUOTA | PROCESS_TERMINATE | PROCESS_SUSPEND_RESUME,
                /*bInheritHandle*/ 0,
                process_id,
            )
        };
        if process.is_null() {
            return Err(io::Error::last_os_error());
        }
        let process = unsafe { OwnedHandle::from_raw_handle(process.cast()) };
        let assignment = self.assign_process(process.as_raw_handle());

        let status = unsafe { NtResumeProcess(process.as_raw_handle().cast()) };
        if !NT_SUCCESS(status) {
            unsafe {
                TerminateProcess(process.as_raw_handle().cast(), /*uExitCode*/ 1)
            };
            return Err(io::Error::other(format!(
                "failed to resume suspended process: NTSTATUS {status:#x}"
            )));
        }

        match assignment {
            Ok(()) => Ok(true),
            Err(error) => {
                log::warn!(
                    "Windows process job assignment unavailable for pid {process_id}: {error}"
                );
                Ok(false)
            }
        }
    }

    /// Starts a process only after assigning it to this Job Object.
    pub fn spawn_contained(&self, command: &mut Command) -> io::Result<Child> {
        self.prepare_suspended_spawn(command);
        let child = command.spawn()?;
        let process_handle = child
            .raw_handle()
            .ok_or_else(|| io::Error::other("missing child process handle"))?;
        self.assign_process(process_handle)?;

        let status = unsafe { NtResumeProcess(process_handle.cast()) };
        if !NT_SUCCESS(status) {
            return Err(io::Error::other(format!(
                "failed to resume contained process: NTSTATUS {status:#x}"
            )));
        }

        Ok(child)
    }

    /// Allows contained descendants to keep running after the root exits normally.
    ///
    /// This disables both explicit job termination and kill-on-close for this
    /// object. Calls race safely with [`Self::terminate`]: whichever operation
    /// acquires the state lock first determines whether the process tree is
    /// preserved or terminated.
    pub fn preserve_descendants(&self) -> io::Result<()> {
        let mut preserve_descendants = self
            .preserve_descendants
            .lock()
            .map_err(|_| io::Error::other("job state lock poisoned"))?;
        if *preserve_descendants {
            return Ok(());
        }

        Self::set_limit_flags(&self.handle, JOB_OBJECT_LIMIT_BREAKAWAY_OK)?;
        *preserve_descendants = true;
        Ok(())
    }

    /// Terminates every process currently assigned to the job.
    pub fn terminate(&self) -> io::Result<()> {
        let preserve_descendants = self
            .preserve_descendants
            .lock()
            .map_err(|_| io::Error::other("job state lock poisoned"))?;
        if *preserve_descendants {
            return Ok(());
        }

        let terminated = unsafe {
            TerminateJobObject(self.handle.as_raw_handle().cast(), /*uExitCode*/ 1)
        };
        if terminated == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

impl AsRawHandle for JobObject {
    fn as_raw_handle(&self) -> RawHandle {
        self.handle.as_raw_handle()
    }
}
