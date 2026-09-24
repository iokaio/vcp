// SPDX-License-Identifier: Apache-2.0
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::{fs::File, io::Read, path::PathBuf};
use vcp_domain::{ids::*, revision::Micros};

pub const MAX_TASK_BYTES: usize = 65_536;
pub const MAX_RUN_SKILLS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Text,
    Jsonl,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Autonomy {
    Plan,
    Ask,
    Workspace,
    Autonomous,
}
#[derive(Debug, Parser)]
#[command(name = "vcp", version, about = "VCP local task control and inspection")]
pub struct Cli {
    #[arg(long, global = true, default_value = "text", value_enum)]
    pub format: Format,
    #[arg(long, global = true)]
    pub non_interactive: bool,
    #[arg(long, global = true, default_value = ".")]
    pub workspace: PathBuf,
    #[arg(long, global = true)]
    pub data_dir: Option<PathBuf>,
    /// Explicit user-owned profile, outside the workspace and sync roots.
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    #[arg(long, global = true)]
    pub control_stdin: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Preview and apply explicit foreign configuration subsets without inference.
    #[cfg(windows)]
    Config {
        #[command(subcommand)]
        command: crate::config_import::ConfigCommand,
    },
    Doctor(crate::doctor::Doctor),
    #[cfg(windows)]
    Restore(crate::restore::Restore),
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    Storage {
        #[command(subcommand)]
        command: crate::storage::Storage,
    },
    Backup {
        #[command(subcommand)]
        command: crate::backup::Backup,
    },
    History {
        #[command(subcommand)]
        command: crate::history::History,
    },
    /// Local optimization evidence and preferences; never starts inference.
    Optimize {
        #[command(subcommand)]
        command: crate::optimize::offline::Command,
    },
    /// Inspect explicitly configured skills without starting inference.
    Skills {
        #[command(subcommand)]
        command: crate::skills::OfflineCommand,
    },
    Prune {
        #[command(subcommand)]
        command: crate::history::Prune,
    },
    Retention {
        #[command(subcommand)]
        command: crate::history::Retention,
    },
    /// Reconcile retained history with the explicitly selected local root.
    Rebind {
        #[arg(value_parser = workspace_id)]
        workspace_id: WorkspaceId,
    },
    Run(Run),
    Resume(Resume),
    Sessions {
        #[command(subcommand)]
        command: Sessions,
    },
    Tasks {
        #[command(subcommand)]
        command: Tasks,
    },
    Memory {
        #[command(subcommand)]
        command: Memory,
    },
    Inspect {
        #[arg(value_parser = scoped_id)]
        id: String,
        #[arg(long, value_enum)]
        view: View,
        #[arg(long, default_value_t = 64, value_parser = clap::value_parser!(u32).range(1..=128))]
        limit: u32,
        /// JSON cursor returned by the preceding page.
        #[arg(long)]
        cursor: Option<String>,
        /// Read an artifact byte range on demand.
        #[arg(long, requires = "length", conflicts_with = "cursor")]
        offset: Option<u64>,
        #[arg(long, requires = "offset", conflicts_with = "cursor", value_parser = clap::value_parser!(u32).range(1..=65536))]
        length: Option<u32>,
    },
}
#[derive(Debug, Subcommand)]
pub enum WorkspaceCommand {
    Trust {
        #[arg(value_parser=workspace_id)]
        workspace_id: WorkspaceId,
        #[arg(long)]
        expected_revision: u64,
    },
    Rebind {
        #[arg(value_parser=workspace_id)]
        workspace_id: WorkspaceId,
    },
}
#[derive(Debug, Subcommand)]
pub enum Memory {
    /// Explicitly build local lexical/vector indexes from retained authorized evidence.
    Build(crate::memory::Build),
    /// Explicitly embed a query using provisioned local assets and search retained evidence.
    Query(crate::memory::Query),
    /// Inspect retained search results, evidence and coverage without starting inference.
    Search(Search),
    /// Browse authorized immutable claim versions and their retained evidence.
    Inspect(crate::history::MemoryInspect),
    /// Preview an exact claim-only retention selection.
    Prune(crate::history::Preview),
}
#[derive(Clone, Debug, Args)]
pub struct Search {
    pub text: String,
    #[arg(long, value_parser = task_id)]
    pub task: Option<TaskId>,
    #[arg(long, value_parser = root_id)]
    pub root: Option<RootId>,
    #[arg(long)]
    pub path: Vec<String>,
    #[arg(long)]
    pub symbol: Vec<String>,
    #[arg(long, default_value_t = 8, value_parser = clap::value_parser!(u32).range(1..=64))]
    pub limit: u32,
    #[arg(long, default_value_t = 4096, value_parser = clap::value_parser!(u32).range(2..=16384))]
    pub tokens: u32,
    #[arg(long)]
    pub minimum_sequence: Option<u64>,
    #[arg(long)]
    pub historical: Option<u64>,
}
impl Search {
    pub fn request(&self, workspace: WorkspaceId) -> vcp_memory::retrieval::Request {
        vcp_memory::retrieval::Request {
            workspace,
            tasks: self.task.clone().map(|value| vec![value]),
            roots: self.root.clone().map(|value| vec![value]),
            paths: (!self.path.is_empty()).then(|| self.path.clone()),
            symbols: (!self.symbol.is_empty()).then(|| self.symbol.clone()),
            text: self.text.clone(),
            historical: self.historical.map(vcp_domain::revision::MemorySeq::new),
            minimum_sequence: self
                .minimum_sequence
                .map(vcp_domain::revision::MemorySeq::new),
            timeout_ms: 5000,
            results: self.limit as usize,
            tokens: self.tokens as usize,
            bytes: 65536,
        }
    }
}
#[derive(Debug, Args)]
pub struct Run {
    #[arg(required_unless_present = "file", conflicts_with = "file")]
    pub objective: Option<String>,
    #[arg(long, value_name = "UTF8_TASK_FILE")]
    pub file: Option<PathBuf>,
    #[arg(long, value_parser = parse_usd)]
    pub budget_usd: Option<Micros>,
    #[arg(long, value_enum, default_value = "ask")]
    pub autonomy: Autonomy,
    /// Explicit skill ID to activate before the first turn; repeat up to 32 times.
    #[arg(long = "skill", value_name = "ID", value_parser = skill_id)]
    pub skills: Vec<String>,
}
#[derive(Debug, Args)]
pub struct Resume {
    #[arg(required_unless_present = "last", conflicts_with = "last", value_parser = task_id)]
    pub task: Option<TaskId>,
    #[arg(long)]
    pub last: bool,
    /// Reject a stale workspace chooser selection.
    #[arg(long, requires = "task")]
    pub expected_revision: Option<u64>,
}
#[derive(Debug, Subcommand)]
pub enum Sessions {
    List,
    Resume {
        #[arg(value_parser = session_id)]
        session: SessionId,
    },
    Fork {
        #[arg(value_parser = session_id)]
        session: SessionId,
        #[arg(long, value_parser = turn_id)]
        through_turn: TurnId,
    },
}
#[derive(Debug, Subcommand)]
pub enum Tasks {
    /// Inspect attributed child progress without starting or resuming work.
    Agents {
        #[arg(value_parser = task_id)]
        task: TaskId,
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },
    Status {
        #[arg(value_parser = task_id)]
        task: TaskId,
    },
    Pause {
        #[arg(value_parser = task_id)]
        task: TaskId,
    },
    Cancel {
        #[arg(value_parser = task_id)]
        task: TaskId,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum View {
    Chain,
    Context,
    Prompts,
    Outputs,
    Routing,
    Policy,
    Tools,
    Costs,
    Verification,
    Memory,
}

/// Resolved before engine construction, including task-file decoding and cap.
pub struct ValidatedRun {
    pub objective: String,
    pub budget: Micros,
    pub autonomy: Autonomy,
    pub skills: Vec<String>,
}

/// Every command resolves its workspace and task input before opening an owner
/// or touching canonical state. These values cannot contain a deferred file read.
pub struct ValidatedCli {
    pub workspace: PathBuf,
    pub format: Format,
    pub non_interactive: bool,
    pub data_dir: Option<PathBuf>,
    pub config: Option<PathBuf>,
    pub control_stdin: bool,
    pub command: ValidatedCommand,
}

impl ValidatedCli {
    /// A redirected stream or automation input keeps the finite CLI protocol.
    /// The terminal renderer writes stderr, so it must also be a console.
    pub fn interactive_terminal(&self, stdin: bool, stdout: bool, stderr: bool) -> bool {
        self.format == Format::Text
            && !self.non_interactive
            && !self.control_stdin
            && stdin
            && stdout
            && stderr
    }
}

pub enum ValidatedCommand {
    #[cfg(windows)]
    ConfigImport(crate::config_import::Command),
    WorkspaceTrust {
        workspace: WorkspaceId,
        expected: vcp_domain::Revision,
    },
    Doctor(crate::doctor::Doctor),
    #[cfg(windows)]
    Restore(crate::restore::Restore),
    Storage(crate::storage::Storage),
    Backup(crate::backup::Backup),
    History(crate::history::History),
    Optimize(crate::optimize::offline::Command),
    Skills(crate::skills::OfflineCommand),
    Prune(crate::history::Prune),
    Retention(crate::history::Retention),
    Discover,
    Rebind(WorkspaceId),
    Run(ValidatedRun),
    Resume(Resume),
    Sessions(Sessions),
    Tasks(Tasks),
    MemorySearch(Search),
    MemoryBuild(crate::memory::Build),
    MemoryQuery(crate::memory::Query),
    MemoryInspect(crate::history::MemoryInspect),
    MemoryPrune(crate::history::Preview),
    Inspect {
        request: vcp_audit::inspection::InspectionQuery,
    },
}

impl Cli {
    pub fn validate(self, persisted_cap: Option<Micros>) -> Result<ValidatedCli, String> {
        let workspace = self
            .workspace
            .canonicalize()
            .map_err(|_| "workspace must be an existing accessible directory")?;
        if !workspace.is_dir() || workspace.to_str().is_none() {
            return Err("workspace must be a Unicode directory".into());
        }
        let command = match self.command {
            None => ValidatedCommand::Discover,
            Some(command) => match command {
                #[cfg(windows)]
                Command::Config {
                    command: crate::config_import::ConfigCommand::Import { command },
                } => ValidatedCommand::ConfigImport(command),
                Command::Doctor(request) => ValidatedCommand::Doctor(request),
                #[cfg(windows)]
                Command::Restore(request) => ValidatedCommand::Restore(request),
                Command::Workspace {
                    command:
                        WorkspaceCommand::Trust {
                            workspace_id,
                            expected_revision,
                        },
                } => ValidatedCommand::WorkspaceTrust {
                    workspace: workspace_id,
                    expected: vcp_domain::Revision::new(expected_revision),
                },
                Command::Workspace {
                    command: WorkspaceCommand::Rebind { workspace_id },
                } => ValidatedCommand::Rebind(workspace_id),
                Command::Storage { command } => ValidatedCommand::Storage(command),
                Command::Backup { command } => ValidatedCommand::Backup(command),
                Command::History { command } => ValidatedCommand::History(command),
                Command::Optimize { command } => {
                    command.validate()?;
                    ValidatedCommand::Optimize(command)
                }
                Command::Skills { command } => ValidatedCommand::Skills(command),
                Command::Prune { command } => ValidatedCommand::Prune(command),
                Command::Retention { command } => ValidatedCommand::Retention(command),
                Command::Rebind { workspace_id } => ValidatedCommand::Rebind(workspace_id),
                Command::Run(run) => ValidatedCommand::Run(run.validate(persisted_cap)?),
                Command::Resume(resume) => ValidatedCommand::Resume(resume),
                Command::Sessions { command } => ValidatedCommand::Sessions(command),
                Command::Tasks { command } => ValidatedCommand::Tasks(command),
                Command::Memory {
                    command: Memory::Build(build),
                } => ValidatedCommand::MemoryBuild(build),
                Command::Memory {
                    command: Memory::Query(query),
                } => {
                    query
                        .search
                        .request(WorkspaceId::parse("preflight").map_err(|e| e.to_string())?)
                        .validate()
                        .map_err(|e| e.to_string())?;
                    if query.search.text.len() > vcp_memory::embedding::CHUNK_BYTES {
                        return Err("local query input exceeds exact embedding chunk limit".into());
                    }
                    ValidatedCommand::MemoryQuery(query)
                }
                Command::Memory {
                    command: Memory::Inspect(inspect),
                } => {
                    inspect.request()?;
                    ValidatedCommand::MemoryInspect(inspect)
                }
                Command::Memory {
                    command: Memory::Prune(preview),
                } => ValidatedCommand::MemoryPrune(preview),
                Command::Memory {
                    command: Memory::Search(search),
                } => {
                    search
                        .request(WorkspaceId::parse("preflight").map_err(|e| e.to_string())?)
                        .validate()
                        .map_err(|e| e.to_string())?;
                    ValidatedCommand::MemorySearch(search)
                }
                Command::Inspect {
                    id,
                    view,
                    limit,
                    cursor,
                    offset,
                    length,
                } => {
                    let view = serde_json::from_value(
                        serde_json::to_value(view).map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| e.to_string())?;
                    let cursor = cursor
                        .map(|s| {
                            serde_json::from_str(&s)
                                .map_err(|_| "invalid inspection cursor".to_owned())
                        })
                        .transpose()?;
                    ValidatedCommand::Inspect {
                        request: vcp_audit::inspection::InspectionQuery {
                            id,
                            view,
                            limit,
                            cursor,
                            range: offset.zip(length).map(|(offset, length)| {
                                vcp_audit::inspection::RangeRequest { offset, length }
                            }),
                        },
                    }
                }
            },
        };
        Ok(ValidatedCli {
            workspace,
            format: self.format,
            non_interactive: self.non_interactive,
            data_dir: self.data_dir,
            config: self.config,
            control_stdin: self.control_stdin,
            command,
        })
    }
}
impl Run {
    pub fn validate(&self, persisted_cap: Option<Micros>) -> Result<ValidatedRun, String> {
        if self.skills.len() > MAX_RUN_SKILLS {
            return Err("at most 32 explicit skills may be selected".into());
        }
        let mut selected = std::collections::BTreeSet::new();
        for id in &self.skills {
            skill_id(id)?;
            if !selected.insert(id) {
                return Err("duplicate --skill selection".into());
            }
        }
        let budget = self
            .budget_usd
            .or(persisted_cap)
            .filter(|cap| cap.get() > 0)
            .ok_or("an explicit --budget-usd or persisted budget cap is required")?;
        let objective = match (&self.objective, &self.file) {
            (Some(text), None) => text.clone(),
            (None, Some(path)) => {
                let file = File::open(path).map_err(|_| "task file could not be opened")?;
                if !file
                    .metadata()
                    .map_err(|_| "task file metadata unavailable")?
                    .is_file()
                {
                    return Err("task input must be a regular file".into());
                }
                let mut bytes = Vec::new();
                file.take((MAX_TASK_BYTES + 1) as u64)
                    .read_to_end(&mut bytes)
                    .map_err(|_| "task file could not be read")?;
                if bytes.len() > MAX_TASK_BYTES {
                    return Err("task file exceeds 65536 bytes".into());
                }
                let text = String::from_utf8(bytes).map_err(|_| "task file must be UTF-8")?;
                text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned()
            }
            _ => return Err("supply exactly one objective or --file".into()),
        };
        if objective.trim().is_empty()
            || objective.len() > MAX_TASK_BYTES
            || objective.contains('\0')
        {
            return Err("task text must contain 1–65536 UTF-8 bytes without NUL".into());
        }
        Ok(ValidatedRun {
            objective,
            budget,
            autonomy: self.autonomy,
            skills: self.skills.clone(),
        })
    }
}
fn skill_id(value: &str) -> Result<String, String> {
    match crate::skills::parse(&["activate", value])? {
        crate::skills::Command::Activate { id, .. } => Ok(id),
        _ => Err("invalid skill ID".into()),
    }
}
/// Decimal USD to integer micros, without float rounding, exponent or sign syntax.
pub fn parse_usd(value: &str) -> Result<Micros, String> {
    let invalid =
        || "budget must be positive decimal USD with at most six fractional digits".to_owned();
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 6
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || (value.contains('.') && fraction.is_empty())
    {
        return Err(invalid());
    }
    let whole = whole.parse::<u64>().map_err(|_| invalid())?;
    let fractional = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u64>().map_err(|_| invalid())? * 10u64.pow(6 - fraction.len() as u32)
    };
    let amount = whole
        .checked_mul(1_000_000)
        .and_then(|v| v.checked_add(fractional))
        .filter(|v| *v > 0)
        .ok_or_else(invalid)?;
    Ok(Micros::new(amount))
}
fn task_id(value: &str) -> Result<TaskId, String> {
    TaskId::parse(value).map_err(|_| "invalid task ID".into())
}
fn root_id(value: &str) -> Result<RootId, String> {
    RootId::parse(value).map_err(|_| "invalid root ID".into())
}
fn workspace_id(value: &str) -> Result<WorkspaceId, String> {
    WorkspaceId::parse(value).map_err(|_| "invalid workspace ID".into())
}
fn scoped_id(value: &str) -> Result<String, String> {
    TaskId::parse(value)
        .map(|id| id.to_string())
        .map_err(|_| "invalid scoped ID".into())
}
fn session_id(value: &str) -> Result<SessionId, String> {
    SessionId::parse(value).map_err(|_| "invalid session ID".into())
}
fn turn_id(value: &str) -> Result<TurnId, String> {
    TurnId::parse(value).map_err(|_| "invalid turn ID".into())
}
