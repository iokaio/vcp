// SPDX-License-Identifier: Apache-2.0
//! Native commands use canonical state and the retained controller.
mod execute;
use crate::{
    args::*,
    control,
    jsonl::{Jsonl, Payload},
    settings::{self, WorkspaceEntry},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::{self, Write},
    path::Path,
};
use vcp_domain::{ids::*, task::Task};
use vcp_protocol::{command::CommandEnvelope, digest_bytes};
use vcp_store::{
    contract::{Collection, State},
    Store,
};

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Query {
    Continuation,
    Sessions,
    Task {
        task: TaskId,
    },
    Agents {
        task: TaskId,
        offset: usize,
    },
    Inspect {
        request: vcp_audit::inspection::InspectionQuery,
    },
    InspectBundle {
        task: TaskId,
    },
    MemorySearch {
        request: vcp_memory::retrieval::Request,
    },
}

pub fn query(state: &State, workspace: &WorkspaceId, query: &Query) -> Result<Value, String> {
    let selected: Vec<Value> = match query {
        Query::InspectBundle { task } => {
            return crate::inspection_bundle::collect(
                state,
                &inspection_access(state, workspace)?,
                task,
            );
        }
        Query::MemorySearch { .. } => {
            return Err("memory search requires the canonical store query boundary".into())
        }
        Query::Continuation => {
            return serde_json::to_value(crate::continuation::discover(state, workspace)?)
                .map_err(|e| e.to_string())
        }
        Query::Sessions => state
            .records
            .values()
            .filter(|r| r.workspace == *workspace && r.collection == Collection::Session)
            .map(|r| r.value.clone())
            .collect(),
        Query::Task { task } => {
            vec![serde_json::to_value(task_from(state, workspace, task)?)
                .map_err(|e| e.to_string())?]
        }
        Query::Agents { task, offset } => {
            let task = task_from(state, workspace, task)?;
            return crate::agents_view::page(state, &task.scope, settings::now(), *offset);
        }
        Query::Inspect { request } => {
            return serde_json::to_value(
                vcp_audit::inspection::records(
                    state,
                    &inspection_access(state, workspace)?,
                    request,
                )
                .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string());
        }
    };
    bounded_records(state.watermark, workspace, selected)
}

/// Current-only owner reads retain the public query envelope while avoiding a
/// full historical snapshot. History-dependent queries stay on `query`.
pub fn query_current(
    state: &vcp_store::CurrentState,
    workspace: &WorkspaceId,
    query: &Query,
) -> Result<Value, String> {
    let selected = match query {
        Query::Sessions => state
            .records_in(Collection::Session, workspace)
            .map(|record| record.value.clone())
            .collect(),
        Query::Task { task } => {
            let task: Task = state
                .record(Collection::Task, task.as_str(), workspace)
                .and_then(|record| record.decode())
                .map_err(|error| error.to_string())?;
            vec![serde_json::to_value(task).map_err(|error| error.to_string())?]
        }
        _ => return Err("query requires retained historical state".into()),
    };
    bounded_records(state.watermark, workspace, selected)
}

fn bounded_records(
    watermark: vcp_domain::Watermark,
    workspace: &WorkspaceId,
    selected: Vec<Value>,
) -> Result<Value, String> {
    let mut records = Vec::new();
    let mut bytes = 0;
    for record in selected.iter().take(256) {
        bytes += serde_json::to_vec(record).map_err(|e| e.to_string())?.len();
        if bytes > 768 * 1024 {
            break;
        }
        records.push(record.clone());
    }
    Ok(
        json!({"watermark":watermark,"workspace":workspace,"truncated":records.len()<selected.len(),"records":records}),
    )
}
fn inspection_access(
    state: &State,
    workspace: &WorkspaceId,
) -> Result<vcp_audit::history::Access, String> {
    let current: vcp_domain::workspace::Workspace = state
        .record(Collection::Workspace, workspace.as_str(), workspace)
        .and_then(|r| r.decode())
        .map_err(|e| e.to_string())?;
    Ok(vcp_audit::history::Access {
        workspace: workspace.clone(),
        authority: current.authority,
        read: true,
        tasks: None,
    })
}
pub async fn query_store(
    store: &Store,
    workspace: &WorkspaceId,
    actor: &ActorId,
    request: &Query,
) -> Result<Value, String> {
    if let Query::MemorySearch { request } = request {
        let access = vcp_memory::access::Access {
            workspace: workspace.clone(),
            actor: actor.clone(),
            authority: inspection_access(store.state(), workspace)?.authority,
            read: true,
            write: false,
            tasks: None,
        };
        return serde_json::to_value(
            vcp_lifecycle::foundation::memory_inspection::inspect_store(
                store,
                &access,
                store.root(),
                request,
            )
            .await?,
        )
        .map_err(|e| e.to_string());
    }
    if let Query::Inspect { request } = request {
        return serde_json::to_value(
            vcp_audit::inspection::inspect(
                store,
                &inspection_access(store.state(), workspace)?,
                request,
            )
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string());
    }
    let mut value = query(store.state(), workspace, request)?;
    if matches!(request, Query::InspectBundle { .. }) {
        value["store_diagnostics"] =
            serde_json::to_value(store.diagnostics()).map_err(|e| e.to_string())?;
    }
    Ok(value)
}
fn task_from(state: &State, workspace: &WorkspaceId, task: &TaskId) -> Result<Task, String> {
    state
        .record(Collection::Task, task.as_str(), workspace)
        .and_then(|r| r.decode())
        .map_err(|e| e.to_string())
}
fn latest(
    state: &State,
    workspace: &WorkspaceId,
    session: Option<&SessionId>,
) -> Result<Task, String> {
    let candidates = crate::continuation::candidates(state, workspace)?;
    let selected = candidates
        .iter()
        .find(|task| session.is_none_or(|s| s == &task.session))
        .ok_or("no unfinished task is available to resume")?;
    task_from(state, workspace, &selected.task)
}
struct DisplayOutput {
    jsonl: bool,
    frame: Vec<u8>,
    frame_limit: usize,
}
impl Write for DisplayOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.jsonl {
            std::io::stdout().write_all(bytes)?;
        } else {
            // serde_json::to_writer emits fragments, not complete records.
            // Decode only the newline-terminated frame, preserving JSON escapes.
            for byte in bytes {
                if *byte == b'\n' {
                    let value: Value = serde_json::from_slice(&self.frame)?;
                    if value["type"] != "event" {
                        writeln!(std::io::stdout(), "{value}")?;
                    }
                    self.frame.clear();
                } else {
                    if self.frame.len() >= self.frame_limit {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "display frame exceeds command output limit",
                        ));
                    }
                    self.frame.push(*byte);
                }
            }
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        if !self.frame.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "incomplete display frame",
            ));
        }
        std::io::stdout().flush()
    }
}
fn command_result(format: Format, data: Value) -> Result<u8, String> {
    command_outcome(format, data, 0)
}
fn command_outcome(format: Format, data: Value, exit_code: u8) -> Result<u8, String> {
    command_outcome_scoped(format, data, exit_code, None)
}
fn command_outcome_scoped(
    format: Format,
    data: Value,
    exit_code: u8,
    scope: Option<&vcp_domain::workspace::Scope>,
) -> Result<u8, String> {
    emit_command_outcome(
        DisplayOutput {
            jsonl: format == Format::Jsonl,
            frame: Vec::new(),
            frame_limit: if data["kind"] == "inspection_bundle" {
                crate::inspection_bundle::MAX_BYTES + 1024
            } else {
                1024 * 1024
            },
        },
        &data,
        exit_code,
        scope,
    )?;
    Ok(exit_code)
}
pub(crate) fn emit_command_outcome(
    output: impl Write,
    data: &Value,
    exit_code: u8,
    scope: Option<&vcp_domain::workspace::Scope>,
) -> Result<(), String> {
    Jsonl::new(output)
        .emit(
            &CommandId::new(),
            scope,
            Payload::CommandResult { exit_code, data },
        )
        .map_err(|e| e.to_string())
}

async fn discover_selection(cli: ValidatedCli, value: Value) -> Result<u8, String> {
    use std::io::{BufRead, IsTerminal};
    if !cli.interactive_terminal(
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
        std::io::stderr().is_terminal(),
    ) {
        return command_result(cli.format, value);
    }
    let rows = value["candidates"]
        .as_array()
        .ok_or("invalid continuation response")?;
    if rows.is_empty() {
        return command_result(cli.format, value);
    }
    for (index, row) in rows.iter().enumerate() {
        // Escape all stored text, including objective and filenames.
        writeln!(std::io::stderr(), "{}: {}", index + 1, row).map_err(|e| e.to_string())?;
    }
    if value["truncated"] == true {
        eprintln!("More tasks exist; use an explicit task ID to resume an unlisted task.");
    }
    eprintln!("Resume a task by number, or press Enter to leave it paused:");
    let choice = tokio::task::spawn_blocking(|| {
        let mut bytes = Vec::new();
        let mut input = std::io::stdin().lock();
        std::io::Read::take(&mut input, 65)
            .read_until(b'\n', &mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 64 {
            return Err("selection exceeds 64 bytes".to_owned());
        }
        String::from_utf8(bytes).map_err(|_| "selection must be UTF-8".to_owned())
    })
    .await
    .map_err(|e| e.to_string())??;
    if choice.trim().is_empty() {
        return Ok(0);
    }
    let index = choice
        .trim()
        .parse::<usize>()
        .ok()
        .and_then(|n| n.checked_sub(1))
        .ok_or("select a displayed task number")?;
    let row = rows.get(index).ok_or("select a displayed task number")?;
    let task = serde_json::from_value(row["task"].clone()).map_err(|_| "invalid task selection")?;
    let expected_revision: vcp_domain::revision::Revision =
        serde_json::from_value(row["expected_revision"].clone())
            .map_err(|_| "invalid task revision")?;
    Box::pin(run(Cli {
        workspace: cli.workspace,
        data_dir: cli.data_dir,
        config: cli.config,
        format: cli.format,
        non_interactive: cli.non_interactive,
        control_stdin: cli.control_stdin,
        command: Some(crate::args::Command::Resume(Resume {
            task: Some(task),
            last: false,
            expected_revision: Some(expected_revision.get()),
        })),
    }))
    .await
}

pub async fn run(cli: Cli) -> Result<u8, String> {
    if cli.command.is_none() {
        return crate::setup_wizard::first_launch(&cli).await;
    }
    if let Some(crate::args::Command::Models { command }) = &cli.command {
        if command.is_none() && crate::setup_wizard::interactive(&cli) {
            return crate::setup_wizard::models(&cli);
        }
        return command_result(
            cli.format,
            crate::model_preferences::execute(command.as_ref(), &cli.workspace).await?,
        );
    }
    if let Some(crate::args::Command::Setup { command }) = &cli.command {
        let Some(command) = command else {
            return crate::setup_wizard::run(&cli).await;
        };
        if let crate::onboarding::Command::Credential { command } = command {
            return command_result(cli.format, crate::setup_wizard::credential(command, &cli)?);
        }
        return command_result(
            cli.format,
            crate::onboarding::execute(
                command,
                &cli.workspace,
                cli.config.as_deref(),
                crate::setup_wizard::interactive(&cli),
            )
            .await?,
        );
    }
    if let Some(crate::args::Command::Config {
        command: crate::config_import::ConfigCommand::Import { command },
    }) = &cli.command
    {
        let profile = cli
            .config
            .as_deref()
            .ok_or("configuration import requires explicit --config")?;
        return command_result(
            cli.format,
            crate::config_import::execute(command, profile, &cli.workspace)?,
        );
    }
    if let Some(crate::args::Command::Doctor(request)) = &cli.command {
        let data = cli
            .data_dir
            .clone()
            .map(Ok)
            .unwrap_or_else(settings::default_data)?;
        return command_result(
            cli.format,
            crate::doctor::execute(request, &data, &cli.workspace)?,
        );
    }
    if let Some(crate::args::Command::Restore(request)) = &cli.command {
        let data = cli
            .data_dir
            .clone()
            .map(Ok)
            .unwrap_or_else(settings::default_data)?;
        return command_result(
            cli.format,
            crate::restore::execute(request, &data, &cli.workspace).await?,
        );
    }
    let workspace = cli
        .workspace
        .canonicalize()
        .map_err(|_| "workspace is unavailable; restore its root or explicitly rebind/reconcile its durable history before continuing")?;
    let data = settings::local_path(
        &cli.data_dir
            .clone()
            .map(Ok)
            .unwrap_or_else(settings::default_data)?,
        &workspace,
    )?;
    let path_key = digest_bytes(workspace.to_string_lossy().to_lowercase().as_bytes());
    let existing_directory = if matches!(
        cli.command,
        Some(
            crate::args::Command::Rebind { .. }
                | crate::args::Command::Workspace {
                    command: crate::args::WorkspaceCommand::Rebind { .. }
                }
        )
    ) {
        None
    } else {
        settings::workspace_directory(&data, &workspace)?
    };
    let directory = existing_directory.unwrap_or(settings::local_path(
        &data.join("workspaces").join(&path_key),
        &workspace,
    )?);
    let key = directory
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("invalid workspace directory")?
        .to_owned();
    let entry_path = directory.join("workspace.json");
    let pipe = control::pipe(&digest_bytes(
        format!("{}:{key}", data.display())
            .to_lowercase()
            .as_bytes(),
    ));
    let needs_profile = matches!(
        cli.command,
        Some(
            crate::args::Command::Run(_)
                | crate::args::Command::Resume(_)
                | crate::args::Command::Sessions {
                    command: Sessions::Resume { .. } | Sessions::Fork { .. }
                }
        )
    );
    let profile = if needs_profile {
        Some(crate::model_preferences::load_for_command(&cli, &data, &directory, &workspace).await
            .map_err(|error| format!("{error}; choose the matching --config for this workspace, or run vcp setup to configure a project"))?)
    } else {
        None
    };
    if let Some(profile) = &profile {
        for root in &profile.sync_roots {
            let root = root
                .canonicalize()
                .map_err(|_| "declared sync root unavailable")?;
            if settings::within(&data, &root) {
                return Err("plaintext data is inside a declared sync root".into());
            }
        }
    }
    let cap = profile
        .as_ref()
        .and_then(|p| p.budget_usd.as_deref())
        .map(parse_usd)
        .transpose()?;
    let cli = cli.validate(cap)?;
    if let ValidatedCommand::WorkspaceTrust {
        workspace: id,
        expected,
    } = &cli.command
    {
        return command_result(
            cli.format,
            crate::workspace_trust::execute(&data, &workspace, id, *expected).await?,
        );
    }
    if let ValidatedCommand::Rebind(id) = &cli.command {
        return command_result(
            cli.format,
            crate::rebind::rebind(&data, &workspace, id).await?,
        );
    }
    if let ValidatedCommand::Storage(command) = &cli.command {
        return command_result(
            cli.format,
            crate::storage::execute(command, &data, &directory, &workspace).await?,
        );
    }
    if needs_profile && !directory.exists() {
        std::fs::create_dir_all(&directory)
            .map_err(|_| "workspace selection directory unavailable")?;
    }
    // Lease the selection before reading it, through canonical owner shutdown.
    let _selection_lease = directory
        .exists()
        .then(|| crate::selection::Lease::shared(&data, &directory))
        .transpose()?;
    let entry: Option<WorkspaceEntry> = if entry_path.exists() {
        Some(
            serde_json::from_slice(&settings::read_workspace_descriptor(&data, &entry_path)?)
                .map_err(|_| "invalid workspace descriptor")?,
        )
    } else {
        None
    };
    if let Some(entry) = &entry {
        if entry.rebind_pending {
            return Err(format!(
                "restored history is selected but rebind is pending; run vcp workspace rebind {}",
                entry.config.workspace
            ));
        }
        crate::selection::validate_location(&directory, entry)?;
        if Path::new(&entry.config.binding.root) != workspace {
            return Err("workspace descriptor binding mismatch".into());
        }
        let root = vcp_repository::Root::open(
            vcp_repository::RootIdentity {
                workspace: entry.config.workspace.clone(),
                root: RootId::parse(entry.config.workspace.as_str()).map_err(|e| e.to_string())?,
                repository: entry.config.binding.repository.clone(),
                worktree: entry.config.binding.worktree.clone(),
                binding: entry.config.binding.revision,
            },
            &workspace,
        )
        .map_err(|e| format!("workspace requires rebind/reconcile: {e}"))?;
        let identity = entry.identity.as_ref().ok_or_else(|| format!(
            "legacy workspace binding is unverified; run vcp rebind {} to reconcile the current root", entry.config.workspace
        ))?;
        crate::binding::verify(&root, identity)?;
    }
    if let ValidatedCommand::Tasks(Tasks::ReconcileCost { task }) = &cli.command {
        let entry = entry.as_ref().ok_or("workspace has no durable session")?;
        let (exit_code, result) =
            crate::provider_reconciliation::execute(entry, task, &workspace).await?;
        let scope = serde_json::from_value(result["scope"].clone())
            .map_err(|_| "cost reconciliation returned invalid scope")?;
        return command_outcome_scoped(cli.format, result, exit_code, Some(&scope));
    }
    if matches!(
        cli.command,
        ValidatedCommand::MemoryBuild(_) | ValidatedCommand::MemoryQuery(_)
    ) {
        let entry = entry.as_ref().ok_or("workspace has no durable session")?;
        let (exit_code, result) = crate::memory::execute(&cli.command, entry).await?;
        return command_outcome(cli.format, result, exit_code);
    }
    if let ValidatedCommand::Backup(crate::backup::Backup::Keys {
        command,
        workspace_id,
    }) = &cli.command
    {
        return command_result(
            cli.format,
            crate::backup::keys(
                command,
                &data,
                &workspace,
                entry.as_ref(),
                workspace_id.as_deref(),
            )?,
        );
    }
    if let ValidatedCommand::Backup(command) = &cli.command {
        let entry = entry.as_ref().ok_or("workspace has no durable session")?;
        match command {
            crate::backup::Backup::Create {
                key,
                git,
                operation,
                retry,
            } => {
                let operation = operation
                    .as_deref()
                    .map(CommandId::parse)
                    .transpose()
                    .map_err(|_| "invalid backup operation")?
                    .unwrap_or_default();
                let request = crate::backup::CreateRequest {
                    key: key.clone(),
                    git: git.clone(),
                    operation: operation.clone(),
                    retry: *retry,
                };
                // A live owner accepts bounded preparation immediately. The
                // standalone maintenance owner waits and closes without models.
                let opened = vcp_lifecycle::foundation::CanonicalHost::open(entry.config.clone());
                match opened {
                    Ok((host, owner)) => {
                        let started = crate::backup::create_on_host(&host, &data, request).await;
                        let result = if let Err(error) = started {
                            Err(error)
                        } else {
                            loop {
                                let progress =
                                    host.backup_progress()?.ok_or("backup progress missing")?;
                                if progress.done() {
                                    break if let Some(error) = progress.error {
                                        Err(error)
                                    } else {
                                        serde_json::to_value(progress).map_err(|e| e.to_string())
                                    };
                                }
                                tokio::select! {
                                    _=tokio::time::sleep(std::time::Duration::from_millis(100))=>{},
                                    _=tokio::signal::ctrl_c()=>{let _=host.cancel_backup(&operation);break Err(format!("backup cancellation requested; inspect operation {operation}"));}
                                }
                            }
                        };
                        owner.close().await?;
                        return command_result(cli.format, result?);
                    }
                    Err(error) if error.contains("canonical root already has an owner") => {
                        return command_result(
                            cli.format,
                            control::request(
                                &pipe,
                                &control::Request::BackupCreate {
                                    workspace: entry.config.workspace.clone(),
                                    data: data.clone(),
                                    request,
                                },
                            )
                            .await?,
                        );
                    }
                    Err(error) => return Err(error),
                }
            }
            crate::backup::Backup::Cancel { operation } => {
                let id = CommandId::parse(operation).map_err(|_| "invalid backup operation")?;
                let result =
                    match vcp_lifecycle::foundation::CanonicalHost::open(entry.config.clone()) {
                        Ok((host, owner)) => {
                            let result = crate::backup::cancel_on_host(&host, &data, id).await;
                            owner.close().await?;
                            result?
                        }
                        Err(error) if error.contains("canonical root already has an owner") => {
                            control::request(
                                &pipe,
                                &control::Request::BackupCancel {
                                    workspace: entry.config.workspace.clone(),
                                    data: data.clone(),
                                    id,
                                },
                            )
                            .await?
                        }
                        Err(error) => return Err(error),
                    };
                return command_result(cli.format, result);
            }
            crate::backup::Backup::Configure { .. } => {
                return command_result(
                    cli.format,
                    crate::backup::configure(command, &data, &workspace, entry)?,
                )
            }
            crate::backup::Backup::Status => {
                let mut value=crate::backup::configuration_status(&data,&workspace,entry)
                    .unwrap_or_else(|error|serde_json::json!({"kind":"backup_status","configuration_unavailable":error}));
                value["canonical"] = match Store::open(
                    &entry.config.canonical_root,
                    entry.config.backend,
                    std::slice::from_ref(&workspace),
                )
                .await
                {
                    Ok(store) => {
                        let value = vcp_lifecycle::foundation::backup::status(
                            store.state(),
                            &entry.config.workspace,
                        )?;
                        store.close().await.map_err(|e| e.to_string())?;
                        value
                    }
                    Err(vcp_store::Error::Conflict("canonical root already has an owner")) => {
                        control::request(
                            &pipe,
                            &control::Request::BackupStatus {
                                workspace: entry.config.workspace.clone(),
                            },
                        )
                        .await?
                    }
                    Err(error) => return Err(error.to_string()),
                };
                return command_result(cli.format, value);
            }
            crate::backup::Backup::Keys { .. } => return Err("key control routing failed".into()),
        }
    }
    if let ValidatedCommand::Skills(crate::skills::OfflineCommand::List { offset }) = &cli.command {
        let entry = entry.as_ref().ok_or("Skill inspection requires a registered workspace; open or restore its durable session first.")?;
        let profile = settings::load(
            &cli.config
                .clone()
                .unwrap_or_else(|| data.join("profile.json")),
            &workspace,
        )?;
        return command_result(
            cli.format,
            crate::skills::offline::execute(&profile, entry, &workspace, &pipe, *offset).await?,
        );
    }
    if let ValidatedCommand::Optimize(command) = &cli.command {
        let entry = entry.as_ref().ok_or(
            "workspace has no durable session; local optimization needs retained workspace history",
        )?;
        let value = crate::optimize::offline::execute(entry, &workspace, &pipe, command).await?;
        return command_result(cli.format, value);
    }
    let history_request = match &cli.command {
        ValidatedCommand::History(command) => Some(
            command.request(
                &entry
                    .as_ref()
                    .ok_or("workspace has no durable session")?
                    .config
                    .workspace,
            )?,
        ),
        ValidatedCommand::Prune(command) => Some(command.request()?),
        ValidatedCommand::Retention(command) => Some(command.request()?),
        ValidatedCommand::MemoryInspect(command) => Some(command.request()?),
        ValidatedCommand::MemoryPrune(command) => Some(
            command.memory_request(
                &entry
                    .as_ref()
                    .ok_or("workspace has no durable session")?
                    .config
                    .workspace,
            )?,
        ),
        _ => None,
    };
    if let Some(request) = history_request {
        let entry = entry.as_ref().ok_or("workspace has no durable session")?;
        // Notice acknowledgement is a canonical write. Do not invalidate the
        // exact preview just presented, or change its source before applying it.
        let notify = !matches!(
            &request,
            vcp_lifecycle::foundation::history_retention::Request::Preview { .. }
                | vcp_lifecycle::foundation::history_retention::Request::PreviewPage { .. }
                | vcp_lifecycle::foundation::history_retention::Request::Apply { .. }
                | vcp_lifecycle::foundation::history_retention::Request::Cleanup { .. }
        );
        // Keep the canonical owner through the query and its notice handling.
        // Reopening for each request replays all retained commits each time.
        let mut history = HistoryControl::open(entry, &workspace, &pipe).await?;
        let result = async {
            let mut value = history.execute(request).await?;
            let notice = if notify {
                history
                    .execute(vcp_lifecycle::foundation::history_retention::Request::Notice)
                    .await
                    .ok()
                    .filter(|v| v["due"] == true)
            } else {
                None
            };
            if let Some(notice) = &notice {
                value["retention_notice"] = notice.clone();
            }
            let code = command_result(cli.format, value)?;
            if notice.is_some() {
                let _ = history
                    .execute(vcp_lifecycle::foundation::history_retention::Request::NoticeShown)
                    .await;
            }
            Ok(code)
        }
        .await;
        history.close().await?;
        return result;
    }
    let read = match &cli.command {
        ValidatedCommand::Discover => Some(Query::Continuation),
        ValidatedCommand::Sessions(Sessions::List) => Some(Query::Sessions),
        ValidatedCommand::Tasks(Tasks::Status { task }) => Some(Query::Task { task: task.clone() }),
        ValidatedCommand::Tasks(Tasks::Agents { task, offset }) => Some(Query::Agents {
            task: task.clone(),
            offset: *offset,
        }),
        ValidatedCommand::InspectBundle { task } => {
            Some(Query::InspectBundle { task: task.clone() })
        }
        ValidatedCommand::Inspect { request } => Some(Query::Inspect {
            request: request.clone(),
        }),
        ValidatedCommand::MemorySearch(search) => Some(Query::MemorySearch {
            request: search.request(
                entry
                    .as_ref()
                    .ok_or("workspace has no durable session")?
                    .config
                    .workspace
                    .clone(),
            ),
        }),
        _ => None,
    };
    if let Some(query_request) = read {
        let Some(entry) = entry.as_ref() else {
            if matches!(query_request, Query::Continuation) {
                return command_result(
                    cli.format,
                    json!({"candidates":[],"truncated":false,"message":"No unfinished tasks. For first-run setup use vcp setup provider --help, then vcp setup profile --help. Validate your selected --config with vcp setup check before vcp run."}),
                );
            }
            return Err("workspace has no durable session".into());
        };
        let store = Store::open(
            &entry.config.canonical_root,
            entry.config.backend,
            std::slice::from_ref(&workspace),
        )
        .await;
        let value = match store {
            Ok(store) => {
                let value = query_store(
                    &store,
                    &entry.config.workspace,
                    &entry.config.actor,
                    &query_request,
                )
                .await;
                store.close().await.map_err(|e| e.to_string())?;
                value?
            }
            Err(vcp_store::Error::Conflict("canonical root already has an owner")) => {
                control::request(
                    &pipe,
                    &control::Request::Query {
                        workspace: entry.config.workspace.clone(),
                        query: query_request,
                    },
                )
                .await?
            }
            Err(error) => return Err(error.to_string()),
        };
        if matches!(cli.command, ValidatedCommand::Discover) {
            return discover_selection(cli, value).await;
        }
        return command_result(cli.format, value);
    }
    if let ValidatedCommand::Tasks(stop) = &cli.command {
        let entry = entry.as_ref().ok_or("workspace has no owning controller")?;
        let (task, cancel) = match stop {
            Tasks::Pause { task } => (task, false),
            Tasks::Cancel { task } => (task, true),
            _ => return Err("unsupported task control".into()),
        };
        let value = control::request(
            &pipe,
            &control::Request::Prepare {
                workspace: entry.config.workspace.clone(),
                task: task.clone(),
                cancel,
            },
        )
        .await?;
        let command: CommandEnvelope =
            serde_json::from_value(value).map_err(|_| "invalid owner envelope")?;
        if command.workspace != entry.config.workspace || command.task.as_ref() != Some(task) {
            return Err("owner control scope mismatch".into());
        }
        return command_result(
            cli.format,
            control::request(
                &pipe,
                &control::Request::Stop {
                    command: Box::new(command),
                },
            )
            .await?,
        );
    }
    execute::execute(
        cli,
        profile.ok_or("user profile required")?,
        cap,
        entry,
        execute::Locations {
            data: &data,
            directory: &directory,
            entry_path: &entry_path,
            pipe: &pipe,
            key: &key,
        },
    )
    .await
}

enum HistoryControl<'a> {
    Local {
        store: Store,
        access: vcp_memory::access::Access,
    },
    Remote {
        entry: &'a WorkspaceEntry,
        pipe: &'a str,
    },
}

impl<'a> HistoryControl<'a> {
    async fn open(
        entry: &'a WorkspaceEntry,
        workspace: &Path,
        pipe: &'a str,
    ) -> Result<Self, String> {
        match Store::open(
            &entry.config.canonical_root,
            entry.config.backend,
            std::slice::from_ref(&workspace.to_owned()),
        )
        .await
        {
            Ok(store) => {
                let workspace: vcp_domain::workspace::Workspace = store
                    .state()
                    .record(
                        Collection::Workspace,
                        entry.config.workspace.as_str(),
                        &entry.config.workspace,
                    )
                    .and_then(|r| r.decode())
                    .map_err(|e| e.to_string())?;
                let access = vcp_memory::access::Access {
                    workspace: workspace.id,
                    actor: entry.config.actor.clone(),
                    authority: workspace.authority,
                    read: true,
                    write: true,
                    tasks: None,
                };
                Ok(Self::Local { store, access })
            }
            Err(vcp_store::Error::Conflict("canonical root already has an owner")) => {
                Ok(Self::Remote { entry, pipe })
            }
            Err(error) => Err(error.to_string()),
        }
    }

    async fn execute(
        &mut self,
        request: vcp_lifecycle::foundation::history_retention::Request,
    ) -> Result<Value, String> {
        match self {
            Self::Local { store, access } => {
                let now = vcp_domain::Timestamp::new(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_err(|e| e.to_string())?
                        .as_millis()
                        .min(u64::MAX as u128) as u64,
                );
                vcp_lifecycle::foundation::history_retention::execute(store, access, request, now)
                    .await
            }
            Self::Remote { entry, pipe } => {
                control::request(
                    pipe,
                    &control::Request::HistoryRetention {
                        workspace: entry.config.workspace.clone(),
                        request,
                    },
                )
                .await
            }
        }
    }

    async fn close(self) -> Result<(), String> {
        match self {
            Self::Local { store, .. } => store.close().await.map_err(|e| e.to_string()),
            Self::Remote { .. } => Ok(()),
        }
    }
}

#[cfg(test)]
mod display_tests {
    use super::*;
    #[tokio::test]
    async fn current_query_preserves_envelope_and_refuses_history_queries() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::open(temporary.path(), vcp_store::BackendKind::Files, &[])
            .await
            .unwrap();
        let workspace = WorkspaceId::new();
        let current = store.current_state();
        assert_eq!(
            query(store.state(), &workspace, &Query::Sessions).unwrap(),
            query_current(&current, &workspace, &Query::Sessions).unwrap()
        );
        let request = Query::Task {
            task: TaskId::new(),
        };
        assert_eq!(
            query(store.state(), &workspace, &request).unwrap_err(),
            query_current(&current, &workspace, &request).unwrap_err()
        );
        assert!(query_current(&current, &workspace, &Query::Continuation).is_err());
        assert!(query_current(
            &current,
            &workspace,
            &Query::InspectBundle {
                task: TaskId::new()
            }
        )
        .is_err());
        let mut state = store.state().clone();
        for index in 0..300 {
            let session = vcp_domain::workspace::Session {
                id: vcp_domain::SessionId::parse(format!("session-{index:04}")).unwrap(),
                workspace: workspace.clone(),
                revision: vcp_domain::Revision::ZERO,
                configuration: vcp_domain::Revision::ZERO,
                fork_origin: None,
                fork_through: None,
            };
            let record = vcp_store::contract::Record::typed(
                Collection::Session,
                session.id.to_string(),
                workspace.clone(),
                session.revision,
                &session,
            )
            .unwrap();
            state.records.insert(record.key(), record);
        }
        let mut current = (*current).clone();
        current.records = state.records.clone();
        let projected = query_current(&current, &workspace, &Query::Sessions).unwrap();
        assert_eq!(
            projected,
            query(&state, &workspace, &Query::Sessions).unwrap()
        );
        assert_eq!(projected["truncated"], true);
        assert_eq!(projected["records"].as_array().unwrap().len(), 256);
        let foreign = WorkspaceId::new();
        assert_eq!(
            query_current(&current, &foreign, &Query::Sessions).unwrap(),
            query(&state, &foreign, &Query::Sessions).unwrap()
        );
        store.close().await.unwrap();
    }
    #[test]
    fn text_display_waits_for_complete_json_frames() {
        let mut output = DisplayOutput {
            jsonl: false,
            frame: Vec::new(),
            frame_limit: 1024 * 1024,
        };
        output.write_all(b"{\"type\":").unwrap();
        assert!(output.flush().is_err());
        output
            .write_all(b"\"event\",\"text\":\"escaped\\ncontrol\"}\n")
            .unwrap();
        output.flush().unwrap();
        assert!(output.frame.is_empty());
    }

    #[test]
    fn text_display_accepts_bundle_sized_frames_but_keeps_explicit_bounds() {
        let bytes = vec![b' '; 1024 * 1024 + 1];
        let mut normal = DisplayOutput {
            jsonl: false,
            frame: Vec::new(),
            frame_limit: 1024 * 1024,
        };
        assert!(normal.write_all(&bytes).is_err());
        let mut bundle = DisplayOutput {
            jsonl: false,
            frame: Vec::new(),
            frame_limit: crate::inspection_bundle::MAX_BYTES + 1024,
        };
        bundle.write_all(&bytes).unwrap();
        assert_eq!(bundle.frame.len(), bytes.len());
        // Test the exact boundary without emitting a multi-megabyte stdout line.
        bundle.frame.resize(bundle.frame_limit, b' ');
        assert!(bundle.write_all(b" ").is_err());
    }
}
