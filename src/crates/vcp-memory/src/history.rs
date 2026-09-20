// SPDX-License-Identifier: Apache-2.0
//! Historical selection uses immutable versions and today's access/retention.
use crate::{
    access::{self, Access},
    repository, Error, Result,
};
use serde::Serialize;
use vcp_domain::{
    ids::*,
    memory::{Outcome, Version},
    revision::*,
    task::Task,
    verification::Fingerprint,
};
use vcp_store::{
    contract::{Collection, State},
    Store,
};

#[derive(Serialize)]
pub struct EvidenceView {
    pub artifact: ArtifactId,
    pub availability: &'static str,
    pub verification_current: bool,
}
#[derive(Serialize)]
pub struct VersionView {
    pub id: ClaimVersionId,
    pub memory_seq: MemorySeq,
    pub version: Option<Version>,
    pub visibility: &'static str,
    pub applicable: bool,
    pub current: bool,
    pub evidence: Vec<EvidenceView>,
}
#[derive(Serialize)]
pub struct ClaimHistory {
    pub workspace: WorkspaceId,
    pub claim: ClaimId,
    pub watermark: Watermark,
    pub at: Option<MemorySeq>,
    pub versions: Vec<VersionView>,
}

pub(crate) fn removed(state: &State, workspace: &WorkspaceId, version: &Version) -> Result<bool> {
    proposal_removed(state, workspace, &version.proposal)
}
pub(crate) fn proposal_removed(
    state: &State,
    workspace: &WorkspaceId,
    proposal: &vcp_domain::memory::Proposal,
) -> Result<bool> {
    let current: vcp_domain::workspace::Workspace = state
        .record(Collection::Workspace, workspace.as_str(), workspace)?
        .decode()?;
    for record in state
        .records
        .values()
        .filter(|r| r.collection == Collection::Tombstone && r.workspace == *workspace)
    {
        let mask: vcp_audit::history::RetentionMask = record.decode()?;
        if mask.schema_version != 1
            || mask.workspace != *workspace
            || mask.first > mask.last
            || mask.deletion > current.deletion
        {
            return Err(Error::Invalid("inconsistent memory retention scope".into()));
        }
        if proposal
            .evidence
            .iter()
            .any(|e| mask.artifacts.contains(&e.artifact))
        {
            return Ok(true);
        }
        if state.events.iter().any(|e| {
            e.event.workspace == *workspace
                && e.event.session == mask.session
                && e.sequence >= mask.first
                && e.sequence <= mask.last
                && (proposal.origins.contains(&e.event.id)
                    || (e.event.correlation == proposal.command
                        && e.event.kind == vcp_protocol::event::EventKind::MemoryResolved))
        }) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Returns at most 256 immutable versions. Larger histories require an explicit
/// bounded sequence interval in the later common retrieval adapter.
pub fn query(
    store: &Store,
    access: &Access,
    claim: &ClaimId,
    at: Option<MemorySeq>,
    fingerprint: Option<&Fingerprint>,
) -> Result<ClaimHistory> {
    access::authorize(store.state(), access, false)?;
    let mut versions: Vec<_> = repository::versions(store.state(), &access.workspace)?
        .into_iter()
        .filter(|v| v.proposal.claim == *claim && at.is_none_or(|seq| v.memory_seq <= seq))
        .collect();
    versions.sort_by_key(|v| v.memory_seq);
    if versions.len() > 256 {
        return Err(Error::Invalid(
            "claim history exceeds bounded query limit".into(),
        ));
    }
    let current = versions
        .iter()
        .filter(|v| v.resolution.outcome == Outcome::Accepted)
        .max_by_key(|v| v.memory_seq)
        .map(|v| v.id.clone());
    let mut rows = Vec::new();
    for version in versions {
        if !access.allows_task(&version.scope.task) {
            return Err(Error::Access);
        }
        let id = version.id.clone();
        let memory_seq = version.memory_seq;
        let selected = current.as_ref() == Some(&id);
        // Retention may hide content, but cannot make an unauthorized origin's
        // version/claim identities observable through a pruned placeholder.
        access::version_scope(store.state(), access, &version)?;
        if removed(store.state(), &access.workspace, &version)? {
            rows.push(VersionView {
                id,
                memory_seq,
                version: None,
                visibility: "pruned",
                applicable: false,
                current: selected,
                evidence: vec![],
            });
            continue;
        }
        let task: Task = store
            .state()
            .record(
                Collection::Task,
                version.scope.task.as_str(),
                &access.workspace,
            )?
            .decode()?;
        let source = fingerprint.unwrap_or(&task.fingerprint);
        let observations = repository::evidence(store, access, &version.proposal)?;
        if observations
            .iter()
            .any(|o| o.status == crate::gates::EvidenceAvailability::InvalidScope)
        {
            return Err(Error::Access);
        }
        let applicable = version
            .proposal
            .applicability
            .fingerprint
            .as_ref()
            .is_none_or(|saved| saved == source)
            && observations
                .iter()
                .all(|o| o.status == crate::gates::EvidenceAvailability::Available)
            && (version.resolution.evidence_status != vcp_domain::memory::EvidenceStatus::Verified
                || observations.iter().any(|o| o.verification_current));
        let evidence = observations
            .into_iter()
            .map(|o| EvidenceView {
                artifact: o.artifact,
                verification_current: o.verification_current,
                availability: match o.status {
                    crate::gates::EvidenceAvailability::Available => "available",
                    crate::gates::EvidenceAvailability::Unavailable => "unavailable",
                    crate::gates::EvidenceAvailability::InvalidScope => "denied",
                    crate::gates::EvidenceAvailability::Missing => "missing",
                    crate::gates::EvidenceAvailability::DigestMismatch => "integrity_failure",
                },
            })
            .collect();
        rows.push(VersionView {
            id,
            memory_seq,
            version: Some(version),
            visibility: "retained",
            applicable,
            current: selected,
            evidence,
        });
    }
    Ok(ClaimHistory {
        workspace: access.workspace.clone(),
        claim: claim.clone(),
        watermark: store.state().watermark,
        at,
        versions: rows,
    })
}
