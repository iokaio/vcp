// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
#[path = "support/native_console.rs"]
mod native_console;
#[path = "support/recovery_config.rs"]
mod recovery_config;
use std::time::{Duration, Instant};
use vcp_domain::{
    effect::{Effect, EffectState},
    task::{Task, TaskState},
};
use vcp_lifecycle::foundation::CanonicalHost;
use vcp_store::{contract::Collection, BackendKind};
#[link(name = "user32")]
unsafe extern "system" {
    fn PostMessageW(window: isize, message: u32, wparam: usize, lparam: isize) -> i32;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_console_close_and_forced_termination_have_distinct_durable_recovery() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for graceful in [true, false] {
            let temp = tempfile::tempdir().unwrap();
            let child = native_console::Console::spawn(
                std::path::Path::new(env!("CARGO_BIN_EXE_vcp-console-fixture")),
                temp.path(),
                if backend == BackendKind::Files {
                    "files"
                } else {
                    "sqlite"
                },
            )
            .unwrap();
            let deadline = Instant::now() + Duration::from_secs(15);
            while !temp.path().join("ready").exists() {
                assert!(
                    !child.finished().unwrap(),
                    "console fixture exited before ready"
                );
                if Instant::now() > deadline {
                    let _ = child.kill();
                    panic!("console fixture startup timed out");
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            if graceful {
                let window: isize = std::fs::read_to_string(temp.path().join("ready"))
                    .unwrap()
                    .parse()
                    .unwrap();
                assert_ne!(unsafe { PostMessageW(window, 0x0010, 0, 0) }, 0); // Actual WM_CLOSE, not GenerateConsoleCtrlEvent.
            } else {
                child.kill().unwrap();
            }
            while !child.finished().unwrap() {
                if Instant::now() > deadline {
                    let _ = child.kill();
                    panic!("console close did not terminate owner");
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            use std::os::windows::fs::OpenOptionsExt;
            loop {
                if std::fs::OpenOptions::new()
                    .write(true)
                    .share_mode(0)
                    .open(temp.path().join("child-lock"))
                    .is_ok()
                {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "contained child survived console owner"
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            let config = recovery_config::config(
                &temp.path().join("canonical"),
                &temp.path().join("workspace"),
                backend,
            );
            let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
            let state = host.snapshot().unwrap();
            let task: Task = state
                .record(
                    Collection::Task,
                    config.root_task.as_str(),
                    &config.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(task.state, TaskState::Paused);
            let effects: Vec<Effect> = state
                .records
                .values()
                .filter(|r| r.collection == Collection::Effect)
                .map(|r| r.decode().unwrap())
                .collect();
            assert_eq!(effects.len(), 1);
            assert_eq!(effects[0].state, EffectState::OutcomeUnknown);
            // Task transition reason lives in event evidence, not a synthetic flag.
            let serialized = serde_json::to_string(&state.events).unwrap();
            assert_eq!(
                serialized.contains("native Windows console close"),
                graceful
            );
            assert_eq!(
                std::fs::read(temp.path().join("workspace/non-idempotent-marker")).unwrap(),
                b"effect\n"
            );
            owner.close().await.unwrap();
        }
    }
}
