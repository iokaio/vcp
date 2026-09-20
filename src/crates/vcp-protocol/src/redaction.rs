// SPDX-License-Identifier: Apache-2.0
//! Exact content erasure transforms, only admitted by canonical retention rewrite.
use crate::{canonical_bytes, command::CommandResult, digest_bytes, event::EventEnvelope};
use vcp_domain::{redaction::ContentRedaction, task::Task, *};
type Result<T> = std::result::Result<T, String>;
fn metadata<T: serde::Serialize>(value: &T, deletion: DeletionEpoch) -> Result<ContentRedaction> {
    let result = ContentRedaction {
        deletion,
        original_digest: digest_bytes(&canonical_bytes(value).map_err(|e| e.to_string())?),
    };
    result.validate().map_err(|e| e.to_string())?;
    Ok(result)
}
pub fn task(source: &Task, deletion: DeletionEpoch) -> Result<Task> {
    if !source.state.terminal() || source.redaction.is_some() {
        return Err("only retained terminal task content may be redacted".into());
    }
    let mut result = source.clone();
    result.redaction = Some(metadata(source, deletion)?);
    result.objectives.clear();
    result.required_checks.clear();
    result.reason.clear();
    result.validate().map_err(|e| e.to_string())?;
    Ok(result)
}
pub fn event(source: &EventEnvelope, deletion: DeletionEpoch) -> Result<EventEnvelope> {
    if source.redaction.is_some() {
        return Err("event payload already redacted".into());
    }
    let mut result = source.clone();
    result.redaction = Some(metadata(
        &(&source.event.data, &source.event.metadata),
        deletion,
    )?);
    result.event.data = serde_json::Value::Null;
    result.event.metadata = None;
    Ok(result)
}
pub fn validate_event(value: &EventEnvelope) -> Result<()> {
    if let Some(redaction) = &value.redaction {
        redaction.validate().map_err(|e| e.to_string())?;
        if !value.event.data.is_null() || value.event.metadata.is_some() {
            return Err("redacted event retains content".into());
        }
    }
    Ok(())
}
pub fn inspection(source: &CommandResult, deletion: DeletionEpoch) -> Result<CommandResult> {
    let CommandResult::Inspection { task } = source else {
        return Err("only inspection snapshots may redact their result".into());
    };
    let metadata = metadata(source, deletion)?;
    Ok(CommandResult::InspectionRedacted {
        task: task.as_ref().map(|t| t.scope.task.clone()),
        deletion,
        original_payload_digest: metadata.original_digest,
    })
}

pub fn origin_output_key(extractor: &str, output_key: &str, origin: &EventId) -> Result<String> {
    Ok(digest_bytes(
        &canonical_bytes(&("redacted-origin-output/1", extractor, output_key, origin))
            .map_err(|e| e.to_string())?,
    ))
}
fn sources(
    proposal: &vcp_domain::memory::Proposal,
    resolution: &vcp_domain::memory::Resolution,
) -> vcp_domain::redaction::Sources {
    use std::collections::BTreeSet;
    let accepted = matches!(
        resolution.outcome,
        vcp_domain::memory::Outcome::Accepted | vcp_domain::memory::Outcome::Disputed
    );
    vcp_domain::redaction::Sources {
        origins: proposal.origins.clone(),
        artifacts: resolution.validated_evidence.clone(),
        verifications: proposal
            .evidence
            .iter()
            .filter(|e| resolution.validated_evidence.contains(&e.artifact))
            .filter_map(|e| e.verification.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        versions: resolution
            .conflicts
            .iter()
            .cloned()
            .chain(proposal.predecessor.iter().filter(|_| accepted).cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    }
}
pub fn proposal(
    source: &vcp_domain::memory::ProposalRecord,
    deletion: DeletionEpoch,
) -> Result<vcp_domain::redaction::RedactedProposal> {
    use vcp_domain::redaction::*;
    let result = RedactedProposal {
        document_type: PROPOSAL.into(),
        schema_version: 1,
        id: source.id.clone(),
        scope: source.scope.clone(),
        revision: source.revision,
        deletion,
        original_digest: metadata(source, deletion)?.original_digest,
        payload_digest: source.payload_digest.clone(),
        command: source.proposal.command.clone(),
        claim: source.proposal.claim.clone(),
        actor: source.proposal.actor.clone(),
        sources: sources(&source.proposal, &source.resolution),
        predecessor: source.proposal.predecessor.clone(),
        outcome: source.resolution.outcome,
        recorded_at: source.recorded_at,
        extractor_digest: crate::digest_bytes(source.proposal.extractor.as_bytes()),
        origin_output_keys: source
            .proposal
            .origins
            .iter()
            .map(|origin| {
                origin_output_key(
                    &source.proposal.extractor,
                    &source.proposal.output_key,
                    origin,
                )
            })
            .collect::<Result<Vec<_>>>()?,
    };
    result.validate().map_err(|e| e.to_string())?;
    Ok(result)
}
pub fn version(
    source: &vcp_domain::memory::Version,
    deletion: DeletionEpoch,
) -> Result<vcp_domain::redaction::RedactedVersion> {
    use vcp_domain::redaction::*;
    let result = RedactedVersion {
        document_type: VERSION.into(),
        schema_version: 1,
        id: source.id.clone(),
        scope: source.scope.clone(),
        revision: source.revision,
        deletion,
        original_digest: metadata(source, deletion)?.original_digest,
        proposal: source.proposal.id.clone(),
        claim: source.proposal.claim.clone(),
        predecessor: source.proposal.predecessor.clone(),
        sources: sources(&source.proposal, &source.resolution),
        outcome: source.resolution.outcome,
        memory_seq: source.memory_seq,
        canonical_watermark: source.canonical_watermark,
        recorded_at: source.recorded_at,
    };
    result.validate().map_err(|e| e.to_string())?;
    Ok(result)
}
pub fn result(
    source: &vcp_domain::memory::ProposalResult,
    deletion: DeletionEpoch,
) -> Result<vcp_domain::redaction::RedactedResult> {
    use vcp_domain::redaction::*;
    let result = RedactedResult {
        document_type: RESULT.into(),
        schema_version: 1,
        id: source.id.clone(),
        scope: source.scope.clone(),
        revision: source.revision,
        deletion,
        original_digest: metadata(source, deletion)?.original_digest,
        proposal: source.proposal.clone(),
        payload_digest: source.payload_digest.clone(),
        transaction: source.transaction.clone(),
        version: source.version.clone(),
        intent: source.intent.clone(),
        outcome: source.resolution.outcome,
        memory_seq: source.memory_seq,
    };
    result.validate().map_err(|e| e.to_string())?;
    Ok(result)
}
