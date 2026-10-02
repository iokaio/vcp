// SPDX-License-Identifier: Apache-2.0
//! Explicit first-run setup. No fixture credentials, implicit trust or defaults
//! that authorize a model call. Profile publication is create-only and offline.
use crate::settings;
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
    /// Create an offline workspace profile from current qualified metadata.
    Profile(Profile),
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
    /// New profile path outside workspaces, repositories and known sync roots.
    #[arg(long)]
    pub output: PathBuf,
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
) -> Result<Value, String> {
    let workspace = workspace
        .canonicalize()
        .map_err(|_| "setup requires an existing accessible workspace directory")?;
    if !workspace.is_dir() {
        return Err("setup workspace must be a directory".into());
    }
    match command {
        Command::Provider(request) => {
            crate::provider_setup::production::run(request, &workspace).await
        }
        Command::ProviderComplete { directory } => {
            crate::provider_setup::production::complete(directory, &workspace).await
        }
        Command::Profile(request) => create(request, &workspace),
        Command::Check => {
            let path = config.ok_or("setup check requires an explicit --config profile path; use setup profile for first-run creation")?;
            check(path, &workspace, credential_present())
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

fn create(request: &Profile, workspace: &Path) -> Result<Value, String> {
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
    let output = settings::local_path(&request.output, workspace)?;
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
    Ok(
        json!({"status":"created","profile":output,"workspace":workspace,"model_calls":0,
        "next":"vcp --workspace <workspace> --config <profile> setup check",
        "checks":"source integrity only; register explicit process profiles and checks for executable verification"}),
    )
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
        let mut request = Profile {
            snapshot: snapshot_path.clone(),
            catalog: catalog.clone(),
            output: temp.path().join("profile.json"),
            trust_workspace: true,
            budget_usd: "1".into(),
            autonomy: crate::args::Autonomy::Ask,
            affected_path: vec![PathBuf::from("README.md")],
        };
        create(&request, &workspace).unwrap();
        let before = std::fs::read(&request.output).unwrap();
        assert!(create(&request, &workspace).is_err());
        assert_eq!(before, std::fs::read(&request.output).unwrap());
        let loaded = settings::load(&request.output, &workspace).unwrap();
        assert!(loaded.prepare(vcp_domain::policy::Autonomy::Ask).is_ok());
        request.output = temp.path().join("next.json");
        request.trust_workspace = false;
        assert!(create(&request, &workspace).is_err());
        request.trust_workspace = true;
        std::fs::write(&catalog, b"{}").unwrap();
        assert!(create(&request, &workspace).is_err());
        std::fs::write(&catalog, raw).unwrap();
        let mut expired = snapshot;
        expired.valid_until = Timestamp::new(observed.get() - 1);
        std::fs::write(snapshot_path, serde_json::to_vec(&expired).unwrap()).unwrap();
        assert!(create(&request, &workspace).is_err());
        assert!(!request.output.exists());
    }
}
