// SPDX-License-Identifier: Apache-2.0
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::{fs::File, io::Read, path::PathBuf};
use vcp_domain::{ids::*, revision::Micros};

pub const MAX_TASK_BYTES: usize = 65_536;

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
    #[command(subcommand)]
    pub command: Option<Command>,
}
#[derive(Debug, Subcommand)]
pub enum Command {
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
    Inspect {
        id: String,
        #[arg(long, value_enum)]
        view: View,
    },
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
}
#[derive(Debug, Args)]
pub struct Resume {
    #[arg(required_unless_present = "last", conflicts_with = "last", value_parser = task_id)]
    pub task: Option<TaskId>,
    #[arg(long)]
    pub last: bool,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum View {
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
}
impl Run {
    pub fn validate(&self, persisted_cap: Option<Micros>) -> Result<ValidatedRun, String> {
        let budget = self.budget_usd.or(persisted_cap)
            .filter(|cap| cap.get() > 0)
            .ok_or("an explicit --budget-usd or persisted budget cap is required")?;
        let objective = match (&self.objective, &self.file) {
            (Some(text), None) => text.clone(),
            (None, Some(path)) => {
                let file = File::open(path).map_err(|_| "task file could not be opened")?;
                if !file.metadata().map_err(|_| "task file metadata unavailable")?.is_file() {
                    return Err("task input must be a regular file".into());
                }
                let mut bytes = Vec::new();
                file.take((MAX_TASK_BYTES + 1) as u64).read_to_end(&mut bytes)
                    .map_err(|_| "task file could not be read")?;
                if bytes.len() > MAX_TASK_BYTES {
                    return Err("task file exceeds 65536 bytes".into());
                }
                let text = String::from_utf8(bytes).map_err(|_| "task file must be UTF-8")?;
                text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned()
            }
            _ => return Err("supply exactly one objective or --file".into()),
        };
        if objective.trim().is_empty() || objective.len() > MAX_TASK_BYTES || objective.contains('\0') {
            return Err("task text must contain 1–65536 UTF-8 bytes without NUL".into());
        }
        Ok(ValidatedRun { objective, budget, autonomy: self.autonomy })
    }
}
/// Decimal USD to integer micros, without float rounding, exponent or sign syntax.
pub fn parse_usd(value: &str) -> Result<Micros, String> {
    let invalid = || "budget must be positive decimal USD with at most six fractional digits".to_owned();
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 6 || !fraction.bytes().all(|b| b.is_ascii_digit())
        || (value.contains('.') && fraction.is_empty()) {
        return Err(invalid());
    }
    let whole = whole.parse::<u64>().map_err(|_| invalid())?;
    let fractional = if fraction.is_empty() { 0 } else {
        fraction.parse::<u64>().map_err(|_| invalid())? * 10u64.pow(6 - fraction.len() as u32)
    };
    let amount = whole.checked_mul(1_000_000).and_then(|v| v.checked_add(fractional))
        .filter(|v| *v > 0).ok_or_else(invalid)?;
    Ok(Micros::new(amount))
}
fn task_id(value: &str) -> Result<TaskId, String> {
    TaskId::parse(value).map_err(|_| "invalid task ID".into())
}
fn session_id(value: &str) -> Result<SessionId, String> {
    SessionId::parse(value).map_err(|_| "invalid session ID".into())
}
fn turn_id(value: &str) -> Result<TurnId, String> {
    TurnId::parse(value).map_err(|_| "invalid turn ID".into())
}
