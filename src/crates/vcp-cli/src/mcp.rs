// SPDX-License-Identifier: Apache-2.0
//! Explicit configured server controls; text and schemas never grant authority.
use serde::Deserialize;
use std::collections::BTreeSet;

pub const HELP: &str = "/mcp list <server> | /mcp call <server> <tool> <identity-digest> <json-object> | /mcp disconnect <server>";

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Server {
    pub name: String,
    pub process: vcp_tools::process::Request,
    pub allowed_tools: BTreeSet<String>,
    pub limits: vcp_extensions::mcp::registration::Limits,
}
pub fn validate(servers: &[Server], profiles: &BTreeSet<String>) -> Result<(), String> {
    if servers.len() > 16 {
        return Err("MCP server limit is 16".into());
    }
    let mut names = BTreeSet::new();
    for server in servers {
        if server.name.is_empty()
            || server.name.len() > 128
            || !server
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            || !names.insert(&server.name)
            || !profiles.contains(&server.process.profile)
        {
            return Err("MCP requires unique server names and configured process profiles".into());
        }
    }
    Ok(())
}
#[cfg(windows)]
impl Server {
    pub fn registration(&self) -> vcp_lifecycle::foundation::mcp::Registration {
        vcp_lifecycle::foundation::mcp::Registration {
            name: self.name.clone(),
            process: self.process.clone(),
            allowed_tools: self.allowed_tools.clone(),
            limits: self.limits.clone(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    List {
        server: String,
    },
    Call {
        server: String,
        tool: String,
        identity_digest: String,
        arguments_json: String,
    },
    Disconnect {
        server: String,
    },
}
fn token(input: &mut &str) -> Result<String, String> {
    *input = input.trim_start();
    let end = input.find(char::is_whitespace).unwrap_or(input.len());
    let value = &input[..end];
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(format!("Use {HELP}"));
    }
    let value = value.to_owned();
    *input = &input[end..];
    Ok(value)
}
pub fn parse(mut input: &str) -> Result<Command, String> {
    let action = token(&mut input)?;
    let server = token(&mut input)?;
    if server.len() > 128 {
        return Err("MCP server name limit".into());
    }
    match action.as_str() {
        "list" if input.trim().is_empty() => Ok(Command::List { server }),
        "disconnect" if input.trim().is_empty() => Ok(Command::Disconnect { server }),
        "call" => {
            let tool = token(&mut input)?;
            let identity_digest = token(&mut input)?;
            let arguments_json = input.trim().to_owned();
            if identity_digest.len() != 64
                || !identity_digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || arguments_json.is_empty()
                || arguments_json.len() > 64 * 1024
            {
                return Err(
                    "MCP call requires a listed identity digest and bounded JSON arguments".into(),
                );
            }
            // Preserve original JSON bytes, including whitespace within strings and
            // duplicate keys. The canonical schema boundary performs validation.
            Ok(Command::Call {
                server,
                tool,
                identity_digest,
                arguments_json,
            })
        }
        _ => Err(format!("Use {HELP}")),
    }
}
#[cfg(windows)]
impl Command {
    fn request(self) -> vcp_lifecycle::foundation::mcp::Request {
        use vcp_lifecycle::foundation::mcp::Request;
        match self {
            Self::List { server } => Request::List { server },
            Self::Disconnect { server } => Request::Disconnect { server },
            Self::Call {
                server,
                tool,
                identity_digest,
                arguments_json,
            } => Request::Call {
                server,
                tool,
                identity_digest,
                arguments_json,
            },
        }
    }
}

/// Keep terminal pause/cancel responsive while the server is waiting.
#[cfg(windows)]
pub struct Running(tokio::task::JoinHandle<Result<serde_json::Value, String>>);
#[cfg(windows)]
impl Running {
    pub fn start(
        host: vcp_lifecycle::foundation::CanonicalHost,
        thread: codex_protocol::ThreadId,
        command: Command,
    ) -> Self {
        Self(tokio::spawn(async move {
            host.mcp_control(thread, command.request()).await
        }))
    }
    pub fn is_finished(&self) -> bool {
        self.0.is_finished()
    }
    pub async fn result(&mut self) -> Result<serde_json::Value, String> {
        (&mut self.0)
            .await
            .map_err(|_| "MCP control observer stopped".to_owned())?
    }
}
#[cfg(windows)]
impl Drop for Running {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commands_preserve_exact_argument_json_and_require_explicit_identity() {
        let digest = "a".repeat(64);
        let arguments = r#"{"text":"keep  two spaces", "x":1,"x":2}"#;
        assert_eq!(
            parse(&format!("call fixture echo {digest} {arguments}")),
            Ok(Command::Call {
                server: "fixture".into(),
                tool: "echo".into(),
                identity_digest: digest,
                arguments_json: arguments.into()
            })
        );
        for input in [
            "",
            "list",
            "list fixture extra",
            "disconnect",
            "call fixture echo guessed {}",
        ] {
            assert!(parse(input).is_err());
        }
        assert_eq!(
            parse("list fixture"),
            Ok(Command::List {
                server: "fixture".into()
            })
        );
    }
}
