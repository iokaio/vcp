// SPDX-License-Identifier: Apache-2.0
//! A launch failure must return to the broker instead of opening a modal dialog.
use std::{io, marker::PhantomData, rc::Rc};
use windows_sys::Win32::System::Diagnostics::Debug::{
    GetThreadErrorMode, SetThreadErrorMode, SEM_FAILCRITICALERRORS,
};

// This guard never leaves the synchronous closure boundary. The marker also
// prevents a future refactor from moving its thread-local restoration elsewhere.
struct RestoreMode {
    previous: u32,
    _thread: PhantomData<Rc<()>>,
}

impl RestoreMode {
    fn set(mode: u32) -> io::Result<Self> {
        let mut previous = 0;
        // SAFETY: the output points to a live DWORD. Only this thread is changed.
        if unsafe { SetThreadErrorMode(mode, &mut previous) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            previous,
            _thread: PhantomData,
        })
    }
}

impl Drop for RestoreMode {
    fn drop(&mut self) {
        // SAFETY: this is the same thread and the mode came from a successful
        // SetThreadErrorMode call. Restoration also runs during error/unwind.
        // Drop cannot report an OS error; a previously accepted mode is valid.
        unsafe { SetThreadErrorMode(self.previous, std::ptr::null_mut()) };
    }
}

/// Call only around synchronous native creation, never around an async wait.
/// Preserve the caller's other flags and propagate setup failure before launch.
pub(super) fn without_critical_error_dialog<T>(
    launch: impl FnOnce() -> io::Result<T>,
) -> io::Result<T> {
    // SAFETY: reads only the calling thread's error mode; there are no pointers.
    let mode = unsafe { GetThreadErrorMode() };
    let _restore = RestoreMode::set(mode | SEM_FAILCRITICALERRORS)?;
    launch()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::windows::process::CommandExt,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::ERROR_EXE_MACHINE_TYPE_MISMATCH,
        System::Diagnostics::Debug::{GetErrorMode, SEM_NOOPENFILEERRORBOX},
    };

    fn mode() -> u32 {
        // SAFETY: reads only this thread's error mode.
        unsafe { GetThreadErrorMode() }
    }

    #[test]
    fn restores_thread_mode_on_success_error_and_unwind() {
        // SAFETY: read-only process setting; this test never changes it.
        let process_mode = unsafe { GetErrorMode() };
        std::thread::scope(|scope| {
            // Keep the sender inside the scope body so panic drops it before
            // scope joins the observer; no thread can wait forever on unwind.
            let (request, requests) = std::sync::mpsc::channel();
            let (response, responses) = std::sync::mpsc::channel();
            scope.spawn(move || {
                while requests.recv().is_ok() {
                    if response.send(mode()).is_err() {
                        break;
                    }
                }
            });
            let observe = || {
                request.send(()).unwrap();
                responses.recv_timeout(Duration::from_secs(2)).unwrap()
            };
            // Compare this already-running thread with its own baseline, not
            // a process setting that the Rust runtime may override per thread.
            let sibling_mode = observe();
            for original in [
                0,
                SEM_NOOPENFILEERRORBOX,
                SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX,
            ] {
                let _reset = RestoreMode::set(original).unwrap();
                let expected = original | SEM_FAILCRITICALERRORS;
                assert_eq!(
                    without_critical_error_dialog(|| {
                        assert_eq!(mode(), expected);
                        assert_eq!(observe(), sibling_mode);
                        Ok(42)
                    })
                    .unwrap(),
                    42
                );
                assert_eq!(mode(), original);
                assert_eq!(observe(), sibling_mode);
                let error = without_critical_error_dialog(|| {
                    assert_eq!(mode(), expected);
                    assert_eq!(observe(), sibling_mode);
                    Err::<(), _>(io::Error::from_raw_os_error(193))
                })
                .unwrap_err();
                assert_eq!(error.raw_os_error(), Some(193));
                assert_eq!(mode(), original);
                assert_eq!(observe(), sibling_mode);
                let unwind = std::panic::catch_unwind(|| {
                    let _ = without_critical_error_dialog(|| -> io::Result<()> {
                        assert_eq!(mode(), expected);
                        panic!("synthetic launch unwind")
                    });
                });
                assert!(unwind.is_err());
                assert_eq!(mode(), original);
                assert_eq!(observe(), sibling_mode);
                // SAFETY: read-only process setting.
                assert_eq!(unsafe { GetErrorMode() }, process_mode);
            }
            drop(request);
        });
    }

    #[test]
    fn nested_launch_scopes_restore_the_enclosing_mode() {
        let _reset = RestoreMode::set(SEM_NOOPENFILEERRORBOX).unwrap();
        without_critical_error_dialog(|| {
            let outer = mode();
            without_critical_error_dialog(|| Ok(()))?;
            assert_eq!(mode(), outer);
            Ok(())
        })
        .unwrap();
        assert_eq!(mode(), SEM_NOOPENFILEERRORBOX);
    }

    struct OwnedChild(std::process::Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn malformed_native_launch_returns_without_dialog() {
        const WORKER: &str = "VCP_TEST_MALFORMED_LAUNCH_WORKER";
        if let Some(completion) = std::env::var_os(WORKER) {
            let _reset = RestoreMode::set(0).unwrap();
            let temp = tempfile::tempdir().unwrap();
            let executable = temp.path().join("node.exe");
            // The same invalid image used by the canonical verification test.
            std::fs::write(&executable, b"synthetic invalid executable").unwrap();
            let job = codex_utils_pty::JobObject::create_without_breakaway().unwrap();
            let mut command = tokio::process::Command::new(&executable);
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let started = Instant::now();
            let result = without_critical_error_dialog(|| job.spawn_contained(&mut command));
            assert!(result.is_err(), "invalid native image cannot execute");
            // These fixture bytes trigger the unsupported-machine image path
            // on native Windows, formerly accompanied by a 16-bit modal dialog.
            assert_eq!(
                result.err().unwrap().raw_os_error(),
                Some(ERROR_EXE_MACHINE_TYPE_MISMATCH as i32)
            );
            assert!(started.elapsed() < Duration::from_secs(5));
            assert_eq!(mode(), 0);
            assert_eq!(job.active_process_count().unwrap(), 0);
            std::fs::write(completion, b"native image rejected; mode restored").unwrap();
            return;
        }
        // A regression must fail under a supervisor deadline, not hang the test
        // runner behind a Windows modal dialog. Retain stderr for diagnostics.
        let supervisor = tempfile::tempdir().unwrap();
        let completion = supervisor.path().join("completed");
        let mut child = OwnedChild(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "process::launch::tests::malformed_native_launch_returns_without_dialog",
                    "--nocapture",
                ])
                .env(WORKER, &completion)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(status.success(), "supervised malformed-image launch failed");
                assert_eq!(
                    std::fs::read(&completion).unwrap(),
                    b"native image rejected; mode restored",
                    "the exact worker test must execute before a pass"
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "native image rejection exceeded supervisor deadline"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}
