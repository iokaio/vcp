// SPDX-License-Identifier: Apache-2.0
//! Owner preferences are account data, never project-file authority. Selecting a
//! set records a preference; fresh provider metadata and admission gate each call.
use crate::settings;
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
use vcp_domain::{
    accounting::{Money, RequestRole, Usage},
    Micros, Units,
};
use vcp_models::{catalog::Snapshot, routing::*};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModelSet {
    pub id: String,
    pub label: String,
    pub maker: String,
    pub project_type: String,
    /// Selection rationale, not measured quality or permission to use tools.
    #[serde(default)]
    pub rationale: String,
    /// Primary followed by eligible alternatives. No role grants tools.
    pub roles: BTreeMap<String, Vec<String>>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub version: u32,
    pub set: ModelSet,
    pub budget_usd: String,
    /// Explicit request rotation opt-in. Empty preserves legacy ordered fallback.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub choice_sets: BTreeMap<String, Vec<ChoicePreferences>>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChoicePreferences {
    pub models: Vec<String>,
    /// Optional owner narrowing. Omitted model entries approve compatible
    /// endpoints captured at task creation; task retention is always exact.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub endpoints: BTreeMap<String, Vec<String>>,
    /// USD ceiling for the same 8k-input/1k-output reference request. Normal
    /// choices default to twice the first member's cheapest eligible quote.
    /// A third reserve choice always requires an explicitly selected ceiling.
    #[serde(default)]
    pub max_reference_request_cost_usd: Option<String>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            set: balanced(),
            budget_usd: "10".into(),
            choice_sets: BTreeMap::new(),
        }
    }
}
fn roles(
    main: &str,
    helper: &str,
    reviewer: &str,
    fallback: &str,
) -> BTreeMap<String, Vec<String>> {
    [
        "main",
        "child",
        "helper",
        "compaction",
        "reviewer",
        "verification",
        "optimizer",
        "memory",
    ]
    .into_iter()
    .map(|role| {
        let primary = match role {
            "main" | "child" => main,
            "reviewer" | "verification" => reviewer,
            _ => helper,
        };
        let mut selected = vec![primary.to_owned()];
        if fallback != primary {
            selected.push(fallback.to_owned());
        }
        (role.into(), selected)
    })
    .collect()
}
pub fn balanced() -> ModelSet {
    ModelSet {
        id: "balanced".into(),
        label: "Balanced quality and cost".into(),
        maker: "mixed".into(),
        project_type: "general".into(),
        rationale: "GPT-4.1 Mini handles everyday implementation and support at lower cost; GPT-4.1 handles review and verification for its instruction-following and precise code-diff strengths. Qwen3 Coder is the coding-focused alternative within this set.".into(),
        roles: roles(
            "openai/gpt-4.1-mini",
            "openai/gpt-4.1-mini",
            "openai/gpt-4.1",
            "qwen/qwen3-coder",
        ),
    }
}
/// Suggestions verified in OpenRouter's public model catalog on 2026-10-02.
/// These labels make no comparative-quality or live-conformance claim.
pub fn builtin_sets() -> Vec<ModelSet> {
    vec![
        balanced(),
        ModelSet {
            id: "openai".into(),
            label: "OpenAI coding".into(),
            maker: "openai".into(),
            project_type: "general".into(),
            rationale: "Keep every role with OpenAI: GPT-4.1 Mini for everyday coding and support, GPT-4.1 for detailed review and verification. Mini is the lower-cost alternative when eligible.".into(),
            roles: roles(
                "openai/gpt-4.1-mini",
                "openai/gpt-4.1-mini",
                "openai/gpt-4.1",
                "openai/gpt-4.1-mini",
            ),
        },
        ModelSet {
            id: "anthropic".into(),
            label: "Anthropic coding".into(),
            maker: "anthropic".into(),
            project_type: "general".into(),
            rationale: "Keep every role with Anthropic: Sonnet 4.5 for implementation and review of multi-step software changes, Haiku 4.5 for responsive support and the in-family alternative.".into(),
            roles: roles(
                "anthropic/claude-sonnet-4.5",
                "anthropic/claude-haiku-4.5",
                "anthropic/claude-sonnet-4.5",
                "anthropic/claude-haiku-4.5",
            ),
        },
        ModelSet {
            id: "qwen".into(),
            label: "Qwen coding".into(),
            maker: "qwen".into(),
            project_type: "general".into(),
            rationale: "Qwen3 Coder handles every role with a focus on code generation, tool use and repository context. This maker-only set has no different-model fallback; choose a mixed set for that option.".into(),
            roles: roles("qwen/qwen3-coder", "qwen/qwen3-coder", "qwen/qwen3-coder", "qwen/qwen3-coder"),
        },
        ModelSet {
            id: "google".into(),
            label: "Google coding and analysis".into(),
            maker: "google".into(),
            project_type: "general".into(),
            rationale: "Gemini 2.5 Flash handles every role, combining coding, mathematics and scientific-analysis strengths. This maker-only set has no different-model fallback; each request still needs supported endpoint metadata.".into(),
            roles: roles("google/gemini-2.5-flash", "google/gemini-2.5-flash", "google/gemini-2.5-flash", "google/gemini-2.5-flash"),
        },
        ModelSet {
            id: "web".into(),
            label: "Web projects".into(),
            maker: "mixed".into(),
            project_type: "web".into(),
            rationale: "Qwen3 Coder leads app and web implementation for its code-generation and agentic tool-use focus. GPT-4.1 checks instructions and code diffs; Mini handles support and the lower-cost alternative.".into(),
            roles: roles(
                "qwen/qwen3-coder",
                "openai/gpt-4.1-mini",
                "openai/gpt-4.1",
                "openai/gpt-4.1-mini",
            ),
        },
        ModelSet {
            id: "backend".into(),
            label: "Backend and services".into(),
            maker: "mixed".into(),
            project_type: "backend".into(),
            rationale: "Sonnet 4.5 leads service changes for its system-design and code-security focus; GPT-4.1 reviews precise changes against requirements. Mini handles support and Qwen3 Coder is the coding alternative.".into(),
            roles: roles("anthropic/claude-sonnet-4.5", "openai/gpt-4.1-mini", "openai/gpt-4.1", "qwen/qwen3-coder"),
        },
        ModelSet {
            id: "systems".into(),
            label: "Systems projects".into(),
            maker: "mixed".into(),
            project_type: "systems".into(),
            rationale: "GPT-4.1 leads precise changes across repository context; Sonnet 4.5 reviews system design, security and specification adherence. Mini handles support; Qwen3 Coder is the coding alternative. This is not a language-specific qualification.".into(),
            roles: roles(
                "openai/gpt-4.1",
                "openai/gpt-4.1-mini",
                "anthropic/claude-sonnet-4.5",
                "qwen/qwen3-coder",
            ),
        },
        ModelSet {
            id: "data".into(),
            label: "Data and scientific code".into(),
            maker: "mixed".into(),
            project_type: "data".into(),
            rationale: "Gemini 2.5 Flash leads coding and analysis for its mathematics and scientific-task focus; GPT-4.1 reviews implementation and instructions. Flash also handles support; Qwen3 Coder is the coding alternative.".into(),
            roles: roles("google/gemini-2.5-flash", "google/gemini-2.5-flash", "openai/gpt-4.1", "qwen/qwen3-coder"),
        },
    ]
}
fn role(name: &str) -> Result<RequestRole, String> {
    serde_json::from_value(json!(name)).map_err(|_| "unknown model role".into())
}
impl Preferences {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || crate::args::parse_usd(&self.budget_usd)? == Micros::ZERO {
            return Err("model preferences require version 1 and a positive budget".into());
        }
        for text in [
            &self.set.id,
            &self.set.label,
            &self.set.maker,
            &self.set.project_type,
        ] {
            if text.is_empty() || text.len() > 128 || text.chars().any(char::is_control) {
                return Err("invalid model set label".into());
            }
        }
        if self.set.rationale.len() > 1024 || self.set.rationale.chars().any(char::is_control) {
            return Err("invalid model set rationale".into());
        }
        if !self.set.roles.contains_key("main") || self.set.roles.len() > 8 {
            return Err("model set requires a main role and at most eight roles".into());
        }
        for (name, models) in &self.set.roles {
            role(name)?;
            if models.is_empty()
                || models.len() > 8
                || models.iter().collect::<BTreeSet<_>>().len() != models.len()
            {
                return Err(
                    "each model role requires one primary and at most seven distinct alternatives"
                        .into(),
                );
            }
            for model in models {
                if model.len() > 256
                    || !model.contains('/')
                    || model
                        .split('/')
                        .any(|part| part.is_empty() || part == "." || part == "..")
                    || !model
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"/-._".contains(&b))
                {
                    return Err("invalid provider model identifier".into());
                }
            }
        }
        if self.choice_sets.len() > 8 {
            return Err("at most eight rotation roles are supported".into());
        }
        for (name, sets) in &self.choice_sets {
            role(name)?;
            if !self.set.roles.contains_key(name) || sets.is_empty() || sets.len() > 3 {
                return Err(
                    "rotation requires one to three consecutive choices for an existing role"
                        .into(),
                );
            }
            let mut seen = BTreeSet::new();
            for (index, choice) in sets.iter().enumerate() {
                let mut legacy = self.clone();
                legacy.choice_sets.clear();
                legacy.set.roles.insert(name.clone(), choice.models.clone());
                legacy.validate()?;
                if choice.models.iter().any(|model| !seen.insert(model)) {
                    return Err("a model may occur only once across a role's choices".into());
                }
                if index == 2 && choice.max_reference_request_cost_usd.is_none() {
                    return Err(
                        "third choice requires an explicit reference-request price ceiling".into(),
                    );
                }
                if let Some(ceiling) = &choice.max_reference_request_cost_usd {
                    crate::args::parse_usd(ceiling)?;
                }
                for (model, endpoints) in &choice.endpoints {
                    if !choice.models.contains(model)
                        || endpoints.is_empty()
                        || endpoints.len() > 32
                        || endpoints.iter().collect::<BTreeSet<_>>().len() != endpoints.len()
                    {
                        return Err(
                            "endpoint narrowing requires distinct endpoints of selected models"
                                .into(),
                        );
                    }
                    for endpoint in endpoints {
                        ModelEndpoint {
                            model: model.clone(),
                            endpoint: endpoint.clone(),
                        }
                        .validate()
                        .map_err(|error| error.to_string())?;
                    }
                }
            }
        }
        Ok(())
    }
}
pub fn account_root() -> Result<PathBuf, String> {
    let root = PathBuf::from(
        std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is required for account setup")?,
    )
    .join("VCP/account");
    // First launch needs no project. Repository and sync-root rejection still
    // apply, without treating the current working directory as a project.
    checked_account_path(&root)
}
fn checked_account_path(root: &Path) -> Result<PathBuf, String> {
    let absolute = std::path::absolute(root).map_err(|_| "absolute account data path required")?;
    let ancestor = absolute
        .ancestors()
        .find(|path| std::fs::symlink_metadata(path).is_ok())
        .ok_or("account data ancestor unavailable")?;
    // Check the original path before canonicalization could erase a junction.
    // Return that original path so later reads and writes recheck its parents.
    let registry = settings::registry_root(ancestor)?;
    let _pin = registry
        .hold(None, true)
        .map_err(|_| "account data ancestor redirected")?;
    settings::local_path(&absolute, &absolute.join("not-a-project"))?;
    Ok(absolute)
}
pub fn ensure_root(root: &Path) -> Result<(), String> {
    let checked = checked_account_path(root)?;
    std::fs::create_dir_all(&checked).map_err(|_| "account data directory unavailable")?;
    settings::registry_root(&checked)?
        .hold(None, true)
        .map_err(|_| "account data directory redirected")?;
    Ok(())
}
pub fn read_record(root: &Path, name: &str) -> Result<Option<Value>, String> {
    record_name(name)?;
    if !root.exists() {
        return Ok(None);
    }
    let registry = settings::registry_root(root)?;
    match registry.read(Path::new(name), 256 * 1024) {
        Ok(source) => serde_json::from_slice(&source.bytes)
            .map(Some)
            .map_err(|_| "invalid account record".into()),
        Err(vcp_repository::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(None)
        }
        Err(_) => Err("account record unavailable or redirected".into()),
    }
}
pub fn save_record(root: &Path, name: &str, value: &Value) -> Result<(), String> {
    use std::io::Write;
    record_name(name)?;
    ensure_root(root)?;
    let registry = settings::registry_root(root)?;
    let _pin = registry
        .hold(None, true)
        .map_err(|_| "account data directory redirected")?;
    read_record(root, name)?;
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|_| "account record serialization failed")?;
    if bytes.len() > 256 * 1024 {
        return Err("account record too large".into());
    }
    let temporary = root.join(format!("record-{}.new", vcp_domain::CommandId::new()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| "account record staging failed")?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| "account record write failed")?;
    drop(file);
    let target = root.join(name);
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let source: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
        let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: both paths are terminated and held through this atomic move.
        if unsafe {
            MoveFileExW(
                source.as_ptr(),
                target.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err("account record publication failed".into());
        }
    }
    #[cfg(not(windows))]
    std::fs::rename(temporary, target).map_err(|_| "account record publication failed")?;
    Ok(())
}
fn record_name(name: &str) -> Result<(), String> {
    let mut parts = Path::new(name).components();
    if !matches!(parts.next(), Some(std::path::Component::Normal(_)))
        || parts.next().is_some()
        || name.is_empty()
        || name.len() > 200
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        || name.ends_with('.')
    {
        return Err("invalid account record name".into());
    }
    Ok(())
}
pub fn read(root: &Path) -> Result<Option<Preferences>, String> {
    read_named(root, "models.json")
}
fn read_named(root: &Path, name: &str) -> Result<Option<Preferences>, String> {
    read_record(root, name)?
        .map(|value| {
            let preferences: Preferences =
                serde_json::from_value(value).map_err(|_| "invalid model preferences")?;
            preferences.validate()?;
            Ok(preferences)
        })
        .transpose()
}
pub fn save(root: &Path, preferences: &Preferences) -> Result<(), String> {
    preferences.validate()?;
    save_record(
        root,
        "models.json",
        &serde_json::to_value(preferences).map_err(|_| "model preferences serialization failed")?,
    )
}
fn project_id(workspace: &Path) -> Result<String, String> {
    let canonical = workspace
        .canonicalize()
        .map_err(|_| "project directory unavailable")?;
    Ok(vcp_protocol::digest_bytes(
        canonical.to_string_lossy().to_lowercase().as_bytes(),
    ))
}
pub fn save_project(
    root: &Path,
    workspace: &Path,
    preferences: &Preferences,
) -> Result<(), String> {
    preferences.validate()?;
    save_record(
        root,
        &format!("project-{}.json", project_id(workspace)?),
        &serde_json::to_value(preferences).map_err(|_| "model preferences serialization failed")?,
    )
}
pub fn effective(root: &Path, workspace: &Path) -> Result<Preferences, String> {
    if let Some(project) = read_named(root, &format!("project-{}.json", project_id(workspace)?))? {
        return Ok(project);
    }
    Ok(read(root)?.unwrap_or_default())
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Browse suggested sets by maker or coding-project type.
    List {
        #[arg(long)]
        maker: Option<String>,
        #[arg(long)]
        project_type: Option<String>,
    },
    /// Inspect a suggested set, or current account/project choices when omitted.
    Show {
        set: Option<String>,
        /// Fetch exact endpoint prices/context metadata; no inference is sent.
        #[arg(long)]
        refresh: bool,
    },
    /// Select account defaults or a project override; existing tasks retain their set.
    Select {
        set: String,
        /// Opt into rotation at this priority (1, 2 or 3); omit for legacy fallback.
        #[arg(long, value_parser = clap::value_parser!(u8).range(1..=3))]
        choice: Option<u8>,
        #[arg(long, requires = "choice")]
        max_reference_request_cost_usd: Option<String>,
        #[arg(long)]
        project: bool,
    },
    /// Replace one role's ordered selections in account defaults or project override.
    Customize {
        #[arg(long)]
        role: String,
        #[arg(long, required = true)]
        model: Vec<String>,
        /// Rotate these models at this priority; later choices require earlier ones.
        #[arg(long, value_parser = clap::value_parser!(u8).range(1..=3))]
        choice: Option<u8>,
        #[arg(long, requires = "choice")]
        max_reference_request_cost_usd: Option<String>,
        /// Restrict an approved model to exact endpoint tags: model=id/region.
        #[arg(long, requires = "choice")]
        endpoint: Vec<String>,
        #[arg(long)]
        project: bool,
    },
    /// Set the default per-task budget.
    Budget {
        usd: String,
        #[arg(long)]
        project: bool,
    },
}
pub async fn execute(command: Option<&Command>, workspace: &Path) -> Result<Value, String> {
    let root = account_root()?;
    let selected = || effective(&root, workspace);
    match command {
        None => Ok(
            json!({"account":read(&root)?.unwrap_or_default(),"effective":selected()?,"precedence":["retained task selection","explicit --config","project override","account defaults"],"rotation":"when explicitly selected: rotate inside the highest ready choice, then fail over to second/third choices; outside models remain unauthorized","reference_request":{"input_tokens":8000,"output_tokens":1024,"normal_suggestion_multiplier":2},"fallback":"legacy selections preserve ordered eligible alternatives"}),
        ),
        Some(Command::List {
            maker,
            project_type,
        }) => Ok(
            json!({"sets":builtin_sets().into_iter().filter(|set| maker.as_ref().is_none_or(|m| &set.maker==m) && project_type.as_ref().is_none_or(|p| &set.project_type==p)).collect::<Vec<_>>(),"qualification":"suggestions only; current provider compatibility and metadata determine eligibility"}),
        ),
        Some(Command::Show { set, refresh }) => {
            let preferences = if let Some(set) = set {
                Preferences {
                    set: builtin_sets()
                        .into_iter()
                        .find(|candidate| &candidate.id == set)
                        .ok_or("unknown model set")?,
                    ..Preferences::default()
                }
            } else {
                selected()?
            };
            if *refresh {
                inspect_endpoints(&preferences).await
            } else if set.is_some() {
                serde_json::to_value(preferences.set)
                    .map_err(|_| "model set serialization failed".into())
            } else {
                serde_json::to_value(preferences)
                    .map_err(|_| "model set serialization failed".into())
            }
        }
        Some(command) => {
            let project = match command {
                Command::Select { project, .. }
                | Command::Customize { project, .. }
                | Command::Budget { project, .. } => *project,
                _ => false,
            };
            let mut preferences = if project {
                selected()?
            } else {
                read(&root)?.unwrap_or_default()
            };
            match command {
                Command::Select {
                    set,
                    choice,
                    max_reference_request_cost_usd,
                    ..
                } => {
                    let selected = builtin_sets()
                        .into_iter()
                        .find(|s| &s.id == set)
                        .ok_or("unknown model set")?;
                    if let Some(choice) = choice {
                        for (role, models) in selected.roles {
                            set_choice(
                                &mut preferences,
                                &role,
                                *choice,
                                models,
                                max_reference_request_cost_usd.clone(),
                            )?;
                        }
                    } else {
                        preferences.set = selected;
                        preferences.choice_sets.clear();
                    }
                }
                Command::Customize {
                    role,
                    model,
                    choice,
                    max_reference_request_cost_usd,
                    endpoint,
                    ..
                } => {
                    if let Some(choice) = choice {
                        set_choice(
                            &mut preferences,
                            role,
                            *choice,
                            model.clone(),
                            max_reference_request_cost_usd.clone(),
                        )?;
                        for value in endpoint {
                            let (model, tag) = value
                                .split_once('=')
                                .ok_or("endpoint syntax requires model=exact-endpoint-tag")?;
                            preferences
                                .choice_sets
                                .get_mut(role)
                                .ok_or("rotation role unavailable")?[usize::from(*choice - 1)]
                            .endpoints
                            .entry(model.into())
                            .or_default()
                            .push(tag.into());
                        }
                    } else {
                        preferences.set.roles.insert(role.clone(), model.clone());
                        preferences.choice_sets.remove(role);
                    }
                    preferences.set.rationale = "Customized owner selections; inspect each role's ordered models. No comparative quality result is implied.".into();
                }
                Command::Budget { usd, .. } => preferences.budget_usd = usd.clone(),
                _ => return Err("unsupported model preference update".into()),
            }
            if project {
                save_project(&root, workspace, &preferences)?
            } else {
                save(&root, &preferences)?
            }
            Ok(
                json!({"status":"saved","scope":if project{"project"}else{"account"},"preferences":preferences,"existing_tasks":"retain their selections","model_calls":0}),
            )
        }
    }
}

async fn inspect_endpoints(preferences: &Preferences) -> Result<Value, String> {
    preferences.validate()?;
    let key = crate::credential::require(false)?;
    let mut endpoints = Vec::new();
    let mut unavailable = Vec::new();
    for model in selected_models(preferences) {
        match crate::provider_setup::connection::refresh_all(model, key.expose()).await {
            Ok(prepared) => {
                for value in prepared {
                    let reference = vcp_models::rotation::reference_cost(
                        &value.snapshot,
                        REFERENCE_INPUT,
                        REFERENCE_OUTPUT,
                    )?;
                    let reservation = vcp_lifecycle::foundation::conformance::reservation(
                        &value.snapshot,
                        REFERENCE_OUTPUT,
                    )
                    .map_err(|error| error.to_string())?;
                    endpoints.push(json!({"model":model,"endpoint":value.snapshot.compatibility.endpoint,"tariffs":value.snapshot.price,"max_input_tokens":value.snapshot.max_input,"max_output_tokens":value.snapshot.max_output,"reference_request_cost":reference,"conservative_full_input_reservation_micros":reservation,"observed_at":value.snapshot.observed_at}));
                }
            }
            Err(_) => unavailable.push(model),
        }
    }
    Ok(
        json!({"preferences":preferences,"endpoints":endpoints,"unavailable_models":unavailable,"reference_request":{"input_tokens":REFERENCE_INPUT,"output_tokens":REFERENCE_OUTPUT,"normal_suggestion_multiplier":2},"qualification":"endpoint metadata only; no comparative quality or independent-capacity claim","model_calls":0}),
    )
}

pub(crate) fn set_choice(
    preferences: &mut Preferences,
    name: &str,
    choice: u8,
    models: Vec<String>,
    ceiling: Option<String>,
) -> Result<(), String> {
    role(name)?;
    if !(1..=3).contains(&choice) {
        return Err("choice must be 1, 2 or 3".into());
    }
    let index = usize::from(choice - 1);
    let sets = preferences.choice_sets.entry(name.into()).or_default();
    if index > sets.len() {
        return Err("select earlier choices first".into());
    }
    let value = ChoicePreferences {
        models: models.clone(),
        endpoints: BTreeMap::new(),
        max_reference_request_cost_usd: ceiling,
    };
    if index == sets.len() {
        sets.push(value);
    } else {
        sets[index] = value;
    }
    preferences.set.roles.entry(name.into()).or_insert(models);
    preferences.validate()
}

fn selected_models(preferences: &Preferences) -> BTreeSet<&String> {
    preferences
        .set
        .roles
        .iter()
        .flat_map(|(role, models)| {
            preferences
                .choice_sets
                .get(role)
                .map(|sets| {
                    sets.iter()
                        .flat_map(|set| set.models.iter())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| models.iter().collect())
        })
        .collect()
}

pub fn routing_configuration(
    preferences: &Preferences,
    prepared: &[(Snapshot, Vec<u8>)],
) -> Result<vcp_lifecycle::foundation::routing::Configuration, String> {
    preferences.validate()?;
    let observed = prepared
        .iter()
        .map(|(s, _)| s.observed_at)
        .max()
        .ok_or("no eligible models in selected set")?;
    let mut entries = vec![];
    let mut raw_catalogs = BTreeMap::new();
    for (snapshot, raw) in prepared {
        let model = &snapshot.compatibility.model;
        if !selected_models(preferences).contains(model) {
            return Err("prepared provider is outside selected model set".into());
        }
        let identity = ModelEndpoint {
            model: model.clone(),
            endpoint: snapshot.compatibility.endpoint.clone(),
        };
        entries.push(Candidate{identity,availability:State::Supported,reasons:vec![],provenance:vec![Provenance{source:"https://openrouter.ai/api/v1/models".into(),sha256:snapshot.raw_sha256.clone(),observed_at:snapshot.observed_at,effective_at:None,limitations:vec!["Explicit owner selection; model-specific live behavior and comparative quality unverified".into()]}],capabilities:BTreeMap::from([("responses_text_tools".into(),State::Supported)]),snapshot:Some(snapshot.clone()),compatibility:vec![],memberships:vec![]});
        raw_catalogs.insert(
            snapshot.id.clone(),
            String::from_utf8(raw.clone()).map_err(|_| "provider catalog is not UTF-8")?,
        );
    }
    let policy = Policy {
        schema_version: 1,
        id: String::new(),
        parent: None,
        profile: Profile::Med,
        allowed_models: entries.iter().map(|e| e.identity.model.clone()).collect(),
        allowed_endpoints: entries
            .iter()
            .map(|e| e.identity.endpoint.clone())
            .collect(),
        allowed_groups: BTreeSet::new(),
        quality_floor_bps: 0,
        minimum_samples: 1,
        maximum_evidence_age_ms: 86_400_000,
        deny_data_collection: true,
        require_zdr: false,
        ordering: vec![
            Preference::TotalCost,
            Preference::Quality,
            Preference::Latency,
            Preference::Capability,
        ],
        pin: None,
        broader_task_class: None,
        output_tokens: None,
        input_tokens: None,
        escalation_limits: None,
        reasoning_effort: None,
        retrieval_limits: None,
    }
    .seal()
    .map_err(|e| e.to_string())?;
    let zero = Money {
        currency: "USD"
            .to_owned()
            .try_into()
            .map_err(|_| "invalid budget currency")?,
        micros: Micros::ZERO,
    };
    let estimates=entries.iter().map(|entry| CostEstimate{candidate:entry.identity.clone(),first_attempt:Usage{input:Units::new(1),output:Units::new(1),requests:Units::new(1),..Usage::default()},retries:Usage::default(),handoff:Usage::default(),support:Some(zero.clone()),children:Some(zero.clone()),verification:Some(zero.clone()),assumptions:vec!["Per-request estimate is filled from actual assembled input and output ceiling; later calls require independent ledger admission".into()],evidence_refs:vec!["owner-model-set/1".into()]}).collect();
    let rotation = capture_rotation(preferences, &entries)?;
    let owner_assignments = preferences
        .set
        .roles
        .iter()
        .map(|(name, legacy)| {
            let request_role = role(name)?;
            let models = preferences
                .choice_sets
                .get(name)
                .map(|sets| {
                    sets.iter()
                        .flat_map(|set| set.models.iter())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| legacy.iter().collect());
            Ok(vcp_lifecycle::foundation::routing::OwnerAssignment {
                role: request_role,
                candidates: if let Some(sets) = rotation
                    .as_ref()
                    .map(|rotation| rotation.sets(request_role))
                    .filter(|sets| !sets.is_empty())
                {
                    sets.iter()
                        .flat_map(|set| set.members.iter().cloned())
                        .collect()
                } else {
                    models
                        .into_iter()
                        .flat_map(|model| {
                            entries
                                .iter()
                                .filter(move |entry| &entry.identity.model == model)
                                .map(|entry| entry.identity.clone())
                        })
                        .collect()
                },
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    if owner_assignments
        .iter()
        .any(|assignment| assignment.candidates.is_empty())
    {
        return Err("a selected role has no eligible original model; ask the owner to choose a new set before using an outside model".into());
    }
    let config = vcp_lifecycle::foundation::routing::Configuration {
        owner_assignments,
        rotation,
        escalation: None,
        catalog: CatalogRevision::create(None, observed, None, entries)
            .map_err(|e| e.to_string())?,
        policy,
        task_class: "owner-selected-coding".into(),
        estimates,
        raw_catalogs,
    };
    config.validate()?;
    Ok(config)
}

const REFERENCE_INPUT: Units = Units::new(8000);
const REFERENCE_OUTPUT: Units = Units::new(1024);

fn capture_rotation(
    preferences: &Preferences,
    entries: &[Candidate],
) -> Result<Option<vcp_models::rotation::Policy>, String> {
    use vcp_models::rotation::{reference_cost, ChoiceSet, Policy as RotationPolicy, RoleSets};
    if preferences.choice_sets.is_empty() {
        return Ok(None);
    }
    let mut roles = Vec::new();
    for (name, choices) in &preferences.choice_sets {
        let mut sets = Vec::new();
        for (index, choice) in choices.iter().enumerate() {
            let allowed = |entry: &Candidate| {
                choice
                    .endpoints
                    .get(&entry.identity.model)
                    .is_none_or(|endpoints| endpoints.contains(&entry.identity.endpoint))
            };
            let explicit_ceiling = choice
                .max_reference_request_cost_usd
                .as_ref()
                .map(|value| crate::args::parse_usd(value))
                .transpose()?;
            let priced = entries
                .iter()
                .filter(|entry| choice.models.contains(&entry.identity.model) && allowed(entry))
                .map(|entry| {
                    let snapshot = entry.snapshot.as_ref().ok_or("rotation snapshot missing")?;
                    Ok((
                        entry,
                        reference_cost(snapshot, REFERENCE_INPUT, REFERENCE_OUTPUT)?,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?;
            let reference = choice.models.iter().find_map(|model| {
                priced
                    .iter()
                    .filter(|(entry, cost)| {
                        &entry.identity.model == model
                            && explicit_ceiling.is_none_or(|ceiling| cost.micros <= ceiling)
                    })
                    .min_by_key(|(_, cost)| cost.micros)
                    .map(|(_, cost)| cost.clone())
            });
            let Some(reference) = reference else {
                continue;
            };
            let ceiling = match explicit_ceiling {
                Some(value) => value,
                None => Micros::new(
                    reference
                        .micros
                        .get()
                        .checked_mul(2)
                        .ok_or("reference cost overflow")?,
                ),
            };
            let mut members = Vec::new();
            for model in &choice.models {
                for (entry, cost) in priced
                    .iter()
                    .filter(|(entry, _)| &entry.identity.model == model)
                {
                    if cost.micros <= ceiling {
                        members.push(entry.identity.clone());
                    }
                }
            }
            if members.is_empty() {
                return Err("choice has no endpoint within selected price ceiling".into());
            }
            sets.push(ChoiceSet {
                id: format!("{name}-choice-{}", index + 1),
                label: format!("{} choice", ["First", "Second", "Third"][index]),
                members,
                max_reference_request_cost: Money {
                    currency: reference.currency.clone(),
                    micros: ceiling,
                },
                reference_request_cost: reference,
            });
        }
        roles.push(RoleSets {
            role: role(name)?,
            sets,
        });
    }
    let policy = RotationPolicy {
        roles,
        reference_input_tokens: REFERENCE_INPUT,
        reference_output_tokens: REFERENCE_OUTPUT,
    };
    policy.validate().map_err(|error| error.to_string())?;
    Ok(Some(policy))
}

pub async fn refresh_set(
    preferences: &Preferences,
    key: &str,
) -> Result<Vec<(Snapshot, Vec<u8>)>, String> {
    preferences.validate()?;
    let mut prepared = vec![];
    for model in selected_models(preferences) {
        // A unavailable member cannot authorize a model outside the set. Keep
        // current eligible alternatives; every role must retain at least one.
        let rotation_member = preferences
            .choice_sets
            .values()
            .flatten()
            .any(|set| set.models.contains(model));
        let result = if rotation_member {
            crate::provider_setup::connection::refresh_all(model, key).await
        } else {
            crate::provider_setup::connection::refresh(model, key)
                .await
                .map(|value| vec![value])
        };
        match result {
            Ok(values)=>prepared.extend(values.into_iter().map(|value| (value.snapshot,value.catalog))),
            Err(_)=>eprintln!("Model {model} is currently ineligible or metadata is unavailable; it will not be used."),
        }
    }
    for (role, legacy) in &preferences.set.roles {
        let models = preferences
            .choice_sets
            .get(role)
            .map(|sets| sets.iter().flat_map(|set| &set.models).collect::<Vec<_>>())
            .unwrap_or_else(|| legacy.iter().collect());
        if !models.into_iter().any(|model| {
            prepared
                .iter()
                .any(|(snapshot, _)| &snapshot.compatibility.model == model)
        }) {
            return Err("a selected role has no eligible model; inspect `vcp models` and select an alternative explicitly".into());
        }
    }
    Ok(prepared)
}
fn publish_profile(
    root: &Path,
    workspace: &Path,
    value: Value,
    preferences: &Preferences,
    prepared: &[(Snapshot, Vec<u8>)],
) -> Result<PathBuf, String> {
    let configuration = routing_configuration(preferences, prepared)?;
    publish_configuration(
        root,
        workspace,
        value,
        &preferences.budget_usd,
        configuration,
    )
}

fn publish_configuration(
    root: &Path,
    workspace: &Path,
    mut value: Value,
    budget_usd: &str,
    configuration: vcp_lifecycle::foundation::routing::Configuration,
) -> Result<PathBuf, String> {
    configuration.validate()?;
    let main = preferred_main(&configuration)?;
    let snapshot = configuration
        .catalog
        .snapshot(main)
        .ok_or("selected main model unavailable")?;
    let raw = configuration
        .raw_catalogs
        .get(&snapshot.id)
        .ok_or("selected main catalog unavailable")?
        .as_bytes();
    ensure_root(root)?;
    let catalog_name = format!("catalog-{}.json", snapshot.id);
    // Exact source bytes are immutable, and are not reformatted on publication.
    let catalog = root.join(&catalog_name);
    if catalog.exists() {
        let existing = settings::registry_root(root)?
            .read(Path::new(&catalog_name), 4 * 1024 * 1024)
            .map_err(|_| "captured model catalog unavailable")?;
        if existing.bytes != raw {
            return Err("captured model catalog differs from immutable identity".into());
        }
    } else {
        use std::io::Write;
        let registry = settings::registry_root(root)?;
        let _pin = registry
            .hold(None, true)
            .map_err(|_| "account root redirected")?;
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&catalog)
            .map_err(|_| "catalog publication failed")?;
        file.write_all(raw)
            .and_then(|()| file.sync_all())
            .map_err(|_| "catalog publication failed")?;
    }
    value["workspace"] = json!(workspace);
    value["provider"] = json!(snapshot);
    value["catalog"] = json!(catalog);
    value["routing"] = json!(configuration);
    value["budget_usd"] = json!(budget_usd);
    let bytes = serde_json::to_vec(&value).map_err(|_| "selected profile serialization failed")?;
    let name = format!("profile-{}.json", vcp_protocol::digest_bytes(&bytes));
    let profile = root.join(&name);
    if !profile.exists() {
        save_record(root, &name, &value)?;
    }
    let parsed: settings::Profile =
        serde_json::from_value(value).map_err(|_| "selected profile rejected")?;
    parsed.prepare(vcp_domain::policy::Autonomy::Plan)?;
    Ok(profile)
}

pub(crate) fn preferred_main(
    configuration: &vcp_lifecycle::foundation::routing::Configuration,
) -> Result<&ModelEndpoint, String> {
    let sets = configuration
        .rotation
        .as_ref()
        .map(|rotation| rotation.sets(RequestRole::Main))
        .unwrap_or(&[]);
    if !sets.is_empty() {
        sets.iter()
            .find_map(|set| set.members.first())
            .ok_or_else(|| "main rotation selection unavailable".into())
    } else {
        configuration
            .owner_assignments
            .iter()
            .find(|assignment| assignment.role == RequestRole::Main)
            .and_then(|assignment| assignment.candidates.first())
            .ok_or_else(|| "main model selection unavailable".into())
    }
}
/// Caller has obtained explicit trust for this project. Merely selecting model
/// preferences cannot create this registration or grant project trust.
pub async fn configure_project(
    root: &Path,
    workspace: &Path,
    affected_paths: &[PathBuf],
    preferences: &Preferences,
    key: &str,
) -> Result<PathBuf, String> {
    let workspace = workspace
        .canonicalize()
        .map_err(|_| "project directory unavailable")?;
    if affected_paths.is_empty() {
        return Err("project setup requires an affected task path".into());
    }
    for path in affected_paths {
        vcp_repository::path::relative(path).map_err(|e| e.to_string())?;
    }
    let prepared = refresh_set(preferences, key).await?;
    let value = project_setup_value(root, &workspace, affected_paths, preferences)?;
    let profile = publish_profile(root, &workspace, value, preferences, &prepared)?;
    save_record(
        root,
        &format!("project-profile-{}.json", project_id(&workspace)?),
        &json!({"profile":profile,"workspace":workspace}),
    )?;
    Ok(profile)
}

fn project_setup_value(
    root: &Path,
    workspace: &Path,
    affected_paths: &[PathBuf],
    preferences: &Preferences,
) -> Result<Value, String> {
    let mut value = if let Some(existing) = project_profile(root, workspace)? {
        serde_json::to_value(settings::load(&existing, workspace)?)
            .map_err(|_| "existing project profile serialization failed")?
    } else {
        json!({"version":1,"workspace":workspace,"maximum_autonomy":"ask","automatic_effects":["read"],"canonical_tools":["vcp_read","vcp_list","vcp_search","vcp_patch","vcp_verify"],"max_requests":8,"output_tokens":"2048","provider_timeout_seconds":120,"max_transport_retries":2,"deadline_seconds":300,"processes":[],"checks":[]})
    };
    // The interview explicitly requests trust, task paths and budget. Preserve
    // the project's existing tool, process, verification and autonomy ceilings.
    value["trust_workspace"] = json!(true);
    value["affected_paths"] = json!(affected_paths);
    value["budget_usd"] = json!(preferences.budget_usd);
    Ok(value)
}
pub fn project_profile(root: &Path, workspace: &Path) -> Result<Option<PathBuf>, String> {
    read_record(
        root,
        &format!("project-profile-{}.json", project_id(workspace)?),
    )?
    .map(|value| {
        let path = value
            .get("profile")
            .and_then(Value::as_str)
            .ok_or("invalid project profile registration")?;
        let path = PathBuf::from(path);
        if path.parent() != Some(root) {
            return Err("registered project profile is outside account directory".into());
        }
        Ok(path)
    })
    .transpose()
}
pub async fn materialize(
    root: &Path,
    base: &Path,
    workspace: &Path,
    key: &str,
) -> Result<PathBuf, String> {
    let preferences = effective(root, workspace)?;
    // Use the normal profile loader first; it validates binding and imports.
    let loaded = settings::load(base, workspace)?;
    let value = serde_json::to_value(loaded).map_err(|_| "project profile serialization failed")?;
    let prepared = refresh_set(&preferences, key).await?;
    publish_profile(root, workspace, value, &preferences, &prepared)
}

pub fn retain_task(
    directory: &Path,
    task: &vcp_domain::TaskId,
    profile: &settings::Profile,
) -> Result<(), String> {
    let name = format!("task-models-{task}.json");
    let value = serde_json::to_value(RetainedModels::from_profile(profile))
        .map_err(|_| "task model selection serialization failed")?;
    if let Some(existing) = read_record(directory, &name)? {
        if existing != value {
            return Err("task already has an immutable model selection".into());
        }
        return Ok(());
    }
    save_record(directory, &name, &value)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedModels {
    version: u32,
    workspace: PathBuf,
    provider: Snapshot,
    catalog: PathBuf,
    routing: Option<vcp_lifecycle::foundation::routing::Configuration>,
}
impl RetainedModels {
    fn from_profile(profile: &settings::Profile) -> Self {
        Self {
            version: 1,
            workspace: profile.workspace.clone(),
            provider: profile.provider.clone(),
            catalog: profile.catalog.clone(),
            routing: profile.routing.clone(),
        }
    }
}

#[cfg(windows)]
pub async fn load_for_command(
    cli: &crate::args::Cli,
    data: &Path,
    directory: &Path,
    workspace: &Path,
) -> Result<settings::Profile, String> {
    use crate::args::{Command as CliCommand, Sessions};
    use vcp_store::contract::Collection;
    let existing = !matches!(cli.command, Some(CliCommand::Run(_)));
    if existing && directory.join("workspace.json").exists() {
        let entry: settings::WorkspaceEntry = serde_json::from_slice(
            &settings::read_workspace_descriptor(data, &directory.join("workspace.json"))?,
        )
        .map_err(|_| "invalid workspace descriptor")?;
        let store = vcp_store::Store::open(
            &entry.config.canonical_root,
            entry.config.backend,
            &[workspace.to_path_buf()],
        )
        .await
        .map_err(|e| e.to_string())?;
        let selection = (|| -> Result<_, String> {
            Ok(match &cli.command {
                Some(CliCommand::Resume(resume)) if resume.task.is_some() => resume.task.clone(),
                Some(CliCommand::Sessions {
                    command: Sessions::Fork { through_turn, .. },
                }) => {
                    let turn: vcp_domain::task::Turn = store
                        .state()
                        .record(
                            Collection::Turn,
                            through_turn.as_str(),
                            &entry.config.workspace,
                        )
                        .and_then(|r| r.decode())
                        .map_err(|_| "fork turn unavailable")?;
                    Some(turn.scope.task)
                }
                _ => {
                    let session = match &cli.command {
                        Some(CliCommand::Sessions {
                            command: Sessions::Resume { session },
                        }) => Some(session),
                        _ => None,
                    };
                    crate::continuation::candidates(store.state(), &entry.config.workspace)?
                        .into_iter()
                        .find(|row| session.is_none_or(|session| session == &row.session))
                        .map(|row| row.task)
                }
            })
        })();
        store.close().await.map_err(|e| e.to_string())?;
        if let Some(task) = selection? {
            if let Some(value) = read_record(directory, &format!("task-models-{task}.json"))? {
                let retained: RetainedModels = serde_json::from_value(value)
                    .map_err(|_| "retained task model selection invalid")?;
                if retained.version != 1
                    || retained
                        .workspace
                        .canonicalize()
                        .map_err(|_| "retained project unavailable")?
                        != workspace
                {
                    return Err("retained task model workspace differs".into());
                }
                let base = current_base(cli, data, workspace)?;
                let mut profile =
                    with_retained_models(settings::load(&base, workspace)?, retained)?;
                if !profile.trust_workspace {
                    return Err(
                        "workspace trust must be explicitly granted in the current user profile"
                            .into(),
                    );
                }
                // Retain role assignments and exact model identities, refreshing
                // only expired metadata. No preference changes enter this path.
                if profile.routing.as_ref().is_some_and(|routing| {
                    !routing.owner_assignments.is_empty()
                        && (needs_metadata_refresh(&profile.provider)
                            || routing
                                .catalog
                                .entries
                                .iter()
                                .filter_map(|entry| entry.snapshot.as_ref())
                                .any(needs_metadata_refresh))
                }) {
                    let key = crate::credential::require(crate::setup_wizard::interactive(cli))?;
                    refresh_retained(&mut profile, key.expose()).await?;
                }
                return Ok(profile);
            }
        }
    }
    // Explicit owner profiles remain authoritative, including imported ceilings.
    if let Some(profile) = &cli.config {
        return settings::load(profile, workspace);
    }
    let root = account_root()?;
    if let Some(base) = project_profile(&root, workspace)? {
        let key = crate::credential::require(crate::setup_wizard::interactive(cli))?;
        let selected = materialize(&root, &base, workspace, key.expose()).await?;
        return settings::load(&selected, workspace);
    }
    settings::load(&data.join("profile.json"), workspace)
}

#[cfg(windows)]
fn current_base(cli: &crate::args::Cli, data: &Path, workspace: &Path) -> Result<PathBuf, String> {
    if let Some(explicit) = &cli.config {
        return Ok(explicit.clone());
    }
    Ok(project_profile(&account_root()?, workspace)?.unwrap_or_else(|| data.join("profile.json")))
}

fn with_retained_models(
    mut current: settings::Profile,
    mut retained: RetainedModels,
) -> Result<settings::Profile, String> {
    fn token_ceiling(selected: Option<Units>, ceiling: Option<Units>) -> Option<Units> {
        match (selected, ceiling) {
            (Some(selected), Some(ceiling)) => Some(selected.min(ceiling)),
            (selected, ceiling) => selected.or(ceiling),
        }
    }

    let ceilings = current.routing.as_ref().map(|routing| &routing.policy);
    if let Some(ceilings) = ceilings {
        ceilings.validate().map_err(|error| error.to_string())?;
    }
    let deny_data_collection = current.provider.compatibility.deny_data_collection
        || ceilings.is_some_and(|policy| policy.deny_data_collection);
    let require_zdr = current.provider.compatibility.require_zdr
        || ceilings.is_some_and(|policy| policy.require_zdr);
    if let Some(routing) = &mut retained.routing {
        routing
            .policy
            .validate()
            .map_err(|error| error.to_string())?;
        // Role order and model/endpoint preferences belong to the task. Current
        // trusted privacy and resource ceilings still constrain every dispatch.
        routing.policy.deny_data_collection |= deny_data_collection;
        routing.policy.require_zdr |= require_zdr;
        if let Some(ceilings) = ceilings {
            routing.policy.input_tokens =
                token_ceiling(routing.policy.input_tokens, ceilings.input_tokens);
            routing.policy.output_tokens =
                token_ceiling(routing.policy.output_tokens, ceilings.output_tokens);
            routing.policy.retrieval_limits =
                match (&routing.policy.retrieval_limits, &ceilings.retrieval_limits) {
                    (Some(selected), Some(ceiling)) => Some(selected.clamp(ceiling)),
                    (selected, ceiling) => selected.clone().or_else(|| ceiling.clone()),
                };
        }
        routing.policy = routing
            .policy
            .clone()
            .seal()
            .map_err(|error| error.to_string())?;
    } else {
        // A fixed snapshot cannot gain unverified provider privacy behavior.
        if (deny_data_collection && !retained.provider.compatibility.deny_data_collection)
            || (require_zdr && !retained.provider.compatibility.require_zdr)
        {
            return Err("retained fixed model does not meet the current privacy policy".into());
        }
        if let Some(ceilings) = ceilings {
            if ceilings.input_tokens.is_some() || ceilings.retrieval_limits.is_some() {
                return Err(
                    "retained fixed model cannot enforce the current routing resource ceilings"
                        .into(),
                );
            }
            current.output_tokens = token_ceiling(current.output_tokens, ceilings.output_tokens);
        }
    }
    current.provider = retained.provider;
    current.catalog = retained.catalog;
    current.routing = retained.routing;
    Ok(current)
}

#[cfg(windows)]
fn needs_metadata_refresh(snapshot: &Snapshot) -> bool {
    snapshot.current(settings::now()).is_err()
        || !vcp_models::catalog::compatibility::admitted(&snapshot.compatibility)
}

#[cfg(windows)]
async fn refresh_retained(profile: &mut settings::Profile, key: &str) -> Result<(), String> {
    let Some(routing) = profile.routing.as_ref() else {
        return Ok(());
    };
    let mut fresh = Vec::new();
    for model in routing
        .catalog
        .entries
        .iter()
        .map(|entry| &entry.identity.model)
        .collect::<BTreeSet<_>>()
    {
        if let Ok(prepared) = crate::provider_setup::connection::refresh_all(model, key).await {
            for endpoint in prepared {
                if routing.catalog.entries.iter().any(|entry| {
                    entry.identity.model == endpoint.snapshot.compatibility.model
                        && entry.identity.endpoint == endpoint.snapshot.compatibility.endpoint
                }) {
                    fresh.push((endpoint.snapshot, endpoint.catalog));
                }
            }
        }
    }
    let configuration = renew_retained_configuration(routing, &fresh)?;
    // Runtime eligibility may narrow the original selections, including using
    // an original fallback if the primary disappeared. Never rewrite the task's
    // retained record or introduce a new model/endpoint identity.
    let root = profile
        .catalog
        .parent()
        .ok_or("retained catalog parent missing")?;
    let selected = publish_configuration(
        root,
        &profile.workspace,
        serde_json::to_value(&*profile).map_err(|_| "retained profile serialization failed")?,
        profile
            .budget_usd
            .as_deref()
            .ok_or("retained task budget missing")?,
        configuration,
    )?;
    *profile = settings::load(&selected, &profile.workspace)?;
    Ok(())
}

/// Renewal changes metadata, never the task's exact per-role endpoint choices
/// or its routing restrictions. A model name alone is not a retained identity.
fn renew_retained_configuration(
    retained: &vcp_lifecycle::foundation::routing::Configuration,
    fresh: &[(Snapshot, Vec<u8>)],
) -> Result<vcp_lifecycle::foundation::routing::Configuration, String> {
    // Older compiled adapter evidence must not admit a request, but remains
    // usable as the identity of a retained owner selection. Check its immutable
    // source binding here, then fully validate only the renewed snapshots below.
    retained
        .policy
        .validate()
        .map_err(|error| error.to_string())?;
    if retained.owner_assignments.is_empty()
        || retained.escalation.is_some()
        || retained.catalog.schema_version != SCHEMA_VERSION
        || retained.catalog.id
            != retained
                .catalog
                .digest()
                .map_err(|error| error.to_string())?
    {
        return Err("invalid retained owner selection identity".into());
    }
    for entry in &retained.catalog.entries {
        entry
            .identity
            .validate()
            .map_err(|error| error.to_string())?;
        let snapshot = entry
            .snapshot
            .as_ref()
            .ok_or("retained endpoint snapshot missing")?;
        let raw = retained
            .raw_catalogs
            .get(&snapshot.id)
            .ok_or("retained endpoint source missing")?;
        if snapshot.compatibility.model != entry.identity.model
            || snapshot.compatibility.endpoint != entry.identity.endpoint
            || snapshot.id
                != snapshot
                    .identity_digest()
                    .map_err(|error| error.to_string())?
            || snapshot.price.id != snapshot.id
            || snapshot.price.model != entry.identity.model
            || snapshot.price.provider != entry.identity.endpoint
            || snapshot.raw_sha256 != vcp_protocol::digest_bytes(raw.as_bytes())
            || snapshot.price.capability
                != vcp_protocol::digest_bytes(
                    &vcp_protocol::canonical_bytes(&snapshot.compatibility)
                        .map_err(|error| error.to_string())?,
                )
        {
            return Err("retained endpoint identity or captured source changed".into());
        }
    }
    let mut configuration = retained.clone();
    let mut entries = Vec::new();
    let mut raw_catalogs = BTreeMap::new();
    for entry in &retained.catalog.entries {
        let matches: Vec<_> = fresh
            .iter()
            .filter(|(snapshot, _)| {
                snapshot.compatibility.model == entry.identity.model
                    && snapshot.compatibility.endpoint == entry.identity.endpoint
            })
            .collect();
        if matches.len() > 1 {
            return Err("duplicate metadata for a retained endpoint".into());
        }
        let Some((snapshot, raw)) = matches.first() else {
            continue;
        };
        let mut renewed = entry.clone();
        renewed.snapshot = Some(snapshot.clone());
        renewed.provenance = vec![Provenance {
            source: format!("https://openrouter.ai/api/v1/models/{}/endpoints", entry.identity.model),
            sha256: snapshot.raw_sha256.clone(),
            observed_at: snapshot.observed_at,
            effective_at: None,
            limitations: vec!["Metadata renewal preserves the original owner assignment; no new quality or live qualification claim".into()],
        }];
        entries.push(renewed);
        raw_catalogs.insert(
            snapshot.id.clone(),
            String::from_utf8(raw.clone()).map_err(|_| "provider catalog is not UTF-8")?,
        );
    }
    for assignment in &mut configuration.owner_assignments {
        assignment
            .candidates
            .retain(|identity| entries.iter().any(|entry| &entry.identity == identity));
        if assignment.candidates.is_empty() {
            return Err("a retained role has no eligible original endpoint; ask the owner to choose a new set for a new task".into());
        }
    }
    if let Some(rotation) = &mut configuration.rotation {
        for role in &mut rotation.roles {
            for set in &mut role.sets {
                set.members
                    .retain(|identity| entries.iter().any(|entry| &entry.identity == identity));
            }
            role.sets.retain(|set| !set.members.is_empty());
        }
    }
    configuration.estimates.retain(|estimate| {
        entries
            .iter()
            .any(|entry| entry.identity == estimate.candidate)
    });
    let observed = entries
        .iter()
        .filter_map(|entry| entry.snapshot.as_ref())
        .map(|snapshot| snapshot.observed_at)
        .max()
        .ok_or("no eligible retained endpoints")?;
    configuration.catalog =
        CatalogRevision::create(Some(retained.catalog.id.clone()), observed, None, entries)
            .map_err(|error| error.to_string())?;
    configuration.raw_catalogs = raw_catalogs;
    configuration.validate()?;
    Ok(configuration)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rotation_metadata(model: &str, endpoints: &[(&str, &str)]) -> Vec<(Snapshot, Vec<u8>)> {
        let raw = serde_json::to_vec(
            &json!({"data":{"id":model,"endpoints":endpoints.iter().map(|(tag,prompt)| json!({
            "tag":tag,"status":0,"context_length":32000,"max_completion_tokens":8000,
            "supported_parameters":["tools","tool_choice","max_tokens"],
            "pricing":{"prompt":prompt,"completion":"0.000002"}
        })).collect::<Vec<_>>()}}),
        )
        .unwrap();
        vcp_models::catalog::compatibility::snapshots(&raw, settings::now())
            .unwrap()
            .into_iter()
            .map(|snapshot| (snapshot, raw.clone()))
            .collect()
    }

    #[test]
    fn legacy_preferences_and_commands_do_not_opt_in_to_rotation() {
        use clap::Parser;
        let old = json!({"version":1,"set":balanced(),"budget_usd":"10"});
        let preferences: Preferences = serde_json::from_value(old.clone()).unwrap();
        assert!(preferences.choice_sets.is_empty());
        preferences.validate().unwrap();
        assert_eq!(serde_json::to_value(preferences).unwrap(), old);
        for args in [
            vec!["vcp", "models", "show", "balanced"],
            vec!["vcp", "models", "show", "--refresh"],
            vec![
                "vcp",
                "models",
                "customize",
                "--role",
                "main",
                "--choice",
                "1",
                "--model",
                "fixture/model",
            ],
        ] {
            assert!(crate::args::Cli::try_parse_from(args).is_ok());
        }
        assert!(crate::args::Cli::try_parse_from([
            "vcp",
            "models",
            "customize",
            "--role",
            "main",
            "--choice",
            "4",
            "--model",
            "fixture/model"
        ])
        .is_err());
    }

    #[test]
    fn choices_require_consecutive_distinct_members_and_explicit_reserve_ceiling() {
        let mut preferences = Preferences::default();
        assert!(set_choice(
            &mut preferences,
            "main",
            2,
            vec!["fixture/two".into()],
            None
        )
        .is_err());
        set_choice(
            &mut preferences,
            "main",
            1,
            vec!["fixture/one".into()],
            None,
        )
        .unwrap();
        set_choice(
            &mut preferences,
            "main",
            2,
            vec!["fixture/two".into()],
            None,
        )
        .unwrap();
        let mut duplicate = preferences.clone();
        assert!(set_choice(&mut duplicate, "main", 2, vec!["fixture/one".into()], None).is_err());
        let mut no_ceiling = preferences.clone();
        assert!(set_choice(
            &mut no_ceiling,
            "main",
            3,
            vec!["fixture/three".into()],
            None
        )
        .is_err());
        set_choice(
            &mut preferences,
            "main",
            3,
            vec!["fixture/three".into()],
            Some("0.10".into()),
        )
        .unwrap();
        assert_eq!(preferences.choice_sets["main"].len(), 3);
    }

    #[test]
    fn rotation_captures_endpoint_diversity_and_same_envelope_price_ceiling() {
        let mut preferences = Preferences::default();
        preferences.set.roles = BTreeMap::from([("main".into(), vec!["fixture/one".into()])]);
        set_choice(
            &mut preferences,
            "main",
            1,
            vec!["fixture/one".into(), "fixture/two".into()],
            None,
        )
        .unwrap();
        set_choice(
            &mut preferences,
            "main",
            2,
            vec!["fixture/three".into()],
            Some("0.1".into()),
        )
        .unwrap();
        let mut prepared = rotation_metadata(
            "fixture/one",
            &[
                ("host/first", "0.000001"),
                ("host/second", "0.000001"),
                ("host/expensive", "0.00001"),
            ],
        );
        prepared.extend(rotation_metadata(
            "fixture/two",
            &[("host/other", "0.000001")],
        ));
        prepared.extend(rotation_metadata(
            "fixture/three",
            &[("host/reserve", "0.000002")],
        ));
        let configuration = routing_configuration(&preferences, &prepared).unwrap();
        let rotation = configuration.rotation.as_ref().unwrap();
        let sets = rotation.sets(RequestRole::Main);
        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0].members.len(), 3);
        assert_eq!(
            sets[0]
                .members
                .iter()
                .filter(|identity| identity.model == "fixture/one")
                .count(),
            2
        );
        assert!(sets[0]
            .members
            .iter()
            .all(|identity| identity.endpoint != "host/expensive"));
        assert_eq!(
            sets[0].max_reference_request_cost.micros.get(),
            sets[0].reference_request_cost.micros.get() * 2
        );
        let mut narrowed = preferences.clone();
        narrowed.choice_sets.get_mut("main").unwrap()[0]
            .endpoints
            .insert("fixture/one".into(), vec!["host/second".into()]);
        let restricted = routing_configuration(&narrowed, &prepared).unwrap();
        assert_eq!(preferred_main(&restricted).unwrap().endpoint, "host/second");
        assert_eq!(
            restricted
                .rotation
                .as_ref()
                .unwrap()
                .sets(RequestRole::Main)[0]
                .members
                .len(),
            2
        );
        narrowed.choice_sets.get_mut("main").unwrap()[0]
            .endpoints
            .insert("fixture/outside".into(), vec!["host/unauthorized".into()]);
        assert!(narrowed.validate().is_err());
        let mut price_limited = preferences.clone();
        price_limited.choice_sets.get_mut("main").unwrap()[0]
            .endpoints
            .insert("fixture/one".into(), vec!["host/expensive".into()]);
        price_limited.choice_sets.get_mut("main").unwrap()[0].max_reference_request_cost_usd =
            Some("0.02".into());
        let peer = routing_configuration(&price_limited, &prepared).unwrap();
        assert_eq!(preferred_main(&peer).unwrap().model, "fixture/two");
        price_limited.choice_sets.get_mut("main").unwrap()[0].max_reference_request_cost_usd =
            Some("0.0001".into());
        let later = routing_configuration(&price_limited, &prepared).unwrap();
        assert_eq!(preferred_main(&later).unwrap().model, "fixture/three");
        assert_eq!(
            later.rotation.as_ref().unwrap().sets(RequestRole::Main)[0].id,
            "main-choice-2"
        );
        let mut fresh = prepared.clone();
        fresh.retain(|(snapshot, _)| snapshot.compatibility.endpoint != "host/first");
        fresh.extend(rotation_metadata(
            "fixture/one",
            &[("host/outside", "0.000001")],
        ));
        let renewed = renew_retained_configuration(&configuration, &fresh).unwrap();
        assert_eq!(
            renewed.rotation.as_ref().unwrap().sets(RequestRole::Main)[0]
                .members
                .len(),
            2
        );
        assert!(renewed
            .catalog
            .entries
            .iter()
            .all(|entry| entry.identity.endpoint != "host/outside"));
        assert_eq!(
            configuration
                .rotation
                .as_ref()
                .unwrap()
                .sets(RequestRole::Main)[0]
                .members
                .len(),
            3
        );
        let mut invalid = configuration;
        invalid.rotation.as_mut().unwrap().roles[0].sets[0]
            .members
            .push(ModelEndpoint {
                model: "fixture/one".into(),
                endpoint: "host/outside".into(),
            });
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn renewal_replaces_obsolete_adapter_evidence_without_changing_owner_authority() {
        let raw = serde_json::to_vec(&json!({"data":{"id":"fixture/model","endpoints":[{
            "tag":"fixture/region","status":0,"context_length":32000,
            "max_completion_tokens":8000,"supported_parameters":["tools","tool_choice","max_tokens"],
            "pricing":{"prompt":"0.000001","completion":"0.000002"}
        }]}})).unwrap();
        let current = vcp_models::catalog::compatibility::snapshots(&raw, settings::now())
            .unwrap()
            .remove(0);
        let mut preferences = Preferences::default();
        preferences.set.roles = BTreeMap::from([("main".into(), vec!["fixture/model".into()])]);
        let mut retained =
            routing_configuration(&preferences, &[(current.clone(), raw.clone())]).unwrap();
        retained.policy.input_tokens = Some(Units::new(4096));
        retained.policy.output_tokens = Some(Units::new(512));
        retained.policy = retained.policy.seal().unwrap();
        let original_policy = retained.policy.clone();
        let original_assignments = serde_json::to_value(&retained.owner_assignments).unwrap();
        let old = retained.catalog.entries[0].snapshot.as_mut().unwrap();
        old.compatibility.id =
            format!("openrouter-responses-adapter-contract/1/{}", "0".repeat(64));
        old.id = old.identity_digest().unwrap();
        old.price.id = old.id.clone();
        old.price.capability =
            vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&old.compatibility).unwrap());
        retained.raw_catalogs =
            BTreeMap::from([(old.id.clone(), String::from_utf8(raw.clone()).unwrap())]);
        retained.catalog.id = retained.catalog.digest().unwrap();
        assert!(
            retained.validate().is_err(),
            "old evidence must remain unusable for dispatch"
        );
        let before = serde_json::to_value(&retained).unwrap();
        let renewed =
            renew_retained_configuration(&retained, &[(current.clone(), raw.clone())]).unwrap();
        renewed.validate().unwrap();
        assert_eq!(renewed.policy, original_policy);
        assert_eq!(
            serde_json::to_value(&renewed.owner_assignments).unwrap(),
            original_assignments
        );
        assert_eq!(
            renewed.catalog.entries[0].identity,
            retained.catalog.entries[0].identity
        );
        assert_eq!(renewed.catalog.entries[0].snapshot.as_ref(), Some(&current));
        assert_eq!(renewed.catalog.parent.as_ref(), Some(&retained.catalog.id));
        assert_eq!(serde_json::to_value(&retained).unwrap(), before);
        let mut changed = retained.clone();
        changed.catalog.entries[0].identity.endpoint = "fixture/outside".into();
        changed.catalog.id = changed.catalog.digest().unwrap();
        assert!(renew_retained_configuration(&changed, &[(current.clone(), raw.clone())]).is_err());
        let source_key = retained.raw_catalogs.keys().next().unwrap().clone();
        retained.raw_catalogs.insert(source_key, "{}".into());
        assert!(renew_retained_configuration(&retained, &[(current, raw)]).is_err());
    }

    #[test]
    fn maker_sets_keep_every_role_and_alternative_with_the_advertised_maker() {
        let sets = builtin_sets();
        assert_eq!(sets.first().unwrap(), &balanced());
        for set in sets {
            let preferences = Preferences {
                set,
                ..Preferences::default()
            };
            preferences.validate().unwrap();
            assert_eq!(preferences.set.roles.len(), 8);
            assert!(!preferences.set.rationale.is_empty());
            if preferences.set.maker != "mixed" {
                let prefix = format!("{}/", preferences.set.maker);
                assert!(
                    preferences
                        .set
                        .roles
                        .values()
                        .flatten()
                        .all(|model| model.starts_with(&prefix)),
                    "{} must not silently leave its maker",
                    preferences.set.id
                );
            }
        }
    }

    #[test]
    fn defaults_project_precedence_and_bad_records_are_explicit() {
        let temporary = tempfile::tempdir().unwrap();
        let account = temporary.path().join("account");
        let workspace = temporary.path().join("project");
        std::fs::create_dir(&workspace).unwrap();
        let initial = effective(&account, &workspace).unwrap();
        assert_eq!(initial.set.id, "balanced");
        initial.validate().unwrap();
        let mut defaults = initial.clone();
        defaults.budget_usd = "3".into();
        save(&account, &defaults).unwrap();
        assert_eq!(effective(&account, &workspace).unwrap().budget_usd, "3");
        let mut project = defaults.clone();
        project.budget_usd = "1".into();
        project.set = builtin_sets().remove(1);
        save_project(&account, &workspace, &project).unwrap();
        defaults.budget_usd = "9".into();
        save(&account, &defaults).unwrap();
        assert_eq!(effective(&account, &workspace).unwrap(), project);
        assert_eq!(read(&account).unwrap().unwrap(), defaults);
        // A lower-precedence corrupt record cannot defeat an explicit project
        // selection. Account inspection still reports that corruption itself.
        std::fs::write(account.join("models.json"), b"invalid-account-json").unwrap();
        assert_eq!(effective(&account, &workspace).unwrap(), project);
        assert!(read(&account).is_err());
        std::fs::remove_file(account.join("models.json")).unwrap();
        save(&account, &defaults).unwrap();
        for name in [
            "..",
            ".",
            "/",
            r"C:\",
            "secret:stream",
            "record.json.",
            "record.json ",
            r"other\file",
            "a/b",
        ] {
            assert!(
                save_record(&account, name, &json!({"x":1})).is_err(),
                "{name}"
            );
        }
        let before = std::fs::read(account.join("models.json")).unwrap();
        let mut invalid = defaults;
        invalid
            .set
            .roles
            .get_mut("main")
            .unwrap()
            .push("secret\nvalue".into());
        assert!(save(&account, &invalid).is_err());
        assert_eq!(before, std::fs::read(account.join("models.json")).unwrap());
    }

    #[test]
    #[cfg(windows)]
    fn account_root_rejects_a_junction_before_resolving_its_target() {
        let temporary = tempfile::tempdir().unwrap();
        let base = temporary.path().canonicalize().unwrap();
        let outside = base.join("outside");
        std::fs::create_dir(&outside).unwrap();
        let redirected = base.join("account");
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
        assert!(checked_account_path(&redirected).is_err());
        assert!(checked_account_path(&redirected.join("new-account")).is_err());
        assert!(ensure_root(&redirected.join("new-account")).is_err());
        assert!(!outside.join("new-account").exists());
        std::fs::remove_dir(redirected).unwrap();
    }

    fn fixture(workspace: &Path, root: &Path) -> settings::Profile {
        let observed = settings::now();
        let raw=serde_json::to_vec(&json!({"data":{"id":"fixture/model","endpoints":[{"tag":"fixture","status":0,"context_length":32000,"max_completion_tokens":8000,"supported_parameters":["tools"],"pricing":{"prompt":"0.000001","completion":"0.000002"}}]}})).unwrap();
        let snapshot = Snapshot::from_endpoints(
            &raw,
            observed,
            vcp_domain::Timestamp::new(observed.get() + 60000),
            vcp_models::catalog::Compatibility {
                id: "offline-preferences-test".into(),
                model: "fixture/model".into(),
                endpoint: "fixture".into(),
                qualified_at: observed,
                valid_until: vcp_domain::Timestamp::new(observed.get() + 60000),
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
        std::fs::write(root.join("catalog.json"), &raw).unwrap();
        serde_json::from_value(json!({"version":1,"workspace":workspace,"trust_workspace":true,"maximum_autonomy":"ask","automatic_effects":["read"],"budget_usd":"1","provider":snapshot,"catalog":root.join("catalog.json"),"affected_paths":["README.md"],"max_requests":8,"deadline_seconds":300,"processes":[],"checks":[]})).unwrap()
    }

    #[tokio::test]
    #[cfg(windows)]
    async fn explicit_profile_precedes_account_preferences_without_network_or_credentials() {
        use clap::Parser;
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let profile = fixture(&workspace, temporary.path());
        let path = temporary.path().join("explicit.json");
        std::fs::write(&path, serde_json::to_vec(&profile).unwrap()).unwrap();
        let mut cli = crate::args::Cli::try_parse_from([
            "vcp",
            "run",
            "--budget-usd",
            "1",
            "--autonomy",
            "ask",
            "fix fixture",
        ])
        .unwrap();
        cli.config = Some(path);
        let loaded = load_for_command(
            &cli,
            temporary.path(),
            &temporary.path().join("state"),
            &workspace,
        )
        .await
        .unwrap();
        assert_eq!(loaded.provider.compatibility.model, "fixture/model");
    }

    #[test]
    fn existing_task_selection_is_immutable_when_defaults_change() {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().join("project");
        std::fs::create_dir(&workspace).unwrap();
        let mut profile = fixture(&workspace, temporary.path());
        let task = vcp_domain::TaskId::new();
        let directory = temporary.path().join("tasks");
        retain_task(&directory, &task, &profile).unwrap();
        let before = read_record(&directory, &format!("task-models-{task}.json")).unwrap();
        save(&temporary.path().join("account"), &Preferences::default()).unwrap();
        retain_task(&directory, &task, &profile).unwrap();
        profile.provider.compatibility.model = "fixture/another-model".into();
        assert!(retain_task(&directory, &task, &profile).is_err());
        assert_eq!(
            before,
            read_record(&directory, &format!("task-models-{task}.json")).unwrap()
        );
    }

    #[test]
    fn retained_models_do_not_restore_revoked_trust_or_previous_tool_permissions() {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().join("project");
        std::fs::create_dir(&workspace).unwrap();
        let retained = fixture(&workspace, temporary.path());
        let mut current = fixture(&workspace, temporary.path());
        current.trust_workspace = false;
        current.maximum_autonomy = vcp_domain::policy::Autonomy::Plan;
        current.automatic_effects.clear();
        current.canonical_tools = serde_json::from_value(json!(["vcp_read"])).unwrap();
        let merged =
            with_retained_models(current, RetainedModels::from_profile(&retained)).unwrap();
        assert!(!merged.trust_workspace);
        assert_eq!(merged.maximum_autonomy, vcp_domain::policy::Autonomy::Plan);
        assert!(merged.automatic_effects.is_empty());
        assert!(!merged.canonical_tools.contains("vcp_patch"));
        assert_eq!(merged.provider.compatibility.model, "fixture/model");
        assert!(merged.prepare(vcp_domain::policy::Autonomy::Plan).is_err());
    }

    fn routed_fixture(workspace: &Path, root: &Path) -> settings::Profile {
        let mut profile = fixture(workspace, root);
        let raw = std::fs::read(&profile.catalog).unwrap();
        let mut alternative_raw: Value = serde_json::from_slice(&raw).unwrap();
        alternative_raw["data"]["endpoints"][0]["tag"] = json!("fixture/alternative");
        let alternative_raw = serde_json::to_vec(&alternative_raw).unwrap();
        let mut compatibility = profile.provider.compatibility.clone();
        compatibility.endpoint = "fixture/alternative".into();
        let alternative = Snapshot::from_endpoints(
            &alternative_raw,
            profile.provider.observed_at,
            profile.provider.valid_until,
            compatibility,
        )
        .unwrap();
        let mut preferences = Preferences::default();
        preferences.set.roles = BTreeMap::from([("main".into(), vec!["fixture/model".into()])]);
        profile.routing = Some(
            routing_configuration(
                &preferences,
                &[
                    (profile.provider.clone(), raw),
                    (alternative, alternative_raw),
                ],
            )
            .unwrap(),
        );
        profile
    }

    #[test]
    fn retained_assignments_obey_current_privacy_and_resource_ceilings() {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().join("project");
        std::fs::create_dir(&workspace).unwrap();
        let mut retained = routed_fixture(&workspace, temporary.path());
        let routing = retained.routing.as_mut().unwrap();
        routing.policy.deny_data_collection = false;
        routing.policy.input_tokens = Some(Units::new(1024));
        routing.policy.output_tokens = Some(Units::new(1024));
        routing.policy.retrieval_limits = Some(RetrievalLimits {
            results: 8,
            tokens: Units::new(512),
            bytes: vcp_domain::ByteCount::new(1024),
        });
        routing.policy = routing.policy.clone().seal().unwrap();
        let original = serde_json::to_value(&retained).unwrap();

        let mut current = routed_fixture(&workspace, temporary.path());
        let ceiling = current.routing.as_mut().unwrap();
        ceiling.owner_assignments[0].candidates.reverse();
        ceiling.policy.allowed_models = BTreeSet::from(["fixture/future-model".into()]);
        ceiling.policy.allowed_endpoints = BTreeSet::from(["fixture/future-endpoint".into()]);
        ceiling.policy.require_zdr = true;
        ceiling.policy.input_tokens = Some(Units::new(256));
        ceiling.policy.output_tokens = Some(Units::new(512));
        ceiling.policy.retrieval_limits = Some(RetrievalLimits {
            results: 16,
            tokens: Units::new(128),
            bytes: vcp_domain::ByteCount::new(2048),
        });
        ceiling.policy = ceiling.policy.clone().seal().unwrap();
        current.max_requests = 2;
        current.max_transport_retries = 0;
        current.deadline_seconds = 30;
        current.output_tokens = Some(Units::new(128));

        let merged =
            with_retained_models(current, RetainedModels::from_profile(&retained)).unwrap();
        assert_eq!(merged.max_requests, 2);
        assert_eq!(merged.max_transport_retries, 0);
        assert_eq!(merged.deadline_seconds, 30);
        assert_eq!(merged.output_tokens, Some(Units::new(128)));
        let effective = merged.routing.as_ref().unwrap();
        effective.validate().unwrap();
        let actual = serde_json::to_value(&merged).unwrap();
        for field in ["provider", "catalog"] {
            assert_eq!(actual[field], original[field]);
        }
        for field in ["owner_assignments", "catalog", "estimates", "raw_catalogs"] {
            assert_eq!(actual["routing"][field], original["routing"][field]);
        }
        assert_eq!(
            effective.policy.allowed_models,
            retained.routing.as_ref().unwrap().policy.allowed_models
        );
        assert_eq!(
            effective.policy.allowed_endpoints,
            retained.routing.as_ref().unwrap().policy.allowed_endpoints
        );
        assert!(effective.policy.deny_data_collection);
        assert!(effective.policy.require_zdr);
        assert_eq!(effective.policy.input_tokens, Some(Units::new(256)));
        assert_eq!(effective.policy.output_tokens, Some(Units::new(512)));
        assert_eq!(
            effective.policy.retrieval_limits,
            Some(RetrievalLimits {
                results: 8,
                tokens: Units::new(128),
                bytes: vcp_domain::ByteCount::new(1024),
            })
        );
        let mut input = RoutingInput {
            workspace: vcp_domain::WorkspaceId::parse("workspace").unwrap(),
            root: vcp_domain::TaskId::parse("task").unwrap(),
            task: vcp_domain::TaskId::parse("task").unwrap(),
            input_revision: vcp_domain::revision::Revision::ZERO,
            steering: vcp_domain::revision::SteeringRevision::ZERO,
            input_digest: "a".repeat(64),
            catalog: effective.catalog.id.clone(),
            policy: effective.policy.id.clone(),
            role: RequestRole::Main,
            task_class: effective.task_class.clone(),
            now: retained.provider.observed_at,
            required_capabilities: BTreeSet::from(["responses_text_tools".into()]),
            excluded: BTreeSet::new(),
            retry_pin: None,
            input_tokens: Units::new(1),
            output_tokens: Units::new(1),
            available: Money {
                currency: "USD".to_owned().try_into().unwrap(),
                micros: Micros::new(10_000_000),
            },
            protected_verification: Micros::ZERO,
            estimates: effective.estimates.clone(),
        };
        let ordered = &effective.owner_assignments[0].candidates;
        let mut previous_privacy = effective.policy.clone();
        previous_privacy.require_zdr = false;
        previous_privacy = previous_privacy.seal().unwrap();
        input.policy = previous_privacy.id.clone();
        let before =
            select_owner_set(&effective.catalog, &previous_privacy, &input, ordered).unwrap();
        assert_eq!(before.selected.as_ref(), ordered.first());
        input.policy = effective.policy.id.clone();
        let decision =
            select_owner_set(&effective.catalog, &effective.policy, &input, ordered).unwrap();
        assert!(decision.selected.is_none());
        assert!(decision
            .candidates
            .iter()
            .all(|candidate| candidate.exclusions.contains(&Exclusion::DataPolicy)));
        input.input_tokens = Units::new(257);
        assert!(select_owner_set(&effective.catalog, &effective.policy, &input, ordered).is_err());
        input.input_tokens = Units::new(1);
        input.output_tokens = Units::new(513);
        assert!(select_owner_set(&effective.catalog, &effective.policy, &input, ordered).is_err());

        // Looser future defaults cannot remove the task's already captured limits.
        let relaxed = routed_fixture(&workspace, temporary.path());
        let reloaded =
            with_retained_models(relaxed, RetainedModels::from_profile(&merged)).unwrap();
        assert_eq!(reloaded.routing.as_ref().unwrap().policy, effective.policy);
        assert_eq!(serde_json::to_value(&retained).unwrap(), original);
    }

    #[test]
    fn retained_fixed_model_fails_closed_for_new_privacy_or_unrepresentable_limits() {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().join("project");
        std::fs::create_dir(&workspace).unwrap();
        let retained = fixture(&workspace, temporary.path());
        let mut current = fixture(&workspace, temporary.path());
        current.provider.compatibility.require_zdr = true;
        assert!(
            with_retained_models(current, RetainedModels::from_profile(&retained))
                .err()
                .unwrap()
                .contains("current privacy policy")
        );

        for retrieval in [false, true] {
            let mut current = routed_fixture(&workspace, temporary.path());
            let policy = &mut current.routing.as_mut().unwrap().policy;
            if retrieval {
                policy.retrieval_limits = Some(RetrievalLimits::default());
            } else {
                policy.input_tokens = Some(Units::new(256));
            }
            *policy = policy.clone().seal().unwrap();
            assert!(
                with_retained_models(current, RetainedModels::from_profile(&retained))
                    .err()
                    .unwrap()
                    .contains("current routing resource ceilings")
            );
        }
        let mut current = routed_fixture(&workspace, temporary.path());
        let policy = &mut current.routing.as_mut().unwrap().policy;
        policy.output_tokens = Some(Units::new(512));
        *policy = policy.clone().seal().unwrap();
        current.output_tokens = Some(Units::new(256));
        let merged =
            with_retained_models(current, RetainedModels::from_profile(&retained)).unwrap();
        assert_eq!(merged.output_tokens, Some(Units::new(256)));
        assert!(merged.routing.is_none());
    }

    #[test]
    fn metadata_narrowing_uses_original_fallback_without_rewriting_preferences() {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let profile = fixture(&workspace, temporary.path());
        let raw = std::fs::read(&profile.catalog).unwrap();
        let prepared = vec![(profile.provider.clone(), raw)];
        let mut preferences = Preferences::default();
        preferences.set.roles = BTreeMap::from([
            (
                "main".into(),
                vec!["fixture/unavailable".into(), "fixture/model".into()],
            ),
            ("helper".into(), vec!["fixture/model".into()]),
        ]);
        let before = preferences.clone();
        let output = publish_profile(
            temporary.path(),
            &workspace,
            serde_json::to_value(profile).unwrap(),
            &preferences,
            &prepared,
        )
        .unwrap();
        let selected = settings::load(&output, &workspace).unwrap();
        assert_eq!(selected.provider.compatibility.model, "fixture/model");
        assert_eq!(preferences, before);
        let routing = selected.routing.unwrap();
        assert_eq!(routing.owner_assignments[1].candidates.len(), 1);
        assert!(routing
            .owner_assignments
            .iter()
            .all(|role| role.candidates.iter().all(|id| id.model == "fixture/model")));
        let mut denied = preferences.clone();
        denied
            .set
            .roles
            .insert("reviewer".into(), vec!["fixture/unavailable".into()]);
        assert!(routing_configuration(&denied, &prepared).is_err());
        let outsider = Preferences::default();
        assert!(routing_configuration(&outsider, &prepared).is_err());
    }

    #[test]
    fn metadata_renewal_preserves_exact_role_endpoints_and_policy_restrictions() {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let profile = fixture(&workspace, temporary.path());
        let raw = std::fs::read(&profile.catalog).unwrap();
        let mut other_raw: Value = serde_json::from_slice(&raw).unwrap();
        other_raw["data"]["endpoints"][0]["tag"] = json!("fixture/other");
        let other_raw = serde_json::to_vec(&other_raw).unwrap();
        let mut compatibility = profile.provider.compatibility.clone();
        compatibility.endpoint = "fixture/other".into();
        let second = Snapshot::from_endpoints(
            &other_raw,
            profile.provider.observed_at,
            profile.provider.valid_until,
            compatibility,
        )
        .unwrap();
        let prepared = vec![(profile.provider.clone(), raw), (second, other_raw)];
        let mut preferences = Preferences::default();
        preferences.set.roles = BTreeMap::from([
            ("main".into(), vec!["fixture/model".into()]),
            ("helper".into(), vec!["fixture/model".into()]),
        ]);
        let mut configuration = routing_configuration(&preferences, &prepared).unwrap();
        let primary = ModelEndpoint {
            model: "fixture/model".into(),
            endpoint: "fixture".into(),
        };
        let helper = ModelEndpoint {
            model: "fixture/model".into(),
            endpoint: "fixture/other".into(),
        };
        for assignment in &mut configuration.owner_assignments {
            assignment.candidates = vec![if assignment.role == RequestRole::Main {
                primary.clone()
            } else {
                helper.clone()
            }];
        }
        configuration.policy.require_zdr = true;
        configuration.policy.output_tokens = Some(Units::new(100));
        configuration.policy = configuration.policy.seal().unwrap();
        let expected = serde_json::to_value(&configuration).unwrap();
        let renewed = renew_retained_configuration(&configuration, &prepared).unwrap();
        assert_eq!(
            serde_json::to_value(&renewed.owner_assignments).unwrap(),
            expected["owner_assignments"]
        );
        assert_eq!(renewed.policy, configuration.policy);
        assert_eq!(
            renewed.catalog.parent,
            Some(configuration.catalog.id.clone())
        );
        assert_eq!(serde_json::to_value(&configuration).unwrap(), expected);
        // Another role's endpoint for the same model cannot become a fallback.
        assert!(renew_retained_configuration(&configuration, &prepared[1..]).is_err());
    }

    #[test]
    fn revisiting_project_setup_preserves_existing_processes_and_capability_ceilings() {
        let temporary = tempfile::tempdir().unwrap();
        let account = temporary.path().join("account");
        ensure_root(&account).unwrap();
        let workspace = temporary.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let mut profile = fixture(&workspace, &account);
        profile.maximum_autonomy = vcp_domain::policy::Autonomy::Plan;
        profile.canonical_tools = serde_json::from_value(json!(["vcp_read"])).unwrap();
        profile.max_requests = 3;
        profile.processes = vec![serde_json::from_value(json!({
            "name":"existing-check", "executable":std::env::current_exe().unwrap(),
            "environment":{}, "required_isolation":[], "reduced_isolation":true, "inputs":[]
        }))
        .unwrap()];
        profile.checks = vec![serde_json::from_value(json!({
            "manifest":"package.json", "runner":"node", "profile":"existing-check",
            "expected_tests":["acceptance"], "rationale":"Keep existing validation"
        }))
        .unwrap()];
        let profile_path = account.join("existing-profile.json");
        let before = serde_json::to_value(&profile).unwrap();
        std::fs::write(&profile_path, serde_json::to_vec(&before).unwrap()).unwrap();
        save_record(
            &account,
            &format!("project-profile-{}.json", project_id(&workspace).unwrap()),
            &json!({"profile":profile_path,"workspace":workspace}),
        )
        .unwrap();
        let mut preferences = Preferences::default();
        preferences.budget_usd = "7".into();
        let revised =
            project_setup_value(&account, &workspace, &["src/main.rs".into()], &preferences)
                .unwrap();
        for field in [
            "processes",
            "checks",
            "canonical_tools",
            "maximum_autonomy",
            "max_requests",
        ] {
            assert_eq!(revised[field], before[field], "{field}");
        }
        assert_eq!(revised["budget_usd"], "7");
        assert_eq!(revised["affected_paths"], json!(["src/main.rs"]));
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(profile_path).unwrap()).unwrap(),
            before
        );
    }
}
