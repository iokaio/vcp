// SPDX-License-Identifier: Apache-2.0
//! Per-user setup. No workspace or inference is required to display help.
mod prompt;
use crate::{
    args::{Cli, Format},
    credential,
    model_preferences::{self, ModelSet, Preferences},
    settings,
};
use clap::CommandFactory;
use prompt::{answer, yes, Prompter};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::IsTerminal,
    path::{Path, PathBuf},
};

const COMPLETE: &str = "setup-complete.json";
const PENDING: &str = "connection-pending.json";

pub fn interactive(cli: &Cli) -> bool {
    cli.format == Format::Text
        && !cli.non_interactive
        && !cli.control_stdin
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && std::io::stderr().is_terminal()
}

pub fn help() -> Result<u8, String> {
    let program = std::env::args_os().next().unwrap_or_else(|| "vcp".into());
    match Cli::command().try_get_matches_from([program, "--help".into()]) {
        Err(error) if error.kind() == clap::error::ErrorKind::DisplayHelp => {
            error.print().map_err(|_| "help output unavailable")?;
            Ok(0)
        }
        _ => Err("CLI help could not be rendered".into()),
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Completion {
    version: u32,
    connection: Value,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    directory: String,
    credential_sha256: String,
    model: String,
    endpoint: String,
}

pub fn completed(root: &Path) -> Result<bool, String> {
    let Some(value) = model_preferences::read_record(root, COMPLETE)? else {
        return Ok(false);
    };
    let completion: Completion = serde_json::from_value(value)
        .map_err(|_| "invalid setup completion record; use vcp setup to repair configuration")?;
    Ok(completion.version == 1 && completion.connection["status"] == "connected")
}

pub async fn first_launch(cli: &Cli) -> Result<u8, String> {
    if !interactive(cli) {
        return help();
    }
    if completed(&model_preferences::account_root()?)? {
        return help();
    }
    run(cli).await
}

pub async fn run(cli: &Cli) -> Result<u8, String> {
    if !interactive(cli) {
        return Err("guided setup requires an interactive Windows terminal; run vcp setup --help for explicit commands".into());
    }
    let result: Result<u8, String> = async {
        let root = model_preferences::account_root()?;
        model_preferences::ensure_root(&root)?;
        let registry = settings::registry_root(&root)?;
        let _pin = registry
            .hold(None, true)
            .map_err(|_| "account setup directory redirected")?;
        let _lease = setup_lease(&root)?;
        // Another process may have finished between first_launch's observation and
        // our lease acquisition. Explicit `vcp setup` still revisits configuration.
        if cli.command.is_none() && completed(&root)? {
            return help();
        }
        let mut backend = Native {
            root,
            prepared: None,
        };
        interview(&mut prompt::Console, &mut backend).await?;
        Ok(0)
    }
    .await;
    attended_result(result)
}

fn attended_result(result: Result<u8, String>) -> Result<u8, String> {
    match result {
        Ok(code) => Ok(code),
        Err(error) => {
            eprintln!("vcp: {error}");
            Ok(2)
        }
    }
}

pub fn credential(
    command: &crate::onboarding::CredentialCommand,
    cli: &Cli,
) -> Result<Value, String> {
    use crate::onboarding::CredentialCommand;
    match command {
        CredentialCommand::Environment { name } => {
            credential::select_environment(name)?;
            Ok(
                json!({"source":"environment","environment":credential::environment_name()?,"model_calls":0}),
            )
        }
        CredentialCommand::Status => Ok(
            json!({"source":credential::source(interactive(cli)),"environment":credential::environment_name()?,"model_calls":0}),
        ),
        CredentialCommand::Store => {
            if !interactive(cli) {
                return Err("credential store requires an interactive Windows terminal".into());
            }
            #[cfg(windows)]
            {
                let secret = crate::console_secret::read("OpenRouter API key (hidden): ")?
                    .ok_or("credential entry cancelled")?;
                credential::store(credential::TARGET, &secret)?;
                credential::select_stored()?;
                Ok(json!({"status":"stored","source":"credential_manager","model_calls":0}))
            }
            #[cfg(not(windows))]
            Err("credential storage requires Windows".into())
        }
        CredentialCommand::Remove => {
            #[cfg(windows)]
            {
                credential::remove(credential::TARGET)?;
                Ok(json!({"status":"removed","model_calls":0}))
            }
            #[cfg(not(windows))]
            Err("credential storage requires Windows".into())
        }
    }
}

trait Backend {
    fn preferences(&self) -> Result<Option<Preferences>, String>;
    fn has_key(&self) -> Result<bool, String>;
    fn set_key(&mut self, key: credential::Secret, store: bool) -> Result<(), String>;
    fn environment_name(&self) -> Result<String, String>;
    fn use_environment(&mut self, name: &str) -> Result<(), String>;
    async fn prepare(&mut self, preferences: &Preferences) -> Result<(String, u64), String>;
    async fn test(&mut self, budget: &str) -> Result<Value, String>;
    async fn finish(
        &mut self,
        prefs: &Preferences,
        project: Option<&Path>,
        affected_path: &Path,
        response: &Value,
    ) -> Result<(), String>;
}

fn display_set(prompt: &mut impl Prompter, set: &ModelSet) {
    prompt.say(&format!(
        "{} ({}) — maker: {}; project type: {}",
        set.label, set.id, set.maker, set.project_type
    ));
    if !set.rationale.is_empty() {
        prompt.say(&set.rationale);
    }
    for (role, models) in &set.roles {
        prompt.say(&format!("  {}: {}", role_label(role), models.join(" -> ")));
    }
    prompt.say("Alternatives are tried only within these assignments and the task budget. Other models require permission.");
}

fn role_label(role: &str) -> &str {
    match role {
        "main" => "Coding",
        "child" => "Delegated coding",
        "helper" => "Investigation and support",
        "compaction" => "Conversation summaries",
        "reviewer" => "Code review",
        "verification" => "Verification assistance",
        "optimizer" => "Optimization advice",
        "memory" => "Memory assistance",
        _ => role,
    }
}

fn choose_set(prompt: &mut impl Prompter, mut set: ModelSet) -> Result<ModelSet, String> {
    loop {
        display_set(prompt, &set);
        match answer(
            prompt,
            "Use this set [Enter], choose another set [choose], or customize [customize]: ",
            "use",
        )?
        .as_str()
        {
            "use" => {
                match (Preferences {
                    version: 1,
                    set: set.clone(),
                    budget_usd: "1".into(),
                })
                .validate()
                {
                    Ok(()) => return Ok(set),
                    Err(error) => prompt.say(&format!(
                        "{error}. Choose another set or customize these selections."
                    )),
                }
            }
            "choose" => {
                let available = model_preferences::builtin_sets();
                let makers: std::collections::BTreeSet<_> =
                    available.iter().map(|s| s.maker.as_str()).collect();
                let projects: std::collections::BTreeSet<_> =
                    available.iter().map(|s| s.project_type.as_str()).collect();
                prompt.say(&format!(
                    "Makers: {}",
                    makers.into_iter().collect::<Vec<_>>().join(", ")
                ));
                prompt.say(&format!(
                    "Project types: {}",
                    projects.into_iter().collect::<Vec<_>>().join(", ")
                ));
                let maker = answer(prompt, "Filter by maker (Enter for all): ", "")?;
                let project = answer(
                    prompt,
                    "Filter by coding-project type (Enter for all): ",
                    "",
                )?;
                let sets: Vec<_> = model_preferences::builtin_sets()
                    .into_iter()
                    .filter(|set| {
                        set.maker.to_lowercase().contains(&maker.to_lowercase())
                            && set
                                .project_type
                                .to_lowercase()
                                .contains(&project.to_lowercase())
                    })
                    .collect();
                if sets.is_empty() {
                    prompt.say("No sets match those filters. Choose again with another filter, or leave filters empty.");
                    continue;
                }
                for (index, candidate) in sets.iter().enumerate() {
                    prompt.say(&format!(
                        "  {}. {} [{}] — {}; {}",
                        index + 1,
                        candidate.label,
                        candidate.id,
                        candidate.maker,
                        candidate.project_type
                    ));
                }
                loop {
                    let id = answer(prompt, "Set number or ID: ", "")?;
                    let chosen = sets
                        .iter()
                        .find(|candidate| candidate.id == id)
                        .or_else(|| {
                            id.parse::<usize>()
                                .ok()
                                .and_then(|n| n.checked_sub(1))
                                .and_then(|n| sets.get(n))
                        });
                    if let Some(chosen) = chosen {
                        set = chosen.clone();
                        break;
                    }
                    prompt.say("Choose a displayed set number or ID, or cancel to stop.");
                }
            }
            "customize" => {
                let mut customized = set.clone();
                for (role, models) in &mut customized.roles {
                    let value = answer(
                        prompt,
                        &format!(
                            "{} model IDs, in fallback order, comma separated [Enter keeps {}]: ",
                            role_label(role),
                            models.join(",")
                        ),
                        "",
                    )?;
                    if !value.is_empty() {
                        *models = value.split(',').map(|s| s.trim().to_owned()).collect();
                    }
                }
                customized.id = "custom".into();
                customized.label = "Custom model selections".into();
                customized.rationale =
                    "Your selected models and ordered alternatives for each work role.".into();
                match (Preferences {
                    version: 1,
                    set: customized.clone(),
                    budget_usd: "1".into(),
                })
                .validate()
                {
                    Ok(()) => set = customized,
                    Err(error) => prompt.say(&format!(
                        "{error}. Previous selections were kept; choose customize to try again."
                    )),
                }
            }
            _ => prompt.say("Choose use, choose, or customize; type cancel to stop."),
        }
    }
}

fn choose_credential(prompt: &mut impl Prompter, backend: &mut impl Backend) -> Result<(), String> {
    let available = match backend.has_key() {
        Ok(available) => available,
        Err(error) => {
            prompt.say(&error);
            false
        }
    };
    if available {
        prompt.say("An OpenRouter credential is available. Its value is hidden.");
        loop {
            match answer(
                prompt,
                "Keep this credential [Enter], or change its source [change]: ",
                "keep",
            )?
            .as_str()
            {
                "keep" => return Ok(()),
                "change" => break,
                _ => prompt.say("Enter keep or change, or cancel to stop."),
            }
        }
    }
    loop {
        prompt.say("Credential sources: 1. Windows protected storage  2. Environment variable");
        match answer(prompt, "Credential source [1]: ", "1")?
            .to_ascii_lowercase()
            .as_str()
        {
            "1" | "store" => {
                let key = prompt
                    .secret("OpenRouter API key (hidden; cancel stops setup): ")?
                    .filter(|key| !key.expose().eq_ignore_ascii_case("cancel"))
                    .ok_or("setup interrupted during credential entry")?;
                if !yes(prompt, "Save this key in Windows Credential Manager for your account on this computer? [y/N]: ")? {
                    prompt.say("Key was not saved. Choose a credential source to continue.");
                    continue;
                }
                backend.set_key(key, true)?;
                return Ok(());
            }
            "2" | "env" | "environment" => {
                let default = backend
                    .environment_name()
                    .unwrap_or_else(|_| credential::ENVIRONMENT.into());
                loop {
                    let name = answer(
                        prompt,
                        &format!(
                            "Environment variable name [{default}] (enter its name, not the key): "
                        ),
                        &default,
                    )?;
                    match backend.use_environment(&name) {
                        Ok(()) => {
                            prompt.say("Using the designated environment variable. Its value is not stored by VCP.");
                            return Ok(());
                        }
                        Err(error) => prompt.say(&error),
                    }
                }
            }
            _ => prompt
                .say("Choose 1 for Windows protected storage or 2 for an environment variable."),
        }
    }
}

/// Reuse the setup chooser for future-task selections without credentials or inference.
pub fn models(cli: &Cli) -> Result<u8, String> {
    let root = model_preferences::account_root()?;
    attended_result(models_interview(&mut prompt::Console, &root, &cli.workspace).map(|()| 0))
}

fn models_interview(
    prompt: &mut impl Prompter,
    root: &Path,
    workspace: &Path,
) -> Result<(), String> {
    prompt.say("Change model selections for future tasks. Existing tasks retain their selections.");
    let project = loop {
        match answer(
            prompt,
            "Change account defaults [Enter] or this project [project]: ",
            "account",
        )?
        .as_str()
        {
            "account" => break false,
            "project" => break true,
            _ => prompt.say("Enter account or project, or cancel to stop."),
        }
    };
    let mut prefs = if project {
        prompt.say(&format!(
            "Project: {}",
            workspace
                .canonicalize()
                .map_err(|_| "project folder is unavailable")?
                .display()
        ));
        model_preferences::effective(root, workspace)?
    } else {
        model_preferences::read(root)?.unwrap_or_default()
    };
    prefs.set = choose_set(prompt, prefs.set)?;
    prefs.validate()?;
    if !yes(
        prompt,
        "Save these model selections for future tasks? [y/N]: ",
    )? {
        prompt.say("Model selections were kept unchanged.");
        return Ok(());
    }
    if project {
        model_preferences::save_project(root, workspace, &prefs)?;
    } else {
        model_preferences::save(root, &prefs)?;
    }
    prompt.say("Model selections saved. No model calls were made.");
    Ok(())
}

async fn interview(prompt: &mut impl Prompter, backend: &mut impl Backend) -> Result<(), String> {
    prompt.say("VCP setup — balanced quality and cost. Project selection is optional. Type cancel or press Ctrl+C to stop.");
    choose_credential(prompt, backend)?;
    let previous = backend.preferences()?;
    let initial = previous
        .as_ref()
        .map(|prefs| prefs.set.clone())
        .unwrap_or_else(|| model_preferences::builtin_sets().remove(0));
    let set = choose_set(prompt, initial)?;
    let default_budget = previous
        .as_ref()
        .map_or("10.00", |prefs| prefs.budget_usd.as_str());
    let budget = budget_answer(
        prompt,
        &format!("Default per-task budget in USD [{default_budget}]: "),
        default_budget,
        1,
        u64::MAX,
    )?;
    #[cfg(windows)]
    let mut _project_pin = None;
    let project = if yes(prompt, "Configure a project now? [y/N]: ")? {
        let path = PathBuf::from(answer(prompt, "Existing project folder: ", "")?);
        let path = path
            .canonicalize()
            .map_err(|_| "project folder is unavailable")?;
        if !path.is_dir() {
            return Err("project must be a directory".into());
        }
        // Bind the trust answer to this directory through the asynchronous
        // metadata/test/configuration steps, including its ancestor namespace.
        #[cfg(windows)]
        {
            _project_pin = Some(
                settings::registry_root(&path)?
                    .hold(None, true)
                    .map_err(|_| "project root is unavailable or redirected")?,
            );
        }
        if !yes(
            prompt,
            &format!("Trust project {} for VCP tasks? [y/N]: ", path.display()),
        )? {
            return Err("project trust was not granted".into());
        }
        Some(path)
    } else {
        None
    };
    let prefs = Preferences {
        version: 1,
        set,
        budget_usd: budget,
    };
    prefs.validate()?;
    let affected_path = if project.is_some() {
        PathBuf::from(answer(
            prompt,
            "Project path to work on, relative to the project [README.md]: ",
            "README.md",
        )?)
    } else {
        PathBuf::from("README.md")
    };
    vcp_repository::path::relative(&affected_path).map_err(|e| e.to_string())?;
    prompt.say("Checking current availability and pricing for the selected models...");
    let (model, reservation) = backend.prepare(&prefs).await?;
    prompt.say(&format!("Connection test: one short prompt to {model}, at most 128 output tokens, no inference retries. Conservative reservation: ${}. This checks this account's text response and reported cost; it does not test every role, model, tool, or coding quality.", usd(reservation)));
    let cap = budget_answer(
        prompt,
        &format!("Test budget in USD [{}], maximum 25: ", usd(reservation)),
        &usd(reservation),
        reservation.max(1),
        25_000_000,
    )?;
    if !yes(prompt, "Run this one budgeted connection test? [y/N]: ")? {
        return Err("connection test declined; setup remains incomplete".into());
    }
    prompt.say("Running the connection test, or recovering its completed result...");
    let report = backend.test(&cap).await?;
    if report["status"] != "connected"
        || report["response"].as_str().is_none_or(|v| v.is_empty())
        || report["reported_cost_micros"].as_u64().is_none()
    {
        return Err("connection test did not produce a successful accounted response".into());
    }
    // JSON escaping makes provider-controlled terminal sequences visible text.
    prompt.say(&format!(
        "Response: {}\nModel: {}\nReported cost: ${}",
        report["response"],
        report["model"],
        usd(report["reported_cost_micros"].as_u64().unwrap_or_default())
    ));
    backend
        .finish(&prefs, project.as_deref(), &affected_path, &report)
        .await?;
    prompt.say("Setup complete. Use vcp models to inspect or change future selections, and vcp setup to revisit configuration.");
    Ok(())
}

fn usd(micros: u64) -> String {
    format!("{}.{:06}", micros / 1_000_000, micros % 1_000_000)
}

fn budget_answer(
    prompt: &mut impl Prompter,
    message: &str,
    default: &str,
    minimum: u64,
    maximum: u64,
) -> Result<String, String> {
    if minimum > maximum {
        return Err("the selected test model requires more than the permitted setup budget; choose a different set".into());
    }
    loop {
        let value = answer(prompt, message, default)?;
        match crate::args::parse_usd(&value) {
            Ok(amount) if amount.get() >= minimum && amount.get() <= maximum => return Ok(value),
            _ => prompt.say(&format!(
                "Enter a positive USD amount of at least ${}{}; cancel stops setup.",
                usd(minimum),
                if maximum == u64::MAX {
                    String::new()
                } else {
                    format!(" and at most ${}", usd(maximum))
                }
            )),
        }
    }
}

struct Native {
    root: PathBuf,
    prepared: Option<crate::provider_setup::connection::PreparedModel>,
}

impl Backend for Native {
    fn preferences(&self) -> Result<Option<Preferences>, String> {
        model_preferences::read(&self.root)
    }
    fn has_key(&self) -> Result<bool, String> {
        Ok(credential::openrouter(true)?.is_some())
    }
    fn set_key(&mut self, key: credential::Secret, store: bool) -> Result<(), String> {
        if store {
            #[cfg(windows)]
            credential::store(credential::TARGET, &key)?;
            #[cfg(not(windows))]
            return Err("credential storage requires Windows".into());
        }
        credential::select_stored()?;
        credential::set_session(key)
    }
    fn environment_name(&self) -> Result<String, String> {
        credential::environment_name()
    }
    fn use_environment(&mut self, name: &str) -> Result<(), String> {
        credential::environment_key(name)?.ok_or(
            "that environment variable is not set; set it in the terminal that launches VCP",
        )?;
        credential::select_environment(name)
    }
    async fn prepare(&mut self, preferences: &Preferences) -> Result<(String, u64), String> {
        let key = credential::require(true)?;
        let mut refreshed = model_preferences::refresh_set(preferences, key.expose()).await?;
        let selected = preferences
            .set
            .roles
            .get("main")
            .ok_or("main model required")?
            .iter()
            .find_map(|model| {
                refreshed
                    .iter()
                    .position(|(snapshot, _)| &snapshot.compatibility.model == model)
            })
            .ok_or("main role has no eligible model in the selected set")?;
        let (snapshot, catalog) = refreshed.remove(selected);
        let prepared = crate::provider_setup::connection::PreparedModel { snapshot, catalog };
        let reservation = crate::provider_setup::connection::reservation(&prepared)?.get();
        let model = prepared.snapshot.compatibility.model.clone();
        self.prepared = Some(prepared);
        Ok((model, reservation))
    }
    async fn test(&mut self, budget: &str) -> Result<Value, String> {
        model_preferences::ensure_root(&self.root)?;
        let root = settings::registry_root(&self.root)?;
        let _pin = root
            .hold(None, true)
            .map_err(|_| "account directory redirected")?;
        let key = credential::require(true)?;
        let prepared = self
            .prepared
            .as_ref()
            .ok_or("connection metadata unavailable")?;
        let fingerprint = vcp_protocol::digest_bytes(key.expose().as_bytes());
        let pending_path = self.root.join(PENDING);
        let pending = if pending_path
            .try_exists()
            .map_err(|_| "pending connection state unavailable")?
        {
            let bytes = root
                .read(Path::new(PENDING), 4096)
                .map_err(|_| "pending connection state redirected")?
                .bytes;
            let pending: Pending =
                serde_json::from_slice(&bytes).map_err(|_| "invalid pending connection state")?;
            if !pending.directory.starts_with("connection-")
                || !pending
                    .directory
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            {
                return Err("invalid pending connection directory".into());
            }
            let path = self.root.join(&pending.directory);
            if let Some(report) = crate::provider_setup::connection::retained_result(&path).await? {
                if pending.credential_sha256 == fingerprint
                    && report.model == prepared.snapshot.compatibility.model
                    && report.endpoint == prepared.snapshot.compatibility.endpoint
                {
                    return connection_value(report);
                }
            }
            std::fs::rename(
                &pending_path,
                self.root.join(format!(
                    "connection-settled-{}.json",
                    vcp_domain::CommandId::new()
                )),
            )
            .map_err(|_| "settled connection pointer could not be archived")?;
            new_attempt(&self.root, prepared, &fingerprint)?
        } else {
            new_attempt(&self.root, prepared, &fingerprint)?
        };
        let report =
            crate::provider_setup::connection::test(prepared, budget, &pending, None, key.expose())
                .await?;
        connection_value(report)
    }
    async fn finish(
        &mut self,
        prefs: &Preferences,
        project: Option<&Path>,
        affected_path: &Path,
        report: &Value,
    ) -> Result<(), String> {
        if let Some(project) = project {
            let key = credential::require(true)?;
            model_preferences::configure_project(
                &self.root,
                project,
                &[affected_path.to_path_buf()],
                prefs,
                key.expose(),
            )
            .await?;
        }
        model_preferences::save(&self.root, prefs)?;
        let root = settings::registry_root(&self.root)?;
        let _pin = root
            .hold(None, true)
            .map_err(|_| "account directory redirected")?;
        if let Ok(prior) = root.read(Path::new(COMPLETE), 256 * 1024) {
            if serde_json::from_slice::<Value>(&prior.bytes).is_err() {
                std::fs::rename(
                    self.root.join(COMPLETE),
                    self.root.join(format!(
                        "setup-invalid-{}.json",
                        vcp_domain::CommandId::new()
                    )),
                )
                .map_err(|_| "invalid completion record could not be preserved")?;
            }
        }
        model_preferences::save_record(
            &self.root,
            COMPLETE,
            &serde_json::to_value(Completion {
                version: 1,
                connection: report.clone(),
            })
            .map_err(|_| "completion serialization failed")?,
        )?;
        // Preserve the successful attempt and its accounting. Only the pending
        // pointer is archived, after preferences and completion are durable.
        // Archival is cleanup: failure cannot turn durable success into a
        // reported setup failure. A retained pointer remains safe to inspect
        // or reuse on the next explicit interview and cannot replay a request.
        if self.root.join(PENDING).exists() {
            let _ = std::fs::rename(
                self.root.join(PENDING),
                self.root.join(format!(
                    "connection-finished-{}.json",
                    vcp_domain::CommandId::new()
                )),
            );
        }
        Ok(())
    }
}

fn setup_lease(root: &Path) -> Result<std::fs::File, String> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000).share_mode(1 | 2);
    }
    let file = options
        .open(root.join("setup.lock"))
        .map_err(|_| "setup lease unavailable")?;
    let metadata = file
        .metadata()
        .map_err(|_| "setup lease metadata unavailable")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("setup lease redirected".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("setup lease redirected".into());
        }
    }
    file.try_lock()
        .map_err(|_| "another setup interview is active for this Windows user")?;
    Ok(file)
}

fn new_attempt(
    root: &Path,
    prepared: &crate::provider_setup::connection::PreparedModel,
    fingerprint: &str,
) -> Result<PathBuf, String> {
    let name = format!("connection-{}", vcp_domain::CommandId::new());
    let pending = Pending {
        directory: name.clone(),
        credential_sha256: fingerprint.into(),
        model: prepared.snapshot.compatibility.model.clone(),
        endpoint: prepared.snapshot.compatibility.endpoint.clone(),
    };
    model_preferences::save_record(
        root,
        PENDING,
        &serde_json::to_value(pending).map_err(|_| "pending state serialization failed")?,
    )?;
    Ok(root.join(name))
}

fn connection_value(
    report: crate::provider_setup::connection::ConnectionReport,
) -> Result<Value, String> {
    let mut value =
        serde_json::to_value(&report).map_err(|_| "connection report serialization failed")?;
    value["status"] = json!("connected");
    value["reported_cost_micros"] = json!(report.reported_cost_micros.get());
    Ok(value)
}

#[cfg(test)]
mod tests;
