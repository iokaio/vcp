// SPDX-License-Identifier: Apache-2.0
//! Bounded owned ConPTY execution. Output is one merged terminal byte stream.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::AsyncWriteExt;

fn application_path(path: &Path) -> io::Result<std::path::PathBuf> {
    // Some legacy console applications cannot initialize from a verbatim
    // executable spelling. Convert only when a native canonical round trip
    // proves the ordinary spelling still names the held source path.
    let Some(raw) = path.to_str().and_then(|s| s.strip_prefix(r"\\?\")) else {
        return Ok(path.into());
    };
    let candidate = if let Some(unc) = raw.strip_prefix("UNC\\") {
        std::path::PathBuf::from(format!("\\\\{unc}"))
    } else if raw.as_bytes().get(1) == Some(&b':') {
        std::path::PathBuf::from(raw)
    } else {
        return Ok(path.into());
    };
    if candidate.canonicalize()? != path.canonicalize()? {
        return Err(io::Error::other(
            "terminal executable spelling changes native identity",
        ));
    }
    Ok(candidate)
}

impl Lifecycle {
    pub fn spawn_pty_with_capture(
        &self,
        thread: ThreadId,
        executable: &Path,
        args: &[OsString],
        cwd: &Path,
        environment: &BTreeMap<OsString, OsString>,
        size: codex_utils_pty::TerminalSize,
        input: Option<String>,
        limits: Limits,
        observer: OutputObserver,
        resources: Arc<dyn Send + Sync>,
    ) -> io::Result<Process> {
        self.spawn_pty_with_capture_generation(
            thread,
            executable,
            args,
            cwd,
            environment,
            size,
            input,
            limits,
            observer,
            resources,
            None,
        )
    }
    pub(crate) fn spawn_pty_with_capture_generation(
        &self,
        thread: ThreadId,
        executable: &Path,
        args: &[OsString],
        cwd: &Path,
        environment: &BTreeMap<OsString, OsString>,
        size: codex_utils_pty::TerminalSize,
        input: Option<String>,
        limits: Limits,
        observer: OutputObserver,
        resources: Arc<dyn Send + Sync>,
        generation: Option<u64>,
    ) -> io::Result<Process> {
        if limits.timeout.is_zero()
            || limits.timeout > Duration::from_millis(vcp_tools::process::MAX_TIMEOUT_MS)
            || limits.output_bytes == 0
            || limits.output_bytes > 8 * 1024 * 1024
            || !(1..=128).contains(&limits.process_count)
            || input.as_ref().is_some_and(|s| s.len() > 32 * 1024)
        {
            return Err(io::Error::other("PTY bounds"));
        }
        let mut permit = HostWorkAdmission::admit(self, thread, HostWorkKind::Tool, "owned-pty")
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
            return Err(io::Error::other("PTY launch sealed"));
        }
        let job = Arc::new(JobObject::create_with_process_limit(limits.process_count)?);
        let executable = application_path(executable)?;
        let pty = codex_utils_pty::spawn_owned_pty(
            &executable,
            args,
            cwd,
            environment,
            size,
            job.clone(),
        )
        .map_err(|e| io::Error::other(e.to_string()))?;
        let pid = pty.child.id();
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
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let result = Process {
            _resources: None,
            job: job.clone(),
            child: None,
            pid,
            stdout: None,
            stderr: None,
            permit: None,
            timer: None,
            control: control.clone(),
            completion: Some(receiver),
        };
        tokio::spawn(async move {
            struct Completed(Arc<AtomicBool>);
            impl Drop for Completed {
                fn drop(&mut self) {
                    self.0.store(true, Ordering::Release);
                }
            }
            let _done = Completed(done);
            let _resources = resources;
            let timer = tokio::spawn({
                let job = job.clone();
                let control = control.clone();
                async move {
                    tokio::time::sleep(limits.timeout).await;
                    if let Ok(mut c) = control.lock() {
                        c.reason
                            .get_or_insert_with(|| "process deadline elapsed".into());
                    }
                    let _ = job.terminate();
                }
            });
            let captured = tokio::spawn(capture(
                tokio::fs::File::from_std(pty.output),
                64 * 1024,
                Some(observer),
                job.clone(),
                control.clone(),
                Some(limits.output_bytes),
                None,
            ));
            let written = tokio::spawn(async move {
                let mut writer = tokio::fs::File::from_std(pty.input);
                if let Some(input) = input {
                    writer.write_all(input.as_bytes()).await?;
                    writer.flush().await?;
                }
                // Closing ConPTY input can terminate attached clients. Retain
                // the writer in the completed task until native exit is seen.
                Ok::<tokio::fs::File, io::Error>(writer)
            });
            let observed = async {
                let exit = tokio::task::spawn_blocking(move || {
                    let mut child = pty.child;
                    child.wait()
                })
                .await
                .map_err(io::Error::other)??;
                job.terminate()?;
                tokio::time::timeout(Duration::from_secs(2), async {
                    while job.active_process_count()? != 0 {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    Ok::<(), io::Error>(())
                })
                .await
                .map_err(|_| io::Error::other("PTY tree did not quiesce"))??;
                let (output, input) = tokio::join!(captured, written);
                let stdout = output.map_err(io::Error::other)??;
                input.map_err(io::Error::other)??;
                let reason = control
                    .lock()
                    .map_err(|_| io::Error::other("poisoned PTY limits"))?
                    .reason
                    .clone();
                permit.complete().map_err(io::Error::other)?;
                Ok(Outcome {
                    exit_code: Some(exit),
                    stdout,
                    stderr: Capture {
                        bytes: vec![],
                        total: 0,
                    },
                    stop_reason: reason,
                })
            }
            .await;
            timer.abort();
            let _ = job.terminate();
            // ReleasePseudoConsole permits EOF after all clients exit; destroy
            // the remaining console handle away from controller/store locks.
            let _ = tokio::task::spawn_blocking(move || drop(pty.console)).await;
            let _ = sender.send(observed);
        });
        Ok(result)
    }
}
