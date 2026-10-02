// SPDX-License-Identifier: Apache-2.0
//! OpenRouter credential sources (ADR-079). `OPENROUTER_API_KEY` in the process
//! environment always wins. An entry the user explicitly stored in Windows
//! Credential Manager is consulted only for interactive terminal sessions, so
//! automation that withholds the variable still cannot reach a paid call. The
//! value never enters arguments, profiles, the data folder, JSONL or
//! diagnostics, and copies are wiped when dropped.
use serde::Serialize;

pub const ENVIRONMENT: &str = "OPENROUTER_API_KEY";
/// Generic credential target, per user and kept on this machine (no roaming).
pub const TARGET: &str = "VCP/OpenRouter/v1";
/// Credential Manager's generic blob ceiling (CRED_MAX_CREDENTIAL_BLOB_SIZE).
pub const STORE_LIMIT: usize = 2560;
const LIMIT: usize = 16_384;
pub const MISSING: &str = "OPENROUTER_API_KEY is required: set it in this terminal with a masked prompt, or store it once with `vcp setup credential store` for interactive terminal sessions (JSONL, redirected and automation runs use only the variable); never pass it as a command argument";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Environment,
    /// Entered during guided setup and held only in this process.
    Session,
    CredentialManager,
}

/// Overwrite memory in place so a dropped copy does not linger.
pub(crate) fn wipe<T: Copy + Default>(values: &mut [T]) {
    for value in values.iter_mut() {
        // SAFETY: `value` is a valid, exclusively borrowed location.
        unsafe { std::ptr::write_volatile(value, T::default()) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

/// A provider key that is wiped when dropped and never formatted.
pub struct Secret(String);

impl Drop for Secret {
    fn drop(&mut self) {
        // SAFETY: zero bytes keep the String valid UTF-8 until it is freed.
        wipe(unsafe { self.0.as_bytes_mut() });
    }
}

impl Secret {
    pub fn new(value: String) -> Result<Self, String> {
        // Wrap first so a rejected value is wiped as well.
        let secret = Self(value);
        let text = &secret.0;
        if text.is_empty() || text.len() > LIMIT || text.chars().any(char::is_control) {
            return Err(
                "the OpenRouter key must be 1 to 16384 characters without control characters"
                    .into(),
            );
        }
        Ok(secret)
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret([redacted])")
    }
}

/// Precedence shared by every caller: a nonempty environment value wins, then
/// the stored entry. `stored` is only consulted when the environment is unset.
fn resolve(
    environment: Option<std::ffi::OsString>,
    other: impl FnOnce() -> Result<Option<(Secret, Source)>, String>,
) -> Result<Option<(Secret, Source)>, String> {
    if let Some(value) = environment.filter(|value| !value.is_empty()) {
        let value = value
            .into_string()
            .map_err(|_| "OPENROUTER_API_KEY must be valid Unicode".to_owned())?;
        return Secret::new(value).map(|secret| Some((secret, Source::Environment)));
    }
    other()
}

/// A key entered during guided setup, held only in this process memory. Child
/// processes never inherit it, unlike the environment variable.
static SESSION: std::sync::Mutex<Option<Secret>> = std::sync::Mutex::new(None);

/// Keep `secret` for the rest of this attended process.
pub fn set_session(secret: Secret) {
    if let Ok(mut session) = SESSION.lock() {
        *session = Some(secret);
    }
}

fn session() -> Option<Secret> {
    SESSION
        .lock()
        .ok()?
        .as_ref()
        .and_then(|secret| Secret::new(secret.expose().to_owned()).ok())
}

/// The key to use and where it came from, if any. `interactive` permits the
/// session and stored keys; it must be true only for an interactive terminal.
pub fn openrouter(interactive: bool) -> Result<Option<(Secret, Source)>, String> {
    resolve(std::env::var_os(ENVIRONMENT), || {
        if !interactive {
            return Ok(None);
        }
        if let Some(secret) = session() {
            return Ok(Some((secret, Source::Session)));
        }
        #[cfg(windows)]
        return Ok(read(TARGET)?.map(|secret| (secret, Source::CredentialManager)));
        #[cfg(not(windows))]
        Ok(None)
    })
}

/// The key, or an error naming both supported sources.
pub fn require(interactive: bool) -> Result<Secret, String> {
    openrouter(interactive)?
        .map(|(secret, _)| secret)
        .ok_or_else(|| MISSING.into())
}

/// Where a key would come from; the value is dropped immediately.
pub fn source(interactive: bool) -> Option<Source> {
    openrouter(interactive)
        .ok()
        .flatten()
        .map(|(_, source)| source)
}

#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// The stored key for `target`, or None when no entry exists.
#[cfg(windows)]
pub(crate) fn read(target: &str) -> Result<Option<Secret>, String> {
    use windows_sys::Win32::{
        Foundation::{GetLastError, ERROR_NOT_FOUND},
        Security::Credentials::{CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC},
    };
    let name = wide(target);
    let mut entry: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: `name` is NUL-terminated and outlives the call; `entry` receives
    // a buffer owned by the credential subsystem until CredFree below.
    if unsafe { CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut entry) } == 0 {
        // SAFETY: reads this thread's last error immediately after the call.
        return match unsafe { GetLastError() } {
            ERROR_NOT_FOUND => Ok(None),
            _ => Err("Windows Credential Manager could not be read".into()),
        };
    }
    // SAFETY: CredReadW succeeded, so `entry` points to a valid CREDENTIALW
    // whose blob holds CredentialBlobSize bytes. The blob is copied, then
    // overwritten before the buffer is released exactly once.
    let copied = unsafe {
        let credential = &*entry;
        let size = credential.CredentialBlobSize as usize;
        let bytes = if credential.CredentialBlob.is_null() || size == 0 {
            Vec::new()
        } else {
            let bytes = std::slice::from_raw_parts(credential.CredentialBlob, size).to_vec();
            wipe(std::slice::from_raw_parts_mut(
                credential.CredentialBlob,
                size,
            ));
            bytes
        };
        CredFree(entry.cast());
        bytes
    };
    let text = String::from_utf8(copied).map_err(|error| {
        wipe(&mut error.into_bytes());
        "the stored OpenRouter key is not valid UTF-8; remove it and store it again"
    })?;
    Secret::new(text).map(Some)
}

/// Create or replace the entry for `target` with `secret`.
#[cfg(windows)]
pub(crate) fn store(target: &str, secret: &Secret) -> Result<(), String> {
    use windows_sys::Win32::Security::Credentials::{
        CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };
    let blob = secret.expose().as_bytes();
    if blob.len() > STORE_LIMIT {
        return Err(
            "this key is too long for Windows Credential Manager; set OPENROUTER_API_KEY instead"
                .into(),
        );
    }
    let mut name = wide(target);
    let mut user = wide("OpenRouter");
    let mut comment = wide("OpenRouter key for VCP; remove with `vcp setup credential remove`.");
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: name.as_mut_ptr(),
        Comment: comment.as_mut_ptr(),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_ptr().cast_mut(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: user.as_mut_ptr(),
        ..Default::default()
    };
    // SAFETY: every pointer refers to a live, NUL-terminated buffer or the
    // blob slice; CredWriteW copies them and does not write through the blob.
    if unsafe { CredWriteW(&credential, 0) } == 0 {
        return Err("Windows Credential Manager refused the key".into());
    }
    Ok(())
}

/// Delete the entry for `target`; false when none existed.
#[cfg(windows)]
pub(crate) fn remove(target: &str) -> Result<bool, String> {
    use windows_sys::Win32::{
        Foundation::{GetLastError, ERROR_NOT_FOUND},
        Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC},
    };
    let name = wide(target);
    // SAFETY: `name` is NUL-terminated and outlives the call.
    if unsafe { CredDeleteW(name.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 {
        // SAFETY: reads this thread's last error immediately after the call.
        return match unsafe { GetLastError() } {
            ERROR_NOT_FOUND => Ok(false),
            _ => Err("Windows Credential Manager could not remove the key".into()),
        };
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_wins_and_secrets_are_never_formatted() {
        let stored = || {
            Ok(Some((
                Secret::new("stored-key".into()).unwrap(),
                Source::CredentialManager,
            )))
        };
        let (secret, source) = resolve(Some("environment-key".into()), stored)
            .unwrap()
            .unwrap();
        assert_eq!(
            (secret.expose(), source),
            ("environment-key", Source::Environment)
        );
        assert_eq!(format!("{secret:?}"), "Secret([redacted])");
        let (secret, source) = resolve(Some("".into()), stored).unwrap().unwrap();
        assert_eq!(
            (secret.expose(), source),
            ("stored-key", Source::CredentialManager)
        );
        assert!(resolve(None, || Ok(None)).unwrap().is_none());
        assert!(resolve(Some("bad\nkey".into()), stored).is_err());
        assert!(resolve(None, || Err("store failure".into())).is_err());
        assert!(resolve(Some("environment-key".into()), || panic!("store consulted")).is_ok());
        for invalid in ["", "line\nbreak", &"k".repeat(16_385)] {
            assert!(Secret::new(invalid.into()).is_err());
        }
    }

    #[cfg(windows)]
    #[test]
    fn credential_manager_round_trip_uses_an_isolated_target() {
        struct Cleanup(String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = remove(&self.0);
            }
        }
        let target = format!(
            "VCP/OpenRouter/test-{}",
            vcp_domain::ids::CommandId::new().as_str()
        );
        let _cleanup = Cleanup(target.clone());
        assert!(read(&target).unwrap().is_none());
        assert!(!remove(&target).unwrap());
        store(&target, &Secret::new("synthetic-store-key".into()).unwrap()).unwrap();
        assert_eq!(
            read(&target).unwrap().unwrap().expose(),
            "synthetic-store-key"
        );
        store(&target, &Secret::new("synthetic-replaced".into()).unwrap()).unwrap();
        assert_eq!(
            read(&target).unwrap().unwrap().expose(),
            "synthetic-replaced"
        );
        assert!(remove(&target).unwrap());
        assert!(read(&target).unwrap().is_none());
        let long = Secret::new("k".repeat(STORE_LIMIT + 1)).unwrap();
        assert!(store(&target, &long).is_err());
        assert!(read(&target).unwrap().is_none());
    }
}
