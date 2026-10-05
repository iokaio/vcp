// SPDX-License-Identifier: Apache-2.0
//! Shared current-record predicates with explicit fallible historical facts.
//! The frozen complete-State reference remains under differential tests.
use super::*;
use crate::CurrentStateView;

fn memory_version(
    state: CurrentStateView<'_>,
    id: &ClaimVersionId,
    workspace: &WorkspaceId,
) -> Result<(ClaimId, ProposalId, MemorySeq)> {
    crate::redaction_contract::version_identity(state, id, workspace)
}
pub(super) fn validate_memory(state: CurrentStateView<'_>, record: &Record) -> Result<()> {
    use vcp_domain::memory::*;
    match record.memory_kind()? {
        Some("vcp_memory_version_v1") => {
            let value: Version = record.decode()?;
            let proposal = state.record(
                Collection::Claim,
                value.proposal.id.as_str(),
                &record.workspace,
            )?;
            if proposal.memory_kind()? != Some("vcp_memory_proposal_v1") {
                return Err(Error::Corruption("memory proposal reference type"));
            }
            let proposal: ProposalRecord = proposal.decode()?;
            if proposal.proposal != value.proposal || proposal.resolution != value.resolution {
                return Err(Error::Corruption("memory version differs from proposal"));
            }
            if let Some(id) = &value.proposal.predecessor {
                let predecessor = memory_version(state, id, &record.workspace)?;
                if predecessor.0 != value.proposal.claim || predecessor.2 >= value.memory_seq {
                    return Err(Error::Corruption("memory predecessor lineage"));
                }
            }
            for id in &value.resolution.conflicts {
                memory_version(state, id, &record.workspace)?;
            }
            if value.canonical_watermark > state.watermark {
                return Err(Error::Corruption("memory version watermark"));
            }
        }
        Some("vcp_memory_head_v1") => {
            let value: Head = record.decode()?;
            for id in value.current.iter().chain(value.disputed.iter()) {
                let version = memory_version(state, id, &record.workspace)?;
                if version.0 != value.id {
                    return Err(Error::Corruption("memory head claim"));
                }
            }
        }
        Some("vcp_memory_index_intent_v1") => {
            let value: IndexIntent = record.decode()?;
            if let Some(deletion) = value.deletion {
                let workspace: Workspace = state
                    .record(
                        Collection::Workspace,
                        record.workspace.as_str(),
                        &record.workspace,
                    )?
                    .decode()?;
                if deletion > workspace.deletion {
                    return Err(Error::Corruption("retention intent exceeds deletion epoch"));
                }
            }
            for id in value.versions.iter().chain(value.supersedes.iter()) {
                memory_version(state, id, &record.workspace)?;
            }
            if value.canonical_watermark > state.watermark {
                return Err(Error::Corruption("memory index watermark"));
            }
        }
        Some("vcp_memory_result_v1") => {
            let value: ProposalResult = record.decode()?;
            let proposal = state.record(
                Collection::Claim,
                value.proposal.as_str(),
                &record.workspace,
            )?;
            if proposal.memory_kind()? != Some("vcp_memory_proposal_v1") {
                return Err(Error::Corruption("memory result proposal type"));
            }
            let proposal: ProposalRecord = proposal.decode()?;
            if proposal.proposal.command != value.id
                || proposal.scope != value.scope
                || proposal.payload_digest != value.payload_digest
                || proposal.resolution != value.resolution
            {
                return Err(Error::Corruption("memory result differs from proposal"));
            }
            if let Some(id) = &value.version {
                let version = memory_version(state, id, &record.workspace)?;
                if version.1 != value.proposal {
                    return Err(Error::Corruption("memory result version"));
                }
            }
            if let Some(id) = &value.intent {
                let intent =
                    state.record(Collection::IndexIntent, id.as_str(), &record.workspace)?;
                if intent.memory_kind()? != Some("vcp_memory_index_intent_v1") {
                    return Err(Error::Corruption("memory result index type"));
                }
            }
        }
        _ => (),
    }
    Ok(())
}
pub(crate) fn validate_records(
    state: CurrentStateView<'_>,
    history: &mut impl crate::historical_facts::EventFacts,
) -> Result<()> {
    let mut facts = record_validation_facts::RecordFacts::default();
    for (key, record) in state.records {
        if &record.key() != key {
            return Err(Error::Corruption("canonical key"));
        }
        record.validate_shape()?;
        validate_memory(state, record)?;
        crate::memory_review_contract::validate_with_history(state, record, history)?;
        crate::forecast_contract::validate_with_history(state, record, history)?;
        if record.collection == Collection::Access
            && record.value["document_type"] == "vcp_authority_v1"
        {
            use vcp_domain::policy::*;
            let document: AuthorityDocument = record.decode()?;
            if let AuthorityData::Grant { grant } = document.data {
                if let Some(id) = &grant.approval {
                    let approval: Approval = state
                        .record(Collection::Approval, id.as_str(), &record.workspace)?
                        .decode()?;
                    if approval.state != vcp_protocol::command::ApprovalState::Allowed
                        || grant.actor != approval.actor
                        || grant.policy != approval.policy
                        || grant.scope
                            != (GrantScope::Task {
                                scope: approval.scope.clone(),
                            })
                        || grant.target
                            != (GrantTarget::Exact {
                                digest: approval.operation_digest.clone(),
                            })
                        || grant.expires_at != approval.expires_at
                        || Some(grant.authority) != approval.authority
                        || Some(grant.binding) != approval.binding
                    {
                        return Err(Error::Corruption("grant differs from recorded approval"));
                    }
                }
            }
        }
        crate::snapshot_inputs::component_scope(state, record)?;
        for reference in record.required_references()? {
            let (target_key, target) = state
                .records
                .get_key_value(&reference)
                .ok_or(Error::Corruption("missing canonical reference"))?;
            if target.workspace != record.workspace {
                return Err(Error::Access);
            }
            let backup_provenance =
                crate::snapshot_inputs::cross_task_provenance(record, target, &reference)?;
            if let (Some(source), Some(target)) =
                (facts.scope(key, record)?, facts.scope(target_key, target)?)
            {
                // Fork and parent links are explicit task relationships. Data
                // belonging to another task cannot be reused as this task's
                // turn input, verification, approval, or effect observation.
                if record.memory_kind()?.is_none()
                    && ingestion_contract::kind(record)?.is_none()
                    && search_contract::kind(record)?.is_none()
                    && !agents_contract::kind(record)?
                    && record.collection != Collection::Task
                    && record.collection != Collection::Ledger
                    && reference.split(':').next() != Some("ledger")
                    && !backup_provenance
                    && source != target
                {
                    return Err(Error::Access);
                }
            }
        }
        if let Some(scope) = facts.scope(key, record)? {
            let task: Task = state
                .record(Collection::Task, scope.task.as_str(), &scope.workspace)?
                .decode()?;
            if task.scope != scope {
                return Err(Error::Access);
            }
        }
        if record.collection == Collection::Session {
            let session: Session = record.decode()?;
            if let Some(boundary) = session.fork_through {
                let turn: Turn = state
                    .record(Collection::Turn, boundary.as_str(), &record.workspace)?
                    .decode()?;
                if Some(&turn.scope.session) != session.fork_origin.as_ref()
                    || turn.state != vcp_domain::task::TurnState::Completed
                {
                    return Err(Error::Corruption(
                        "session fork boundary differs from ancestry",
                    ));
                }
            }
        }
        if record.collection == Collection::Task {
            let task: Task = record.decode()?;
            let mut seen = BTreeSet::from([task.scope.task.clone()]);
            let mut parent = task.parent.clone();
            while let Some(id) = parent {
                if !seen.insert(id.clone()) {
                    return Err(Error::Corruption("task ancestry cycle"));
                }
                let ancestor: Task = state
                    .record(Collection::Task, id.as_str(), &task.scope.workspace)?
                    .decode()?;
                if ancestor.root != task.root || ancestor.scope.session != task.scope.session {
                    return Err(Error::Corruption("task root or session"));
                }
                parent = ancestor.parent;
            }
            let session: Session = state
                .record(
                    Collection::Session,
                    task.scope.session.as_str(),
                    &task.scope.workspace,
                )?
                .decode()?;
            if session.workspace != task.scope.workspace {
                return Err(Error::Access);
            }
        }
    }
    Ok(())
}
