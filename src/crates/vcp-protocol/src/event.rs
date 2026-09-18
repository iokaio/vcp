// SPDX-License-Identifier: Apache-2.0
use serde::{Deserialize, Serialize};
use vcp_domain::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    WorkspaceBound,
    SessionStarted,
    TaskCreated,
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
    pub data: serde_json::Value,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventEnvelope {
    pub version: u32,
    pub sequence: SessionSeq,
    pub watermark: Watermark,
    pub event: EventInput,
}
