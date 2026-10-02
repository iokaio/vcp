// SPDX-License-Identifier: Apache-2.0
//! Guided `vcp setup` for an interactive terminal (ADR-077). Each step uses the
//! same code as an explicit setup command. Paid steps show computed minimums
//! and require a typed amount and a typed `yes`; nothing is pre-filled,
//! substituted or retried.
mod prompt;
#[cfg(all(test, windows))]
mod tests;
pub use prompt::{ConsolePrompter, Prompter, ScriptedPrompter};

use crate::{
    credential::{self, Secret, Source},
    model_sets, onboarding, profile_selection,
    provider_setup::{estimate, production},
    render::{self, View},
    settings,
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use vcp_domain::Micros;

const STEPS: usize = 7;
const ATTEMPTS: usize = 3;

pub struct Context {
    /// The workspace as given by --workspace (default ".").
    pub workspace: PathBuf,
    pub data_dir: Option<PathBuf>,
    /// Provider origin; tests inject a local server.
    pub api: String,
    /// Pause between free generation-receipt retrievals.
    pub receipt_wait: std::time::Duration,
    /// Where an attended session would take the key from.
    pub lookup: fn(bool) -> Option<Source>,
    /// Credential Manager entry written when the user opts in.
    pub credential_target: String,
}

impl Context {
    pub fn attended(workspace: PathBuf, data_dir: Option<PathBuf>) -> Self {
        Self {
            workspace,
            data_dir,
            api: production::OPENROUTER_API.into(),
            receipt_wait: std::time::Duration::from_secs(15),
            lookup: credential::source,
            credential_target: credential::TARGET.into(),
        }
    }
}

pub struct Outcome {
    pub value: Value,
    pub exit_code: u8,
    /// Canonical workspace and objective for an approved read-only test task.
    pub smoke_task: Option<(PathBuf, String)>,
}

fn stopped(step: &str, reason: &str) -> Outcome {
    Outcome {
        value: json!({"status":"stopped","step":step,"reason":reason,
            "next":"run `vcp setup` again, or `vcp setup --help` for the explicit steps"}),
        exit_code: 2,
        smoke_task: None,
    }
}

fn step(prompts: &mut dyn Prompter, number: usize, title: &str) {
    prompts.say(&format!("\nStep {number}/{STEPS}  {title}"));
}

fn yes(answer: &Option<String>) -> bool {
    answer.as_deref().is_some_and(|answer| {
        answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes")
    })
}

fn micros(value: &Value) -> Option<u64> {
    value.as_str().and_then(|text| text.parse().ok())
}

fn display(path: &Path) -> String {
    settings::display_path(path)
}

/// Why a folder cannot be the workspace, if it cannot.
fn workspace_problem(candidate: &Path, data: &Path) -> Option<String> {
    let Ok(canonical) = candidate.canonicalize() else {
        return Some(format!("{} is not an existing folder", candidate.display()));
    };
    if !canonical.is_dir() {
        return Some(format!("{} is not a folder", display(&canonical)));
    }
    if canonical.parent().is_none() {
        return Some(format!(
            "{} is a drive root, not a project folder",
            display(&canonical)
        ));
    }
    let home = std::env::var_os("USERPROFILE")
        .and_then(|home| PathBuf::from(home).canonicalize().ok())
        .is_some_and(|home| {
            settings::within(&home, &canonical) && settings::within(&canonical, &home)
        });
    if home {
        return Some(format!(
            "{} is your home folder, not a project folder",
            display(&canonical)
        ));
    }
    settings::local_path(data, &canonical)
        .err()
        .map(|error| render::data_placement(error, data, &canonical))
}

fn choose_workspace(
    context: &Context,
    data: &Path,
    prompts: &mut dyn Prompter,
) -> Result<Option<PathBuf>, String> {
    let mut candidate = context.workspace.clone();
    for _ in 0..ATTEMPTS {
        match workspace_problem(&candidate, data) {
            None => {
                return candidate
                    .canonicalize()
                    .map(Some)
                    .map_err(|_| "workspace folder became unavailable".into())
            }
            Some(problem) => {
                prompts.say(&format!("This folder can't be used: {problem}."));
                match prompts.line("Project folder to set up (Enter to stop): ")? {
                    Some(answer) if !answer.is_empty() => candidate = PathBuf::from(answer),
                    _ => return Ok(None),
                }
            }
        }
    }
    Ok(None)
}

/// A typed USD amount within bounds, or None when the user stops.
fn amount(
    prompts: &mut dyn Prompter,
    prompt: &str,
    minimum: u64,
    maximum: Option<u64>,
) -> Result<Option<(String, u64)>, String> {
    for _ in 0..ATTEMPTS {
        let Some(answer) = prompts.line(prompt)?.filter(|answer| !answer.is_empty()) else {
            return Ok(None);
        };
        let text = answer.trim_start_matches('$').to_owned();
        match crate::args::parse_usd(&text) {
            Ok(value) if value.get() < minimum => prompts.say(&format!(
                "That is below the minimum of {}.",
                render::usd(&json!(minimum))
            )),
            Ok(value) if maximum.is_some_and(|maximum| value.get() > maximum) => {
                prompts.say(&format!(
                    "That is above the maximum of {}.",
                    render::usd(&json!(maximum))
                ))
            }
            Ok(value) => return Ok(Some((text, value.get()))),
            Err(_) => prompts.say("Enter a dollar amount such as 7 or 7.50."),
        }
    }
    Ok(None)
}

fn choose_set(prompts: &mut dyn Prompter) -> Result<Option<estimate::Estimate>, String> {
    let sets = model_sets::catalog()?;
    let mut lines = Vec::new();
    for (index, set) in sets.iter().enumerate() {
        let group = match set.category {
            model_sets::Category::Default => "default",
            model_sets::Category::Vendor => "by vendor",
            model_sets::Category::Level => "by level",
        };
        let note = if set.distinct().len() > 1 {
            "  (needs per-role assignment; not available yet)"
        } else {
            ""
        };
        lines.push(format!("  {}) {} [{group}]{note}", index + 1, set.title));
    }
    lines.push(format!(
        "  {}) Custom: an exact model and endpoint",
        sets.len() + 1
    ));
    prompts.say(&lines.join("\n"));
    for _ in 0..ATTEMPTS {
        let Some(answer) = prompts.line("Choose a model set (Enter = 1, Quick test): ")? else {
            return Ok(None);
        };
        let choice = if answer.is_empty() {
            1
        } else {
            answer.parse::<usize>().unwrap_or(0)
        };
        if choice == sets.len() + 1 {
            let model = prompts.line("Exact OpenRouter model ID (organization/model): ")?;
            let endpoint = prompts.line("Exact endpoint tag: ")?;
            let (Some(model), Some(endpoint)) = (model, endpoint) else {
                return Ok(None);
            };
            if model.is_empty() || endpoint.is_empty() {
                return Ok(None);
            }
            return Ok(Some(estimate::Estimate {
                set: None,
                model: Some(model),
                endpoint: Some(endpoint),
                request_price_limit: None,
                output_tokens: None,
            }));
        }
        match choice.checked_sub(1).and_then(|index| sets.get(index)) {
            Some(set) if set.distinct().len() > 1 => prompts.say(&format!(
                "{} assigns a different model to delegated children; verifying both members is not available yet. Choose a single-model set or Custom.",
                set.title
            )),
            Some(set) => {
                return Ok(Some(estimate::Estimate {
                    set: Some(set.id.clone()),
                    model: None,
                    endpoint: None,
                    request_price_limit: None,
                    output_tokens: None,
                }))
            }
            None => prompts.say("Choose one of the listed numbers."),
        }
    }
    Ok(None)
}

fn choose_affected_path(
    workspace: &Path,
    prompts: &mut dyn Prompter,
) -> Result<Option<PathBuf>, String> {
    if workspace.join("README.md").is_file() {
        prompts.say("The first task will focus on README.md.");
        return Ok(Some(PathBuf::from("README.md")));
    }
    let mut entries: Vec<String> = std::fs::read_dir(workspace)
        .map_err(|_| "workspace cannot be listed")?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.'))
        .collect();
    entries.sort();
    entries.truncate(12);
    if !entries.is_empty() {
        let listed: Vec<_> = entries
            .iter()
            .enumerate()
            .map(|(index, name)| {
                format!("  {}) {}", index + 1, crate::terminal::sanitize(name, 120))
            })
            .collect();
        prompts.say(&format!(
            "Which path should tasks focus on first?\n{}",
            listed.join("\n")
        ));
    }
    for _ in 0..ATTEMPTS {
        let Some(answer) = prompts
            .line("Choose a number or type a relative path (Enter to stop): ")?
            .filter(|answer| !answer.is_empty())
        else {
            return Ok(None);
        };
        let path = match answer.parse::<usize>() {
            Ok(number) => match number.checked_sub(1).and_then(|index| entries.get(index)) {
                Some(name) => PathBuf::from(name),
                None => {
                    prompts.say("Choose one of the listed numbers.");
                    continue;
                }
            },
            Err(_) => PathBuf::from(answer),
        };
        if path.is_absolute()
            || path
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            prompts.say("Use a path inside the workspace, such as src or README.md.");
            continue;
        }
        return Ok(Some(path));
    }
    Ok(None)
}

/// Obtain the provider key for this attended session.
fn key(context: &Context, prompts: &mut dyn Prompter) -> Result<Option<Secret>, String> {
    if let Some(source) = (context.lookup)(true) {
        let described = match source {
            Source::Environment => "OPENROUTER_API_KEY in this terminal",
            Source::Session => "the key entered earlier in this session",
            Source::CredentialManager => "Windows Credential Manager",
        };
        prompts.say(&format!(
            "Using the OpenRouter key from {described} (value not shown)."
        ));
        return credential::require(true).map(Some);
    }
    prompts.say("No OpenRouter key was found. Create one at https://openrouter.ai/keys. It is entered hidden, kept out of files and output, and never passed as an argument.");
    let Some(secret) = prompts.secret("OpenRouter API key (input hidden): ")? else {
        return Ok(None);
    };
    let save = prompts
        .line("Save it in Windows Credential Manager for future interactive sessions? [y/N] ")?;
    if yes(&save) {
        #[cfg(windows)]
        {
            credential::store(&context.credential_target, &secret)?;
            prompts.say("Stored. Remove it any time with `vcp setup credential remove`.");
        }
        #[cfg(not(windows))]
        prompts.say(
            "Credential Manager storage requires Windows; the key is kept for this session only.",
        );
    } else {
        prompts.say("The key is kept only for this setup session.");
    }
    let copy = Secret::new(secret.expose().to_owned())?;
    credential::set_session(secret);
    Ok(Some(copy))
}

/// Run the provider probe pair, then finish delayed receipts for free.
async fn verify(
    context: &Context,
    request: &production::Request,
    workspace: &Path,
    key: &Secret,
    prompts: &mut dyn Prompter,
) -> Result<Result<Value, String>, String> {
    let redact = |error: String| error.replace(key.expose(), "[redacted]");
    match production::run_with(request, workspace, key.expose(), &context.api).await {
        Ok(value) => return Ok(Ok(value)),
        Err(error) => {
            let error = redact(error.to_string());
            if !error.contains(production::RECEIPTS_PENDING) {
                return Ok(Err(error));
            }
        }
    }
    prompts.say("The provider has not published the generation receipts yet; retrieving them without repeating any request.");
    let mut last = String::new();
    for _ in 0..4 {
        tokio::time::sleep(context.receipt_wait).await;
        match production::complete_with(&request.output, workspace, key.expose(), &context.api)
            .await
        {
            Ok(value) => return Ok(Ok(value)),
            Err(error) => last = redact(error.to_string()),
        }
    }
    Ok(Err(format!(
        "{last}. Run `vcp setup provider-complete --directory \"{}\"` later; it never repeats inference",
        display(&request.output)
    )))
}

pub async fn run(context: &Context, prompts: &mut dyn Prompter) -> Result<Outcome, String> {
    let now = settings::now;
    prompts.say("VCP guided setup. Paid steps always show the amount and ask before spending. An empty answer stops; nothing is pre-filled.");
    step(prompts, 1, "Environment");
    let data = context
        .data_dir
        .clone()
        .map(Ok)
        .unwrap_or_else(settings::default_data)?;
    prompts.say(&format!(
        "VCP {}; private data folder {}.",
        env!("CARGO_PKG_VERSION"),
        display(&std::path::absolute(&data).unwrap_or(data.clone()))
    ));

    step(prompts, 2, "Workspace");
    let Some(workspace) = choose_workspace(context, &data, prompts)? else {
        return Ok(stopped("workspace", "no usable project folder was chosen"));
    };
    prompts.say(&format!(
        "Workspace: {}\nTrusting it lets VCP read files here and propose changes; edits and commands still ask for approval under the default `ask` autonomy.",
        display(&workspace)
    ));
    if !yes(&prompts.line("Trust this folder? [y/N] ")?) {
        return Ok(stopped("workspace", "the workspace was not trusted"));
    }
    if let Ok(resolved) = profile_selection::resolve(&data, &workspace, None) {
        match onboarding::check(&resolved.path, &workspace, (context.lookup)(true).is_some()) {
            Ok(value) => {
                prompts.say(&format!(
                    "This workspace already has a ready profile:\n{}",
                    render::human(View::SetupCheck, &value, now())
                ));
                let answer =
                    prompts.line("1) Keep it  2) Set up another model set  (Enter = 1): ")?;
                if answer
                    .as_deref()
                    .is_none_or(|answer| answer.is_empty() || answer == "1")
                {
                    return Ok(Outcome {
                        value: json!({"status":"ready","workspace":workspace,"profile":resolved.path,
                            "valid_until":value["valid_until"],"changed":false}),
                        exit_code: 0,
                        smoke_task: None,
                    });
                }
            }
            Err(error) => prompts.say(&format!(
                "The selected profile needs attention ({}); continuing sets up a new one.",
                crate::terminal::sanitize(&error, 400)
            )),
        }
    }

    step(prompts, 3, "OpenRouter key");
    let Some(key) = key(context, prompts)? else {
        return Ok(stopped("credential", "no key was entered"));
    };

    step(prompts, 4, "Model set");
    let Some(choice) = choose_set(prompts)? else {
        return Ok(stopped("model_set", "no model set was chosen"));
    };
    let priced = estimate::estimate_with(&choice, &context.api, now()).await?;
    prompts.say(&render::human(View::SetupEstimate, &priced, now()));
    let member = &priced["members"][0];
    if priced["status"] != "estimated" || member["within_setup_ceiling"] != true {
        return Ok(stopped(
            "model_set",
            "the chosen model cannot be set up as listed; nothing was substituted",
        ));
    }
    let (Some(probe), Some(pair), Some(task_minimum)) = (
        micros(&member["probe_reservation_micros"]),
        micros(&member["probe_pair_micros"]),
        micros(&member["task_request_reservation_micros"]),
    ) else {
        return Err("estimate is missing reservation amounts".into());
    };

    step(prompts, 5, "Verify the model (paid)");
    prompts.say(&format!(
        "Verification makes at most 2 paid requests, never retried. Each reserves up to {}; only actual usage is charged.",
        render::usd(&json!(probe))
    ));
    let Some((cap_text, cap)) = amount(
        prompts,
        &format!(
            "Verification cap in USD (at least {}, at most $25; Enter to stop): ",
            render::usd(&json!(probe))
        ),
        probe,
        Some(estimate::SETUP_CEILING_MICROS),
    )?
    else {
        return Ok(stopped(
            "verify",
            "no verification cap was entered; nothing was spent",
        ));
    };
    if cap < pair {
        prompts.say(&format!(
            "Note: a cap below {} can stop the second request while the first charge is still settling.",
            render::usd(&json!(pair))
        ));
    }
    let confirm = prompts.line(&format!(
        "Type yes to allow at most 2 paid requests within {}: ",
        render::usd(&json!(cap))
    ))?;
    if confirm.as_deref() != Some("yes") {
        return Ok(stopped(
            "verify",
            "spending was not confirmed; nothing was spent",
        ));
    }
    let target = production::Target {
        model: member["model"].as_str().unwrap_or_default().to_owned(),
        endpoint: member["endpoint"].as_str().unwrap_or_default().to_owned(),
        request_price_limit: member["request_price_limit"]
            .as_str()
            .unwrap_or(model_sets::REQUEST_PRICE_LIMIT)
            .to_owned(),
    };
    let request = production::Request {
        output: production::default_output(&data, &workspace, &target)
            .map_err(|e| e.to_string())?,
        target,
        budget_usd: cap_text,
    };
    prompts.say("Verifying; this can take a minute.");
    let verified = match verify(context, &request, &workspace, &key, prompts).await? {
        Ok(value) => value,
        Err(error) => {
            return Err(format!(
                "verification did not complete: {error}. The folder {} is kept with its accounting; reconcile any charge before trying again",
                display(&request.output)
            ))
        }
    };
    prompts.say(&render::human(View::SetupProvider, &verified, now()));

    step(prompts, 6, "Profile");
    let Some((budget, _)) = amount(
        prompts,
        &format!(
            "Per-task budget in USD (at least {} for one request; Enter to stop): ",
            render::usd(&json!(task_minimum))
        ),
        task_minimum,
        None,
    )?
    else {
        return Ok(stopped("profile", "no task budget was entered"));
    };
    let Some(path) = choose_affected_path(&workspace, prompts)? else {
        return Ok(stopped("profile", "no focus path was chosen"));
    };
    let set = choice.set.clone();
    let profile = onboarding::create(
        &onboarding::Profile {
            provider: Some(request.output.clone()),
            snapshot: None,
            catalog: None,
            set: set.clone(),
            output: None,
            trust_workspace: true,
            budget_usd: budget.clone(),
            autonomy: crate::args::Autonomy::Ask,
            affected_path: vec![path.clone()],
        },
        &workspace,
        &data,
    )?;
    profile_selection::select(&data, &workspace, &profile, set.as_deref(), now())?;
    prompts.say(&format!(
        "Created and selected profile {}.",
        display(&profile)
    ));

    step(prompts, 7, "Check and test");
    let check = onboarding::check(&profile, &workspace, true)?;
    prompts.say(&render::human(View::SetupCheck, &check, now()));
    let objective = format!(
        "Read {} and summarize its purpose in three sentences with citations. Do not change files.",
        path.display()
    );
    let test = yes(&prompts.line(&format!(
        "Run a short read-only test task now? It makes paid requests within your {} task budget. [y/N] ",
        render::usd(&json!(crate::args::parse_usd(&budget).map(Micros::get).unwrap_or(0)))
    ))?);
    prompts.say(&format!(
        "Setup complete. Model metadata is {}; run `vcp setup` again to renew it.\nNext: vcp run \"<task>\"",
        render::validity(&check["valid_until"], now())
    ));
    Ok(Outcome {
        value: json!({"status":"ready","workspace":workspace,"profile":profile,"set":set,
            "provider_directory":request.output,"valid_until":check["valid_until"],
            "changed":true,"smoke_test":test}),
        exit_code: 0,
        smoke_task: test.then(|| (workspace.clone(), objective)),
    })
}
