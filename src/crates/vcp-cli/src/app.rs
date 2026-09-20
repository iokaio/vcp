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
    Inspect {
        request: vcp_audit::inspection::InspectionQuery,
    },
}

pub fn query(state: &State, workspace: &WorkspaceId, query: &Query) -> Result<Value, String> {
    let selected: Vec<Value> = match query {
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
        json!({"watermark":state.watermark,"workspace":workspace,"truncated":records.len()<selected.len(),"records":records}),
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
pub fn query_store(
    store: &Store,
    workspace: &WorkspaceId,
    request: &Query,
) -> Result<Value, String> {
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
    query(store.state(), workspace, request)
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
}
impl Write for DisplayOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.jsonl {
            std::io::stdout().write_all(bytes)?;
        } else {
            let value: Value = serde_json::from_slice(bytes)?;
            // JSON escaping prevents untrusted terminal control execution.
            if value["type"] != "event" {
                writeln!(std::io::stdout(), "{value}")?;
            }
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        std::io::stdout().flush()
    }
}
fn command_result(format: Format, data: Value) -> Result<u8, String> {
    Jsonl::new(DisplayOutput {
        jsonl: format == Format::Jsonl,
    })
    .emit(
        &CommandId::new(),
        None,
        Payload::CommandResult {
            exit_code: 0,
            data: &data,
        },
    )
    .map_err(|e| e.to_string())?;
    Ok(0)
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
    let existing_directory = if matches!(cli.command, Some(crate::args::Command::Rebind { .. })) {
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
        Some(settings::load(
            &cli.config
                .clone()
                .unwrap_or_else(|| data.join("profile.json")),
            &workspace,
        )?)
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
    if let ValidatedCommand::Rebind(id) = &cli.command {
        return command_result(
            cli.format,
            crate::rebind::rebind(&data, &workspace, id).await?,
        );
    }
    let entry: Option<WorkspaceEntry> = if entry_path.exists() {
        Some(
            serde_json::from_slice(&settings::read_workspace_descriptor(&data, &entry_path)?)
                .map_err(|_| "invalid workspace descriptor")?,
        )
    } else {
        None
    };
    if let Some(entry) = &entry {
        if entry.version != 1
            || Path::new(&entry.config.binding.root) != workspace
            || entry.config.canonical_root != directory.join("canonical")
        {
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
    let read = match &cli.command {
        ValidatedCommand::Discover => Some(Query::Continuation),
        ValidatedCommand::Sessions(Sessions::List) => Some(Query::Sessions),
        ValidatedCommand::Tasks(Tasks::Status { task }) => Some(Query::Task { task: task.clone() }),
        ValidatedCommand::Inspect { request } => Some(Query::Inspect {
            request: request.clone(),
        }),
        _ => None,
    };
    if let Some(query_request) = read {
        let Some(entry) = entry.as_ref() else {
            if matches!(query_request, Query::Continuation) {
                return command_result(
                    cli.format,
                    json!({"candidates":[],"truncated":false,"message":"No unfinished tasks. Use vcp run to start a task."}),
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
                let value = query_store(&store, &entry.config.workspace, &query_request);
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
