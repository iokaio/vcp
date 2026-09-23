// SPDX-License-Identifier: Apache-2.0
//! Initialization negotiates compatibility only; it never grants authority.
use crate::jsonrpc::{RpcError, MAX_FRAME_BYTES};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeSet;

pub const PROTOCOL_VERSION: &str = "1.0";
pub const EVENT_SCHEMA_VERSION: &str = "1.0";
pub const SCHEMA_VERSION: &str = "1.0";
// Profile bounds are Unicode scalar lengths (matching JSON Schema maxLength),
// not UTF-8 byte lengths. The transport separately caps total frame bytes.
pub const MAX_CLIENT_NAME: usize = 128;
pub const MAX_CLIENT_VERSION: usize = 64;
pub const MAX_CAPABILITY_NAME: usize = 128;
pub const MAX_CAPABILITIES: usize = 128;

#[cfg(feature = "schema")]
fn capability_list_schema(_: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
    use schemars::schema::{ArrayValidation, InstanceType, Schema, SchemaObject, StringValidation};
    let item = Schema::Object(SchemaObject {
        instance_type: Some(InstanceType::String.into()),
        string: Some(Box::new(StringValidation {
            min_length: Some(1),
            max_length: Some(MAX_CAPABILITY_NAME as u32),
            ..Default::default()
        })),
        ..Default::default()
    });
    SchemaObject {
        instance_type: Some(InstanceType::Array.into()),
        array: Some(Box::new(ArrayValidation {
            items: Some(item.into()),
            max_items: Some(MAX_CAPABILITIES as u32),
            ..Default::default()
        })),
        ..Default::default()
    }
    .into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ClientInfo {
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 128)))]
    pub name: String,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 64)))]
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct InitializeParams {
    pub protocol_version: String,
    pub client: ClientInfo,
    /// Optional capabilities: absence at the server does not reject a peer.
    #[cfg_attr(feature = "schema", schemars(schema_with = "capability_list_schema"))]
    pub capabilities: Vec<String>,
    /// Required capabilities fail closed, including unknown names.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(schema_with = "capability_list_schema"))]
    pub required_capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ConnectionLimits {
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 16777216)))]
    pub maximum_frame_bytes: u32,
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 1024)))]
    pub maximum_pending_requests: u32,
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 128)))]
    pub maximum_subscriptions: u32,
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 16777216)))]
    pub maximum_subscriber_queue_bytes: u32,
}

/// Host facts are supplied by the authenticated runtime, never client claims.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ExecutionHost {
    pub id: String,
    pub platform: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct InitializeResult {
    pub protocol_version: String,
    pub event_schema_version: String,
    pub schema_version: String,
    pub engine_build: String,
    /// Intersection of requested (optional and required) and supported features.
    pub capabilities: Vec<String>,
    pub methods: Vec<String>,
    pub limits: ConnectionLimits,
    pub execution_host: ExecutionHost,
    pub sandbox_capabilities: Vec<String>,
}

/// Server-owned facts. Do not populate from unauthenticated wire parameters.
#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub engine_build: String,
    pub capabilities: BTreeSet<String>,
    pub methods: Vec<String>,
    pub limits: ConnectionLimits,
    pub execution_host: ExecutionHost,
    pub sandbox_capabilities: Vec<String>,
}

/// A newer 1.x peer can fall back to 1.0 only if every required capability is
/// available. Different major versions fail; version never implies capability.
pub fn negotiate(
    params: &InitializeParams,
    server: &ServerInfo,
) -> Result<InitializeResult, RpcError> {
    let (major, _) =
        parse_version(&params.protocol_version).ok_or_else(RpcError::invalid_params)?;
    if major != 1 {
        return Err(RpcError::application(
            "unsupported_version",
            json!({"requested":params.protocol_version,"supported":[PROTOCOL_VERSION]}),
        ));
    }
    fn bounded_name(value: &str, maximum: usize) -> bool {
        !value.trim().is_empty() && !value.contains('\0') && value.chars().count() <= maximum
    }
    if !bounded_name(&params.client.name, MAX_CLIENT_NAME)
        || !bounded_name(&params.client.version, MAX_CLIENT_VERSION)
        || params.capabilities.len() > MAX_CAPABILITIES
        || params.required_capabilities.len() > MAX_CAPABILITIES
        || params
            .capabilities
            .iter()
            .chain(&params.required_capabilities)
            .any(|c| !bounded_name(c, MAX_CAPABILITY_NAME))
    {
        return Err(RpcError::invalid_params());
    }
    let missing: BTreeSet<_> = params
        .required_capabilities
        .iter()
        .filter(|c| !server.capabilities.contains(*c))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(RpcError::application(
            "unsupported_capability",
            json!({"missing":missing}),
        ));
    }
    if server.limits.maximum_frame_bytes == 0
        || server.limits.maximum_frame_bytes > MAX_FRAME_BYTES
        || server.limits.maximum_pending_requests == 0
        || server.limits.maximum_pending_requests > 1024
        || server.limits.maximum_subscriptions == 0
        || server.limits.maximum_subscriptions > 128
        || server.limits.maximum_subscriber_queue_bytes == 0
        || server.limits.maximum_subscriber_queue_bytes > MAX_FRAME_BYTES
    {
        return Err(RpcError::internal_error());
    }
    let capabilities: BTreeSet<_> = params
        .capabilities
        .iter()
        .chain(&params.required_capabilities)
        .filter(|c| server.capabilities.contains(*c))
        .cloned()
        .collect();
    Ok(InitializeResult {
        protocol_version: PROTOCOL_VERSION.into(),
        event_schema_version: EVENT_SCHEMA_VERSION.into(),
        schema_version: SCHEMA_VERSION.into(),
        engine_build: server.engine_build.clone(),
        capabilities: capabilities.into_iter().collect(),
        methods: server.methods.clone(),
        limits: server.limits.clone(),
        execution_host: server.execution_host.clone(),
        sandbox_capabilities: server.sandbox_capabilities.clone(),
    })
}

fn parse_version(value: &str) -> Option<(u32, u32)> {
    let (major, minor) = value.split_once('.')?;
    fn number(value: &str) -> Option<u32> {
        if value.is_empty()
            || (value.len() > 1 && value.starts_with('0'))
            || !value.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        value.parse().ok()
    }
    Some((number(major)?, number(minor)?))
}

/// Per-connection initialization stage. Authentication remains a separate gate
/// in the transport adapter; this state is not proof of authentication.
#[derive(Debug, Default)]
pub struct Handshake {
    initialized: bool,
}

impl Handshake {
    pub fn initialize(
        &mut self,
        params: &InitializeParams,
        server: &ServerInfo,
    ) -> Result<InitializeResult, RpcError> {
        if self.initialized {
            return Err(RpcError::application("already_initialized", json!({})));
        }
        let result = negotiate(params, server)?;
        self.initialized = true;
        Ok(result)
    }
    pub fn require_initialized(&self) -> Result<(), RpcError> {
        if self.initialized {
            Ok(())
        } else {
            Err(RpcError::application("not_initialized", json!({})))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initialization_names_capabilities_and_advertised_limits_are_bounded() {
        let (params, mut server) = fixture();
        for limit in [0, MAX_FRAME_BYTES + 1] {
            server.limits.maximum_frame_bytes = limit;
            assert_eq!(negotiate(&params, &server).unwrap_err().code, -32603);
        }
        server.limits.maximum_frame_bytes = MAX_FRAME_BYTES;
        assert!(negotiate(&params, &server).is_ok());
        server.limits.maximum_pending_requests = 1025;
        assert_eq!(negotiate(&params, &server).unwrap_err().code, -32603);
        let (mut params, server) = fixture();
        params.client.name = "é".repeat(MAX_CLIENT_NAME);
        assert!(negotiate(&params, &server).is_ok());
        params.client.name.push('x');
        assert_eq!(negotiate(&params, &server).unwrap_err().code, -32602);
        let (mut params, server) = fixture();
        params.capabilities = vec!["a".into(); MAX_CAPABILITIES + 1];
        assert_eq!(negotiate(&params, &server).unwrap_err().code, -32602);
        params.capabilities = vec!["a".repeat(MAX_CAPABILITY_NAME + 1)];
        assert_eq!(negotiate(&params, &server).unwrap_err().code, -32602);
        params.capabilities = vec!["unsafe\0name".into()];
        assert_eq!(negotiate(&params, &server).unwrap_err().code, -32602);
    }
    fn fixture() -> (InitializeParams, ServerInfo) {
        (
            InitializeParams {
                protocol_version: "1.0".into(),
                client: ClientInfo {
                    name: "test".into(),
                    version: "1".into(),
                },
                capabilities: vec!["events.resume".into(), "future.optional".into()],
                required_capabilities: vec![],
            },
            ServerInfo {
                engine_build: "test".into(),
                capabilities: BTreeSet::from(["events.resume".into()]),
                methods: vec!["events/subscribe".into()],
                limits: ConnectionLimits {
                    maximum_frame_bytes: 1024,
                    maximum_pending_requests: 8,
                    maximum_subscriptions: 2,
                    maximum_subscriber_queue_bytes: 4096,
                },
                execution_host: ExecutionHost {
                    id: "local".into(),
                    platform: "windows".into(),
                },
                sandbox_capabilities: vec![],
            },
        )
    }
    #[test]
    fn old_and_new_minor_peers_use_capabilities_not_version() {
        let (mut params, server) = fixture();
        for version in ["1.0", "1.9"] {
            params.protocol_version = version.into();
            let result = negotiate(&params, &server).unwrap();
            assert_eq!(result.protocol_version, "1.0");
            assert_eq!(result.capabilities, vec!["events.resume"]);
            assert_eq!(
                serde_json::from_value::<InitializeResult>(serde_json::to_value(&result).unwrap())
                    .unwrap(),
                result
            );
        }
        params.required_capabilities.push("future.required".into());
        assert_eq!(
            negotiate(&params, &server).unwrap_err().data.unwrap().kind,
            "unsupported_capability"
        );
        params.protocol_version = "2.0".into();
        assert_eq!(
            negotiate(&params, &server).unwrap_err().data.unwrap().kind,
            "unsupported_version"
        );
    }
    #[test]
    fn malformed_version_and_unknown_authority_fields_reject() {
        let (mut params, server) = fixture();
        for version in ["1", "1.0.0", "01.0", "+1.0", "1.-1", "1.4294967296"] {
            params.protocol_version = version.into();
            assert_eq!(negotiate(&params, &server).unwrap_err().code, -32602);
        }
        let mut value = serde_json::to_value(&params).unwrap();
        value["grant_controller"] = json!(true);
        assert!(serde_json::from_value::<InitializeParams>(value).is_err());
    }
    #[test]
    fn failed_init_never_unlocks_gate_and_success_cannot_be_repeated() {
        let (mut params, server) = fixture();
        let mut handshake = Handshake::default();
        assert!(handshake.require_initialized().is_err());
        params.required_capabilities.push("absent".into());
        assert!(handshake.initialize(&params, &server).is_err());
        assert!(handshake.require_initialized().is_err());
        params.required_capabilities.clear();
        handshake.initialize(&params, &server).unwrap();
        handshake.require_initialized().unwrap();
        assert_eq!(
            handshake
                .initialize(&params, &server)
                .unwrap_err()
                .data
                .unwrap()
                .kind,
            "already_initialized"
        );
    }
}
