// SPDX-License-Identifier: Apache-2.0
use crate::{Error, Result};
use std::collections::BTreeSet;
use vcp_domain::{ids::*, revision::*, workspace::Workspace};
use vcp_store::contract::{Collection, State};
use vcp_store::CurrentStateView;

mod origin_proof;
pub(crate) use origin_proof::version_scope_store;
use origin_proof::{OriginProof, StateOrigins};

/// Issued by the authenticated host. Serialized proposals cannot mint access.
pub struct Access {
    pub workspace: WorkspaceId,
    pub actor: ActorId,
    pub authority: AuthorityRevision,
    pub read: bool,
    pub write: bool,
    pub tasks: Option<BTreeSet<TaskId>>,
}
impl Access {
    pub fn allows_task(&self, task: &TaskId) -> bool {
        self.tasks.as_ref().is_none_or(|tasks| tasks.contains(task))
    }
    pub fn history(&self) -> vcp_audit::history::Access {
        vcp_audit::history::Access {
            workspace: self.workspace.clone(),
            authority: self.authority,
            read: self.read,
            tasks: self.tasks.clone(),
        }
    }
}
pub(crate) fn authorize<'a>(
    state: impl Into<CurrentStateView<'a>>,
    access: &Access,
    write: bool,
) -> Result<Workspace> {
    let state = state.into();
    if !access.read || (write && !access.write) {
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
pub(crate) fn policy<'a>(
    state: impl Into<CurrentStateView<'a>>,
    workspace: &WorkspaceId,
) -> Result<PolicyRevision> {
    let state = state.into();
    use vcp_domain::policy::{AuthorityData, AuthorityDocument};
    let Some(record) = state.records.get(&vcp_store::contract::key(
        Collection::Access,
        workspace.as_str(),
    )) else {
        return Ok(PolicyRevision::ZERO);
    };
    if record.workspace != *workspace {
        return Err(Error::Access);
    }
    let document: AuthorityDocument = record.decode()?;
    match document.data {
        AuthorityData::Policy { policy } => Ok(policy.revision),
        _ => Err(Error::Access),
    }
}

/// Current task access applies to every source of a derived claim, even when
/// its own owning task remains visible. Missing sources are availability facts;
/// known foreign/hidden sources are authorization failures, never degradations.
pub(crate) fn proposal_scope(
    state: &State,
    access: &Access,
    proposal: &vcp_domain::memory::Proposal,
) -> Result<()> {
    proposal_scope_with_history(state.into(), access, proposal, &mut StateOrigins(state))
}

fn proposal_scope_with_history(
    state: CurrentStateView<'_>,
    access: &Access,
    proposal: &vcp_domain::memory::Proposal,
    history: &mut impl OriginProof,
) -> Result<()> {
    use vcp_domain::{artifact::ArtifactDescriptor, verification::Verification};
    if proposal.scope.workspace != access.workspace || !access.allows_task(&proposal.scope.task) {
        return Err(Error::Access);
    }
    history.check(access, &proposal.origins, false)?;
    for reference in &proposal.evidence {
        if let Some(row) = state.records.get(&vcp_store::contract::key(
            Collection::Artifact,
            reference.artifact.as_str(),
        )) {
            if row.workspace != access.workspace {
                return Err(Error::Access);
            }
            let artifact: ArtifactDescriptor = row.decode()?;
            if !access.allows_task(&artifact.spec.scope.task) {
                return Err(Error::Access);
            }
        }
        if let Some(id) = &reference.verification {
            if let Some(row) = state.records.get(&vcp_store::contract::key(
                Collection::Verification,
                id.as_str(),
            )) {
                if row.workspace != access.workspace {
                    return Err(Error::Access);
                }
                let verification: Verification = row.decode()?;
                if !access.allows_task(&verification.scope.task) {
                    return Err(Error::Access);
                }
            }
        }
    }
    if let Some(id) = &proposal.predecessor {
        if let Some(row) = state
            .records
            .get(&vcp_store::contract::key(Collection::Claim, id.as_str()))
        {
            if row.workspace != access.workspace {
                return Err(Error::Access);
            }
            if row.value["document_type"] == "vcp_memory_version_v1" {
                let prior: vcp_domain::memory::Version = row.decode()?;
                if !access.allows_task(&prior.scope.task) {
                    return Err(Error::Access);
                }
            } else if row.value["document_type"] == vcp_domain::redaction::VERSION {
                let prior: vcp_domain::redaction::RedactedVersion = row.decode()?;
                redacted_scope_with_history(state, access, &prior.scope, &prior.sources, history)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn resolution_scope(
    state: &State,
    access: &Access,
    resolution: &vcp_domain::memory::Resolution,
) -> Result<()> {
    resolution_scope_with_history(state.into(), access, resolution, &mut StateOrigins(state))
}

fn resolution_scope_with_history(
    state: CurrentStateView<'_>,
    access: &Access,
    resolution: &vcp_domain::memory::Resolution,
    history: &mut impl OriginProof,
) -> Result<()> {
    for id in &resolution.conflicts {
        let row = state.record(Collection::Claim, id.as_str(), &access.workspace)?;
        if row.value["document_type"] == vcp_domain::redaction::VERSION {
            let version: vcp_domain::redaction::RedactedVersion = row.decode()?;
            redacted_scope_with_history(state, access, &version.scope, &version.sources, history)?;
            continue;
        }
        let version: vcp_domain::memory::Version = state
            .record(Collection::Claim, id.as_str(), &access.workspace)?
            .decode()?;
        proposal_scope_with_history(state, access, &version.proposal, history)?;
    }
    Ok(())
}

pub(crate) fn version_scope(
    state: &State,
    access: &Access,
    version: &vcp_domain::memory::Version,
) -> Result<()> {
    version_scope_with_history(state.into(), access, version, &mut StateOrigins(state))
}

fn version_scope_with_history(
    state: CurrentStateView<'_>,
    access: &Access,
    version: &vcp_domain::memory::Version,
    history: &mut impl OriginProof,
) -> Result<()> {
    proposal_scope_with_history(state, access, &version.proposal, history)?;
    resolution_scope_with_history(state, access, &version.resolution, history)
}

/// Redaction preserves provenance identities, never authority. Resolve today's
/// source scopes before exposing even a purged version identifier.
pub(crate) fn redacted_scope(
    state: &State,
    access: &Access,
    scope: &vcp_domain::workspace::Scope,
    sources: &vcp_domain::redaction::Sources,
) -> Result<()> {
    redacted_scope_with_history(
        state.into(),
        access,
        scope,
        sources,
        &mut StateOrigins(state),
    )
}

fn redacted_scope_with_history(
    state: CurrentStateView<'_>,
    access: &Access,
    scope: &vcp_domain::workspace::Scope,
    sources: &vcp_domain::redaction::Sources,
    history: &mut impl OriginProof,
) -> Result<()> {
    fn check(
        state: CurrentStateView<'_>,
        access: &Access,
        scope: &vcp_domain::workspace::Scope,
        sources: &vcp_domain::redaction::Sources,
        visited: &mut BTreeSet<ClaimVersionId>,
        history: &mut impl OriginProof,
    ) -> Result<()> {
        if scope.workspace != access.workspace || !access.allows_task(&scope.task) {
            return Err(Error::Access);
        }
        history.check(access, &sources.origins, true)?;
        for id in &sources.artifacts {
            let artifact: vcp_domain::artifact::ArtifactDescriptor = state
                .record(Collection::Artifact, id.as_str(), &access.workspace)?
                .decode()?;
            if !access.allows_task(&artifact.spec.scope.task) {
                return Err(Error::Access);
            }
        }
        for id in &sources.verifications {
            let verification: vcp_domain::verification::Verification = state
                .record(Collection::Verification, id.as_str(), &access.workspace)?
                .decode()?;
            if !access.allows_task(&verification.scope.task) {
                return Err(Error::Access);
            }
        }
        for id in &sources.versions {
            if !visited.insert(id.clone()) {
                continue;
            }
            if visited.len() > 256 {
                return Err(Error::Access);
            }
            let row = state.record(Collection::Claim, id.as_str(), &access.workspace)?;
            if row.value["document_type"] == vcp_domain::redaction::VERSION {
                let version: vcp_domain::redaction::RedactedVersion = row.decode()?;
                check(
                    state,
                    access,
                    &version.scope,
                    &version.sources,
                    visited,
                    history,
                )?;
            } else {
                let version: vcp_domain::memory::Version = row.decode()?;
                // Convert only metadata to share the same bounded traversal.
                let redacted = vcp_protocol::redaction::version(&version, DeletionEpoch::new(1))
                    .map_err(Error::Invalid)?;
                check(
                    state,
                    access,
                    &redacted.scope,
                    &redacted.sources,
                    visited,
                    history,
                )?;
            }
        }
        Ok(())
    }
    check(state, access, scope, sources, &mut BTreeSet::new(), history)
}
