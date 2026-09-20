// SPDX-License-Identifier: Apache-2.0
use crate::{
    access::{self, Access},
    gates::{self, EvidenceAvailability as Availability, EvidenceObservation, GovernanceContext},
    Error, Result,
};
use serde::Serialize;
use vcp_domain::{
    artifact::{ArtifactDescriptor, CaptureState},
    ids::*,
    memory::*,
    revision::*,
    task::Task,
    verification::Verification,
    workspace::Workspace,
};
use vcp_protocol::{
    canonical_bytes,
    command::CommandResult,
    digest_bytes,
    event::{EventInput, EventKind},
};
use vcp_store::{contract::*, Store};

pub struct MemoryCommit {
    pub receipt: Receipt,
    pub result: ProposalResult,
    pub indexing: Option<IndexStatus>,
}

fn put<T: Serialize>(
    collection: Collection,
    id: &str,
    workspace: &WorkspaceId,
    revision: Revision,
    expected: Option<Revision>,
    value: &T,
) -> Result<Mutation> {
    Ok(Mutation::Put {
        expected,
        record: Record::typed(collection, id, workspace.clone(), revision, value)?,
    })
}
fn rejection(rule: &str, message: &str) -> Resolution {
    Resolution {
        outcome: Outcome::Rejected,
        evidence_status: EvidenceStatus::Unverified,
        findings: vec![Finding {
            rule: rule.into(),
            severity: Severity::Block,
            message: message.into(),
            versions: vec![],
            evidence: vec![],
        }],
        conflicts: vec![],
        validated_evidence: vec![],
    }
}
pub(crate) fn versions(state: &State, workspace: &WorkspaceId) -> Result<Vec<Version>> {
    state
        .records
        .values()
        .filter(|r| {
            r.workspace == *workspace
                && r.collection == Collection::Claim
                && r.value["document_type"] == "vcp_memory_version_v1"
        })
        .map(|r| r.decode().map_err(Error::from))
        .collect()
}
fn current(state: &State, access: &Access) -> Result<Vec<Version>> {
    let mut result = Vec::new();
    for record in state.records.values().filter(|r| {
        r.workspace == access.workspace
            && r.collection == Collection::Projection
            && r.value["document_type"] == "vcp_memory_head_v1"
    }) {
        let head: Head = record.decode()?;
        if let Some(id) = head.current {
            let version: Version = state
                .record(Collection::Claim, id.as_str(), &access.workspace)?
                .decode()?;
            // Hidden overlapping claims cannot silently disappear from conflict checks.
            if !access.allows_task(&version.scope.task) {
                return Err(Error::Access);
            }
            if crate::history::removed(state, &access.workspace, &version)? {
                continue;
            }
            access::version_scope(state, access, &version)?;
            result.push(version);
        }
    }
    Ok(result)
}
pub(crate) fn evidence(
    store: &Store,
    access: &Access,
    proposal: &Proposal,
) -> Result<Vec<EvidenceObservation>> {
    proposal
        .evidence
        .iter()
        .map(|reference| {
            let record = store
                .state()
                .records
                .get(&key(Collection::Artifact, reference.artifact.as_str()));
            let Some(record) = record else {
                return Ok(EvidenceObservation {
                    artifact: reference.artifact.clone(),
                    status: Availability::Missing,
                    verification_current: false,
                });
            };
            if record.workspace != access.workspace {
                return Ok(EvidenceObservation {
                    artifact: reference.artifact.clone(),
                    status: Availability::InvalidScope,
                    verification_current: false,
                });
            }
            let descriptor: ArtifactDescriptor = record.decode()?;
            if !access.allows_task(&descriptor.spec.scope.task) {
                return Ok(EvidenceObservation {
                    artifact: reference.artifact.clone(),
                    status: Availability::InvalidScope,
                    verification_current: false,
                });
            }
            let status = if descriptor.sha256 != reference.sha256
                || reference
                    .range
                    .as_ref()
                    .is_some_and(|r| r.end > descriptor.length || r.start >= r.end)
            {
                Availability::DigestMismatch
            } else if descriptor.state != CaptureState::Complete
                || !descriptor.spec.omissions.is_empty()
            {
                Availability::Unavailable
            } else {
                match vcp_audit::history::History::read_artifact(
                    store,
                    &access.history(),
                    &reference.artifact,
                    std::io::sink(),
                ) {
                    Ok(_) => Availability::Available,
                    Err(vcp_audit::Error::Access) => Availability::InvalidScope,
                    Err(vcp_audit::Error::Removed) => Availability::Unavailable,
                    Err(
                        vcp_audit::Error::Integrity(_)
                        | vcp_audit::Error::Store(vcp_store::Error::Corruption(_)),
                    ) => Availability::DigestMismatch,
                    Err(_) => Availability::Unavailable,
                }
            };
            let verification_current = match &reference.verification {
                None => false,
                Some(id) => verify_evidence(store, access, proposal, reference, id)?,
            };
            Ok(EvidenceObservation {
                artifact: reference.artifact.clone(),
                status,
                verification_current,
            })
        })
        .collect()
}
fn verify_evidence(
    store: &Store,
    access: &Access,
    proposal: &Proposal,
    reference: &EvidenceRef,
    id: &VerificationId,
) -> Result<bool> {
    let state = store.state();
    let Some(record) = state
        .records
        .get(&key(Collection::Verification, id.as_str()))
    else {
        return Ok(false);
    };
    if record.workspace != access.workspace {
        return Ok(false);
    }
    let verification: Verification = record.decode()?;
    if !access.allows_task(&verification.scope.task) {
        return Ok(false);
    }
    let task: Task = state
        .record(
            Collection::Task,
            verification.scope.task.as_str(),
            &access.workspace,
        )?
        .decode()?;
    if !verification.applies(&task.scope, task.steering, &task.fingerprint)
        || reference.source.as_ref() != Some(&verification.fingerprint)
        || proposal.applicability.fingerprint.as_ref() != Some(&verification.fingerprint)
        || !(verification.outputs.contains(&reference.artifact)
            || verification
                .checks
                .iter()
                .any(|c| c.output == reference.artifact))
    {
        return Ok(false);
    }
    Ok(match &proposal.value {
        ClaimValue::VerifiedFix {
            verification: expected,
            after,
            ..
        } => {
            expected == id
                && after == &verification.fingerprint
                && verification.satisfies(&task.required_checks, true)
                && crate::fix_proof::matches(store, access, proposal)?
        }
        ClaimValue::Command {
            verification: Some(expected),
            outcome: Some(outcome),
            ..
        } => {
            expected == id
                && crate::proof::command_matches(store, access, proposal, reference)?
                && verification
                    .checks
                    .iter()
                    .any(|c| c.output == reference.artifact && &c.outcome == outcome)
        }
        _ => false,
    })
}
fn validate_origins(
    state: &State,
    access: &Access,
    proposal: &Proposal,
    workspace: &Workspace,
) -> Result<Option<Resolution>> {
    if proposal.applicability.repository != workspace.binding.repository
        || proposal.applicability.worktree != workspace.binding.worktree
        || proposal
            .applicability
            .roots
            .iter()
            .any(|r| r.as_str() != workspace.id.as_str())
    {
        return Ok(Some(rejection(
            "vcp.binding",
            "applicability differs from the current registered workspace binding",
        )));
    }
    for origin in &proposal.origins {
        let Some(event) = state.events.iter().find(|e| &e.event.id == origin) else {
            return Ok(Some(rejection(
                "vcp.origin",
                "origin event is not retained",
            )));
        };
        if event.event.workspace != access.workspace
            || event
                .event
                .task
                .as_ref()
                .is_some_and(|id| !access.allows_task(id))
        {
            return Err(Error::Access);
        }
    }
    if let ClaimValue::UserPreference {
        explicit_origin, ..
    } = &proposal.value
    {
        if !proposal.origins.contains(explicit_origin) {
            return Ok(Some(rejection(
                "vcp.preference",
                "explicit user statement must be an origin event",
            )));
        }
        // Explicit user input is captured as objective creation/steering, not a model assertion.
        if !state.events.iter().any(|e| {
            e.event.id == *explicit_origin
                && matches!(
                    e.event.kind,
                    EventKind::TaskCreated | EventKind::ObjectiveChanged
                )
        }) {
            return Ok(Some(rejection(
                "vcp.preference",
                "preference lacks explicit user task/steering evidence",
            )));
        }
    }
    for id in proposal.value.artifacts() {
        if !proposal.evidence.iter().any(|e| &e.artifact == id) {
            return Ok(Some(rejection(
                "vcp.registry-evidence",
                "structured claim source is not in its evidence allowlist",
            )));
        }
    }
    if let ClaimValue::ModuleRelationship { from, to, .. } = &proposal.value {
        for endpoint in [from, to] {
            if endpoint.root.as_str() != workspace.id.as_str()
                || !proposal
                    .evidence
                    .iter()
                    .any(|e| e.artifact == endpoint.artifact && e.sha256 == endpoint.sha256)
            {
                return Ok(Some(rejection(
                    "vcp.module-source",
                    "module endpoint differs from its scoped immutable source",
                )));
            }
        }
    }
    Ok(None)
}

/// One durable transaction includes resolution, immutable version, head, event,
/// indexing intent and retry result. There is no external effect to replay.
pub async fn propose(
    store: &mut Store,
    access: &Access,
    proposal: Proposal,
    now: Timestamp,
) -> Result<MemoryCommit> {
    if canonical_bytes(&proposal)?.len() > 256 * 1024 {
        return Err(Error::Invalid("proposal exceeds 256 KiB".into()));
    }
    let digest = digest_bytes(&canonical_bytes(&proposal)?);
    for _ in 0..3 {
        let workspace = access::authorize(store.state(), access, true)?;
        if proposal.scope.workspace != access.workspace
            || proposal.actor != access.actor
            || !access.allows_task(&proposal.scope.task)
            || proposal.epochs.authority != workspace.authority
            || proposal.epochs.deletion != workspace.deletion
            || proposal.epochs.policy != access::policy(store.state(), &access.workspace)?
        {
            return Err(Error::Access);
        }
        let task: Task = store
            .state()
            .record(
                Collection::Task,
                proposal.scope.task.as_str(),
                &access.workspace,
            )?
            .decode()?;
        if task.scope != proposal.scope {
            return Err(Error::Access);
        }
        if crate::history::proposal_removed(store.state(), &access.workspace, &proposal)? {
            return Err(Error::Access);
        }
        // The stable origin/extractor/output mapping survives lost acknowledgements.
        for row in store.state().records.values().filter(|r| {
            r.workspace == access.workspace
                && r.collection == Collection::Claim
                && r.value["document_type"] == "vcp_memory_proposal_v1"
        }) {
            let previous: ProposalRecord = row.decode()?;
            let same_origin = previous.proposal.origins == proposal.origins
                && previous.proposal.extractor == proposal.extractor
                && previous.proposal.output_key == proposal.output_key;
            if previous.id == proposal.id
                || previous.proposal.command == proposal.command
                || same_origin
            {
                if previous.payload_digest != digest {
                    return Err(Error::Conflict(
                        "proposal/origin identity reused with different content",
                    ));
                }
                let result: ProposalResult = store
                    .state()
                    .record(
                        Collection::Projection,
                        previous.proposal.command.as_str(),
                        &access.workspace,
                    )?
                    .decode()?;
                // Rejected allegations may deliberately name nonexistent or
                // foreign evidence. Their bounded rejection receipt contains
                // no derived claim; retain exact retry behavior for them.
                if result.resolution.outcome != Outcome::Rejected {
                    access::proposal_scope(store.state(), access, &previous.proposal)?;
                }
                access::resolution_scope(store.state(), access, &result.resolution)?;
                let receipt = store
                    .state()
                    .transactions
                    .get(&result.transaction)
                    .ok_or(Error::Conflict("missing canonical memory receipt"))?
                    .clone();
                let indexing = result
                    .intent
                    .as_ref()
                    .map(|id| {
                        store
                            .state()
                            .record(Collection::IndexIntent, id.as_str(), &access.workspace)
                            .and_then(Record::decode::<IndexIntent>)
                            .map(|i| i.status)
                    })
                    .transpose()?;
                return Ok(MemoryCommit {
                    receipt,
                    result,
                    indexing,
                });
            }
        }
        crate::projections::rebuild(store, access, now).await?;
        let head: Option<Head> = store
            .state()
            .records
            .get(&key(Collection::Projection, proposal.claim.as_str()))
            .map(Record::decode)
            .transpose()?;
        if head
            .as_ref()
            .is_some_and(|h| h.scope.workspace != access.workspace)
        {
            return Err(Error::Access);
        }
        let sequence: Option<MemoryHead> = store
            .state()
            .records
            .get(&key(Collection::Projection, access.workspace.as_str()))
            .map(Record::decode)
            .transpose()?;
        let next_seq = sequence
            .as_ref()
            .map_or(MemorySeq::ZERO, |s| s.sequence)
            .next()?;
        let context = GovernanceContext {
            workspace: access.workspace.clone(),
            current: current(store.state(), access)?,
            evidence: evidence(store, access, &proposal)?,
            head: head.as_ref().and_then(|h| h.current.clone()),
        };
        let resolution = match validate_origins(store.state(), access, &proposal, &workspace)? {
            Some(rejected) => rejected,
            None => gates::evaluate(&proposal, &context).map_err(Error::Invalid)?,
        };
        let tx = TransactionId::new();
        let watermark = store.state().watermark.next()?;
        let mut mutations = Vec::new();
        let recorded = ProposalRecord {
            document_type: DocumentType::Proposal,
            schema_version: 1,
            id: proposal.id.clone(),
            scope: proposal.scope.clone(),
            revision: Revision::ZERO,
            proposal: proposal.clone(),
            resolution: resolution.clone(),
            payload_digest: digest.clone(),
            recorded_at: now,
        };
        mutations.push(put(
            Collection::Claim,
            recorded.id.as_str(),
            &access.workspace,
            Revision::ZERO,
            None,
            &recorded,
        )?);
        let mut version_id = None;
        let mut intent_id = None;
        if matches!(resolution.outcome, Outcome::Accepted | Outcome::Disputed) {
            let version = Version {
                document_type: DocumentType::Version,
                schema_version: 1,
                id: ClaimVersionId::new(),
                scope: proposal.scope.clone(),
                revision: Revision::ZERO,
                proposal: proposal.clone(),
                memory_seq: next_seq,
                canonical_watermark: watermark,
                recorded_at: now,
                resolution: resolution.clone(),
            };
            let mut next_head = head.clone().unwrap_or(Head {
                document_type: DocumentType::Head,
                schema_version: 1,
                id: proposal.claim.clone(),
                scope: proposal.scope.clone(),
                revision: Revision::ZERO,
                current: None,
                disputed: vec![],
            });
            if let Some(prior) = &head {
                next_head.revision = prior.revision.next()?;
            }
            if resolution.outcome == Outcome::Accepted {
                next_head.current = Some(version.id.clone());
            } else {
                next_head.disputed.push(version.id.clone());
            }
            let intent = IndexIntent {
                document_type: DocumentType::IndexIntent,
                schema_version: 1,
                id: IndexIntentId::new(),
                scope: proposal.scope.clone(),
                revision: Revision::ZERO,
                transaction: tx.clone(),
                versions: vec![version.id.clone()],
                supersedes: if resolution.outcome == Outcome::Accepted {
                    proposal.predecessor.clone().into_iter().collect()
                } else {
                    vec![]
                },
                memory_seq: next_seq,
                canonical_watermark: watermark,
                status: IndexStatus::Pending,
            };
            mutations.push(put(
                Collection::Claim,
                version.id.as_str(),
                &access.workspace,
                Revision::ZERO,
                None,
                &version,
            )?);
            mutations.push(put(
                Collection::Projection,
                next_head.id.as_str(),
                &access.workspace,
                next_head.revision,
                head.as_ref().map(|h| h.revision),
                &next_head,
            )?);
            mutations.push(put(
                Collection::IndexIntent,
                intent.id.as_str(),
                &access.workspace,
                Revision::ZERO,
                None,
                &intent,
            )?);
            version_id = Some(version.id);
            intent_id = Some(intent.id);
        }
        let next_sequence = MemoryHead {
            document_type: DocumentType::Sequence,
            schema_version: 1,
            id: access.workspace.clone(),
            workspace: access.workspace.clone(),
            revision: sequence
                .as_ref()
                .map_or(Ok(Revision::ZERO), |s| s.revision.next())?,
            sequence: next_seq,
        };
        mutations.push(put(
            Collection::Projection,
            next_sequence.id.as_str(),
            &access.workspace,
            next_sequence.revision,
            sequence.as_ref().map(|s| s.revision),
            &next_sequence,
        )?);
        let result = ProposalResult {
            document_type: DocumentType::Result,
            schema_version: 1,
            id: proposal.command.clone(),
            scope: proposal.scope.clone(),
            revision: Revision::ZERO,
            proposal: proposal.id.clone(),
            payload_digest: digest.clone(),
            transaction: tx.clone(),
            version: version_id,
            intent: intent_id,
            resolution: resolution.clone(),
            memory_seq: next_seq,
        };
        mutations.push(put(
            Collection::Projection,
            result.id.as_str(),
            &access.workspace,
            Revision::ZERO,
            None,
            &result,
        )?);
        let records: Vec<_> = mutations
            .iter()
            .filter_map(|m| match m {
                Mutation::Put { record, .. } => Some(serde_json::json!({"collection":record.collection,"id":record.id,"revision":record.revision})),
                _ => None,
            })
            .collect();
        let event = EventInput {
            id: EventId::new(),
            workspace: access.workspace.clone(),
            session: proposal.scope.session.clone(),
            task: Some(proposal.scope.task.clone()),
            actor: access.actor.clone(),
            correlation: proposal.command.clone(),
            causation: proposal.origins.first().cloned(),
            timestamp: now,
            kind: EventKind::MemoryResolved,
            artifacts: vec![],
            data: serde_json::json!({"schema_version":1,"proposal":proposal.id,"resolution":resolution.outcome,"records":records}),
            metadata: None,
        };
        let transaction = Transaction {
            id: tx,
            expected_watermark: store.state().watermark,
            mutations,
            events: vec![event],
            command: Some(ReceiptInput {
                command: proposal.command.clone(),
                workspace: access.workspace.clone(),
                session: proposal.scope.session.clone(),
                digest: digest.clone(),
                result: CommandResult::Accepted {
                    revision: next_sequence.revision,
                },
            }),
        };
        match store.transact(transaction).await {
            Ok(receipt) => {
                return Ok(MemoryCommit {
                    receipt,
                    indexing: result.intent.as_ref().map(|_| IndexStatus::Pending),
                    result,
                })
            }
            Err(vcp_store::Error::Conflict("stale canonical watermark")) => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(Error::Conflict("memory commit retry limit"))
}
