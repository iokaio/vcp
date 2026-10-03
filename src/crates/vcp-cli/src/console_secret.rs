// SPDX-License-Identifier: Apache-2.0
//! Hidden console entry for a provider key. Echo is disabled only for this
//! read; the original mode is restored by a guard, and by a temporary control
//! handler if the console is closed or Ctrl+Break ends the process.
#[cfg(windows)]
use crate::credential::wipe;
use crate::credential::Secret;

#[cfg(any(windows, test))]
const CTRL_C: u16 = 0x03;
#[cfg(windows)]
const MAX_UNITS: usize = 16_384 + 2;

/// UTF-16 input wiped when dropped. Callers size it up front so it never
/// reallocates and leaves an unwiped copy behind.
#[cfg(windows)]
struct Units(Vec<u16>);

#[cfg(windows)]
impl Drop for Units {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

/// Decode one entered line. Ctrl+C, `cancel` or an empty line cancels with None.
#[cfg(any(windows, test))]
pub(crate) fn decode(units: &[u16]) -> Result<Option<Secret>, String> {
    if units.contains(&CTRL_C) {
        return Ok(None);
    }
    let end = units
        .iter()
        .position(|unit| *unit == u16::from(b'\r') || *unit == u16::from(b'\n'))
        .unwrap_or(units.len());
    if end == 0 {
        return Ok(None);
    }
    let key = Secret::from_utf16(&units[..end])?;
    if key.expose().eq_ignore_ascii_case("cancel") {
        return Ok(None);
    }
    Ok(Some(key))
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
    use std::sync::Mutex;
    use windows_sys::Win32::{
        Foundation::HANDLE,
        System::Console::{
            FlushConsoleInputBuffer, GetConsoleMode, GetStdHandle, ReadConsoleW,
            SetConsoleCtrlHandler, SetConsoleMode, CONSOLE_READCONSOLE_CONTROL, ENABLE_ECHO_INPUT,
            ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT, STD_INPUT_HANDLE,
        },
    };

    static INPUT: AtomicUsize = AtomicUsize::new(0);
    static ORIGINAL: AtomicU32 = AtomicU32::new(0);
    static HIDDEN: AtomicBool = AtomicBool::new(false);
    static READING: Mutex<()> = Mutex::new(());

    fn restore_mode() {
        if HIDDEN.swap(false, Ordering::SeqCst) {
            // SAFETY: the stored handle is this process's console input
            // handle, which stays valid for the process lifetime.
            unsafe {
                SetConsoleMode(
                    INPUT.load(Ordering::SeqCst) as HANDLE,
                    ORIGINAL.load(Ordering::SeqCst),
                )
            };
        }
    }

    unsafe extern "system" fn on_control(_kind: u32) -> windows_sys::core::BOOL {
        restore_mode();
        // Let the default handler run after the echo mode is restored.
        0
    }

    struct Hidden;
    impl Drop for Hidden {
        fn drop(&mut self) {
            restore_mode();
            // SAFETY: removes the handler registered in `read` below.
            unsafe { SetConsoleCtrlHandler(Some(on_control), 0) };
        }
    }

    pub fn read(prompt: &str) -> Result<Option<Secret>, String> {
        use std::io::Write;
        let _reading = READING
            .lock()
            .map_err(|_| "console key entry unavailable")?;
        // SAFETY: querying the standard input handle has no preconditions.
        let handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
        let mut original = 0;
        // SAFETY: `original` is a valid out pointer; failure means stdin is
        // not a console, which is reported without reading anything.
        if handle.is_null() || unsafe { GetConsoleMode(handle, &mut original) } == 0 {
            return Err("hidden key entry needs an interactive console; use the configured OpenRouter environment variable in this terminal instead".into());
        }
        INPUT.store(handle as usize, Ordering::SeqCst);
        ORIGINAL.store(original, Ordering::SeqCst);
        // SAFETY: registers a handler with static lifetime; the guard removes it.
        if unsafe { SetConsoleCtrlHandler(Some(on_control), 1) } == 0 {
            return Err(
                "console interruption handling could not be installed; nothing was read".into(),
            );
        }
        let hidden = (original | ENABLE_LINE_INPUT) & !(ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT);
        HIDDEN.store(true, Ordering::SeqCst);
        let _guard = Hidden;
        // SAFETY: `handle` is the console input handle validated above.
        if unsafe { SetConsoleMode(handle, hidden) } == 0 {
            return Err("console echo could not be disabled; nothing was read".into());
        }
        let mut stderr = std::io::stderr();
        write!(stderr, "{prompt}").map_err(|_| "key prompt could not be written")?;
        stderr
            .flush()
            .map_err(|_| "key prompt could not be written")?;
        let mut chunk = Units(vec![0u16; 256]);
        let mut units = Units(Vec::with_capacity(MAX_UNITS + chunk.0.len()));
        let control = CONSOLE_READCONSOLE_CONTROL {
            nLength: std::mem::size_of::<CONSOLE_READCONSOLE_CONTROL>() as u32,
            nInitialChars: 0,
            dwCtrlWakeupMask: 1 << CTRL_C,
            dwControlKeyState: 0,
        };
        loop {
            let mut read = 0u32;
            // SAFETY: `chunk` has room for its length in UTF-16 units and
            // `read` and `control` are valid for the call.
            let ok = unsafe {
                ReadConsoleW(
                    handle,
                    chunk.0.as_mut_ptr().cast(),
                    chunk.0.len() as u32,
                    &mut read,
                    &control,
                )
            };
            if ok == 0 || read == 0 {
                // SAFETY: input belongs to this validated console. Discard a
                // partial pasted key before echo is restored by the guard.
                unsafe { FlushConsoleInputBuffer(handle) };
                let _ = writeln!(stderr);
                return Err("console key entry ended before a complete line was read".into());
            }
            let part = &chunk.0[..read as usize];
            units.0.extend_from_slice(part);
            if part
                .iter()
                .any(|u| *u == CTRL_C || *u == u16::from(b'\r') || *u == u16::from(b'\n'))
                || units.0.len() > MAX_UNITS
            {
                break;
            }
        }
        let _ = writeln!(stderr);
        if units.0.len() > MAX_UNITS {
            // SAFETY: discard any unread suffix of the oversized secret so
            // it cannot become a later, echoed console command or answer.
            unsafe { FlushConsoleInputBuffer(handle) };
            return Err("the entered key is too long".into());
        }
        let decoded = decode(&units.0);
        if !matches!(&decoded, Ok(Some(_))) {
            // SAFETY: a cancelled or invalid pasted entry must not leave its
            // suffix available to the next, potentially echoed input prompt.
            unsafe { FlushConsoleInputBuffer(handle) };
        }
        decoded
    }
}

/// Prompt on stderr and read one hidden line from the console.
#[cfg(windows)]
pub fn read(prompt: &str) -> Result<Option<Secret>, String> {
    native::read(prompt)
}

#[cfg(not(windows))]
pub fn read(_prompt: &str) -> Result<Option<Secret>, String> {
    Err("hidden key entry currently requires a Windows console".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    #[test]
    fn decoding_stops_at_the_line_end_and_cancels_on_ctrl_c_or_empty() {
        assert_eq!(
            decode(&units("sk-or-v1-key\r\n"))
                .unwrap()
                .unwrap()
                .expose(),
            "sk-or-v1-key"
        );
        assert_eq!(decode(&units("key\n")).unwrap().unwrap().expose(), "key");
        assert_eq!(
            decode(&units("ключ\r\n")).unwrap().unwrap().expose(),
            "ключ"
        );
        assert!(decode(&units("\r\n")).unwrap().is_none());
        assert!(decode(&[]).unwrap().is_none());
        assert!(decode(&units("partial\u{3}")).unwrap().is_none());
        assert!(decode(&units("cancel\r\n")).unwrap().is_none());
        assert!(decode(&units("CANCEL\r\n")).unwrap().is_none());
        assert!(decode(&[0xd800, 0x0d]).is_err());
        assert!(decode(&[b'k' as u16, 0xd800, 0x0d]).is_err());
        assert!(decode(&units("tab\tkey\r\n")).is_err());
        assert!(decode(&units(&"k".repeat(16_385))).is_err());
        assert!(decode(&units(&"é".repeat(8_193))).is_err());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "child fixture for hidden_console_input_never_echoes_and_restores_mode"]
    fn hidden_console_fixture() {
        use std::io::Write;
        use windows_sys::Win32::System::Console::{GetConsoleMode, GetStdHandle, STD_INPUT_HANDLE};
        let case = std::env::var("VCP_CREDENTIAL_TEST_CASE").expect("isolated fixture case");
        // SAFETY: query this test child's own console mode before and after entry.
        let handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
        let mut original = 0;
        assert_ne!(unsafe { GetConsoleMode(handle, &mut original) }, 0);
        let entered = read("Hidden test key: ").unwrap();
        if case == "key" {
            assert!(entered.is_some_and(|key| key.expose() == "synthetic-hidden-secret"));
        } else {
            assert!(entered.is_none());
        }
        let mut restored = 0;
        assert_ne!(unsafe { GetConsoleMode(handle, &mut restored) }, 0);
        assert_eq!(restored, original, "console mode must be restored");
        print!("Restored echo: ");
        std::io::stdout().flush().unwrap();
        let mut public = String::new();
        std::io::stdin().read_line(&mut public).unwrap();
        assert_eq!(public.trim(), "visible-after-secret");
    }

    #[cfg(windows)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn hidden_console_input_never_echoes_and_restores_mode() {
        use codex_utils_pty::{spawn_pty_process, TerminalSize};
        use std::{collections::HashMap, time::Duration};
        let executable = std::env::current_exe().unwrap();
        let directory = tempfile::tempdir().unwrap();
        for (case, entered) in [
            ("key", b"synthetic-hidden-secret\r".as_slice()),
            ("cancel", b"partial-hidden-secret\x03".as_slice()),
            ("cancel-word", b"cancel\r".as_slice()),
            ("empty", b"\r".as_slice()),
        ] {
            let mut environment: HashMap<String, String> = std::env::vars().collect();
            environment.remove(crate::credential::ENVIRONMENT);
            environment.insert("VCP_CREDENTIAL_TEST_CASE".into(), case.into());
            let mut child = spawn_pty_process(
                executable.to_str().unwrap(),
                &[
                    "--exact".into(),
                    "console_secret::tests::hidden_console_fixture".into(),
                    "--ignored".into(),
                    "--nocapture".into(),
                ],
                directory.path(),
                &environment,
                &None,
                TerminalSize {
                    rows: 24,
                    cols: 100,
                },
                &[],
            )
            .await
            .unwrap();
            let writer = child.session.writer_sender();
            let (display, mut observed) = tokio::sync::watch::channel(String::new());
            let output = tokio::spawn(async move {
                let mut captured = Vec::new();
                while let Some(chunk) = child.stdout_rx.recv().await {
                    assert!(captured.len() + chunk.len() <= 64 * 1024);
                    captured.extend(chunk);
                    display.send_replace(String::from_utf8_lossy(&captured).into_owned());
                }
                captured
            });
            let exercise = async {
                for (prompt, input) in [
                    ("Hidden test key: ", entered),
                    ("Restored echo: ", b"visible-after-secret\r".as_slice()),
                ] {
                    while !observed.borrow().contains(prompt) {
                        observed
                            .changed()
                            .await
                            .expect("console fixture exited before prompt");
                    }
                    writer.send(input.to_vec()).await.unwrap();
                }
                (&mut child.exit_rx).await.unwrap()
            };
            let outcome = tokio::time::timeout(Duration::from_secs(15), exercise).await;
            child.session.terminate();
            let captured = tokio::time::timeout(Duration::from_secs(5), output)
                .await
                .unwrap()
                .unwrap();
            let captured = String::from_utf8_lossy(&captured);
            assert!(
                outcome.is_ok(),
                "{case}: console entry timed out: {captured}"
            );
            assert_eq!(outcome.unwrap(), 0, "{case}: {captured}");
            assert!(
                !captured.contains("synthetic-hidden-secret"),
                "secret was echoed"
            );
            assert!(
                !captured.contains("partial-hidden-secret"),
                "partial secret was echoed"
            );
            assert!(
                captured.contains("visible-after-secret"),
                "echo was not restored"
            );
        }
    }
}
