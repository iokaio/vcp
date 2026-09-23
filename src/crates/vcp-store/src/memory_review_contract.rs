// SPDX-License-Identifier: Apache-2.0
//! Immutable manual-review evidence, distinct from automatically governed proposals.
use crate::{contract::*, Error, Result};
use std::collections::BTreeSet;
use vcp_domain::{
    memory::*,
    memory_review::*,
    task::Task,
    workspace::{Scope, Workspace},
    *,
};
use vcp_protocol::{canonical_bytes, digest_bytes, event::EventKind};

pub fn decision_id(scope: &Scope, submission: &ProposalId) -> Result<String> {
    Ok(format!(
        "{DECISION_PREFIX}{}",
        digest_bytes(&canonical_bytes(&(DECISION, scope, submission))?)
    ))
}
pub(crate) fn kind(row: &Record) -> Result<Option<&str>> {
    let tag = row.value["document_type"].as_str();
    if row.collection == Collection::Projection
        && row.id.starts_with(DECISION_PREFIX)
        && !matches!(tag, Some(DECISION | REDACTED_DECISION))
    {
        return Err(Error::Corruption("reserved manual review decision key"));
    }
    let collection = match tag {
        Some(SUBMISSION | REDACTED_SUBMISSION) => Collection::Claim,
        Some(DECISION | REDACTED_DECISION) => Collection::Projection,
        Some(tag)
            if tag.starts_with("vcp_memory_review_")
                || tag.starts_with("vcp_memory_redacted_review_") =>
        {
            return Err(Error::Incompatible)
        }
        _ => return Ok(None),
    };
    if row.collection != collection {
        return Err(Error::Corruption("manual review collection"));
    }
    Ok(tag)
}
pub(crate) fn scope(row: &Record) -> Result<Scope> {
    Ok(match kind(row)? {
        Some(SUBMISSION) => row.decode::<Submission>()?.scope,
        Some(DECISION) => row.decode::<Decision>()?.scope,
        Some(REDACTED_SUBMISSION) => row.decode::<RedactedSubmission>()?.scope,
        Some(REDACTED_DECISION) => row.decode::<RedactedDecision>()?.scope,
        _ => return Err(Error::Corruption("manual review document required")),
    })
}
pub(crate) fn shape(row: &Record) -> Result<()> {
    let (id, scope, revision) = match kind(row)? {
        Some(SUBMISSION) => {
            let v: Submission = row.decode()?;
            v.validate()?;
            if v.candidate_digest != digest_bytes(&canonical_bytes(&v.candidate)?) {
                return Err(Error::Corruption("manual candidate digest"));
            }
            (v.id.to_string(), v.scope, v.revision)
        }
        Some(DECISION) => {
            let v: Decision = row.decode()?;
            v.validate()?;
            if v.id != decision_id(&v.scope, &v.submission)? {
                return Err(Error::Corruption("manual decision key"));
            }
            (v.id, v.scope, v.revision)
        }
        Some(REDACTED_SUBMISSION) => {
            let v: RedactedSubmission = row.decode()?;
            v.validate()?;
            (v.id.to_string(), v.scope, v.revision)
        }
        Some(REDACTED_DECISION) => {
            let v: RedactedDecision = row.decode()?;
            v.validate()?;
            if v.id != decision_id(&v.scope, &v.submission)? {
                return Err(Error::Corruption("redacted manual decision key"));
            }
            (v.id, v.scope, v.revision)
        }
        _ => return Err(Error::Corruption("manual review document required")),
    };
    if id != row.id || scope.workspace != row.workspace || revision != row.revision {
        return Err(Error::Corruption("manual review identity"));
    }
    Ok(())
}
fn sources(submission: &Submission) -> vcp_domain::redaction::Sources {
    vcp_protocol::redaction::review_sources(submission)
}

fn source_refs(refs: &mut BTreeSet<String>, sources: &vcp_domain::redaction::Sources) {
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
    let scope = scope(row)?;
    let mut refs = BTreeSet::from([
        key(Collection::Workspace, scope.workspace.as_str()),
        key(Collection::Session, scope.session.as_str()),
        key(Collection::Task, scope.task.as_str()),
    ]);
    match kind(row)? {
        Some(SUBMISSION) => source_refs(&mut refs, &sources(&row.decode::<Submission>()?)),
        Some(REDACTED_SUBMISSION) => {
            source_refs(&mut refs, &row.decode::<RedactedSubmission>()?.sources)
        }
        Some(DECISION) => {
            let v: Decision = row.decode()?;
            refs.insert(key(Collection::Claim, v.submission.as_str()));
            refs.extend(
                v.governed_proposal
                    .iter()
                    .map(|id| key(Collection::Claim, id.as_str())),
            );
            refs.extend(
                v.resolution
                    .conflicts
                    .iter()
                    .map(|id| key(Collection::Claim, id.as_str())),
            );
            if v.governed_proposal.is_some() {
                refs.insert(key(Collection::Projection, v.command.as_str()));
            }
        }
        Some(REDACTED_DECISION) => {
            let v: RedactedDecision = row.decode()?;
            refs.insert(key(Collection::Claim, v.submission.as_str()));
            source_refs(&mut refs, &v.sources);
            refs.extend(
                v.governed_proposal
                    .iter()
                    .map(|id| key(Collection::Claim, id.as_str())),
            );
            if v.governed_proposal.is_some() {
                refs.insert(key(Collection::Projection, v.command.as_str()));
            }
        }
        _ => return Err(Error::Corruption("manual review references")),
    }
    Ok(refs)
}
pub(crate) fn validate(state: &State, row: &Record) -> Result<()> {
    match kind(row)? {
        Some(SUBMISSION) => {
            let v: Submission = row.decode()?;
            for origin in &v.candidate.origins {
                if !state.events.iter().any(|e| {
                    e.event.id == *origin
                        && e.event.workspace == v.scope.workspace
                        && e.event.session == v.scope.session
                }) {
                    return Err(Error::Corruption("manual submission origin scope"));
                }
            }
            for evidence in &v.candidate.evidence {
                let artifact: vcp_domain::artifact::ArtifactDescriptor = state
                    .record(
                        Collection::Artifact,
                        evidence.artifact.as_str(),
                        &v.scope.workspace,
                    )?
                    .decode()?;
                if artifact.spec.scope.session != v.scope.session
                    || artifact.sha256 != evidence.sha256
                    || evidence
                        .range
                        .as_ref()
                        .is_some_and(|r| r.end.get() > artifact.length.get())
                {
                    return Err(Error::Corruption("manual submission evidence identity"));
                }
            }
        }
        Some(DECISION) => {
            let v: Decision = row.decode()?;
            let submission: Submission = state
                .record(Collection::Claim, v.submission.as_str(), &row.workspace)?
                .decode()?;
            v.validate_submission(&submission)?;
            if let Some(id) = &v.governed_proposal {
                let proposal: ProposalRecord = state
                    .record(Collection::Claim, id.as_str(), &row.workspace)?
                    .decode()?;
                let mut expected = submission.candidate.clone();
                expected.id = id.clone();
                expected.command = v.command.clone();
                expected.actor = v.actor.clone();
                expected.epochs = v.epochs.clone();
                if proposal.proposal != expected
                    || proposal.resolution != v.resolution
                    || proposal.recorded_at != v.recorded_at
                    || proposal.payload_digest != digest_bytes(&canonical_bytes(&expected)?)
                {
                    return Err(Error::Corruption("manual decision governed candidate"));
                }
                let result: ProposalResult = state
                    .record(Collection::Projection, v.command.as_str(), &row.workspace)?
                    .decode()?;
                if result.proposal != *id
                    || result.resolution != v.resolution
                    || result.scope != v.scope
                {
                    return Err(Error::Corruption("manual decision governed result"));
                }
            }
        }
        Some(REDACTED_DECISION) => {
            let v: RedactedDecision = row.decode()?;
            let source = state.record(Collection::Claim, v.submission.as_str(), &row.workspace)?;
            let (scope, digest) = match kind(source)? {
                Some(SUBMISSION) => {
                    let s: Submission = source.decode()?;
                    (s.scope, s.candidate_digest)
                }
                Some(REDACTED_SUBMISSION) => {
                    let s: RedactedSubmission = source.decode()?;
                    (s.scope, s.candidate_digest)
                }
                _ => return Err(Error::Corruption("redacted manual submission type")),
            };
            if scope != v.scope || digest != v.submission_digest {
                return Err(Error::Corruption("redacted manual decision linkage"));
            }
        }
        _ => (),
    }
    Ok(())
}
fn current(
    state: &State,
    scope: &Scope,
    revision: Revision,
    steering: SteeringRevision,
    epochs: &Epochs,
    head: &Option<ClaimVersionId>,
    claim: &ClaimId,
) -> Result<()> {
    let workspace: Workspace = state
        .record(
            Collection::Workspace,
            scope.workspace.as_str(),
            &scope.workspace,
        )?
        .decode()?;
    let task: Task = state
        .record(Collection::Task, scope.task.as_str(), &scope.workspace)?
        .decode()?;
    if task.scope != *scope
        || task.revision != revision
        || task.steering != steering
        || task.redaction.is_some()
        || workspace.authority != epochs.authority
        || workspace.deletion != epochs.deletion
    {
        return Err(Error::Conflict("stale manual review source"));
    }
    let policy = match state
        .records
        .get(&key(Collection::Access, scope.workspace.as_str()))
    {
        None => PolicyRevision::ZERO,
        Some(row) => match row.decode::<vcp_domain::policy::AuthorityDocument>()?.data {
            vcp_domain::policy::AuthorityData::Policy { policy } => policy.revision,
            _ => return Err(Error::Corruption("manual review policy")),
        },
    };
    if policy != epochs.policy {
        return Err(Error::Conflict("stale manual review policy"));
    }
    let actual = state
        .records
        .get(&key(Collection::Projection, claim.as_str()))
        .map(|r| r.decode::<Head>())
        .transpose()?
        .and_then(|h| h.current);
    if actual != *head {
        return Err(Error::Conflict("stale manual review claim head"));
    }
    Ok(())
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    schema_version: u32,
    action: String,
    id: String,
    record_digest: String,
}
pub(crate) fn transaction(state: &State, tx: &Transaction) -> Result<()> {
    let records: Vec<_> = tx
        .mutations
        .iter()
        .filter_map(|m| {
            if let Mutation::Put { record, .. } = m {
                Some(record)
            } else {
                None
            }
        })
        .collect();
    let reviews: Vec<_> = records
        .iter()
        .filter(|r| {
            matches!(
                r.value["document_type"].as_str(),
                Some(SUBMISSION | DECISION)
            )
        })
        .copied()
        .collect();
    let marked = tx
        .events
        .iter()
        .any(|e| e.data.get("memory_review").is_some());
    let manual = records.iter().any(|r| {
        r.value["document_type"] == "vcp_memory_proposal_v1"
            && r.value["proposal"]["extractor"] == MANUAL_EXTRACTOR
    });
    if reviews.is_empty() {
        if marked || manual {
            return Err(Error::Conflict("manual review decision required"));
        }
        return Ok(());
    }
    if reviews.len() != 1 || tx.events.len() != 1 || records.len() != tx.mutations.len() {
        return Err(Error::Conflict("manual review atomic transaction"));
    }
    let row = reviews[0];
    shape(row)?;
    if state.records.contains_key(&row.key()) {
        return Err(Error::Conflict("manual review immutable identity"));
    }
    let mut accepted_revision = Revision::ZERO;
    let (scope, command, digest, actor, time, action) = if row.value["document_type"] == SUBMISSION
    {
        let v: Submission = row.decode()?;
        if records.len() != 1 {
            return Err(Error::Conflict("submission cannot govern memory"));
        }
        current(
            state,
            &v.scope,
            v.task_revision,
            v.steering,
            &v.candidate.epochs,
            &v.expected_head,
            &v.candidate.claim,
        )?;
        (
            v.scope,
            v.candidate.command,
            v.command_digest,
            v.candidate.actor,
            v.recorded_at,
            "submission",
        )
    } else {
        let v: Decision = row.decode()?;
        let submission: Submission = state
            .record(Collection::Claim, v.submission.as_str(), &row.workspace)?
            .decode()?;
        v.validate_submission(&submission)?;
        if v.expected_head != submission.expected_head {
            return Err(Error::Conflict("manual decision changed expected head"));
        }
        current(
            state,
            &v.scope,
            v.task_revision,
            v.steering,
            &v.epochs,
            &v.expected_head,
            &submission.candidate.claim,
        )?;
        match v.choice {
            Choice::Reject if records.len() != 1 => {
                return Err(Error::Conflict("rejection cannot create governed state"))
            }
            Choice::Accept => {
                let id = v
                    .governed_proposal
                    .as_ref()
                    .ok_or(Error::Corruption("missing governed proposal"))?;
                let proposal = records
                    .iter()
                    .find(|r| r.collection == Collection::Claim && r.id == id.as_str())
                    .ok_or(Error::Conflict("governed proposal must be atomic"))?;
                let p: ProposalRecord = proposal.decode()?;
                let result = records
                    .iter()
                    .find(|r| r.collection == Collection::Projection && r.id == v.command.as_str())
                    .ok_or(Error::Conflict("governed result must be atomic"))?;
                let result: ProposalResult = result.decode()?;
                if p.resolution != v.resolution
                    || result.resolution != v.resolution
                    || result.transaction != tx.id
                    || result.proposal != *id
                {
                    return Err(Error::Conflict("manual governed resolution mismatch"));
                }
                let sequence: MemoryHead = records
                    .iter()
                    .find(|r| {
                        r.collection == Collection::Projection && r.id == v.scope.workspace.as_str()
                    })
                    .ok_or(Error::Conflict("manual governed sequence required"))?
                    .decode()?;
                let prior_sequence = state
                    .records
                    .get(&key(Collection::Projection, v.scope.workspace.as_str()))
                    .map(|r| r.decode::<MemoryHead>())
                    .transpose()?
                    .map_or(MemorySeq::ZERO, |s| s.sequence);
                accepted_revision = sequence.revision;
                if sequence.sequence != prior_sequence.next()?
                    || result.memory_seq != sequence.sequence
                {
                    return Err(Error::Conflict("manual governed sequence linkage"));
                }
                if let (Some(version_id), Some(intent_id)) = (&result.version, &result.intent) {
                    let version: Version = records
                        .iter()
                        .find(|r| r.collection == Collection::Claim && r.id == version_id.as_str())
                        .ok_or(Error::Conflict("manual governed version required"))?
                        .decode()?;
                    let intent: IndexIntent = records
                        .iter()
                        .find(|r| {
                            r.collection == Collection::IndexIntent && r.id == intent_id.as_str()
                        })
                        .ok_or(Error::Conflict("manual governed intent required"))?
                        .decode()?;
                    let head: Head = records
                        .iter()
                        .find(|r| {
                            r.collection == Collection::Projection
                                && r.id == submission.candidate.claim.as_str()
                        })
                        .ok_or(Error::Conflict("manual governed head required"))?
                        .decode()?;
                    let previous = state
                        .records
                        .get(&key(
                            Collection::Projection,
                            submission.candidate.claim.as_str(),
                        ))
                        .map(|r| r.decode::<Head>())
                        .transpose()?;
                    let mut expected_head = previous.unwrap_or(Head {
                        document_type: DocumentType::Head,
                        schema_version: 1,
                        id: submission.candidate.claim.clone(),
                        scope: v.scope.clone(),
                        revision: Revision::ZERO,
                        current: None,
                        disputed: vec![],
                    });
                    expected_head.revision = head.revision;
                    if v.resolution.outcome == Outcome::Accepted {
                        expected_head.current = Some(version_id.clone());
                    } else {
                        expected_head.disputed.push(version_id.clone());
                    }
                    let supersedes = if v.resolution.outcome == Outcome::Accepted {
                        submission
                            .candidate
                            .predecessor
                            .clone()
                            .into_iter()
                            .collect::<Vec<_>>()
                    } else {
                        vec![]
                    };
                    if head != expected_head
                        || version.memory_seq != sequence.sequence
                        || version.canonical_watermark != state.watermark.next()?
                        || version.recorded_at != v.recorded_at
                        || intent.transaction != tx.id
                        || intent.memory_seq != sequence.sequence
                        || intent.canonical_watermark != state.watermark.next()?
                        || intent.scope != v.scope
                        || intent.versions != vec![version_id.clone()]
                        || intent.supersedes != supersedes
                        || intent.status != IndexStatus::Pending
                        || intent.deletion.is_some()
                    {
                        return Err(Error::Conflict(
                            "manual governed version head intent linkage",
                        ));
                    }
                }
                let mut allowed = BTreeSet::from([
                    row.key(),
                    proposal.key(),
                    key(Collection::Projection, v.command.as_str()),
                    key(Collection::Projection, v.scope.workspace.as_str()),
                ]);
                if let Some(version) = &result.version {
                    allowed.insert(key(Collection::Claim, version.as_str()));
                    allowed.insert(key(
                        Collection::Projection,
                        submission.candidate.claim.as_str(),
                    ));
                }
                if let Some(intent) = &result.intent {
                    allowed.insert(key(Collection::IndexIntent, intent.as_str()));
                }
                if records.len() != allowed.len()
                    || records.iter().any(|r| !allowed.contains(&r.key()))
                {
                    return Err(Error::Conflict("unrelated manual decision mutation"));
                }
            }
            _ => (),
        }
        (
            v.scope,
            v.command,
            v.command_digest,
            v.actor,
            v.recorded_at,
            "decision",
        )
    };
    let receipt = tx
        .command
        .as_ref()
        .ok_or(Error::Conflict("manual review public receipt required"))?;
    let event = &tx.events[0];
    let marker: Marker = serde_json::from_value(event.data["memory_review"].clone())?;
    if marker.schema_version != 1
        || marker.action != action
        || marker.id != row.id
        || marker.record_digest != digest_bytes(&canonical_bytes(&row.value)?)
        || event.data["schema_version"] != 1
        || event.kind != EventKind::MemoryResolved
        || event.workspace != scope.workspace
        || event.session != scope.session
        || event.task.as_ref() != Some(&scope.task)
        || event.actor != actor
        || event.correlation != command
        || event.timestamp != time
        || receipt.workspace != scope.workspace
        || receipt.session != scope.session
        || receipt.command != command
        || receipt.digest != digest
        || receipt.result
            != (vcp_protocol::command::CommandResult::Accepted {
                revision: accepted_revision,
            })
    {
        return Err(Error::Conflict("manual review receipt event identity"));
    }
    Ok(())
}

pub(crate) fn redact(
    state: &State,
    row: &Record,
    deletion: DeletionEpoch,
) -> Result<serde_json::Value> {
    match kind(row)? {
        Some(SUBMISSION) => Ok(serde_json::to_value(
            vcp_protocol::redaction::review_submission(&row.decode::<Submission>()?, deletion)
                .map_err(|_| Error::Corruption("manual submission redaction"))?,
        )?),
        Some(DECISION) => {
            let decision: Decision = row.decode()?;
            let row = state.record(
                Collection::Claim,
                decision.submission.as_str(),
                &row.workspace,
            )?;
            let sources = if kind(row)? == Some(SUBMISSION) {
                sources(&row.decode::<Submission>()?)
            } else {
                row.decode::<RedactedSubmission>()?.sources
            };
            Ok(serde_json::to_value(
                vcp_protocol::redaction::review_decision(&decision, &sources, deletion)
                    .map_err(|_| Error::Corruption("manual decision redaction"))?,
            )?)
        }
        _ => Err(Error::Conflict("manual review redaction type")),
    }
}

#[cfg(test)]
#[path = "memory_review_tests.rs"]
mod tests;
