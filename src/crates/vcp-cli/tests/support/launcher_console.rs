// SPDX-License-Identifier: Apache-2.0
//! Qualification-only real console broker. Never attaches to an existing console.
use codex_utils_pty::JobObject;
use serde_json::{json, Value};
use std::{
    ffi::{c_void, OsStr},
    fs,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
    process::Stdio,
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant},
};
use windows_sys::Win32::System::Threading::{
    CreateProcessW, GetExitCodeProcess, OpenProcess, QueryFullProcessImageNameW, ResumeThread,
    TerminateProcess, WaitForSingleObject, PROCESS_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, STARTUPINFOW,
};

type Result<T> = std::result::Result<T, String>;
const PROMPT: &str = "Resume a task by number, or press Enter to leave it paused:";
static EVENTS: AtomicU32 = AtomicU32::new(0);

#[repr(C)]
#[derive(Clone, Copy)]
struct Coord {
    x: i16,
    y: i16,
}
#[repr(C)]
struct Rect {
    left: i16,
    top: i16,
    right: i16,
    bottom: i16,
}
#[repr(C)]
struct Screen {
    size: Coord,
    cursor: Coord,
    attributes: u16,
    window: Rect,
    maximum: Coord,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
    fn GenerateConsoleCtrlEvent(event: u32, group: u32) -> i32;
    fn GetConsoleProcessList(processes: *mut u32, count: u32) -> u32;
    fn GetStdHandle(kind: u32) -> *mut c_void;
    fn GetConsoleScreenBufferInfo(handle: *mut c_void, info: *mut Screen) -> i32;
    fn ReadConsoleOutputCharacterW(
        handle: *mut c_void,
        text: *mut u16,
        length: u32,
        position: Coord,
        read: *mut u32,
    ) -> i32;
    fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
    fn IsProcessInJob(process: *mut c_void, job: *mut c_void, result: *mut i32) -> i32;
}

fn os_error() -> String {
    std::io::Error::last_os_error().to_string()
}
fn ensure(value: bool, message: &str) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

/// Windows CRT argument quoting, including trailing slashes and embedded quotes.
fn quote(value: &OsStr) -> Vec<u16> {
    let mut out = vec![b'"' as u16];
    let mut slashes = 0;
    for ch in value.encode_wide() {
        if ch == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        out.extend(std::iter::repeat_n(
            b'\\' as u16,
            if ch == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        slashes = 0;
        out.push(ch);
    }
    out.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    out.push(b'"' as u16);
    out
}

const ENVIRONMENT: &[&str] = &[
    "SystemRoot",
    "WINDIR",
    "USERPROFILE",
    "LOCALAPPDATA",
    "APPDATA",
    "TEMP",
    "TMP",
    "ProgramFiles",
    "ProgramFiles(x86)",
    "PROCESSOR_ARCHITECTURE",
];

pub fn clean_environment(command: &mut tokio::process::Command) {
    command.env_clear();
    for name in ENVIRONMENT {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    if let Some(root) = std::env::var_os("SystemRoot") {
        let root = PathBuf::from(root);
        command.env(
            "PATH",
            std::env::join_paths([root.join("System32"), root]).unwrap(),
        );
    }
}

fn environment(input: &Path) -> Vec<u16> {
    let mut rows: Vec<(String, std::ffi::OsString)> = ENVIRONMENT
        .iter()
        .filter_map(|key| std::env::var_os(key).map(|value| ((*key).into(), value)))
        .collect();
    rows.push((
        "VCP_BETA_CONSOLE_BROKER_INPUT".into(),
        input.as_os_str().to_owned(),
    ));
    rows.sort_by_key(|(key, _)| key.to_ascii_uppercase());
    let mut block = Vec::new();
    for (key, value) in rows {
        block.extend(key.encode_utf16());
        block.push(b'=' as u16);
        block.extend(value.encode_wide());
        block.push(0);
    }
    block.push(0);
    block
}

struct Process(OwnedHandle);
impl Process {
    fn open(pid: u32) -> Result<Self> {
        // Synchronize/query only; these handles can never signal unrelated PIDs.
        let raw = unsafe {
            OpenProcess(
                PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                pid,
            )
        };
        ensure(
            !raw.is_null(),
            "cannot open owned process observation handle",
        )?;
        Ok(Self(unsafe { OwnedHandle::from_raw_handle(raw) }))
    }
    fn exit(&self) -> Result<Option<u32>> {
        match unsafe { WaitForSingleObject(self.0.as_raw_handle(), 0) } {
            258 => Ok(None),
            0 => {
                let mut code = 0;
                ensure(
                    unsafe { GetExitCodeProcess(self.0.as_raw_handle(), &mut code) } != 0,
                    "exit code unavailable",
                )?;
                Ok(Some(code))
            }
            _ => Err(os_error()),
        }
    }
    fn image(&self) -> Result<PathBuf> {
        let mut buffer = vec![0u16; 32768];
        let mut size = buffer.len() as u32;
        ensure(
            unsafe {
                QueryFullProcessImageNameW(
                    self.0.as_raw_handle(),
                    0,
                    buffer.as_mut_ptr(),
                    &mut size,
                )
            } != 0,
            "process image unavailable",
        )?;
        use std::os::windows::ffi::OsStringExt;
        fs::canonicalize(PathBuf::from(std::ffi::OsString::from_wide(
            &buffer[..size as usize],
        )))
        .map_err(|e| e.to_string())
    }
    fn in_job(&self, job: &JobObject) -> Result<bool> {
        let mut yes = 0;
        ensure(
            unsafe { IsProcessInJob(self.0.as_raw_handle(), job.as_raw_handle(), &mut yes) } != 0,
            "job membership unavailable",
        )?;
        Ok(yes != 0)
    }
}

fn console_processes() -> Result<Vec<u32>> {
    let mut pids = [0u32; 16];
    let count = unsafe { GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32) };
    ensure(
        count > 0 && count as usize <= pids.len(),
        "console membership unavailable or exceeds bound",
    )?;
    let mut values = pids[..count as usize].to_vec();
    values.sort_unstable();
    Ok(values)
}

fn screen() -> Result<String> {
    // This handle belongs only to the broker's newly created hidden console.
    let handle = unsafe { GetStdHandle((-11i32) as u32) };
    let mut info: Screen = unsafe { std::mem::zeroed() };
    ensure(
        unsafe { GetConsoleScreenBufferInfo(handle, &mut info) } != 0,
        "owned console screen unavailable",
    )?;
    ensure(
        info.size.x > 0 && info.size.y > 0,
        "invalid console dimensions",
    )?;
    let count = (info.size.x as usize * info.size.y as usize).min(65536);
    let mut buffer = vec![0u16; count];
    let mut read = 0;
    ensure(
        unsafe {
            ReadConsoleOutputCharacterW(
                handle,
                buffer.as_mut_ptr(),
                count as u32,
                Coord { x: 0, y: 0 },
                &mut read,
            )
        } != 0,
        "owned console read failed",
    )?;
    Ok(String::from_utf16_lossy(&buffer[..read as usize]))
}

unsafe extern "system" fn control(event: u32) -> i32 {
    if event <= 1 {
        EVENTS.fetch_or(1 << event, Ordering::SeqCst);
        1
    } else {
        0
    }
}

fn install_handler() -> Result<()> {
    // PowerShell/supervisors can pass an inherited Ctrl+C-ignore attribute.
    // Explicitly enable Ctrl+C in this new console before launching children;
    // never use NULL/TRUE, which would make those children ignore the event.
    ensure(
        unsafe { SetConsoleCtrlHandler(None, 0) } != 0,
        "cannot enable Ctrl+C in owned console",
    )?;
    ensure(
        unsafe { SetConsoleCtrlHandler(Some(control), 1) } != 0,
        "broker handler failed",
    )
}

pub async fn self_test() -> Result<Value> {
    ensure(
        console_processes()? == [std::process::id()],
        "self-test console not newly isolated",
    )?;
    install_handler()?;
    eprintln!("{PROMPT}");
    ensure(
        screen()?.contains(PROMPT),
        "real console screen readiness unavailable",
    )?;
    for event in [0, 1] {
        ensure(
            unsafe { GenerateConsoleCtrlEvent(event, 0) } != 0,
            "self-test console event failed",
        )?;
        let began = Instant::now();
        while EVENTS.load(Ordering::SeqCst) & (1 << event) == 0 {
            ensure(
                began.elapsed() < Duration::from_secs(2),
                "self-test event not observed",
            )?;
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    ensure(
        console_processes()? == [std::process::id()],
        "self-test console acquired unexpected members",
    )?;
    Ok(json!({"schema":"vcp-console-broker-self-test/1","status":"pass","events":[0,1]}))
}

/// Parent owns this job before the broker's first instruction. SW_HIDE applies
/// from creation, and CREATE_NEW_CONSOLE prevents any user-console attachment.
pub async fn run_broker(input: &Path) -> Result<Value> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let application = wide(exe.as_os_str());
    let mut command = quote(exe.as_os_str());
    command.extend(" --exact launcher_console_broker --ignored --nocapture".encode_utf16());
    command.push(0);
    let mut environment = environment(input);
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    let mut information: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    startup.dwFlags = 1;
    startup.wShowWindow = 0; // STARTF_USESHOWWINDOW, SW_HIDE
    let job = JobObject::create_without_breakaway().map_err(|e| e.to_string())?;
    ensure(
        unsafe {
            CreateProcessW(
                application.as_ptr(),
                command.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                0x0000_0010 | 0x0000_0004 | 0x0000_0400, // NEW_CONSOLE | SUSPENDED | UNICODE_ENVIRONMENT
                environment.as_mut_ptr().cast(),
                std::ptr::null(),
                &startup,
                &mut information,
            )
        } != 0,
        "hidden broker creation failed",
    )?;
    let process = Process(unsafe { OwnedHandle::from_raw_handle(information.hProcess) });
    let thread = unsafe { OwnedHandle::from_raw_handle(information.hThread) };
    if unsafe { AssignProcessToJobObject(job.as_raw_handle(), process.0.as_raw_handle()) } == 0 {
        unsafe {
            TerminateProcess(process.0.as_raw_handle(), 1);
            WaitForSingleObject(process.0.as_raw_handle(), 10000);
        }
        return Err("broker containment refused before execution".into());
    }
    ensure(
        unsafe { ResumeThread(thread.as_raw_handle()) } != u32::MAX,
        "broker resume failed",
    )?;
    let started = Instant::now();
    let outcome = loop {
        if let Some(code) = process.exit()? {
            break Ok(code);
        }
        if started.elapsed() >= Duration::from_secs(60) {
            break Err("console broker exceeded 60 seconds".to_owned());
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    let cleanup = drain(&job, Duration::from_secs(5)).await?;
    if !cleanup {
        job.terminate().map_err(|e| e.to_string())?;
    }
    let stopped = drain(&job, Duration::from_secs(10)).await?;
    ensure(
        stopped && cleanup,
        "console broker required forced cleanup or retained descendants",
    )?;
    let exit_code = outcome?;
    let input: Value = serde_json::from_slice(&fs::read(input).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let report: Value = serde_json::from_slice(
        &fs::read(input["output"].as_str().ok_or("missing broker output")?)
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if exit_code != 0 || report["status"] != "pass" {
        return Err(format!("console broker failed: {}", report["error"]));
    }
    Ok(report)
}

pub async fn drain(job: &JobObject, limit: Duration) -> Result<bool> {
    let began = Instant::now();
    loop {
        if job.active_process_count().map_err(|e| e.to_string())? == 0 {
            return Ok(true);
        }
        if began.elapsed() >= limit {
            return Ok(false);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

pub async fn broker(input: &Value) -> Result<Value> {
    ensure(
        console_processes()? == [std::process::id()],
        "broker console was not newly isolated",
    )?;
    // A custom handler is not inherited. NULL/TRUE would make the native child
    // ignore Ctrl+C and invalidate the measurement, so it is never used here.
    install_handler()?;
    let signal = input["signal"].as_u64().ok_or("missing signal")? as u32;
    ensure(signal <= 1, "unsupported console event")?;
    let executable = PathBuf::from(input["executable"].as_str().ok_or("missing executable")?);
    let engine = fs::canonicalize(input["engine"].as_str().ok_or("missing engine")?)
        .map_err(|e| e.to_string())?;
    let launched = input["mode"] == "launcher";
    let mut command = tokio::process::Command::new(&executable);
    command
        .args([
            "--workspace",
            input["workspace"].as_str().ok_or("missing workspace")?,
            "--data-dir",
            input["data"].as_str().ok_or("missing data")?,
        ])
        .current_dir(input["workspace"].as_str().ok_or("missing workspace")?)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    clean_environment(&mut command);
    let job = JobObject::create_without_breakaway().map_err(|e| e.to_string())?;
    let mut child = job
        .spawn_contained(&mut command)
        .map_err(|e| e.to_string())?;
    let outcome = observe(&job, &mut child, &executable, &engine, launched, signal).await;
    let clean = drain(&job, Duration::from_secs(2)).await?;
    if !clean {
        job.terminate().map_err(|e| e.to_string())?;
    }
    let stopped = drain(&job, Duration::from_secs(10)).await?;
    let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    ensure(
        clean && stopped,
        "native console case required forced cleanup or retained descendants",
    )?;
    outcome
}

async fn observe(
    job: &JobObject,
    child: &mut tokio::process::Child,
    executable: &Path,
    engine: &Path,
    launched: bool,
    signal: u32,
) -> Result<Value> {
    let began = Instant::now();
    loop {
        ensure(
            child.try_wait().map_err(|e| e.to_string())?.is_none(),
            "native process exited before paused chooser",
        )?;
        if screen()?.contains(PROMPT) {
            break;
        }
        ensure(
            began.elapsed() < Duration::from_secs(30),
            "paused chooser readiness exceeded 30 seconds",
        )?;
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let readiness_ms = began.elapsed().as_millis();
    let pids = console_processes()?;
    let child_id = child.id().ok_or("native child identity unavailable")?;
    let mut native = None;
    let expected = if launched { 3 } else { 2 };
    ensure(
        pids.len() == expected && pids.contains(&child_id) && pids.contains(&std::process::id()),
        "unexpected console membership; event refused",
    )?;
    let executable = fs::canonicalize(executable).map_err(|e| e.to_string())?;
    for pid in pids.iter().filter(|id| **id != std::process::id()) {
        let process = Process::open(*pid)?;
        ensure(
            process.in_job(job)?,
            "console process is outside owned job; event refused",
        )?;
        let image = process.image()?;
        if *pid == child_id {
            ensure(image == executable, "root process image differs")?;
        }
        if image == engine {
            ensure(native.is_none(), "more than one native engine in chooser")?;
            native = Some((*pid, process));
        } else {
            ensure(
                launched && *pid == child_id && image == executable,
                "unknown console process image; event refused",
            )?;
        }
    }
    ensure(
        job.active_process_count().map_err(|e| e.to_string())? as usize == expected - 1,
        "unexpected non-console descendants; event refused",
    )?;
    let (native_id, native) = native.ok_or("native engine not attached to owned console")?;
    ensure(native.exit()?.is_none(), "engine exited before event")?;
    ensure(
        console_processes()? == pids,
        "console membership changed before event",
    )?;
    let sent = Instant::now();
    // Group zero reaches this newly created console only. A nonzero Ctrl+C
    // process group would misleadingly return success without delivering it.
    ensure(
        unsafe { GenerateConsoleCtrlEvent(signal, 0) } != 0,
        "real console event delivery failed",
    )?;
    let exit = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status.code().ok_or("native root exit code unavailable")? as u32;
        }
        ensure(
            sent.elapsed() < Duration::from_secs(10),
            "native root did not exit within 10 seconds of event",
        )?;
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    ensure(
        drain(job, Duration::from_secs(5)).await?,
        "owned descendants survived console cancellation",
    )?;
    let native_exit = native
        .exit()?
        .ok_or("native engine handle remains active")?;
    ensure(
        native_exit == exit,
        "launcher exit differs from actual native child",
    )?;
    ensure(
        console_processes()? == [std::process::id()],
        "console retains descendants after event",
    )?;
    ensure(
        EVENTS.load(Ordering::SeqCst) & (1 << signal) != 0,
        "broker did not observe console event",
    )?;
    Ok(
        json!({"status":"pass","event":signal,"event_api":"GenerateConsoleCtrlEvent",
        "event_group":0,"readiness":"paused-task-chooser","readiness_ms":readiness_ms,
        "signal_to_exit_ms":sent.elapsed().as_millis(),"root_pid":child_id,"engine_pid":native_id,
        "console_pids_before":pids,"root_exit_u32":exit,"engine_exit_u32":native_exit,
        "owned_job_active_processes":0,"console_members_after":[std::process::id()],"forced_cleanup":false}),
    )
}
