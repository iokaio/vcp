// SPDX-License-Identifier: Apache-2.0
//! Sequential data-only client. Host commits intent and current source/authority
//! fences before each write; no method here performs IO or implicitly retries.
use super::{
    content::{
        self, Catalog, Descriptor, DiscoveredPrompt, DiscoveredResource, PromptResult,
        RejectedContent, ResourceResult,
    },
    identity::{ConnectionIdentity, ContentIdentity, ContentKind, ToolIdentity, PROTOCOL},
    registration::{self, Registration, Transport},
    schema::{self, CheckedArguments, Schema},
};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingKind {
    Initialize,
    ListTools,
    CallTool,
    ListResources,
    ReadResource,
    ListPrompts,
    GetPrompt,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboundKind {
    Request(PendingKind),
    Initialized,
    Control,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("MCP registration is not supported by this client profile")]
    Registration,
    #[error("MCP client state does not permit this operation")]
    State,
    #[error("MCP frame or discovery exceeds configured bounds")]
    Bounds,
    #[error("MCP frame violates the admitted protocol profile")]
    Protocol,
    #[error("MCP protocol revision is not supported")]
    Version,
    #[error("MCP tool identity is stale or unavailable")]
    Stale,
    #[error("MCP arguments do not match the selected schema")]
    Arguments,
}
pub struct Outbound {
    bytes: Vec<u8>,
    digest: String,
    request_id: Option<String>,
    kind: OutboundKind,
    owner: String,
}
impl Outbound {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }
    pub fn kind(&self) -> OutboundKind {
        self.kind
    }
}
impl fmt::Debug for Outbound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Outbound")
            .field("kind", &self.kind)
            .field("bytes", &self.bytes.len())
            .field("digest", &self.digest)
            .finish()
    }
}
#[derive(Clone, Serialize)]
pub struct DiscoveredTool {
    pub identity: ToolIdentity,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub input_schema: Value,
    pub output_schema: Option<Value>,
    /// Hints only; they never determine policy effects.
    pub annotations: BTreeMap<String, Value>,
    #[serde(skip)]
    schema: Schema,
    #[serde(skip)]
    output: Option<Schema>,
}
impl DiscoveredTool {
    pub fn check_arguments(&self, bytes: &[u8]) -> Result<CheckedArguments, schema::Error> {
        self.schema.arguments(bytes)
    }
    pub fn canonical_input_schema(&self) -> &[u8] {
        self.schema.canonical_bytes()
    }
}
impl fmt::Debug for DiscoveredTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DiscoveredTool")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct RejectedTool {
    pub name: String,
    pub reason: Rejection,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rejection {
    NotAllowed,
    UnsupportedSchema,
    Metadata,
}
#[derive(Serialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}
impl fmt::Debug for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RpcError")
            .field("code", &self.code)
            .finish_non_exhaustive()
    }
}
#[derive(Serialize)]
pub struct ToolResult {
    pub is_error: bool,
    pub text: Vec<String>,
    pub structured_content: Option<Value>,
    /// Payload is retained by the host's raw response artifact; it is never fetched.
    pub omitted_content: Vec<String>,
}
impl fmt::Debug for ToolResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToolResult")
            .field("is_error", &self.is_error)
            .field("text_blocks", &self.text.len())
            .field("omitted_blocks", &self.omitted_content.len())
            .finish_non_exhaustive()
    }
}
#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "result", rename_all = "snake_case")]
pub enum CallReply {
    ToolResult(ToolResult),
    RpcError(RpcError),
}
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Notification {
    ToolListChanged,
    ResourceListChanged,
    ResourceUpdated { uri: String },
    PromptListChanged,
    Progress,
    Logging,
}
impl fmt::Debug for Notification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ToolListChanged => "ToolListChanged",
            Self::ResourceListChanged => "ResourceListChanged",
            Self::ResourceUpdated { .. } => "ResourceUpdated([omitted])",
            Self::PromptListChanged => "PromptListChanged",
            Self::Progress => "Progress",
            Self::Logging => "Logging",
        })
    }
}
#[derive(Debug)]
pub enum Incoming {
    Initialized {
        connection: ConnectionIdentity,
        notification: Outbound,
    },
    DiscoveryPage {
        next: bool,
    },
    DiscoveryComplete {
        tools: Vec<DiscoveredTool>,
        rejected: Vec<RejectedTool>,
    },
    CallReply(CallReply),
    ResourceDiscoveryPage {
        next: bool,
    },
    ResourceDiscoveryComplete {
        resources: Vec<DiscoveredResource>,
        rejected: Vec<RejectedContent>,
    },
    PromptDiscoveryPage {
        next: bool,
    },
    PromptDiscoveryComplete {
        prompts: Vec<DiscoveredPrompt>,
        rejected: Vec<RejectedContent>,
    },
    ResourceReply(ResourceResult),
    PromptReply(PromptResult),
    RpcError {
        request: PendingKind,
        error: RpcError,
    },
    ControlReply(Outbound),
    Notification(Notification),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    New,
    Initializing,
    AwaitingNotice,
    Ready,
    Closed,
}
struct Pending {
    id: String,
    kind: PendingKind,
    digest: String,
    sent: bool,
    tool: Option<DiscoveredTool>,
    content: Option<ContentIdentity>,
}
pub struct Client {
    registration: Registration,
    phase: Phase,
    connection: Option<ConnectionIdentity>,
    pending: Option<Pending>,
    controls: BTreeSet<String>,
    sequence: u64,
    received: u64,
    catalog_revision: u64,
    tools: BTreeMap<String, DiscoveredTool>,
    building: BTreeMap<String, DiscoveredTool>,
    rejected: Vec<RejectedTool>,
    names: BTreeSet<String>,
    cursors: BTreeSet<String>,
    cursor: Option<String>,
    listing: bool,
    pages: u64,
    discovery_bytes: u64,
    owner: String,
    resources: Catalog,
    prompts: Catalog,
    server_tools: bool,
    server_resources: bool,
    server_prompts: bool,
}
impl Client {
    pub fn new(registration: Registration) -> Result<Self, Error> {
        if !matches!(registration.transport, Transport::Stdio { .. }) {
            return Err(Error::Registration);
        }
        Self::new_inner(registration)
    }
    /// HTTP admission is exposed only through the session adapter, which also
    /// enforces response acknowledgements and private session-header state.
    pub(super) fn new_http(registration: Registration) -> Result<Self, Error> {
        if !matches!(registration.transport, Transport::StreamableHttp { .. }) {
            return Err(Error::Registration);
        }
        Self::new_inner(registration)
    }
    fn new_inner(registration: Registration) -> Result<Self, Error> {
        registration.validate().map_err(|_| Error::Registration)?;
        Ok(Self {
            registration,
            phase: Phase::New,
            connection: None,
            pending: None,
            controls: BTreeSet::new(),
            sequence: 0,
            received: 0,
            catalog_revision: 0,
            tools: BTreeMap::new(),
            building: BTreeMap::new(),
            rejected: vec![],
            names: BTreeSet::new(),
            cursors: BTreeSet::new(),
            cursor: None,
            listing: false,
            pages: 0,
            discovery_bytes: 0,
            owner: vcp_domain::ExecutionId::new().to_string(),
            resources: Catalog::new(ContentKind::Resource),
            prompts: Catalog::new(ContentKind::Prompt),
            server_tools: false,
            server_resources: false,
            server_prompts: false,
        })
    }
    pub fn connection(&self) -> Option<&ConnectionIdentity> {
        self.connection.as_ref()
    }
    pub(super) fn owns_outbound(&self, out: &Outbound) -> bool {
        self.owner == out.owner
    }
    pub fn pending_kind(&self) -> Option<PendingKind> {
        self.pending.as_ref().map(|p| p.kind)
    }
    pub fn tools(&self) -> impl Iterator<Item = &DiscoveredTool> {
        self.tools.values()
    }
    pub fn tool(&self, identity: &ToolIdentity) -> Result<&DiscoveredTool, Error> {
        self.tools
            .get(identity.remote_name())
            .filter(|t| &t.identity == identity)
            .ok_or(Error::Stale)
    }
    pub fn resources(&self) -> impl Iterator<Item = &DiscoveredResource> {
        self.resources
            .entries
            .values()
            .filter_map(|entry| match entry {
                Descriptor::Resource(resource) => Some(resource),
                _ => None,
            })
    }
    pub fn prompts(&self) -> impl Iterator<Item = &DiscoveredPrompt> {
        self.prompts
            .entries
            .values()
            .filter_map(|entry| match entry {
                Descriptor::Prompt(prompt) => Some(prompt),
                _ => None,
            })
    }
    pub fn resource(&self, identity: &ContentIdentity) -> Result<&DiscoveredResource, Error> {
        self.resources()
            .find(|resource| &resource.identity == identity)
            .ok_or(Error::Stale)
    }
    pub fn prompt(&self, identity: &ContentIdentity) -> Result<&DiscoveredPrompt, Error> {
        self.prompts()
            .find(|prompt| &prompt.identity == identity)
            .ok_or(Error::Stale)
    }
    pub fn list_resources(&mut self) -> Result<Outbound, Error> {
        self.ready()?;
        if !self.registration.resources_enabled()
            || !self.server_resources
            || self.listing
            || self.prompts.listing
        {
            return Err(Error::State);
        }
        let params = self.resources.request(self.registration.limits.pages)?;
        self.request(PendingKind::ListResources, "resources/list", params, None)
    }
    pub fn read_resource(&mut self, identity: &ContentIdentity) -> Result<Outbound, Error> {
        self.ready()?;
        if self.listing || self.resources.listing || self.prompts.listing {
            return Err(Error::State);
        }
        let uri = self.resource(identity)?.uri.clone();
        let out = self.request(
            PendingKind::ReadResource,
            "resources/read",
            json!({"uri":uri}),
            None,
        )?;
        self.pending.as_mut().ok_or(Error::State)?.content = Some(identity.clone());
        Ok(out)
    }
    pub fn list_prompts(&mut self) -> Result<Outbound, Error> {
        self.ready()?;
        if !self.registration.prompts_enabled()
            || !self.server_prompts
            || self.listing
            || self.resources.listing
        {
            return Err(Error::State);
        }
        let params = self.prompts.request(self.registration.limits.pages)?;
        self.request(PendingKind::ListPrompts, "prompts/list", params, None)
    }
    pub fn get_prompt(
        &mut self,
        identity: &ContentIdentity,
        arguments: &CheckedArguments,
    ) -> Result<Outbound, Error> {
        self.ready()?;
        if self.listing || self.resources.listing || self.prompts.listing {
            return Err(Error::State);
        }
        let prompt = self.prompt(identity)?;
        if arguments.schema_digest() != prompt.schema_digest() {
            return Err(Error::Arguments);
        }
        let params = json!({"name":prompt.name,"arguments":serde_json::from_slice::<Value>(arguments.canonical_bytes()).map_err(|_|Error::Arguments)?});
        let out = self.request(PendingKind::GetPrompt, "prompts/get", params, None)?;
        self.pending.as_mut().ok_or(Error::State)?.content = Some(identity.clone());
        Ok(out)
    }
    pub fn initialize(&mut self) -> Result<Outbound, Error> {
        if self.phase != Phase::New {
            return Err(Error::State);
        }
        let out = self.request(PendingKind::Initialize, "initialize", json!({"protocolVersion":PROTOCOL,"capabilities":{},"clientInfo":{"name":"vcp","version":"0.1.0"}}), None)?;
        self.phase = Phase::Initializing;
        Ok(out)
    }
    pub fn list_tools(&mut self) -> Result<Outbound, Error> {
        self.ready()?;
        if !self.server_tools || self.resources.listing || self.prompts.listing {
            return Err(Error::State);
        }
        if !self.listing {
            self.catalog_revision = self.catalog_revision.checked_add(1).ok_or(Error::Bounds)?;
            self.tools.clear();
            self.building.clear();
            self.rejected.clear();
            self.names.clear();
            self.cursors.clear();
            self.cursor = None;
            self.pages = 0;
            self.discovery_bytes = 0;
            self.listing = true;
        }
        if self.pages >= self.registration.limits.pages {
            return Err(Error::Bounds);
        }
        let params = self
            .cursor
            .as_ref()
            .map_or_else(|| json!({}), |cursor| json!({"cursor":cursor}));
        self.request(PendingKind::ListTools, "tools/list", params, None)
    }
    pub fn call(
        &mut self,
        identity: &ToolIdentity,
        arguments: &CheckedArguments,
    ) -> Result<Outbound, Error> {
        self.ready()?;
        if self.listing || self.resources.listing || self.prompts.listing {
            return Err(Error::State);
        }
        let tool = self.tool(identity)?.clone();
        if arguments.schema_digest() != tool.identity.schema_digest() {
            return Err(Error::Arguments);
        }
        let arguments: Value =
            serde_json::from_slice(arguments.canonical_bytes()).map_err(|_| Error::Arguments)?;
        self.request(
            PendingKind::CallTool,
            "tools/call",
            json!({"name":tool.name,"arguments":arguments}),
            Some(tool),
        )
    }
    /// Host checks this before any physical write, then applies current canonical
    /// authority/source fences. This token check is not a dispatch permit.
    pub fn validate_send(&self, out: &Outbound) -> Result<(), Error> {
        if self.phase == Phase::Closed || self.owner != out.owner {
            return Err(Error::State);
        }
        match out.kind {
            OutboundKind::Request(kind) => {
                let pending = self.pending.as_ref().ok_or(Error::State)?;
                if pending.sent
                    || pending.kind != kind
                    || pending.digest != out.digest
                    || out.request_id.as_ref() != Some(&pending.id)
                {
                    return Err(Error::State);
                }
                if let Some(identity) = &pending.content {
                    match identity.kind() {
                        ContentKind::Resource => {
                            self.resource(identity)?;
                        }
                        ContentKind::Prompt => {
                            self.prompt(identity)?;
                        }
                    }
                }
            }
            OutboundKind::Initialized | OutboundKind::Control => {
                if !self.controls.contains(&out.digest)
                    || (out.kind == OutboundKind::Initialized
                        && self.phase != Phase::AwaitingNotice)
                {
                    return Err(Error::State);
                }
            }
        }
        Ok(())
    }
    /// Call only after the exact frame has been fully written. On write failure
    /// or cancellation abandon the connection instead; never repeat bytes.
    pub fn confirm_sent(&mut self, out: &Outbound) -> Result<(), Error> {
        self.validate_send(out)?;
        match out.kind {
            OutboundKind::Request(_) => {
                let pending = self.pending.as_mut().ok_or(Error::State)?;
                if pending.sent || pending.digest != out.digest {
                    return Err(Error::State);
                }
                pending.sent = true;
            }
            OutboundKind::Initialized | OutboundKind::Control => {
                if !self.controls.remove(&out.digest) {
                    return Err(Error::State);
                }
                if out.kind == OutboundKind::Initialized {
                    if self.phase != Phase::AwaitingNotice {
                        return Err(Error::State);
                    }
                    self.phase = Phase::Ready;
                }
            }
        }
        Ok(())
    }
    pub fn abandon(&mut self) -> Option<PendingKind> {
        let pending = self.pending.take().map(|p| p.kind);
        self.phase = Phase::Closed;
        self.controls.clear();
        self.tools.clear();
        self.building.clear();
        self.resources.clear();
        self.prompts.clear();
        pending
    }
    /// Any malformed/unmatched frame poisons this connection. The host retains
    /// prior pending kind and raw response artifact to record an unknown outcome.
    pub fn receive(&mut self, bytes: &[u8]) -> Result<Incoming, Error> {
        let result = self.receive_inner(bytes);
        if result.is_err() {
            self.abandon();
        }
        result
    }
    fn ready(&self) -> Result<(), Error> {
        if self.phase != Phase::Ready || self.pending.is_some() || !self.controls.is_empty() {
            Err(Error::State)
        } else {
            Ok(())
        }
    }
    fn request(
        &mut self,
        kind: PendingKind,
        method: &str,
        params: Value,
        tool: Option<DiscoveredTool>,
    ) -> Result<Outbound, Error> {
        if self.pending.is_some() || !self.controls.is_empty() || self.sequence >= 256 {
            return Err(Error::State);
        }
        self.sequence += 1;
        let id = format!("vcp-{}", self.sequence);
        let out = self.encode(
            json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
            Some(id.clone()),
            OutboundKind::Request(kind),
        )?;
        self.pending = Some(Pending {
            id,
            kind,
            digest: out.digest.clone(),
            sent: false,
            tool,
            content: None,
        });
        Ok(out)
    }
    fn encode(
        &self,
        value: Value,
        request_id: Option<String>,
        kind: OutboundKind,
    ) -> Result<Outbound, Error> {
        let bytes = vcp_protocol::canonical_bytes(&value).map_err(|_| Error::Protocol)?;
        if bytes.len() > self.registration.limits.frame_bytes as usize {
            return Err(Error::Bounds);
        }
        Ok(Outbound {
            digest: vcp_protocol::digest_bytes(&bytes),
            bytes,
            request_id,
            kind,
            owner: self.owner.clone(),
        })
    }
    fn control(&mut self, value: Value, kind: OutboundKind) -> Result<Outbound, Error> {
        let out = self.encode(value, None, kind)?;
        if !self.controls.insert(out.digest.clone()) {
            return Err(Error::State);
        }
        Ok(out)
    }
    fn receive_inner(&mut self, bytes: &[u8]) -> Result<Incoming, Error> {
        if matches!(self.phase, Phase::New | Phase::Closed) || !self.controls.is_empty() {
            return Err(Error::State);
        }
        self.received += 1;
        if self.received > 1024 {
            return Err(Error::Bounds);
        }
        let value = schema::bounded_json(
            bytes,
            schema::Limits {
                bytes: self.registration.limits.frame_bytes as usize,
                depth: 32,
                nodes: 65536,
            },
        )
        .map_err(|e| {
            if e == schema::Error::Bounds {
                Error::Bounds
            } else {
                Error::Protocol
            }
        })?;
        let object = value.as_object().ok_or(Error::Protocol)?;
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Err(Error::Protocol);
        }
        if let Some(method) = object.get("method") {
            if object.contains_key("result") || object.contains_key("error") {
                return Err(Error::Protocol);
            }
            return self.inbound_method(object, method.as_str().ok_or(Error::Protocol)?);
        }
        if object
            .keys()
            .any(|key| !["jsonrpc", "id", "result", "error"].contains(&key.as_str()))
        {
            return Err(Error::Protocol);
        }
        let pending = self.pending.as_ref().ok_or(Error::Protocol)?;
        if !pending.sent
            || object.get("id").and_then(Value::as_str) != Some(&pending.id)
            || object.contains_key("result") == object.contains_key("error")
        {
            return Err(Error::Protocol);
        }
        let pending = self.pending.take().ok_or(Error::State)?;
        if let Some(error) = object.get("error") {
            let error = rpc_error(error)?;
            if pending.kind == PendingKind::CallTool {
                return Ok(Incoming::CallReply(CallReply::RpcError(error)));
            }
            if matches!(
                pending.kind,
                PendingKind::ListResources | PendingKind::ListPrompts
            ) {
                match pending.kind {
                    PendingKind::ListResources => self.resources.clear(),
                    _ => self.prompts.clear(),
                };
            } else if !matches!(
                pending.kind,
                PendingKind::ReadResource | PendingKind::GetPrompt
            ) {
                self.phase = Phase::Closed;
            }
            return Ok(Incoming::RpcError {
                request: pending.kind,
                error,
            });
        }
        let result = object
            .get("result")
            .and_then(Value::as_object)
            .ok_or(Error::Protocol)?;
        match pending.kind {
            PendingKind::Initialize => self.initialized(result),
            PendingKind::ListTools => self.page(result, bytes.len()),
            PendingKind::CallTool => Ok(Incoming::CallReply(CallReply::ToolResult(tool_result(
                result,
                pending.tool.as_ref().ok_or(Error::State)?,
            )?))),
            PendingKind::ListResources => {
                let page = self.resources.page(
                    result,
                    self.connection.as_ref().ok_or(Error::State)?,
                    &self.registration,
                    bytes.len(),
                )?;
                if page.complete {
                    Ok(Incoming::ResourceDiscoveryComplete {
                        resources: self.resources().cloned().collect(),
                        rejected: page.rejected,
                    })
                } else {
                    Ok(Incoming::ResourceDiscoveryPage { next: true })
                }
            }
            PendingKind::ListPrompts => {
                let page = self.prompts.page(
                    result,
                    self.connection.as_ref().ok_or(Error::State)?,
                    &self.registration,
                    bytes.len(),
                )?;
                if page.complete {
                    Ok(Incoming::PromptDiscoveryComplete {
                        prompts: self.prompts().cloned().collect(),
                        rejected: page.rejected,
                    })
                } else {
                    Ok(Incoming::PromptDiscoveryPage { next: true })
                }
            }
            PendingKind::ReadResource => {
                Ok(Incoming::ResourceReply(content::resource_result(result)?))
            }
            PendingKind::GetPrompt => Ok(Incoming::PromptReply(content::prompt_result(result)?)),
        }
    }
    fn inbound_method(
        &mut self,
        object: &Map<String, Value>,
        method: &str,
    ) -> Result<Incoming, Error> {
        if object
            .keys()
            .any(|key| !["jsonrpc", "id", "method", "params"].contains(&key.as_str()))
        {
            return Err(Error::Protocol);
        }
        if let Some(id) = object.get("id") {
            if !(id.is_i64()
                || id.is_u64()
                || id.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 256))
            {
                return Err(Error::Protocol);
            }
            let response = if method == "ping" {
                if object
                    .get("params")
                    .is_some_and(|p| !p.as_object().is_some_and(Map::is_empty))
                {
                    json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":"Invalid params"}})
                } else {
                    json!({"jsonrpc":"2.0","id":id,"result":{}})
                }
            } else {
                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}})
            };
            return Ok(Incoming::ControlReply(
                self.control(response, OutboundKind::Control)?,
            ));
        }
        match method {
            "notifications/resources/list_changed"
                if self.phase == Phase::Ready
                    && self.registration.resources_enabled()
                    && self.server_resources =>
            {
                self.resources.clear();
                if self
                    .pending
                    .as_ref()
                    .is_some_and(|p| p.kind == PendingKind::ListResources)
                {
                    return Err(Error::Stale);
                }
                Ok(Incoming::Notification(Notification::ResourceListChanged))
            }
            "notifications/resources/updated"
                if self.phase == Phase::Ready
                    && self.registration.resources_enabled()
                    && self.server_resources =>
            {
                let uri = content::string(
                    object
                        .get("params")
                        .and_then(Value::as_object)
                        .and_then(|params| params.get("uri")),
                    4096,
                )?
                .to_owned();
                if !content::valid_uri(&uri) {
                    return Err(Error::Protocol);
                }
                self.resources.entries.remove(&uri);
                if self.resources.listing {
                    self.resources.clear();
                    return Err(Error::Stale);
                }
                Ok(Incoming::Notification(Notification::ResourceUpdated {
                    uri,
                }))
            }
            "notifications/prompts/list_changed"
                if self.phase == Phase::Ready
                    && self.registration.prompts_enabled()
                    && self.server_prompts =>
            {
                self.prompts.clear();
                if self
                    .pending
                    .as_ref()
                    .is_some_and(|p| p.kind == PendingKind::ListPrompts)
                {
                    return Err(Error::Stale);
                }
                Ok(Incoming::Notification(Notification::PromptListChanged))
            }
            "notifications/tools/list_changed" if self.phase == Phase::Ready => {
                self.tools.clear();
                self.building.clear();
                self.cursor = None;
                self.listing = false;
                if self
                    .pending
                    .as_ref()
                    .is_some_and(|p| p.kind == PendingKind::ListTools)
                {
                    return Err(Error::Stale);
                }
                Ok(Incoming::Notification(Notification::ToolListChanged))
            }
            "notifications/progress" => Ok(Incoming::Notification(Notification::Progress)),
            "notifications/message" => Ok(Incoming::Notification(Notification::Logging)),
            _ => Err(Error::Protocol),
        }
    }
    fn initialized(&mut self, result: &Map<String, Value>) -> Result<Incoming, Error> {
        if self.phase != Phase::Initializing {
            return Err(Error::State);
        }
        if result.get("protocolVersion").and_then(Value::as_str) != Some(PROTOCOL) {
            return Err(Error::Version);
        }
        let capabilities = result
            .get("capabilities")
            .and_then(Value::as_object)
            .ok_or(Error::Protocol)?;
        for name in ["tools", "resources", "prompts"] {
            if capabilities.get(name).is_some_and(|v| !v.is_object()) {
                return Err(Error::Protocol);
            }
            if let Some(value) = capabilities.get(name).and_then(Value::as_object) {
                if value.get("listChanged").is_some_and(|v| !v.is_boolean())
                    || (name == "resources"
                        && value.get("subscribe").is_some_and(|v| !v.is_boolean()))
                {
                    return Err(Error::Protocol);
                }
            }
        }
        self.server_tools = capabilities.contains_key("tools");
        self.server_resources = capabilities.contains_key("resources");
        self.server_prompts = capabilities.contains_key("prompts");
        let requires_tools = !self.registration.allowed_tools.is_empty()
            || (!self.registration.resources_enabled() && !self.registration.prompts_enabled());
        if (requires_tools && !self.server_tools)
            || (self.registration.resources_enabled() && !self.server_resources)
            || (self.registration.prompts_enabled() && !self.server_prompts)
        {
            return Err(Error::Protocol);
        }
        let info = result
            .get("serverInfo")
            .and_then(Value::as_object)
            .ok_or(Error::Protocol)?;
        bounded_string(info.get("name"), 256)?;
        bounded_string(info.get("version"), 128)?;
        let connection = ConnectionIdentity::new(&self.registration, PROTOCOL, None)
            .map_err(|_| Error::Registration)?;
        self.connection = Some(connection.clone());
        self.phase = Phase::AwaitingNotice;
        let notification = self.control(
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            OutboundKind::Initialized,
        )?;
        Ok(Incoming::Initialized {
            connection,
            notification,
        })
    }
    fn page(&mut self, result: &Map<String, Value>, bytes: usize) -> Result<Incoming, Error> {
        self.pages += 1;
        self.discovery_bytes = self
            .discovery_bytes
            .checked_add(bytes as u64)
            .ok_or(Error::Bounds)?;
        if !self.listing
            || self.pages > self.registration.limits.pages
            || self.discovery_bytes > self.registration.limits.total_discovery_bytes
        {
            return Err(Error::Bounds);
        }
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .ok_or(Error::Protocol)?;
        for value in tools {
            let tool = value.as_object().ok_or(Error::Protocol)?;
            let name = bounded_string(tool.get("name"), 256)?.to_owned();
            if !registration::remote_name(&name) || !self.names.insert(name.clone()) {
                return Err(Error::Protocol);
            }
            if self.names.len() as u64 > self.registration.limits.tools {
                return Err(Error::Bounds);
            }
            if !self.registration.allowed_tools.contains(&name) {
                self.rejected.push(RejectedTool {
                    name,
                    reason: Rejection::NotAllowed,
                });
                continue;
            }
            match self.admit_tool(tool, &name) {
                Ok(tool) => {
                    self.building.insert(name, tool);
                }
                Err(reason) => self.rejected.push(RejectedTool { name, reason }),
            }
        }
        self.cursor = result
            .get("nextCursor")
            .map(|v| bounded_string(Some(v), 1024).map(str::to_owned))
            .transpose()?;
        if let Some(cursor) = &self.cursor {
            if !self.cursors.insert(cursor.clone()) || self.pages >= self.registration.limits.pages
            {
                return Err(Error::Bounds);
            }
            return Ok(Incoming::DiscoveryPage { next: true });
        }
        self.listing = false;
        self.tools = std::mem::take(&mut self.building);
        Ok(Incoming::DiscoveryComplete {
            tools: self.tools.values().cloned().collect(),
            rejected: std::mem::take(&mut self.rejected),
        })
    }
    fn admit_tool(
        &self,
        tool: &Map<String, Value>,
        name: &str,
    ) -> Result<DiscoveredTool, Rejection> {
        let metadata = || Rejection::Metadata;
        let title = tool
            .get("title")
            .map(|v| bounded_string(Some(v), 256).map(str::to_owned))
            .transpose()
            .map_err(|_| metadata())?;
        let description = tool
            .get("description")
            .map(|v| bounded_string(Some(v), 4096).map(str::to_owned))
            .transpose()
            .map_err(|_| metadata())?;
        let input_schema = tool
            .get("inputSchema")
            .cloned()
            .ok_or(Rejection::UnsupportedSchema)?;
        let compile = |value: &Value| -> Result<Schema, Rejection> {
            let bytes =
                vcp_protocol::canonical_bytes(value).map_err(|_| Rejection::UnsupportedSchema)?;
            Schema::compile(&bytes, schema::Limits::default())
                .map_err(|_| Rejection::UnsupportedSchema)
        };
        let schema = compile(&input_schema)?;
        let output_schema = tool.get("outputSchema").cloned();
        let output = output_schema.as_ref().map(compile).transpose()?;
        let mut annotations = BTreeMap::new();
        if let Some(value) = tool.get("annotations") {
            for (key, value) in value.as_object().ok_or_else(metadata)? {
                match key.as_str() {
                    "title" if value.as_str().is_some_and(|s| s.len() <= 256) => (),
                    "readOnlyHint" | "destructiveHint" | "idempotentHint" | "openWorldHint"
                        if value.is_boolean() => {}
                    _ => return Err(metadata()),
                }
                annotations.insert(key.clone(), value.clone());
            }
        }
        let identity = ToolIdentity::for_catalog(
            self.connection.clone().ok_or(Rejection::Metadata)?,
            name.to_owned(),
            &schema,
            self.catalog_revision,
        )
        .map_err(|_| Rejection::Metadata)?;
        Ok(DiscoveredTool {
            identity,
            name: name.to_owned(),
            title,
            description,
            input_schema,
            output_schema,
            annotations,
            schema,
            output,
        })
    }
}
fn bounded_string(value: Option<&Value>, max: usize) -> Result<&str, Error> {
    value
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= max)
        .ok_or(Error::Protocol)
}
fn rpc_error(value: &Value) -> Result<RpcError, Error> {
    let object = value.as_object().ok_or(Error::Protocol)?;
    Ok(RpcError {
        code: object
            .get("code")
            .and_then(Value::as_i64)
            .ok_or(Error::Protocol)?,
        message: bounded_string(object.get("message"), 4096)?.to_owned(),
    })
}
fn tool_result(result: &Map<String, Value>, tool: &DiscoveredTool) -> Result<ToolResult, Error> {
    let is_error = result
        .get("isError")
        .map(|v| v.as_bool().ok_or(Error::Protocol))
        .transpose()?
        .unwrap_or(false);
    let content = result
        .get("content")
        .and_then(Value::as_array)
        .ok_or(Error::Protocol)?;
    if content.len() > 64 {
        return Err(Error::Bounds);
    }
    let mut text = Vec::new();
    let mut omitted_content = Vec::new();
    for block in content {
        let block = block.as_object().ok_or(Error::Protocol)?;
        match block
            .get("type")
            .and_then(Value::as_str)
            .ok_or(Error::Protocol)?
        {
            "text" => text.push(
                block
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or(Error::Protocol)?
                    .to_owned(),
            ),
            kind @ ("image" | "audio" | "resource" | "resource_link") => {
                omitted_content.push(kind.to_owned())
            }
            _ => return Err(Error::Protocol),
        }
    }
    let structured_content = result.get("structuredContent").cloned();
    if structured_content.as_ref().is_some_and(|v| !v.is_object()) {
        return Err(Error::Protocol);
    }
    if let Some(schema) = &tool.output {
        match &structured_content {
            Some(value) => {
                schema
                    .arguments(&vcp_protocol::canonical_bytes(value).map_err(|_| Error::Protocol)?)
                    .map_err(|_| Error::Protocol)?;
            }
            None if !is_error => return Err(Error::Protocol),
            _ => (),
        }
    }
    Ok(ToolResult {
        is_error,
        text,
        structured_content,
        omitted_content,
    })
}
