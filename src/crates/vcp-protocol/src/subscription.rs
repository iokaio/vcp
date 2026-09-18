// SPDX-License-Identifier: Apache-2.0
use crate::event::EventEnvelope;
use serde::{Deserialize, Serialize};
use vcp_domain::{ids::*, revision::*};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    pub version: u32,
    pub snapshot: SnapshotId,
    pub workspace: WorkspaceId,
    pub session: SessionId,
    pub after: SessionSeq,
    pub end: SessionSeq,
    pub watermark: Watermark,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
    pub expires_at: Timestamp,
    pub limit: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EventPage {
    Events {
        events: Vec<EventEnvelope>,
        next_cursor: Cursor,
        snapshot_watermark: Watermark,
        at_end: bool,
    },
    Gap {
        reason: GapReason,
        restart_from_snapshot: bool,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapReason {
    SnapshotExpired,
    ScopeChanged,
    RetentionChanged,
    SequenceUnavailable,
    CursorChanged,
}
