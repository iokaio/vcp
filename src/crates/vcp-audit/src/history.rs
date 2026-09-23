// SPDX-License-Identifier: Apache-2.0
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{artifact::ArtifactDescriptor, ids::*, revision::*, workspace::Workspace};
use vcp_protocol::{canonical_bytes, digest_bytes, event::*};
use vcp_store::{contract::*, Snapshot, Store};

const MAX_SNAPSHOTS: usize = 4;
const MAX_SCAN: usize = 256;

/// Generic history cannot authorize the transitive sources of a memory claim.
/// Preserve navigation while leaving derived payloads to governed queries.
pub(crate) fn public_event(event: &EventEnvelope) -> EventEnvelope {
    let mut result = event.clone();
    if event.redaction.is_none() && event.event.kind == EventKind::MemoryResolved {
        let proposal = event
            .event
            .data
            .get("proposal")
            .and_then(|v| v.as_str())
            .and_then(|id| ProposalId::parse(id).ok());
        let resolution = event
            .event
            .data
            .get("resolution")
            .cloned()
            .and_then(|v| serde_json::from_value::<vcp_domain::memory::Outcome>(v).ok());
        result.event.data = serde_json::json!({"schema_version":1,"proposal":proposal,"resolution":resolution,
            "visibility":"governed_query_required","reason":"memory payload requires current origin, evidence and retention checks"});
    }
    result
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Filter {
    pub session: Option<SessionId>,
    pub task: Option<TaskId>,
    pub agent: Option<AgentId>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub path: Option<String>,
    pub kind: Option<EventKind>,
    pub from_inclusive: Option<Timestamp>,
    pub to_exclusive: Option<Timestamp>,
}
impl Filter {
    fn validate(&self) -> Result<()> {
        if self
            .from_inclusive
            .zip(self.to_exclusive)
            .is_some_and(|(a, b)| a >= b)
        {
            return Err(Error::Integrity("empty or reversed time range"));
        }
        for value in [&self.provider, &self.model, &self.path]
            .into_iter()
            .flatten()
        {
            if value.is_empty() || value.len() > 32768 || value.contains('\0') {
                return Err(Error::Limit);
            }
        }
        Ok(())
    }
    fn matches(&self, event: &EventEnvelope) -> bool {
        let e = &event.event;
        if self.session.as_ref().is_some_and(|id| id != &e.session)
            || self
                .task
                .as_ref()
                .is_some_and(|id| Some(id) != e.task.as_ref())
            || self.kind.as_ref().is_some_and(|kind| kind != &e.kind)
            || self.from_inclusive.is_some_and(|from| e.timestamp < from)
            || self.to_exclusive.is_some_and(|to| e.timestamp >= to)
        {
            return false;
        }
        let metadata = e.metadata.as_ref();
        if self
            .agent
            .as_ref()
            .is_some_and(|id| metadata.and_then(|m| m.agent.as_ref()) != Some(id))
            || self
                .provider
                .as_ref()
                .is_some_and(|name| metadata.and_then(|m| m.provider.as_ref()) != Some(name))
            || self
                .model
                .as_ref()
                .is_some_and(|name| metadata.and_then(|m| m.model.as_ref()) != Some(name))
            || self
                .path
                .as_ref()
                .is_some_and(|path| !metadata.is_some_and(|m| m.paths.contains(path)))
        {
            return false;
        }
        true
    }
}
/// Authenticated local host capability, not deserializable cursor input.
pub struct Access {
    pub workspace: WorkspaceId,
    pub authority: AuthorityRevision,
    pub read: bool,
    pub tasks: Option<BTreeSet<TaskId>>,
}
pub(crate) fn authorize(state: &State, access: &Access) -> Result<Workspace> {
    if !access.read {
        return Err(Error::Access);
    }
    let workspace: Workspace = state
        .record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        )?
        .decode()?;
    if workspace.authority != access.authority {
        return Err(Error::Access);
    }
    Ok(workspace)
}
pub(crate) fn allows(access: &Access, task: Option<&TaskId>) -> bool {
    access
        .tasks
        .as_ref()
        .is_none_or(|tasks| task.is_some_and(|id| tasks.contains(id)))
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    pub version: u32,
    pub snapshot: SnapshotId,
    pub workspace: WorkspaceId,
    pub query_digest: String,
    pub access_digest: String,
    pub watermark: Watermark,
    /// Number of canonical events already examined; exclusive resume boundary.
    pub after: Units,
    pub end: Units,
    pub limit: u32,
    pub expires_at: Timestamp,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedGap {
    pub session: SessionId,
    pub first: SessionSeq,
    pub last: SessionSeq,
    pub reason: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub events: Vec<EventEnvelope>,
    pub gaps: Vec<RetainedGap>,
    pub next_cursor: Cursor,
    pub snapshot_watermark: Watermark,
    pub at_end: bool,
}
pub use vcp_domain::retention::RetentionMask;
pub(crate) fn masks(state: &State, workspace: &WorkspaceId) -> Result<Vec<RetentionMask>> {
    let mut result = Vec::new();
    for record in state
        .records
        .values()
        .filter(|r| r.collection == Collection::Tombstone && &r.workspace == workspace)
    {
        let mask: RetentionMask = record.decode()?;
        if &mask.workspace != workspace || mask.validate().is_err() {
            return Err(Error::Version);
        }
        result.push(mask);
    }
    Ok(result)
}
fn access_digest(access: &Access) -> Result<String> {
    Ok(digest_bytes(&canonical_bytes(&access.tasks)?))
}
struct Pinned {
    snapshot: Snapshot,
    filter: Filter,
    cursor: Cursor,
}
#[derive(Default)]
pub struct History {
    snapshots: BTreeMap<SnapshotId, Pinned>,
}
impl History {
    pub fn start(
        &mut self,
        store: &Store,
        access: &Access,
        filter: Filter,
        limit: u32,
        now: Timestamp,
    ) -> Result<Cursor> {
        let workspace = authorize(store.state(), access)?;
        filter.validate()?;
        if limit == 0 || limit as usize > vcp_protocol::version::MAX_PAGE_EVENTS {
            return Err(Error::Limit);
        }
        if filter
            .task
            .as_ref()
            .is_some_and(|task| !allows(access, Some(task)))
        {
            return Err(Error::Access);
        }
        self.snapshots
            .retain(|_, snapshot| snapshot.cursor.expires_at > now);
        if self.snapshots.len() >= MAX_SNAPSHOTS {
            return Err(Error::Limit);
        }
        let snapshot = store.snapshot()?;
        let cursor = Cursor {
            version: 1,
            snapshot: SnapshotId::new(),
            workspace: access.workspace.clone(),
            query_digest: digest_bytes(&canonical_bytes(&filter)?),
            access_digest: access_digest(access)?,
            watermark: snapshot.state().watermark,
            after: Units::ZERO,
            end: Units::new(snapshot.state().events.len() as u64),
            limit,
            expires_at: Timestamp::new(now.get().checked_add(60_000).ok_or(Error::Limit)?),
            authority: workspace.authority,
            deletion: workspace.deletion,
        };
        self.snapshots.insert(
            cursor.snapshot.clone(),
            Pinned {
                snapshot,
                filter,
                cursor: cursor.clone(),
            },
        );
        Ok(cursor)
    }
    pub fn page(
        &self,
        store: &Store,
        access: &Access,
        filter: &Filter,
        cursor: &Cursor,
        now: Timestamp,
    ) -> Result<Page> {
        let workspace = authorize(store.state(), access)?;
        if cursor.workspace != access.workspace {
            return Err(Error::Access);
        }
        let pinned = self
            .snapshots
            .get(&cursor.snapshot)
            .ok_or(Error::Restart("snapshot unavailable"))?;
        if now >= pinned.cursor.expires_at {
            return Err(Error::Restart("snapshot expired"));
        }
        let mut expected = pinned.cursor.clone();
        expected.after = cursor.after;
        if cursor != &expected
            || cursor.after > cursor.end
            || &pinned.filter != filter
            || cursor.query_digest != digest_bytes(&canonical_bytes(filter)?)
        {
            return Err(Error::Restart("cursor or filter changed"));
        }
        if cursor.authority != workspace.authority || cursor.access_digest != access_digest(access)?
        {
            return Err(Error::Restart("current access changed"));
        }
        if cursor.deletion != workspace.deletion {
            return Err(Error::Restart("retention scope changed"));
        }
        let masks = masks(store.state(), &access.workspace)?;
        if masks.iter().any(|mask| mask.deletion > workspace.deletion) {
            return Err(Error::Integrity("retention epoch"));
        }
        let mut events = Vec::new();
        let mut gaps = Vec::new();
        let mut next = cursor.clone();
        for event in pinned
            .snapshot
            .state()
            .events
            .iter()
            .skip(cursor.after.get() as usize)
            .take(MAX_SCAN)
        {
            next.after = next.after.next()?;
            if event.event.workspace != access.workspace
                || !allows(access, event.event.task.as_ref())
            {
                continue;
            }
            // Suppression happens before content-sensitive filtering, so old
            // metadata cannot leak through a filter on removed event content.
            let hidden = masks.iter().find(|mask| {
                mask.session == event.event.session
                    && event.sequence >= mask.first
                    && event.sequence <= mask.last
            });
            if let Some(mask) = hidden {
                if filter
                    .session
                    .as_ref()
                    .is_none_or(|session| session == &mask.session)
                {
                    let gap = RetainedGap {
                        session: mask.session.clone(),
                        first: mask.first,
                        last: mask.last,
                        reason: mask.reason.clone(),
                    };
                    if !gaps.contains(&gap) {
                        gaps.push(gap);
                    }
                }
                continue;
            }
            if filter.matches(event) {
                events.push(public_event(event));
                if events.len() == cursor.limit as usize {
                    break;
                }
            }
        }
        Ok(Page {
            events,
            gaps,
            snapshot_watermark: cursor.watermark,
            at_end: next.after == next.end,
            next_cursor: next,
        })
    }
    pub fn close(&mut self, snapshot: &SnapshotId) {
        self.snapshots.remove(snapshot);
    }
    pub fn read_artifact(
        store: &Store,
        access: &Access,
        id: &ArtifactId,
        sink: impl std::io::Write,
    ) -> Result<ArtifactDescriptor> {
        authorize(store.state(), access)?;
        let artifact: ArtifactDescriptor = store
            .state()
            .record(Collection::Artifact, id.as_str(), &access.workspace)?
            .decode()?;
        if !allows(access, Some(&artifact.spec.scope.task)) {
            return Err(Error::Access);
        }
        // Aggregate forecast payloads can depend on many tasks and retained
        // sources. Only the specialized optimizer loader validates that full
        // manifest, including logical exclusion and current deletion epochs.
        if artifact.spec.schema == "vcp-optimization-forecast-v1" {
            return Err(Error::Access);
        }
        vcp_store::export_contract::validate_read(
            store.state(),
            access.authority,
            access.tasks.as_ref(),
            &artifact,
        )?;
        if masks(store.state(), &access.workspace)?
            .iter()
            .any(|mask| mask.artifacts.contains(id))
        {
            return Err(Error::Removed);
        }
        store.spool().read(&artifact, sink)?;
        Ok(artifact)
    }
}
