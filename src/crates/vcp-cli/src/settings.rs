// SPDX-License-Identifier: Apache-2.0
//! Explicit per-user startup data. Project files cannot grant trust or profiles.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
use vcp_domain::{
    policy::{Autonomy, EffectClass, Isolation},
    revision::*,
    Units,
};
use vcp_models::catalog::Snapshot;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub version: u32,
    pub workspace: PathBuf,
    pub trust_workspace: bool,
    #[serde(default)]
    pub sync_roots: Vec<PathBuf>,
    pub maximum_autonomy: Autonomy,
    pub automatic_effects: BTreeSet<EffectClass>,
    pub budget_usd: Option<String>,
    pub provider: Snapshot,
    #[serde(default)]
    pub routing: Option<vcp_lifecycle::foundation::routing::Configuration>,
    #[cfg(windows)]
    #[serde(default)]
    pub decisions: Option<crate::decision::Configuration>,
    #[serde(default)]
    pub skills: Option<crate::skills::Configuration>,
    #[serde(default)]
    pub mcp: Vec<crate::mcp::Server>,
    #[serde(default)]
    pub mcp_http: Vec<crate::mcp::HttpServer>,
    pub catalog: PathBuf,
    pub affected_paths: Vec<PathBuf>,
    pub max_requests: u32,
    #[serde(default)]
    pub output_tokens: Option<Units>,
    #[serde(default)]
    pub provider_timeout_seconds: Option<u32>,
    #[serde(default = "vcp_lifecycle::foundation::default_max_transport_retries")]
    pub max_transport_retries: u32,
    pub deadline_seconds: u32,
    pub processes: Vec<ProcessProfile>,
    /// Executable hooks are explicit trusted owner configuration, never imports.
    #[serde(default)]
    pub hooks: Vec<vcp_extensions::hooks::registry::HookDefinition>,
    /// Optional local observers require an explicit owner profile opt-in.
    #[cfg(windows)]
    #[serde(default)]
    pub observers: Option<vcp_lifecycle::foundation::observers::Configuration>,
    pub checks: Vec<vcp_tools::verification::Requirement>,
    #[cfg(feature = "qualification")]
    pub qualification_endpoint: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessProfile {
    #[serde(default)]
    pub max_timeout_ms: Option<u64>,
    #[serde(default)]
    pub output_encoding: Option<vcp_tools::process::output::Encoding>,
    pub name: String,
    pub executable: PathBuf,
    pub environment: BTreeMap<String, String>,
    pub required_isolation: BTreeSet<Isolation>,
    pub reduced_isolation: bool,
    pub inputs: Vec<String>,
}

pub struct PreparedProfile {
    pub profile: Profile,
    pub raw_catalog: Vec<u8>,
    pub processes: Vec<vcp_tools::process::Profile>,
}

pub fn now() -> Timestamp {
    Timestamp::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64),
    )
}

pub fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|_| "configuration or data file is unavailable")?;
    if !file
        .metadata()
        .map_err(|_| "file metadata unavailable")?
        .is_file()
    {
        return Err("regular file required".into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "file read failed")?;
    if bytes.len() > limit {
        return Err("configuration or data file exceeds size limit".into());
    }
    Ok(bytes)
}

pub fn default_data() -> Result<PathBuf, String> {
    std::env::var_os("LOCALAPPDATA")
        .map(|path| PathBuf::from(path).join("VCP"))
        .ok_or("LOCALAPPDATA or --data-dir is required".into())
}

/// Resolve existing ancestors, including junctions, before writing plaintext.
pub fn local_path(path: &Path, workspace: &Path) -> Result<PathBuf, String> {
    if path
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err("parent traversal is not a local data path".into());
    }
    let absolute = std::path::absolute(path).map_err(|_| "absolute local path required")?;
    let ancestor = absolute
        .ancestors()
        .find(|p| p.exists())
        .ok_or("local path ancestor unavailable")?;
    let resolved = ancestor
        .canonicalize()
        .map_err(|_| "local path cannot be resolved")?;
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        let drive = match resolved.components().next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive,
                _ => {
                    return Err(
                        "local drive required; network and device roots are rejected".into(),
                    )
                }
            },
            _ => return Err("local drive required".into()),
        };
        let root = [u16::from(drive), b':' as u16, b'\\' as u16, 0];
        // SAFETY: the terminated local drive root lives through this query.
        if unsafe { windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(root.as_ptr()) } == 4 {
            return Err("mapped network data roots are rejected".into());
        }
    }
    let mut forbidden = vec![workspace.to_path_buf()];
    for name in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
        if let Some(root) = std::env::var_os(name) {
            if Path::new(&root).exists() {
                forbidden.push(
                    PathBuf::from(root)
                        .canonicalize()
                        .map_err(|_| "sync root cannot be resolved")?,
                );
            }
        }
    }
    if forbidden.iter().any(|root| within(&resolved, root))
        || resolved.ancestors().any(|p| p.join(".git").exists())
    {
        return Err(
            "plaintext configuration/data must be outside repositories and known sync roots".into(),
        );
    }
    let suffix = absolute
        .strip_prefix(ancestor)
        .map_err(|_| "local path resolution failed")?;
    Ok(if suffix.as_os_str().is_empty() {
        resolved
    } else {
        resolved.join(suffix)
    })
}

pub fn within(path: &Path, root: &Path) -> bool {
    let path: Vec<_> = path
        .components()
        .map(|p| p.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    let root: Vec<_> = root
        .components()
        .map(|p| p.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    path.starts_with(&root)
}

pub fn load(path: &Path, workspace: &Path) -> Result<Profile, String> {
    #[cfg(not(windows))]
    let path = local_path(path, workspace)?;
    #[cfg(windows)]
    let (bytes, imported) = crate::config_import::store::read(
        &std::path::absolute(path).map_err(|_| "profile path unavailable")?,
        workspace,
    )?;
    #[cfg(not(windows))]
    let bytes = read_bounded(&path, 256 * 1024)?;
    #[allow(unused_mut)]
    let mut profile: Profile =
        serde_json::from_slice(&bytes).map_err(|_| "invalid user profile or unknown setting")?;
    if profile.version != 1
        || profile
            .workspace
            .canonicalize()
            .map_err(|_| "profile workspace unavailable")?
            != workspace
    {
        return Err("profile version or workspace binding does not match".into());
    }
    #[cfg(windows)]
    if let Some(imported) = imported {
        if imported.base_sha256 != vcp_protocol::digest_bytes(&bytes) {
            return Err("native profile changed since import; review config import preview or rollback-preview before using its imported preferences".into());
        }
        crate::config_import::materialize(&mut profile, &imported.preferences)?;
    }
    Ok(profile)
}

pub fn autonomy(mode: crate::args::Autonomy) -> Autonomy {
    match mode {
        crate::args::Autonomy::Plan => Autonomy::Plan,
        crate::args::Autonomy::Ask => Autonomy::Ask,
        crate::args::Autonomy::Workspace => Autonomy::Workspace,
        crate::args::Autonomy::Autonomous => Autonomy::Autonomous,
    }
}
fn rank(mode: Autonomy) -> u8 {
    match mode {
        Autonomy::Plan => 0,
        Autonomy::Ask => 1,
        Autonomy::Workspace => 2,
        Autonomy::Autonomous => 3,
    }
}

fn startup_output_ceiling(
    selected: Option<Units>,
    provider_maximum: Units,
) -> Result<Units, String> {
    // Larger reasoning budgets require an explicit profile selection; retain
    // the existing default for profiles that omit output_tokens.
    let selected = selected.unwrap_or(Units::new(4096));
    if selected == Units::ZERO || selected.get() > 16384 || provider_maximum == Units::ZERO {
        return Err("startup output tokens must be 1..16384 within provider capacity".into());
    }
    Ok(Units::new(selected.get().min(provider_maximum.get())))
}

fn startup_provider_timeout(
    selected: Option<u32>,
    deadline_seconds: u32,
) -> Result<Duration, String> {
    match selected {
        Some(seconds) if seconds == 0 || seconds > 360 || seconds > deadline_seconds => Err(
            "explicit provider timeout must be 1..360 seconds and not exceed the task deadline"
                .into(),
        ),
        Some(seconds) => Ok(Duration::from_secs(u64::from(seconds))),
        // Preserve legacy profiles, including tasks shorter than 120 seconds;
        // the task deadline remains an independent cancellation boundary.
        None => Ok(Duration::from_secs(120)),
    }
}

#[cfg(test)]
mod request_limit_tests {
    use super::*;

    #[test]
    fn trusted_process_and_check_duration_defaults_and_bounds() {
        let old = serde_json::json!({"name":"check","executable":"C:/fixture/tool.exe","environment":{},"required_isolation":[],"reduced_isolation":true,"inputs":[]});
        let old: ProcessProfile = serde_json::from_value(old).unwrap();
        assert_eq!(old.max_timeout_ms, None);
        assert_eq!(old.output_encoding, None);
        let profile = vcp_tools::process::Profile::new(
            "check".into(),
            std::path::absolute("fixture.exe").unwrap(),
            vcp_tools::process::Mode::Direct,
            BTreeMap::new(),
            BTreeSet::new(),
            true,
        )
        .unwrap();
        let mut check: vcp_tools::verification::Requirement = serde_json::from_value(serde_json::json!({"manifest":"package.json","runner":"node","profile":"check","expected_tests":["acceptance"],"rationale":"Configured check"})).unwrap();
        assert_eq!(check.timeout_ms, None);
        assert!(validate_check_durations(&[check.clone()], &[profile.clone()], 600).is_ok());
        check.timeout_ms = Some(120_001);
        assert!(
            validate_check_durations(&[check.clone()], &[profile.clone()], 600)
                .unwrap_err()
                .contains("profile check")
        );
        let extended = profile.with_max_timeout_ms(180_000).unwrap();
        assert!(validate_check_durations(&[check.clone()], &[extended.clone()], 600).is_ok());
        assert!(
            validate_check_durations(&[check.clone()], &[extended.clone()], 120)
                .unwrap_err()
                .contains("task deadline")
        );
        for invalid in [0, vcp_tools::process::MAX_TIMEOUT_MS + 1, u64::MAX] {
            check.timeout_ms = Some(invalid);
            assert!(validate_check_durations(&[check.clone()], &[extended.clone()], 3600).is_err());
        }
    }

    #[test]
    fn startup_output_is_bounded_and_legacy_default_is_preserved() {
        assert_eq!(
            startup_output_ceiling(None, Units::new(32768)).unwrap(),
            Units::new(4096)
        );
        assert_eq!(
            startup_output_ceiling(None, Units::new(2000)).unwrap(),
            Units::new(2000)
        );
        assert_eq!(
            startup_output_ceiling(Some(Units::new(512)), Units::new(8000)).unwrap(),
            Units::new(512)
        );
        assert_eq!(
            startup_output_ceiling(Some(Units::new(512)), Units::new(256)).unwrap(),
            Units::new(256)
        );
        for output in [1, 4097, 16384] {
            assert_eq!(
                startup_output_ceiling(Some(Units::new(output)), Units::new(32768)).unwrap(),
                Units::new(output)
            );
        }
        assert_eq!(
            startup_output_ceiling(Some(Units::new(16384)), Units::new(8000)).unwrap(),
            Units::new(8000)
        );
        for output in [0, 16385, u64::MAX] {
            assert!(startup_output_ceiling(Some(Units::new(output)), Units::new(32768)).is_err());
        }
        assert!(startup_output_ceiling(None, Units::ZERO).is_err());
        assert!(startup_output_ceiling(Some(Units::new(16384)), Units::ZERO).is_err());
    }

    #[test]
    fn provider_timeout_preserves_default_and_bounds_explicit_selection() {
        for deadline in [60, 900] {
            assert_eq!(
                startup_provider_timeout(None, deadline).unwrap(),
                Duration::from_secs(120)
            );
        }
        for seconds in [1, 60, 120, 180, 360] {
            assert_eq!(
                startup_provider_timeout(Some(seconds), 900).unwrap(),
                Duration::from_secs(u64::from(seconds))
            );
        }
        assert_eq!(
            startup_provider_timeout(Some(60), 60).unwrap(),
            Duration::from_secs(60)
        );
        for seconds in [0, 361, u32::MAX] {
            assert!(startup_provider_timeout(Some(seconds), 3600).is_err());
        }
        assert!(startup_provider_timeout(Some(61), 60).is_err());
        assert!(startup_provider_timeout(Some(180), 179).is_err());
        assert!(startup_provider_timeout(Some(360), 359).is_err());
        assert!(startup_provider_timeout(Some(1), 0).is_err());
    }

    #[test]
    fn profile_deserialization_defaults_and_explicit_zero_retry_are_distinct() {
        // Deserialization-only provider data, never conformance evidence.
        let legacy = serde_json::json!({
            "version":1,"workspace":"fixture","trust_workspace":true,
            "maximum_autonomy":"autonomous","automatic_effects":[],"budget_usd":null,
            "provider":{"id":"fixture","observed_at":"1","valid_until":"2","raw_sha256":"fixture",
                "compatibility":{"id":"fixture","model":"fixture/model","endpoint":"fixture/provider",
                    "qualified_at":"1","valid_until":"2","responses_text_tools":true,
                    "byte_ceiling_qualified":true,"provider_preferences_qualified":true,
                    "deny_data_collection":true,"require_zdr":true,"request_price_limit":"0",
                    "required_parameters":[]},"context":"10000","max_input":"9000","max_output":"8000",
                "price":{"id":"fixture","provider":"fixture/provider","model":"fixture/model","currency":"USD",
                    "capability":"fixture","valid_until":"2","rates":{}}},
            "catalog":"fixture","affected_paths":["file.txt"],"max_requests":1,
            "deadline_seconds":60,"processes":[],"checks":[]
        });
        let old: Profile = serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(old.output_tokens, None);
        assert_eq!(old.output_ceiling().unwrap(), Units::new(4096));
        assert_eq!(old.provider_timeout_seconds, None);
        assert_eq!(old.provider_timeout().unwrap(), Duration::from_secs(120));
        assert_eq!(old.max_transport_retries, 2);
        let mut selected = legacy;
        selected["output_tokens"] = serde_json::json!("512");
        selected["max_transport_retries"] = serde_json::json!(0);
        let configured: Profile = serde_json::from_value(selected.clone()).unwrap();
        assert_eq!(configured.output_ceiling().unwrap(), Units::new(512));
        assert_eq!(configured.max_transport_retries, 0);
        selected["output_tokens"] = serde_json::json!("16384");
        let configured: Profile = serde_json::from_value(selected.clone()).unwrap();
        assert_eq!(configured.output_tokens, Some(Units::new(16384)));
        assert_eq!(configured.output_ceiling().unwrap(), Units::new(8000));
        selected["provider"]["max_output"] = serde_json::json!("32768");
        let configured: Profile = serde_json::from_value(selected.clone()).unwrap();
        assert_eq!(configured.output_ceiling().unwrap(), Units::new(16384));
        assert_eq!(configured.max_transport_retries, 0);
        selected["provider_timeout_seconds"] = serde_json::json!(360);
        let invalid: Profile = serde_json::from_value(selected.clone()).unwrap();
        assert!(invalid.provider_timeout().is_err());
        assert!(
            matches!(invalid.prepare(Autonomy::Autonomous), Err(reason) if reason.contains("explicit provider timeout"))
        );
        selected["deadline_seconds"] = serde_json::json!(900);
        let configured: Profile = serde_json::from_value(selected.clone()).unwrap();
        assert_eq!(configured.provider_timeout_seconds, Some(360));
        assert_eq!(
            configured.provider_timeout().unwrap(),
            Duration::from_secs(360)
        );
        selected["max_transport_retries"] = serde_json::json!(3);
        let invalid: Profile = serde_json::from_value(selected).unwrap();
        assert!(
            matches!(invalid.prepare(Autonomy::Autonomous), Err(reason) if reason == "transport retry ceiling must be 0..2")
        );
    }
}

impl Profile {
    pub fn output_ceiling(&self) -> Result<Units, String> {
        startup_output_ceiling(self.output_tokens, self.provider.max_output)
    }

    pub fn provider_timeout(&self) -> Result<Duration, String> {
        startup_provider_timeout(self.provider_timeout_seconds, self.deadline_seconds)
    }

    pub fn prepare(self, requested: Autonomy) -> Result<PreparedProfile, String> {
        self.output_ceiling()?;
        self.provider_timeout()?;
        if self.max_transport_retries > 2 {
            return Err("transport retry ceiling must be 0..2".into());
        }
        if rank(requested) > rank(self.maximum_autonomy) {
            return Err("requested autonomy exceeds trusted user profile".into());
        }
        if !self.trust_workspace {
            return Err("workspace trust must be explicitly granted in the user profile".into());
        }
        if self.max_requests == 0
            || self.max_requests > 128
            || self.deadline_seconds == 0
            || self.deadline_seconds > 3600
            || (!self.checks.is_empty() && self.deadline_seconds <= 120)
            || self.affected_paths.is_empty()
            || self.affected_paths.len() > 256
            || self.checks.len() > 32
            || self.processes.len() > 32
        {
            return Err("profile resource or acceptance bounds rejected".into());
        }
        for path in &self.affected_paths {
            vcp_repository::path::relative(path).map_err(|e| e.to_string())?;
        }
        for check in &self.checks {
            check.validate().map_err(|e| e.to_string())?;
        }
        let raw_catalog = read_bounded(
            &local_path(
                &self.catalog,
                &self
                    .workspace
                    .canonicalize()
                    .map_err(|_| "workspace unavailable")?,
            )?,
            4 * 1024 * 1024,
        )?;
        self.provider.current(now()).map_err(|e| e.to_string())?;
        let expected = Snapshot::from_endpoints(
            &raw_catalog,
            self.provider.observed_at,
            self.provider.valid_until,
            self.provider.compatibility.clone(),
        )
        .map_err(|e| e.to_string())?;
        if expected != self.provider {
            return Err("provider snapshot does not match captured catalog".into());
        }
        if let Some(routing) = &self.routing {
            routing.validate()?;
        }
        #[cfg(windows)]
        if let Some(decisions) = &self.decisions {
            decisions.validate()?;
        }
        if let Some(skills) = &self.skills {
            skills.validate()?;
        }
        let mut names = BTreeSet::new();
        let mut processes = Vec::new();
        for profile in &self.processes {
            if !names.insert(profile.name.clone()) || !profile.executable.is_file() {
                return Err("duplicate or unavailable executable profile".into());
            }
            processes.push(
                vcp_tools::process::Profile::new(
                    profile.name.clone(),
                    profile.executable.clone(),
                    vcp_tools::process::Mode::Direct,
                    profile.environment.clone(),
                    profile.required_isolation.clone(),
                    profile.reduced_isolation,
                )
                .and_then(|p| p.with_inputs(profile.inputs.clone()))
                .and_then(|p| p.with_output_encoding(profile.output_encoding))
                .and_then(|p| {
                    p.with_max_timeout_ms(
                        profile
                            .max_timeout_ms
                            .unwrap_or(vcp_tools::process::DEFAULT_TIMEOUT_MS),
                    )
                })
                .map_err(|e| e.to_string())?,
            );
        }
        if self
            .checks
            .iter()
            .any(|check| !names.contains(&check.profile))
        {
            return Err("verification requires an explicit executable profile".into());
        }
        if self.hooks.len() > 128 {
            return Err("hook registry ceiling exceeded".into());
        }
        for hook in &self.hooks {
            hook.validate().map_err(|e| e.to_string())?;
            if !names.contains(&hook.command.profile) {
                return Err("hook requires an explicit executable profile".into());
            }
        }
        validate_check_durations(&self.checks, &processes, self.deadline_seconds)?;
        crate::mcp::validate(&self.mcp, &names)?;
        crate::mcp::validate_http(&self.mcp_http, &self.mcp)?;
        Ok(PreparedProfile {
            profile: self,
            raw_catalog,
            processes,
        })
    }
}

fn validate_check_durations(
    checks: &[vcp_tools::verification::Requirement],
    processes: &[vcp_tools::process::Profile],
    deadline_seconds: u32,
) -> Result<(), String> {
    for check in checks {
        check
            .validate()
            .map_err(|e| format!("check {}: {e}", check.manifest))?;
        let profile = processes
            .iter()
            .find(|p| p.name() == check.profile)
            .ok_or("verification process profile missing")?;
        let requested = check
            .timeout_ms
            .unwrap_or(vcp_tools::process::DEFAULT_TIMEOUT_MS);
        if requested > profile.max_timeout_ms() {
            return Err(format!(
                "check {} duration exceeds profile {} ceiling",
                check.manifest, check.profile
            ));
        }
        if requested > u64::from(deadline_seconds) * 1000 {
            return Err(format!(
                "check {} duration exceeds configured task deadline",
                check.manifest
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceEntry {
    pub version: u32,
    #[serde(default)]
    pub rebind_pending: bool,
    pub config: vcp_lifecycle::foundation::Config,
    #[serde(default)]
    pub identity: Option<crate::binding::WorkspaceIdentity>,
}

/// Descriptors are local hints. The caller still validates canonical identity
/// and obtains the store lock before any operation that can change state.
pub fn workspace_directory(data: &Path, workspace: &Path) -> Result<Option<PathBuf>, String> {
    if !data.exists() {
        return Ok(None);
    }
    let root = registry_root(data)?;
    let base = data.join("workspaces");
    let _base_pin = match root.hold(Some(Path::new("workspaces")), true) {
        Ok(pin) => pin,
        Err(vcp_repository::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None)
        }
        Err(_) => return Err("workspace registry is unavailable or redirected".into()),
    };
    let workspace_root = registry_root(workspace)?;
    let workspace_pin = workspace_root
        .hold(None, true)
        .map_err(|_| "workspace root changed during discovery")?;
    let mut found = None;
    let mut moved = None;
    for (index, item) in fs::read_dir(&base).map_err(|e| e.to_string())?.enumerate() {
        if index >= 4096 {
            return Err("workspace registry exceeds discovery limit".into());
        }
        let item = item.map_err(|e| e.to_string())?;
        let kind = item.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err("workspace registry directory is redirected".into());
        }
        if !kind.is_dir() {
            continue;
        }
        let relative = Path::new("workspaces").join(item.file_name());
        let _directory_pin = root
            .hold(Some(&relative), true)
            .map_err(|_| "workspace registry directory is unavailable or redirected")?;
        let source = match root.read(&relative.join("workspace.json"), 256 * 1024) {
            Ok(source) => source,
            Err(vcp_repository::Error::Io(error))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                continue
            }
            Err(_) => return Err("workspace descriptor is unavailable or redirected".into()),
        };
        let entry: WorkspaceEntry = serde_json::from_slice(&source.bytes)
            .map_err(|_| "invalid workspace registry descriptor")?;
        if Path::new(&entry.config.binding.root) == workspace {
            if found.is_some() {
                return Err("ambiguous workspace binding; reconcile the registry".into());
            }
            found = Some(data.join(relative));
        } else if entry
            .identity
            .as_ref()
            .is_some_and(|identity| identity.directory_identity == workspace_pin.native_identity)
        {
            moved = Some(entry.config.workspace);
        }
    }
    if found.is_none() {
        if let Some(id) = moved {
            return Err(format!("workspace root moved; run vcp rebind {id} at this root to reconcile its retained history"));
        }
    }
    Ok(found)
}

pub(crate) fn registry_root(data: &Path) -> Result<vcp_repository::Root, String> {
    vcp_repository::Root::open(
        vcp_repository::RootIdentity {
            workspace: vcp_domain::WorkspaceId::new(),
            root: vcp_domain::RootId::new(),
            repository: "local-workspace-registry".into(),
            worktree: "local-workspace-registry".into(),
            binding: Revision::ZERO,
        },
        data,
    )
    .map_err(|_| "local workspace registry root is unavailable or redirected".into())
}

/// Recheck no-follow containment when consuming a descriptor after discovery.
/// The directory chosen by an earlier scan does not authorize redirected reads.
pub fn read_workspace_descriptor(data: &Path, path: &Path) -> Result<Vec<u8>, String> {
    let relative = path
        .strip_prefix(data)
        .map_err(|_| "workspace descriptor is outside the local data root")?;
    let root = registry_root(data)?;
    root.read(relative, 256 * 1024)
        .map(|source| source.bytes)
        .map_err(|_| "workspace descriptor is unavailable or redirected".into())
}

#[cfg(all(test, windows))]
mod registry_tests {
    use super::*;

    #[test]
    fn discovery_and_second_read_reject_junctions_without_touching_target() {
        let temporary = tempfile::tempdir().unwrap();
        let base = temporary.path().canonicalize().unwrap();
        let data = base.join("data");
        let row = data.join("workspaces/row");
        let outside = base.join("outside");
        let workspace = base.join("workspace");
        fs::create_dir_all(&row).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::create_dir(&workspace).unwrap();
        let descriptor = row.join("workspace.json");
        fs::write(&descriptor, b"retained descriptor bytes").unwrap();
        assert_eq!(
            read_workspace_descriptor(&data, &descriptor).unwrap(),
            b"retained descriptor bytes"
        );
        fs::write(outside.join("workspace.json"), b"outside marker").unwrap();
        assert!(read_workspace_descriptor(&data, &outside.join("workspace.json")).is_err());
        fs::rename(&row, data.join("preserved")).unwrap();
        let status = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&row)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "junction fixture prerequisite failed"
        );
        // The path was valid at first read. Both discovery and subsequent
        // consumption must now reject the newly redirected registry row.
        assert!(read_workspace_descriptor(&data, &descriptor).is_err());
        assert!(workspace_directory(&data, &workspace).is_err());
        assert_eq!(
            fs::read(outside.join("workspace.json")).unwrap(),
            b"outside marker"
        );
        fs::remove_dir(&row).unwrap();
    }
}

pub fn save(path: &Path, entry: &WorkspaceEntry) -> Result<(), String> {
    // The caller holds the canonical store owner lock; never replace another
    // process's active entry. Write and sync before publishing the new hint.
    let temporary = path.with_extension("new");
    let bytes = serde_json::to_vec(entry).map_err(|e| e.to_string())?;
    use std::io::Write;
    // A stale temporary is never truncated: it may be a redirected path.
    let temporary = temporary.with_extension(vcp_domain::ids::CommandId::new().as_str());
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let source: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
        let target: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: both UTF-16 paths are terminated and remain alive for the call.
        if unsafe {
            MoveFileExW(
                source.as_ptr(),
                target.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err("workspace descriptor publication failed".into());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    fs::rename(temporary, path).map_err(|e| e.to_string())
}
