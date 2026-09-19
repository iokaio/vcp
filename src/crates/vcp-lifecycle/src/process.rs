// SPDX-License-Identifier: Apache-2.0
//! Native Windows feasibility adapter over the retained Codex Job Object.
//! Job membership is process containment, not filesystem/network isolation.
mod pty;
use super::{Error, Lifecycle};
use codex_extension_api::{HostWorkAdmission, HostWorkKind, HostWorkPermit};
use codex_protocol::ThreadId;
use codex_utils_pty::JobObject;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    io,
    path::Path,
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
    task::JoinHandle,
};

pub struct Process {
    _resources: Option<Arc<dyn Send + Sync>>,
    job: Arc<JobObject>,
    child: Option<Child>,
    pid: Option<u32>,
    stdout: Option<JoinHandle<io::Result<Capture>>>,
    stderr: Option<JoinHandle<io::Result<Capture>>>,
    permit: Option<Box<dyn HostWorkPermit>>,
    timer: Option<JoinHandle<()>>,
    control: Arc<Mutex<Control>>,
    completion: Option<tokio::sync::oneshot::Receiver<io::Result<Outcome>>>,
}
#[derive(Default)]
struct Control {
    total: u64,
    reason: Option<String>,
}
#[derive(Clone, Copy)]
pub struct Limits {
    pub timeout: Duration,
    pub output_bytes: u64,
    pub process_count: u32,
}

#[derive(Debug)]
pub struct Capture {
    pub bytes: Vec<u8>,
    pub total: u64,
}

/// Full observed bytes are offered before applying the display limit. Returning
/// an error terminates the owned job and leaves the work receipt unresolved.
pub type OutputObserver = Arc<dyn Fn(&[u8]) -> io::Result<()> + Send + Sync>;
#[derive(Debug)]
pub struct Outcome {
    pub exit_code: Option<i32>,
    pub stdout: Capture,
    pub stderr: Capture,
    pub stop_reason: Option<String>,
}

impl Process {
    pub fn id(&self) -> Option<u32> {
        self.pid
    }
    pub fn active_process_count(&self) -> io::Result<u32> {
        self.job.active_process_count()
    }

    pub async fn wait(mut self) -> io::Result<Outcome> {
        if let Some(completion) = self.completion.take() {
            return completion
                .await
                .map_err(|_| io::Error::other("native process observer stopped"))?;
        }
        let status = self
            .child
            .as_mut()
            .ok_or_else(|| io::Error::other("process consumed"))?
            .wait()
            .await?;
        // A root exit cannot detach background grandchildren from the owner.
        self.job.terminate()?;
        tokio::time::timeout(Duration::from_secs(2), async {
            while self.job.active_process_count()? != 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Ok::<(), io::Error>(())
        })
        .await
        .map_err(|_| io::Error::other("process tree did not quiesce"))??;
        let (stdout, stderr) =
            tokio::join!(self.stdout.take().unwrap(), self.stderr.take().unwrap());
        let stdout = stdout.map_err(io::Error::other)??;
        let stderr = stderr.map_err(io::Error::other)??;
        self.permit
            .as_mut()
            .unwrap()
            .complete()
            .map_err(io::Error::other)?;
        if let Some(timer) = self.timer.take() {
            timer.abort();
        }
        let stop_reason = self
            .control
            .lock()
            .map_err(|_| io::Error::other("poisoned process limits"))?
            .reason
            .clone();
        Ok(Outcome {
            exit_code: status.code(),
            stdout,
            stderr,
            stop_reason,
        })
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.job.terminate();
        if let Some(timer) = self.timer.take() {
            timer.abort();
        }
        if let Some(task) = &self.stdout {
            task.abort();
        }
        if let Some(task) = &self.stderr {
            task.abort();
        }
    }
}

async fn capture(
    mut input: impl AsyncRead + Unpin,
    limit: usize,
    observer: Option<OutputObserver>,
    job: Arc<JobObject>,
    control: Arc<Mutex<Control>>,
    ceiling: Option<u64>,
) -> io::Result<Capture> {
    let mut result = Capture {
        bytes: Vec::new(),
        total: 0,
    };
    let mut buffer = [0; 8192];
    loop {
        // Once the global output limit fires, stop reading. At most one bounded
        // read per stream can already be in flight; every observed byte is still
        // captured before the limit result is returned.
        if control
            .lock()
            .map_err(|_| io::Error::other("poisoned process limits"))?
            .reason
            .as_deref()
            == Some("output limit exceeded")
        {
            return Ok(result);
        }
        let count = input.read(&mut buffer).await?;
        if count == 0 {
            return Ok(result);
        }
        if let Some(observer) = &observer {
            if let Err(error) = observer(&buffer[..count]) {
                let _ = job.terminate();
                return Err(error);
            }
        }
        result.total += count as u64;
        let keep = count.min(limit.saturating_sub(result.bytes.len()));
        result.bytes.extend_from_slice(&buffer[..keep]);
        let exceeded = {
            let mut state = control
                .lock()
                .map_err(|_| io::Error::other("poisoned process limits"))?;
            state.total = state.total.saturating_add(count as u64);
            if ceiling.is_some_and(|max| state.total > max) {
                state
                    .reason
                    .get_or_insert_with(|| "output limit exceeded".into());
                true
            } else {
                false
            }
        };
        if exceeded {
            job.terminate()?;
            return Ok(result);
        }
    }
}

impl Lifecycle {
    /// Explicit executable, argv, cwd and environment. No shell interpolation or
    /// implicit environment inheritance. Unknown/held scopes cannot launch.
    pub fn spawn_process(
        &self,
        thread: ThreadId,
        executable: &Path,
        args: &[OsString],
        cwd: &Path,
        environment: &BTreeMap<OsString, OsString>,
        output_limit: usize,
    ) -> io::Result<Process> {
        self.spawn_process_with_capture(
            thread,
            executable,
            args,
            cwd,
            environment,
            output_limit,
            None,
            None,
        )
    }

    pub fn spawn_process_with_capture(
        &self,
        thread: ThreadId,
        executable: &Path,
        args: &[OsString],
        cwd: &Path,
        environment: &BTreeMap<OsString, OsString>,
        output_limit: usize,
        stdout_observer: Option<OutputObserver>,
        stderr_observer: Option<OutputObserver>,
    ) -> io::Result<Process> {
        self.spawn_bounded_process_with_capture(
            thread,
            executable,
            args,
            cwd,
            environment,
            output_limit,
            stdout_observer,
            stderr_observer,
            None,
            false,
            None,
        )
    }
    pub fn spawn_bounded_process_with_capture(
        &self,
        thread: ThreadId,
        executable: &Path,
        args: &[OsString],
        cwd: &Path,
        environment: &BTreeMap<OsString, OsString>,
        output_limit: usize,
        stdout_observer: Option<OutputObserver>,
        stderr_observer: Option<OutputObserver>,
        limits: Option<Limits>,
        cmd_script: bool,
        resources: Option<Arc<dyn Send + Sync>>,
    ) -> io::Result<Process> {
        self.spawn_bounded_process_with_capture_generation(
            thread,
            executable,
            args,
            cwd,
            environment,
            output_limit,
            stdout_observer,
            stderr_observer,
            limits,
            cmd_script,
            resources,
            None,
        )
    }
    pub(crate) fn spawn_bounded_process_with_capture_generation(
        &self,
        thread: ThreadId,
        executable: &Path,
        args: &[OsString],
        cwd: &Path,
        environment: &BTreeMap<OsString, OsString>,
        output_limit: usize,
        stdout_observer: Option<OutputObserver>,
        stderr_observer: Option<OutputObserver>,
        limits: Option<Limits>,
        cmd_script: bool,
        resources: Option<Arc<dyn Send + Sync>>,
        generation: Option<u64>,
    ) -> io::Result<Process> {
        if limits.is_some_and(|l| {
            l.timeout.is_zero()
                || l.timeout > Duration::from_secs(120)
                || l.output_bytes == 0
                || l.output_bytes > 8 * 1024 * 1024
                || !(1..=128).contains(&l.process_count)
        }) {
            return Err(io::Error::other("process limits"));
        }
        if !executable.is_absolute() || !cwd.is_absolute() || output_limit > 1024 * 1024 {
            return Err(io::Error::other(
                "absolute paths and a bounded capture limit are required",
            ));
        }
        let mut permit =
            HostWorkAdmission::admit(self, thread, HostWorkKind::Tool, "native-process")
                .map_err(io::Error::other)?;
        let state = self
            .0
            .state
            .lock()
            .map_err(|_| io::Error::other("poisoned lifecycle"))?;
        if !state.attached
            || state.held(thread)
            || generation.is_some_and(|expected| !state.admission_current(thread, expected))
        {
            drop(state);
            permit.complete().map_err(io::Error::other)?;
            return Err(io::Error::other("process launch sealed"));
        }
        let job = Arc::new(match limits {
            Some(limits) => JobObject::create_with_process_limit(limits.process_count)?,
            None => JobObject::create_without_breakaway()?,
        });
        let mut command = Command::new(executable);
        if cmd_script {
            use std::os::windows::process::CommandExt;
            if args.len() != 4 || args[0] != "/d" || args[1] != "/s" || args[2] != "/c" {
                return Err(io::Error::other("explicit cmd script conversion required"));
            }
            let script = args[3]
                .to_str()
                .ok_or_else(|| io::Error::other("Unicode cmd script required"))?;
            // cmd does not use the CRT argv escaping rules. /s removes these
            // outer quotes; quotes and metacharacters inside remain the exact
            // explicitly authorized script, never an implicitly inferred shell.
            command.args(&args[..3]);
            command.as_std_mut().raw_arg(format!("\"{script}\""));
        } else {
            command.args(args);
        }
        command
            .current_dir(cwd)
            .env_clear()
            .envs(environment)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Retained implementation creates suspended, assigns via owned handle,
        // then resumes. Assignment failure cannot run an uncontained fallback.
        let mut child = job.spawn_contained(&mut command)?;
        self.0
            .jobs
            .lock()
            .map_err(|_| io::Error::other("poisoned jobs"))?
            .entry(thread)
            .or_default()
            .push(job.clone());
        let observer = if limits.is_some() {
            let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
            self.0
                .process_observers
                .lock()
                .map_err(|_| io::Error::other("poisoned process observers"))?
                .entry(thread)
                .or_default()
                .push(done.clone());
            Some(done)
        } else {
            None
        };
        drop(state);
        let control = Arc::new(Mutex::new(Control::default()));
        let timer = limits.map(|limits| {
            let job = job.clone();
            let control = control.clone();
            tokio::spawn(async move {
                tokio::time::sleep(limits.timeout).await;
                if let Ok(mut state) = control.lock() {
                    state
                        .reason
                        .get_or_insert_with(|| "process deadline elapsed".into());
                }
                let _ = job.terminate();
            })
        });
        let stdout = Some(tokio::spawn(capture(
            child.stdout.take().unwrap(),
            output_limit,
            stdout_observer,
            job.clone(),
            control.clone(),
            limits.map(|l| l.output_bytes),
        )));
        let stderr = Some(tokio::spawn(capture(
            child.stderr.take().unwrap(),
            output_limit,
            stderr_observer,
            job.clone(),
            control.clone(),
            limits.map(|l| l.output_bytes),
        )));
        let pid = child.id();
        let process = Process {
            _resources: resources,
            job,
            child: Some(child),
            pid,
            stdout,
            stderr,
            permit: Some(permit),
            timer,
            control,
            completion: None,
        };
        if let Some(done) = observer {
            // Bounded broker processes always have an owned observer. Owner
            // close drains it before closing the checkpoint, independent of
            // whether the caller has begun waiting for the returned result.
            struct Completed(Arc<std::sync::atomic::AtomicBool>);
            impl Drop for Completed {
                fn drop(&mut self) {
                    self.0.store(true, std::sync::atomic::Ordering::Release);
                }
            }
            let (sender, receiver) = tokio::sync::oneshot::channel();
            let owned = Process {
                _resources: None,
                job: process.job.clone(),
                child: None,
                pid,
                stdout: None,
                stderr: None,
                permit: None,
                timer: None,
                control: process.control.clone(),
                completion: Some(receiver),
            };
            tokio::spawn(async move {
                let _done = Completed(done);
                let result = process.wait().await;
                let _ = sender.send(result);
            });
            Ok(owned)
        } else {
            Ok(process)
        }
    }

    pub(super) async fn stop_processes(&self, threads: &[ThreadId]) -> Result<(), Error> {
        let jobs: Vec<_> = {
            let mut all = self.0.jobs.lock().map_err(|_| Error::Poisoned)?;
            threads
                .iter()
                .flat_map(|id| {
                    let jobs = all.entry(*id).or_default();
                    jobs.clone()
                })
                .collect()
        };
        for job in &jobs {
            job.terminate().map_err(|_| Error::InterruptFailed)?;
        }
        let observers: Vec<_> = {
            let all = self
                .0
                .process_observers
                .lock()
                .map_err(|_| Error::Poisoned)?;
            threads
                .iter()
                .flat_map(|id| all.get(id).into_iter().flatten().cloned())
                .collect()
        };
        tokio::time::timeout(self.0.deadline, async {
            loop {
                let mut active = false;
                for job in &jobs {
                    active |= job
                        .active_process_count()
                        .map_err(|_| Error::InterruptFailed)?
                        > 0;
                }
                if !active
                    && observers
                        .iter()
                        .all(|done| done.load(std::sync::atomic::Ordering::Acquire))
                {
                    return Ok(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .map_err(|_| Error::DrainTimeout)?
    }
}
