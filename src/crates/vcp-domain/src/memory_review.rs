// SPDX-License-Identifier: Apache-2.0
//! Explicit manual review; automatic governed proposals keep their existing path.
//! These immutable records are evidence, never persisted execution grants.
use crate::{
    memory::{Epochs, Outcome, Proposal, Resolution},
    workspace::Scope,
    *,
};
use serde::{Deserialize, Serialize};

pub const SUBMISSION: &str = "vcp_memory_review_submission_v1";
pub const DECISION: &str = "vcp_memory_review_decision_v1";
pub const DECISION_PREFIX: &str = "memory-review-decision-";
pub const MANUAL_EXTRACTOR: &str = "explicit-public-candidate/1";

/// Pending means recorded for review, not accepted into recall or indexing.
/// The command digest binds the original public request; candidate_digest binds
/// the host-filled Proposal independently of that wire identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Submission {
    pub document_type: String,
    pub schema_version: u32,
    pub id: ProposalId,
    pub scope: Scope,
    pub revision: Revision,
    pub candidate: Proposal,
    pub candidate_digest: String,
    pub command_digest: String,
    pub task_revision: Revision,
    pub steering: SteeringRevision,
    pub expected_head: Option<ClaimVersionId>,
    pub recorded_at: Timestamp,
}
impl Submission {
    pub fn validate(&self) -> Result<()> {
        if self.document_type != SUBMISSION
            || self.schema_version != 1
            || self.revision != Revision::ZERO
            || self.id != self.candidate.id
            || self.scope != self.candidate.scope
            || self.candidate.extractor != MANUAL_EXTRACTOR
        {
            return Err(Error::Invalid("manual memory submission identity"));
        }
        self.candidate.validate()?;
        digest(&self.candidate_digest)?;
        digest(&self.command_digest)?;
        if self.candidate.predecessor.is_some() && self.candidate.predecessor != self.expected_head
        {
            return Err(Error::Invalid("manual memory correction head"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    Accept,
    Reject,
}

/// Exactly one immutable decision key per scoped submission. The store must
/// verify the key's domain-separated SHA-256 derivation and source linkage.
/// Accept requests governance evaluation; it does not dictate its outcome.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub document_type: String,
    pub schema_version: u32,
    pub id: String,
    pub scope: Scope,
    pub revision: Revision,
    pub submission: ProposalId,
    pub submission_digest: String,
    pub command: CommandId,
    pub command_digest: String,
    pub actor: ActorId,
    pub epochs: Epochs,
    pub task_revision: Revision,
    pub steering: SteeringRevision,
    pub expected_head: Option<ClaimVersionId>,
    pub choice: Choice,
    pub reason: String,
    /// Present when evaluation emitted an ordinary governed proposal/result.
    /// A direct explicit rejection may have no governed proposal or version.
    pub governed_proposal: Option<ProposalId>,
    pub resolution: Resolution,
    pub recorded_at: Timestamp,
}
impl Decision {
    pub fn validate(&self) -> Result<()> {
        if self.document_type != DECISION
            || self.schema_version != 1
            || self.revision != Revision::ZERO
            || !self
                .id
                .strip_prefix(DECISION_PREFIX)
                .is_some_and(valid_digest)
            || self.reason.trim().is_empty()
            || self.reason.len() > 4096
            || self.reason.contains('\0')
        {
            return Err(Error::Invalid("manual memory decision identity"));
        }
        digest(&self.submission_digest)?;
        digest(&self.command_digest)?;
        self.resolution.validate()?;
        if self.resolution.outcome == Outcome::AwaitingReview
            || (self.choice == Choice::Accept && self.governed_proposal.is_none())
            || (self.choice == Choice::Reject
                && (self.resolution.outcome != Outcome::Rejected
                    || self.governed_proposal.is_some()))
        {
            return Err(Error::Invalid("manual memory decision outcome"));
        }
        Ok(())
    }

    /// Shape/linkage validation only. The transaction owner separately checks
    /// current authority, policy, retained source access and the actual claim head.
    pub fn validate_submission(&self, submission: &Submission) -> Result<()> {
        self.validate()?;
        submission.validate()?;
        if self.scope != submission.scope
            || self.submission != submission.id
            || self.submission_digest != submission.candidate_digest
            || self.recorded_at < submission.recorded_at
            || self.governed_proposal.as_ref() == Some(&submission.id)
        {
            return Err(Error::Invalid("manual memory decision submission"));
        }
        Ok(())
    }
}
fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn digest(value: &str) -> Result<()> {
    if valid_digest(value) {
        Ok(())
    } else {
        Err(Error::Invalid("manual memory digest"))
    }
}

pub const REDACTED_SUBMISSION: &str = "vcp_memory_redacted_review_submission_v1";
pub const REDACTED_DECISION: &str = "vcp_memory_redacted_review_decision_v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedactedSubmission {
    pub document_type: String,
    pub schema_version: u32,
    pub id: ProposalId,
    pub scope: Scope,
    pub revision: Revision,
    pub deletion: DeletionEpoch,
    pub original_digest: String,
    pub candidate_digest: String,
    pub command_digest: String,
    pub command: CommandId,
    pub actor: ActorId,
    pub claim: ClaimId,
    pub sources: crate::redaction::Sources,
    pub recorded_at: Timestamp,
}
impl RedactedSubmission {
    pub fn validate(&self) -> Result<()> {
        erased_header(
            &self.document_type,
            REDACTED_SUBMISSION,
            self.schema_version,
            self.revision,
            self.deletion,
            &self.original_digest,
        )?;
        digest(&self.candidate_digest)?;
        digest(&self.command_digest)?;
        self.sources.validate()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedactedDecision {
    pub document_type: String,
    pub schema_version: u32,
    pub id: String,
    pub scope: Scope,
    pub revision: Revision,
    pub deletion: DeletionEpoch,
    pub original_digest: String,
    pub submission: ProposalId,
    pub submission_digest: String,
    pub command: CommandId,
    pub command_digest: String,
    pub actor: ActorId,
    pub choice: Choice,
    pub outcome: Outcome,
    pub governed_proposal: Option<ProposalId>,
    pub sources: crate::redaction::Sources,
    pub recorded_at: Timestamp,
}
impl RedactedDecision {
    pub fn validate(&self) -> Result<()> {
        erased_header(
            &self.document_type,
            REDACTED_DECISION,
            self.schema_version,
            self.revision,
            self.deletion,
            &self.original_digest,
        )?;
        if !self
            .id
            .strip_prefix(DECISION_PREFIX)
            .is_some_and(valid_digest)
            || self.outcome == Outcome::AwaitingReview
            || (self.choice == Choice::Accept && self.governed_proposal.is_none())
            || (self.choice == Choice::Reject
                && (self.governed_proposal.is_some() || self.outcome != Outcome::Rejected))
        {
            return Err(Error::Invalid("redacted review decision outcome"));
        }
        digest(&self.submission_digest)?;
        digest(&self.command_digest)?;
        self.sources.validate()
    }
}
fn erased_header(
    tag: &str,
    expected: &str,
    schema: u32,
    revision: Revision,
    deletion: DeletionEpoch,
    original: &str,
) -> Result<()> {
    if tag != expected || schema != 1 || revision != Revision::ZERO {
        return Err(Error::Invalid("redacted review header"));
    }
    crate::redaction::ContentRedaction {
        deletion,
        original_digest: original.into(),
    }
    .validate()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn submission_keeps_host_scope_immutable_and_correction_pinned_to_head() {
        let scope = Scope {
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
            task: TaskId::new(),
        };
        let candidate = Proposal {
            id: ProposalId::new(),
            command: CommandId::new(),
            claim: ClaimId::new(),
            scope: scope.clone(),
            actor: ActorId::new(),
            epochs: Epochs {
                authority: AuthorityRevision::ZERO,
                policy: PolicyRevision::ZERO,
                deletion: DeletionEpoch::ZERO,
            },
            registry_version: crate::memory::REGISTRY_VERSION,
            extractor: MANUAL_EXTRACTOR.into(),
            output_key: "candidate".into(),
            origins: vec![EventId::new()],
            subject: "parser".into(),
            predicate: "architecture".into(),
            statement: "Parser isolates syntax".into(),
            value: crate::memory::ClaimValue::Architecture {
                decision: "isolate syntax".into(),
                rationale: "retained source".into(),
                inference: true,
            },
            applicability: crate::memory::Applicability {
                repository: "repo".into(),
                worktree: "main".into(),
                roots: vec![],
                paths: vec![],
                symbols: vec![],
                branch: None,
                fingerprint: None,
                conditions: Default::default(),
                valid_from: None,
                valid_until: None,
            },
            evidence: vec![],
            predecessor: None,
            correction_reason: None,
            retention: "workspace".into(),
        };
        let mut submission = Submission {
            document_type: SUBMISSION.into(),
            schema_version: 1,
            id: candidate.id.clone(),
            scope,
            revision: Revision::ZERO,
            candidate,
            candidate_digest: "a".repeat(64),
            command_digest: "b".repeat(64),
            task_revision: Revision::new(3),
            steering: SteeringRevision::new(2),
            expected_head: None,
            recorded_at: Timestamp::new(100),
        };
        assert!(submission.validate().is_ok());
        let head = ClaimVersionId::new();
        submission.candidate.predecessor = Some(head.clone());
        submission.candidate.correction_reason = Some("explicit source correction".into());
        assert!(submission.validate().is_err());
        submission.expected_head = Some(head);
        assert!(submission.validate().is_ok());
        submission.scope.session = SessionId::new();
        assert!(submission.validate().is_err());
    }
    #[test]
    fn accepting_review_cannot_promise_a_governed_acceptance_or_persist_a_grant() {
        let mut decision = Decision {
            document_type: DECISION.into(),
            schema_version: 1,
            id: format!("{DECISION_PREFIX}{}", "a".repeat(64)),
            scope: Scope {
                workspace: WorkspaceId::new(),
                session: SessionId::new(),
                task: TaskId::new(),
            },
            revision: Revision::ZERO,
            submission: ProposalId::new(),
            submission_digest: "b".repeat(64),
            command: CommandId::new(),
            command_digest: "c".repeat(64),
            actor: ActorId::new(),
            epochs: Epochs {
                authority: AuthorityRevision::ZERO,
                policy: PolicyRevision::ZERO,
                deletion: DeletionEpoch::ZERO,
            },
            task_revision: Revision::ZERO,
            steering: SteeringRevision::ZERO,
            expected_head: None,
            choice: Choice::Accept,
            reason: "evaluate retained candidate".into(),
            governed_proposal: Some(ProposalId::new()),
            resolution: Resolution {
                outcome: Outcome::Disputed,
                evidence_status: crate::memory::EvidenceStatus::Inferred,
                findings: vec![],
                conflicts: vec![],
                validated_evidence: vec![],
            },
            recorded_at: Timestamp::new(1),
        };
        assert!(decision.validate().is_ok());
        decision.resolution.outcome = Outcome::Rejected;
        assert!(decision.validate().is_ok());
        decision.resolution.outcome = Outcome::AwaitingReview;
        assert!(decision.validate().is_err());
        decision.choice = Choice::Reject;
        decision.resolution.outcome = Outcome::Rejected;
        assert!(decision.validate().is_err());
        decision.governed_proposal = None;
        assert!(decision.validate().is_ok());
        decision.resolution.outcome = Outcome::Accepted;
        assert!(decision.validate().is_err());
        let mut value = serde_json::to_value(&decision).unwrap();
        value["controller_token"] = "not-a-grant".into();
        assert!(serde_json::from_value::<Decision>(value).is_err());
    }
}
