// SPDX-License-Identifier: Apache-2.0
//! Line-oriented presentation over immutable canonical snapshots. Native console
//! line editing and wrapping handle Unicode and resize without a raw-mode screen.
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use tokio::sync::{mpsc, watch};
use vcp_domain::{task::Task, workspace::Scope};
use vcp_store::contract::{Collection, State};

#[cfg(test)]
mod tests;

#[cfg(windows)]
mod owner;
#[cfg(windows)]
pub use owner::{prepare_resume, run};

pub const INPUT_LIMIT: usize = 65_536;
pub const DISPLAY_LIMIT: usize = 16_384;

#[derive(Debug, PartialEq, Eq)]
pub enum Input {
    Pause,
    Resume,
    Cancel,
    Exit,
    Status,
    Cost,
    History,
    Maintenance(Vec<String>),
    Optimize(crate::optimize::Command),
    Skills(crate::skills::Command),
    Mcp(crate::mcp::Command),
    Next,
    Agents,
    Inspect(String),
    Read { id: String, offset: u64 },
    Answer { id: String, allow: bool },
    Unavailable(String),
    Help,
    Steer(String),
}

pub fn parse(line: &str) -> Result<Option<Input>, String> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(None);
    }
    if line.len() > INPUT_LIMIT {
        return Err("input exceeds 64 KiB".into());
    }
    if let Some(arguments) = line
        .strip_prefix("/mcp")
        .filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
    {
        return crate::mcp::parse(arguments).map(|command| Some(Input::Mcp(command)));
    }
    let words: Vec<_> = line.split_whitespace().collect();
    Ok(Some(match words.as_slice() {
        ["/pause"] => Input::Pause,
        ["/resume"] => Input::Resume,
        ["/cancel"] => Input::Cancel,
        ["/exit"] => Input::Exit,
        ["/status"] => Input::Status,
        ["/cost"] => Input::Cost,
        ["/history"] => Input::History,
        ["/history" | "/prune" | "/retention" | "/memory", _, ..] => Input::Maintenance(
            words
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    if i == 0 {
                        s.trim_start_matches('/').to_owned()
                    } else {
                        (*s).to_owned()
                    }
                })
                .collect(),
        ),
        ["/next"] => Input::Next,
        ["/agents"] => Input::Agents,
        ["/help"] => Input::Help,
        ["/optimize", arguments @ ..] => Input::Optimize(crate::optimize::parse(arguments)?),
        ["/groups", arguments @ ..] => Input::Optimize(crate::optimize::parse_groups(arguments)?),
        ["/skills", arguments @ ..] => Input::Skills(crate::skills::parse(arguments)?),
        ["/memory"] => Input::Unavailable(line.into()),
        ["/inspect", id] => Input::Inspect((*id).into()),
        ["/read", id, offset] => Input::Read {
            id: (*id).into(),
            offset: offset
                .parse()
                .map_err(|_| "byte offset must be an unsigned integer")?,
        },
        ["/answer", id, choice @ ("allow" | "deny")] => Input::Answer {
            id: (*id).into(),
            allow: *choice == "allow",
        },
        _ if line.starts_with('/') => return Err("unknown command or arguments; use /help".into()),
        _ => Input::Steer(line.into()),
    }))
}

/// Require a complete line: EOF cannot submit a partial answer. Allocation and
/// the mailbox stay bounded even when a pipe sends an endless line or flood.
pub fn read_line(reader: &mut impl BufRead) -> io::Result<Option<String>> {
    let mut bytes = Vec::new();
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "unfinished terminal input",
                ))
            };
        }
        let end = chunk.iter().position(|b| *b == b'\n');
        let count = end.map_or(chunk.len(), |i| i + 1);
        if bytes.len().saturating_add(count) > INPUT_LIMIT + 2 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "terminal input exceeds limit",
            ));
        }
        bytes.extend_from_slice(&chunk[..count]);
        reader.consume(count);
        if end.is_some() {
            return String::from_utf8(bytes).map(Some).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "terminal input must be UTF-8")
            });
        }
    }
}

pub fn input<R: BufRead + Send + 'static>(
    mut reader: R,
) -> io::Result<mpsc::Receiver<io::Result<String>>> {
    let (sender, receiver) = mpsc::channel(8);
    std::thread::Builder::new()
        .name("vcp-terminal-input".into())
        .spawn(move || loop {
            let result = match read_line(&mut reader) {
                Ok(Some(line)) => Ok(line),
                Ok(None) => break,
                Err(e) => Err(e),
            };
            let failed = result.is_err();
            if sender.blocking_send(result).is_err() || failed {
                break;
            }
        })?;
    Ok(receiver)
}

/// Escape all controls (including C1 and bidi controls) before touching a terminal.
/// UTF-8 truncation is explicit; full bytes remain in canonical artifacts.
pub fn sanitize(text: &str, limit: usize) -> String {
    let mut result = String::new();
    for ch in text.chars() {
        let escaped =
            if ch.is_control() || matches!(ch, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {
                ch.escape_unicode().to_string()
            } else {
                ch.to_string()
            };
        if result.len() + escaped.len() > limit {
            result.push_str("… [display truncated; use /inspect]");
            break;
        }
        result.push_str(&escaped);
    }
    result
}

/// Renderer owns bytes only. A stalled terminal cannot stall pause/cancellation;
/// only the newest progress view is retained. Durable history is never discarded.
pub struct Renderer {
    sender: watch::Sender<String>,
    failed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Renderer {
    pub fn new<W: Write + Send + 'static>(mut writer: W) -> io::Result<Self> {
        let (sender, mut receiver) = watch::channel(String::new());
        let failed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let failure = failed.clone();
        let runtime = tokio::runtime::Handle::try_current().map_err(io::Error::other)?;
        std::thread::Builder::new()
            .name("vcp-terminal-renderer".into())
            .spawn(move || {
                while runtime.block_on(receiver.changed()).is_ok() {
                    let text = receiver.borrow_and_update().clone();
                    if writeln!(writer, "{text}")
                        .and_then(|()| writer.flush())
                        .is_err()
                    {
                        failure.store(true, std::sync::atomic::Ordering::Release);
                        break;
                    }
                }
            })?;
        Ok(Self { sender, failed })
    }
    pub fn show(&self, text: &str) {
        self.sender.send_replace(sanitize(text, DISPLAY_LIMIT));
    }
    /// Each string is untrusted; only separators inserted here are controls.
    pub fn show_lines(&self, lines: &[String]) {
        let per_line = DISPLAY_LIMIT / lines.len().max(1);
        self.sender.send_replace(
            lines
                .iter()
                .map(|line| sanitize(line, per_line))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    pub fn failed(&self) -> bool {
        self.failed.load(std::sync::atomic::Ordering::Acquire)
    }
}

/// Reduce one immutable snapshot. No renderer has access to a mutable host.
pub fn view(state: &State, scope: &Scope, model: &str) -> Result<Value, String> {
    view_at(state, scope, model, crate::settings::now())
}

pub fn view_at(
    state: &State,
    scope: &Scope,
    model: &str,
    now: vcp_domain::revision::Timestamp,
) -> Result<Value, String> {
    let task: Task = state
        .record(Collection::Task, scope.task.as_str(), &scope.workspace)
        .and_then(|r| r.decode())
        .map_err(|e| e.to_string())?;
    if task.scope != *scope {
        return Err("terminal scope mismatch".into());
    }
    let tasks: Vec<Task> = state
        .records
        .values()
        .filter(|r| r.workspace == scope.workspace && r.collection == Collection::Task)
        .map(|r| r.decode().map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    let included: std::collections::BTreeSet<_> = tasks
        .iter()
        .filter(|t| t.scope.session == scope.session && t.root == task.root)
        .map(|t| t.scope.task.as_str())
        .collect();
    let mut costs = Value::Null;
    let mut questions = Vec::new();
    let mut changes = Vec::new();
    let mut checks = Vec::new();
    let mut step = None;
    let mut step_watermark = None;
    for row in state
        .records
        .values()
        .filter(|r| r.workspace == scope.workspace)
    {
        let scoped = row
            .value
            .get("scope")
            .or_else(|| row.value.pointer("/spec/scope"));
        if !scoped.is_some_and(|s| {
            s["session"] == scope.session.as_str()
                && s["task"].as_str().is_some_and(|id| included.contains(id))
        }) {
            continue;
        }
        match row.collection {
            Collection::Ledger if row.value["scope"]["task"] == task.root.as_str() => {
                costs = json!({"currency":row.value["currency"],"known":row.value["settled"],
                    "reserved":row.value["active"],"uncertain":row.value["unresolved"],"cap":row.value["cap"],"units":"micros"});
            }
            Collection::Approval if row.value["state"] == "pending" => {
                let approval: vcp_protocol::command::Approval =
                    row.decode().map_err(|e| e.to_string())?;
                let actionable = crate::questions::actionable(state, &approval, now)?;
                questions.push(json!({"id":row.id,"task":row.value["scope"]["task"],"effect":row.value["effect"],
                    "actionable":actionable,"expires_at":row.value["expires_at"],"operation_digest":row.value["operation_digest"]}));
            }
            Collection::Effect => {
                let watermark = state
                    .events
                    .iter()
                    .find(|e| Some(e.event.id.as_str()) == row.value["cause"].as_str())
                    .map(|e| e.watermark);
                let unresolved = matches!(
                    row.value["state"].as_str(),
                    Some("running" | "dispatch_recorded" | "outcome_unknown")
                );
                changes.push((unresolved,watermark,json!({"id":row.id,"task":row.value["scope"]["task"],"state":row.value["state"],
                    "changes":row.value["observed_changes"].as_array().map(|a|a.iter().take(4).collect::<Vec<_>>()),
                    "reason":sanitize(row.value["reason"].as_str().unwrap_or(""),256)})));
            }
            Collection::Turn
                if row.value["scope"]["task"] == scope.task.as_str()
                    && row.value["steering"]
                        == serde_json::to_value(task.steering).map_err(|e| e.to_string())? =>
            {
                let watermark = state
                    .events
                    .iter()
                    .find(|e| Some(e.event.id.as_str()) == row.value["cause"].as_str())
                    .map(|e| e.watermark);
                if step.is_none() || watermark > step_watermark {
                    step_watermark = watermark;
                    step = Some(
                        json!({"id":row.id,"state":row.value["state"],"reason":sanitize(row.value["reason"].as_str().unwrap_or(""),256)}),
                    );
                }
            }
            Collection::Verification => {
                let report: vcp_domain::verification::Verification =
                    row.decode().map_err(|e| e.to_string())?;
                if report.applies(scope, task.steering, &task.fingerprint) {
                    let watermark = state
                        .events
                        .iter()
                        .rev()
                        .find(|e| {
                            e.event.data["facts"].as_array().is_some_and(|facts| {
                                facts
                                    .iter()
                                    .any(|f| f["collection"] == "verification" && f["id"] == row.id)
                            })
                        })
                        .map(|e| e.watermark);
                    checks.push((watermark,json!({"id":report.id,"satisfies":report.satisfies(&task.required_checks,task.editing),
                        "checks":report.checks.iter().take(8).map(|c|json!({"specification":sanitize(&c.specification,128),"outcome":c.outcome,"output":c.output})).collect::<Vec<_>>() })));
                }
            }
            _ => {}
        }
    }
    let question_count = questions.len();
    let change_count = changes.len();
    questions.sort_by_key(|question| {
        std::cmp::Reverse(question["actionable"].as_bool().unwrap_or(false))
    });
    questions.truncate(8);
    changes.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    changes.truncate(8);
    checks.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    checks.truncate(1);
    let changes: Vec<_> = changes.into_iter().map(|(_, _, value)| value).collect();
    let checks: Vec<_> = checks.into_iter().map(|(_, value)| value).collect();
    Ok(
        json!({"task":task.scope.task,"revision":task.revision,"state":task.state,
        "objective":task.objectives.last().map(|o|sanitize(&o.text,1024)),
        "model":model,"group":"fixed qualified model; routing groups unavailable",
        "children":tasks.iter().filter(|t|t.scope.task!=scope.task && included.contains(t.scope.task.as_str())).take(8)
            .map(|t|json!({"task":t.scope.task,"state":t.state,"reason":sanitize(&t.reason,256)})).collect::<Vec<_>>(),
        "cost":costs,"current_step":step,"pending_questions":questions,"question_count":question_count,
        "changes":changes,"change_count":change_count,"current_checks":checks,
        "details":"bounded summaries; /history and /next page full evidence; /inspect <effect-id> shows question operation",
        "pause":"admission stops immediately; effects may still be stopping or unknown",
        "questions":"/answer <id> allow|deny; no default; answering never resumes work"}),
    )
}
