// SPDX-License-Identifier: Apache-2.0
//! Per-workspace profile selection (ADR-077). The pointer lives in the private
//! data root and only names an absolute profile path. It grants nothing:
//! loading still enforces the profile's workspace binding, trust and expiry.
use crate::settings;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use vcp_domain::Timestamp;

pub const SCHEMA: &str = "vcp-workspace-profile-selection/1";
const LIMIT: u64 = 64 * 1024;
const MAX_SETS: usize = 32;
pub const NO_PROFILE: &str =
    "no profile is selected for this workspace; run `vcp setup`, or pass --config <profile>";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub schema: String,
    pub workspace: PathBuf,
    /// Active profile for this workspace.
    pub profile: PathBuf,
    /// Model set of the active profile, when it was prepared from one.
    pub active_set: Option<String>,
    /// Prepared profile per model set, for switching without re-entry.
    pub sets: BTreeMap<String, PathBuf>,
    pub previous: Option<PathBuf>,
    pub selected_at: Timestamp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Explicit,
    Selected,
    Legacy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub path: PathBuf,
    pub source: Source,
}

fn relative(workspace: &Path) -> PathBuf {
    Path::new("profiles")
        .join("selected")
        .join(format!("{}.json", settings::workspace_path_key(workspace)))
}

fn same(left: &Path, right: &Path) -> bool {
    settings::within(left, right) && settings::within(right, left)
}

fn valid_set(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn validate(selection: &Selection, workspace: &Path) -> Result<(), String> {
    if selection.schema != SCHEMA
        || !same(&selection.workspace, workspace)
        || !selection.profile.is_absolute()
        || selection
            .previous
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
        || selection.sets.len() > MAX_SETS
        || selection
            .sets
            .iter()
            .any(|(id, path)| !valid_set(id) || !path.is_absolute())
        || selection
            .active_set
            .as_ref()
            .is_some_and(|id| selection.sets.get(id) != Some(&selection.profile))
    {
        return Err("selected profile pointer is invalid for this workspace; run `vcp setup select` with the intended --config".into());
    }
    Ok(())
}

/// The current selection for a canonical workspace, if one was recorded.
pub fn read(data: &Path, workspace: &Path) -> Result<Option<Selection>, String> {
    if !data.exists() {
        return Ok(None);
    }
    let root = settings::registry_root(data)?;
    let source = match root.read(&relative(workspace), LIMIT) {
        Ok(source) => source,
        Err(vcp_repository::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None)
        }
        Err(_) => return Err("selected profile pointer is unavailable or redirected".into()),
    };
    let selection: Selection = serde_json::from_slice(&source.bytes)
        .map_err(|_| "selected profile pointer is malformed")?;
    validate(&selection, workspace)?;
    Ok(Some(selection))
}

/// An explicit `--config` wins, then this workspace's selection, then the
/// legacy `<data>\profile.json` when present.
pub fn resolve(data: &Path, workspace: &Path, explicit: Option<&Path>) -> Result<Resolved, String> {
    if let Some(path) = explicit {
        return Ok(Resolved {
            path: path.to_path_buf(),
            source: Source::Explicit,
        });
    }
    if let Some(selection) = read(data, workspace)? {
        return Ok(Resolved {
            path: selection.profile,
            source: Source::Selected,
        });
    }
    let legacy = data.join("profile.json");
    if legacy.is_file() {
        return Ok(Resolved {
            path: legacy,
            source: Source::Legacy,
        });
    }
    Err(NO_PROFILE.into())
}

/// Record `profile` as this workspace's selection after it loads for the
/// workspace. Returns the previously selected profile, if any.
pub fn select(
    data: &Path,
    workspace: &Path,
    profile: &Path,
    set: Option<&str>,
    now: Timestamp,
) -> Result<Option<PathBuf>, String> {
    if set.is_some_and(|id| !valid_set(id)) {
        return Err("model set identifiers use lowercase letters, digits and hyphens".into());
    }
    let profile = std::path::absolute(profile).map_err(|_| "absolute profile path required")?;
    settings::load(&profile, workspace).map_err(|error| {
        format!("{error}; only a profile created for this workspace can be selected")
    })?;
    let data = settings::local_path(data, workspace)?;
    let relative = relative(workspace);
    let directory = relative.parent().ok_or("selection directory unavailable")?;
    std::fs::create_dir_all(data.join(directory))
        .map_err(|_| "selection directory cannot be created in the private data folder")?;
    let root = settings::registry_root(&data)?;
    let _pin = root
        .hold(Some(directory), true)
        .map_err(|_| "selection directory is unavailable or redirected")?;
    let existing = read(&data, workspace)?;
    let previous = existing.as_ref().map(|selection| selection.profile.clone());
    let mut sets = existing.map(|selection| selection.sets).unwrap_or_default();
    if let Some(id) = set {
        sets.insert(id.to_owned(), profile.clone());
        if sets.len() > MAX_SETS {
            return Err("too many prepared model sets for this workspace".into());
        }
    }
    let selection = Selection {
        schema: SCHEMA.into(),
        workspace: workspace.to_path_buf(),
        profile,
        active_set: set.map(str::to_owned),
        sets,
        previous: previous.clone(),
        selected_at: now,
    };
    validate(&selection, workspace)?;
    let bytes = serde_json::to_vec_pretty(&selection).map_err(|e| e.to_string())?;
    settings::replace_file(&data.join(relative), &bytes)
        .map_err(|_| "selected profile pointer publication failed")?;
    Ok(previous)
}
