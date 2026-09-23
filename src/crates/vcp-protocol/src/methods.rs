// SPDX-License-Identifier: Apache-2.0
//! Canonical public DTOs. Defining a method never advertises its runtime capability.
//! The transport authenticates before decoding/admitting scoped operations.
use serde::{Deserialize, Serialize};

pub const MAX_METHOD_BYTES: usize = 256 * 1024;
pub const MAX_PAGE: u32 = 128;
pub const MAX_RANGE: u32 = 64 * 1024;

/// IDs are references, never authorization. Match the internal opaque-ID alphabet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Id(String);
impl TryFrom<String> for Id {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty()
            || value.len() > 96
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        {
            return Err("invalid opaque ID");
        }
        Ok(Self(value))
    }
}
impl From<Id> for String {
    fn from(value: Id) -> Self {
        value.0
    }
}
impl Id {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Exact unsigned 64-bit quantities use canonical decimal strings on the wire.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Counter(String);
impl TryFrom<String> for Counter {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let number = value.parse::<u64>().map_err(|_| "invalid counter")?;
        if number.to_string() != value {
            return Err("noncanonical counter");
        }
        Ok(Self(value))
    }
}
impl From<Counter> for String {
    fn from(value: Counter) -> Self {
        value.0
    }
}
impl From<u64> for Counter {
    fn from(value: u64) -> Self {
        Self(value.to_string())
    }
}
impl Counter {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(feature = "schema")]
fn string_schema(pattern: String, max: u32) -> schemars::schema::Schema {
    use schemars::schema::{InstanceType, SchemaObject, StringValidation};
    SchemaObject {
        instance_type: Some(InstanceType::String.into()),
        string: Some(Box::new(StringValidation {
            pattern: Some(pattern),
            min_length: Some(1),
            max_length: Some(max),
            ..Default::default()
        })),
        ..Default::default()
    }
    .into()
}
#[cfg(feature = "schema")]
impl schemars::JsonSchema for Id {
    fn schema_name() -> String {
        "Id".into()
    }
    fn json_schema(_: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        string_schema("^[a-zA-Z0-9_-]{1,96}(?![\\s\\S])".into(), 96)
    }
}
#[cfg(feature = "schema")]
impl schemars::JsonSchema for Counter {
    fn schema_name() -> String {
        "Counter".into()
    }
    fn json_schema(_: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        // Exact u64 range even for schema readers that cannot compare decimal
        // strings numerically. Each branch first differs below u64::MAX.
        let maximum = u64::MAX.to_string();
        let mut alternatives = vec!["0".to_string(), "[1-9][0-9]{0,18}".to_string()];
        for (index, digit) in maximum.bytes().enumerate() {
            let minimum = if index == 0 { b'1' } else { b'0' };
            if digit > minimum {
                alternatives.push(format!(
                    "{}[{}-{}][0-9]{{{}}}",
                    &maximum[..index],
                    minimum as char,
                    (digit - 1) as char,
                    maximum.len() - index - 1
                ));
            }
        }
        alternatives.push(maximum);
        string_schema(format!("^({})(?![\\s\\S])", alternatives.join("|")), 20)
    }
}

// Every DTO rejects unknown governance fields. Presentation-only additions belong
// in negotiated result schemas, not unvalidated request dictionaries.
macro_rules! dto {
    ($name:ident { $($(#[$attribute:meta])* $field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        #[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
        pub struct $name { $($(#[$attribute])* pub $field: $ty),* }
    };
}
macro_rules! enumeration {
    ($name:ident { $($variant:ident),* $(,)? }) => {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        #[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
        pub enum $name { $($variant),* }
    };
}
dto!(Scope {
    workspace: Id,
    session: Id
});
// Stable across reconnect. Controller generation is transport authority and is
// deliberately not part of durable mutation identity or supplied host facts.
dto!(Mutation {
    command_id: Id,
    expected_revision: Counter,
    steering_revision: Counter
});
// Controller authority is authenticated host state, never a caller-supplied
// actor, connection ID, lease token, or steering revision.
dto!(ControllerRead { scope: Scope });
dto!(ControllerAcquire { scope: Scope, command_id: Id, expected_revision: Option<Counter> });
dto!(ControllerRelease {
    scope: Scope,
    command_id: Id,
    expected_revision: Counter,
    generation: Counter
});
dto!(ControllerRecover {
    scope: Scope,
    command_id: Id,
    expected_revision: Counter,
    generation: Counter
});

dto!(WorkspaceOpen {
    command_id: Id,
    host: Id,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 32768)))]
    root: String
});
dto!(SessionCreate {
    scope: Scope,
    mutation: Mutation,
    new_session: Id,
    configuration_revision: Counter
});
dto!(SessionRead { scope: Scope });
dto!(SessionSnapshotRead { scope: Scope, #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 128)))] limit: u32, #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))] cursor: Option<String> });
dto!(SessionList { scope: Scope, #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))] cursor: Option<String>, #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 128)))] limit: u32 });
dto!(SessionResume {
    scope: Scope,
    mutation: Mutation,
    task: Id
});
dto!(SessionFork {
    scope: Scope,
    mutation: Mutation,
    new_session: Id,
    new_task: Id,
    through_turn: Id
});
dto!(TaskRead {
    scope: Scope,
    task: Id
});
dto!(TaskCancel {
    scope: Scope,
    mutation: Mutation,
    task: Id,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))]
    reason: String
});
dto!(Budget {
    cap_micros: Counter,
    currency: Currency,
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 1024)))]
    max_requests: u32,
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 86400)))]
    deadline_seconds: u32
});
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Currency {
    #[serde(rename = "USD")]
    Usd,
}
dto!(TurnStart { scope: Scope, mutation: Mutation, task: Id, turn: Id, #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 65536)))] objective: String, #[cfg_attr(feature = "schema", schemars(length(max = 64)))] constraints: Vec<String>, #[cfg_attr(feature = "schema", schemars(length(max = 64)))] acceptance: Vec<String>, budget: Budget });
dto!(TurnSteer { scope: Scope, mutation: Mutation, task: Id, turn: Id, #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 65536)))] objective: String, #[cfg_attr(feature = "schema", schemars(length(max = 64)))] constraints: Vec<String>, #[cfg_attr(feature = "schema", schemars(length(max = 64)))] acceptance: Vec<String> });
dto!(TurnControl {
    scope: Scope,
    mutation: Mutation,
    task: Id,
    turn: Id,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))]
    reason: String
});
dto!(ApprovalRespond {
    scope: Scope,
    mutation: Mutation,
    task: Id,
    approval: Id,
    #[cfg_attr(
        feature = "schema",
        schemars(regex(pattern = "^[0-9a-f]{64}(?![\\s\\S])"))
    )]
    operation_digest: String,
    effect_revision: Counter,
    policy_revision: Counter,
    decision: ApprovalDecision
});
enumeration!(ApprovalDecision { Allow, Deny });
dto!(EventsSubscribe {
    scope: Scope,
    after_sequence: Counter,
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 128)))]
    limit: u32
});
dto!(EventsNext {
    scope: Scope,
    subscription: Id,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))]
    cursor: String
});
dto!(EventsUnsubscribe {
    scope: Scope,
    subscription: Id
});
dto!(ArtifactRead {
    scope: Scope,
    task: Id,
    artifact: Id,
    offset: Counter,
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 65536)))]
    length: u32
});
dto!(DiffRead {
    scope: Scope,
    task: Id,
    change: Id,
    offset: Counter,
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 65536)))]
    length: u32
});
dto!(Inspect { scope: Scope, task: Id, target: Option<Id>, #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))] cursor: Option<String>, #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 128)))] limit: u32 });
dto!(MemoryQuery {
    scope: Scope,
    task: Id,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 16384)))]
    query: String,
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 128)))]
    limit: u32
});
dto!(MemoryInspect { scope: Scope, task: Id, claim: Id, version: Option<Id> });
dto!(MemoryPropose { scope: Scope, mutation: Mutation, task: Id, #[cfg_attr(feature = "schema", schemars(length(max = 65536)))] content: String, #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 128)))] evidence: Vec<Id> });
dto!(MemoryResolve {
    scope: Scope,
    mutation: Mutation,
    task: Id,
    proposal: Id,
    decision: MemoryDecision
});
enumeration!(MemoryDecision { Accept, Reject });
dto!(MemoryForget {
    scope: Scope,
    mutation: Mutation,
    task: Id,
    preview: Id,
    #[cfg_attr(
        feature = "schema",
        schemars(regex(pattern = "^[0-9a-f]{64}(?![\\s\\S])"))
    )]
    preview_digest: String
});
dto!(EditorContext { scope: Scope, mutation: Mutation, task: Id, #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 16)))] documents: Vec<DocumentObservation> });
dto!(DocumentObservation { host: Id, #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 32768)))] uri: String, version: Counter, #[cfg_attr(feature = "schema", schemars(regex(pattern = "^[0-9a-f]{64}(?![\\s\\S])")))] content_sha256: String, dirty: bool, #[cfg_attr(feature = "schema", schemars(length(max = 65536)))] content: Option<String>, #[cfg_attr(feature = "schema", schemars(regex(pattern = "^[0-9a-f]{64}(?![\\s\\S])")))] disk_sha256: Option<String> });
dto!(EditorChangeResult { scope: Scope, mutation: Mutation, task: Id, change: Id, outcome: EditorOutcome, #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 16)))] documents: Vec<DocumentObservation> });
enumeration!(EditorOutcome {
    Applied,
    Rejected,
    Partial,
    Unknown
});
dto!(SessionExport { scope: Scope, mutation: Mutation, task: Option<Id>, capture: CaptureScope });
enumeration!(CaptureScope {
    VisibleHistory,
    VisibleHistoryAndArtifacts
});
dto!(CommandRead {
    scope: Scope,
    command_id: Id
});

// The registry and tagged union share one definition, so negotiation cannot
// recognize a method that has no canonical parameter schema (or vice versa).
macro_rules! calls {
    ($($variant:ident($params:ty) => $method:literal),* $(,)?) => {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(tag = "method", content = "params", deny_unknown_fields)]
        #[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
        pub enum Call { $(#[serde(rename = $method)] $variant($params)),* }
        impl Call { pub const METHODS: &'static [&'static str] = &[$($method),*]; }
    };
}
calls! {
    ControllerRead(ControllerRead) => "controller/read",
    ControllerAcquire(ControllerAcquire) => "controller/acquire",
    ControllerRelease(ControllerRelease) => "controller/release",
    ControllerRecover(ControllerRecover) => "controller/recover",
    WorkspaceOpen(WorkspaceOpen) => "workspace/open",
    SessionCreate(SessionCreate) => "session/create",
    SessionRead(SessionRead) => "session/read",
    SessionSnapshot(SessionSnapshotRead) => "session/snapshot",
    SessionList(SessionList) => "session/list",
    SessionResume(SessionResume) => "session/resume",
    SessionFork(SessionFork) => "session/fork",
    TaskRead(TaskRead) => "task/read",
    TaskCancel(TaskCancel) => "task/cancel",
    TurnStart(TurnStart) => "turn/start",
    TurnSteer(TurnSteer) => "turn/steer",
    TurnPause(TurnControl) => "turn/pause",
    TurnCancel(TurnControl) => "turn/cancel",
    ApprovalRespond(ApprovalRespond) => "approval/respond",
    EventsSubscribe(EventsSubscribe) => "events/subscribe",
    EventsNext(EventsNext) => "events/next",
    EventsUnsubscribe(EventsUnsubscribe) => "events/unsubscribe",
    ArtifactRead(ArtifactRead) => "artifact/read",
    DiffRead(DiffRead) => "diff/read",
    ContextInspect(Inspect) => "context/inspect",
    RoutingExplain(Inspect) => "routing/explain",
    UsageRead(Inspect) => "usage/read",
    MemoryQuery(MemoryQuery) => "memory/query",
    MemoryInspect(MemoryInspect) => "memory/inspect",
    MemoryPropose(crate::memory_governance::ProposeParams) => "memory/propose",
    MemoryResolve(crate::memory_governance::ResolveParams) => "memory/resolve",
    MemoryReview(crate::memory_governance::ReviewRead) => "memory/review",
    MemoryForget(MemoryForget) => "memory/forget",
    MemoryForgetPreview(crate::memory_retention::PreviewRequest) => "memory/forgetPreview",
    MemoryForgetPreviewRead(crate::memory_retention::PreviewPageRequest) => "memory/forgetPreviewRead",
    MemoryForgetRead(crate::memory_retention::JobRead) => "memory/forgetRead",
    EditorContext(EditorContext) => "editor/context",
    EditorChangeResult(EditorChangeResult) => "editor/changeResult",
    SessionExport(SessionExport) => "session/export",
    CommandRead(CommandRead) => "command/read",
}

impl Call {
    /// Decode only after envelope/init/current authentication checks. Bounds
    /// complement strict serde shape checks; no deserialized flag grants access.
    pub fn decode(method: &str, params: serde_json::Value) -> Result<Self, &'static str> {
        let encoded = serde_json::to_vec(&params).map_err(|_| "invalid params")?;
        if encoded.len() > MAX_METHOD_BYTES {
            return Err("method byte limit");
        }
        let call: Self =
            serde_json::from_value(serde_json::json!({"method":method,"params":params}))
                .map_err(|_| "invalid method parameters")?;
        call.validate()?;
        Ok(call)
    }

    pub fn mutation(&self) -> Option<&Mutation> {
        match self {
            Self::SessionCreate(p) => Some(&p.mutation),
            Self::SessionResume(p) => Some(&p.mutation),
            Self::SessionFork(p) => Some(&p.mutation),
            Self::TaskCancel(p) => Some(&p.mutation),
            Self::TurnStart(p) => Some(&p.mutation),
            Self::TurnSteer(p) => Some(&p.mutation),
            Self::TurnPause(p) | Self::TurnCancel(p) => Some(&p.mutation),
            Self::ApprovalRespond(p) => Some(&p.mutation),
            Self::MemoryPropose(p) => Some(p.mutation()),
            Self::MemoryResolve(p) => Some(p.mutation()),
            Self::MemoryForget(p) => Some(&p.mutation),
            Self::EditorContext(p) => Some(&p.mutation),
            Self::EditorChangeResult(p) => Some(&p.mutation),
            Self::SessionExport(p) => Some(&p.mutation),
            _ => None,
        }
    }
    pub fn is_mutation(&self) -> bool {
        self.command_id().is_some()
    }

    /// Durable operation identity, including host controller operations whose
    /// revision contract has no task steering counter.
    pub fn command_id(&self) -> Option<&Id> {
        match self {
            Self::WorkspaceOpen(p) => Some(&p.command_id),
            Self::ControllerAcquire(p) => Some(&p.command_id),
            Self::ControllerRelease(p) => Some(&p.command_id),
            Self::ControllerRecover(p) => Some(&p.command_id),
            _ => self.mutation().map(|mutation| &mutation.command_id),
        }
    }

    /// Bind the authenticated principal, exact version, scope and semantics;
    /// transport request IDs and reconnecting controller epochs are excluded.
    pub fn digest(&self, authenticated_actor: &str) -> Result<String, serde_json::Error> {
        Ok(crate::digest_bytes(&crate::canonical_bytes(
            &serde_json::json!({
                "protocol":"vcp-public/1.0", "actor":authenticated_actor, "call":self
            }),
        )?))
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        fn text(value: &str, cap: usize) -> Result<(), &'static str> {
            if value.trim().is_empty() || value.len() > cap || value.contains('\0') {
                Err("text bound")
            } else {
                Ok(())
            }
        }
        fn page(limit: u32) -> Result<(), &'static str> {
            if limit == 0 || limit > MAX_PAGE {
                Err("page limit")
            } else {
                Ok(())
            }
        }
        fn digest(value: &str) -> Result<(), &'static str> {
            if value.len() != 64
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                Err("SHA-256 required")
            } else {
                Ok(())
            }
        }
        fn objective(
            value: &str,
            constraints: &[String],
            acceptance: &[String],
        ) -> Result<(), &'static str> {
            text(value, 65536)?;
            if constraints.len() > 64 || acceptance.len() > 64 {
                return Err("objective list limit");
            }
            for v in constraints.iter().chain(acceptance) {
                text(v, 4096)?;
            }
            Ok(())
        }
        fn documents(values: &[DocumentObservation]) -> Result<(), &'static str> {
            if values.is_empty() || values.len() > 16 {
                return Err("document limit");
            }
            for v in values {
                text(&v.uri, 32768)?;
                digest(&v.content_sha256)?;
                if let Some(hash) = &v.disk_sha256 {
                    digest(hash)?;
                }
                if let Some(content) = &v.content {
                    if content.len() > 65536
                        || crate::digest_bytes(content.as_bytes()) != v.content_sha256
                    {
                        return Err("document content mismatch");
                    }
                }
            }
            Ok(())
        }
        if crate::canonical_bytes(self)
            .map_err(|_| "invalid call")?
            .len()
            > MAX_METHOD_BYTES
        {
            return Err("method byte limit");
        }
        match self {
            Self::ControllerRelease(p) if p.generation.as_str() == "0" => {
                Err("controller generation must be positive")
            }
            Self::ControllerRecover(p) if p.generation.as_str() == "0" => {
                Err("controller generation must be positive")
            }
            Self::WorkspaceOpen(p) => text(&p.root, 32768),
            Self::SessionSnapshot(p) => {
                page(p.limit)?;
                if let Some(cursor) = &p.cursor {
                    text(cursor, 4096)?;
                }
                Ok(())
            }
            Self::SessionList(p) => {
                page(p.limit)?;
                if let Some(c) = &p.cursor {
                    text(c, 4096)?;
                }
                Ok(())
            }
            Self::TaskCancel(p) => text(&p.reason, 4096),
            Self::TurnPause(p) | Self::TurnCancel(p) => text(&p.reason, 4096),
            Self::TurnStart(p) => {
                objective(&p.objective, &p.constraints, &p.acceptance)?;
                if p.budget.cap_micros.as_str() == "0"
                    || p.budget.max_requests == 0
                    || p.budget.max_requests > 1024
                    || p.budget.deadline_seconds == 0
                    || p.budget.deadline_seconds > 86400
                {
                    return Err("budget bound");
                }
                Ok(())
            }
            Self::TurnSteer(p) => objective(&p.objective, &p.constraints, &p.acceptance),
            Self::ApprovalRespond(p) => digest(&p.operation_digest),
            Self::EventsSubscribe(p) => page(p.limit),
            Self::EventsNext(p) => text(&p.cursor, 4096),
            Self::ArtifactRead(p) => {
                if p.length == 0 || p.length > MAX_RANGE {
                    Err("artifact range limit")
                } else {
                    Ok(())
                }
            }
            Self::DiffRead(p) => {
                if p.length == 0 || p.length > MAX_RANGE {
                    Err("diff range limit")
                } else {
                    Ok(())
                }
            }
            Self::ContextInspect(p) | Self::RoutingExplain(p) | Self::UsageRead(p) => {
                page(p.limit)?;
                if let Some(c) = &p.cursor {
                    text(c, 4096)?;
                }
                Ok(())
            }
            Self::MemoryQuery(p) => {
                page(p.limit)?;
                text(&p.query, 16384)
            }
            Self::MemoryPropose(p) => p.validate(),
            Self::MemoryResolve(p) => p.validate(),
            Self::MemoryForget(p) => digest(&p.preview_digest),
            Self::MemoryForgetPreview(p) => p.validate(),
            Self::MemoryForgetPreviewRead(p) => p.validate(),
            Self::EditorContext(p) => documents(&p.documents),
            Self::EditorChangeResult(p) => documents(&p.documents),
            _ => Ok(()),
        }
    }
}

enumeration!(TaskStatus {
    Pending,
    Running,
    WaitingForInput,
    Blocked,
    Paused,
    Completed,
    Failed,
    Cancelled
});
enumeration!(EffectStatus {
    Pending,
    Known,
    Partial,
    Unknown
});
enumeration!(InputKind {
    Approval,
    Question,
    Reconciliation
});
enumeration!(Trust { Untrusted, Trusted });
dto!(PendingInput {
    id: Id,
    kind: InputKind,
    revision: Counter,
    operation_digest: Option<String>,
    // Present on the wire only with negotiated approval/source-revisions/1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    effect_revision: Option<Counter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    policy_revision: Option<Counter>
});
// Relative to the authenticated connection, never a reusable ownership grant.
enumeration!(ControllerOwnership {
    Unclaimed,
    ThisConnection,
    OtherConnection,
    PreviousProcess,
    Released
});
dto!(ControllerView { scope: Scope, revision: Option<Counter>, generation: Counter, ownership: ControllerOwnership, watermark: Counter });
dto!(TaskView { scope: Scope, task: Id, root: Id, parent: Option<Id>, turn: Option<Id>, revision: Counter, steering_revision: Counter, state: TaskStatus, #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))] reason: String, pending_inputs: Vec<PendingInput>, effects: EffectStatus });
dto!(SessionView { scope: Scope, revision: Counter, configuration_revision: Counter, fork_origin: Option<Id>, fork_through: Option<Id> });
// A completed page sequence describes one canonical boundary. The replay cursor
// starts after that boundary; it is not a grant or a live producer subscription.
dto!(SessionSnapshot {
    session: SessionView,
    sequence: Counter,
    watermark: Counter,
    subscription: Id,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))]
    event_cursor: String,
    #[cfg_attr(feature = "schema", schemars(length(max = 128)))]
    tasks: Vec<TaskView>,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))]
    next_cursor: Option<String>,
    complete: bool
});
dto!(WorkspaceView {
    workspace: Id,
    host: Id,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    root_id: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    binding_revision: Option<Counter>,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 32768)))]
    root: String,
    trust: Trust,
    revision: Counter,
    authority_revision: Counter
});
dto!(Acceptance { command_id: Id, scope: Scope, task: Option<Id>, turn: Option<Id>, revision: Counter, watermark: Counter, outcome: OperationOutcome });
enumeration!(OperationOutcome {
    Accepted,
    WaitingForInput,
    Partial,
    Unknown,
    Completed,
    Cancelled,
    Failed
});
dto!(UsageView {
    scope: Scope,
    task: Id,
    root: Id,
    currency: Currency,
    cap_micros: Counter,
    settled_micros: Counter,
    reserved_micros: Counter,
    unresolved_micros: Counter,
    overrun: bool
});
dto!(ArtifactRange {
    artifact: Id,
    offset: Counter,
    total_bytes: Counter,
    encoding: ArtifactEncoding,
    #[cfg_attr(feature = "schema", schemars(length(max = 65536)))]
    content: String,
    complete: bool,
    sha256: String
});
enumeration!(ArtifactEncoding { Utf8, Base64 });
dto!(SessionPage { watermark: Counter, sessions: Vec<SessionView>, #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))] next_cursor: Option<String> });
dto!(EvidenceReference {
    artifact: Id,
    offset: Counter,
    length: Counter,
    sha256: String
});
dto!(EvidenceRow {
    id: Id,
    schema: String,
    revision: Counter,
    content: EvidenceReference
});
dto!(EvidencePage { scope: Scope, task: Id, watermark: Counter, rows: Vec<EvidenceRow>, #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))] next_cursor: Option<String>, complete: bool });
dto!(MemoryFinding {
    claim: Id,
    version: Id,
    evidence: Vec<EvidenceReference>,
    #[cfg_attr(feature = "schema", schemars(length(max = 65536)))]
    content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    state: Option<crate::memory::InspectionState>
});
dto!(MemoryPage { scope: Scope, task: Id, generation: Option<Id>, sequence: Counter, findings: Vec<MemoryFinding>, complete: bool });
// Invalidation/evidence metadata, never a raw internal fact or complete reducer.
dto!(Event { id: Id, scope: Scope, sequence: Counter, schema_version: String, timestamp_ms: Counter, kind: String, command_id: Option<Id>, task: Option<Id>, outcome: Option<OperationOutcome>, redacted: bool, evidence_complete: bool, #[cfg_attr(feature = "schema", schemars(length(max = 128)))] evidence: Vec<EvidenceReference> });
enumeration!(GapReason {
    RetentionChanged,
    AuthorityChanged,
    CursorExpired,
    SequenceUnavailable,
    SlowConsumer
});
dto!(EventBatch { subscription: Id, snapshot_sequence: Counter, #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))] cursor: String, #[cfg_attr(feature = "schema", schemars(length(max = 128)))] events: Vec<Event>, at_end: bool });
dto!(EventGap {
    subscription: Id,
    reason: GapReason,
    snapshot_sequence: Counter,
    resubscribe_required: bool
});
dto!(ExportView {
    scope: Scope,
    artifact: Id,
    visibility_manifest: Id,
    complete: bool
});

/// Typed outcomes never reinterpret pending/partial/unknown as success.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    deny_unknown_fields,
    rename_all = "snake_case"
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ResultValue {
    Snapshot(SessionSnapshot),
    Controller(ControllerView),
    Workspace(WorkspaceView),
    Session(SessionView),
    Sessions(SessionPage),
    Task(TaskView),
    Acceptance(Acceptance),
    Usage(UsageView),
    Artifact(ArtifactRange),
    Evidence(EvidencePage),
    Memory(MemoryPage),
    MemoryQuery(crate::memory_query::Page),
    MemoryReview(crate::memory_governance::ReviewView),
    MemoryReviewed(crate::memory_governance::ReviewResult),
    RetentionPreview(crate::memory_retention::PreviewPage),
    Retention(crate::memory_retention::JobView),
    Forgotten(crate::memory_retention::ForgetResult),
    Events(EventBatch),
    Gap(EventGap),
    Export(ExportView),
    Unsubscribed { subscription: Id },
}

/// Schema root bundles canonical definitions; this object is never a wire frame.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PublicApi {
    pub call: Call,
    pub result: ResultValue,
    pub initialize: crate::handshake::InitializeParams,
    pub initialized: crate::handshake::InitializeResult,
    pub error: crate::jsonrpc::RpcError,
    pub application_error: crate::errors::ApplicationError,
    pub envelope: WireFrame,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum JsonRpcVersion {
    #[serde(rename = "2.0")]
    V2,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum WireParams {
    Object(std::collections::BTreeMap<String, serde_json::Value>),
    Array(Vec<serde_json::Value>),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RequestEnvelope {
    pub jsonrpc: JsonRpcVersion,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    #[cfg_attr(feature = "schema", schemars(with = "crate::jsonrpc::RequestId"))]
    pub id: Option<crate::jsonrpc::RequestId>,
    pub method: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    #[cfg_attr(feature = "schema", schemars(with = "WireParams"))]
    pub params: Option<WireParams>,
}
fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    decoder: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(decoder).map(Some)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ResultEnvelope {
    pub jsonrpc: JsonRpcVersion,
    pub id: crate::jsonrpc::RequestId,
    pub result: serde_json::Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ErrorEnvelope {
    pub jsonrpc: JsonRpcVersion,
    pub id: crate::jsonrpc::RequestId,
    pub error: crate::jsonrpc::RpcError,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum WireEnvelope {
    Request(RequestEnvelope),
    Result(ResultEnvelope),
    Error(ErrorEnvelope),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum WireFrame {
    Single(WireEnvelope),
    Batch(#[cfg_attr(feature = "schema", schemars(length(min = 1, max = 64)))] Vec<WireEnvelope>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn params() -> serde_json::Value {
        json!({"scope":{"workspace":"ws","session":"s"},"mutation":{"command_id":"cmd","expected_revision":"18446744073709551615","steering_revision":"0"},"new_session":"next","configuration_revision":"0"})
    }
    #[test]
    fn controller_params_have_exact_revision_identity_and_no_wire_authority() {
        let scope = json!({"workspace":"ws","session":"s"});
        let acquire = json!({"scope":scope,"command_id":"acquire","expected_revision":null});
        let first = Call::decode("controller/acquire", acquire.clone()).unwrap();
        assert!(first.is_mutation());
        assert_eq!(first.command_id().unwrap().as_str(), "acquire");
        assert!(first.mutation().is_none());
        let read = Call::decode("controller/read", json!({"scope":scope})).unwrap();
        assert!(!read.is_mutation());
        assert!(read.command_id().is_none());
        for field in ["actor", "connection", "token", "write", "steering_revision"] {
            let mut injected = acquire.clone();
            injected[field] = json!("forged");
            assert!(Call::decode("controller/acquire", injected).is_err());
        }
        let mut changed = acquire;
        changed["expected_revision"] = json!("0");
        assert_ne!(
            first.digest("actor").unwrap(),
            Call::decode("controller/acquire", changed)
                .unwrap()
                .digest("actor")
                .unwrap()
        );
        for method in ["controller/release", "controller/recover"] {
            let params = json!({"scope":scope,"command_id":"control","expected_revision":"18446744073709551615","generation":"9223372036854775808"});
            let call = Call::decode(method, params.clone()).unwrap();
            assert!(call.is_mutation());
            assert_eq!(call.command_id().unwrap().as_str(), "control");
            for bad in [
                json!(0),
                json!("0"),
                json!("01"),
                json!("18446744073709551616"),
            ] {
                let mut invalid = params.clone();
                invalid["generation"] = bad;
                assert!(Call::decode(method, invalid).is_err());
            }
            let mut missing = params;
            missing.as_object_mut().unwrap().remove("expected_revision");
            assert!(Call::decode(method, missing).is_err());
        }
        assert!(serde_json::from_value::<ControllerOwnership>(json!("future_owner")).is_err());
    }

    #[test]
    fn schema_envelope_roundtrip_preserves_present_null_id_and_rejects_null_params() {
        for value in [
            json!({"jsonrpc":"2.0","method":"x","id":null}),
            json!({"jsonrpc":"2.0","method":"x"}),
        ] {
            let decoded: RequestEnvelope = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), value);
        }
        assert!(serde_json::from_value::<RequestEnvelope>(
            json!({"jsonrpc":"2.0","method":"x","params":null})
        )
        .is_err());
    }
    #[test]
    fn exact_counters_unknown_authority_and_enum_values_fail_closed() {
        let p = params();
        let call = Call::decode("session/create", p.clone()).unwrap();
        assert_eq!(serde_json::to_value(call).unwrap()["params"], p);
        for bad in [
            json!(9007199254740992u64),
            json!("01"),
            json!("18446744073709551616"),
            json!("+1"),
            json!("-1"),
        ] {
            let mut p = params();
            p["mutation"]["expected_revision"] = bad;
            assert!(Call::decode("session/create", p).is_err());
        }
        let mut p = params();
        p["trusted"] = json!(true);
        assert!(Call::decode("session/create", p).is_err());
        assert!(serde_json::from_value::<OperationOutcome>(json!("future_success")).is_err());
        assert!(serde_json::from_value::<Id>(json!("../foreign")).is_err());
    }
    #[test]
    fn identity_is_canonical_and_binds_actor_scope_and_payload() {
        let a = Call::decode("session/create", params()).unwrap();
        let b = Call::decode(
            "session/create",
            serde_json::from_str(&serde_json::to_string(&params()).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(a.digest("owner").unwrap(), b.digest("owner").unwrap());
        assert_ne!(a.digest("owner").unwrap(), b.digest("observer").unwrap());
        let mut changed = params();
        changed["new_session"] = json!("different");
        assert_ne!(
            a.digest("owner").unwrap(),
            Call::decode("session/create", changed)
                .unwrap()
                .digest("owner")
                .unwrap()
        );
    }
    #[test]
    fn ranges_pages_and_content_are_bounded() {
        let p = json!({"scope":{"workspace":"ws","session":"s"},"task":"t","artifact":"a","offset":"9007199254740993","length":65536});
        assert!(Call::decode("artifact/read", p.clone()).is_ok());
        let mut too_large = p;
        too_large["length"] = json!(65537);
        assert!(Call::decode("artifact/read", too_large).is_err());
        assert!(Call::decode(
            "session/list",
            json!({"scope":{"workspace":"ws","session":"s"},"limit":0,"cursor":null})
        )
        .is_err());
        let p = json!({"command_id":"c","host":"h","root":"x".repeat(MAX_METHOD_BYTES)});
        assert!(Call::decode("workspace/open", p).is_err());
    }
}
