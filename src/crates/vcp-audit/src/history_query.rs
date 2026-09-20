// SPDX-License-Identifier: Apache-2.0
//! Raw-history navigation, separate from governed memory retrieval. A cursor
//! freezes the append-only event boundary, but never freezes access/retention.
use crate::{
    history::{self, Access},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use vcp_domain::{
    retention_selector::{Criterion, Facts, Selector, Status, Tree, Truth},
    *,
};
use vcp_protocol::{canonical_bytes, digest_bytes, event::EventEnvelope};
use vcp_store::contract::{Collection, State};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Query {
    pub selector: Selector,
    pub text: Option<String>,
    pub limit: u32,
    pub cursor: Option<Cursor>,
    /// Navigate back from an artifact without reading its retained bytes.
    pub artifact: Option<ArtifactId>,
    #[serde(default)]
    pub expand_compacted: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    pub version: u32,
    pub workspace: WorkspaceId,
    pub watermark: Watermark,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
    pub query_digest: String,
    pub access_digest: String,
    pub metadata_digest: String,
    pub after: u64,
    pub end: u64,
    pub boundary_digest: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Row {
    pub event: EventEnvelope,
    pub content_truncated: bool,
    pub visibility: String,
    pub recall_excluded: bool,
    pub compacted: bool,
    /// Each ID accepts `inspect ID --view outputs --offset N --length M`.
    pub artifacts: Vec<ArtifactId>,
    pub artifact_links: Vec<ArtifactLink>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtifactLink {
    pub id: ArtifactId,
    pub availability: String,
    pub original_bytes: Option<ByteCount>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Page {
    pub workspace: WorkspaceId,
    pub selector: Selector,
    pub kind: String,
    pub source_watermark: Watermark,
    pub newer_events: u64,
    pub rows: Vec<Row>,
    pub gaps: Vec<history::RetainedGap>,
    pub next_cursor: Option<Cursor>,
    pub search_scope: String,
}
pub fn query(state: &State, access: &Access, query: &Query) -> Result<Page> {
    let workspace = history::authorize(state, access)?;
    let selector = query.selector.clone().normalized()?;
    if !(1..=128).contains(&query.limit)
        || query
            .text
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.len() > 512 || s.contains('\0'))
    {
        return Err(Error::Limit);
    }
    let query_digest = digest_bytes(&canonical_bytes(&(
        &selector,
        &query.text,
        query.limit,
        &query.artifact,
        query.expand_compacted,
    ))?);
    let access_digest = digest_bytes(&canonical_bytes(&(
        &access.workspace,
        access.authority,
        &access.tasks,
    ))?);
    // Current task status is mutable metadata, unlike event facts. If requested,
    // bind its projection so a transition cannot silently skip an older match.
    fn task_status(tree: &Tree) -> bool {
        match tree {
            Tree::Match(Criterion::Status(Status::Task(_))) => true,
            Tree::All(items) | Tree::Any(items) => items.iter().any(task_status),
            Tree::Not(item) => task_status(item),
            _ => false,
        }
    }
    let metadata_digest = if task_status(&selector.tree) {
        let rows: Vec<_> = state
            .records
            .values()
            .filter(|r| r.workspace == access.workspace && r.collection == Collection::Task)
            .filter(|r| {
                access
                    .tasks
                    .as_ref()
                    .is_none_or(|tasks| tasks.iter().any(|id| id.as_str() == r.id))
            })
            .map(|r| (&r.id, &r.value["state"]))
            .collect();
        digest_bytes(&canonical_bytes(&rows)?)
    } else {
        String::new()
    };
    let mut cursor = query.cursor.clone().unwrap_or(Cursor {
        version: 1,
        workspace: access.workspace.clone(),
        watermark: state.watermark,
        authority: workspace.authority,
        deletion: workspace.deletion,
        query_digest: query_digest.clone(),
        access_digest: access_digest.clone(),
        metadata_digest: metadata_digest.clone(),
        after: 0,
        end: state.events.len() as u64,
        boundary_digest: digest_bytes(&canonical_bytes(&state.events.last().map(|e| &e.event.id))?),
    });
    if cursor.version != 1
        || cursor.workspace != access.workspace
        || cursor.authority != workspace.authority
        || cursor.deletion != workspace.deletion
        || cursor.query_digest != query_digest
        || cursor.access_digest != access_digest
        || cursor.metadata_digest != metadata_digest
        || cursor.after > cursor.end
        || cursor.end > state.events.len() as u64
        || cursor.watermark > state.watermark
        || cursor.boundary_digest
            != digest_bytes(&canonical_bytes(
                &cursor
                    .end
                    .checked_sub(1)
                    .and_then(|i| state.events.get(i as usize))
                    .map(|e| &e.event.id),
            )?)
    {
        return Err(Error::Restart(
            "history scope, retention or ordering changed",
        ));
    }
    let masks = history::masks(state, &access.workspace)?;
    let mut rows = Vec::new();
    let mut gaps = Vec::new();
    let mut bytes = 0usize;
    for envelope in state
        .events
        .iter()
        .skip(cursor.after as usize)
        .take((cursor.end - cursor.after).min(512) as usize)
    {
        cursor.after += 1;
        let e = &envelope.event;
        if e.workspace != access.workspace || !history::allows(access, e.task.as_ref()) {
            continue;
        }
        if let Some(mask) = masks.iter().find(|m| {
            m.session == e.session && envelope.sequence >= m.first && envelope.sequence <= m.last
        }) {
            let gap = history::RetainedGap {
                session: mask.session.clone(),
                first: mask.first,
                last: mask.last,
                reason: mask.reason.clone(),
            };
            if !gaps.contains(&gap) {
                gaps.push(gap);
            }
            continue;
        }
        let target = serde_json::json!({"kind":"event","id":e.id});
        let decision_id = format!("recall-{}", digest_bytes(&canonical_bytes(&target)?));
        let decision = state
            .records
            .get(&vcp_store::contract::key(
                Collection::Projection,
                &decision_id,
            ))
            .filter(|r| {
                r.workspace == access.workspace
                    && r.value["document_type"] == "vcp_retention_decision_v1"
                    && r.value["target"] == target
            });
        if decision.is_some_and(|r| r.value["purged"] == true) {
            gaps.push(history::RetainedGap {
                session: e.session.clone(),
                first: envelope.sequence,
                last: envelope.sequence,
                reason: "purged by current retention".into(),
            });
            continue;
        }
        let compacted = decision.is_some_and(|r| r.value["compacted"] == true);
        let recall_excluded = decision.is_some_and(|r| r.value["recall_excluded"] == true);
        let event = history::public_event(envelope);
        let metadata = event.event.metadata.as_ref();
        let kind = serde_json::to_value(&e.kind)?;
        let task = e
            .task
            .as_ref()
            .and_then(|id| {
                state
                    .records
                    .get(&vcp_store::contract::key(Collection::Task, id.as_str()))
            })
            .filter(|r| r.workspace == access.workspace)
            .map(|r| r.decode::<vcp_domain::task::Task>())
            .transpose()?;
        let facts = Facts {
            workspace: &e.workspace,
            timestamp: Some(e.timestamp),
            roots: None,
            paths: metadata.map(|m| m.paths.as_slice()),
            task: e.task.as_ref(),
            actor: Some(&e.actor),
            agent: metadata.and_then(|m| m.agent.as_ref()),
            model: metadata.and_then(|m| m.model.as_deref()),
            provider: metadata.and_then(|m| m.provider.as_deref()),
            event: kind.as_str(),
            claim: None,
            task_status: task.as_ref().map(|t| t.state),
            claim_status: None,
            superseded: None,
        };
        if selector.evaluate(&facts)? != Truth::Match
            || query
                .artifact
                .as_ref()
                .is_some_and(|a| !event.event.artifacts.contains(a))
        {
            continue;
        }
        let serialized = serde_json::to_vec(&event)?;
        if query.text.as_ref().is_some_and(|text| {
            event.redaction.is_some() || !String::from_utf8_lossy(&serialized).contains(text)
        }) {
            continue;
        }
        let mut shown = event;
        let truncated = serialized.len() > 8192 || (compacted && !query.expand_compacted);
        if truncated {
            shown.event.data =
                serde_json::json!({"visibility":"bounded_history_summary","full_event_id":e.id});
            shown.event.metadata = None;
        }
        let size = serde_json::to_vec(&shown)?.len();
        if bytes + size > 256 * 1024 && !rows.is_empty() {
            cursor.after -= 1;
            break;
        }
        bytes += size;
        let visibility = if shown.redaction.is_some() {
            "purged"
        } else if compacted && !query.expand_compacted {
            "compacted_presentation_raw_retained"
        } else if e.kind == vcp_protocol::event::EventKind::MemoryResolved {
            "governed_memory_navigation"
        } else {
            "retained_raw_history"
        };
        let mut artifact_links = Vec::new();
        for id in &shown.event.artifacts {
            let descriptor = state
                .records
                .get(&vcp_store::contract::key(Collection::Artifact, id.as_str()));
            if let Some(row) = descriptor {
                if row.workspace != access.workspace {
                    continue;
                }
                let value: vcp_domain::artifact::ArtifactDescriptor = row.decode()?;
                if !history::allows(access, Some(&value.spec.scope.task)) {
                    continue;
                }
                let availability = if masks.iter().any(|mask| mask.artifacts.contains(id)) {
                    "purged"
                } else {
                    match value.state {
                        vcp_domain::artifact::CaptureState::Purged => "purged",
                        vcp_domain::artifact::CaptureState::Complete
                            if !value.spec.omissions.is_empty() =>
                        {
                            "retained_with_omissions"
                        }
                        vcp_domain::artifact::CaptureState::Complete => "retained",
                        _ => "partial_capture",
                    }
                };
                artifact_links.push(ArtifactLink {
                    id: id.clone(),
                    availability: availability.into(),
                    original_bytes: Some(value.length),
                });
            } else {
                artifact_links.push(ArtifactLink {
                    id: id.clone(),
                    availability: "unavailable".into(),
                    original_bytes: None,
                });
            }
        }
        shown.event.artifacts = artifact_links.iter().map(|link| link.id.clone()).collect();
        rows.push(Row {
            artifacts: shown.event.artifacts.clone(),
            event: shown,
            content_truncated: truncated,
            visibility: visibility.into(),
            recall_excluded,
            compacted,
            artifact_links,
        });
        if rows.len() == query.limit as usize {
            break;
        }
    }
    let newer_events = state
        .events
        .iter()
        .skip(cursor.end as usize)
        .filter(|e| {
            e.event.workspace == access.workspace && history::allows(access, e.event.task.as_ref())
        })
        .count() as u64;
    Ok(Page{workspace:access.workspace.clone(),selector,kind:"raw_history".into(),source_watermark:cursor.watermark,newer_events,rows,gaps,
        next_cursor:(cursor.after<cursor.end).then_some(cursor),search_scope:"retained event facts; artifact bytes are available through explicit bounded inspection, not searched implicitly".into()})
}
