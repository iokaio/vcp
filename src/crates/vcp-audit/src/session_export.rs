// SPDX-License-Identifier: Apache-2.0
//! Bounded metadata history, optionally with retained artifact bytes. Arbitrary
//! event data/fact payloads are omitted, not claimed to have been sanitized.
use crate::{
    history::{Access, History},
    Error, Result,
};
use serde::Serialize;
use vcp_domain::{
    artifact::{ArtifactDescriptor, CaptureState},
    *,
};
use vcp_protocol::{event::EventKind, methods::CaptureScope};
use vcp_store::{
    export_contract::{Sources, MAX_BYTES},
    Store,
};

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct HistoryEntry {
    id: EventId,
    sequence: SessionSeq,
    watermark: Watermark,
    task: Option<TaskId>,
    kind: EventKind,
    timestamp: Timestamp,
    actor: ActorId,
    correlation: CommandId,
    causation: Option<EventId>,
    artifacts: Vec<ArtifactId>,
    payload_omitted: bool,
}
#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct RetainedArtifact {
    descriptor: ArtifactDescriptor,
    /// JSON octets, not decoded text. No secret-content filtering is implied.
    bytes: Vec<u8>,
}
#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    schema: &'static str,
    history_profile: &'static str,
    sources: Sources,
    history: Vec<HistoryEntry>,
    artifacts: Vec<RetainedArtifact>,
    omissions: Vec<String>,
}

/// Trusted in-process renderer output; the host does not deserialize it from
/// RPC input. Engine admission checks the source proof before capture/commit.
pub use vcp_store::export_contract::Rendered;

pub fn render(
    store: &Store,
    access: &Access,
    sources: &Sources,
    capture: CaptureScope,
) -> Result<Rendered> {
    crate::history::authorize(store.state(), access)?;
    if sources.scope().workspace != access.workspace {
        return Err(Error::Access);
    }
    sources.validate_current(store.state(), access.authority, access.tasks.as_ref())?;
    let masks = crate::history::masks(store.state(), &access.workspace)?;
    let mut omissions = vec!["event_data_and_fact_payloads_omitted; metadata-only history, not a conversational transcript".to_owned()];
    let mut history = Vec::new();
    for envelope in sources.events(store.state())? {
        if envelope.redaction.is_some()
            || masks.iter().any(|m| {
                m.session == envelope.event.session
                    && m.first <= envelope.sequence
                    && envelope.sequence <= m.last
            })
        {
            omissions.push(format!(
                "history_sequence_{}_removed",
                envelope.sequence.get()
            ));
            continue;
        }
        let event = &envelope.event;
        history.push(HistoryEntry {
            id: event.id.clone(),
            sequence: envelope.sequence,
            watermark: envelope.watermark,
            task: event.task.clone(),
            kind: event.kind.clone(),
            timestamp: event.timestamp,
            actor: event.actor.clone(),
            correlation: event.correlation.clone(),
            causation: event.causation.clone(),
            artifacts: event.artifacts.clone(),
            payload_omitted: true,
        });
    }
    let mut artifacts = Vec::new();
    let mut bytes_left = MAX_BYTES / 8; // JSON octets need up to four encoded bytes each.
    for artifact in sources.artifacts(store.state())? {
        if capture == CaptureScope::VisibleHistory {
            omissions.push(format!("artifact_{}_bytes_not_requested", artifact.spec.id));
            continue;
        }
        if artifact.state == CaptureState::Purged
            || masks
                .iter()
                .any(|m| m.artifacts.contains(&artifact.spec.id))
        {
            omissions.push(format!("artifact_{}_removed", artifact.spec.id));
            continue;
        }
        let length = usize::try_from(artifact.length.get()).map_err(|_| Error::Limit)?;
        if length > bytes_left {
            return Err(Error::Limit);
        }
        let mut bytes = Vec::with_capacity(length);
        History::read_artifact(store, access, &artifact.spec.id, &mut bytes)?;
        bytes_left -= bytes.len();
        if artifact.state != CaptureState::Complete {
            omissions.push(format!("artifact_{}_capture_incomplete", artifact.spec.id));
        }
        artifacts.push(RetainedArtifact {
            descriptor: artifact,
            bytes,
        });
    }
    // Recursive exports/forecasts are deliberately excluded by the source
    // collector. Their aggregate visibility cannot become a raw-data bypass.
    omissions.push("aggregate_export_and_forecast_payloads_excluded".to_owned());
    let payload = Payload {
        schema: "vcp-session-export/1",
        history_profile: "event-metadata/1",
        sources: sources.clone(),
        history,
        artifacts,
        omissions: omissions.clone(),
    };
    let bytes = serde_json::to_vec(&payload)?;
    if bytes.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    Ok(Rendered {
        sources: sources.clone(),
        capture,
        payload: bytes,
        omissions,
        complete: false,
    })
}
