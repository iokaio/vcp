// SPDX-License-Identifier: Apache-2.0
//! OpenRouter credential sources. A designated process environment variable is
//! the only source when explicitly selected. An explicit stored selection uses
//! only the attended session or Windows Credential Manager in a terminal. The
//! legacy default prefers `OPENROUTER_API_KEY`, then an attended key. An
//! entry the user explicitly stored is consulted only for interactive sessions, so
//! automation never reads stored keys. Presence of `VCP_DENY_PROVIDER_CREDENTIALS`
//! denies every provider credential lookup before account selection, including
//! named environment variables. Offline preflights use that explicit guard. The
//! value never enters arguments, profiles, the data folder, JSONL or
//! diagnostics. This module's secret wrappers wipe their owned bytes on drop;
//! provider transports and operating-system allocations have separate lifetimes.
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const ENVIRONMENT: &str = "OPENROUTER_API_KEY";
/// Any value, including an empty value, prevents provider credential access.
pub const DENY_ENVIRONMENT: &str = "VCP_DENY_PROVIDER_CREDENTIALS";
pub const DENIED: &str = "provider credential access is disabled by VCP_DENY_PROVIDER_CREDENTIALS";
/// Generic credential target, per user and kept on this machine (no roaming).
pub const TARGET: &str = "VCP/OpenRouter/v1";
/// Credential Manager's generic blob ceiling (CRED_MAX_CREDENTIAL_BLOB_SIZE).
pub const STORE_LIMIT: usize = 2560;
const LIMIT: usize = 16_384;
const CONFIGURATION: &str = "credential.json";
pub const MISSING: &str = "OPENROUTER_API_KEY is required: set it in this terminal with a masked prompt, or store it once with `vcp setup credential store` for interactive terminal sessions (JSONL, redirected and automation runs use only the variable); never pass it as a command argument";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Selection {
    // Empty struct variants keep deny_unknown_fields active. Serde's
    // internally tagged unit variants otherwise accept unrecognized fields.
    Default {},
    Environment { name: String },
    Stored {},
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    version: u32,
    selection: Selection,
}

/// Environment variable names are metadata, never credential values. A portable
/// ASCII identifier avoids shell syntax and terminal control characters.
pub fn validate_environment_name(name: &str) -> Result<(), String> {
    let mut bytes = name.bytes();
    if name.len() > 128
        || !bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err("environment variable name must be 1 to 128 ASCII letters, digits or underscores and start with a letter or underscore".into());
    }
    Ok(())
}

fn selection_at(root: &Path) -> Result<Selection, String> {
    let Some(value) = crate::model_preferences::read_record(root, CONFIGURATION)? else {
        return Ok(Selection::Default {});
    };
    let configuration: Configuration = serde_json::from_value(value)
        .map_err(|_| "invalid account credential selection; use vcp setup to configure it")?;
    if configuration.version != 1 {
        return Err("unsupported account credential selection version".into());
    }
    if let Selection::Environment { name } = &configuration.selection {
        validate_environment_name(name)?;
    }
    Ok(configuration.selection)
}

fn selection() -> Result<Selection, String> {
    // Existing environment-only automation does not require account setup.
    // When account storage is configured, invalid records fail closed.
    if std::env::var_os("LOCALAPPDATA").is_none() {
        return Ok(Selection::Default {});
    }
    selection_at(&crate::model_preferences::account_root()?)
}

fn save_selection_at(root: &Path, selection: Selection) -> Result<(), String> {
    if let Selection::Environment { name } = &selection {
        validate_environment_name(name)?;
    }
    // Recovery is an explicit configuration write, never part of credential
    // lookup. Preserve malformed bytes under the same validated, pinned root.
    crate::model_preferences::ensure_root(root)?;
    let registry = crate::settings::registry_root(root)?;
    let _pin = registry
        .hold(None, true)
        .map_err(|_| "account credential directory redirected")?;
    match registry.read(Path::new(CONFIGURATION), 256 * 1024) {
        Ok(prior) if serde_json::from_slice::<serde_json::Value>(&prior.bytes).is_err() => {
            let archive = format!("credential-invalid-{}.json", vcp_domain::CommandId::new());
            std::fs::rename(root.join(CONFIGURATION), root.join(&archive))
                .map_err(|_| "invalid credential selection could not be preserved")?;
            let preserved = registry
                .read(Path::new(&archive), 256 * 1024)
                .map_err(|_| "preserved credential selection unavailable or redirected")?;
            if preserved.bytes != prior.bytes
                || preserved.version.native_identity != prior.version.native_identity
            {
                return Err("credential selection changed during recovery".into());
            }
        }
        Ok(_) => {}
        Err(vcp_repository::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("account credential selection unavailable or redirected".into()),
    }
    crate::model_preferences::save_record(
        root,
        CONFIGURATION,
        &serde_json::to_value(Configuration {
            version: 1,
            selection,
        })
        .map_err(|_| "credential selection serialization failed")?,
    )
}

/// The explicitly designated variable, or None for default source precedence.
pub fn configured_environment() -> Result<Option<String>, String> {
    Ok(match selection()? {
        Selection::Default {} | Selection::Stored {} => None,
        Selection::Environment { name } => Some(name),
    })
}

/// Name used for the process environment lookup; it contains no credential.
pub fn environment_name() -> Result<String, String> {
    Ok(configured_environment()?.unwrap_or_else(|| ENVIRONMENT.into()))
}

/// Remember only the variable's name. Neither missing nor invalid contents can
/// fall back to another key after this explicit choice. Setup should validate
/// its presence through `environment_key` before running the connection test.
pub fn select_environment(name: &str) -> Result<(), String> {
    validate_environment_name(name)?;
    save_selection_at(
        &crate::model_preferences::account_root()?,
        Selection::Environment { name: name.into() },
    )
}

/// Restore default environment-first precedence, with session/stored keys only
/// in attended terminals. This neither changes environment nor deletes a key.
pub fn select_default() -> Result<(), String> {
    save_selection_at(
        &crate::model_preferences::account_root()?,
        Selection::Default {},
    )
}

/// Explicitly select the attended session or Windows-protected key. This
/// prevents an existing environment value from overriding a newly entered key.
/// Unattended invocations still consult only OPENROUTER_API_KEY.
pub fn select_stored() -> Result<(), String> {
    save_selection_at(
        &crate::model_preferences::account_root()?,
        Selection::Stored {},
    )
}

/// Read exactly this named environment variable, with no other credential
/// sources. Present but empty, non-Unicode or malformed values fail closed.
pub fn environment_key(name: &str) -> Result<Option<Secret>, String> {
    allow_credentials(std::env::var_os(DENY_ENVIRONMENT))?;
    validate_environment_name(name)?;
    resolve(std::env::var_os(name), false, || Ok(None)).map(|value| value.map(|(key, _)| key))
}

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
        secret.validate()?;
        Ok(secret)
    }
    fn validate(&self) -> Result<(), String> {
        let text = &self.0;
        if text.is_empty() || text.len() > LIMIT || text.chars().any(char::is_control) {
            return Err(
                "the OpenRouter key must be 1 to 16384 bytes without control characters".into(),
            );
        }
        Ok(())
    }
    /// Decode directly into a wiped buffer with enough capacity for every
    /// UTF-16 unit, including invalid input's already decoded prefix. No secret
    /// bytes are left in intermediate reallocations or a conversion error.
    #[cfg(any(windows, test))]
    pub(crate) fn from_utf16(units: &[u16]) -> Result<Self, String> {
        if units.len() > LIMIT {
            return Err("the entered key is too long".into());
        }
        let mut secret = Self(String::with_capacity(units.len() * 3));
        for character in char::decode_utf16(units.iter().copied()) {
            secret
                .0
                .push(character.map_err(|_| "the entered key is not valid text")?);
        }
        secret.validate()?;
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

/// A present environment value wins or fails closed, including an empty value.
/// Other sources are consulted only when the environment is absent and the
/// caller explicitly permits attended credential lookup.
fn resolve(
    environment: Option<std::ffi::OsString>,
    interactive: bool,
    other: impl FnOnce() -> Result<Option<(Secret, Source)>, String>,
) -> Result<Option<(Secret, Source)>, String> {
    if let Some(value) = environment {
        let value = String::from_utf8(value.into_encoded_bytes()).map_err(|error| {
            wipe(&mut error.into_bytes());
            "the OpenRouter environment credential must be valid Unicode".to_owned()
        })?;
        return Secret::new(value).map(|secret| Some((secret, Source::Environment)));
    }
    if !interactive {
        return Ok(None);
    }
    other()
}

fn resolve_selected(
    selection: &Selection,
    interactive: bool,
    environment: impl FnOnce(&str) -> Option<std::ffi::OsString>,
    other: impl FnOnce() -> Result<Option<(Secret, Source)>, String>,
) -> Result<Option<(Secret, Source)>, String> {
    let (name, attended) = match selection {
        Selection::Default {} => (ENVIRONMENT, interactive),
        Selection::Environment { name } => (name.as_str(), false),
        Selection::Stored {} if interactive => return other(),
        Selection::Stored {} => (ENVIRONMENT, false),
    };
    resolve(environment(name), attended, other)
}

fn allow_credentials(denial: Option<std::ffi::OsString>) -> Result<(), String> {
    if denial.is_some() {
        return Err(DENIED.into());
    }
    Ok(())
}

fn resolve_sources(
    denial: Option<std::ffi::OsString>,
    interactive: bool,
    selection: impl FnOnce() -> Result<Selection, String>,
    environment: impl FnOnce(&str) -> Option<std::ffi::OsString>,
    other: impl FnOnce() -> Result<Option<(Secret, Source)>, String>,
) -> Result<Option<(Secret, Source)>, String> {
    allow_credentials(denial)?;
    resolve_selected(&selection()?, interactive, environment, other)
}

/// A key entered during guided setup, held only in this process memory. Child
/// processes never inherit it, unlike the environment variable.
static SESSION: std::sync::Mutex<Option<Secret>> = std::sync::Mutex::new(None);

/// Keep `secret` for the rest of this attended process.
pub fn set_session(secret: Secret) -> Result<(), String> {
    let mut session = SESSION
        .lock()
        .map_err(|_| "session credential unavailable")?;
    *session = Some(secret);
    Ok(())
}

fn session() -> Result<Option<Secret>, String> {
    SESSION
        .lock()
        .map_err(|_| "session credential unavailable")?
        .as_ref()
        .map(|secret| Secret::new(secret.expose().to_owned()))
        .transpose()
}

/// The key to use and where it came from, if any. `interactive` permits the
/// session and stored keys; it must be true only for an interactive terminal.
pub fn openrouter(interactive: bool) -> Result<Option<(Secret, Source)>, String> {
    resolve_sources(
        std::env::var_os(DENY_ENVIRONMENT),
        interactive,
        selection,
        |name| std::env::var_os(name),
        || {
            if let Some(secret) = session()? {
                return Ok(Some((secret, Source::Session)));
            }
            #[cfg(windows)]
            return Ok(read(TARGET)?.map(|secret| (secret, Source::CredentialManager)));
            #[cfg(not(windows))]
            Ok(None)
        },
    )
}

/// The key, or an error describing the selected source without exposing a key.
pub fn require(interactive: bool) -> Result<Secret, String> {
    openrouter(interactive)?
        .map(|(secret, _)| secret)
        .ok_or_else(|| match selection() {
            Ok(Selection::Environment { name }) => format!("the designated OpenRouter environment variable {name} is not set; set it in this terminal or change the credential selection with vcp setup; never pass the key as a command argument"),
            Ok(Selection::Stored {}) if interactive => "the selected Windows-protected OpenRouter key is unavailable; store it with vcp setup credential store or change the credential selection with vcp setup; never pass the key as a command argument".into(),
            Ok(_) => MISSING.into(),
            Err(error) => error,
        })
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
            "this key is too long for Windows Credential Manager; select an environment variable with vcp setup instead"
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
        let (secret, source) = resolve(Some("environment-key".into()), true, stored)
            .unwrap()
            .unwrap();
        assert_eq!(
            (secret.expose(), source),
            ("environment-key", Source::Environment)
        );
        assert_eq!(format!("{secret:?}"), "Secret([redacted])");
        let (secret, source) = resolve(None, true, stored).unwrap().unwrap();
        assert_eq!(
            (secret.expose(), source),
            ("stored-key", Source::CredentialManager)
        );
        assert!(resolve(None, true, || Ok(None)).unwrap().is_none());
        assert!(resolve(Some("".into()), true, || panic!("empty key fell back")).is_err());
        assert!(resolve(Some("bad\nkey".into()), true, stored).is_err());
        assert!(resolve(None, true, || Err("store failure".into())).is_err());
        assert!(resolve(Some("environment-key".into()), true, || panic!(
            "store consulted"
        ))
        .is_ok());
        for invalid in ["", "line\nbreak", &"k".repeat(16_385)] {
            assert!(Secret::new(invalid.into()).is_err());
        }
    }

    #[test]
    fn automation_never_consults_session_or_stored_credentials() {
        assert!(
            resolve(None, false, || panic!("attended credential consulted"))
                .unwrap()
                .is_none()
        );
        assert!(resolve(Some("".into()), false, || panic!("empty key fell back")).is_err());
        let (key, source) = resolve(Some("automation-key".into()), false, || {
            panic!("attended credential consulted")
        })
        .unwrap()
        .unwrap();
        assert_eq!(key.expose(), "automation-key");
        assert_eq!(source, Source::Environment);
        assert!(resolve(Some("invalid\nkey".into()), false, || {
            panic!("invalid environment key must fail closed")
        })
        .is_err());
    }

    #[test]
    fn denial_precedes_account_selection_and_every_credential_source() {
        for value in ["", "0", "false", "1"] {
            for interactive in [false, true] {
                assert_eq!(
                    resolve_sources(
                        Some(value.into()),
                        interactive,
                        || panic!("denied lookup read account selection"),
                        |_| panic!("denied lookup read environment credential"),
                        || panic!("denied lookup read session or stored credential"),
                    )
                    .unwrap_err(),
                    DENIED,
                );
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            assert_eq!(
                allow_credentials(Some(std::ffi::OsString::from_wide(&[0xd800]))).unwrap_err(),
                DENIED,
            );
        }
        let (key, source) = resolve_sources(
            None,
            false,
            || {
                Ok(Selection::Environment {
                    name: "MY_ROUTER_KEY".into(),
                })
            },
            |name| {
                assert_eq!(name, "MY_ROUTER_KEY");
                Some("synthetic-alias-key".into())
            },
            || panic!("unattended named environment consulted stored key"),
        )
        .unwrap()
        .unwrap();
        assert_eq!(key.expose(), "synthetic-alias-key");
        assert_eq!(source, Source::Environment);
    }

    #[cfg(windows)]
    #[test]
    fn invalid_unicode_environment_fails_closed_without_exposing_the_key() {
        use std::os::windows::ffi::OsStringExt;
        let invalid = std::ffi::OsString::from_wide(&[0xd800]);
        let error = resolve(Some(invalid), true, || panic!("store consulted")).unwrap_err();
        assert_eq!(
            error,
            "the OpenRouter environment credential must be valid Unicode"
        );
    }

    #[test]
    fn named_environment_is_the_only_source_even_when_absent_or_invalid() {
        let selected = Selection::Environment {
            name: "MY_ROUTER_KEY".into(),
        };
        for interactive in [true, false] {
            let (key, source) = resolve_selected(
                &selected,
                interactive,
                |name| {
                    assert_eq!(name, "MY_ROUTER_KEY");
                    Some("named-key".into())
                },
                || panic!("explicit environment selection fell back"),
            )
            .unwrap()
            .unwrap();
            assert_eq!(key.expose(), "named-key");
            assert_eq!(source, Source::Environment);
            assert!(resolve_selected(
                &selected,
                interactive,
                |_| None,
                || panic!("missing designated variable fell back")
            )
            .unwrap()
            .is_none());
            for value in ["", "invalid\nsecret"] {
                let error = resolve_selected(
                    &selected,
                    interactive,
                    |_| Some(value.into()),
                    || panic!("invalid designated variable fell back"),
                )
                .unwrap_err();
                assert!(!error.contains("invalid\nsecret"));
            }
        }
        let (key, source) = resolve_selected(
            &Selection::Default {},
            true,
            |name| {
                assert_eq!(name, ENVIRONMENT);
                None
            },
            || {
                Ok(Some((
                    Secret::new("stored-key".into()).unwrap(),
                    Source::CredentialManager,
                )))
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(key.expose(), "stored-key");
        assert_eq!(source, Source::CredentialManager);
    }

    #[test]
    fn explicit_stored_selection_ignores_environment_only_for_attended_calls() {
        for source in [Source::Session, Source::CredentialManager] {
            let (key, actual_source) = resolve_selected(
                &Selection::Stored {},
                true,
                |_| panic!("environment overrode explicitly selected attended credential"),
                || Ok(Some((Secret::new("selected-key".into()).unwrap(), source))),
            )
            .unwrap()
            .unwrap();
            assert_eq!(key.expose(), "selected-key");
            assert_eq!(actual_source, source);
        }
        assert!(resolve_selected(
            &Selection::Stored {},
            true,
            |_| panic!("missing stored key fell back to environment"),
            || Ok(None),
        )
        .unwrap()
        .is_none());

        let (key, source) = resolve_selected(
            &Selection::Stored {},
            false,
            |name| {
                assert_eq!(name, ENVIRONMENT);
                Some("automation-key".into())
            },
            || panic!("automation consulted stored credential"),
        )
        .unwrap()
        .unwrap();
        assert_eq!(key.expose(), "automation-key");
        assert_eq!(source, Source::Environment);
        assert!(resolve_selected(
            &Selection::Stored {},
            false,
            |_| None,
            || panic!("automation without environment consulted stored credential"),
        )
        .unwrap()
        .is_none());
        for invalid in ["", "invalid\nkey"] {
            assert!(resolve_selected(
                &Selection::Stored {},
                false,
                |_| Some(invalid.into()),
                || panic!("automation with invalid environment consulted stored credential"),
            )
            .is_err());
        }
    }

    #[test]
    fn environment_names_are_bounded_identifiers_and_values_are_never_saved() {
        for name in [ENVIRONMENT, "MY_ROUTER_KEY", "_KEY2", &"K".repeat(128)] {
            validate_environment_name(name).unwrap();
        }
        for name in [
            "",
            "2KEY",
            "KEY=secret",
            " KEY",
            "KEY ",
            "$KEY",
            "KEY\n",
            "KÉY",
            &"K".repeat(129),
        ] {
            assert!(validate_environment_name(name).is_err());
        }
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("account");
        assert_eq!(selection_at(&root).unwrap(), Selection::Default {});
        let selection = Selection::Environment {
            name: "MY_ROUTER_KEY".into(),
        };
        save_selection_at(&root, selection.clone()).unwrap();
        assert_eq!(selection_at(&root).unwrap(), selection);
        let value = crate::model_preferences::read_record(&root, CONFIGURATION)
            .unwrap()
            .unwrap();
        assert_eq!(
            value,
            serde_json::json!({"version":1,"selection":{"kind":"environment","name":"MY_ROUTER_KEY"}})
        );
        save_selection_at(&root, Selection::Default {}).unwrap();
        assert_eq!(selection_at(&root).unwrap(), Selection::Default {});
        save_selection_at(&root, Selection::Stored {}).unwrap();
        assert_eq!(selection_at(&root).unwrap(), Selection::Stored {});
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    }

    #[test]
    fn malformed_account_credential_selection_fails_closed() {
        let temporary = tempfile::tempdir().unwrap();
        for value in [
            serde_json::json!({"version":2,"selection":{"kind":"default"}}),
            serde_json::json!({"version":1,"selection":{"kind":"environment","name":"BAD=secret"}}),
            serde_json::json!({"version":1,"selection":{"kind":"environment"}}),
            serde_json::json!({"version":1,"selection":{"kind":"default","secret":"private-sentinel"}}),
            serde_json::json!({"version":1,"selection":{"kind":"stored","secret":"private-sentinel"}}),
            serde_json::json!({"version":1,"selection":{"kind":"default"},"secret":"private-sentinel"}),
        ] {
            crate::model_preferences::save_record(temporary.path(), CONFIGURATION, &value).unwrap();
            let error = selection_at(temporary.path()).unwrap_err();
            assert!(!error.contains("private-sentinel"));
            assert!(!error.contains("BAD=secret"));
        }
        std::fs::write(
            temporary.path().join(CONFIGURATION),
            vec![b' '; 256 * 1024 + 1],
        )
        .unwrap();
        assert!(selection_at(temporary.path()).is_err());
    }

    #[test]
    fn explicit_selection_recovers_malformed_json_and_preserves_exact_prior_bytes() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("account");
        crate::model_preferences::ensure_root(&root).unwrap();
        let first = b"{\"selection\": \"private-sentinel\"";
        std::fs::write(root.join(CONFIGURATION), first).unwrap();
        assert!(selection_at(&root).is_err());
        assert!(save_selection_at(
            &root,
            Selection::Environment {
                name: "BAD=name".into()
            }
        )
        .is_err());
        assert_eq!(std::fs::read(root.join(CONFIGURATION)).unwrap(), first);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        save_selection_at(&root, Selection::Stored {}).unwrap();
        assert_eq!(selection_at(&root).unwrap(), Selection::Stored {});
        let second = b"\xffinvalid JSON\x00";
        std::fs::write(root.join(CONFIGURATION), second).unwrap();
        save_selection_at(
            &root,
            Selection::Environment {
                name: "MY_ROUTER_KEY".into(),
            },
        )
        .unwrap();
        assert_eq!(
            selection_at(&root).unwrap(),
            Selection::Environment {
                name: "MY_ROUTER_KEY".into()
            }
        );
        let mut preserved = std::fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("credential-invalid-")
            })
            .map(|path| std::fs::read(path).unwrap())
            .collect::<Vec<_>>();
        preserved.sort();
        let mut expected = vec![first.to_vec(), second.to_vec()];
        expected.sort();
        assert_eq!(preserved, expected);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 3);
    }

    #[test]
    fn credential_recovery_rejects_oversized_records_and_directories_without_moving_them() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("account");
        crate::model_preferences::ensure_root(&root).unwrap();
        let target = root.join(CONFIGURATION);
        let oversized = vec![b'x'; 256 * 1024 + 1];
        std::fs::write(&target, &oversized).unwrap();
        assert!(save_selection_at(&root, Selection::Stored {}).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), oversized);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_file(&target).unwrap();
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("keep.txt"), b"unchanged").unwrap();
        assert!(save_selection_at(&root, Selection::Stored {}).is_err());
        assert_eq!(
            std::fs::read(target.join("keep.txt")).unwrap(),
            b"unchanged"
        );
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn credential_recovery_rejects_redirected_account_and_record_paths() {
        let temporary = tempfile::tempdir().unwrap();
        let base = temporary.path().canonicalize().unwrap();
        let outside = base.join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join(CONFIGURATION), b"{invalid").unwrap();
        for redirected in [
            base.join("account"),
            base.join("safe-account").join(CONFIGURATION),
        ] {
            std::fs::create_dir_all(redirected.parent().unwrap()).unwrap();
            let result = std::process::Command::new("cmd.exe")
                .args(["/d", "/c", "mklink", "/J"])
                .arg(&redirected)
                .arg(&outside)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "junction fixture prerequisite failed"
            );
            let root = if redirected.file_name().unwrap() == CONFIGURATION {
                redirected.parent().unwrap()
            } else {
                redirected.as_path()
            };
            assert!(save_selection_at(root, Selection::Stored {}).is_err());
            assert_eq!(
                std::fs::read(outside.join(CONFIGURATION)).unwrap(),
                b"{invalid"
            );
            assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 1);
            std::fs::remove_dir(redirected).unwrap();
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
