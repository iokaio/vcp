// SPDX-License-Identifier: Apache-2.0
//! Prepared protocol operations share canonical admission; content is never authority.
use super::*;
use vcp_mcp::{
    client::{CallReply, Outbound},
    identity::{ConnectionIdentity, ContentIdentity, ToolIdentity},
    schema::CheckedArguments,
};

#[derive(Clone)]
pub(in crate::foundation) enum PreparedOperation {
    Tools {
        connection: Option<ConnectionIdentity>,
    },
    Resources {
        connection: Option<ConnectionIdentity>,
    },
    Prompts {
        connection: Option<ConnectionIdentity>,
    },
    Tool {
        identity: ToolIdentity,
        arguments: CheckedArguments,
    },
    Resource {
        identity: ContentIdentity,
    },
    Prompt {
        identity: ContentIdentity,
        arguments: CheckedArguments,
    },
}
impl PreparedOperation {
    pub(in crate::foundation) fn connection(&self) -> Option<&ConnectionIdentity> {
        match self {
            Self::Tools { connection }
            | Self::Resources { connection }
            | Self::Prompts { connection } => connection.as_ref(),
            Self::Tool { identity, .. } => Some(identity.connection()),
            Self::Resource { identity } | Self::Prompt { identity, .. } => {
                Some(identity.connection())
            }
        }
    }
    pub(super) fn reset_discovery_connection(&mut self) {
        match self {
            Self::Tools { connection }
            | Self::Resources { connection }
            | Self::Prompts { connection } => *connection = None,
            _ => {}
        }
    }
    pub(super) fn discovery(&self) -> bool {
        matches!(
            self,
            Self::Tools { .. } | Self::Resources { .. } | Self::Prompts { .. }
        )
    }
    pub(super) fn resource(&self) -> Option<&ContentIdentity> {
        if let Self::Resource { identity } = self {
            Some(identity)
        } else {
            None
        }
    }
    pub(in crate::foundation) fn arguments(&self) -> Option<&CheckedArguments> {
        match self {
            Self::Tool { arguments, .. } | Self::Prompt { arguments, .. } => Some(arguments),
            _ => None,
        }
    }
    /// Discovery observes a catalog, not a call to an identified member.
    /// Preserve the receipt contract independently of richer admission evidence.
    pub(super) fn receipt_identity(&self) -> serde_json::Value {
        match self {
            Self::Tools { .. } | Self::Resources { .. } | Self::Prompts { .. } => {
                serde_json::Value::Null
            }
            Self::Tool { identity, .. } => serde_json::json!(identity),
            Self::Resource { identity } | Self::Prompt { identity, .. } => {
                serde_json::json!(identity)
            }
        }
    }
    pub(in crate::foundation) fn evidence(&self) -> serde_json::Value {
        match self {
            Self::Tools { connection } => {
                serde_json::json!({"kind":"tools","connection":connection})
            }
            Self::Resources { connection } => {
                serde_json::json!({"kind":"resources","connection":connection})
            }
            Self::Prompts { connection } => {
                serde_json::json!({"kind":"prompts","connection":connection})
            }
            Self::Tool { identity, .. } => serde_json::json!(identity),
            Self::Resource { identity } => {
                serde_json::json!({"kind":"resource","identity":identity})
            }
            Self::Prompt { identity, .. } => {
                serde_json::json!({"kind":"prompt","identity":identity})
            }
        }
    }
}
macro_rules! operation_methods {
    ($select:ident,$validate:ident,$begin:ident,$client:ty) => {
        impl PreparedOperation {
            pub(super) fn $select(
                request: &Request,
                client: Option<&$client>,
            ) -> Result<Self, String> {
                let connection = client.and_then(|c| c.connection()).cloned();
                let selected = match request {
                    Request::List { .. } => Self::Tools { connection },
                    Request::Resources { .. } => Self::Resources { connection },
                    Request::Prompts { .. } => Self::Prompts { connection },
                    Request::Call {
                        tool,
                        identity_digest,
                        arguments_json,
                        ..
                    } => {
                        if arguments_json.len() > 128 * 1024 {
                            return Err("MCP argument ceiling".into());
                        }
                        let entry = client
                            .ok_or("MCP discovery required")?
                            .tools()
                            .find(|v| v.name == *tool)
                            .ok_or("MCP tool not discovered")?;
                        if entry.identity.digest().map_err(|e| e.to_string())? != *identity_digest {
                            return Err("MCP identity changed".into());
                        }
                        Self::Tool {
                            identity: entry.identity.clone(),
                            arguments: entry
                                .check_arguments(arguments_json.as_bytes())
                                .map_err(|e| e.to_string())?,
                        }
                    }
                    Request::ReadResource {
                        uri,
                        identity_digest,
                        ..
                    } => {
                        let entry = client
                            .ok_or("MCP discovery required")?
                            .resources()
                            .find(|v| v.uri == *uri)
                            .ok_or("MCP resource not discovered")?;
                        if entry.identity.digest().map_err(|e| e.to_string())? != *identity_digest {
                            return Err("MCP identity changed".into());
                        }
                        Self::Resource {
                            identity: entry.identity.clone(),
                        }
                    }
                    Request::GetPrompt {
                        prompt,
                        identity_digest,
                        arguments_json,
                        ..
                    } => {
                        if arguments_json.len() > 128 * 1024 {
                            return Err("MCP argument ceiling".into());
                        }
                        let entry = client
                            .ok_or("MCP discovery required")?
                            .prompts()
                            .find(|v| v.name == *prompt)
                            .ok_or("MCP prompt not discovered")?;
                        if entry.identity.digest().map_err(|e| e.to_string())? != *identity_digest {
                            return Err("MCP identity changed".into());
                        }
                        Self::Prompt {
                            identity: entry.identity.clone(),
                            arguments: entry
                                .check_arguments(arguments_json.as_bytes())
                                .map_err(|e| e.to_string())?,
                        }
                    }
                    _ => return Err("MCP protocol operation required".into()),
                };
                Ok(selected)
            }
            pub(super) fn $validate(&self, client: &$client) -> Result<(), String> {
                if self.connection().is_some() && self.connection() != client.connection() {
                    return Err("MCP connection changed".into());
                }
                match self {
                    Self::Tool { identity, .. } => {
                        client.tool(identity).map_err(|e| e.to_string())?;
                    }
                    Self::Resource { identity } => {
                        client.resource(identity).map_err(|e| e.to_string())?;
                    }
                    Self::Prompt { identity, .. } => {
                        client.prompt(identity).map_err(|e| e.to_string())?;
                    }
                    _ => {}
                }
                Ok(())
            }
            pub(super) fn $begin(&self, client: &mut $client) -> Result<Outbound, String> {
                self.$validate(client)?;
                match self {
                    Self::Tools { .. } => client.list_tools(),
                    Self::Resources { .. } => client.list_resources(),
                    Self::Prompts { .. } => client.list_prompts(),
                    Self::Tool {
                        identity,
                        arguments,
                    } => client.call(identity, arguments),
                    Self::Resource { identity } => client.read_resource(identity),
                    Self::Prompt {
                        identity,
                        arguments,
                    } => client.get_prompt(identity, arguments),
                }
                .map_err(|e| e.to_string())
            }
        }
    };
}
operation_methods!(select_client, validate_client, begin_client, Client);
operation_methods!(
    select_session,
    validate_session,
    begin_session,
    vcp_mcp::http::Session
);

pub(super) struct Observed {
    pub value: serde_json::Value,
    pub success: bool,
}
pub(super) fn observe(incoming: Incoming, server: &str) -> Result<Option<Observed>, String> {
    let value = match incoming {
        Incoming::DiscoveryPage { next: true }
        | Incoming::ResourceDiscoveryPage { next: true }
        | Incoming::PromptDiscoveryPage { next: true } => return Ok(None),
        Incoming::DiscoveryComplete { tools, rejected } => {
            let metadata=tools.into_iter().map(|tool|Ok(serde_json::json!({"server":server,"tool":tool.name,"qualified_name":format!("{}::{}",server,tool.name),"identity":tool.identity,"identity_digest":tool.identity.digest().map_err(|e|e.to_string())?,"input_schema":tool.input_schema,"description":tool.description,"annotations_are_hints":true}))).collect::<Result<Vec<_>,String>>()?;
            serde_json::json!({"catalog":{"tools":metadata,"rejected":rejected,"external_content":true},"connected":true})
        }
        Incoming::ResourceDiscoveryComplete {
            resources,
            rejected,
        } => {
            let metadata = resources
                .into_iter()
                .map(|item| {
                    let digest = item.identity.digest().map_err(|e| e.to_string())?;
                    let mut value = serde_json::to_value(item).map_err(|e| e.to_string())?;
                    value["identity_digest"] = digest.into();
                    value["server"] = server.into();
                    Ok(value)
                })
                .collect::<Result<Vec<_>, String>>()?;
            serde_json::json!({"catalog":{"resources":metadata,"rejected":rejected,"external_content":true},"connected":true})
        }
        Incoming::PromptDiscoveryComplete { prompts, rejected } => {
            let metadata = prompts
                .into_iter()
                .map(|item| {
                    let digest = item.identity.digest().map_err(|e| e.to_string())?;
                    let mut value = serde_json::to_value(item).map_err(|e| e.to_string())?;
                    value["identity_digest"] = digest.into();
                    value["server"] = server.into();
                    Ok(value)
                })
                .collect::<Result<Vec<_>, String>>()?;
            serde_json::json!({"catalog":{"prompts":metadata,"rejected":rejected,"external_content":true},"connected":true})
        }
        Incoming::CallReply(reply) => {
            let success = matches!(&reply,CallReply::ToolResult(value) if !value.is_error);
            return Ok(Some(Observed {
                value: serde_json::json!({"result":reply}),
                success,
            }));
        }
        Incoming::ResourceReply(reply) => serde_json::json!({"result":reply}),
        Incoming::PromptReply(reply) => {
            serde_json::json!({"result":reply,"roles_are_external_data":true})
        }
        Incoming::RpcError { error, .. } => {
            return Ok(Some(Observed {
                value: serde_json::json!({"rpc_error":error}),
                success: false,
            }))
        }
        _ => return Err("MCP response type rejected".into()),
    };
    Ok(Some(Observed {
        value,
        success: true,
    }))
}

#[derive(Clone)]
pub(super) struct CachedResource {
    pub identity: ContentIdentity,
    pub scope: vcp_domain::workspace::Scope,
    pub artifact: ArtifactId,
}

impl CanonicalHost {
    pub(super) fn mcp_cached(
        &self,
        thread: ThreadId,
        slot: &Slot,
        artifact: &ArtifactId,
        mut provenance: Provenance,
    ) -> Result<ControlOutcome, String> {
        let connection = slot
            .connection
            .as_ref()
            .ok_or("MCP resource cache requires a current connection")?;
        let entry = connection
            .cache
            .get(artifact)
            .ok_or("MCP cached resource unavailable")?
            .clone();
        connection
            .client
            .resource(&entry.identity)
            .map_err(|e| e.to_string())?;
        // A retained descriptor is not proof that its stdio connection remains live.
        // This observes the pinned generation and owned job without sending bytes.
        connection.process.resume_identity()?;
        let binding = self.binding(thread)?;
        if entry.scope != binding.scope {
            return Err("MCP cached resource scope rejected".into());
        }
        let process = connection.prepared.clone();
        let server = connection.registration.id.clone();
        let expected = connection
            .registration
            .digest()
            .map_err(|e| e.to_string())?;
        self.worker.run(move |context| {
            if provenance.revisions.is_none() {
                provenance.revisions = Some(context.context_revisions(&binding)?);
            }
            context.validate_mcp_provenance(&binding, &provenance)?;
            let (_, current) = context.mcp_registration(&binding, &server)?;
            if current.digest()? != expected
                || !current.allowed_resources.contains(entry.identity.key())
            {
                return Err("MCP cached resource registration changed".into());
            }
            if !matches!(
                context.process_decision(&binding, &process)?,
                vcp_policy::Decision::Allow { .. }
            ) {
                return Err("MCP cached resource authority rejected".into());
            }
            context.read_mcp_cache(&binding, &entry.artifact)
        })
    }
}
