// SPDX-License-Identifier: Apache-2.0
use crate::{catalog::usd_micros, request::Tools, *};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{
    accounting::{Money, Usage},
    Micros, Units,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Call {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedUsage {
    pub raw: Value,
    pub tokens: Option<Usage>,
    pub cost: Option<Money>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Completed,
    Incomplete,
    Failed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultBody {
    pub response_id: String,
    pub served_model: Option<String>,
    pub served_provider: Option<String>,
    pub status: Status,
    pub usage: Option<ObservedUsage>,
    pub calls: Vec<Call>,
    /// Non-whitespace visible text from completed assistant items. Deltas and
    /// opaque reasoning cannot establish that the response is usable.
    #[serde(default)]
    pub visible_text_bytes: u64,
    pub raw_terminal_sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Text(String),
    /// Arrival observation only. It carries no parsed call or execution right.
    ToolFragment {
        item_id: String,
        bytes: usize,
    },
    Opaque {
        kind: String,
    },
    TerminalObserved,
}
#[derive(Clone, Debug)]
struct Pending {
    call_id: String,
    name: String,
    arguments: String,
    done: bool,
}

/// One parser per admitted attempt. Raw bytes must be captured before push.
/// Bounded framing operates on bytes so arbitrary UTF-8 transport splits work.
pub struct Stream {
    tools: Tools,
    line: Vec<u8>,
    data: Vec<u8>,
    event_type: Option<String>,
    skip_lf: bool,
    first_line: bool,
    total: usize,
    events: usize,
    pending: BTreeMap<String, Pending>,
    messages: BTreeMap<String, String>,
    terminal: Option<ResultBody>,
    terminal_data: Option<String>,
    done: bool,
    failed: bool,
}
impl Stream {
    pub fn terminal_identity(&self) -> Option<&str> {
        if self.failed {
            None
        } else {
            self.terminal.as_ref().map(|r| r.response_id.as_str())
        }
    }
    pub fn new(tools: Tools) -> Self {
        Self {
            tools,
            line: vec![],
            data: vec![],
            event_type: None,
            skip_lf: false,
            first_line: true,
            total: 0,
            events: 0,
            pending: BTreeMap::new(),
            messages: BTreeMap::new(),
            terminal: None,
            terminal_data: None,
            done: false,
            failed: false,
        }
    }
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Event>> {
        if self.failed {
            return Err(Error::Protocol("stream already invalid"));
        }
        let result = self.push_inner(bytes);
        if result.is_err() {
            self.failed = true;
            self.terminal = None;
        }
        result
    }
    fn push_inner(&mut self, bytes: &[u8]) -> Result<Vec<Event>> {
        if bytes.len() > 65_536 {
            return Err(Error::Limit("transport chunk"));
        }
        self.total = self
            .total
            .checked_add(bytes.len())
            .ok_or(Error::Limit("response bytes"))?;
        if self.total > 16 * 1024 * 1024 {
            return Err(Error::Limit("response bytes"));
        }
        let mut events = vec![];
        for &byte in bytes {
            if self.skip_lf {
                self.skip_lf = false;
                if byte == b'\n' {
                    continue;
                }
            }
            if byte == b'\n' || byte == b'\r' {
                self.skip_lf = byte == b'\r';
                if let Some(event) = self.line()? {
                    events.push(event);
                }
            } else {
                if self.line.len() >= 1024 * 1024 {
                    return Err(Error::Limit("SSE line"));
                }
                self.line.push(byte);
            }
        }
        Ok(events)
    }
    fn line(&mut self) -> Result<Option<Event>> {
        let bytes = std::mem::take(&mut self.line);
        let mut line = std::str::from_utf8(&bytes).map_err(|_| Error::Protocol("SSE UTF-8"))?;
        if self.first_line {
            self.first_line = false;
            line = line.trim_start_matches('\u{feff}');
        }
        if line.is_empty() {
            let event_type = self.event_type.take();
            if self.data.is_empty() {
                return Ok(None);
            }
            self.data.pop(); // Last data-line newline.
            let data = std::mem::take(&mut self.data);
            let text = std::str::from_utf8(&data).map_err(|_| Error::Protocol("event UTF-8"))?;
            return self.event(text, event_type.as_deref());
        }
        if line.starts_with(':') {
            return Ok(None);
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "data" => {
                if self.data.len() + value.len() + 1 > 1024 * 1024 {
                    return Err(Error::Limit("SSE event"));
                }
                self.data.extend_from_slice(value.as_bytes());
                self.data.push(b'\n');
            }
            "event" => {
                if value.len() > 128 {
                    return Err(Error::Limit("event name"));
                }
                self.event_type = Some(value.into());
            }
            "id" | "retry" => (), // Never creates a transport retry or billing identity.
            _ => (),
        }
        Ok(None)
    }
    fn event(&mut self, text: &str, event_type: Option<&str>) -> Result<Option<Event>> {
        self.events += 1;
        if self.events > 100_000 {
            return Err(Error::Limit("SSE events"));
        }
        if text == "[DONE]" {
            if self.terminal.is_none() || self.done {
                return Err(Error::Protocol("premature/duplicate DONE"));
            }
            self.done = true;
            return Ok(None);
        }
        if self.done {
            return Err(Error::Protocol("payload after DONE"));
        }
        let value: Value = serde_json::from_str(text)?;
        let kind = value["type"]
            .as_str()
            .ok_or(Error::Protocol("event type missing"))?;
        if event_type.is_some_and(|event| event != "message" && event != kind) {
            return Err(Error::Protocol("SSE/payload type mismatch"));
        }
        if self.terminal.is_some() {
            if self.terminal_data.as_deref() == Some(text) {
                return Ok(None);
            }
            return Err(Error::Protocol("conflicting event after terminal"));
        }
        match kind {
            "response.output_item.done" if value["item"]["type"] == "message" => {
                self.observe_message(&value["item"])?;
                Ok(Some(Event::Opaque { kind: kind.into() }))
            }
            "response.output_text.delta" => {
                let text = value["delta"]
                    .as_str()
                    .ok_or(Error::Protocol("text delta"))?;
                Ok(Some(Event::Text(text.into())))
            }
            "response.output_item.added" => {
                let item = &value["item"];
                if item["type"].as_str() != Some("function_call") {
                    return Ok(Some(Event::Opaque { kind: kind.into() }));
                }
                if self.pending.len() >= 64 {
                    return Err(Error::Limit("tool calls"));
                }
                let id = identity(item, "id")?;
                let call_id = identity(item, "call_id")?;
                let name = identity(item, "name")?;
                if self.pending.values().any(|p| p.call_id == call_id) {
                    return Err(Error::Protocol("duplicate call identity"));
                }
                let arguments = item.get("arguments").and_then(Value::as_str).unwrap_or("");
                if arguments.len() > 256 * 1024 {
                    return Err(Error::Limit("function arguments"));
                }
                if self
                    .pending
                    .insert(
                        id,
                        Pending {
                            call_id,
                            name,
                            arguments: arguments.into(),
                            done: false,
                        },
                    )
                    .is_some()
                {
                    return Err(Error::Protocol("duplicate item identity"));
                }
                Ok(None)
            }
            "response.function_call_arguments.delta" => {
                let id = identity(&value, "item_id")?;
                let delta = value["delta"]
                    .as_str()
                    .ok_or(Error::Protocol("argument delta"))?;
                let pending = self
                    .pending
                    .get_mut(&id)
                    .ok_or(Error::Protocol("unknown argument item"))?;
                if pending.done || pending.arguments.len() + delta.len() > 256 * 1024 {
                    return Err(Error::Protocol("arguments after done or over limit"));
                }
                pending.arguments.push_str(delta);
                Ok(Some(Event::ToolFragment {
                    item_id: id,
                    bytes: delta.len(),
                }))
            }
            "response.function_call_arguments.done" => {
                let id = identity(&value, "item_id")?;
                let arguments = value["arguments"]
                    .as_str()
                    .ok_or(Error::Protocol("finished arguments"))?;
                let pending = self
                    .pending
                    .get_mut(&id)
                    .ok_or(Error::Protocol("unknown finished item"))?;
                if pending.done || pending.arguments != arguments {
                    return Err(Error::Protocol("finished arguments differ from deltas"));
                }
                pending.done = true;
                Ok(None)
            }
            "response.completed" | "response.incomplete" | "response.failed" => {
                let response = &value["response"];
                let status = match kind {
                    "response.completed" => Status::Completed,
                    "response.incomplete" => Status::Incomplete,
                    _ => Status::Failed,
                };
                let expected = match status {
                    Status::Completed => "completed",
                    Status::Incomplete => "incomplete",
                    Status::Failed => "failed",
                };
                if response["status"].as_str() != Some(expected) {
                    return Err(Error::Protocol("terminal status mismatch"));
                }
                let mut calls = vec![];
                let mut call_ids = BTreeSet::new();
                let mut item_ids = BTreeSet::new();
                let output = response["output"]
                    .as_array()
                    .ok_or(Error::Protocol("terminal output"))?;
                if output.len() > 256 {
                    return Err(Error::Limit("terminal items"));
                }
                for item in output {
                    match item["type"].as_str() {
                        Some("function_call") => {
                            let id = identity(item, "id")?;
                            let call_id = identity(item, "call_id")?;
                            let name = identity(item, "name")?;
                            let arguments = item["arguments"]
                                .as_str()
                                .ok_or(Error::Protocol("terminal arguments"))?;
                            if !call_ids.insert(call_id.clone()) || !item_ids.insert(id.clone()) {
                                return Err(Error::Protocol("duplicate terminal call"));
                            }
                            if let Some(pending) = self.pending.get(&id) {
                                if pending.call_id != call_id
                                    || pending.name != name
                                    || pending.arguments != arguments
                                {
                                    return Err(Error::Protocol(
                                        "terminal call differs from stream",
                                    ));
                                }
                            }
                            if status == Status::Completed {
                                calls.push(Call {
                                    id: call_id,
                                    name: name.clone(),
                                    arguments: self.tools.validate_call(&name, arguments)?,
                                });
                            }
                        }
                        Some("message") => self.observe_message(item)?,
                        Some("reasoning") => (),
                        _ => return Err(Error::Capability("unsupported response item")),
                    }
                }
                if status == Status::Completed
                    && self.pending.keys().any(|id| !item_ids.contains(id))
                {
                    return Err(Error::Protocol("unfinished tool omitted from terminal"));
                }
                let usage = match response.get("usage").filter(|v| !v.is_null()) {
                    Some(raw) => Some(normalize_usage(raw)?),
                    None => None,
                };
                self.terminal = Some(ResultBody {
                    response_id: identity(response, "id")?,
                    served_model: optional_identity(response, "model")?,
                    served_provider: optional_identity(response, "provider")?,
                    status,
                    usage,
                    calls,
                    visible_text_bytes: self
                        .messages
                        .values()
                        .map(|text| text.trim().len() as u64)
                        .sum(),
                    raw_terminal_sha256: vcp_protocol::digest_bytes(text.as_bytes()),
                });
                self.terminal_data = Some(text.into());
                Ok(Some(Event::TerminalObserved))
            }
            "error" => Err(Error::Protocol(
                "provider error event; raw evidence retained",
            )),
            _ => {
                if kind.len() > 128 {
                    return Err(Error::Limit("event kind"));
                }
                Ok(Some(Event::Opaque { kind: kind.into() }))
            }
        }
    }
    fn observe_message(&mut self, item: &Value) -> Result<()> {
        if item["role"] != "assistant"
            || item
                .get("status")
                .is_some_and(|status| status != "completed")
        {
            return Err(Error::Protocol("completed assistant message required"));
        }
        let id = identity(item, "id")?;
        let content = item["content"]
            .as_array()
            .ok_or(Error::Protocol("message content"))?;
        let mut text = String::new();
        for part in content {
            let field = match part["type"].as_str() {
                Some("output_text") => "text",
                Some("refusal") => "refusal",
                _ => return Err(Error::Capability("unsupported assistant content")),
            };
            let part = part[field]
                .as_str()
                .ok_or(Error::Protocol("assistant text"))?;
            if text.len() + part.len() > 1024 * 1024 {
                return Err(Error::Limit("assistant message"));
            }
            text.push_str(part);
        }
        if let Some(previous) = self.messages.get(&id) {
            if previous != &text {
                return Err(Error::Protocol(
                    "terminal message differs from completed item",
                ));
            }
        } else {
            if self.messages.len() >= 256 {
                return Err(Error::Limit("assistant messages"));
            }
            self.messages.insert(id, text);
        }
        Ok(())
    }
    /// No executable proposal is exposed until the captured prefix ends on a
    /// frame boundary and a qualified terminal agrees with observed fragments.
    /// The retained client observes through terminal, not unseen trailing bytes.
    pub fn finish(mut self) -> Result<ResultBody> {
        if self.failed {
            return Err(Error::Protocol("invalid stream"));
        }
        if !self.line.is_empty() || !self.data.is_empty() {
            return Err(Error::Protocol("truncated SSE frame"));
        }
        self.terminal
            .take()
            .ok_or(Error::Protocol("EOF before response terminal"))
    }
}
fn identity(value: &Value, key: &str) -> Result<String> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
        .map(str::to_owned)
        .ok_or(Error::Protocol("response identity"))
}
fn optional_identity(value: &Value, key: &str) -> Result<Option<String>> {
    if value.get(key).is_none_or(Value::is_null) {
        Ok(None)
    } else {
        identity(value, key).map(Some)
    }
}
pub fn normalize_usage(raw: &Value) -> Result<ObservedUsage> {
    if !raw.is_object() {
        return Err(Error::Protocol("usage object"));
    }
    let numeric = |value: &Value| -> Result<Option<u64>> {
        if value.is_null() {
            Ok(None)
        } else {
            value
                .as_u64()
                .map(Some)
                .ok_or(Error::Protocol("nonnegative integer usage"))
        }
    };
    let input = numeric(&raw["input_tokens"])?;
    let output = numeric(&raw["output_tokens"])?;
    let total = numeric(&raw["total_tokens"])?;
    let cached = numeric(&raw["input_tokens_details"]["cached_tokens"])?;
    let reasoning = numeric(&raw["output_tokens_details"]["reasoning_tokens"])?;
    if let (Some(i), Some(o), Some(t)) = (input, output, total) {
        if i.checked_add(o) != Some(t) {
            return Err(Error::Protocol("inconsistent cumulative usage"));
        }
    }
    if let (Some(i), Some(c)) = (input, cached) {
        if c > i {
            return Err(Error::Protocol("cached usage exceeds input"));
        }
    }
    if let (Some(o), Some(r)) = (output, reasoning) {
        if r > o {
            return Err(Error::Protocol("reasoning exceeds output"));
        }
    }
    let tokens = match (input, output, cached, reasoning) {
        (Some(i), Some(o), Some(c), Some(r)) => Some(Usage {
            input: Units::new(i),
            output: Units::new(o),
            cache_read: Units::new(c),
            cache_write: Units::ZERO,
            reasoning: Units::new(r),
            requests: Units::new(1),
            provider_tools: Units::ZERO,
        }),
        _ => None,
    };
    let cost = match raw.get("cost").filter(|v| !v.is_null()) {
        Some(value) => {
            let decimal = match value {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                _ => return Err(Error::Decimal),
            };
            Some(Money {
                currency: "USD".to_owned().try_into().map_err(|_| Error::Decimal)?,
                micros: Micros::new(usd_micros(&decimal)?),
            })
        }
        None => None,
    };
    Ok(ObservedUsage {
        raw: raw.clone(),
        tokens,
        cost,
    })
}
