// SPDX-License-Identifier: Apache-2.0
//! Disposable native console fixture; never admits model or tool work.
#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{io::IsTerminal, time::Duration};
    use vcp_cli::terminal::{input, parse, Input};
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
    struct Key {
        kind: u16,
        padding: u16,
        down: i32,
        repeat: u16,
        virtual_key: u16,
        scan: u16,
        character: u16,
        controls: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetStdHandle(kind: u32) -> isize;
        fn GetConsoleWindow() -> isize;
        fn WriteConsoleInputW(
            handle: isize,
            records: *const Key,
            count: u32,
            written: *mut u32,
        ) -> i32;
        fn SetConsoleWindowInfo(handle: isize, absolute: i32, rect: *const Rect) -> i32;
        fn SetConsoleScreenBufferSize(handle: isize, size: Coord) -> i32;
    }
    fn inject(text: &str) -> std::io::Result<()> {
        let keys: Vec<_> = text
            .encode_utf16()
            .map(|character| Key {
                kind: 1,
                padding: 0,
                down: 1,
                repeat: 1,
                virtual_key: if character == 13 { 13 } else { 0 },
                scan: 0,
                character,
                controls: 0,
            })
            .collect();
        let mut written = 0;
        // SAFETY: valid console handle and INPUT_RECORD-compatible key records.
        if unsafe {
            WriteConsoleInputW(
                GetStdHandle(-10i32 as u32),
                keys.as_ptr(),
                keys.len() as u32,
                &mut written,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        assert_eq!(written as usize, keys.len());
        Ok(())
    }
    let root = std::path::PathBuf::from(std::env::args_os().nth(1).ok_or("fixture root required")?);
    assert!(std::io::stdin().is_terminal());
    assert!(std::io::stdout().is_terminal());
    let mut lines = input(std::io::BufReader::new(std::io::stdin()))?;
    let output = unsafe { GetStdHandle(-11i32 as u32) };
    let rect = Rect {
        left: 0,
        top: 0,
        right: 19,
        bottom: 7,
    };
    assert_ne!(unsafe { SetConsoleWindowInfo(output, 1, &rect) }, 0);
    assert_ne!(
        unsafe { SetConsoleScreenBufferSize(output, Coord { x: 20, y: 100 }) },
        0
    );
    // Resizing an actual console must not submit any question/default.
    assert!(
        tokio::time::timeout(Duration::from_millis(150), lines.recv())
            .await
            .is_err()
    );
    let unicode = "路径 e\u{301} 🦀 C:\\long path\\file.rs";
    inject(&format!("{unicode}\r"))?;
    let received = tokio::time::timeout(Duration::from_secs(5), lines.recv())
        .await?
        .ok_or("input closed")??;
    assert_eq!(received.trim(), unicode);
    assert_eq!(parse(&received)?, Some(Input::Steer(unicode.into())));
    inject("/answer question-1 allow")?;
    assert!(
        tokio::time::timeout(Duration::from_millis(150), lines.recv())
            .await
            .is_err()
    );
    inject("\r")?;
    let answer = tokio::time::timeout(Duration::from_secs(5), lines.recv())
        .await?
        .ok_or("input closed")??;
    assert_eq!(
        parse(&answer)?,
        Some(Input::Answer {
            id: "question-1".into(),
            allow: true
        })
    );
    let window = unsafe { GetConsoleWindow() };
    assert_ne!(window, 0);
    std::fs::write(
        root.join("evidence.json"),
        serde_json::to_vec(&serde_json::json!({
            "unicode": received.trim(), "columns": 20, "resize_submitted": false,
            "partial_answer_submitted": false, "explicit_answer": answer.trim(),
        }))?,
    )?;
    std::fs::write(root.join("ready"), window.to_string())?;
    loop {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
#[cfg(not(windows))]
fn main() {}
