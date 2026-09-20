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

/// Trusted per-user configuration contains references, never credential values.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpServer {
    pub name: String,
    pub endpoint: String,
    pub credential: Option<CredentialSource>,
    pub allowed_tools: BTreeSet<String>,
    pub limits: vcp_extensions::mcp::registration::Limits,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialSource {
    pub reference: String,
    pub environment: String,
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}
pub fn validate_http(remote: &[HttpServer], local: &[Server]) -> Result<(), String> {
    if remote.len().saturating_add(local.len()) > 16 {
        return Err("MCP server limit is 16 across transports".into());
    }
    let mut names: BTreeSet<_> = local.iter().map(|server| server.name.as_str()).collect();
    for server in remote {
        if server.limits.timeout_ms > 120_000 {
            return Err("MCP HTTP timeout exceeds the 120-second transport ceiling".into());
        }
        if !identifier(&server.name) || !names.insert(&server.name) || server.endpoint.len() > 4096
        {
            return Err("MCP HTTP requires unique bounded server registrations".into());
        }
        if let Some(source) = &server.credential {
            let bytes = source.environment.as_bytes();
            if !identifier(&source.reference)
                || bytes.is_empty()
                || bytes.len() > 128
                || !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_')
                || !bytes
                    .iter()
                    .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
            {
                return Err("MCP HTTP credential requires an explicit reference and environment variable name".into());
            }
        }
        // This temporary identity validates configuration shape only. The real
        // workspace is bound by the owner at installation, before any send.
        let workspace = vcp_domain::WorkspaceId::new();
        vcp_extensions::mcp::registration::Registration {
            id: server.name.clone(),
            revision: vcp_domain::Revision::ZERO,
            scope: vcp_domain::policy::GrantScope::Workspace {
                workspace: workspace.clone(),
            },
            transport: vcp_extensions::mcp::registration::Transport::StreamableHttp {
                remote_profile: server.name.clone(),
                resolved_digest: "0".repeat(64),
            },
            auth_refs: server
                .credential
                .iter()
                .map(|source| source.reference.clone())
                .collect(),
            allowed_tools: server.allowed_tools.clone(),
            trusted_effects: Default::default(),
            limits: server.limits.clone(),
            capabilities: Default::default(),
        }
        .validate()
        .map_err(|error| error.to_string())?;
        #[cfg(windows)]
        server.profile(workspace)?;
    }
    Ok(())
}
#[cfg(windows)]
impl HttpServer {
    fn profile(
        &self,
        workspace: vcp_domain::WorkspaceId,
    ) -> Result<vcp_lifecycle::foundation::mcp::remote_authority::RemoteProfile, String> {
        use vcp_lifecycle::foundation::mcp::remote_authority::{
            RemoteProfile, RemoteProfileConfig,
        };
        RemoteProfile::new(RemoteProfileConfig {
            workspace,
            server: self.name.clone(),
            revision: vcp_domain::Revision::ZERO,
            endpoint: self.endpoint.clone(),
            credential_ref: self
                .credential
                .as_ref()
                .map(|source| source.reference.clone()),
        })
        .map_err(|error| error.to_string())
    }
}

#[cfg(windows)]
pub fn configure_http(
    host: &vcp_lifecycle::foundation::CanonicalHost,
    workspace: &vcp_domain::WorkspaceId,
    servers: &[HttpServer],
    deadline_seconds: u32,
) -> Result<(), String> {
    use vcp_lifecycle::foundation::mcp::{
        remote_authority::CredentialMaterial, RemoteRegistration,
    };
    if deadline_seconds == 0 || deadline_seconds > 3600 {
        return Err("MCP HTTP credential lifetime exceeds run bounds".into());
    }
    let expires = crate::settings::now()
        .get()
        .checked_add(u64::from(deadline_seconds) * 1000)
        .ok_or("MCP HTTP credential expiry overflow")?;
    for server in servers {
        let registration = RemoteRegistration::new(
            server.profile(workspace.clone())?,
            server.allowed_tools.clone(),
            server.limits.clone(),
        )?;
        host.configure_mcp_remote(registration)?;
        if let Some(source) = &server.credential {
            // Resolve exactly the trusted configured variable. No enumeration,
            // fallback, raw profile/argv token, or secret-bearing error text.
            let material = CredentialMaterial::bearer(
                std::env::var(&source.environment)
                    .map_err(|_| "configured MCP credential variable is unavailable")?,
            )
            .map_err(|_| "configured MCP credential format rejected")?;
            host.install_mcp_credential(
                &server.name,
                None,
                material,
                vcp_domain::Timestamp::new(expires),
            )?;
        }
    }
    Ok(())
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
    fn http_value() -> serde_json::Value {
        serde_json::json!({"name":"remote", "endpoint":"https://example.test/mcp",
            "credential":{"reference":"remote-key","environment":"VCP_SYNTHETIC_MCP_TOKEN"},
            "allowed_tools":["echo"],
            "limits":{"frame_bytes":4096,"total_discovery_bytes":8192,"tools":8,"pages":2,"timeout_ms":10000,"stderr_bytes":0}})
    }
    #[test]
    fn remote_configuration_references_credentials_and_rejects_raw_secrets() {
        let server: HttpServer = serde_json::from_value(http_value()).unwrap();
        validate_http(std::slice::from_ref(&server), &[]).unwrap();
        assert!(validate_http(&[server.clone(), server], &[]).is_err());
        for field in ["token", "authorization", "headers", "password"] {
            let mut value = http_value();
            value[field] = "synthetic-credential".into();
            assert!(serde_json::from_value::<HttpServer>(value).is_err());
        }
        for environment in ["", "bad=name", "has space", "1INVALID", "bad\nname"] {
            let mut value = http_value();
            value["credential"]["environment"] = environment.into();
            let server = serde_json::from_value(value).unwrap();
            assert!(validate_http(&[server], &[]).is_err());
        }
        let mut value = http_value();
        value["credential"] = serde_json::Value::Null;
        validate_http(&[serde_json::from_value(value).unwrap()], &[]).unwrap();
        let mut value = http_value();
        value["limits"]["frame_bytes"] = 0.into();
        assert!(validate_http(&[serde_json::from_value(value).unwrap()], &[]).is_err());
        let mut value = http_value();
        value["limits"]["timeout_ms"] = 120_000.into();
        validate_http(&[serde_json::from_value(value.clone()).unwrap()], &[]).unwrap();
        value["limits"]["timeout_ms"] = 120_001.into();
        assert!(validate_http(&[serde_json::from_value(value).unwrap()], &[]).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn remote_configuration_rejects_ambiguous_or_credential_bearing_endpoints() {
        for endpoint in [
            "http://example.test/mcp",
            "https://secret@example.test/mcp",
            "https://example.test/mcp?token=synthetic",
            "https://example.test/mcp#fragment",
            "https://example.test/a/../mcp",
        ] {
            let mut value = http_value();
            value["endpoint"] = endpoint.into();
            assert!(validate_http(&[serde_json::from_value(value).unwrap()], &[]).is_err());
        }
    }
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
