// SPDX-License-Identifier: Apache-2.0
//! Explicit per-user startup data. Project files cannot grant trust or profiles.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use vcp_domain::{
    policy::{Autonomy, EffectClass, Isolation},
    revision::*,
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
    pub catalog: PathBuf,
    pub affected_paths: Vec<PathBuf>,
    pub max_requests: u32,
    pub deadline_seconds: u32,
    pub processes: Vec<ProcessProfile>,
    pub checks: Vec<vcp_tools::verification::Requirement>,
    #[cfg(feature = "qualification")]
    pub qualification_endpoint: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessProfile {
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
    let path = local_path(path, workspace)?;
    let profile: Profile = serde_json::from_slice(&read_bounded(&path, 256 * 1024)?)
        .map_err(|_| "invalid user profile or unknown setting")?;
    if profile.version != 1
        || profile
            .workspace
            .canonicalize()
            .map_err(|_| "profile workspace unavailable")?
            != workspace
    {
        return Err("profile version or workspace binding does not match".into());
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

impl Profile {
    pub fn prepare(self, requested: Autonomy) -> Result<PreparedProfile, String> {
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
        Ok(PreparedProfile {
            profile: self,
            raw_catalog,
            processes,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceEntry {
    pub version: u32,
    pub config: vcp_lifecycle::foundation::Config,
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
