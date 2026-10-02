// SPDX-License-Identifier: Apache-2.0
//! Explicit first-run setup. No fixture credentials, implicit trust or defaults
//! that authorize a model call. Profile publication is create-only and offline.
use crate::{profile_selection, settings};
use clap::{Args, Subcommand};
use serde_json::{json, Value};
use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};
use vcp_models::catalog::Snapshot;

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Make at most two accounted provider calls and validate generation receipts.
    Provider(crate::provider_setup::production::Provider),
    /// Retrieve delayed generation receipts for a settled probe, without inference.
    ProviderComplete {
        #[arg(long)]
        directory: PathBuf,
    },
    /// Create and select an offline workspace profile from current qualified metadata.
    Profile(Profile),
    /// Select the --config profile for this workspace, offline.
    Select,
    /// Validate the selected profile, tools, budgets and expiry without inference.
    Check,
}

#[derive(Debug, Args)]
pub struct Profile {
    /// Snapshot produced by a successful `vcp setup provider` command.
    #[arg(long)]
    pub snapshot: PathBuf,
    /// Exact captured endpoint catalog matching the snapshot.
    #[arg(long)]
    pub catalog: PathBuf,
    /// New profile path outside workspaces, repositories and known sync roots;
    /// defaults to a new file under the private data folder's `profiles`.
    #[arg(long)]
    pub output: Option<PathBuf>,
    /// Explicitly grant trust to the selected --workspace root.
    #[arg(long, required = true)]
    pub trust_workspace: bool,
    /// Per-task budget; this command itself performs no inference.
    #[arg(long)]
    pub budget_usd: String,
    /// Maximum policy mode. Reads are the only automatically permitted effects.
    #[arg(long, value_enum)]
    pub autonomy: crate::args::Autonomy,
    /// Expected task paths, relative to the selected workspace; repeat as needed.
    #[arg(long, required = true)]
    pub affected_path: Vec<PathBuf>,
}

pub async fn execute(
    command: &Command,
    workspace: &Path,
    config: Option<&Path>,
    data_dir: Option<&Path>,
) -> Result<Value, String> {
    let workspace = workspace
        .canonicalize()
        .map_err(|_| "setup requires an existing accessible workspace directory")?;
    if !workspace.is_dir() {
        return Err("setup workspace must be a directory".into());
    }
    let data = || {
        data_dir
            .map(Path::to_path_buf)
            .map(Ok)
            .unwrap_or_else(settings::default_data)
    };
    match command {
        Command::Provider(request) => {
            crate::provider_setup::production::run(request, &workspace).await
        }
        Command::ProviderComplete { directory } => {
            crate::provider_setup::production::complete(directory, &workspace).await
        }
        Command::Profile(request) => {
            let data = data()?;
            let output = create(request, &workspace, &data)?;
            let selection =
                profile_selection::select(&data, &workspace, &output, None, settings::now());
            let next = match &selection {
                Ok(_) => "vcp --workspace <workspace> setup check".to_owned(),
                Err(_) => "vcp --workspace <workspace> --config <profile> setup select".to_owned(),
            };
            Ok(
                json!({"status":"created","profile":output,"workspace":workspace,"model_calls":0,
                "selected":selection.is_ok(),"previous":selection.as_ref().ok().cloned().flatten(),
                "selection_error":selection.err(),"next":next,
                "checks":"source integrity only; register explicit process profiles and checks for executable verification"}),
            )
        }
        Command::Select => {
            let path = config.ok_or("setup select requires --config <profile>")?;
            let previous =
                profile_selection::select(&data()?, &workspace, path, None, settings::now())?;
            Ok(json!({"status":"selected","workspace":workspace,
                "profile":std::path::absolute(path).map_err(|_| "absolute profile path required")?,
                "previous":previous,"model_calls":0,"next":"vcp --workspace <workspace> setup check"}))
        }
        Command::Check => {
            let resolved = profile_selection::resolve(&data()?, &workspace, config)?;
            let mut value = check(&resolved.path, &workspace, credential_present())?;
            value["profile_source"] = json!(resolved.source);
            Ok(value)
        }
    }
}

/// Whether the provider key is present; its value is never read here.
pub fn credential_present() -> bool {
    std::env::var_os("OPENROUTER_API_KEY").is_some_and(|key| !key.is_empty())
}

/// Offline profile validation shared by `setup check` and `doctor`.
pub fn check(path: &Path, workspace: &Path, credential_present: bool) -> Result<Value, String> {
    let profile = settings::load(path, workspace).map_err(|e| {
        format!(
            "{e}; select the matching --config for this workspace or run vcp setup profile --help"
        )
    })?;
    let cap = profile
        .budget_usd
        .as_deref()
        .ok_or("profile needs an explicit budget_usd")?;
    crate::args::parse_usd(cap)?;
    let prepared = profile.prepare(vcp_domain::policy::Autonomy::Plan)
        .map_err(|e| format!("{e}; renew expired provider metadata with vcp setup provider --help and create a new profile; review unavailable process/check configuration"))?;
    Ok(
        json!({"status":"ready","workspace":workspace,"profile":path,
        "valid_until":prepared.profile.provider.valid_until,
        "canonical_tools":prepared.profile.canonical_tools,"checks":prepared.profile.checks.len(),
        "processes":prepared.processes.len(),"credential_required":"OPENROUTER_API_KEY in process environment",
        "credential_present":credential_present,
        "model_calls":0,"trust_granted":prepared.profile.trust_workspace}),
    )
}

/// A new profile filename under `<data>\profiles`, named after the workspace.
fn default_output(data: &Path, workspace: &Path) -> Result<PathBuf, String> {
    let leaf: String = workspace
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(40)
        .collect();
    let leaf = if leaf.trim_matches('-').is_empty() {
        "workspace".into()
    } else {
        leaf
    };
    let directory = settings::local_path(data, workspace)?.join("profiles");
    std::fs::create_dir_all(&directory)
        .map_err(|_| "profiles folder cannot be created in the private data folder")?;
    Ok(directory.join(format!(
        "{leaf}-{}.json",
        vcp_domain::ids::CommandId::new().as_str()
    )))
}

fn create(request: &Profile, workspace: &Path, data: &Path) -> Result<PathBuf, String> {
    if !request.trust_workspace {
        return Err("explicit --trust-workspace is required".into());
    }
    if crate::args::parse_usd(&request.budget_usd)? == vcp_domain::Micros::ZERO {
        return Err("profile budget must be greater than zero".into());
    }
    let snapshot_path = settings::local_path(&request.snapshot, workspace)?;
    let catalog = settings::local_path(&request.catalog, workspace)?;
    let snapshot: Snapshot =
        serde_json::from_slice(&settings::read_bounded(&snapshot_path, 256 * 1024)?)
            .map_err(|_| "invalid qualified snapshot; run vcp setup provider --help")?;
    let explicit = request
        .output
        .as_ref()
        .map(|output| settings::local_path(output, workspace))
        .transpose()?;
    let value = json!({"version":1,"workspace":workspace,"trust_workspace":true,
        "maximum_autonomy":settings::autonomy(request.autonomy),"automatic_effects":["read"],
        "budget_usd":request.budget_usd,"provider":snapshot,"catalog":catalog,
        "affected_paths":request.affected_path,
        "canonical_tools":["vcp_read","vcp_list","vcp_search","vcp_patch","vcp_verify"],
        "max_requests":8,"output_tokens":"2048","provider_timeout_seconds":120,
        "max_transport_retries":0,"deadline_seconds":300,"processes":[],"checks":[]});
    let profile: settings::Profile =
        serde_json::from_value(value.clone()).map_err(|_| "profile settings rejected")?;
    profile
        .prepare(vcp_domain::policy::Autonomy::Plan)
        .map_err(|e| {
            format!("{e}; require a fresh matching catalog/snapshot from vcp setup provider")
        })?;
    let output = match explicit {
        Some(output) => output,
        None => default_output(data, workspace)?,
    };
    let parent = output.parent().ok_or("profile output parent required")?;
    let root = settings::registry_root(parent)?;
    let _pin = root
        .hold(None, true)
        .map_err(|_| "profile parent is unavailable or redirected")?;
    let bytes = serde_json::to_vec_pretty(&value).map_err(|_| "profile serialization failed")?;
    let mut file = OpenOptions::new().create_new(true).write(true).open(&output)
        .map_err(|_| "profile output must not exist; use a new generation filename to preserve prior settings")?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| "profile publication failed")?;
    Ok(output)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use clap::Parser;
    use vcp_domain::Timestamp;
    use vcp_models::catalog::Compatibility;
    #[test]
    fn cli_requires_explicit_trust_budget_and_never_accepts_a_credential_argument() {
        let args = [
            "vcp",
            "setup",
            "profile",
            "--snapshot",
            "snapshot.json",
            "--catalog",
            "catalog.json",
            "--output",
            "profile.json",
            "--budget-usd",
            "1",
            "--autonomy",
            "ask",
            "--affected-path",
            "README.md",
        ];
        assert!(crate::args::Cli::try_parse_from(args).is_err());
        assert!(
            crate::args::Cli::try_parse_from(args.into_iter().chain(["--trust-workspace"])).is_ok()
        );
        assert!(crate::args::Cli::try_parse_from([
            "vcp",
            "setup",
            "provider",
            "--model",
            "fixture/model",
            "--endpoint",
            "fixture",
            "--request-price-limit",
            "0.001",
            "--budget-usd",
            "1",
            "--output",
            "private",
            "--api-key",
            "secret"
        ])
        .is_err());
    }
    #[test]
    fn offline_creation_preserves_user_files_and_checks_expiry_catalog_and_trust() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let catalog = temp.path().join("catalog.json");
        let snapshot_path = temp.path().join("snapshot.json");
        let raw=serde_json::to_vec(&json!({"data":{"id":"fixture/model","endpoints":[{"tag":"fixture","status":0,"context_length":32000,"max_completion_tokens":8000,"supported_parameters":["tools"],"pricing":{"prompt":"0.000001","completion":"0.000002"}}]}})).unwrap();
        let observed = settings::now();
        let snapshot = Snapshot::from_endpoints(
            &raw,
            observed,
            Timestamp::new(observed.get() + 60000),
            Compatibility {
                id: "offline-test-only".into(),
                model: "fixture/model".into(),
                endpoint: "fixture".into(),
                qualified_at: observed,
                valid_until: Timestamp::new(observed.get() + 60000),
                responses_text_tools: true,
                byte_ceiling_qualified: false,
                provider_preferences_qualified: true,
                qualified_reasoning_efforts: Default::default(),
                deny_data_collection: true,
                require_zdr: false,
                request_price_limit: "0.001".into(),
                required_parameters: Default::default(),
            },
        )
        .unwrap();
        std::fs::write(&catalog, &raw).unwrap();
        std::fs::write(&snapshot_path, serde_json::to_vec(&snapshot).unwrap()).unwrap();
        let data = temp.path().join("data");
        let output = temp.path().join("profile.json");
        let mut request = Profile {
            snapshot: snapshot_path.clone(),
            catalog: catalog.clone(),
            output: Some(output.clone()),
            trust_workspace: true,
            budget_usd: "1".into(),
            autonomy: crate::args::Autonomy::Ask,
            affected_path: vec![PathBuf::from("README.md")],
        };
        create(&request, &workspace, &data).unwrap();
        let before = std::fs::read(&output).unwrap();
        assert!(create(&request, &workspace, &data).is_err());
        assert_eq!(before, std::fs::read(&output).unwrap());
        let loaded = settings::load(&output, &workspace).unwrap();
        assert!(loaded.prepare(vcp_domain::policy::Autonomy::Ask).is_ok());
        assert!(
            !data.exists(),
            "an explicit output never touches the data folder"
        );

        // Without --output the profile is a new file under <data>\profiles,
        // and selecting it lets later commands omit --config.
        request.output = None;
        let first = create(&request, &workspace, &data).unwrap();
        let second = create(&request, &workspace, &data).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            first.parent(),
            Some(data.join("profiles").canonicalize().unwrap().as_path())
        );
        assert!(first
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("workspace-"));
        assert_eq!(
            profile_selection::resolve(&data, &workspace, None),
            Err(profile_selection::NO_PROFILE.to_owned())
        );
        let now = settings::now();
        assert_eq!(
            profile_selection::select(&data, &workspace, &first, None, now),
            Ok(None)
        );
        let resolved = profile_selection::resolve(&data, &workspace, None).unwrap();
        assert_eq!(
            (resolved.path, resolved.source),
            (first.clone(), profile_selection::Source::Selected)
        );
        assert!(check(&first, &workspace, false).is_ok());
        assert_eq!(
            profile_selection::select(&data, &workspace, &second, Some("quick"), now),
            Ok(Some(first.clone()))
        );
        let selection = profile_selection::read(&data, &workspace).unwrap().unwrap();
        assert_eq!(selection.active_set.as_deref(), Some("quick"));
        assert_eq!(selection.sets.get("quick"), Some(&second));
        assert_eq!(selection.previous, Some(first.clone()));
        assert_eq!(
            profile_selection::resolve(&data, &workspace, Some(&output))
                .unwrap()
                .source,
            profile_selection::Source::Explicit
        );
        // A profile bound to another workspace cannot be selected here.
        let other = temp.path().join("other");
        std::fs::create_dir(&other).unwrap();
        let other = other.canonicalize().unwrap();
        assert!(profile_selection::select(&data, &other, &first, None, now).is_err());
        assert!(profile_selection::read(&data, &other).unwrap().is_none());

        request.output = Some(temp.path().join("next.json"));
        request.trust_workspace = false;
        assert!(create(&request, &workspace, &data).is_err());
        request.trust_workspace = true;
        std::fs::write(&catalog, b"{}").unwrap();
        assert!(create(&request, &workspace, &data).is_err());
        std::fs::write(&catalog, raw).unwrap();
        let mut expired = snapshot;
        expired.valid_until = Timestamp::new(observed.get() - 1);
        std::fs::write(snapshot_path, serde_json::to_vec(&expired).unwrap()).unwrap();
        assert!(create(&request, &workspace, &data).is_err());
        assert!(!temp.path().join("next.json").exists());
    }

    #[test]
    fn selection_pointer_rejects_tampering_and_redirection() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let workspace = base.join("workspace");
        let data = base.join("data");
        let outside = base.join("outside");
        for path in [&workspace, &data, &outside] {
            std::fs::create_dir(path).unwrap();
        }
        let key = settings::workspace_path_key(&workspace);
        let selected = data.join("profiles").join("selected");
        std::fs::create_dir_all(&selected).unwrap();
        let pointer = selected.join(format!("{key}.json"));
        let other_workspace = json!({"schema":profile_selection::SCHEMA,"workspace":outside,
            "profile":base.join("p.json"),"active_set":null,"sets":{},"previous":null,"selected_at":"1"});
        std::fs::write(&pointer, other_workspace.to_string()).unwrap();
        assert!(profile_selection::read(&data, &workspace).is_err());
        let mut unknown = other_workspace.clone();
        unknown["workspace"] = json!(workspace);
        unknown["grants"] = json!(["write"]);
        std::fs::write(&pointer, unknown.to_string()).unwrap();
        assert!(profile_selection::read(&data, &workspace).is_err());
        let mut mismatched_set = other_workspace;
        mismatched_set["workspace"] = json!(workspace);
        mismatched_set["active_set"] = json!("quick");
        std::fs::write(&pointer, mismatched_set.to_string()).unwrap();
        assert!(profile_selection::read(&data, &workspace).is_err());
        std::fs::remove_file(&pointer).unwrap();
        assert_eq!(profile_selection::read(&data, &workspace), Ok(None));

        std::fs::remove_dir(&selected).unwrap();
        std::fs::write(outside.join(format!("{key}.json")), b"outside marker").unwrap();
        let junction = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&selected)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(
            junction.status.success(),
            "junction fixture prerequisite failed"
        );
        assert!(profile_selection::read(&data, &workspace).is_err());
        assert!(profile_selection::resolve(&data, &workspace, None).is_err());
        assert_eq!(
            std::fs::read(outside.join(format!("{key}.json"))).unwrap(),
            b"outside marker"
        );
        std::fs::remove_dir(&selected).unwrap();
    }
}
