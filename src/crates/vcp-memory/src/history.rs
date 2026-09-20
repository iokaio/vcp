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
    proposal_removed_with_check(state, workspace, proposal, &|| Ok(()))
}
fn proposal_removed_with_check(
    state: &State,
    workspace: &WorkspaceId,
    proposal: &vcp_domain::memory::Proposal,
    check: &dyn Fn() -> Result<()>,
) -> Result<bool> {
    check()?;
    if crate::retention::purged(
        state,
        workspace,
        &crate::retention::Target::Record(vcp_store::contract::key(
            Collection::Claim,
            proposal.id.as_str(),
        )),
    )? {
        return Ok(true);
    }
    let current: vcp_domain::workspace::Workspace = state
        .record(Collection::Workspace, workspace.as_str(), workspace)?
        .decode()?;
    for record in state.records.values() {
        check()?;
        if record.collection != Collection::Tombstone || record.workspace != *workspace {
            continue;
        }
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
        for e in &state.events {
            check()?;
            if e.event.workspace == *workspace
                && e.event.session == mask.session
                && e.sequence >= mask.first
                && e.sequence <= mask.last
                && (proposal.origins.contains(&e.event.id)
                    || (e.event.correlation == proposal.command
                        && e.event.kind == vcp_protocol::event::EventKind::MemoryResolved))
            {
                return Ok(true);
            }
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
    query_with_check(store, access, claim, at, fingerprint, &|| Ok(()))
}
/// Cooperative history scan. The caller's callback may enforce a shared
/// deadline/cancellation budget; interruption never returns a partial history.
pub fn query_with_check(
    store: &Store,
    access: &Access,
    claim: &ClaimId,
    at: Option<MemorySeq>,
    fingerprint: Option<&Fingerprint>,
    check: &dyn Fn() -> Result<()>,
) -> Result<ClaimHistory> {
    check()?;
    access::authorize(store.state(), access, false)?;
    let mut versions = Vec::new();
    for row in store.state().records.values() {
        check()?;
        if row.workspace == access.workspace
            && row.collection == Collection::Claim
            && row.value["document_type"] == "vcp_memory_version_v1"
            && row.value["proposal"]["claim"].as_str() == Some(claim.as_str())
        {
            let version: Version = row.decode()?;
            if at.is_none_or(|seq| version.memory_seq <= seq) {
                versions.push(version);
            }
            if versions.len() > 256 {
                return Err(Error::Invalid(
                    "claim history exceeds bounded query limit".into(),
                ));
            }
        }
    }
    from_versions_with_check(store, access, claim, at, fingerprint, versions, check)
}
/// Inventory already grouped canonical versions; do not rescan/decode the whole
/// workspace for each claim. This is internal, never a supplied history source.
pub(crate) fn from_versions_with_check(
    store: &Store,
    access: &Access,
    claim: &ClaimId,
    at: Option<MemorySeq>,
    fingerprint: Option<&Fingerprint>,
    versions: Vec<Version>,
    check: &dyn Fn() -> Result<()>,
) -> Result<ClaimHistory> {
    materialize(
        store,
        access,
        claim,
        at,
        fingerprint,
        versions,
        None,
        None,
        check,
    )
}

#[derive(Serialize)]
pub struct OriginLink {
    pub origin: EventId,
    pub claim: ClaimId,
    pub version: ClaimVersionId,
    pub memory_seq: MemorySeq,
}
/// Bounded navigation only: source identities are checked before publishing a
/// claim link. Detailed content still requires the governed history query.
pub fn origin_links(
    store: &Store,
    access: &Access,
    origins: &std::collections::BTreeSet<EventId>,
) -> Result<(Vec<OriginLink>, bool)> {
    if origins.len() > 128 {
        return Err(Error::Invalid("origin navigation limit".into()));
    }
    access::authorize(store.state(), access, false)?;
    let mut links = Vec::new();
    for row in store.state().records.values().filter(|row| {
        row.workspace == access.workspace
            && row.collection == Collection::Claim
            && row.value["document_type"] == "vcp_memory_version_v1"
    }) {
        if !row.value["proposal"]["origins"]
            .as_array()
            .is_some_and(|ids| {
                ids.iter().any(|id| {
                    id.as_str()
                        .is_some_and(|id| origins.iter().any(|origin| origin.as_str() == id))
                })
            })
        {
            continue;
        }
        let version: Version = row.decode()?;
        match access::version_scope(store.state(), access, &version) {
            Err(Error::Access) => continue,
            result => result?,
        }
        if removed(store.state(), &access.workspace, &version)? {
            continue;
        }
        for origin in version
            .proposal
            .origins
            .iter()
            .filter(|id| origins.contains(*id))
        {
            if links.len() == 128 {
                return Ok((links, true));
            }
            links.push(OriginLink {
                origin: origin.clone(),
                claim: version.proposal.claim.clone(),
                version: version.id.clone(),
                memory_seq: version.memory_seq,
            });
        }
    }
    links.sort_by(|a, b| {
        (&a.origin, a.memory_seq, &a.version).cmp(&(&b.origin, b.memory_seq, &b.version))
    });
    Ok((links, false))
}

/// A stable sequence window over immutable claim versions. Only the bounded
/// window retains decoded payloads; current authority applies to the entire
/// claim, including the head used to label historical rows.
pub fn window(
    store: &Store,
    access: &Access,
    claim: &ClaimId,
    at: Option<MemorySeq>,
    after: MemorySeq,
    limit: usize,
) -> Result<(ClaimHistory, MemorySeq, bool)> {
    if !(1..=32).contains(&limit) {
        return Err(Error::Invalid("memory window limit must be 1..32".into()));
    }
    access::authorize(store.state(), access, false)?;
    let mut selected = std::collections::BTreeMap::new();
    let mut current: Option<(MemorySeq, ClaimVersionId)> = None;
    let mut maximum = MemorySeq::ZERO;
    for row in store
        .state()
        .records
        .values()
        .filter(|row| row.workspace == access.workspace && row.collection == Collection::Claim)
    {
        let (seq, id, outcome, full, redacted) = if row.value["document_type"]
            == "vcp_memory_version_v1"
            && row.value["proposal"]["claim"].as_str() == Some(claim.as_str())
        {
            let version: Version = row.decode()?;
            access::version_scope(store.state(), access, &version)?;
            (
                version.memory_seq,
                version.id.clone(),
                version.resolution.outcome,
                Some(version),
                None,
            )
        } else if row.value["document_type"] == vcp_domain::redaction::VERSION
            && row.value["claim"].as_str() == Some(claim.as_str())
        {
            let version: vcp_domain::redaction::RedactedVersion = row.decode()?;
            access::redacted_scope(store.state(), access, &version.scope, &version.sources)?;
            (
                version.memory_seq,
                version.id.clone(),
                version.outcome,
                None,
                Some(version),
            )
        } else {
            continue;
        };
        if at.is_some_and(|upper| seq > upper) {
            continue;
        }
        maximum = maximum.max(seq);
        if outcome == Outcome::Accepted
            && current
                .as_ref()
                .is_none_or(|previous| (seq, &id) > (previous.0, &previous.1))
        {
            current = Some((seq, id.clone()));
        }
        if seq > after {
            selected.insert((seq, id), (full, redacted));
            if selected.len() > limit + 1 {
                selected.pop_last();
            }
        }
    }
    let more = selected.len() > limit;
    if more {
        selected.pop_last();
    }
    let mut full = Vec::new();
    let mut purged = Vec::new();
    for (version, redacted) in selected.into_values() {
        full.extend(version);
        purged.extend(redacted);
    }
    let upper = at.unwrap_or(maximum);
    let history = materialize(
        store,
        access,
        claim,
        Some(upper),
        None,
        full,
        Some(purged),
        Some(current.map(|(_, id)| id)),
        &|| Ok(()),
    )?;
    Ok((history, upper, more))
}

#[allow(clippy::too_many_arguments)]
fn materialize(
    store: &Store,
    access: &Access,
    claim: &ClaimId,
    at: Option<MemorySeq>,
    fingerprint: Option<&Fingerprint>,
    mut versions: Vec<Version>,
    selected_purged: Option<Vec<vcp_domain::redaction::RedactedVersion>>,
    selected_head: Option<Option<ClaimVersionId>>,
    check: &dyn Fn() -> Result<()>,
) -> Result<ClaimHistory> {
    check()?;
    access::authorize(store.state(), access, false)?;
    versions.sort_by_key(|v| v.memory_seq);
    let supplied_purged = selected_purged.is_some();
    let mut purged = selected_purged.unwrap_or_default();
    for row in store.state().records.values().filter(|r| {
        r.workspace == access.workspace
            && r.collection == Collection::Claim
            && r.value["document_type"] == vcp_domain::redaction::VERSION
    }) {
        if supplied_purged {
            break;
        }
        check()?;
        let version: vcp_domain::redaction::RedactedVersion = row.decode()?;
        if version.claim == *claim && at.is_none_or(|seq| version.memory_seq <= seq) {
            purged.push(version);
        }
    }
    if versions.len() + purged.len() > 256 {
        return Err(Error::Invalid(
            "claim history exceeds bounded query limit".into(),
        ));
    }
    let current = selected_head.unwrap_or_else(|| {
        versions
            .iter()
            .filter(|v| v.resolution.outcome == Outcome::Accepted)
            .map(|v| (v.memory_seq, v.id.clone()))
            .chain(
                purged
                    .iter()
                    .filter(|v| v.outcome == Outcome::Accepted)
                    .map(|v| (v.memory_seq, v.id.clone())),
            )
            .max()
            .map(|(_, id)| id)
    });
    let mut rows = Vec::new();
    for version in purged {
        access::redacted_scope(store.state(), access, &version.scope, &version.sources)?;
        rows.push(VersionView {
            current: current.as_ref() == Some(&version.id),
            id: version.id,
            memory_seq: version.memory_seq,
            version: None,
            visibility: "purged",
            applicable: false,
            evidence: vec![],
        });
    }
    for version in versions {
        check()?;
        if !access.allows_task(&version.scope.task) {
            return Err(Error::Access);
        }
        let id = version.id.clone();
        let memory_seq = version.memory_seq;
        let selected = current.as_ref() == Some(&id);
        // Retention may hide content, but cannot make an unauthorized origin's
        // version/claim identities observable through a pruned placeholder.
        access::version_scope(store.state(), access, &version)?;
        if proposal_removed_with_check(store.state(), &access.workspace, &version.proposal, check)?
        {
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
        let observations =
            repository::evidence_with_check(store, access, &version.proposal, check)?;
        check()?;
        if observations
            .iter()
            .any(|o| o.status == crate::gates::EvidenceAvailability::InvalidScope)
        {
            return Err(Error::Access);
        }
        let recall_allowed = crate::retention::recall_allowed(
            store.state(),
            &access.workspace,
            &crate::retention::Target::Record(vcp_store::contract::key(
                Collection::Claim,
                version.id.as_str(),
            )),
        )?;
        let applicable = recall_allowed
            && version
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
    rows.sort_by(|a, b| (a.memory_seq, &a.id).cmp(&(b.memory_seq, &b.id)));
    check()?;
    Ok(ClaimHistory {
        workspace: access.workspace.clone(),
        claim: claim.clone(),
        watermark: store.state().watermark,
        at,
        versions: rows,
    })
}
