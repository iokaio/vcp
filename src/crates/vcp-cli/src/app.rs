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
    for event in state.events.iter().rev() {
        if event.event.workspace != *workspace || session.is_some_and(|s| s != &event.event.session)
        {
            continue;
        }
        if let Some(id) = &event.event.task {
            let task = task_from(state, workspace, id)?;
            if task.parent.is_none() {
                return Ok(task);
            }
        }
    }
    Err("no task is available to resume".into())
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

pub async fn run(cli: Cli) -> Result<u8, String> {
    let workspace = cli
        .workspace
        .canonicalize()
        .map_err(|_| "workspace is unavailable")?;
    let data = settings::local_path(
        &cli.data_dir
            .clone()
            .map(Ok)
            .unwrap_or_else(settings::default_data)?,
        &workspace,
    )?;
    let key = digest_bytes(workspace.to_string_lossy().to_lowercase().as_bytes());
    let directory = settings::local_path(&data.join("workspaces").join(&key), &workspace)?;
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
    let entry: Option<WorkspaceEntry> = if entry_path.exists() {
        Some(
            serde_json::from_slice(&settings::read_bounded(&entry_path, 256 * 1024)?)
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
    }
    let read = match &cli.command {
        ValidatedCommand::Sessions(Sessions::List) => Some(Query::Sessions),
        ValidatedCommand::Tasks(Tasks::Status { task }) => Some(Query::Task { task: task.clone() }),
        ValidatedCommand::Inspect { request } => Some(Query::Inspect {
            request: request.clone(),
        }),
        _ => None,
    };
    if let Some(query_request) = read {
        let entry = entry.as_ref().ok_or("workspace has no durable session")?;
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
