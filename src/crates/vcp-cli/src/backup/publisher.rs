// SPDX-License-Identifier: Apache-2.0
//! Explicit native host selection. Recovery bytes never enter public bootstrap.
use super::*;
use std::sync::Arc;
use vcp_lifecycle::foundation::{backup_run::Capabilities, CanonicalHost};
use vcp_repository::{git::Git, path::HeldPath};

pub(crate) enum Selection {
    Direct { key: PathBuf, git: PathBuf },
    Profile(PathBuf),
}
pub(crate) struct Loaded {
    pub capabilities: Arc<Capabilities>,
    pub git: Arc<Git>,
    pub automatic: bool,
    pub revision: u64,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    version: u32,
    key: PathBuf,
    git: PathBuf,
}
const PROFILE_LIMIT: usize = 16 * 1024;
const UNAVAILABLE: &str = "native encrypted publisher capability unavailable";

fn selected_file(
    path: &Path,
    workspace: &Path,
    excluded: &[PathBuf],
) -> Result<(PathBuf, HeldPath)> {
    if !path.is_absolute() || path.as_os_str().len() > 32768 {
        return Err(UNAVAILABLE.into());
    }
    // Open original spelling first: canonicalization alone would hide junctions.
    let parent = path.parent().ok_or(UNAVAILABLE)?;
    let name = Path::new(path.file_name().ok_or(UNAVAILABLE)?);
    let root = crate::settings::registry_root(parent).map_err(|_| UNAVAILABLE)?;
    let pin = root.hold(Some(name), false).map_err(|_| UNAVAILABLE)?;
    let canonical = crate::settings::local_path(path, workspace).map_err(|_| UNAVAILABLE)?;
    // A workspace-writable hard-link alias must not supply host authority.
    let file = std::fs::File::open(&canonical).map_err(|_| UNAVAILABLE)?;
    use std::os::windows::io::AsRawHandle;
    let mut information = std::mem::MaybeUninit::zeroed();
    // SAFETY: the owned file remains open and the output has the native layout.
    if unsafe {
        windows_sys::Win32::Storage::FileSystem::GetFileInformationByHandle(
            file.as_raw_handle(),
            information.as_mut_ptr(),
        )
    } == 0
    {
        return Err(UNAVAILABLE.into());
    }
    if unsafe { information.assume_init() }.nNumberOfLinks != 1 {
        return Err(UNAVAILABLE.into());
    }
    for forbidden in excluded {
        let forbidden = forbidden.canonicalize().map_err(|_| UNAVAILABLE)?;
        if crate::settings::within(&canonical, &forbidden) {
            return Err(UNAVAILABLE.into());
        }
    }
    Ok((canonical, pin))
}
fn profile(
    path: &Path,
    workspace: &Path,
    excluded: &[PathBuf],
) -> Result<(Profile, HeldPath, HeldPath)> {
    let (path, _profile_pin) = selected_file(path, workspace, excluded)?;
    let bytes = crate::settings::read_bounded(&path, PROFILE_LIMIT).map_err(|_| UNAVAILABLE)?;
    let profile: Profile = serde_json::from_slice(&bytes).map_err(|_| UNAVAILABLE)?;
    if profile.version != 1 || !profile.key.is_absolute() || !profile.git.is_absolute() {
        return Err(UNAVAILABLE.into());
    }
    // Independently reject workspace/reparse key references before recovery verification.
    let (key, key_pin) = selected_file(&profile.key, workspace, excluded)?;
    let (git, git_pin) = selected_file(&profile.git, workspace, excluded)?;
    Ok((
        Profile {
            version: 1,
            key,
            git,
        },
        git_pin,
        key_pin,
    ))
}
pub(crate) async fn load(
    host: &CanonicalHost,
    data: &Path,
    selection: Selection,
) -> Result<Loaded> {
    let config = host.backup_configuration()?;
    let data = data.to_owned();
    tokio::task::spawn_blocking(move || -> Result<Loaded> {
        let workspace = Path::new(&config.binding.root);
        let roots = forbidden(workspace, &config.canonical_root, &[])?;
        let path = crate::settings::local_path(&trust_path(&data, &config.workspace), workspace)?;
        let trust = TrustStore::open(&path, &roots).map_err(|e| e.to_string())?;
        let setup =
            vcp_lifecycle::foundation::backup::setup(&trust)?.ok_or("backup is not configured")?;
        let mut exclusions = forbidden(workspace, &config.canonical_root, &setup.sync_roots)?;
        exclusions.extend([path, setup.staging.clone(), setup.vault.clone()]);
        let (key, git, git_pin, key_pin) = match selection {
            Selection::Direct { key, git } => (key, git, None, None),
            Selection::Profile(path) => {
                let (profile, pin, key_pin) = profile(&path, workspace, &exclusions)?;
                (profile.key, profile.git, Some(pin), Some(key_pin))
            }
        };
        let keys = verified_key(&key, &exclusions)?;
        drop(key_pin);
        let git = Git::new(
            git,
            ["PATH", "SystemRoot", "WINDIR", "TEMP", "TMP"]
                .into_iter()
                .filter_map(|name| {
                    std::env::var_os(name).map(|value| (std::ffi::OsString::from(name), value))
                })
                .collect(),
            std::time::Duration::from_secs(30),
            8 * 1024 * 1024,
        )
        .map_err(|e| e.to_string())?;
        let git = match git_pin {
            Some(pin) => git.with_executable_pin(pin).map_err(|e| e.to_string())?,
            None => git,
        };
        let capabilities =
            Capabilities::open(trust, keys, &setup, workspace, &config.canonical_root)?;
        Ok(Loaded {
            capabilities: Arc::new(capabilities),
            git: Arc::new(git),
            automatic: setup.automatic,
            revision: setup.revision,
        })
    })
    .await
    .map_err(|_| "backup key loading worker stopped")?
}
/// Failed selection leaves ordinary inspection usable, without logging native errors.
/// Native executable authority is retained by the loaded Git capability.
pub(crate) async fn install(host: &CanonicalHost, data: &Path, path: PathBuf) -> Option<()> {
    let loaded = load(host, data, Selection::Profile(path)).await.ok()?;
    host.load_backup_with_revision(loaded.capabilities, loaded.git, false, loaded.revision)
        .ok()?;
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn publisher_profile_is_bounded_external_and_pins_git() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let workspace = base.join("workspace");
        let private = base.join("private");
        std::fs::create_dir(&workspace).unwrap();
        std::fs::create_dir(&private).unwrap();
        let key = private.join("key.recovery");
        let git = private.join("git.exe");
        let selected = private.join("publisher.json");
        std::fs::write(&key, b"path-only fixture, not a key").unwrap();
        std::fs::write(&git, b"not executed").unwrap();
        let write = |value: serde_json::Value| {
            std::fs::write(&selected, serde_json::to_vec(&value).unwrap()).unwrap()
        };
        write(serde_json::json!({"version":1,"key":key,"git":git}));
        let (loaded, pin, _key_pin) = profile(&selected, &workspace, &[]).unwrap();
        assert_eq!(loaded.key, key);
        assert_eq!(loaded.git, git);
        assert!(std::fs::write(&git, b"replacement").is_err());
        drop(pin);
        for value in [
            serde_json::json!({"version":2,"key":key,"git":git}),
            serde_json::json!({"version":1,"key":"relative","git":git}),
            serde_json::json!({"version":1,"key":key,"git":git,"automatic":true}),
        ] {
            write(value);
            assert!(profile(&selected, &workspace, &[]).is_err());
        }
        write(serde_json::json!({"version":1,"key":key,"git":git}));
        assert!(profile(&selected, &workspace, std::slice::from_ref(&private)).is_err());
        let inside = workspace.join("publisher.json");
        std::fs::copy(&selected, &inside).unwrap();
        assert!(profile(&inside, &workspace, &[]).is_err());
        let alias = workspace.join("alias.json");
        std::fs::hard_link(&selected, &alias).unwrap();
        assert!(profile(&selected, &workspace, &[]).is_err());
        std::fs::remove_file(alias).unwrap();
        std::fs::write(&selected, vec![b' '; PROFILE_LIMIT + 1]).unwrap();
        assert!(profile(&selected, &workspace, &[]).is_err());
    }
    #[test]
    fn publisher_profile_rejects_redirected_parent() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let workspace = base.join("workspace");
        let private = base.join("private");
        let link = base.join("redirected");
        std::fs::create_dir(&workspace).unwrap();
        std::fs::create_dir(&private).unwrap();
        std::fs::write(private.join("publisher.json"), b"{}").unwrap();
        let output = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&link)
            .arg(&private)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(profile(&link.join("publisher.json"), &workspace, &[]).is_err());
        std::fs::remove_dir(&link).unwrap();
        assert_eq!(
            std::fs::read(private.join("publisher.json")).unwrap(),
            b"{}"
        );
    }
}
