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
#[derive(Clone, Debug, Serialize)]
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

impl<'de> Deserialize<'de> for EventPage {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let raw: Box<serde_json::value::RawValue> = Deserialize::deserialize(decoder)?;
        // A canonical store state is capped at 64 MiB. Leave room for the page
        // envelope and cursor without lowering any store-backed history limit.
        if raw.get().len() > 128 * 1024 * 1024 {
            return Err(serde::de::Error::custom("event page byte limit"));
        }
        // Raw dispatch avoids internal-tag buffering before EventInput.data's
        // literal-value decoder. Preserve EventPage's additive-field behavior.
        #[derive(Deserialize)]
        struct Tag {
            status: String,
        }
        #[derive(Deserialize)]
        struct Events {
            #[serde(rename = "status")]
            _status: String,
            events: Vec<EventEnvelope>,
            next_cursor: Cursor,
            snapshot_watermark: Watermark,
            at_end: bool,
        }
        #[derive(Deserialize)]
        struct Gap {
            #[serde(rename = "status")]
            _status: String,
            reason: GapReason,
            restart_from_snapshot: bool,
        }
        let tag: Tag = serde_json::from_str(raw.get()).map_err(serde::de::Error::custom)?;
        match tag.status.as_str() {
            "events" => {
                let v: Events =
                    serde_json::from_str(raw.get()).map_err(serde::de::Error::custom)?;
                Ok(Self::Events {
                    events: v.events,
                    next_cursor: v.next_cursor,
                    snapshot_watermark: v.snapshot_watermark,
                    at_end: v.at_end,
                })
            }
            "gap" => {
                let v: Gap = serde_json::from_str(raw.get()).map_err(serde::de::Error::custom)?;
                Ok(Self::Gap {
                    reason: v.reason,
                    restart_from_snapshot: v.restart_from_snapshot,
                })
            }
            _ => Err(serde::de::Error::custom("unknown event page status")),
        }
    }
}
