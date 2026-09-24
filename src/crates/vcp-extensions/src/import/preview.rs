// SPDX-License-Identifier: Apache-2.0
use super::{
    compatibility::Format,
    normalize::{effective, Context, Transport},
    Error, Preferences, Restriction, Result,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    AllowedTools,
    TimeoutMs,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub id: String,
    pub server: String,
    pub field: Field,
    pub old: Restriction,
    pub new: Restriction,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Mapped,
    Ignored,
    Unsupported,
    Conflicting,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub location: String,
    pub status: Status,
    pub reason: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preview {
    pub schema_version: u32,
    pub format: Format,
    pub source_sha256: String,
    pub context_digest: String,
    pub preferences_digest: String,
    pub changes: Vec<Change>,
    pub diagnostics: Vec<Diagnostic>,
    pub digest: String,
}
impl Preview {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 || self.changes.len() > 256 || self.diagnostics.len() > 16384 {
            return Err(Error::Invalid("preview bounds"));
        }
        let mut copy = self.clone();
        copy.digest.clear();
        if self.digest != super::digest(&copy)? {
            return Err(Error::Stale);
        }
        Ok(())
    }
}
fn record(
    value: &Value,
    location: String,
    status: Status,
    reason: &str,
    out: &mut Vec<Diagnostic>,
    depth: usize,
) -> Result<()> {
    if depth > 16 || out.len() >= 16384 {
        return Err(Error::Limit("source fields"));
    }
    out.push(Diagnostic {
        location: location.clone(),
        status: status.clone(),
        reason: reason.into(),
    });
    match value {
        Value::Object(map) => {
            for (index, (_, child)) in map.iter().enumerate() {
                record(
                    child,
                    format!("{location}.field-{index}"),
                    status.clone(),
                    reason,
                    out,
                    depth + 1,
                )?;
            }
        }
        Value::Array(list) => {
            for (index, child) in list.iter().enumerate() {
                record(
                    child,
                    format!("{location}[{index}]"),
                    status.clone(),
                    reason,
                    out,
                    depth + 1,
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn names(value: &Value, gemini: bool) -> Option<BTreeSet<String>> {
    let values = value.as_array()?;
    if values.len() > 4096 {
        return None;
    }
    let mut result = BTreeSet::new();
    for value in values {
        let name = value.as_str()?;
        if name.is_empty()
            || name.len() > 256
            || name.chars().any(char::is_control)
            || (gemini && (name.contains('(') || name.contains(')')))
            || !result.insert(name.to_owned())
        {
            return None;
        }
    }
    Some(result)
}
pub fn preview(
    bytes: &[u8],
    format: &Format,
    context: &Context,
    current: &Preferences,
) -> Result<Preview> {
    if bytes.len() > 524288 {
        return Err(Error::Limit("source bytes"));
    }
    let effective = effective(context, current)?;
    let is_gemini = matches!(format, Format::Gemini6a466a7e);
    let source = if is_gemini {
        super::gemini::parse(bytes)?
    } else {
        super::codex::parse(bytes)?
    };
    let root = source.as_object().ok_or(Error::Invalid("source object"))?;
    if root.contains_key("schema_version") || root.contains_key("version") {
        return Err(Error::Version);
    }
    let mut result = Preview {
        schema_version: 1,
        format: *format,
        source_sha256: vcp_protocol::digest_bytes(bytes),
        context_digest: super::digest(context)?,
        preferences_digest: super::digest(current)?,
        changes: vec![],
        diagnostics: vec![],
        digest: String::new(),
    };
    let container = if is_gemini {
        "mcpServers"
    } else {
        "mcp_servers"
    };
    for (top_index, (key, value)) in root.iter().enumerate() {
        if key != container {
            record(
                value,
                format!("field-{top_index}"),
                Status::Unsupported,
                "outside restriction-only compatibility subset",
                &mut result.diagnostics,
                0,
            )?;
            continue;
        }
        let servers = value
            .as_object()
            .ok_or(Error::Invalid("MCP server object"))?;
        if servers.len() > 128 {
            return Err(Error::Limit("source servers"));
        }
        result.diagnostics.push(Diagnostic {
            location: container.into(),
            status: Status::Mapped,
            reason: "existing server restriction container".into(),
        });
        for (index, (name, settings)) in servers.iter().enumerate() {
            let location = format!("{container}.server-{index}");
            let Some(server) = context.servers.iter().find(|s| s.name == *name) else {
                record(
                    settings,
                    location,
                    Status::Unsupported,
                    "no existing target server",
                    &mut result.diagnostics,
                    0,
                )?;
                continue;
            };
            let Some(map) = settings.as_object() else {
                record(
                    settings,
                    location,
                    Status::Conflicting,
                    "server must be an object",
                    &mut result.diagnostics,
                    0,
                )?;
                continue;
            };
            let forbidden = [
                "env",
                "env_vars",
                "headers",
                "http_headers",
                "env_http_headers",
                "http_headers_helper",
                "bearer_token",
                "auth",
                "oauth",
                "tcp",
            ]
            .iter()
            .any(|key| map.contains_key(*key));
            let matched = !forbidden
                && match &server.transport {
                    Transport::Http {
                        endpoint,
                        credential_environment,
                    } => {
                        let url = if is_gemini {
                            if map.get("type").and_then(Value::as_str) == Some("http") {
                                map.get("url").or_else(|| map.get("httpUrl"))
                            } else {
                                map.get("httpUrl")
                            }
                        } else {
                            map.get("url")
                        };
                        let credential = map.get("bearer_token_env_var");
                        !map.contains_key("command")
                            && !map.contains_key("args")
                            && !map.contains_key("cwd")
                            && map
                                .get("type")
                                .is_none_or(|value| is_gemini && value.as_str() == Some("http"))
                            && (!is_gemini || credential.is_none())
                            && !(map.contains_key("url") && map.contains_key("httpUrl"))
                            && url.and_then(Value::as_str) == Some(endpoint.as_str())
                            && credential.is_none_or(|v| {
                                v.as_str() == credential_environment.as_deref() && v.is_string()
                            })
                    }
                    Transport::Stdio { command, args, cwd } => {
                        !map.contains_key("url")
                            && !map.contains_key("httpUrl")
                            // The pinned Gemini schema's only explicit types
                            // are HTTP/SSE. Stdio is selected by command alone.
                            && !map.contains_key("type")
                            && !map.contains_key("bearer_token_env_var")
                            && map.get("command").and_then(Value::as_str) == Some(command.as_str())
                            && map.get("cwd").and_then(Value::as_str) == Some(cwd.as_str())
                            && map.get("args").map_or(args.is_empty(), |v| {
                                serde_json::from_value::<Vec<String>>(v.clone())
                                    .is_ok_and(|v| v == *args)
                            })
                    }
                };
            if !matched {
                let missing_reference = matches!(
                    &server.transport,
                    Transport::Http {
                        credential_environment: None,
                        ..
                    }
                ) && map
                    .get("bearer_token_env_var")
                    .is_some_and(Value::is_string);
                record(
                    settings,
                    location,
                    Status::Conflicting,
                    if missing_reference {
                        "source credential reference has no matching native reference; configure the native credential reference explicitly"
                    } else {
                        "transport identity or credential reference not exactly matched"
                    },
                    &mut result.diagnostics,
                    0,
                )?;
                continue;
            }
            result.diagnostics.push(Diagnostic {
                location: location.clone(),
                status: Status::Mapped,
                reason: "existing transport identity matched without creating authority".into(),
            });
            let include = if is_gemini {
                "includeTools"
            } else {
                "enabled_tools"
            };
            let exclude = if is_gemini {
                "excludeTools"
            } else {
                "disabled_tools"
            };
            let old = effective
                .get(name)
                .ok_or(Error::Invalid("target missing"))?;
            let mut tools = old
                .allowed_tools
                .clone()
                .ok_or(Error::Invalid("target tools"))?;
            let mut tools_valid = true;
            for key in [include, exclude] {
                if let Some(value) = map.get(key) {
                    if let Some(set) = names(value, is_gemini) {
                        if key == include {
                            tools = tools.intersection(&set).cloned().collect();
                        } else {
                            tools = tools.difference(&set).cloned().collect();
                        }
                    } else {
                        tools_valid = false;
                    }
                }
            }
            let mut timeout = old.timeout_ms.ok_or(Error::Invalid("target timeout"))?;
            let timeout_keys: &[&str] = if is_gemini {
                &["timeout"]
            } else {
                &["startup_timeout_sec", "tool_timeout_sec"]
            };
            let mut timeout_valid = true;
            for key in timeout_keys {
                if let Some(value) = map.get(*key) {
                    if let Some(value) = value
                        .as_u64()
                        .and_then(|v| v.checked_mul(if is_gemini { 1 } else { 1000 }))
                        .filter(|v| *v > 0)
                    {
                        timeout = timeout.min(value);
                    } else {
                        timeout_valid = false;
                    }
                }
            }
            if tools_valid && old.allowed_tools.as_ref() != Some(&tools) {
                let mut new = old.clone();
                new.allowed_tools = Some(tools);
                result.changes.push(Change {
                    id: format!("server.{name}.allowed_tools"),
                    server: name.clone(),
                    field: Field::AllowedTools,
                    old: old.clone(),
                    new,
                });
            }
            if timeout_valid && old.timeout_ms != Some(timeout) {
                let mut new = old.clone();
                new.timeout_ms = Some(timeout);
                result.changes.push(Change {
                    id: format!("server.{name}.timeout_ms"),
                    server: name.clone(),
                    field: Field::TimeoutMs,
                    old: old.clone(),
                    new,
                });
            }
            for (field_index, (key, value)) in map.iter().enumerate() {
                let (status, reason) = if key == include || key == exclude {
                    if tools_valid {
                        (
                            Status::Mapped,
                            "allowlist intersection; excluded tools removed",
                        )
                    } else {
                        (
                            Status::Conflicting,
                            "invalid tool selector; tool restriction not mapped",
                        )
                    }
                } else if timeout_keys.contains(&key.as_str()) {
                    if timeout_valid {
                        (
                            Status::Mapped,
                            "timeout minimum applies to all VCP MCP phases",
                        )
                    } else {
                        (
                            Status::Conflicting,
                            "timeout must be a positive bounded integer",
                        )
                    }
                } else if [
                    "url",
                    "httpUrl",
                    "type",
                    "command",
                    "args",
                    "cwd",
                    "bearer_token_env_var",
                ]
                .contains(&key.as_str())
                {
                    (
                        Status::Ignored,
                        "matched identity only; target transport unchanged",
                    )
                } else {
                    (
                        Status::Unsupported,
                        "field excluded from restriction-only compatibility subset",
                    )
                };
                record(
                    value,
                    format!("{location}.field-{field_index}"),
                    status,
                    reason,
                    &mut result.diagnostics,
                    0,
                )?;
            }
        }
    }
    result.digest = super::digest(&result)?;
    result.validate()?;
    Ok(result)
}
