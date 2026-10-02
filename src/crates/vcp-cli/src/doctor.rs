// SPDX-License-Identifier: Apache-2.0
//! Read-only native path diagnostics for explicit portability setup, plus an
//! offline readiness checklist. Neither reads credential values nor calls a model.
use clap::Args;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use vcp_domain::Timestamp;

#[derive(Debug, Args)]
pub struct Doctor {
    #[arg(long)]
    pub vault: Option<PathBuf>,
    #[arg(long)]
    pub staging: Option<PathBuf>,
    #[arg(long)]
    pub sync_root: Vec<PathBuf>,
}

/// Process facts the readiness checklist reads; tests supply their own.
pub struct Environment {
    /// "installed" or "portable", or why the installation cannot be resolved.
    pub installation: Result<&'static str, String>,
    /// Where an interactive session would take the provider key from.
    pub credential: Option<crate::credential::Source>,
    pub home: Option<PathBuf>,
    pub now: Timestamp,
}

impl Environment {
    pub fn current() -> Self {
        #[cfg(windows)]
        let installation = std::env::current_exe()
            .map_err(|_| "CLI location unavailable".to_owned())
            .and_then(|executable| crate::installation::installed_data(&executable))
            .map(|data| {
                if data.is_some() {
                    "installed"
                } else {
                    "portable"
                }
            });
        #[cfg(not(windows))]
        let installation = Ok("portable");
        Self {
            installation,
            credential: crate::credential::source(true),
            home: std::env::var_os("USERPROFILE")
                .and_then(|home| PathBuf::from(home).canonicalize().ok()),
            now: crate::settings::now(),
        }
    }
}

fn item(check: &str, label: &str, status: &str, detail: String, next: Option<&str>) -> Value {
    json!({"check":check,"label":label,"status":status,"detail":detail,"next":next})
}

/// Offline checks in setup order. A failure names its next step; later checks
/// still run so one report shows everything that needs attention.
pub fn readiness(
    data: &Path,
    workspace: &Path,
    config: Option<&Path>,
    environment: &Environment,
) -> Vec<Value> {
    let version = env!("CARGO_PKG_VERSION");
    let mut items = vec![match &environment.installation {
        Ok(kind) => item(
            "installation",
            "Installation",
            "ok",
            format!("{kind} engine {version}"),
            None,
        ),
        Err(error) => item(
            "installation",
            "Installation",
            "fail",
            format!("engine {version}: {error}"),
            Some("repair or reinstall VCP with its setup program"),
        ),
    }];
    let Ok(workspace) = workspace.canonicalize() else {
        items.push(item(
            "workspace",
            "Workspace",
            "fail",
            format!("{} is not an accessible folder", workspace.display()),
            Some("pass --workspace <existing project folder>"),
        ));
        return items;
    };
    let shown = crate::settings::display_path(&workspace);
    let home = environment.home.as_deref().is_some_and(|home| {
        crate::settings::within(home, &workspace) && crate::settings::within(&workspace, home)
    });
    items.push(if home {
        item(
            "workspace",
            "Workspace",
            "fail",
            format!("{shown} is your home folder, not a project folder"),
            Some("cd into a project folder and run `vcp setup`, or pass --workspace <project folder>"),
        )
    } else {
        item("workspace", "Workspace", "ok", shown, None)
    });
    items.push(match crate::settings::local_path(data, &workspace) {
        Err(error) => {
            let message = crate::render::data_placement(error, data, &workspace);
            let (detail, next) = message
                .split_once("\n  Next: ")
                .map_or((message.clone(), None), |(detail, next)| {
                    (detail.to_owned(), Some(next.replace("\n  ", " ")))
                });
            item(
                "data_folder",
                "Data folder",
                "fail",
                detail,
                next.as_deref(),
            )
        }
        Ok(resolved) if resolved.is_dir() => item(
            "data_folder",
            "Data folder",
            "ok",
            crate::settings::display_path(&resolved),
            None,
        ),
        Ok(resolved) => item(
            "data_folder",
            "Data folder",
            "warn",
            format!(
                "{} does not exist yet",
                crate::settings::display_path(&resolved)
            ),
            Some("create it, or pass --data-dir <existing private folder>"),
        ),
    });
    items.push(match environment.credential {
        Some(crate::credential::Source::Environment) => item(
            "credential",
            "Provider key",
            "ok",
            "OPENROUTER_API_KEY is set in this terminal (value not shown)".into(),
            None,
        ),
        Some(crate::credential::Source::Session) => item(
            "credential",
            "Provider key",
            "ok",
            "entered for this setup session only (value not shown)".into(),
            None,
        ),
        Some(crate::credential::Source::CredentialManager) => item(
            "credential",
            "Provider key",
            "ok",
            "stored in Windows Credential Manager (value not shown); used only in interactive terminal sessions".into(),
            None,
        ),
        None => item(
            "credential",
            "Provider key",
            "warn",
            "OPENROUTER_API_KEY is not set and no key is stored".into(),
            Some("run `vcp setup credential store`, or set OPENROUTER_API_KEY with a masked prompt; see docs/usage/beta-onboarding.md"),
        ),
    });
    let (path, source) = match crate::profile_selection::resolve(data, &workspace, config) {
        Ok(resolved) => (resolved.path, resolved.source),
        Err(error) => {
            let detail = if error == crate::profile_selection::NO_PROFILE {
                "no profile selected for this workspace".into()
            } else {
                error
            };
            items.push(item(
                "profile",
                "Profile",
                "fail",
                detail,
                Some("run `vcp setup`, or pass --config <profile>"),
            ));
            return items;
        }
    };
    let shown = crate::settings::display_path(&path);
    let profile = match crate::settings::load(&path, &workspace) {
        Ok(profile) => profile,
        Err(error) => {
            items.push(item(
                "profile",
                "Profile",
                "fail",
                format!("{shown}: {error}"),
                Some("pass the --config created for this workspace, or run `vcp setup`"),
            ));
            return items;
        }
    };
    let source = match source {
        crate::profile_selection::Source::Explicit => "from --config",
        crate::profile_selection::Source::Selected => "selected for this workspace",
        crate::profile_selection::Source::Legacy => "legacy default in the data folder",
    };
    items.push(item(
        "profile",
        "Profile",
        "ok",
        format!("{shown} ({source})"),
        None,
    ));
    let provider = &profile.provider.compatibility;
    let model = format!("{} @ {}", provider.model, provider.endpoint);
    let current = profile.provider.current(environment.now).is_ok();
    let mut metadata = item(
        "provider_metadata",
        "Model metadata",
        if current { "ok" } else { "fail" },
        crate::terminal::sanitize(&model, 256),
        (!current)
            .then_some("renew with `vcp setup provider` and create a new profile; see `vcp setup`"),
    );
    metadata["valid_until"] = json!(profile.provider.valid_until);
    items.push(metadata);
    if current {
        items.push(
            match crate::onboarding::check(&path, &workspace, environment.credential.is_some()) {
                Ok(_) => item(
                    "profile_check",
                    "Profile check",
                    "ok",
                    "tools, budget, checks and processes validated offline".into(),
                    None,
                ),
                Err(error) => item(
                    "profile_check",
                    "Profile check",
                    "fail",
                    crate::terminal::sanitize(&error, 512),
                    Some("run `vcp setup check` for this profile and correct the reported setting"),
                ),
            },
        );
    }
    items
}

pub fn execute(
    request: &Doctor,
    data: &Path,
    workspace: &Path,
    config: Option<&Path>,
) -> Result<serde_json::Value, String> {
    execute_with(request, data, workspace, config, &Environment::current())
}

pub fn execute_with(
    request: &Doctor,
    data: &Path,
    workspace: &Path,
    config: Option<&Path>,
    environment: &Environment,
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
    let readiness = readiness(data, workspace, config, environment);
    let ready = issues.is_empty() && readiness.iter().all(|item| item["status"] != "fail");
    Ok(serde_json::json!({
        "workspace": workspace.canonicalize().unwrap_or(workspace.to_path_buf()),
        "ready": ready,
        "readiness": readiness,
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
        let result = execute(&request, &data, &workspace, None).unwrap();
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
            execute(&request, &data, &workspace, None).unwrap()["path_checks_passed"],
            true
        );
        request.staging = Some(data.clone());
        assert_eq!(
            execute(&request, &data, &workspace, None).unwrap()["path_checks_passed"],
            false
        );
        request.staging = Some(temp.path().join("missing"));
        assert_eq!(
            execute(&request, &data, &workspace, None).unwrap()["path_checks_passed"],
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
        let observed = execute(&request, &data, &workspace, None).unwrap();
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

    fn environment(credential_present: bool, home: Option<PathBuf>) -> Environment {
        let credential = credential_present.then_some(crate::credential::Source::Environment);
        Environment {
            installation: Ok("portable"),
            credential,
            home,
            now: Timestamp::new(1_759_400_000_000),
        }
    }

    fn status<'a>(items: &'a [Value], check: &str) -> &'a Value {
        items
            .iter()
            .find(|item| item["check"] == check)
            .unwrap_or_else(|| panic!("missing {check}: {items:?}"))
    }

    #[test]
    fn readiness_names_each_missing_step_without_reading_secrets() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let data = temp.path().join("data");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        let items = readiness(&data, &workspace, None, &environment(false, None));
        assert_eq!(status(&items, "installation")["status"], "ok");
        assert_eq!(status(&items, "workspace")["status"], "ok");
        assert_eq!(status(&items, "data_folder")["status"], "ok");
        assert_eq!(status(&items, "credential")["status"], "warn");
        let profile = status(&items, "profile");
        assert_eq!(profile["status"], "fail");
        assert_eq!(
            profile["next"],
            "run `vcp setup`, or pass --config <profile>"
        );
        let items = readiness(&data, &workspace, None, &environment(true, None));
        assert_eq!(status(&items, "credential")["status"], "ok");
        assert!(!items.iter().any(|item| item.to_string().contains("sk-")));

        let invalid = temp.path().join("invalid.json");
        std::fs::write(&invalid, b"{\"not\":\"a profile\"}").unwrap();
        let items = readiness(&data, &workspace, Some(&invalid), &environment(true, None));
        let profile = status(&items, "profile");
        assert_eq!(profile["status"], "fail");
        assert!(profile["detail"].as_str().unwrap().contains("invalid.json"));
        assert!(items
            .iter()
            .all(|item| item["check"] != "provider_metadata"));
    }

    #[test]
    fn readiness_explains_home_and_contained_data_folders() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().canonicalize().unwrap();
        let data = home.join("AppData").join("Local").join("VCP");
        std::fs::create_dir_all(&data).unwrap();
        let items = readiness(&data, &home, None, &environment(true, Some(home.clone())));
        let workspace = status(&items, "workspace");
        assert_eq!(workspace["status"], "fail");
        assert!(workspace["next"]
            .as_str()
            .unwrap()
            .starts_with("cd into a project folder"));
        let folder = status(&items, "data_folder");
        assert_eq!(folder["status"], "fail");
        assert!(
            folder["detail"].as_str().unwrap().contains("inside"),
            "{folder}"
        );
        assert!(
            folder["next"].as_str().unwrap().contains("--data-dir"),
            "{folder}"
        );
        let request = Doctor {
            vault: None,
            staging: None,
            sync_root: Vec::new(),
        };
        let report = execute_with(
            &request,
            &data,
            &home,
            None,
            &environment(true, Some(home.clone())),
        )
        .unwrap();
        assert_eq!(report["ready"], false);
        assert_eq!(report["readiness"].as_array().unwrap().len(), items.len());
    }
}
