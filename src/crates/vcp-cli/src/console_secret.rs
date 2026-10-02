// SPDX-License-Identifier: Apache-2.0
//! Hidden console entry for a provider key. Echo is disabled only for this
//! read; the original mode is restored by a guard, and by a temporary control
//! handler if the console is closed or Ctrl+Break ends the process.
use crate::credential::Secret;
use zeroize::Zeroizing;

const CTRL_C: u16 = 0x03;
const MAX_UNITS: usize = 16_384 + 2;

/// Decode one entered line. Ctrl+C or an empty line cancels with `None`.
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
    let text = Zeroizing::new(
        String::from_utf16(&units[..end]).map_err(|_| "the entered key is not valid text")?,
    );
    Secret::new(text.to_string()).map(Some)
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
    use windows_sys::Win32::{
        Foundation::HANDLE,
        System::Console::{
            GetConsoleMode, GetStdHandle, ReadConsoleW, SetConsoleCtrlHandler, SetConsoleMode,
            CONSOLE_READCONSOLE_CONTROL, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT,
            ENABLE_PROCESSED_INPUT, STD_INPUT_HANDLE,
        },
    };

    static INPUT: AtomicUsize = AtomicUsize::new(0);
    static ORIGINAL: AtomicU32 = AtomicU32::new(0);
    static HIDDEN: AtomicBool = AtomicBool::new(false);

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
        // SAFETY: querying the standard input handle has no preconditions.
        let handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
        let mut original = 0;
        // SAFETY: `original` is a valid out pointer; failure means stdin is
        // not a console, which is reported without reading anything.
        if handle.is_null() || unsafe { GetConsoleMode(handle, &mut original) } == 0 {
            return Err("hidden key entry needs an interactive console; set OPENROUTER_API_KEY in this terminal instead".into());
        }
        INPUT.store(handle as usize, Ordering::SeqCst);
        ORIGINAL.store(original, Ordering::SeqCst);
        // SAFETY: registers a handler with static lifetime; the guard removes it.
        unsafe { SetConsoleCtrlHandler(Some(on_control), 1) };
        let hidden = (original | ENABLE_LINE_INPUT) & !(ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT);
        HIDDEN.store(true, Ordering::SeqCst);
        let _guard = Hidden;
        // SAFETY: `handle` is the console input handle validated above.
        if unsafe { SetConsoleMode(handle, hidden) } == 0 {
            return Err("console echo could not be disabled; nothing was read".into());
        }
        let mut stderr = std::io::stderr();
        let _ = write!(stderr, "{prompt}");
        let _ = stderr.flush();
        let mut units = Zeroizing::new(Vec::with_capacity(256));
        let mut chunk = Zeroizing::new(vec![0u16; 256]);
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
                    chunk.as_mut_ptr().cast(),
                    chunk.len() as u32,
                    &mut read,
                    &control,
                )
            };
            if ok == 0 || read == 0 {
                break;
            }
            let part = &chunk[..read as usize];
            units.extend_from_slice(part);
            if part
                .iter()
                .any(|u| *u == CTRL_C || *u == u16::from(b'\r') || *u == u16::from(b'\n'))
                || units.len() > MAX_UNITS
            {
                break;
            }
        }
        let _ = writeln!(stderr);
        if units.len() > MAX_UNITS {
            return Err("the entered key is too long".into());
        }
        decode(&units)
    }
}

/// Prompt on stderr and read one hidden line from the console.
#[cfg(windows)]
pub fn read(prompt: &str) -> Result<Option<Secret>, String> {
    native::read(prompt)
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
        assert!(decode(&[0xd800, 0x0d]).is_err());
        assert!(decode(&units("tab\tkey\r\n")).is_err());
    }
}
