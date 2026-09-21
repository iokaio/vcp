// SPDX-License-Identifier: Apache-2.0
use serde::{Deserialize, Serialize};
use vcp_domain::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    WorkspaceBound,
    SessionStarted,
    TaskCreated,
    ChildGraphChanged,
    TaskTransition,
    ObjectiveChanged,
    FingerprintObserved,
    TurnTransition,
    EffectTransition,
    VerificationRecorded,
    ArtifactAttached,
    ApprovalRequested,
    ApprovalResolved,
    CommandInspected,
    ReservationCreated,
    AttemptSubmitted,
    UsageReconciled,
    ReservationReleased,
    LiabilityRetained,
    AccountingResolved,
    LocalResourcesObserved,
    MemoryResolved,
    AccessChanged,
    RetentionChanged,
    Commentary,
    Diagnostic,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventInput {
    pub id: EventId,
    pub workspace: WorkspaceId,
    pub session: SessionId,
    pub task: Option<TaskId>,
    pub actor: ActorId,
    pub correlation: CommandId,
    pub causation: Option<EventId>,
    pub timestamp: Timestamp,
    pub kind: EventKind,
    pub artifacts: Vec<ArtifactId>,
    /// Versioned fact data. Diagnostics are explicitly marked, never commentary.
    #[serde(deserialize_with = "crate::persisted_json::deserialize")]
    pub data: serde_json::Value,
    /// Optional additive metadata preserves the exact canonical bytes of older
    /// version-1 events when absent. It is filter data, never authority.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<EventMetadata>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventMetadata {
    pub agent: Option<AgentId>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub paths: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventEnvelope {
    pub version: u32,
    pub sequence: SessionSeq,
    pub watermark: Watermark,
    pub event: EventInput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redaction: Option<vcp_domain::redaction::ContentRedaction>,
}
