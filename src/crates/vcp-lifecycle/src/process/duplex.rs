// SPDX-License-Identifier: Apache-2.0
//! Owned newline-framed pipes. This transport grants no protocol or tool authority.
use super::*;
use std::{
    future::poll_fn,
    pin::Pin,
    sync::atomic::{AtomicBool, Ordering},
    task::Poll,
};
use tokio::{io::AsyncWrite, process::ChildStdin, sync::mpsc};

#[derive(Clone, Copy)]
pub struct DuplexLimits {
    pub process: Limits,
    pub frame_bytes: usize,
    pub queued_frames: usize,
    pub input_bytes: u64,
    /// Optional independent stderr quota; zero permits no stderr bytes.
    pub stderr_bytes: Option<u64>,
}
pub struct Duplex {
    process: Option<Process>,
    input: Option<ChildStdin>,
    frames: mpsc::Receiver<Vec<u8>>,
    lifecycle: Lifecycle,
    thread: ThreadId,
    generation: u64,
    limits: DuplexLimits,
    written: u64,
}
fn stopped(job: &JobObject, control: &Mutex<Control>, reason: &str) -> io::Result<()> {
    control
        .lock()
        .map_err(|_| io::Error::other("poisoned duplex limits"))?
        .reason
        .get_or_insert_with(|| reason.into());
    job.terminate()
}
impl Duplex {
    pub(crate) fn owned_job(&self) -> io::Result<Arc<JobObject>> {
        self.process
            .as_ref()
            .map(|process| process.job.clone())
            .ok_or_else(|| io::Error::other("duplex consumed"))
    }
    pub fn id(&self) -> Option<u32> {
        self.process.as_ref().and_then(Process::id)
    }
    pub fn active_process_count(&self) -> io::Result<u32> {
        self.process
            .as_ref()
            .ok_or_else(|| io::Error::other("duplex consumed"))?
            .active_process_count()
    }
    pub fn terminate(&self) -> io::Result<()> {
        let process = self
            .process
            .as_ref()
            .ok_or_else(|| io::Error::other("duplex consumed"))?;
        stopped(&process.job, &process.control, "duplex terminated")
    }
    pub fn close_stdin(&mut self) {
        self.input.take();
    }
    pub async fn wait(mut self) -> io::Result<Outcome> {
        self.close_stdin();
        self.process
            .take()
            .ok_or_else(|| io::Error::other("duplex consumed"))?
            .wait()
            .await
    }
    /// A cancelled partial write terminates the connection: framing cannot be resumed safely.
    pub async fn write_line(&mut self, bytes: &[u8]) -> io::Result<()> {
        if bytes.is_empty()
            || bytes.len() > self.limits.frame_bytes
            || bytes.contains(&b'\n')
            || bytes.contains(&b'\r')
        {
            return Err(io::Error::other("duplex input frame bound or delimiter"));
        }
        let next = self
            .written
            .checked_add(bytes.len() as u64 + 1)
            .filter(|total| *total <= self.limits.input_bytes)
            .ok_or_else(|| io::Error::other("duplex input lifetime limit"))?;
        let process = self
            .process
            .as_ref()
            .ok_or_else(|| io::Error::other("duplex consumed"))?;
        struct Writing {
            job: Arc<JobObject>,
            control: Arc<Mutex<Control>>,
            complete: bool,
        }
        impl Drop for Writing {
            fn drop(&mut self) {
                if !self.complete {
                    let _ = stopped(&self.job, &self.control, "duplex write interrupted");
                }
            }
        }
        let mut writing = Writing {
            job: process.job.clone(),
            control: process.control.clone(),
            complete: false,
        };
        let input = self
            .input
            .as_mut()
            .ok_or_else(|| io::Error::other("duplex stdin closed"))?;
        self.written = next;
        for chunk in [bytes, b"\n".as_slice()] {
            let mut offset = 0;
            while offset < chunk.len() {
                let count = poll_fn(|cx| {
                    let state = match self.lifecycle.0.state.lock() {
                        Ok(state) => state,
                        Err(_) => return Poll::Ready(Err(io::Error::other("poisoned lifecycle"))),
                    };
                    if !state.attached
                        || state.held(self.thread)
                        || !state.admission_current(self.thread, self.generation)
                    {
                        return Poll::Ready(Err(io::Error::other("duplex input admission sealed")));
                    }
                    Pin::new(&mut *input).poll_write(cx, &chunk[offset..])
                })
                .await?;
                if count == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "duplex write returned zero",
                    ));
                }
                offset += count;
            }
        }
        poll_fn(|cx| {
            let state = match self.lifecycle.0.state.lock() {
                Ok(state) => state,
                Err(_) => return Poll::Ready(Err(io::Error::other("poisoned lifecycle"))),
            };
            if !state.attached
                || state.held(self.thread)
                || !state.admission_current(self.thread, self.generation)
            {
                return Poll::Ready(Err(io::Error::other("duplex input admission sealed")));
            }
            Pin::new(&mut *input).poll_flush(cx)
        })
        .await?;
        writing.complete = true;
        Ok(())
    }
    pub async fn read_line(&mut self) -> io::Result<Option<Vec<u8>>> {
        let control = self
            .process
            .as_ref()
            .ok_or_else(|| io::Error::other("duplex consumed"))?
            .control
            .clone();
        // Retain only the shared control, not a shared Process reference: its
        // owned scheduler permit is Send but intentionally does not require Sync.
        let lifecycle = &self.lifecycle;
        let frames = &mut self.frames;
        let thread = self.thread;
        let generation = self.generation;
        poll_fn(|cx| {
            let state = match lifecycle.0.state.lock() {
                Ok(state) => state,
                Err(_) => return Poll::Ready(Err(io::Error::other("poisoned lifecycle"))),
            };
            if !state.attached || state.held(thread) || !state.admission_current(thread, generation)
            {
                return Poll::Ready(Err(io::Error::other("duplex output admission sealed")));
            }
            let control = match control.lock() {
                Ok(control) => control,
                Err(_) => return Poll::Ready(Err(io::Error::other("poisoned duplex limits"))),
            };
            if let Some(reason) = &control.reason {
                return Poll::Ready(Err(io::Error::other(reason.clone())));
            }
            frames.poll_recv(cx).map(Ok)
        })
        .await
    }
}
impl Drop for Duplex {
    fn drop(&mut self) {
        if self.process.is_some() {
            let _ = self.terminate();
        }
    }
}

async fn frames(
    mut output: impl AsyncRead + Unpin,
    sender: mpsc::Sender<Vec<u8>>,
    limits: DuplexLimits,
    capture_limit: usize,
    observer: Option<OutputObserver>,
    job: Arc<JobObject>,
    control: Arc<Mutex<Control>>,
) -> io::Result<Capture> {
    let mut result = Capture {
        bytes: Vec::new(),
        total: 0,
    };
    let mut frame = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let count = output.read(&mut buffer).await?;
        if count == 0 {
            if !frame.is_empty() {
                stopped(&job, &control, "duplex stdout ended inside a frame")?;
            }
            return Ok(result);
        }
        if let Some(observer) = &observer {
            if let Err(error) = observer(&buffer[..count]) {
                let _ = job.terminate();
                return Err(error);
            }
        }
        result.total += count as u64;
        result.bytes.extend_from_slice(
            &buffer[..count.min(capture_limit.saturating_sub(result.bytes.len()))],
        );
        let exceeded = {
            let mut state = control
                .lock()
                .map_err(|_| io::Error::other("poisoned duplex limits"))?;
            state.total = state.total.saturating_add(count as u64);
            state.total > limits.process.output_bytes
        };
        if exceeded {
            stopped(&job, &control, "output limit exceeded")?;
            return Ok(result);
        }
        for byte in &buffer[..count] {
            if *byte == b'\n' {
                if frame.last() == Some(&b'\r') {
                    frame.pop();
                }
                if sender.try_send(std::mem::take(&mut frame)).is_err() {
                    stopped(&job, &control, "duplex frame queue unavailable or full")?;
                    return Ok(result);
                }
            } else {
                if frame.len() == limits.frame_bytes {
                    stopped(&job, &control, "duplex output frame limit")?;
                    return Ok(result);
                }
                frame.push(*byte);
            }
        }
    }
}

impl Lifecycle {
    #[allow(clippy::too_many_arguments)] // Mirrors the existing explicit native process boundary.
    pub fn spawn_duplex_process(
        &self,
        thread: ThreadId,
        executable: &Path,
        args: &[OsString],
        cwd: &Path,
        environment: &BTreeMap<OsString, OsString>,
        capture_limit: usize,
        stdout_observer: Option<OutputObserver>,
        stderr_observer: Option<OutputObserver>,
        limits: DuplexLimits,
        resources: Option<Arc<dyn Send + Sync>>,
    ) -> io::Result<Duplex> {
        self.spawn_duplex_process_generation(
            thread,
            executable,
            args,
            cwd,
            environment,
            capture_limit,
            stdout_observer,
            stderr_observer,
            limits,
            resources,
            None,
        )
    }
    #[allow(clippy::too_many_arguments)] // Generation is supplied only by the canonical owner.
    pub(crate) fn spawn_duplex_process_generation(
        &self,
        thread: ThreadId,
        executable: &Path,
        args: &[OsString],
        cwd: &Path,
        environment: &BTreeMap<OsString, OsString>,
        capture_limit: usize,
        stdout_observer: Option<OutputObserver>,
        stderr_observer: Option<OutputObserver>,
        limits: DuplexLimits,
        resources: Option<Arc<dyn Send + Sync>>,
        generation: Option<u64>,
    ) -> io::Result<Duplex> {
        if !executable.is_absolute()
            || !cwd.is_absolute()
            || capture_limit > 1024 * 1024
            || limits.process.timeout.is_zero()
            || limits.process.timeout > Duration::from_secs(120)
            || !(1..=8 * 1024 * 1024).contains(&limits.process.output_bytes)
            || !(1..=128).contains(&limits.process.process_count)
            || !(1..=1024 * 1024).contains(&limits.frame_bytes)
            || !(1..=64).contains(&limits.queued_frames)
            || !(1..=8 * 1024 * 1024).contains(&limits.input_bytes)
            || limits
                .stderr_bytes
                .is_some_and(|bytes| bytes > 8 * 1024 * 1024)
        {
            return Err(io::Error::other("duplex process bounds or absolute paths"));
        }
        let mut permit =
            HostWorkAdmission::admit(self, thread, HostWorkKind::Tool, "native-duplex-process")
                .map_err(io::Error::other)?;
        let state = self
            .0
            .state
            .lock()
            .map_err(|_| io::Error::other("poisoned lifecycle"))?;
        let current = state
            .entries
            .get(&thread)
            .map(|entry| entry.admission_generation)
            .ok_or_else(|| io::Error::other("unknown duplex scope"))?;
        if !state.attached
            || state.held(thread)
            || generation.is_some_and(|expected| !state.admission_current(thread, expected))
        {
            drop(state);
            permit.complete().map_err(io::Error::other)?;
            return Err(io::Error::other("duplex launch sealed"));
        }
        let job = Arc::new(JobObject::create_with_process_limit(
            limits.process.process_count,
        )?);
        let mut command = Command::new(executable);
        command
            .args(args)
            .current_dir(cwd)
            .env_clear()
            .envs(environment)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child =
            super::launch::without_critical_error_dialog(|| job.spawn_contained(&mut command))?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("duplex stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("duplex stdout unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("duplex stderr unavailable"))?;
        let pid = child.id();
        let done = Arc::new(AtomicBool::new(false));
        self.0
            .jobs
            .lock()
            .map_err(|_| io::Error::other("poisoned jobs"))?
            .entry(thread)
            .or_default()
            .push(job.clone());
        self.0
            .process_observers
            .lock()
            .map_err(|_| io::Error::other("poisoned observers"))?
            .entry(thread)
            .or_default()
            .push(done.clone());
        drop(state);
        let control = Arc::new(Mutex::new(Control::default()));
        let timer = tokio::spawn({
            let job = job.clone();
            let control = control.clone();
            async move {
                tokio::time::sleep(limits.process.timeout).await;
                let _ = stopped(&job, &control, "process deadline elapsed");
            }
        });
        let (sender, receiver) = mpsc::channel(limits.queued_frames);
        let stdout = tokio::spawn(frames(
            stdout,
            sender,
            limits,
            capture_limit,
            stdout_observer,
            job.clone(),
            control.clone(),
        ));
        let stderr = tokio::spawn(capture(
            stderr,
            capture_limit,
            stderr_observer,
            job.clone(),
            control.clone(),
            Some(limits.process.output_bytes),
            limits.stderr_bytes,
        ));
        let process = Process {
            _resources: resources,
            job: job.clone(),
            child: Some(child),
            pid,
            stdout: Some(stdout),
            stderr: Some(stderr),
            permit: Some(permit),
            timer: Some(timer),
            control: control.clone(),
            completion: None,
        };
        let (sender, completion) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            struct Completed(Arc<AtomicBool>);
            impl Drop for Completed {
                fn drop(&mut self) {
                    self.0.store(true, Ordering::Release);
                }
            }
            let _done = Completed(done);
            let _ = sender.send(process.wait().await);
        });
        Ok(Duplex {
            process: Some(Process {
                _resources: None,
                job,
                child: None,
                pid,
                stdout: None,
                stderr: None,
                permit: None,
                timer: None,
                control,
                completion: Some(completion),
            }),
            input: Some(input),
            frames: receiver,
            lifecycle: self.clone(),
            thread,
            generation: current,
            limits,
            written: 0,
        })
    }
}
