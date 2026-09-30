// SPDX-License-Identifier: Apache-2.0
//! Installed launch selection. Installation metadata grants no workspace trust.
use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use vcp_repository::path::HeldPath;

type Result<T> = std::result::Result<T, String>;

#[derive(Deserialize)]
struct Owner {
    schema: String,
    install_root: PathBuf,
    data_root: PathBuf,
}

#[derive(Deserialize)]
struct Pointer {
    schema: String,
    release: String,
    package_sha256: String,
    data_root: PathBuf,
}

#[derive(Deserialize)]
struct Manifest {
    schema: String,
    files: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    path: String,
    bytes: u64,
    sha256: String,
}

fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn read<T: serde::de::DeserializeOwned>(root: &vcp_repository::Root, path: &Path) -> Result<T> {
    let bytes = root.read(path, 2 * 1024 * 1024).map_err(|_| {
        "installation metadata is unavailable or redirected; repair the installation"
    })?;
    serde_json::from_slice(&bytes.bytes)
        .map_err(|_| "invalid installation metadata; repair the installation".into())
}

fn owner(root: &vcp_repository::Root) -> Result<Owner> {
    let _pin = root
        .hold(Some(Path::new(".vcp-install-owned.json")), false)
        .map_err(|_| "installation ownership is unavailable or redirected")?;
    let owner: Owner = read(root, Path::new(".vcp-install-owned.json"))?;
    if owner.schema != "vcp-install-owned/1"
        || !owner.install_root.is_absolute()
        || !owner.data_root.is_absolute()
    {
        return Err("unsupported installation ownership or data directory".into());
    }
    let selected = crate::settings::registry_root(&owner.install_root)?;
    if selected
        .hold(None, true)
        .map_err(|_| "installation root unavailable")?
        .native_identity
        != root
            .hold(None, true)
            .map_err(|_| "installation root unavailable")?
            .native_identity
    {
        return Err("installation ownership names a different root".into());
    }
    // Reuse native no-follow validation for every existing data-path ancestor.
    let ancestor = owner
        .data_root
        .ancestors()
        .find(|path| path.exists())
        .ok_or("installed data directory has no accessible ancestor")?;
    let data_ancestor = crate::settings::registry_root(ancestor)?;
    let data = data_ancestor.path().join(
        owner
            .data_root
            .strip_prefix(ancestor)
            .map_err(|_| "invalid installed data directory")?,
    );
    let install = root.path();
    if data.starts_with(install)
        || install.starts_with(&data)
        || owner
            .data_root
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err("installed data and program directories must be disjoint".into());
    }
    Ok(owner)
}

/// Read the chosen data root for an engine running inside a retained release.
/// Portable executables have no installation ancestry and retain their default.
pub fn installed_data(executable: &Path) -> Result<Option<PathBuf>> {
    let Some(release) = executable.parent() else {
        return Ok(None);
    };
    let Some(releases) = release.parent() else {
        return Ok(None);
    };
    if !releases
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("releases"))
    {
        return Ok(None);
    }
    let Some(id) = release
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| name.len() == 64 && name.bytes().all(|byte| byte.is_ascii_hexdigit()))
    else {
        return Ok(None);
    };
    let install = releases
        .parent()
        .ok_or("installed release has no owned root")?;
    let root = crate::settings::registry_root(install)?;
    let _release = root
        .hold(Some(&Path::new("releases").join(id)), true)
        .map_err(|_| "installed release is unavailable or redirected")?;
    Ok(Some(owner(&root)?.data_root))
}

/// Pins the selected executable against writes and replacement until launch.
pub struct Selection {
    pub executable: PathBuf,
    pub data: PathBuf,
    _executable: HeldPath,
    _owner: HeldPath,
}

pub fn select(install: &Path) -> Result<Selection> {
    let root = crate::settings::registry_root(install)?;
    let owner_pin = root
        .hold(Some(Path::new(".vcp-install-owned.json")), false)
        .map_err(|_| "installation ownership is unavailable")?;
    let _pointer_pin = root
        .hold(Some(Path::new("active.json")), false)
        .map_err(|_| "no active installed release; finish or repair the installation")?;
    let owner = owner(&root)?;
    let pointer: Pointer = read(&root, Path::new("active.json"))?;
    if pointer.schema != "vcp-install-pointer/1"
        || !hash(&pointer.release)
        || pointer.package_sha256 != pointer.release
        || pointer.data_root != owner.data_root
    {
        return Err(
            "active release identity or data directory does not match installation ownership"
                .into(),
        );
    }
    let release = Path::new("releases").join(&pointer.release);
    let manifest_path = release.join("manifest.json");
    let _manifest_pin = root
        .hold(Some(&manifest_path), false)
        .map_err(|_| "installed manifest unavailable")?;
    let manifest: Manifest = read(&root, &manifest_path)?;
    if manifest.schema != "vcp-distribution-manifest/1" || manifest.files.len() > 4096 {
        return Err("unsupported installed payload manifest".into());
    }
    let mut entries = manifest
        .files
        .iter()
        .filter(|entry| entry.path.eq_ignore_ascii_case("vcp.exe"));
    let entry = entries
        .next()
        .ok_or("installed engine missing from manifest")?;
    if entries.next().is_some()
        || entry.path != "vcp.exe"
        || !hash(&entry.sha256)
        || entry.bytes == 0
        || entry.bytes > 1024 * 1024 * 1024
    {
        return Err("invalid installed engine manifest entry".into());
    }
    let relative = release.join("vcp.exe");
    let pin = root
        .hold(Some(&relative), false)
        .map_err(|_| "installed engine unavailable or redirected")?;
    // The held file and all ancestor handles deny replacement while the second
    // handle streams the large native image; never load it as source/context.
    let file = std::fs::File::open(root.path().join(&relative))
        .map_err(|_| "installed engine cannot be read")?;
    let (digest, bytes) = vcp_protocol::digest_reader(file.take(entry.bytes + 1))
        .map_err(|_| "installed engine cannot be verified")?;
    if bytes != entry.bytes || digest != entry.sha256 {
        return Err("installed engine hash mismatch; repair the installation".into());
    }
    Ok(Selection {
        executable: root.path().join(relative),
        data: owner.data_root,
        _executable: pin,
        _owner: owner_pin,
    })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temporary = tempfile::tempdir().unwrap();
        let install = temporary.path().join("Program Files β");
        let data = temporary.path().join("User Data β");
        let id = "a".repeat(64);
        let release = install.join("releases").join(&id);
        fs::create_dir_all(&release).unwrap();
        fs::write(release.join("vcp.exe"), b"fake native image").unwrap();
        fs::write(
            install.join(".vcp-install-owned.json"),
            serde_json::to_vec(&json!({
                "schema":"vcp-install-owned/1", "install_root":install, "data_root":data,
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(install.join("active.json"), serde_json::to_vec(&json!({
            "schema":"vcp-install-pointer/1", "release":id, "package_sha256":id, "data_root":data,
        })).unwrap()).unwrap();
        fs::write(
            release.join("manifest.json"),
            serde_json::to_vec(&json!({
                "schema":"vcp-distribution-manifest/1", "files":[{"path":"vcp.exe", "bytes":17,
                    "sha256":vcp_protocol::digest_bytes(b"fake native image")}],
            }))
            .unwrap(),
        )
        .unwrap();
        (temporary, install, data)
    }

    #[test]
    fn installed_launch_binds_data_and_holds_image_against_writes() {
        let (_temporary, install, data) = fixture();
        let selected = select(&install).unwrap();
        assert_eq!(selected.data, data);
        assert_eq!(installed_data(&selected.executable).unwrap(), Some(data));
        assert_eq!(
            installed_data(
                &install
                    .join("Releases")
                    .join("A".repeat(64))
                    .join("VCP.EXE")
            )
            .unwrap(),
            Some(selected.data.clone())
        );
        assert!(fs::write(&selected.executable, b"replacement").is_err());
        assert!(fs::write(
            install.join(".vcp-install-owned.json"),
            b"changed data root"
        )
        .is_err());
        assert!(fs::rename(
            &selected.executable,
            selected.executable.with_extension("old")
        )
        .is_err());
        let executable = selected.executable.clone();
        drop(selected);
        fs::write(&executable, b"replacement").unwrap();
        assert!(select(&install).is_err());
    }

    #[test]
    fn tampered_pointer_and_ownership_never_select_a_fallback() {
        let (_temporary, install, _data) = fixture();
        let path = install.join("active.json");
        let bytes = fs::read(&path).unwrap();
        for (key, value) in [
            ("release", "../../outside"),
            ("package_sha256", "wrong"),
            ("data_root", "C:/different"),
            ("schema", "vcp-install-pointer/2"),
        ] {
            let mut pointer: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            pointer[key] = value.into();
            fs::write(&path, serde_json::to_vec(&pointer).unwrap()).unwrap();
            assert!(select(&install).is_err(), "{key}");
        }
        fs::write(path, bytes).unwrap();
        let engine = select(&install).unwrap().executable;
        fs::remove_file(install.join(".vcp-install-owned.json")).unwrap();
        assert!(select(&install).is_err());
        assert!(installed_data(&engine).is_err());
    }

    #[test]
    fn linked_release_is_refused_and_portable_has_no_installed_default() {
        let (_temporary, install, data) = fixture();
        let selected = select(&install).unwrap();
        let release = selected.executable.parent().unwrap().to_path_buf();
        drop(selected);
        let retained = install.join("retained");
        fs::rename(&release, &retained).unwrap();
        let status = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&release)
            .arg(&retained)
            .output()
            .unwrap();
        assert!(status.status.success());
        assert!(select(&install).is_err());
        fs::remove_dir(&release).unwrap();
        assert_eq!(
            installed_data(&data.join("portable/vcp.exe")).unwrap(),
            None
        );
    }
}
