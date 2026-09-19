// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
#[path = "../../vcp-lifecycle/tests/support/native_console.rs"]
mod native_console;
use std::time::{Duration, Instant};
#[link(name = "user32")]
unsafe extern "system" {
    fn PostMessageW(window: isize, message: u32, wparam: usize, lparam: isize) -> i32;
}

#[tokio::test]
async fn native_keyboard_unicode_resize_explicit_answer_and_close() {
    let temp = tempfile::tempdir().unwrap();
    let child = native_console::Console::spawn(
        std::path::Path::new(env!("CARGO_BIN_EXE_vcp-terminal-fixture")),
        temp.path(),
        "terminal",
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while !temp.path().join("ready").exists() {
        assert!(
            !child.finished().unwrap(),
            "native input fixture exited before evidence"
        );
        if Instant::now() >= deadline {
            child.kill().unwrap();
            panic!("native input fixture timed out");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let evidence: serde_json::Value =
        serde_json::from_slice(&std::fs::read(temp.path().join("evidence.json")).unwrap()).unwrap();
    assert_eq!(
        evidence["unicode"],
        "路径 e\u{301} 🦀 C:\\long path\\file.rs"
    );
    assert_eq!(evidence["columns"], 20);
    assert_eq!(evidence["resize_submitted"], false);
    assert_eq!(evidence["partial_answer_submitted"], false);
    assert_eq!(evidence["explicit_answer"], "/answer question-1 allow");
    let window: isize = std::fs::read_to_string(temp.path().join("ready"))
        .unwrap()
        .parse()
        .unwrap();
    assert_ne!(unsafe { PostMessageW(window, 0x0010, 0, 0) }, 0);
    while !child.finished().unwrap() {
        assert!(Instant::now() < deadline, "native console close timed out");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
