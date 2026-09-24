// SPDX-License-Identifier: Apache-2.0
//! Stateful public dispatch without a transport. The host supplies current access
//! on every request; initialization and client fields never grant authority.
//! Notifications (including mutations and initialize) are ignored. A request ID
//! is required for execution; responses to notifications are always suppressed.
//! Oversized batches or responses close the session without a JSON-RPC reply.
//! Transports must close when `is_closed()` becomes true; reconnecting callers
//! reconcile outstanding durable command identities before retrying.
use crate::{
    public::PublicError,
    query::{Query, QueryError, QueryResult, SessionCursor},
    Access, Engine, HostFacts,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;
use vcp_domain::{ids::*, revision::Revision, workspace::Session};
use vcp_protocol::{
    command::{CommandReceipt, CommandResult},
    errors::{ApplicationError, Code, Retry},
    handshake::{Handshake, InitializeParams, ServerInfo},
    jsonrpc::{self, Envelope, Message, Request, Response, RpcError},
    methods::{self, Call, ResultValue},
};
use vcp_store::contract::CanonicalStore;

pub const MAX_BATCH: usize = 64;
/// Direct canonical reads and mutations; live controls require the lifecycle host.
pub const METHODS: &[&str] = &[
    "session/create",
    "session/fork",
    "session/read",
    "session/list",
    "task/read",
    "task/presentation",
    "usage/read",
    "context/inspect",
    "routing/explain",
    "turn/steer",
    "approval/respond",
    "command/read",
];
pub const ESSENTIAL_CAPABILITIES: &[&str] = &["jsonrpc/2.0", "durable-command/1"];
pub const APPROVAL_SOURCE_REVISIONS_CAPABILITY: &str = "approval/source-revisions/1";
pub const MEMORY_INSPECTION_STATE_CAPABILITY: &str = "memory/inspection-state/1";
pub const MEMORY_QUERY_SOURCES_CAPABILITY: &str = vcp_protocol::memory_query::CAPABILITY;
pub const MEMORY_GOVERNANCE_CAPABILITY: &str = vcp_protocol::memory_governance::CAPABILITY;
pub const SESSION_EXPORT_LOCAL_CAPABILITY: &str = "session/export-local/1";
pub const WORKSPACE_BINDING_CAPABILITY: &str = "workspace/binding/1";
pub const MEMORY_RETENTION_CAPABILITY: &str = vcp_protocol::memory_retention::CAPABILITY;

#[cfg(test)]
mod approval_source_tests;
#[cfg(test)]
mod memory_inspection_tests;
#[cfg(test)]
mod memory_query_tests;
#[cfg(test)]
mod workspace_binding_tests;
#[cfg(test)]
mod inspector_query_tests;

/// Presentation extensions require explicit negotiation and implemented methods.
/// Method registration in the schema alone never advertises the extension.
pub fn capabilities_for_methods(methods: &[String]) -> BTreeSet<String> {
    let mut capabilities: BTreeSet<_> = methods
        .iter()
        .cloned()
        .chain(ESSENTIAL_CAPABILITIES.iter().map(|s| (*s).to_owned()))
        .collect();
    if capabilities.contains("task/read") && capabilities.contains("approval/respond") {
        capabilities.insert(APPROVAL_SOURCE_REVISIONS_CAPABILITY.to_owned());
    }
    if capabilities.contains("memory/inspect") {
        capabilities.insert(MEMORY_INSPECTION_STATE_CAPABILITY.to_owned());
    }
    if capabilities.contains("memory/query") {
        capabilities.insert(MEMORY_QUERY_SOURCES_CAPABILITY.to_owned());
    }
    if capabilities.contains("history/query") {
        capabilities.insert(vcp_protocol::history::CAPABILITY.to_owned());
    }
    if capabilities.contains("memory/history") {
        capabilities.insert(vcp_protocol::memory_history::CAPABILITY.to_owned());
    }
    if capabilities.contains("policy/read") {
        capabilities.insert(vcp_protocol::policy_inspection::CAPABILITY.to_owned());
    }
    if capabilities.contains("routing/status") {
        capabilities.insert(vcp_protocol::routing_inspection::CAPABILITY.to_owned());
    }
    if ["routing/reportCapture", "routing/reportRead", "routing/preview", "routing/apply", "routing/rollback"].iter().any(|method| capabilities.contains(*method)) {
        capabilities.insert(vcp_protocol::routing_optimizer::CAPABILITY.to_owned());
    }
    if ["backup/status", "backup/create", "backup/read", "backup/retry", "backup/cancel"]
        .iter()
        .any(|method| capabilities.contains(*method))
    {
        capabilities.insert(vcp_protocol::backup_publisher::CAPABILITY.to_owned());
    }
    if capabilities.contains("session/export") {
        capabilities.insert(SESSION_EXPORT_LOCAL_CAPABILITY.to_owned());
    }
    if capabilities.contains("workspace/open") {
        capabilities.insert(WORKSPACE_BINDING_CAPABILITY.to_owned());
    }
    if [
        "editor/context",
        "editor/prepare",
        "editor/changeRead",
        "editor/dispatch",
        "editor/changeResult",
    ]
    .iter()
    .all(|method| capabilities.contains(*method))
    {
        capabilities.insert(vcp_protocol::editor::CAPABILITY.to_owned());
    }
    if [
        "memory/forget",
        "memory/forgetPreview",
        "memory/forgetPreviewRead",
        "memory/forgetRead",
    ]
    .iter()
    .all(|method| capabilities.contains(*method))
    {
        capabilities.insert(MEMORY_RETENTION_CAPABILITY.to_owned());
    }
    if ["memory/propose", "memory/resolve", "memory/review"]
        .iter()
        .all(|method| capabilities.contains(*method))
    {
        capabilities.insert(MEMORY_GOVERNANCE_CAPABILITY.to_owned());
    }
    capabilities
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum RpcResponses {
    Single(Response),
    Batch(Vec<Response>),
}

/// Implemented by trusted hosts, never by wire parameters. A live host must
/// recheck current access and controller leases in the same owned operation as
/// admission, and project its typed result at that canonical boundary. In
/// particular, steering must drain live authority before committing. Dropping
/// the call waiter must not abandon an already accepted operation.
///
/// Static async dispatch intentionally imposes no object-safe or Send contract;
/// the eventual transport chooses its execution task without changing ownership.
#[allow(async_fn_in_trait)]
pub trait RpcHost {
    fn supported_methods(&self) -> &[&str];
    fn authorize(&self, current: &Access) -> Result<(), RpcError>;
    async fn call(&mut self, call: Call, current: &Access) -> Result<ResultValue, RpcError>;
}

/// Direct canonical adapter for the existing in-process boundary. This does not
/// replace a live CanonicalHost's authority draining or execution supervision.
pub struct EngineRpcHost<'a, S: CanonicalStore> {
    pub engine: &'a mut Engine<S>,
    pub facts: &'a HostFacts,
}

pub struct RpcSession {
    handshake: Handshake,
    server: ServerInfo,
    negotiated: BTreeSet<String>,
    principal: Option<(ActorId, WorkspaceId, SessionId)>,
    closed: bool,
}

impl RpcSession {
    /// Configuration names typed methods only. Initialization additionally checks
    /// every advertised method against the selected host's actual implementation.
    /// Resource/host facts remain the trusted caller's property.
    pub fn new(server: ServerInfo) -> Result<Self, RpcError> {
        let methods: BTreeSet<_> = server.methods.iter().cloned().collect();
        if methods.len() != server.methods.len()
            || methods
                .iter()
                .any(|method| !Call::METHODS.contains(&method.as_str()))
        {
            return Err(RpcError::internal_error());
        }
        let supported: BTreeSet<_> = methods
            .iter()
            .cloned()
            .chain(ESSENTIAL_CAPABILITIES.iter().map(|s| (*s).to_owned()))
            .collect();
        let available = capabilities_for_methods(&server.methods);
        if !supported.is_subset(&server.capabilities) || !server.capabilities.is_subset(&available)
        {
            return Err(RpcError::internal_error());
        }
        Ok(Self {
            handshake: Handshake::default(),
            server,
            negotiated: BTreeSet::new(),
            principal: None,
            closed: false,
        })
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    pub async fn dispatch<S: CanonicalStore>(
        &mut self,
        engine: &mut Engine<S>,
        access: &Access,
        host: &HostFacts,
        envelope: Envelope,
    ) -> Option<RpcResponses> {
        self.dispatch_host(
            &mut EngineRpcHost {
                engine,
                facts: host,
            },
            access,
            envelope,
        )
        .await
    }

    pub async fn dispatch_host<H: RpcHost>(
        &mut self,
        host: &mut H,
        access: &Access,
        envelope: Envelope,
    ) -> Option<RpcResponses> {
        if self.closed {
            return None;
        }
        let responses = match envelope {
            Envelope::Single(message) => self
                .message(host, access, message)
                .await
                .map(RpcResponses::Single),
            Envelope::Batch(messages) => {
                if messages.is_empty() {
                    return Some(RpcResponses::Single(Response::invalid(
                        RpcError::invalid_request(),
                    )));
                }
                if messages.len() > MAX_BATCH {
                    self.closed = true;
                    return None;
                }
                let mut responses = Vec::new();
                for message in messages {
                    if let Some(response) = self.message(host, access, message).await {
                        responses.push(response);
                    }
                }
                if responses.is_empty() {
                    None
                } else {
                    Some(RpcResponses::Batch(responses))
                }
            }
        };
        // A committed mutation stays committed even if its response cannot fit.
        // The client reconciles original command IDs; this is never a retry grant.
        let responses = responses?;
        if jsonrpc::encode_frame(&responses, self.server.limits.maximum_frame_bytes as usize)
            .is_err()
        {
            self.closed = true;
            None
        } else {
            Some(responses)
        }
    }

    async fn message<H: RpcHost>(
        &mut self,
        host: &mut H,
        access: &Access,
        message: Result<Message, RpcError>,
    ) -> Option<Response> {
        match message {
            Err(error) => Some(Response::invalid(error)),
            Ok(Message::Response(_)) => Some(Response::invalid(RpcError::invalid_request())),
            Ok(Message::Request(request)) if request.is_notification() => None,
            Ok(Message::Request(request)) => {
                let outcome = self.request(host, access, &request).await;
                request.respond(outcome)
            }
        }
    }

    async fn request<H: RpcHost>(
        &mut self,
        host: &mut H,
        access: &Access,
        request: &Request,
    ) -> Result<Value, RpcError> {
        if jsonrpc::encode_frame(request, self.server.limits.maximum_frame_bytes as usize).is_err()
        {
            return Err(application(
                Code::ResourceLimit,
                Retry::Never,
                None,
                "request frame exceeds negotiated limit",
            ));
        }
        // A host must refresh access before dispatch; cached initialization is not
        // a grant. Bind identity too, so one connection cannot switch principals.
        host.authorize(access)?;
        let principal = (
            access.actor.clone(),
            access.workspace.clone(),
            access.session.clone(),
        );
        if self
            .principal
            .as_ref()
            .is_some_and(|current| current != &principal)
        {
            return Err(application(
                Code::PolicyDenied,
                Retry::Never,
                None,
                "connection principal or scope changed",
            ));
        }
        if request.method == "initialize" {
            if self
                .server
                .methods
                .iter()
                .any(|method| !host.supported_methods().contains(&method.as_str()))
            {
                return Err(RpcError::internal_error());
            }
            let params: InitializeParams =
                serde_json::from_value(request.params.clone().unwrap_or(Value::Null))
                    .map_err(|_| RpcError::invalid_params())?;
            let preview = vcp_protocol::handshake::negotiate(&params, &self.server)?;
            let preview = request
                .respond(serde_json::to_value(preview).map_err(|_| RpcError::internal_error()));
            if jsonrpc::encode_frame(&preview, self.server.limits.maximum_frame_bytes as usize)
                .is_err()
            {
                return Err(application(
                    Code::ResourceLimit,
                    Retry::Never,
                    None,
                    "initialization response exceeds frame limit",
                ));
            }
            let result = self.handshake.initialize(&params, &self.server)?;
            self.negotiated = result.capabilities.iter().cloned().collect();
            self.principal = Some(principal);
            return serde_json::to_value(result).map_err(|_| RpcError::internal_error());
        }
        self.handshake.require_initialized()?;
        if !Call::METHODS.contains(&request.method.as_str()) {
            return Err(RpcError::method_not_found());
        }
        if !self
            .server
            .methods
            .iter()
            .any(|method| method == &request.method)
            || !self.negotiated.contains(&request.method)
            || !host.supported_methods().contains(&request.method.as_str())
        {
            return Err(application(
                Code::CapabilityUnavailable,
                Retry::Never,
                None,
                "method capability was not negotiated",
            ));
        }
        let call = Call::decode(
            &request.method,
            request.params.clone().unwrap_or(Value::Null),
        )
        .map_err(|_| RpcError::invalid_params())?;
        if matches!(
            call,
            Call::EditorContext(_)
                | Call::EditorPrepare(_)
                | Call::EditorChangeRead(_)
                | Call::EditorDispatch(_)
                | Call::EditorChangeResult(_)
        ) && !self.negotiated.contains(vcp_protocol::editor::CAPABILITY)
        {
            return Err(application(
                Code::CapabilityUnavailable,
                Retry::Never,
                call.command_id().cloned(),
                "prepared editor capability was not negotiated",
            ));
        }
        let inspector_profile = match &call {
            Call::BackupStatus(_) | Call::BackupCreate(_) | Call::BackupRead(_) | Call::BackupRetry(_) | Call::BackupCancel(_) => Some(vcp_protocol::backup_publisher::CAPABILITY),
            Call::HistoryQuery(_) => Some(vcp_protocol::history::CAPABILITY),
            Call::MemoryHistory(_) => Some(vcp_protocol::memory_history::CAPABILITY),
            Call::PolicyRead(_) => Some(vcp_protocol::policy_inspection::CAPABILITY),
            Call::RoutingStatus(_) => Some(vcp_protocol::routing_inspection::CAPABILITY),
            Call::RoutingReportCapture(_) | Call::RoutingReportRead(_) | Call::RoutingPreview(_)
                | Call::RoutingApply(_) | Call::RoutingRollback(_) => Some(vcp_protocol::routing_optimizer::CAPABILITY),
            _ => None,
        };
        if inspector_profile.is_some_and(|profile| !self.negotiated.contains(profile)) {
            return Err(application(
                Code::CapabilityUnavailable,
                Retry::Never,
                None,
                "inspector query profile was not negotiated",
            ));
        }
        if matches!(call, Call::MemoryQuery(_))
            && !self.negotiated.contains(MEMORY_QUERY_SOURCES_CAPABILITY)
        {
            return Err(application(
                Code::CapabilityUnavailable,
                Retry::Never,
                None,
                "memory query source capability was not negotiated",
            ));
        }
        if matches!(call, Call::SessionExport(_))
            && !self.negotiated.contains(SESSION_EXPORT_LOCAL_CAPABILITY)
        {
            return Err(application(
                Code::CapabilityUnavailable,
                Retry::Never,
                call.command_id().cloned(),
                "local export profile was not negotiated",
            ));
        }
        if matches!(
            call,
            Call::MemoryPropose(_) | Call::MemoryResolve(_) | Call::MemoryReview(_)
        ) && !self.negotiated.contains(MEMORY_GOVERNANCE_CAPABILITY)
        {
            return Err(application(
                Code::CapabilityUnavailable,
                Retry::Never,
                call.command_id().cloned(),
                "memory governance capability was not negotiated",
            ));
        }
        if matches!(call, Call::MemoryInspect(_))
            && !self.negotiated.contains(MEMORY_INSPECTION_STATE_CAPABILITY)
        {
            return Err(application(
                Code::CapabilityUnavailable,
                Retry::Never,
                None,
                "memory inspection state capability was not negotiated",
            ));
        }
        if matches!(
            call,
            Call::MemoryForget(_)
                | Call::MemoryForgetPreview(_)
                | Call::MemoryForgetPreviewRead(_)
                | Call::MemoryForgetRead(_)
        ) && !self.negotiated.contains(MEMORY_RETENTION_CAPABILITY)
        {
            return Err(application(
                Code::CapabilityUnavailable,
                Retry::Never,
                call.command_id().cloned(),
                "memory retention profile was not negotiated",
            ));
        }
        let mut result = host.call(call, access).await?;
        if !self.negotiated.contains(WORKSPACE_BINDING_CAPABILITY) {
            if let ResultValue::Workspace(view) = &mut result {
                view.root_id = None;
                view.binding_revision = None;
            }
        }
        if !self
            .negotiated
            .contains(APPROVAL_SOURCE_REVISIONS_CAPABILITY)
        {
            omit_approval_source_revisions(&mut result);
        }
        serde_json::to_value(result).map_err(|_| RpcError::internal_error())
    }
}

fn omit_approval_source_revisions(result: &mut ResultValue) {
    let omit = |task: &mut methods::TaskView| {
        for input in &mut task.pending_inputs {
            input.effect_revision = None;
            input.policy_revision = None;
        }
    };
    match result {
        ResultValue::Task(task) => omit(task),
        ResultValue::Snapshot(snapshot) => snapshot.tasks.iter_mut().for_each(omit),
        _ => (),
    }
}

impl<S: CanonicalStore> RpcHost for EngineRpcHost<'_, S> {
    fn supported_methods(&self) -> &[&str] {
        METHODS
    }

    fn authorize(&self, access: &Access) -> Result<(), RpcError> {
        self.engine.authorize(access).map_err(|_| {
            application(
                Code::PolicyDenied,
                Retry::AfterRevalidation,
                None,
                "current read access denied",
            )
        })?;
        self.engine
            .query(
                access,
                &Query::Session {
                    session: access.session.clone(),
                },
            )
            .map_err(query_error)?;
        Ok(())
    }

    async fn call(&mut self, call: Call, access: &Access) -> Result<ResultValue, RpcError> {
        self.authorize(access)?;
        let engine = &mut *self.engine;
        let host = self.facts;
        let result = match &call {
            Call::SessionRead(p) => {
                check_scope(&p.scope, access)?;
                match engine
                    .query(
                        access,
                        &Query::Session {
                            session: access.session.clone(),
                        },
                    )
                    .map_err(query_error)?
                {
                    QueryResult::Session { session, .. } => {
                        ResultValue::Session(session_view(session)?)
                    }
                    _ => return Err(RpcError::internal_error()),
                }
            }
            Call::SessionList(p) => {
                check_scope(&p.scope, access)?;
                let cursor: Option<SessionCursor> = p
                    .cursor
                    .as_ref()
                    .map(|value| serde_json::from_str(value))
                    .transpose()
                    .map_err(|_| RpcError::invalid_params())?;
                match engine
                    .query(
                        access,
                        &Query::Sessions {
                            limit: p.limit,
                            cursor,
                        },
                    )
                    .map_err(query_error)?
                {
                    QueryResult::Sessions {
                        watermark,
                        sessions,
                        next,
                    } => ResultValue::Sessions(methods::SessionPage {
                        watermark: watermark.get().into(),
                        sessions: sessions
                            .into_iter()
                            .map(session_view)
                            .collect::<Result<_, _>>()?,
                        next_cursor: next
                            .map(|cursor| serde_json::to_string(&cursor))
                            .transpose()
                            .map_err(|_| RpcError::internal_error())?,
                    }),
                    _ => return Err(RpcError::internal_error()),
                }
            }
            Call::TaskRead(p) => {
                check_scope(&p.scope, access)?;
                let task =
                    TaskId::parse(p.task.as_str()).map_err(|_| RpcError::invalid_params())?;
                match engine
                    .query(access, &Query::Task { task })
                    .map_err(query_error)?
                {
                    QueryResult::Task { task, .. } => {
                        ResultValue::Task(task_view(engine.store().state(), task)?)
                    }
                    _ => return Err(RpcError::internal_error()),
                }
            }
            Call::TaskPresentation(p) => ResultValue::Presentation(
                engine
                    .public_presentation(access, p, self.facts.now)
                    .map_err(query_error)?,
            ),
            Call::UsageRead(p) => {
                ResultValue::Usage(engine.public_usage(access, p).map_err(query_error)?)
            }
            Call::ContextInspect(p) => {
                ResultValue::Evidence(engine.public_context(access, p).map_err(query_error)?)
            }
            Call::RoutingExplain(p) => {
                ResultValue::Evidence(engine.public_routing(access, p).map_err(query_error)?)
            }
            Call::CommandRead(p) => {
                check_scope(&p.scope, access)?;
                let command = CommandId::parse(p.command_id.as_str())
                    .map_err(|_| RpcError::invalid_params())?;
                match engine
                    .query(access, &Query::Command { command })
                    .map_err(query_error)?
                {
                    QueryResult::Command { receipt, .. } => acceptance(engine, access, &receipt)?,
                    _ => return Err(RpcError::internal_error()),
                }
            }
            Call::SessionCreate(_)
            | Call::SessionFork(_)
            | Call::TurnSteer(_)
            | Call::ApprovalRespond(_) => {
                let operation = call.mutation().map(|mutation| mutation.command_id.clone());
                let approval = matches!(call, Call::ApprovalRespond(_));
                let receipt = engine
                    .handle_public(call, access, host)
                    .await
                    .map_err(|error| public_error(error, operation, approval))?;
                acceptance(engine, access, &receipt)?
            }
            _ => {
                return Err(application(
                    Code::CapabilityUnavailable,
                    Retry::Never,
                    None,
                    "method has no public adapter",
                ))
            }
        };
        Ok(result)
    }
}

fn check_scope(scope: &methods::Scope, access: &Access) -> Result<(), RpcError> {
    if scope.workspace.as_str() != access.workspace.as_str()
        || scope.session.as_str() != access.session.as_str()
    {
        return Err(application(
            Code::PolicyDenied,
            Retry::Never,
            None,
            "request scope differs from authenticated scope",
        ));
    }
    Ok(())
}
fn id(value: &str) -> Result<methods::Id, RpcError> {
    value
        .to_owned()
        .try_into()
        .map_err(|_| RpcError::internal_error())
}
pub(crate) fn session_view(session: Session) -> Result<methods::SessionView, RpcError> {
    Ok(methods::SessionView {
        scope: methods::Scope {
            workspace: id(session.workspace.as_str())?,
            session: id(session.id.as_str())?,
        },
        revision: session.revision.get().into(),
        configuration_revision: session.configuration.get().into(),
        fork_origin: session
            .fork_origin
            .as_ref()
            .map(|value| id(value.as_str()))
            .transpose()?,
        fork_through: session
            .fork_through
            .as_ref()
            .map(|value| id(value.as_str()))
            .transpose()?,
    })
}
/// Project only inspected canonical records in this task's scope. Pending inputs
/// describe retained approval questions, not a promise that a response can still
/// be admitted: expiry, ownership and policy are rechecked by approval/respond.
/// There is no persisted addressable generic Question/Reconciliation input model;
/// waiting state and unknown effects must not manufacture input identities.
pub(crate) fn task_view(
    state: &vcp_store::contract::State,
    task: vcp_domain::task::Task,
) -> Result<methods::TaskView, RpcError> {
    use vcp_domain::{
        effect::{Effect, EffectState},
        task::TaskState,
    };
    use vcp_protocol::command::{Approval, ApprovalState};
    use vcp_store::contract::Collection;
    let invalid = || query_error(QueryError::InvalidData);
    let mut pending_inputs = Vec::new();
    let mut pending_bytes = 0usize;
    // Known means every retained effect's disposition was inspected, not success.
    // Old-steering effects are included: steering cannot erase uncertainty.
    let mut effect_rank = 0;
    for row in state
        .records
        .values()
        .filter(|row| row.workspace == task.scope.workspace)
    {
        match row.collection {
            Collection::Approval => {
                let approval: Approval = row.decode().map_err(|_| invalid())?;
                if approval.id.as_str() != row.id
                    || approval.revision != row.revision
                    || approval.scope.workspace != row.workspace
                {
                    return Err(invalid());
                }
                if approval.scope != task.scope
                    || approval.steering != task.steering
                    || approval.state != ApprovalState::Pending
                {
                    continue;
                }
                pending_bytes = pending_bytes.saturating_add(approval.operation_digest.len());
                if pending_inputs.len() == 128 || pending_bytes > crate::query::MAX_RESULT_BYTES {
                    return Err(query_error(QueryError::Limit));
                }
                pending_inputs.push(methods::PendingInput {
                    id: id(approval.id.as_str())?,
                    kind: methods::InputKind::Approval,
                    revision: approval.revision.get().into(),
                    operation_digest: Some(approval.operation_digest),
                    effect_revision: Some(approval.effect_revision.get().into()),
                    policy_revision: Some(approval.policy.get().into()),
                });
            }
            Collection::Effect => {
                let effect: Effect = row.decode().map_err(|_| invalid())?;
                if effect.id.as_str() != row.id
                    || effect.revision != row.revision
                    || effect.scope.workspace != row.workspace
                {
                    return Err(invalid());
                }
                if effect.scope != task.scope {
                    continue;
                }
                let rank = if effect.redaction.is_some() {
                    3
                } else {
                    match effect.state {
                        EffectState::OutcomeUnknown | EffectState::DispatchRecorded => 3,
                        EffectState::Failed | EffectState::Cancelled
                            if !effect.observed_changes.is_empty() =>
                        {
                            2
                        }
                        EffectState::Proposed
                        | EffectState::Validated
                        | EffectState::Authorized
                        | EffectState::Running => 1,
                        EffectState::Succeeded | EffectState::Failed | EffectState::Cancelled => 0,
                    }
                };
                effect_rank = effect_rank.max(rank);
            }
            _ => {}
        }
    }
    let turn = match crate::public::current_public_turn(state, &task.scope) {
        Ok(turn) => turn
            .filter(|turn| turn.steering == task.steering)
            .map(|turn| id(turn.id.as_str()))
            .transpose()?,
        // Retention can remove creation evidence. Null is an unknown selection,
        // never an arbitrary opaque-ID or per-turn-revision ordering fallback.
        Err(PublicError::Unavailable) => None,
        Err(error) => return Err(public_error(error, None, false)),
    };
    let reason = if task.redaction.is_some() {
        "Task content was removed.".to_owned()
    } else {
        task.reason
    };
    if reason.is_empty() {
        return Err(invalid());
    }
    if reason.chars().count() > 4096 {
        return Err(query_error(QueryError::Limit));
    }
    let view = methods::TaskView {
        scope: methods::Scope {
            workspace: id(task.scope.workspace.as_str())?,
            session: id(task.scope.session.as_str())?,
        },
        task: id(task.scope.task.as_str())?,
        root: id(task.root.as_str())?,
        parent: task
            .parent
            .as_ref()
            .map(|parent| id(parent.as_str()))
            .transpose()?,
        turn,
        revision: task.revision.get().into(),
        steering_revision: task.steering.get().into(),
        state: match task.state {
            TaskState::Pending => methods::TaskStatus::Pending,
            TaskState::Running => methods::TaskStatus::Running,
            TaskState::WaitingForInput => methods::TaskStatus::WaitingForInput,
            TaskState::Blocked => methods::TaskStatus::Blocked,
            TaskState::Paused => methods::TaskStatus::Paused,
            TaskState::Completed => methods::TaskStatus::Completed,
            TaskState::Failed => methods::TaskStatus::Failed,
            TaskState::Cancelled => methods::TaskStatus::Cancelled,
        },
        reason,
        pending_inputs,
        effects: match effect_rank {
            3 => methods::EffectStatus::Unknown,
            2 => methods::EffectStatus::Partial,
            1 => methods::EffectStatus::Pending,
            _ => methods::EffectStatus::Known,
        },
    };
    // A projection cannot silently truncate pending inputs or a large reason.
    if jsonrpc::encode_frame(&view, crate::query::MAX_RESULT_BYTES).is_err() {
        return Err(query_error(QueryError::Limit));
    }
    Ok(view)
}
/// Project a canonical receipt inside the host's serialized admission operation.
/// Recheck current access and persisted identity even when the caller just wrote
/// the receipt, so this helper cannot project a forged or out-of-scope result.
pub fn acceptance<S: CanonicalStore>(
    engine: &Engine<S>,
    access: &Access,
    receipt: &CommandReceipt,
) -> Result<ResultValue, RpcError> {
    match engine
        .query(
            access,
            &Query::Command {
                command: receipt.command.clone(),
            },
        )
        .map_err(query_error)?
    {
        QueryResult::Command {
            receipt: current, ..
        } if current == *receipt => (),
        _ => return Err(RpcError::internal_error()),
    }
    let command_id = id(receipt.command.as_str())?;
    let revision: Revision = match receipt.result {
        CommandResult::Accepted { revision } => revision,
        _ => {
            return Err(application(
                Code::CapabilityUnavailable,
                Retry::Never,
                Some(id(receipt.command.as_str())?),
                "legacy command result has no public acceptance projection",
            ))
        }
    };
    let event = engine
        .store()
        .state()
        .events
        .iter()
        .find(|event| {
            event.watermark == receipt.watermark
                && event.event.correlation == receipt.command
                && event.event.workspace == access.workspace
                && event.event.session == access.session
        })
        .ok_or_else(|| {
            application(
                Code::CursorGap,
                Retry::AfterRevalidation,
                Some(command_id.clone()),
                "command projection scope evidence unavailable",
            )
        })?;
    Ok(ResultValue::Acceptance(methods::Acceptance {
        command_id,
        scope: methods::Scope {
            workspace: id(access.workspace.as_str())?,
            session: id(access.session.as_str())?,
        },
        task: event
            .event
            .task
            .as_ref()
            .map(|task| id(task.as_str()))
            .transpose()?,
        turn: accepted_turn(engine, access, receipt)?,
        revision: revision.get().into(),
        watermark: receipt.watermark.get().into(),
        outcome: methods::OperationOutcome::Accepted,
    }))
}

/// A receipt identifies the turn it created, never whichever turn is current
/// when a retry is read. Only the original retained Queued genesis fact proves
/// that identity; subsequent task/turn changes cannot redirect the receipt.
fn accepted_turn<S: CanonicalStore>(
    engine: &Engine<S>,
    access: &Access,
    receipt: &CommandReceipt,
) -> Result<Option<methods::Id>, RpcError> {
    use vcp_domain::{
        retention::RetentionMask,
        task::{Turn, TurnState},
    };
    use vcp_protocol::event::EventKind;
    use vcp_store::contract::Collection;
    let unavailable = || {
        application(
            Code::CursorGap,
            Retry::AfterRevalidation,
            methods::Id::try_from(receipt.command.to_string()).ok(),
            "accepted turn identity evidence unavailable",
        )
    };
    let state = engine.store().state();
    // Public start v1 commits exactly four ordered facts: task, input artifact,
    // root ledger, queued turn. The retained receipt span survives projection
    // pruning and distinguishes it from legacy one-event Start/AdvanceTurn
    // receipts. Legacy receipts keep their existing metadata-only projection.
    if receipt
        .last_event
        .get()
        .checked_sub(receipt.first_event.get())
        != Some(3)
    {
        return Ok(None);
    }
    let mut expected = None;
    for genesis in state.events.iter().filter(|event| {
        event.watermark == receipt.watermark
            && event.event.correlation == receipt.command
            && event.event.workspace == access.workspace
            && event.event.session == access.session
            && event.event.kind == EventKind::TaskCreated
    }) {
        let task = genesis.event.task.as_ref().ok_or_else(unavailable)?;
        let scope = vcp_domain::workspace::Scope {
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            task: task.clone(),
        };
        if expected.is_some()
            || crate::public_start::retained_start_budget(state, &scope)
                .map_err(|_| unavailable())?
                .is_none()
        {
            return Err(unavailable());
        }
        expected = Some(id(genesis.event.data["public_start"]["turn"]
            .as_str()
            .ok_or_else(unavailable)?)?);
    }
    let expected = expected.ok_or_else(unavailable)?;
    let mut result = None;
    for event in state.events.iter().filter(|event| {
        event.watermark == receipt.watermark
            && event.event.correlation == receipt.command
            && event.event.workspace == access.workspace
            && event.event.session == access.session
            && event.event.kind == EventKind::TurnTransition
    }) {
        if event.redaction.is_some() || event.event.data["schema_version"] != 1 {
            return Err(unavailable());
        }
        for row in state.records.values().filter(|row| {
            row.collection == Collection::Tombstone && row.workspace == access.workspace
        }) {
            let mask: RetentionMask = row.decode().map_err(|_| unavailable())?;
            mask.validate().map_err(|_| unavailable())?;
            if mask.workspace != access.workspace
                || (mask.session == access.session
                    && mask.first <= event.sequence
                    && event.sequence <= mask.last)
            {
                return Err(unavailable());
            }
        }
        for fact in event.event.data["facts"]
            .as_array()
            .ok_or_else(unavailable)?
        {
            if fact["collection"] != "turn" {
                continue;
            }
            let revision: Revision =
                serde_json::from_value(fact["revision"].clone()).map_err(|_| unavailable())?;
            if revision != Revision::ZERO {
                continue;
            }
            let turn: Turn =
                serde_json::from_value(fact["value"].clone()).map_err(|_| unavailable())?;
            if turn.scope.workspace != access.workspace
                || turn.scope.session != access.session
                || event.event.task.as_ref() != Some(&turn.scope.task)
                || turn.revision != Revision::ZERO
                || turn.state != TurnState::Queued
                || turn.cause != event.event.id
                || turn.redaction.is_some()
                || fact["id"] != turn.id.as_str()
                || result.is_some()
            {
                return Err(unavailable());
            }
            result = Some(id(turn.id.as_str())?);
        }
    }
    if result.as_ref() != Some(&expected) {
        return Err(unavailable());
    }
    Ok(result)
}
fn application(
    code: Code,
    retry: Retry,
    operation: Option<methods::Id>,
    explanation: &str,
) -> RpcError {
    ApplicationError {
        code,
        retry,
        operation,
        explanation: explanation.into(),
        reconciliation: None,
    }
    .into_rpc()
}
pub fn query_error(error: QueryError) -> RpcError {
    let (code, retry) = match error {
        QueryError::Access | QueryError::Unavailable => {
            (Code::PolicyDenied, Retry::AfterRevalidation)
        }
        QueryError::StaleCursor => (Code::CursorGap, Retry::AfterRevalidation),
        QueryError::Limit => (Code::ResourceLimit, Retry::Never),
        QueryError::InvalidData => (Code::StoreUnavailable, Retry::Never),
    };
    application(code, retry, None, "scoped read unavailable")
}
/// Preserve the same stable error classification across direct and live hosts.
pub fn public_error(
    error: PublicError,
    operation: Option<methods::Id>,
    approval: bool,
) -> RpcError {
    let (code, retry) = match error {
        PublicError::Access => (Code::PolicyDenied, Retry::AfterRevalidation),
        PublicError::InvalidParameters => return RpcError::invalid_params(),
        PublicError::CapabilityUnavailable => (Code::CapabilityUnavailable, Retry::Never),
        PublicError::CommandConflict => (Code::CommandConflict, Retry::Never),
        PublicError::StaleState => (
            if approval {
                Code::ApprovalStale
            } else {
                Code::VersionConflict
            },
            Retry::AfterRevalidation,
        ),
        PublicError::Unavailable => (Code::PolicyDenied, Retry::AfterRevalidation),
        PublicError::OutcomeUnknown => (Code::OutcomeUnknown, Retry::ReconcileOriginal),
    };
    application(
        code,
        retry,
        operation,
        "public command was not acknowledged",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vcp_domain::{revision::*, workspace::Binding};
    use vcp_protocol::{
        command::{Command, CommandEnvelope},
        handshake::{ConnectionLimits, ExecutionHost},
    };
    use vcp_store::{BackendKind, Store};

    #[test]
    fn workspace_binding_capability_requires_implemented_workspace_projection() {
        assert!(!capabilities_for_methods(&["session/read".into()])
            .contains(WORKSPACE_BINDING_CAPABILITY));
        assert!(capabilities_for_methods(&["workspace/open".into()])
            .contains(WORKSPACE_BINDING_CAPABILITY));
    }

    #[test]
    fn prepared_editor_capability_requires_every_hosted_method() {
        let all = [
            "editor/context",
            "editor/prepare",
            "editor/changeRead",
            "editor/dispatch",
            "editor/changeResult",
        ]
        .map(str::to_owned);
        assert!(capabilities_for_methods(&all).contains(vcp_protocol::editor::CAPABILITY));
        for missing in 0..all.len() {
            let partial = all
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != missing)
                .map(|(_, method)| method.clone())
                .collect::<Vec<_>>();
            assert!(!capabilities_for_methods(&partial).contains(vcp_protocol::editor::CAPABILITY));
        }
    }

    struct ProbeHost {
        allowed: bool,
        enabled: bool,
        authorizations: std::cell::Cell<usize>,
        calls: usize,
    }

    impl RpcHost for ProbeHost {
        fn supported_methods(&self) -> &[&str] {
            if self.enabled {
                &["session/read"]
            } else {
                &[]
            }
        }
        fn authorize(&self, access: &Access) -> Result<(), RpcError> {
            self.authorizations.set(self.authorizations.get() + 1);
            if !self.allowed || !access.read {
                return Err(application(
                    Code::PolicyDenied,
                    Retry::AfterRevalidation,
                    None,
                    "fixture current access denied",
                ));
            }
            Ok(())
        }
        async fn call(&mut self, call: Call, access: &Access) -> Result<ResultValue, RpcError> {
            self.calls += 1;
            let Call::SessionRead(params) = call else {
                panic!("unexpected host call")
            };
            check_scope(&params.scope, access)?;
            Ok(ResultValue::Session(methods::SessionView {
                scope: params.scope,
                revision: 0.into(),
                configuration_revision: 0.into(),
                fork_origin: None,
                fork_through: None,
            }))
        }
    }

    /// This verifies the dispatch seam only. Real engine tests below remain the
    /// canonical behavioral evidence; this probe makes no transport/security claim.
    #[tokio::test]
    async fn host_seam_authorizes_before_decode_and_dispatches_only_negotiated_supported_calls() {
        let mut configuration = server();
        configuration.methods = vec!["session/read".into()];
        configuration.capabilities = std::iter::once("session/read".to_owned())
            .chain(ESSENTIAL_CAPABILITIES.iter().map(|name| (*name).to_owned()))
            .collect();
        let mut rpc = RpcSession::new(configuration).unwrap();
        let mut host = ProbeHost {
            allowed: false,
            enabled: true,
            authorizations: std::cell::Cell::new(0),
            calls: 0,
        };
        let malformed = request(1, "initialize", json!({"invented_governance":true}));
        let response = rpc
            .dispatch_host(
                &mut host,
                &access(),
                jsonrpc::parse_frame(&malformed.to_string()),
            )
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(response).unwrap()["error"]["data"]["details"]["code"],
            "POLICY_DENIED"
        );
        assert_eq!(host.authorizations.get(), 1);
        assert_eq!(host.calls, 0);
        host.allowed = true;
        let initialized = rpc
            .dispatch_host(
                &mut host,
                &access(),
                jsonrpc::parse_frame(&init().to_string()),
            )
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(initialized).unwrap()["result"]["methods"],
            json!(["session/read"])
        );
        let read = request(
            2,
            "session/read",
            json!({"scope":{"workspace":"workspace","session":"session"}}),
        );
        let result = rpc
            .dispatch_host(
                &mut host,
                &access(),
                jsonrpc::parse_frame(&read.to_string()),
            )
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap()["result"]["kind"],
            "session"
        );
        assert_eq!(host.calls, 1);
        host.allowed = false;
        let malformed = request(3, "session/read", json!({"invented_governance":true}));
        let response = rpc
            .dispatch_host(
                &mut host,
                &access(),
                jsonrpc::parse_frame(&malformed.to_string()),
            )
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(response).unwrap()["error"]["data"]["details"]["code"],
            "POLICY_DENIED"
        );
        assert_eq!(host.calls, 1);
        host.allowed = true;
        host.enabled = false;
        let response = rpc
            .dispatch_host(
                &mut host,
                &access(),
                jsonrpc::parse_frame(&read.to_string()),
            )
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(response).unwrap()["error"]["data"]["details"]["code"],
            "CAPABILITY_UNAVAILABLE"
        );
        assert_eq!(host.calls, 1);
    }

    #[tokio::test]
    async fn host_cannot_advertise_unimplemented_methods_during_initialization() {
        let mut rpc = RpcSession::new(server()).unwrap();
        let mut host = ProbeHost {
            allowed: true,
            enabled: true,
            authorizations: std::cell::Cell::new(0),
            calls: 0,
        };
        let response = rpc
            .dispatch_host(
                &mut host,
                &access(),
                jsonrpc::parse_frame(&init().to_string()),
            )
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(response).unwrap()["error"]["code"],
            -32603
        );
        assert_eq!(host.authorizations.get(), 1);
        assert_eq!(host.calls, 0);
    }

    #[tokio::test]
    async fn optional_controller_method_requires_typed_registry_and_selected_host_support() {
        struct ControllerProbe;
        impl RpcHost for ControllerProbe {
            fn supported_methods(&self) -> &[&str] {
                &["controller/read"]
            }
            fn authorize(&self, _: &Access) -> Result<(), RpcError> {
                Ok(())
            }
            async fn call(&mut self, call: Call, access: &Access) -> Result<ResultValue, RpcError> {
                let Call::ControllerRead(params) = call else {
                    panic!("wrong method")
                };
                check_scope(&params.scope, access)?;
                Ok(ResultValue::Controller(methods::ControllerView {
                    scope: params.scope,
                    revision: None,
                    generation: 0.into(),
                    ownership: methods::ControllerOwnership::Unclaimed,
                    watermark: 0.into(),
                }))
            }
        }
        let mut configuration = server();
        configuration.methods = vec!["controller/read".into()];
        configuration.capabilities = std::iter::once("controller/read".to_owned())
            .chain(
                ESSENTIAL_CAPABILITIES
                    .iter()
                    .map(|value| (*value).to_owned()),
            )
            .collect();
        let mut invalid = configuration.clone();
        invalid.methods.push("controller/invented".into());
        assert!(RpcSession::new(invalid).is_err());
        let initialize = request(
            1,
            "initialize",
            json!({"protocol_version":"1.0", "client":{"name":"fixture","version":"1"}, "capabilities":[], "required_capabilities":["controller/read"]}),
        );
        let mut rpc = RpcSession::new(configuration.clone()).unwrap();
        let mut host = ControllerProbe;
        let initialized = rpc
            .dispatch_host(
                &mut host,
                &access(),
                jsonrpc::parse_frame(&initialize.to_string()),
            )
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(initialized).unwrap()["result"]["methods"],
            json!(["controller/read"])
        );
        let read = request(
            2,
            "controller/read",
            json!({"scope":{"workspace":"workspace","session":"session"}}),
        );
        let result = rpc
            .dispatch_host(
                &mut host,
                &access(),
                jsonrpc::parse_frame(&read.to_string()),
            )
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap()["result"]["value"]["ownership"],
            "unclaimed"
        );
        let temp = tempfile::tempdir().unwrap();
        let mut engine = setup(temp.path(), BackendKind::Files).await;
        let mut unsupported = RpcSession::new(configuration).unwrap();
        assert_eq!(
            send(&mut unsupported, &mut engine, &access(), initialize.clone())
                .await
                .unwrap()["error"]["code"],
            -32603
        );
        let mut direct = RpcSession::new(server()).unwrap();
        assert_eq!(
            send(&mut direct, &mut engine, &access(), init())
                .await
                .unwrap()["result"]["methods"],
            json!(METHODS)
        );
        assert!(send(&mut direct, &mut engine, &access(), read)
            .await
            .unwrap()
            .get("error")
            .is_some());
        engine.into_store().close().await.unwrap();
    }

    pub(super) fn access() -> Access {
        Access {
            actor: ActorId::parse("owner").unwrap(),
            workspace: WorkspaceId::parse("workspace").unwrap(),
            session: SessionId::parse("session").unwrap(),
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            bootstrap: true,
        }
    }
    pub(super) fn server() -> ServerInfo {
        ServerInfo {
            engine_build: "fixture".into(),
            capabilities: METHODS
                .iter()
                .chain(ESSENTIAL_CAPABILITIES)
                .map(|s| (*s).to_owned())
                .collect(),
            methods: METHODS.iter().map(|s| (*s).to_owned()).collect(),
            limits: ConnectionLimits {
                maximum_frame_bytes: 256 * 1024,
                maximum_pending_requests: 64,
                maximum_subscriptions: 1,
                maximum_subscriber_queue_bytes: 256 * 1024,
            },
            execution_host: ExecutionHost {
                id: "fixture-host".into(),
                platform: "test".into(),
            },
            sandbox_capabilities: vec![],
        }
    }
    pub(super) async fn setup(path: &std::path::Path, backend: BackendKind) -> Engine<Store> {
        let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
        let access = access();
        let command = CommandEnvelope {
            version: 1,
            id: CommandId::new(),
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            task: None,
            caller: access.actor.clone(),
            controller: engine.controller().clone(),
            owner_epoch: engine.owner_epoch(),
            expected: Revision::ZERO,
            steering: SteeringRevision::ZERO,
            payload: Command::Initialize {
                binding: Binding {
                    host: HostId::new(),
                    root: "C:/rpc-fixture".into(),
                    repository: "fixture".into(),
                    worktree: "main".into(),
                    revision: Revision::ZERO,
                },
            },
        };
        engine
            .handle(command, &access, &HostFacts::inspect(Timestamp::new(1)))
            .await
            .unwrap();
        engine
    }
    pub(super) fn request(id: i64, method: &str, params: Value) -> Value {
        json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
    }
    pub(super) fn init() -> Value {
        request(
            0,
            "initialize",
            json!({"protocol_version":"1.0","client":{"name":"fixture","version":"1"},"capabilities":METHODS,"required_capabilities":ESSENTIAL_CAPABILITIES}),
        )
    }
    fn create(id: i64) -> Value {
        request(
            id,
            "session/create",
            json!({"scope":{"workspace":"workspace","session":"session"},
        "mutation":{"command_id":"create-once","expected_revision":"0","steering_revision":"0"},"new_session":"created","configuration_revision":"0"}),
        )
    }
    pub(super) async fn send(
        rpc: &mut RpcSession,
        engine: &mut Engine<Store>,
        grant: &Access,
        value: Value,
    ) -> Option<Value> {
        rpc.dispatch(
            engine,
            grant,
            &HostFacts::inspect(Timestamp::new(1)),
            jsonrpc::parse_frame(&value.to_string()),
        )
        .await
        .map(|response| serde_json::to_value(response).unwrap())
    }

    #[tokio::test]
    async fn actual_envelopes_replay_durable_commands_and_project_typed_queries_after_restart() {
        for backend in [BackendKind::Sqlite, BackendKind::Files] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("store");
            let mut engine = setup(&root, backend).await;
            let mut rpc = RpcSession::new(server()).unwrap();
            assert_eq!(
                send(&mut rpc, &mut engine, &access(), create(1))
                    .await
                    .unwrap()["error"]["data"]["kind"],
                "not_initialized"
            );
            let initialized = send(&mut rpc, &mut engine, &access(), init())
                .await
                .unwrap();
            assert_eq!(initialized["result"]["methods"], json!(METHODS));
            assert!(initialized["result"]["methods"]
                .as_array()
                .unwrap()
                .contains(&json!("task/read")));
            let first = send(&mut rpc, &mut engine, &access(), create(1))
                .await
                .unwrap();
            assert_eq!(first["result"]["kind"], "acceptance");
            let watermark = engine.store().state().watermark;
            let retry = send(&mut rpc, &mut engine, &access(), create(2))
                .await
                .unwrap();
            assert_eq!(first["result"], retry["result"]);
            assert_eq!(retry["id"], 2);
            let mut changed = create(3);
            changed["params"]["new_session"] = json!("changed");
            assert_eq!(
                send(&mut rpc, &mut engine, &access(), changed)
                    .await
                    .unwrap()["error"]["data"]["details"]["code"],
                "COMMAND_CONFLICT"
            );
            let query = request(
                4,
                "command/read",
                json!({"scope":{"workspace":"workspace","session":"session"},"command_id":"create-once"}),
            );
            let receipt = send(&mut rpc, &mut engine, &access(), query.clone())
                .await
                .unwrap();
            assert_eq!(receipt["result"], first["result"]);
            assert!(receipt["result"]["value"].get("digest").is_none());
            let sessions = request(
                5,
                "session/list",
                json!({"scope":{"workspace":"workspace","session":"session"},"cursor":null,"limit":128}),
            );
            let page = send(&mut rpc, &mut engine, &access(), sessions)
                .await
                .unwrap();
            assert_eq!(
                page["result"]["value"]["sessions"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(
                page["result"]["value"]["sessions"][0]["scope"]["session"],
                "session"
            );
            assert_eq!(engine.store().state().watermark, watermark);
            engine.into_store().close().await.unwrap();
            let mut engine = Engine::new(Store::open(&root, backend, &[]).await.unwrap()).unwrap();
            let mut rpc = RpcSession::new(server()).unwrap();
            send(&mut rpc, &mut engine, &access(), init())
                .await
                .unwrap();
            assert_eq!(
                send(&mut rpc, &mut engine, &access(), create(8))
                    .await
                    .unwrap()["result"],
                first["result"]
            );
            assert_eq!(
                send(&mut rpc, &mut engine, &access(), query).await.unwrap()["result"],
                first["result"]
            );
            assert_eq!(engine.store().state().watermark, watermark);
            engine.into_store().close().await.unwrap();
        }
    }

    #[tokio::test]
    async fn notification_batch_capability_and_current_authority_gates_prevent_mutations() {
        let temp = tempfile::tempdir().unwrap();
        let mut engine = setup(temp.path(), BackendKind::Sqlite).await;
        let mut rpc = RpcSession::new(server()).unwrap();
        let mut notification = init();
        notification.as_object_mut().unwrap().remove("id");
        assert!(send(&mut rpc, &mut engine, &access(), notification)
            .await
            .is_none());
        assert_eq!(
            send(&mut rpc, &mut engine, &access(), create(1))
                .await
                .unwrap()["error"]["data"]["kind"],
            "not_initialized"
        );
        send(&mut rpc, &mut engine, &access(), init())
            .await
            .unwrap();
        let watermark = engine.store().state().watermark;
        let mut notification = create(1);
        notification.as_object_mut().unwrap().remove("id");
        assert!(send(
            &mut rpc,
            &mut engine,
            &access(),
            json!([notification.clone(), notification.clone()])
        )
        .await
        .is_none());
        let batch = send(
            &mut rpc,
            &mut engine,
            &access(),
            json!([notification, 7, {"jsonrpc":"2.0","id":7,"result":{}}]),
        )
        .await
        .unwrap();
        assert_eq!(batch.as_array().unwrap().len(), 2);
        assert_eq!(batch[0]["error"]["code"], -32600);
        assert_eq!(batch[1]["error"]["code"], -32600);
        let mut observer = access();
        observer.write = false;
        assert_eq!(
            send(&mut rpc, &mut engine, &observer, create(2))
                .await
                .unwrap()["error"]["data"]["details"]["code"],
            "POLICY_DENIED"
        );
        let mut revoked = access();
        revoked.authority = AuthorityRevision::new(1);
        assert_eq!(
            send(&mut rpc, &mut engine, &revoked, create(3))
                .await
                .unwrap()["error"]["data"]["details"]["code"],
            "POLICY_DENIED"
        );
        let mut changed_actor = access();
        changed_actor.actor = ActorId::new();
        assert_eq!(
            send(&mut rpc, &mut engine, &changed_actor, create(4))
                .await
                .unwrap()["error"]["data"]["details"]["code"],
            "POLICY_DENIED"
        );
        let mut rpc = RpcSession::new(server()).unwrap();
        let mut limited = init();
        limited["params"]["capabilities"] = json!(["session/read"]);
        send(&mut rpc, &mut engine, &access(), limited)
            .await
            .unwrap();
        assert_eq!(
            send(&mut rpc, &mut engine, &access(), create(5))
                .await
                .unwrap()["error"]["data"]["details"]["code"],
            "CAPABILITY_UNAVAILABLE"
        );
        assert_eq!(engine.store().state().watermark, watermark);
        engine.into_store().close().await.unwrap();
    }

    #[tokio::test]
    async fn unsupported_versions_required_capabilities_and_unknown_governance_fields_fail_closed()
    {
        let temp = tempfile::tempdir().unwrap();
        let mut engine = setup(temp.path(), BackendKind::Files).await;
        let mut invalid_server = server();
        invalid_server.methods.push("task/read".into());
        assert!(RpcSession::new(invalid_server).is_err());
        let mut invalid_server = server();
        invalid_server
            .capabilities
            .insert("background-owner".into());
        assert!(RpcSession::new(invalid_server).is_err());
        let mut rpc = RpcSession::new(server()).unwrap();
        let mut wrong = init();
        wrong["params"]["protocol_version"] = json!("2.0");
        assert_eq!(
            send(&mut rpc, &mut engine, &access(), wrong).await.unwrap()["error"]["data"]["kind"],
            "unsupported_version"
        );
        let mut wrong = init();
        wrong["params"]["required_capabilities"] = json!(["task/cancel"]);
        assert_eq!(
            send(&mut rpc, &mut engine, &access(), wrong).await.unwrap()["error"]["data"]["kind"],
            "unsupported_capability"
        );
        send(&mut rpc, &mut engine, &access(), init())
            .await
            .unwrap();
        let watermark = engine.store().state().watermark;
        assert_eq!(
            send(
                &mut rpc,
                &mut engine,
                &access(),
                request(10, "invented/method", json!({}))
            )
            .await
            .unwrap()["error"]["code"],
            -32601
        );
        assert_eq!(
            send(
                &mut rpc,
                &mut engine,
                &access(),
                request(11, "task/cancel", json!({}))
            )
            .await
            .unwrap()["error"]["data"]["details"]["code"],
            "CAPABILITY_UNAVAILABLE"
        );
        let mut wrong = create(1);
        wrong["params"]["grant_controller"] = json!(true);
        assert_eq!(
            send(&mut rpc, &mut engine, &access(), wrong).await.unwrap()["error"]["code"],
            -32602
        );
        let mut wrong = create(2);
        wrong["params"]["scope"]["session"] = json!("foreign");
        assert_eq!(
            send(&mut rpc, &mut engine, &access(), wrong).await.unwrap()["error"]["data"]
                ["details"]["code"],
            "POLICY_DENIED"
        );
        assert_eq!(engine.store().state().watermark, watermark);
        engine.into_store().close().await.unwrap();
    }

    #[tokio::test]
    async fn oversized_batches_close_without_replies_or_partial_execution() {
        for backend in [BackendKind::Sqlite, BackendKind::Files] {
            let temp = tempfile::tempdir().unwrap();
            let mut engine = setup(temp.path(), backend).await;
            let watermark = engine.store().state().watermark;
            for mixed in [false, true] {
                let mut rpc = RpcSession::new(server()).unwrap();
                send(&mut rpc, &mut engine, &access(), init())
                    .await
                    .unwrap();
                let entries = (0..65)
                    .map(|index| {
                        let mut value = create(index);
                        if !mixed || index % 2 == 0 {
                            value.as_object_mut().unwrap().remove("id");
                        }
                        value
                    })
                    .collect();
                assert!(
                    send(&mut rpc, &mut engine, &access(), Value::Array(entries))
                        .await
                        .is_none()
                );
                assert!(rpc.is_closed());
                assert_eq!(engine.store().state().watermark, watermark);
                assert!(send(&mut rpc, &mut engine, &access(), create(99))
                    .await
                    .is_none());
                assert_eq!(engine.store().state().watermark, watermark);
            }
            engine.into_store().close().await.unwrap();
        }
    }

    #[tokio::test]
    async fn response_overflow_closes_and_reconnect_reconciles_committed_mutation() {
        for backend in [BackendKind::Sqlite, BackendKind::Files] {
            let temp = tempfile::tempdir().unwrap();
            let mut engine = setup(temp.path(), backend).await;
            let mut configuration = server();
            configuration.limits.maximum_frame_bytes = 1536;
            let mut rpc = RpcSession::new(configuration).unwrap();
            let initialized = send(&mut rpc, &mut engine, &access(), init())
                .await
                .unwrap();
            assert!(initialized.get("result").is_some());
            let before = engine.store().state().watermark;
            let read = request(
                2,
                "command/read",
                json!({"scope":{"workspace":"workspace","session":"session"},"command_id":"create-once"}),
            );
            let batch = Value::Array(
                std::iter::once(create(1))
                    .chain((2..9).map(|request_id| {
                        let mut request = read.clone();
                        request["id"] = json!(request_id);
                        request
                    }))
                    .collect(),
            );
            assert!(serde_json::to_vec(&batch).unwrap().len() <= 1536);
            assert!(send(&mut rpc, &mut engine, &access(), batch)
                .await
                .is_none());
            assert!(rpc.is_closed());
            let committed = engine.store().state().watermark;
            assert!(committed > before);
            assert_eq!(engine.store().state().commands.len(), 2);
            assert!(send(&mut rpc, &mut engine, &access(), create(3))
                .await
                .is_none());
            let mut reconnected = RpcSession::new(server()).unwrap();
            send(&mut reconnected, &mut engine, &access(), init())
                .await
                .unwrap();
            let receipt = send(&mut reconnected, &mut engine, &access(), read)
                .await
                .unwrap();
            let retried = send(&mut reconnected, &mut engine, &access(), create(4))
                .await
                .unwrap();
            assert_eq!(receipt["result"], retried["result"]);
            assert_eq!(engine.store().state().watermark, committed);
            engine.into_store().close().await.unwrap();
        }
    }
}

#[cfg(test)]
#[path = "rpc_task_tests.rs"]
mod task_projection;
