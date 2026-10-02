// SPDX-License-Identifier: Apache-2.0
//! Read-only native path diagnostics for explicit portability setup.
use clap::Args;
use std::path::{Path, PathBuf};

#[derive(Debug, Args)]
pub struct Doctor {
    #[arg(long)]
    pub vault: Option<PathBuf>,
    #[arg(long)]
    pub staging: Option<PathBuf>,
    #[arg(long)]
    pub sync_root: Vec<PathBuf>,
}

pub fn execute(
    request: &Doctor,
    data: &Path,
    workspace: &Path,
) -> Result<serde_json::Value, String> {
    if request.sync_root.len() > 64 {
        return Err("too many declared synchronization roots".into());
    }
    let mut paths = vec![
        ("workspace", workspace.to_owned()),
        ("local_data", data.to_owned()),
    ];
    if let Some(path) = &request.staging {
        paths.push(("staging", path.clone()));
    }
    if let Some(path) = &request.vault {
        paths.push(("vault", path.clone()));
    }
    paths.extend(request.sync_root.iter().cloned().map(|p| ("sync_root", p)));
    for name in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
        if let Some(path) = std::env::var_os(name) {
            if Path::new(&path).exists() {
                paths.push(("sync_root", path.into()));
            }
        }
    }
    let mut roots = Vec::new();
    let mut private_guards = Vec::new();
    let mut cloud_guards = Vec::new();
    let mut observations = Vec::new();
    let mut issues = Vec::new();
    for (kind, path) in paths {
        let checked = if matches!(kind, "vault" | "sync_root") {
            vcp_store::vault_publish::Vault::open(&path, &[workspace.to_owned(), data.to_owned()])
                .map(|vault| {
                    let path = vault.directory().to_owned();
                    cloud_guards.push(vault);
                    path
                })
                .map_err(|error| error.to_string())
        } else {
            crate::settings::registry_root(&path).and_then(|root| {
                let pin = root.hold(None, true).map_err(|e| e.to_string())?;
                let path = root.path().to_owned();
                private_guards.push((root, pin));
                Ok(path)
            })
        };
        match checked {
            Ok(path) => {
                observations
                    .push(serde_json::json!({"kind":kind,"path":path,"native_path_verified":true}));
                roots.push((kind, path));
            }
            Err(_) => {
                observations.push(
                    serde_json::json!({"kind":kind,"path":path,"native_path_verified":false}),
                );
                issues.push(serde_json::json!({"category":"path_unavailable_or_redirected","kind":kind,"next_action":"select an existing native directory without redirected ancestors"}));
            }
        }
    }
    for (index, (left_kind, left)) in roots.iter().enumerate() {
        for (right_kind, right) in roots.iter().skip(index + 1) {
            // A vault inside its declared synchronization root is expected.
            // Private state and the workspace must remain outside those roots.
            if (*left_kind == "sync_root" && *right_kind == "sync_root")
                || (*left_kind == "vault" && *right_kind == "sync_root")
                || (*right_kind == "vault" && *left_kind == "sync_root")
            {
                continue;
            }
            if left.starts_with(right) || right.starts_with(left) {
                issues.push(serde_json::json!({"category":"path_overlap","left":left_kind,"right":right_kind,"next_action":"select separate private, workspace, and vault directories"}));
            }
        }
    }
    Ok(serde_json::json!({
        "path_checks_passed": issues.is_empty(),
        "paths": observations,
        "issues": issues,
        "vault_checked":request.vault.is_some(),
        "staging_checked":request.staging.is_some(),
        "scope":"explicit paths and known or declared synchronization roots only",
        "unknown_synchronizers_detected":false,
        "recovery_verified":false,
        "cloud_transfer_verified":false,
        "next_action":"separately verify recovery keys and perform actual provider handoff"
    }))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires explicit VCP_TEST_ONEDRIVE_ROOT; read-only native cloud-path qualification"]
    fn actual_cloud_directory_uses_public_vault_policy() {
        let sync = PathBuf::from(std::env::var_os("VCP_TEST_ONEDRIVE_ROOT").unwrap());
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let data = temp.path().join("data");
        let staging = temp.path().join("staging");
        for path in [&workspace, &data, &staging] {
            std::fs::create_dir(path).unwrap();
        }
        let request = Doctor {
            vault: Some(sync.clone()),
            staging: Some(staging),
            sync_root: vec![sync],
        };
        let result = execute(&request, &data, &workspace).unwrap();
        assert_eq!(result["path_checks_passed"], true, "{result}");
        assert_eq!(result["cloud_transfer_verified"], false);
    }
    #[test]
    fn detects_private_overlap_and_preserves_files() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let data = temp.path().join("data");
        let sync = temp.path().join("sync");
        let vault = sync.join("vault");
        let staging = temp.path().join("staging");
        for path in [&workspace, &data, &vault, &staging] {
            std::fs::create_dir_all(path).unwrap();
        }
        std::fs::write(workspace.join("marker"), b"unchanged").unwrap();
        let mut request = Doctor {
            vault: Some(vault),
            staging: Some(staging),
            sync_root: vec![sync],
        };
        assert_eq!(
            execute(&request, &data, &workspace).unwrap()["path_checks_passed"],
            true
        );
        request.staging = Some(data.clone());
        assert_eq!(
            execute(&request, &data, &workspace).unwrap()["path_checks_passed"],
            false
        );
        request.staging = Some(temp.path().join("missing"));
        assert_eq!(
            execute(&request, &data, &workspace).unwrap()["path_checks_passed"],
            false
        );
        let redirected = temp.path().join("redirected");
        let junction = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&redirected)
            .arg(&data)
            .output()
            .unwrap();
        assert!(
            junction.status.success(),
            "junction fixture prerequisite failed"
        );
        request.staging = Some(redirected.clone());
        let observed = execute(&request, &data, &workspace).unwrap();
        assert_eq!(observed["path_checks_passed"], false);
        assert!(observed["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["category"] == "path_unavailable_or_redirected"));
        std::fs::remove_dir(redirected).unwrap();
        assert_eq!(
            std::fs::read(workspace.join("marker")).unwrap(),
            b"unchanged"
        );
    }
}
