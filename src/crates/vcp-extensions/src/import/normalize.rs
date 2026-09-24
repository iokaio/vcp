// SPDX-License-Identifier: Apache-2.0
use super::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub servers: BTreeMap<String, Restriction>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Restriction {
    pub allowed_tools: Option<BTreeSet<String>>,
    pub timeout_ms: Option<u64>,
}
impl Preferences {
    pub fn validate(&self) -> Result<()> {
        if self.servers.len() > 128 {
            return Err(Error::Limit("server preferences"));
        }
        for (name, value) in &self.servers {
            if !super::identifier(name) {
                return Err(Error::Invalid("server preference identity"));
            }
            value.validate()?;
        }
        if serde_json::to_vec(self)
            .map_err(|_| Error::Invalid("preference serialization"))?
            .len()
            > 1_048_576
        {
            return Err(Error::Limit("preference bytes"));
        }
        Ok(())
    }
}
impl Restriction {
    pub fn validate(&self) -> Result<()> {
        if let Some(tools) = &self.allowed_tools {
            validate_tools(tools)?;
        }
        if self.timeout_ms.is_some_and(|v| v == 0 || v > 120_000) {
            return Err(Error::Invalid("timeout ceiling"));
        }
        Ok(())
    }
}
pub(crate) fn validate_tools(tools: &BTreeSet<String>) -> Result<()> {
    if tools.len() > 4096 {
        return Err(Error::Limit("tool names"));
    }
    if tools
        .iter()
        .any(|v| v.is_empty() || v.len() > 256 || v.chars().any(char::is_control))
    {
        return Err(Error::Invalid("tool name"));
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub servers: Vec<Server>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Server {
    pub name: String,
    pub transport: Transport,
    pub allowed_tools: BTreeSet<String>,
    pub timeout_ms: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Transport {
    Http {
        endpoint: String,
        credential_environment: Option<String>,
    },
    /// Main adapter supplies the existing pinned absolute executable and exact
    /// literal arguments/directory; no PATH resolution is performed by imports.
    Stdio {
        command: String,
        args: Vec<String>,
        cwd: String,
    },
}
impl Context {
    pub fn validate(&self) -> Result<()> {
        if self.servers.len() > 128 {
            return Err(Error::Limit("registered servers"));
        }
        let mut names = BTreeSet::new();
        for server in &self.servers {
            if !super::identifier(&server.name) || !names.insert(&server.name) {
                return Err(Error::Invalid("duplicate or invalid target server"));
            }
            Restriction {
                allowed_tools: Some(server.allowed_tools.clone()),
                timeout_ms: Some(server.timeout_ms),
            }
            .validate()?;
            match &server.transport {
                Transport::Http {
                    endpoint,
                    credential_environment,
                } => {
                    if endpoint.is_empty()
                        || endpoint.len() > 4096
                        || endpoint.chars().any(char::is_control)
                        || credential_environment
                            .as_ref()
                            .is_some_and(|v| !super::identifier(v))
                    {
                        return Err(Error::Invalid("target transport"));
                    }
                }
                Transport::Stdio { command, args, cwd } => {
                    if command.is_empty()
                        || command.len() > 4096
                        || cwd.len() > 4096
                        || args.len() > 256
                        || args.iter().any(|v| v.len() > 4096)
                        || command.contains('\0')
                        || cwd.contains('\0')
                        || args.iter().any(|v| v.contains('\0'))
                    {
                        return Err(Error::Invalid("target transport"));
                    }
                }
            }
        }
        if serde_json::to_vec(self)
            .map_err(|_| Error::Invalid("target serialization"))?
            .len()
            > 1_048_576
        {
            return Err(Error::Limit("target bytes"));
        }
        Ok(())
    }
}
/// Runtime and rollback share the same intersection/minimum semantics. Stored
/// preferences cannot resurrect removed servers or exceed today's trusted base.
pub fn effective(
    context: &Context,
    preferences: &Preferences,
) -> Result<BTreeMap<String, Restriction>> {
    context.validate()?;
    preferences.validate()?;
    Ok(context
        .servers
        .iter()
        .map(|server| {
            let preference = preferences.servers.get(&server.name);
            let allowed_tools = preference
                .and_then(|p| p.allowed_tools.as_ref())
                .map_or_else(
                    || server.allowed_tools.clone(),
                    |tools| server.allowed_tools.intersection(tools).cloned().collect(),
                );
            let timeout_ms = preference
                .and_then(|p| p.timeout_ms)
                .map_or(server.timeout_ms, |timeout| server.timeout_ms.min(timeout));
            (
                server.name.clone(),
                Restriction {
                    allowed_tools: Some(allowed_tools),
                    timeout_ms: Some(timeout_ms),
                },
            )
        })
        .collect())
}
