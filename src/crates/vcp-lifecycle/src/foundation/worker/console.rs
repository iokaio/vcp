// SPDX-License-Identifier: Apache-2.0
//! Windows console close has a short OS grace period. The callback only signals
//! an owned worker and waits boundedly; all storage runs on the canonical queue.
use super::*;
use std::sync::atomic::AtomicU32;

static INSTALLED: AtomicBool = AtomicBool::new(false);
static EVENT: AtomicU32 = AtomicU32::new(u32::MAX);
static FINISHED: AtomicBool = AtomicBool::new(false);
#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
    fn Sleep(milliseconds: u32);
}
unsafe extern "system" fn close_handler(event: u32) -> i32 {
    // CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT and CTRL_SHUTDOWN_EVENT. Ctrl+C stays
    // under the foreground CLI's existing interrupt semantics.
    if !matches!(event, 2 | 5 | 6) {
        return 0;
    }
    EVENT.store(event, Ordering::SeqCst);
    for _ in 0..400 {
        if FINISHED.load(Ordering::SeqCst) {
            break;
        }
        unsafe {
            Sleep(10);
        }
    }
    1
}
struct Guard {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        unsafe {
            SetConsoleCtrlHandler(Some(close_handler), 0);
        }
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        INSTALLED.store(false, Ordering::SeqCst);
    }
}
impl super::super::CanonicalHost {
    /// Install once in the owning console process and retain the returned guard.
    /// Forced termination bypasses this path and relies on durable recovery.
    pub fn install_console_close_handler(&self) -> std::result::Result<impl Drop, String> {
        if INSTALLED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("a console owner is already registered".into());
        }
        EVENT.store(u32::MAX, Ordering::SeqCst);
        FINISHED.store(false, Ordering::SeqCst);
        if unsafe { SetConsoleCtrlHandler(Some(close_handler), 1) } == 0 {
            INSTALLED.store(false, Ordering::SeqCst);
            return Err(std::io::Error::last_os_error().to_string());
        }
        let host = self.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let spawned = std::thread::Builder::new()
            .name("vcp-console-close".into())
            .spawn(move || {
                while !stopping.load(Ordering::SeqCst) {
                    if EVENT.load(Ordering::SeqCst) != u32::MAX {
                        // Fence native/model/queued work before waiting for storage.
                        let waiter = host.runtime.lose_owner();
                        host.worker.fence();
                        let _ = host.worker.run_cleanup(|context| {
                            context.pause_all("native Windows console close")
                        });
                        if let Some(waiter) = waiter {
                            let _ = host.runtime.0.runtime.block_on(async {
                                tokio::time::timeout(Duration::from_secs(2), waiter.wait()).await
                            });
                        }
                        let _ = host.worker.run_cleanup(|context| {
                            let ids: Vec<_> = context.outputs.keys().cloned().collect();
                            for id in ids {
                                context.finish_output(&id, true)?;
                            }
                            let attempts: Vec<Attempt> = context
                                .engine
                                .store()
                                .state()
                                .records
                                .values()
                                .filter(|row| row.collection == Collection::Attempt)
                                .map(Record::decode)
                                .collect::<std::result::Result<_, _>>()?;
                            for attempt in attempts {
                                if attempt.phase == ReservationState::Submitted {
                                    let binding = ThreadBinding {
                                        scope: attempt.scope.clone(),
                                        agent: AgentId::new(),
                                        role: RequestRole::Main,
                                    };
                                    context.unknown(
                                        &binding,
                                        &attempt.id,
                                        "console closed before final provider observation",
                                    )?;
                                }
                            }
                            context.reconcile_effects()?;
                            Ok(())
                        });
                        FINISHED.store(true, Ordering::SeqCst);
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            });
        match spawned {
            Ok(thread) => Ok(Guard {
                stop,
                thread: Some(thread),
            }),
            Err(error) => {
                unsafe {
                    SetConsoleCtrlHandler(Some(close_handler), 0);
                }
                INSTALLED.store(false, Ordering::SeqCst);
                Err(error.to_string())
            }
        }
    }
}
