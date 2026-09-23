// SPDX-License-Identifier: Apache-2.0
//! Explicit owner review. Pending submissions never enter recall or indexing.
//! Every mutation binds the original authenticated public command in its one
//! canonical transaction; ordinary automatic ingestion uses repository::propose.
use crate::{
    access::{self, Access},
    Error, Result,
};
use serde::Serialize;
use vcp_domain::{
    ids::*,
    memory::*,
    memory_review::{self, Choice, Decision, Submission},
    revision::*,
    task::Task,
    workspace::Scope,
};
use vcp_protocol::{
    canonical_bytes,
    command::CommandResult,
    digest_bytes,
    event::{EventInput, EventKind},
};
use vcp_store::{contract::*, Store};

pub struct ReviewState {
    pub submission: Submission,
    pub decision: Option<Decision>,
    pub result: Option<ProposalResult>,
    pub indexing: Option<IndexStatus>,
}
pub struct ReviewCommit {
    pub receipt: Receipt,
    pub review: ReviewState,
}
/// Values are preconditions, not grants. Access and controller ownership remain
/// host-owned; this repository never creates or refreshes either.
pub struct Resolve {
    pub scope: Scope,
    pub submission: ProposalId,
    pub submission_digest: String,
    pub command: CommandId,
    pub command_digest: String,
    pub task_revision: Revision,
    pub steering: SteeringRevision,
    pub epochs: Epochs,
    pub expected_head: Option<ClaimVersionId>,
    pub choice: Choice,
    pub reason: String,
    pub now: Timestamp,
}

fn task(store: &Store, access: &Access, scope: &Scope, write: bool) -> Result<Task> {
    access::authorize(store.state(), access, write)?;
    if scope.workspace != access.workspace || !access.allows_task(&scope.task) {
        return Err(Error::Access);
    }
    let task: Task = store
        .state()
        .record(Collection::Task, scope.task.as_str(), &access.workspace)?
        .decode()?;
    if task.scope != *scope || task.redaction.is_some() {
        return Err(Error::Access);
    }
    Ok(task)
}
fn retained(store: &Store, access: &Access, submission: &Submission) -> Result<()> {
    submission.validate()?;
    access::proposal_scope(store.state(), access, &submission.candidate)?;
    if crate::history::proposal_removed(store.state(), &access.workspace, &submission.candidate)? {
        return Err(Error::Conflict(
            "manual memory submission unavailable after retention",
        ));
    }
    if digest_bytes(&canonical_bytes(&submission.candidate)?) != submission.candidate_digest {
        return Err(Error::Conflict("manual memory candidate integrity"));
    }
    Ok(())
}
/// A pruned or physically purged review returns an unavailable error, never its
/// old candidate text. Observers need current source access but no write grant.
pub fn read(store: &Store, access: &Access, scope: &Scope, id: &ProposalId) -> Result<ReviewState> {
    task(store, access, scope, false)?;
    let row = store
        .state()
        .record(Collection::Claim, id.as_str(), &access.workspace)?;
    if row.value["document_type"] != memory_review::SUBMISSION {
        return Err(Error::Conflict("manual memory submission unavailable"));
    }
    let submission: Submission = row.decode()?;
    if submission.scope != *scope {
        return Err(Error::Access);
    }
    retained(store, access, &submission)?;
    let decision_id = memory_review_decision_id(scope, id)?;
    let decision = store
        .state()
        .records
        .get(&key(Collection::Projection, &decision_id))
        .map(|row| -> Result<Decision> {
            if row.workspace != access.workspace
                || row.value["document_type"] != memory_review::DECISION
            {
                return Err(Error::Conflict("manual memory decision unavailable"));
            }
            let value: Decision = row.decode()?;
            value.validate_submission(&submission)?;
            access::resolution_scope(store.state(), access, &value.resolution)?;
            let mut decided_source = submission.candidate.clone();
            decided_source.command = value.command.clone();
            if crate::history::proposal_removed(store.state(), &access.workspace, &decided_source)?
                || crate::retention::purged(
                    store.state(),
                    &access.workspace,
                    &crate::retention::Target::Record(key(Collection::Projection, &value.id)),
                )?
            {
                return Err(Error::Conflict(
                    "manual memory decision unavailable after retention",
                ));
            }
            Ok(value)
        })
        .transpose()?;
    let result = decision
        .as_ref()
        .filter(|d| d.governed_proposal.is_some())
        .map(|d| -> Result<ProposalResult> {
            let result: ProposalResult = store
                .state()
                .record(
                    Collection::Projection,
                    d.command.as_str(),
                    &access.workspace,
                )?
                .decode()?;
            if result.scope != *scope
                || Some(&result.proposal) != d.governed_proposal.as_ref()
                || result.resolution != d.resolution
            {
                return Err(Error::Conflict("manual memory governed result integrity"));
            }
            Ok(result)
        })
        .transpose()?;
    let indexing = result
        .as_ref()
        .and_then(|r| r.intent.as_ref())
        .map(|id| -> Result<IndexStatus> {
            let intent: IndexIntent = store
                .state()
                .record(Collection::IndexIntent, id.as_str(), &access.workspace)?
                .decode()?;
            Ok(intent.status)
        })
        .transpose()?;
    Ok(ReviewState {
        submission,
        decision,
        result,
        indexing,
    })
}

fn receipt(
    store: &Store,
    scope: &Scope,
    command: &CommandId,
    digest: &str,
) -> Result<Option<Receipt>> {
    let Some(command_receipt) = store
        .state()
        .commands
        .get(&command_key(&scope.workspace, command))
    else {
        return Ok(None);
    };
    if command_receipt.digest != digest {
        return Err(Error::Conflict("manual memory command payload conflict"));
    }
    let receipt = store
        .state()
        .transactions
        .get(&command_receipt.transaction)
        .ok_or(Error::Conflict("manual memory receipt unavailable"))?;
    if receipt.command.as_ref() != Some(command_receipt) {
        return Err(Error::Conflict("manual memory receipt integrity"));
    }
    Ok(Some(receipt.clone()))
}
fn current_head(store: &Store, access: &Access, claim: &ClaimId) -> Result<Option<ClaimVersionId>> {
    let head = store
        .state()
        .records
        .get(&key(Collection::Projection, claim.as_str()))
        .map(|row| -> Result<Head> {
            if row.workspace != access.workspace {
                return Err(Error::Access);
            }
            let head: Head = row.decode()?;
            if !access.allows_task(&head.scope.task) {
                return Err(Error::Access);
            }
            Ok(head)
        })
        .transpose()?;
    // Heads are disposable, but review must never interpret a lost projection as
    // an empty claim. Repair is a separate explicit operation, not a side effect.
    let mut accepted = None::<(MemorySeq, ClaimVersionId)>;
    for row in store
        .state()
        .records
        .values()
        .filter(|r| r.collection == Collection::Claim && r.workspace == access.workspace)
    {
        match row.value["document_type"].as_str() {
            Some("vcp_memory_version_v1") => {
                let version: Version = row.decode()?;
                if &version.proposal.claim == claim
                    && version.resolution.outcome == Outcome::Accepted
                {
                    access::version_scope(store.state(), access, &version)?;
                    if accepted
                        .as_ref()
                        .is_none_or(|(seq, _)| version.memory_seq > *seq)
                    {
                        accepted = Some((version.memory_seq, version.id));
                    }
                }
            }
            Some(vcp_domain::redaction::VERSION) => {
                let version: vcp_domain::redaction::RedactedVersion = row.decode()?;
                if &version.claim == claim && version.outcome == Outcome::Accepted {
                    access::redacted_scope(
                        store.state(),
                        access,
                        &version.scope,
                        &version.sources,
                    )?;
                    if accepted
                        .as_ref()
                        .is_none_or(|(seq, _)| version.memory_seq > *seq)
                    {
                        accepted = Some((version.memory_seq, version.id));
                    }
                }
            }
            _ => (),
        }
    }
    let actual = head.and_then(|h| h.current);
    if actual != accepted.map(|(_, id)| id) {
        return Err(Error::Conflict(
            "manual memory claim projection requires repair",
        ));
    }
    Ok(actual)
}
fn guards(
    store: &Store,
    access: &Access,
    candidate: &Proposal,
    revision: Revision,
    steering: SteeringRevision,
    epochs: &Epochs,
    expected_head: &Option<ClaimVersionId>,
) -> Result<()> {
    let task = task(store, access, &candidate.scope, true)?;
    let workspace = access::authorize(store.state(), access, true)?;
    if task.revision != revision
        || task.steering != steering
        || epochs.authority != workspace.authority
        || epochs.deletion != workspace.deletion
        || epochs.policy != access::policy(store.state(), &access.workspace)?
    {
        return Err(Error::Conflict("manual memory preconditions changed"));
    }
    if &current_head(store, access, &candidate.claim)? != expected_head {
        return Err(Error::Conflict("manual memory claim head changed"));
    }
    Ok(())
}
pub(crate) fn validate_fresh(
    store: &Store,
    access: &Access,
    submission: &Submission,
    request: &Resolve,
) -> Result<()> {
    retained(store, access, submission)?;
    if request.scope != submission.scope
        || request.submission != submission.id
        || request.submission_digest != submission.candidate_digest
        || request.expected_head != submission.expected_head
    {
        return Err(Error::Conflict(
            "manual memory submission preconditions changed",
        ));
    }
    guards(
        store,
        access,
        &submission.candidate,
        request.task_revision,
        request.steering,
        &request.epochs,
        &request.expected_head,
    )?;
    if request.choice == Choice::Accept {
        // Automatic ingestion repairs these projections before evaluating gates.
        // Manual review instead fails closed if any competing accepted claim has
        // lost its head; otherwise an invisible conflict could win acceptance.
        let mut claims = std::collections::BTreeSet::new();
        let mut sequence = MemorySeq::ZERO;
        for row in store
            .state()
            .records
            .values()
            .filter(|r| r.workspace == access.workspace)
        {
            match row.value["document_type"].as_str() {
                Some("vcp_memory_version_v1") => {
                    let v: Version = row.decode()?;
                    claims.insert(v.proposal.claim);
                }
                Some(vcp_domain::redaction::VERSION) => {
                    let v: vcp_domain::redaction::RedactedVersion = row.decode()?;
                    claims.insert(v.claim);
                }
                Some("vcp_memory_result_v1") => {
                    let r: ProposalResult = row.decode()?;
                    sequence = sequence.max(r.memory_seq);
                }
                Some(vcp_domain::redaction::RESULT) => {
                    let r: vcp_domain::redaction::RedactedResult = row.decode()?;
                    sequence = sequence.max(r.memory_seq);
                }
                _ => (),
            }
        }
        for claim in claims {
            current_head(store, access, &claim)?;
        }
        let saved = store
            .state()
            .records
            .get(&key(Collection::Projection, access.workspace.as_str()))
            .map(Record::decode::<MemoryHead>)
            .transpose()?;
        if saved.as_ref().map_or(MemorySeq::ZERO, |v| v.sequence) != sequence {
            return Err(Error::Conflict(
                "manual memory sequence projection requires repair",
            ));
        }
    }
    Ok(())
}
pub(crate) fn decision(
    submission: &Submission,
    request: &Resolve,
    actor: &ActorId,
    governed_proposal: Option<ProposalId>,
    resolution: Resolution,
) -> Result<Decision> {
    let decision = Decision {
        document_type: memory_review::DECISION.into(),
        schema_version: 1,
        id: memory_review_decision_id(&submission.scope, &submission.id)?,
        scope: submission.scope.clone(),
        revision: Revision::ZERO,
        submission: submission.id.clone(),
        submission_digest: submission.candidate_digest.clone(),
        command: request.command.clone(),
        command_digest: request.command_digest.clone(),
        actor: actor.clone(),
        epochs: request.epochs.clone(),
        task_revision: request.task_revision,
        steering: request.steering,
        expected_head: request.expected_head.clone(),
        choice: request.choice,
        reason: request.reason.clone(),
        governed_proposal,
        resolution,
        recorded_at: request.now,
    };
    decision.validate_submission(submission)?;
    Ok(decision)
}
pub(crate) fn decorate<T: Serialize>(
    event: &mut EventInput,
    action: &str,
    id: &str,
    record: &T,
) -> Result<()> {
    event.data["memory_review"] = serde_json::json!({"schema_version":1,"action":action,"id":id,"record_digest":digest_bytes(&canonical_bytes(record)?)});
    Ok(())
}
fn transaction<T: Serialize>(
    store: &Store,
    scope: &Scope,
    actor: &ActorId,
    command: &CommandId,
    digest: &str,
    now: Timestamp,
    collection: Collection,
    id: &str,
    value: &T,
    action: &str,
) -> Result<Transaction> {
    let record = Record::typed(
        collection,
        id,
        scope.workspace.clone(),
        Revision::ZERO,
        value,
    )?;
    let mut event = EventInput {
        id: EventId::new(),
        workspace: scope.workspace.clone(),
        session: scope.session.clone(),
        task: Some(scope.task.clone()),
        actor: actor.clone(),
        correlation: command.clone(),
        causation: None,
        timestamp: now,
        kind: EventKind::MemoryResolved,
        artifacts: vec![],
        data: serde_json::json!({"schema_version":1,"records":[{"collection":collection,"id":id,"revision":Revision::ZERO}]}),
        metadata: None,
    };
    decorate(&mut event, action, id, value)?;
    Ok(Transaction {
        id: TransactionId::new(),
        expected_watermark: store.state().watermark,
        mutations: vec![Mutation::Put {
            expected: None,
            record,
        }],
        events: vec![event],
        command: Some(ReceiptInput {
            command: command.clone(),
            workspace: scope.workspace.clone(),
            session: scope.session.clone(),
            digest: digest.into(),
            result: CommandResult::Accepted {
                revision: Revision::ZERO,
            },
        }),
    })
}
pub async fn submit(
    store: &mut Store,
    access: &Access,
    submission: Submission,
) -> Result<ReviewCommit> {
    task(store, access, &submission.scope, true)?;
    if submission.candidate.actor != access.actor {
        return Err(Error::Access);
    }
    if let Some(receipt) = receipt(
        store,
        &submission.scope,
        &submission.candidate.command,
        &submission.command_digest,
    )? {
        let review = read(store, access, &submission.scope, &submission.id)?;
        if review.submission.candidate.command != submission.candidate.command
            || review.submission.command_digest != submission.command_digest
        {
            return Err(Error::Conflict("manual memory submission command identity"));
        }
        return Ok(ReviewCommit { receipt, review });
    }
    if canonical_bytes(&submission)?.len() > 256 * 1024 {
        return Err(Error::Invalid("manual submission exceeds 256 KiB".into()));
    }
    retained(store, access, &submission)?;
    guards(
        store,
        access,
        &submission.candidate,
        submission.task_revision,
        submission.steering,
        &submission.candidate.epochs,
        &submission.expected_head,
    )?;
    let tx = transaction(
        store,
        &submission.scope,
        &access.actor,
        &submission.candidate.command,
        &submission.command_digest,
        submission.recorded_at,
        Collection::Claim,
        submission.id.as_str(),
        &submission,
        "submission",
    )?;
    let receipt = store.transact(tx).await?;
    Ok(ReviewCommit {
        receipt,
        review: ReviewState {
            submission,
            decision: None,
            result: None,
            indexing: None,
        },
    })
}
pub async fn resolve(store: &mut Store, access: &Access, request: Resolve) -> Result<ReviewCommit> {
    task(store, access, &request.scope, true)?;
    let review = read(store, access, &request.scope, &request.submission)?;
    if request.submission_digest != review.submission.candidate_digest {
        return Err(Error::Conflict("manual memory candidate digest changed"));
    }
    if let Some(receipt) = receipt(
        store,
        &request.scope,
        &request.command,
        &request.command_digest,
    )? {
        if review.decision.as_ref().is_none_or(|d| {
            d.command != request.command || d.command_digest != request.command_digest
        }) {
            return Err(Error::Conflict("manual memory decision command identity"));
        }
        return Ok(ReviewCommit { receipt, review });
    }
    if review.decision.is_some() {
        return Err(Error::Conflict("manual memory submission already decided"));
    }
    validate_fresh(store, access, &review.submission, &request)?;
    let receipt = match request.choice {
        Choice::Accept => {
            let mut proposal = review.submission.candidate.clone();
            proposal.id = ProposalId::new();
            proposal.command = request.command.clone();
            proposal.actor = access.actor.clone();
            proposal.epochs = request.epochs.clone();
            crate::repository::propose_inner(
                store,
                access,
                proposal,
                request.now,
                Some((&review.submission, &request)),
            )
            .await?
            .receipt
        }
        Choice::Reject => {
            let resolution = Resolution {
                outcome: Outcome::Rejected,
                evidence_status: EvidenceStatus::Unverified,
                findings: vec![Finding {
                    rule: "vcp.manual-review".into(),
                    severity: Severity::Block,
                    message: "explicit owner rejection".into(),
                    versions: vec![],
                    evidence: vec![],
                }],
                conflicts: vec![],
                validated_evidence: vec![],
            };
            let decision = decision(
                &review.submission,
                &request,
                &access.actor,
                None,
                resolution,
            )?;
            let tx = transaction(
                store,
                &request.scope,
                &access.actor,
                &request.command,
                &request.command_digest,
                request.now,
                Collection::Projection,
                &decision.id,
                &decision,
                "decision",
            )?;
            store.transact(tx).await?
        }
    };
    let review = read(store, access, &request.scope, &request.submission)?;
    Ok(ReviewCommit { receipt, review })
}
