// SPDX-License-Identifier: Apache-2.0
//! Native Windows feasibility adapter over the retained Codex Job Object.
//! Job membership is process containment, not filesystem/network isolation.
use super::{Error, Lifecycle};
use codex_extension_api::{HostWorkAdmission, HostWorkKind, HostWorkPermit};
use codex_protocol::ThreadId;
use codex_utils_pty::JobObject;
use std::{collections::BTreeMap, ffi::OsString, io, path::Path, process::Stdio, sync::Arc};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
    task::JoinHandle,
};

pub struct Process {
    job: Arc<JobObject>,
    child: Child,
    stdout: Option<JoinHandle<io::Result<Capture>>>,
    stderr: Option<JoinHandle<io::Result<Capture>>>,
    permit: Box<dyn HostWorkPermit>,
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
}

impl Process {
    pub fn id(&self) -> Option<u32> {
        self.child.id()
    }
    pub fn active_process_count(&self) -> io::Result<u32> {
        self.job.active_process_count()
    }

    pub async fn wait(mut self) -> io::Result<Outcome> {
        let status = self.child.wait().await?;
        // A root exit cannot detach background grandchildren from the owner.
        self.job.terminate()?;
        let stdout = self
            .stdout
            .take()
            .unwrap()
            .await
            .map_err(io::Error::other)??;
        let stderr = self
            .stderr
            .take()
            .unwrap()
            .await
            .map_err(io::Error::other)??;
        self.permit.complete().map_err(io::Error::other)?;
        Ok(Outcome {
            exit_code: status.code(),
            stdout,
            stderr,
        })
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.job.terminate();
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
) -> io::Result<Capture> {
    let mut result = Capture {
        bytes: Vec::new(),
        total: 0,
    };
    let mut buffer = [0; 8192];
    loop {
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
        if !state.attached || state.held(thread) {
            drop(state);
            permit.complete().map_err(io::Error::other)?;
            return Err(io::Error::other("process launch sealed"));
        }
        let job = Arc::new(JobObject::create_without_breakaway()?);
        let mut command = Command::new(executable);
        command
            .args(args)
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
        drop(state);
        let stdout = Some(tokio::spawn(capture(
            child.stdout.take().unwrap(),
            output_limit,
            stdout_observer,
            job.clone(),
        )));
        let stderr = Some(tokio::spawn(capture(
            child.stderr.take().unwrap(),
            output_limit,
            stderr_observer,
            job.clone(),
        )));
        Ok(Process {
            job,
            child,
            stdout,
            stderr,
            permit,
        })
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
        tokio::time::timeout(self.0.deadline, async {
            loop {
                let mut active = false;
                for job in &jobs {
                    active |= job
                        .active_process_count()
                        .map_err(|_| Error::InterruptFailed)?
                        > 0;
                }
                if !active {
                    return Ok(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .map_err(|_| Error::DrainTimeout)?
    }
}
