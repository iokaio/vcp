// SPDX-License-Identifier: Apache-2.0
//! POST-only session state. No transport, DNS, credentials, replay or polling.
//!
//! Host owns a bounded `Decoder` for each exchange, rejects duplicate headers,
//! and passes complete private JSON frames to `receive`. It may park one SSE
//! stream while a separately admitted control POST is acknowledged. `written`
//! is a trusted-host acknowledgement: first consume the transport's opaque,
//! single-exchange write observation and match its exact body digest. HTTP
//! response headers alone never prove that the request was fully written.
use super::ResponseTo;
use crate::mcp::{
    client::{Client, DiscoveredTool, Incoming, Outbound, OutboundKind, PendingKind},
    content::{DiscoveredPrompt, DiscoveredResource},
    identity::{ConnectionIdentity, ContentIdentity, ToolIdentity, PROTOCOL},
    registration::Registration,
    schema::CheckedArguments,
};
use std::{collections::BTreeMap, fmt};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SessionError {
    #[error("HTTP session state does not permit this operation")]
    State,
    #[error("HTTP session header violates the bounded session profile")]
    Header,
    #[error("HTTP session has expired; this connection cannot be reused")]
    Expired,
    #[error("HTTP status is outside the admitted response profile")]
    Status,
    #[error("HTTP acknowledgement must have an empty body")]
    AcknowledgementBody,
    #[error("HTTP exchange ended without its matching MCP reply")]
    MissingReply,
    #[error(transparent)]
    Protocol(#[from] crate::mcp::client::Error),
}

/// Bound to one session instance and one attempt; cannot be manufactured from
/// a request digest or serialized as authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExchangeId {
    owner: String,
    sequence: u64,
}

/// Private transport view. Do not persist, log or attach these values to model
/// context: the session identifier can itself be sensitive external data.
pub struct RequestHeaders<'a> {
    protocol: bool,
    session: Option<&'a str>,
}
impl RequestHeaders<'_> {
    pub fn visit(&self, mut visitor: impl FnMut(&'static str, &str)) {
        visitor("accept", "application/json, text/event-stream");
        visitor("content-type", "application/json");
        if self.protocol {
            visitor("mcp-protocol-version", PROTOCOL);
        }
        if let Some(session) = self.session {
            visitor("mcp-session-id", session);
        }
    }
}
impl fmt::Debug for RequestHeaders<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestHeaders")
            .field("negotiated", &self.protocol)
            .field("has_session", &self.session.is_some())
            .finish()
    }
}

struct Exchange {
    digest: String,
    kind: OutboundKind,
    written: bool,
    head: bool,
    replied: bool,
    proposed_session: Option<String>,
}

/// A fresh instance is a fresh connection generation. Expiry/failure never
/// creates a replacement instance, refreshes credentials, or replays a call.
pub struct Session {
    client: Client,
    owner: String,
    sequence: u64,
    exchanges: BTreeMap<u64, Exchange>,
    session: Option<String>,
    closed: bool,
}
impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("closed", &self.closed)
            .field("exchanges", &self.exchanges.len())
            .field("has_session", &self.session.is_some())
            .finish()
    }
}
impl Session {
    pub fn new(registration: Registration) -> Result<Self, SessionError> {
        Ok(Self {
            client: Client::new_http(registration)?,
            owner: vcp_domain::ExecutionId::new().to_string(),
            sequence: 0,
            exchanges: BTreeMap::new(),
            session: None,
            closed: false,
        })
    }
    pub fn connection(&self) -> Option<&ConnectionIdentity> {
        if self.closed {
            None
        } else {
            self.client.connection()
        }
    }
    pub fn pending_kind(&self) -> Option<PendingKind> {
        self.client.pending_kind()
    }
    pub fn tools(&self) -> impl Iterator<Item = &DiscoveredTool> {
        self.client.tools()
    }
    pub fn tool(&self, identity: &ToolIdentity) -> Result<&DiscoveredTool, SessionError> {
        Ok(self.client.tool(identity)?)
    }
    pub fn resources(&self) -> impl Iterator<Item = &DiscoveredResource> {
        self.client.resources()
    }
    pub fn prompts(&self) -> impl Iterator<Item = &DiscoveredPrompt> {
        self.client.prompts()
    }
    pub fn resource(
        &self,
        identity: &ContentIdentity,
    ) -> Result<&DiscoveredResource, SessionError> {
        Ok(self.client.resource(identity)?)
    }
    pub fn prompt(&self, identity: &ContentIdentity) -> Result<&DiscoveredPrompt, SessionError> {
        Ok(self.client.prompt(identity)?)
    }
    pub fn list_resources(&mut self) -> Result<Outbound, SessionError> {
        self.idle()?;
        Ok(self.client.list_resources()?)
    }
    pub fn read_resource(&mut self, identity: &ContentIdentity) -> Result<Outbound, SessionError> {
        self.idle()?;
        Ok(self.client.read_resource(identity)?)
    }
    pub fn list_prompts(&mut self) -> Result<Outbound, SessionError> {
        self.idle()?;
        Ok(self.client.list_prompts()?)
    }
    pub fn get_prompt(
        &mut self,
        identity: &ContentIdentity,
        arguments: &CheckedArguments,
    ) -> Result<Outbound, SessionError> {
        self.idle()?;
        Ok(self.client.get_prompt(identity, arguments)?)
    }
    pub fn session_digest(&self) -> Option<String> {
        self.session
            .as_ref()
            .map(|id| vcp_protocol::digest_bytes(id.as_bytes()))
    }
    /// Private capture-only view of committed and staged session identifiers.
    /// Host must screen complete decoded JSON (including keys and callback IDs)
    /// before persistence or model exposure. Retain a bounded private capture
    /// snapshot before an operation that may abort/expire this session; abort
    /// erases these identifiers even when earlier reply evidence is retained.
    /// These values never authorize a send or permit session resumption.
    pub fn visit_sensitive_values(&self, mut visitor: impl FnMut(&str)) {
        if let Some(value) = self.session.as_deref() {
            visitor(value);
        }
        for exchange in self.exchanges.values() {
            if let Some(value) = exchange.proposed_session.as_deref() {
                visitor(value);
            }
        }
    }
    pub fn initialize(&mut self) -> Result<Outbound, SessionError> {
        self.idle()?;
        Ok(self.client.initialize()?)
    }
    pub fn list_tools(&mut self) -> Result<Outbound, SessionError> {
        self.idle()?;
        Ok(self.client.list_tools()?)
    }
    pub fn call(
        &mut self,
        identity: &ToolIdentity,
        arguments: &CheckedArguments,
    ) -> Result<Outbound, SessionError> {
        self.idle()?;
        Ok(self.client.call(identity, arguments)?)
    }
    pub fn begin_exchange(&mut self, out: &Outbound) -> Result<ExchangeId, SessionError> {
        if self.closed || self.exchanges.len() >= 2 {
            return Err(SessionError::State);
        }
        self.client.validate_send(out)?;
        if self.exchanges.values().any(|e| e.digest == out.digest())
            || match out.kind() {
                OutboundKind::Request(_) => !self.exchanges.is_empty(),
                OutboundKind::Initialized | OutboundKind::Control => self
                    .exchanges
                    .values()
                    .any(|e| !matches!(e.kind, OutboundKind::Request(_))),
            }
        {
            return Err(SessionError::State);
        }
        self.sequence = self.sequence.checked_add(1).ok_or(SessionError::State)?;
        self.exchanges.insert(
            self.sequence,
            Exchange {
                digest: out.digest().to_owned(),
                kind: out.kind(),
                written: false,
                head: false,
                replied: false,
                proposed_session: None,
            },
        );
        Ok(ExchangeId {
            owner: self.owner.clone(),
            sequence: self.sequence,
        })
    }
    pub fn headers(&self, id: &ExchangeId) -> Result<RequestHeaders<'_>, SessionError> {
        let exchange = self.exchange(id)?;
        if exchange.written {
            return Err(SessionError::State);
        }
        Ok(RequestHeaders {
            protocol: self.client.connection().is_some(),
            session: self.session.as_deref(),
        })
    }
    /// Call after checking the exact transport-owned full-write observation.
    /// For control/initialized frames, the codec remains blocked until `finish`.
    pub fn written(&mut self, id: &ExchangeId, out: &Outbound) -> Result<(), SessionError> {
        self.validate_send(id, out)?;
        if matches!(out.kind(), OutboundKind::Request(_)) {
            self.client.confirm_sent(out)?;
        }
        self.exchange_mut(id)?.written = true;
        Ok(())
    }
    /// Token validation before physical send, independent of the host's
    /// mandatory current canonical authority and recipient fences.
    pub fn validate_send(&self, id: &ExchangeId, out: &Outbound) -> Result<(), SessionError> {
        let exchange = self.exchange(id)?;
        if exchange.written || exchange.digest != out.digest() || exchange.kind != out.kind() {
            return Err(SessionError::State);
        }
        self.client.validate_send(out)?;
        Ok(())
    }
    /// Host must reject duplicate session/header fields and non-UTF-8 values
    /// before this call, then construct the bounded Decoder with this class.
    /// A head may arrive before Written; no JSON may be accepted until Written.
    pub fn head(
        &mut self,
        id: &ExchangeId,
        status: u16,
        session_header: Option<&str>,
    ) -> Result<ResponseTo, SessionError> {
        let result = self.head_inner(id, status, session_header);
        self.poison_on_error(result)
    }
    fn head_inner(
        &mut self,
        id: &ExchangeId,
        status: u16,
        session_header: Option<&str>,
    ) -> Result<ResponseTo, SessionError> {
        let exchange = self.exchange(id)?;
        if exchange.head {
            return Err(SessionError::State);
        }
        if status == 404 {
            return Err(SessionError::Expired);
        }
        let response_to = match exchange.kind {
            OutboundKind::Request(_) if status == 200 => ResponseTo::Request,
            OutboundKind::Initialized | OutboundKind::Control if status == 202 => {
                ResponseTo::NotificationOrResponse
            }
            _ => return Err(SessionError::Status),
        };
        if let Some(value) = session_header {
            if value.is_empty()
                || value.len() > 1024
                || !value.bytes().all(|b| (0x21..=0x7e).contains(&b))
            {
                return Err(SessionError::Header);
            }
            if exchange.kind != OutboundKind::Request(PendingKind::Initialize)
                && self.session.as_deref() != Some(value)
            {
                return Err(SessionError::Header);
            }
        }
        let exchange = self.exchange_mut(id)?;
        exchange.head = true;
        if exchange.kind == OutboundKind::Request(PendingKind::Initialize) {
            exchange.proposed_session = session_header.map(str::to_owned);
        }
        Ok(response_to)
    }
    /// Complete bounded JSON only. Capture/sanitize a separate copy without
    /// mutating the bytes used for protocol validation or identity derivation.
    pub fn receive(&mut self, id: &ExchangeId, bytes: &[u8]) -> Result<Incoming, SessionError> {
        let result = self.receive_inner(id, bytes);
        self.poison_on_error(result)
    }
    fn receive_inner(&mut self, id: &ExchangeId, bytes: &[u8]) -> Result<Incoming, SessionError> {
        let exchange = self.exchange(id)?;
        if !exchange.written
            || !exchange.head
            || exchange.replied
            || !matches!(exchange.kind, OutboundKind::Request(_))
        {
            return Err(SessionError::State);
        }
        let incoming = self.client.receive(bytes)?;
        let terminal = matches!(
            incoming,
            Incoming::Initialized { .. }
                | Incoming::DiscoveryPage { .. }
                | Incoming::DiscoveryComplete { .. }
                | Incoming::CallReply(_)
                | Incoming::ResourceDiscoveryPage { .. }
                | Incoming::ResourceDiscoveryComplete { .. }
                | Incoming::PromptDiscoveryPage { .. }
                | Incoming::PromptDiscoveryComplete { .. }
                | Incoming::ResourceReply(_)
                | Incoming::PromptReply(_)
                | Incoming::RpcError { .. }
        );
        if matches!(incoming, Incoming::Initialized { .. }) {
            self.session = self.exchange_mut(id)?.proposed_session.take();
        }
        if terminal {
            self.exchange_mut(id)?.replied = true;
        }
        Ok(incoming)
    }
    /// After successful Decoder::finish, first pass any final JSON frame to
    /// receive, then pass that decoder's exact body byte count here. Never call
    /// this on transport/framing failure. 202 alone is not a tool receipt.
    pub fn finish(
        &mut self,
        id: &ExchangeId,
        out: &Outbound,
        body_bytes: usize,
    ) -> Result<(), SessionError> {
        let result = self.finish_inner(id, out, body_bytes);
        self.poison_on_error(result)
    }
    fn finish_inner(
        &mut self,
        id: &ExchangeId,
        out: &Outbound,
        body_bytes: usize,
    ) -> Result<(), SessionError> {
        let exchange = self.exchange(id)?;
        if !exchange.written
            || !exchange.head
            || exchange.digest != out.digest()
            || exchange.kind != out.kind()
            || !self.client.owns_outbound(out)
        {
            return Err(SessionError::State);
        }
        match exchange.kind {
            OutboundKind::Request(_) if !exchange.replied => {
                return Err(SessionError::MissingReply)
            }
            OutboundKind::Request(_) => (),
            OutboundKind::Initialized | OutboundKind::Control => {
                if body_bytes != 0 {
                    return Err(SessionError::AcknowledgementBody);
                }
                self.client.confirm_sent(out)?;
            }
        }
        self.exchanges.remove(&id.sequence);
        Ok(())
    }
    pub fn abort(&mut self) -> Option<PendingKind> {
        self.closed = true;
        self.session = None;
        self.exchanges.clear();
        self.client.abandon()
    }
    fn idle(&self) -> Result<(), SessionError> {
        if self.closed || !self.exchanges.is_empty() {
            Err(SessionError::State)
        } else {
            Ok(())
        }
    }
    fn exchange(&self, id: &ExchangeId) -> Result<&Exchange, SessionError> {
        if self.closed || id.owner != self.owner {
            return Err(SessionError::State);
        }
        self.exchanges.get(&id.sequence).ok_or(SessionError::State)
    }
    fn exchange_mut(&mut self, id: &ExchangeId) -> Result<&mut Exchange, SessionError> {
        self.exchange(id)?;
        self.exchanges
            .get_mut(&id.sequence)
            .ok_or(SessionError::State)
    }
    fn poison_on_error<T>(&mut self, result: Result<T, SessionError>) -> Result<T, SessionError> {
        if result.is_err() {
            self.abort();
        }
        result
    }
}
