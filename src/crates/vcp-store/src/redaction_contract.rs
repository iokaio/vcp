// SPDX-License-Identifier: Apache-2.0
//! Exact typed rewrite contract. Ordinary Put never admits these transformations.
use crate::{contract::*, Error, Result};
use std::collections::BTreeSet;
use vcp_domain::{
    accounting::*, artifact::ArtifactDescriptor, effect::Effect, task::Task, workspace::Workspace,
    *,
};
use vcp_domain::{
    artifact::CaptureState,
    memory::{ProposalRecord, ProposalResult, Version},
    redaction::{
        self, RedactedAdvisory, RedactedObserver, RedactedProposal, RedactedResult,
        RedactedVersion, Sources,
    },
    workspace::Scope,
};
use vcp_protocol::command::{CommandReceipt, CommandResult};

pub(crate) fn kind(row: &Record) -> Result<Option<&str>> {
    let Some(tag) = row.value["document_type"].as_str() else {
        return Ok(None);
    };
    match tag {
        vcp_domain::memory_review::REDACTED_SUBMISSION if row.collection == Collection::Claim => {
            Ok(Some(tag))
        }
        vcp_domain::memory_review::REDACTED_DECISION
            if row.collection == Collection::Projection =>
        {
            Ok(Some(tag))
        }
        redaction::PROPOSAL | redaction::VERSION if row.collection == Collection::Claim => {
            Ok(Some(tag))
        }
        redaction::RESULT if row.collection == Collection::Projection => Ok(Some(tag)),
        redaction::ADVISORY if row.collection == Collection::Projection => Ok(Some(tag)),
        redaction::OBSERVER if row.collection == Collection::Projection => Ok(Some(tag)),
        vcp_domain::forecast::REDACTED if row.collection == Collection::Projection => Ok(Some(tag)),
        _ if tag.starts_with("vcp_memory_redacted_")
            || tag.starts_with("vcp_escalation_redacted_")
            || tag.starts_with("vcp_optimization_redacted_")
            || tag.starts_with("vcp_observer_redacted_") =>
        {
            Err(Error::Corruption("redacted entity type or collection"))
        }
        _ => Ok(None),
    }
}
pub(crate) fn scope(row: &Record) -> Result<Scope> {
    if crate::memory_review_contract::kind(row)?.is_some() {
        return crate::memory_review_contract::scope(row);
    }
    match kind(row)? {
        Some(redaction::PROPOSAL) => Ok(row.decode::<RedactedProposal>()?.scope),
        Some(redaction::VERSION) => Ok(row.decode::<RedactedVersion>()?.scope),
        Some(redaction::RESULT) => Ok(row.decode::<RedactedResult>()?.scope),
        Some(redaction::ADVISORY) => Ok(row.decode::<RedactedAdvisory>()?.scope),
        Some(redaction::OBSERVER) => Ok(row.decode::<RedactedObserver>()?.scope),
        _ => Err(Error::Corruption("redacted entity expected")),
    }
}
pub(crate) fn shape(row: &Record) -> Result<()> {
    if crate::memory_review_contract::kind(row)?.is_some() {
        return crate::memory_review_contract::shape(row);
    }
    if kind(row)? == Some(vcp_domain::forecast::REDACTED) {
        let value: vcp_domain::forecast::RedactedSources = row.decode()?;
        value.validate()?;
        if value.id != row.id || value.workspace != row.workspace || value.revision != row.revision
        {
            return Err(Error::Corruption("redacted forecast identity"));
        }
        return Ok(());
    }
    let (id, scope, revision) = match kind(row)? {
        Some(redaction::OBSERVER) => {
            let value: RedactedObserver = row.decode()?;
            value.validate()?;
            (value.id, value.scope, value.revision)
        }
        Some(redaction::ADVISORY) => {
            let value: RedactedAdvisory = row.decode()?;
            value.validate()?;
            (value.id, value.scope, value.revision)
        }
        Some(redaction::PROPOSAL) => {
            let value: RedactedProposal = row.decode()?;
            value.validate()?;
            (value.id.to_string(), value.scope, value.revision)
        }
        Some(redaction::VERSION) => {
            let value: RedactedVersion = row.decode()?;
            value.validate()?;
            (value.id.to_string(), value.scope, value.revision)
        }
        Some(redaction::RESULT) => {
            let value: RedactedResult = row.decode()?;
            value.validate()?;
            (value.id.to_string(), value.scope, value.revision)
        }
        _ => return Err(Error::Corruption("redacted entity expected")),
    };
    if id != row.id || scope.workspace != row.workspace || revision != row.revision {
        return Err(Error::Corruption("redacted entity identity"));
    }
    Ok(())
}
fn source_refs(refs: &mut BTreeSet<String>, sources: &Sources) {
    refs.extend(
        sources
            .artifacts
            .iter()
            .map(|id| key(Collection::Artifact, id.as_str())),
    );
    refs.extend(
        sources
            .verifications
            .iter()
            .map(|id| key(Collection::Verification, id.as_str())),
    );
    refs.extend(
        sources
            .versions
            .iter()
            .map(|id| key(Collection::Claim, id.as_str())),
    );
}
pub(crate) fn references(row: &Record) -> Result<BTreeSet<String>> {
    if crate::memory_review_contract::kind(row)?.is_some() {
        return crate::memory_review_contract::references(row);
    }
    if kind(row)? == Some(vcp_domain::forecast::REDACTED) {
        return Ok(BTreeSet::from([key(
            Collection::Workspace,
            row.workspace.as_str(),
        )]));
    }
    let mut refs = BTreeSet::from([key(Collection::Task, scope(row)?.task.as_str())]);
    match kind(row)? {
        Some(redaction::ADVISORY | redaction::OBSERVER) => (),
        Some(redaction::PROPOSAL) => {
            source_refs(&mut refs, &row.decode::<RedactedProposal>()?.sources)
        }
        Some(redaction::VERSION) => {
            let value: RedactedVersion = row.decode()?;
            source_refs(&mut refs, &value.sources);
            refs.insert(key(Collection::Claim, value.proposal.as_str()));
        }
        Some(redaction::RESULT) => {
            let value: RedactedResult = row.decode()?;
            refs.insert(key(Collection::Claim, value.proposal.as_str()));
            refs.extend(
                value
                    .version
                    .iter()
                    .map(|id| key(Collection::Claim, id.as_str())),
            );
            refs.extend(
                value
                    .intent
                    .iter()
                    .map(|id| key(Collection::IndexIntent, id.as_str())),
            );
        }
        _ => return Err(Error::Corruption("redacted entity expected")),
    }
    Ok(refs)
}
pub(crate) fn version_identity(
    state: &State,
    id: &ClaimVersionId,
    workspace: &WorkspaceId,
) -> Result<(ClaimId, ProposalId, MemorySeq)> {
    let row = state.record(Collection::Claim, id.as_str(), workspace)?;
    if kind(row)? == Some(redaction::VERSION) {
        let value: RedactedVersion = row.decode()?;
        Ok((value.claim, value.proposal, value.memory_seq))
    } else if row.value["document_type"] == "vcp_memory_version_v1" {
        let value: Version = row.decode()?;
        Ok((value.proposal.claim, value.proposal.id, value.memory_seq))
    } else {
        Err(Error::Corruption("memory version reference type"))
    }
}
pub(crate) fn validate(state: &State) -> Result<()> {
    for row in state.records.values() {
        if row.collection == Collection::Task {
            let task: Task = row.decode()?;
            if let Some(redaction) = task.redaction {
                if redaction.deletion > epoch(state, &row.workspace)? {
                    return Err(Error::Corruption("task redaction exceeds deletion epoch"));
                }
            }
        }
        if row.collection == Collection::Settlement {
            let settlement: Settlement = row.decode()?;
            if settlement.redaction.is_none() && settlement.observation_digest.is_some() {
                return Err(Error::Corruption(
                    "settlement digest requires explicit redaction",
                ));
            }
        }
        let erased = match row.collection {
            Collection::Verification => {
                row.decode::<vcp_domain::verification::Verification>()?
                    .redaction
            }
            Collection::Effect => row.decode::<vcp_domain::effect::Effect>()?.redaction,
            Collection::Turn => row.decode::<vcp_domain::task::Turn>()?.redaction,
            Collection::Attempt => row.decode::<Attempt>()?.redaction,
            Collection::Settlement => row.decode::<Settlement>()?.redaction,
            _ => None,
        };
        if let Some(erased) = erased {
            erased.validate()?;
            if erased.deletion > epoch(state, &row.workspace)? {
                return Err(Error::Corruption("redaction epoch"));
            }
            // Protection is checked against the exact source at rewrite admission.
            // Later newly captured liability may coexist with erased old evidence.
            if row.collection == Collection::Verification {
                let value: vcp_domain::verification::Verification = row.decode()?;
                let retained_check_text = value.checks.iter().any(|check| {
                    !check.specification.is_empty()
                        || matches!(
                            &check.outcome,
                            vcp_domain::verification::CheckOutcome::Failed { reason }
                                | vcp_domain::verification::CheckOutcome::NotRun { reason }
                                if !reason.is_empty()
                        )
                });
                let retained_cost_text = matches!(
                    &value.cost,
                    vcp_domain::verification::CostCertainty::Uncertain { reason, .. }
                        if !reason.is_empty()
                );
                if !value.outstanding_issues.is_empty() || retained_check_text || retained_cost_text
                {
                    return Err(Error::Corruption("redacted verification payload"));
                }
            } else if row.collection == Collection::Attempt {
                row.decode::<Attempt>()?.validate()?;
            } else if row.collection == Collection::Settlement {
                let value: Settlement = row.decode()?;
                if value
                    .observation_digest
                    .as_deref()
                    .is_none_or(|digest| !valid_hash(digest))
                    || value
                        .observation
                        .correction
                        .as_ref()
                        .is_some_and(|correction| {
                            !correction.reason.is_empty()
                                || !correction.remaining_uncertainty.is_empty()
                        })
                {
                    return Err(Error::Corruption("redacted settlement narrative"));
                }
            } else if row.value["reason"] != "" {
                return Err(Error::Corruption("redacted lifecycle payload"));
            }
        }
        if kind(row)?.is_none() {
            continue;
        }
        shape(row)?;
        let workspace: Workspace = state
            .record(
                Collection::Workspace,
                row.workspace.as_str(),
                &row.workspace,
            )?
            .decode()?;
        let epoch = match kind(row)? {
            Some(vcp_domain::memory_review::REDACTED_SUBMISSION) => {
                row.decode::<vcp_domain::memory_review::RedactedSubmission>()?
                    .deletion
            }
            Some(vcp_domain::memory_review::REDACTED_DECISION) => {
                row.decode::<vcp_domain::memory_review::RedactedDecision>()?
                    .deletion
            }
            Some(vcp_domain::forecast::REDACTED) => {
                row.decode::<vcp_domain::forecast::RedactedSources>()?
                    .deletion
            }
            Some(redaction::ADVISORY) => row.decode::<RedactedAdvisory>()?.deletion,
            Some(redaction::OBSERVER) => row.decode::<RedactedObserver>()?.deletion,
            Some(redaction::PROPOSAL) => row.decode::<RedactedProposal>()?.deletion,
            Some(redaction::VERSION) => {
                let value: RedactedVersion = row.decode()?;
                let proposal: RedactedProposal = state
                    .record(Collection::Claim, value.proposal.as_str(), &row.workspace)?
                    .decode()?;
                if proposal.claim != value.claim
                    || proposal.scope != value.scope
                    || proposal.outcome != value.outcome
                    || proposal.sources != value.sources
                {
                    return Err(Error::Corruption("redacted version proposal lineage"));
                }
                if let Some(previous) = &value.predecessor {
                    let (claim, _, sequence) = version_identity(state, previous, &row.workspace)?;
                    if claim != value.claim || sequence >= value.memory_seq {
                        return Err(Error::Corruption("redacted predecessor lineage"));
                    }
                }
                value.deletion
            }
            Some(redaction::RESULT) => {
                let value: RedactedResult = row.decode()?;
                let proposal: RedactedProposal = state
                    .record(Collection::Claim, value.proposal.as_str(), &row.workspace)?
                    .decode()?;
                if proposal.command != value.id
                    || proposal.scope != value.scope
                    || proposal.payload_digest != value.payload_digest
                    || proposal.outcome != value.outcome
                {
                    return Err(Error::Corruption("redacted result proposal identity"));
                }
                if let Some(id) = &value.version {
                    let (_, proposal, sequence) = version_identity(state, id, &row.workspace)?;
                    if proposal != value.proposal || sequence != value.memory_seq {
                        return Err(Error::Corruption("redacted result version identity"));
                    }
                }
                if !state.transactions.contains_key(&value.transaction) {
                    return Err(Error::Corruption("redacted result receipt missing"));
                }
                value.deletion
            }
            _ => return Err(Error::Corruption("redacted entity expected")),
        };
        if epoch > workspace.deletion {
            return Err(Error::Corruption("redaction exceeds deletion epoch"));
        }
    }
    for event in &state.events {
        vcp_protocol::redaction::validate_event(event)
            .map_err(|_| Error::Corruption("explicit event redaction"))?;
        if let Some(redaction) = &event.redaction {
            if redaction.deletion > epoch(state, &event.event.workspace)? {
                return Err(Error::Corruption("event redaction exceeds deletion epoch"));
            }
        }
    }
    Ok(())
}
pub(crate) fn redact_record(
    state: &State,
    source: &Record,
    deletion: DeletionEpoch,
) -> Result<Record> {
    let mut next = source.clone();
    next.value = match source.collection {
        Collection::Artifact => {
            let mut value: ArtifactDescriptor = source.decode()?;
            if value.state != CaptureState::Complete && value.state != CaptureState::Aborted {
                return Err(Error::Conflict("artifact capture is protected or purged"));
            }
            value.state = CaptureState::Purged;
            value.retained.clear();
            serde_json::to_value(value)?
        }
        Collection::Attempt => {
            let mut value: Attempt = source.decode()?;
            if value
                .redacted_at_revision
                .is_some_and(|revision| revision >= value.revision)
                || !matches!(
                    value.phase,
                    ReservationState::Settled
                        | ReservationState::Released
                        | ReservationState::ExplicitlyResolved
                )
            {
                return Err(Error::Conflict("attempt accounting protected"));
            }
            value.redaction = Some(redaction::ContentRedaction {
                deletion,
                original_digest: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(
                    &value,
                )?),
            });
            value.uncertain = None;
            value.redacted_at_revision = Some(value.revision);
            serde_json::to_value(value)?
        }
        Collection::Settlement => {
            let mut value: Settlement = source.decode()?;
            if value.redaction.is_some() || value.observation_digest.is_some() {
                return Err(Error::Conflict("settlement already redacted"));
            }
            value.redaction = Some(redaction::ContentRedaction {
                deletion,
                original_digest: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(
                    &value,
                )?),
            });
            value.observation_digest = Some(vcp_protocol::digest_bytes(&serde_json::to_vec(
                &value.observation,
            )?));
            if let Some(correction) = &mut value.observation.correction {
                correction.reason.clear();
                correction.remaining_uncertainty.clear();
            }
            serde_json::to_value(value)?
        }
        Collection::Verification => {
            let mut value: vcp_domain::verification::Verification = source.decode()?;
            if value.redaction.is_some() {
                return Err(Error::Conflict("already redacted verification"));
            }
            value.redaction = Some(vcp_domain::redaction::ContentRedaction {
                deletion,
                original_digest: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(
                    &value,
                )?),
            });
            for check in &mut value.checks {
                check.specification.clear();
                match &mut check.outcome {
                    vcp_domain::verification::CheckOutcome::Failed { reason }
                    | vcp_domain::verification::CheckOutcome::NotRun { reason } => reason.clear(),
                    _ => (),
                }
            }
            value.outstanding_issues.clear();
            if let vcp_domain::verification::CostCertainty::Uncertain { reason, .. } =
                &mut value.cost
            {
                reason.clear();
            }
            serde_json::to_value(value)?
        }
        Collection::Effect => {
            let mut value: vcp_domain::effect::Effect = source.decode()?;
            if value.redaction.is_some()
                || !matches!(
                    value.state,
                    vcp_domain::effect::EffectState::Succeeded
                        | vcp_domain::effect::EffectState::Failed
                        | vcp_domain::effect::EffectState::Cancelled
                )
            {
                return Err(Error::Conflict("effect protected"));
            }
            value.redaction = Some(vcp_domain::redaction::ContentRedaction {
                deletion,
                original_digest: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(
                    &value,
                )?),
            });
            value.reason.clear();
            serde_json::to_value(value)?
        }
        Collection::Turn => {
            let mut value: vcp_domain::task::Turn = source.decode()?;
            if value.redaction.is_some()
                || !matches!(
                    value.state,
                    vcp_domain::task::TurnState::Completed
                        | vcp_domain::task::TurnState::Failed
                        | vcp_domain::task::TurnState::Cancelled
                )
            {
                return Err(Error::Conflict("turn protected"));
            }
            value.redaction = Some(vcp_domain::redaction::ContentRedaction {
                deletion,
                original_digest: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(
                    &value,
                )?),
            });
            value.reason.clear();
            serde_json::to_value(value)?
        }
        Collection::Task => serde_json::to_value(
            vcp_protocol::redaction::task(&source.decode()?, deletion)
                .map_err(|_| Error::Conflict("task content protected"))?,
        )?,
        _ => match source.value["document_type"].as_str() {
            Some(redaction::OBSERVER_SOURCE) if source.collection == Collection::Projection => {
                let scope = crate::observer_contract::scope(source)?;
                let value = RedactedObserver {
                    document_type: redaction::OBSERVER.into(),
                    schema_version: 1,
                    id: source.id.clone(),
                    scope,
                    revision: source.revision,
                    deletion,
                    original_digest: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(
                        &source.value,
                    )?),
                };
                value.validate()?;
                serde_json::to_value(value)?
            }
            Some(vcp_domain::forecast::SOURCES) if source.collection == Collection::Projection => {
                crate::forecast_contract::shape(source)?;
                serde_json::to_value(vcp_domain::forecast::RedactedSources {
                    document_type: vcp_domain::forecast::REDACTED.into(),
                    schema_version: 1,
                    id: source.id.clone(),
                    workspace: source.workspace.clone(),
                    revision: source.revision,
                    deletion,
                    original_digest: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(
                        &source.value,
                    )?),
                })?
            }
            Some(tag)
                if source.collection == Collection::Projection
                    && redaction::advisory_document(tag) =>
            {
                if source.value["schema_version"] != 1
                    || source.value["document_version"] != 1
                    || source.value["routing_encoding"] != "object_v1"
                    || source.value["id"] != source.id
                    || source.value["workspace"] != source.workspace.as_str()
                {
                    return Err(Error::Corruption("advisory redaction source identity"));
                }
                let task_id = source.value["task"]
                    .as_str()
                    .ok_or(Error::Corruption("advisory redaction task identity"))?;
                let task: Task = state
                    .record(Collection::Task, task_id, &source.workspace)?
                    .decode()?;
                let value = RedactedAdvisory {
                    document_type: redaction::ADVISORY.into(),
                    schema_version: 1,
                    original_document_type: tag.into(),
                    id: source.id.clone(),
                    scope: task.scope,
                    revision: source.revision,
                    deletion,
                    original_digest: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(
                        &source.value,
                    )?),
                };
                value.validate()?;
                serde_json::to_value(value)?
            }
            Some(vcp_domain::memory_review::SUBMISSION | vcp_domain::memory_review::DECISION) => {
                crate::memory_review_contract::redact(state, source, deletion)?
            }
            Some("vcp_memory_proposal_v1") => serde_json::to_value(
                vcp_protocol::redaction::proposal(&source.decode::<ProposalRecord>()?, deletion)
                    .map_err(|_| Error::Corruption("proposal redaction"))?,
            )?,
            Some("vcp_memory_version_v1") => serde_json::to_value(
                vcp_protocol::redaction::version(&source.decode::<Version>()?, deletion)
                    .map_err(|_| Error::Corruption("version redaction"))?,
            )?,
            Some("vcp_memory_result_v1") => serde_json::to_value(
                vcp_protocol::redaction::result(&source.decode::<ProposalResult>()?, deletion)
                    .map_err(|_| Error::Corruption("result redaction"))?,
            )?,
            _ => {
                return Err(Error::Conflict(
                    "record has no qualified redaction contract",
                ))
            }
        },
    };
    Ok(next)
}
pub(crate) fn unprotected(
    state: &State,
    workspace: &WorkspaceId,
    task: Option<&TaskId>,
) -> Result<()> {
    let root = task
        .map(|id| {
            state
                .record(Collection::Task, id.as_str(), workspace)?
                .decode::<Task>()
                .map(|t| t.root)
        })
        .transpose()?;
    for row in state
        .records
        .values()
        .filter(|row| &row.workspace == workspace)
    {
        match row.collection {
            Collection::Task => {
                let task: Task = row.decode()?;
                if root.as_ref().is_none_or(|root| root == &task.root) && !task.state.terminal() {
                    return Err(Error::Conflict(
                        "active task recovery protects retention source",
                    ));
                }
            }
            Collection::Reservation => {
                let reservation: Reservation = row.decode()?;
                if root.as_ref().is_none_or(|root| root == &reservation.root)
                    && (!matches!(
                        reservation.phase,
                        ReservationState::Settled
                            | ReservationState::Released
                            | ReservationState::ExplicitlyResolved
                    ) || reservation.liability != Micros::ZERO)
                {
                    return Err(Error::Conflict(
                        "unsettled accounting protects retention source",
                    ));
                }
            }
            Collection::Turn => {
                let turn: vcp_domain::task::Turn = row.decode()?;
                let owner: Task = state
                    .record(Collection::Task, turn.scope.task.as_str(), workspace)?
                    .decode()?;
                if root.as_ref().is_none_or(|root| root == &owner.root)
                    && !matches!(
                        turn.state,
                        vcp_domain::task::TurnState::Completed
                            | vcp_domain::task::TurnState::Failed
                            | vcp_domain::task::TurnState::Cancelled
                    )
                {
                    return Err(Error::Conflict(
                        "active turn recovery protects retention source",
                    ));
                }
            }
            Collection::Effect => {
                let effect: Effect = row.decode()?;
                let owner: Task = state
                    .record(Collection::Task, effect.scope.task.as_str(), workspace)?
                    .decode()?;
                if root.as_ref().is_none_or(|root| root == &owner.root)
                    && !matches!(
                        effect.state,
                        vcp_domain::effect::EffectState::Succeeded
                            | vcp_domain::effect::EffectState::Failed
                            | vcp_domain::effect::EffectState::Cancelled
                    )
                {
                    return Err(Error::Conflict(
                        "unsettled effect protects retention source",
                    ));
                }
            }
            _ => (),
        }
    }
    Ok(())
}
fn epoch(state: &State, workspace: &WorkspaceId) -> Result<DeletionEpoch> {
    let workspace: Workspace = state
        .record(Collection::Workspace, workspace.as_str(), workspace)?
        .decode()?;
    if workspace.deletion == DeletionEpoch::ZERO {
        return Err(Error::Conflict(
            "retention rewrite needs committed deletion epoch",
        ));
    }
    Ok(workspace.deletion)
}
fn command_change(state: &State, before: &CommandReceipt, after: &CommandReceipt) -> Result<()> {
    if before == after {
        return Ok(());
    }
    let CommandResult::Inspection { task } = &before.result else {
        return Err(Error::Conflict(
            "only inspection command payload may redact",
        ));
    };
    unprotected(
        state,
        &before.workspace,
        task.as_ref().map(|task| &task.scope.task),
    )?;
    let mut expected = before.clone();
    expected.result =
        vcp_protocol::redaction::inspection(&before.result, epoch(state, &before.workspace)?)
            .map_err(|_| Error::Corruption("inspection redaction"))?;
    if &expected != after {
        return Err(Error::Corruption(
            "redaction changed command receipt identity",
        ));
    }
    Ok(())
}
/// Validates a redacted replay base against the still-open prior canonical view.
/// Epoch/tombstones must already be committed. Keys, ordering, IDs and opaque
/// historical transaction digests never change; only exact typed erasures may.
pub(crate) fn validate_rewrite(source: &State, target: &State) -> Result<()> {
    if source.watermark != target.watermark
        || source.sequences != target.sequences
        || source.records.keys().ne(target.records.keys())
        || source.commands.keys().ne(target.commands.keys())
        || source.transactions.keys().ne(target.transactions.keys())
        || source.events.len() != target.events.len()
    {
        return Err(Error::Corruption(
            "redaction changed canonical identity inventory",
        ));
    }
    for (key, before) in &source.records {
        let after = &target.records[key];
        if before == after {
            continue;
        }
        let scope = before.task_scope()?;
        unprotected(
            source,
            &before.workspace,
            scope.as_ref().map(|scope| &scope.task),
        )?;
        if redact_record(source, before, epoch(source, &before.workspace)?)? != *after {
            return Err(Error::Corruption("unqualified canonical record rewrite"));
        }
    }
    for (before, after) in source.events.iter().zip(&target.events) {
        if before == after {
            continue;
        }
        unprotected(source, &before.event.workspace, before.event.task.as_ref())?;
        let expected =
            vcp_protocol::redaction::event(before, epoch(source, &before.event.workspace)?)
                .map_err(|_| Error::Corruption("event redaction"))?;
        if expected != *after {
            return Err(Error::Corruption("redaction changed event identity"));
        }
    }
    for (key, before) in &source.commands {
        command_change(source, before, &target.commands[key])?;
    }
    for (key, before) in &source.transactions {
        let after = &target.transactions[key];
        let mut expected = before.clone();
        match (&before.command, &after.command) {
            (Some(a), Some(b)) => {
                command_change(source, a, b)?;
                expected.command = Some(b.clone());
            }
            (None, None) => (),
            _ => return Err(Error::Corruption("redaction changed nested receipt")),
        }
        if expected != *after {
            return Err(Error::Corruption(
                "redaction changed historical commit digest",
            ));
        }
    }
    // Both independently indexed copies of an inspection acknowledgement agree.
    for receipt in target.commands.values() {
        if target
            .transactions
            .get(&receipt.transaction)
            .and_then(|r| r.command.as_ref())
            != Some(receipt)
        {
            return Err(Error::Corruption("redaction command receipt copies differ"));
        }
    }
    target.validate()?;
    Ok(())
}
