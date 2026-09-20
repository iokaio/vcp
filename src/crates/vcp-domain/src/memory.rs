// SPDX-License-Identifier: Apache-2.0
//! Provider-independent governed-memory records. Validation grants no authority.
use crate::{
    artifact::Range,
    verification::{CheckOutcome, Fingerprint},
    workspace::Scope,
    *,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const REGISTRY_VERSION: u32 = 1;
pub const MAX_EVIDENCE: usize = 64;
pub const MAX_FINDINGS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentType {
    #[serde(rename = "vcp_memory_proposal_v1")]
    Proposal,
    #[serde(rename = "vcp_memory_version_v1")]
    Version,
    #[serde(rename = "vcp_memory_head_v1")]
    Head,
    #[serde(rename = "vcp_memory_sequence_v1")]
    Sequence,
    #[serde(rename = "vcp_memory_index_intent_v1")]
    IndexIntent,
    #[serde(rename = "vcp_memory_result_v1")]
    Result,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Epochs {
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
    pub policy: PolicyRevision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    Command,
    ModuleRelationship,
    Architecture,
    EnvironmentConstraint,
    VerifiedFix,
    UserPreference,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandPurpose {
    Test,
    Build,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEndpoint {
    pub root: RootId,
    pub path: String,
    pub symbol: Option<String>,
    pub artifact: ArtifactId,
    pub sha256: String,
}
impl SourceEndpoint {
    pub fn validate(&self) -> Result<()> {
        relative_path(&self.path)?;
        optional_text(&self.symbol, 1024)?;
        digest(&self.sha256)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClaimValue {
    Command {
        purpose: CommandPurpose,
        argv: Vec<String>,
        cwd: String,
        configuration: ArtifactId,
        outcome: Option<CheckOutcome>,
        verification: Option<VerificationId>,
    },
    ModuleRelationship {
        from: SourceEndpoint,
        relation: String,
        to: SourceEndpoint,
    },
    Architecture {
        decision: String,
        rationale: String,
        inference: bool,
    },
    EnvironmentConstraint {
        component: String,
        requirement: String,
        observed_value: Option<String>,
    },
    VerifiedFix {
        issue: String,
        patch: ArtifactId,
        verification: VerificationId,
        before: Fingerprint,
        after: Fingerprint,
    },
    UserPreference {
        key: String,
        value: String,
        explicit_origin: EventId,
    },
}
impl ClaimValue {
    pub fn kind(&self) -> ClaimKind {
        match self {
            Self::Command { .. } => ClaimKind::Command,
            Self::ModuleRelationship { .. } => ClaimKind::ModuleRelationship,
            Self::Architecture { .. } => ClaimKind::Architecture,
            Self::EnvironmentConstraint { .. } => ClaimKind::EnvironmentConstraint,
            Self::VerifiedFix { .. } => ClaimKind::VerifiedFix,
            Self::UserPreference { .. } => ClaimKind::UserPreference,
        }
    }
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Command {
                argv,
                cwd,
                outcome,
                verification,
                ..
            } => {
                if argv.is_empty() || argv.len() > 64 {
                    return Err(Error::Invalid("memory command arguments"));
                }
                text(&argv[0], 4096)?;
                for argument in argv {
                    if argument.len() > 4096 || argument.contains('\0') {
                        return Err(Error::Invalid("memory command argument"));
                    }
                }
                relative_path(cwd)?;
                if outcome.is_some() && verification.is_none() {
                    return Err(Error::Invalid("memory command outcome verification"));
                }
                if let Some(CheckOutcome::Failed { reason } | CheckOutcome::NotRun { reason }) =
                    outcome
                {
                    text(reason, 4096)?;
                }
            }
            Self::ModuleRelationship { from, relation, to } => {
                from.validate()?;
                to.validate()?;
                text(relation, 256)?;
            }
            Self::Architecture {
                decision,
                rationale,
                ..
            } => {
                text(decision, 8192)?;
                text(rationale, 8192)?;
            }
            Self::EnvironmentConstraint {
                component,
                requirement,
                observed_value,
            } => {
                text(component, 256)?;
                text(requirement, 4096)?;
                optional_text(observed_value, 4096)?;
            }
            Self::VerifiedFix {
                issue,
                before,
                after,
                ..
            } => {
                text(issue, 8192)?;
                before.validate()?;
                after.validate()?;
            }
            Self::UserPreference { key, value, .. } => {
                text(key, 256)?;
                text(value, 8192)?;
            }
        }
        Ok(())
    }
    /// Claimed source dependencies, not yet validated or authorized references.
    pub fn artifacts(&self) -> Vec<&ArtifactId> {
        match self {
            Self::Command { configuration, .. } => vec![configuration],
            Self::ModuleRelationship { from, to, .. } => vec![&from.artifact, &to.artifact],
            Self::VerifiedFix { patch, .. } => vec![patch],
            _ => vec![],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Applicability {
    pub repository: String,
    pub worktree: String,
    pub roots: Vec<RootId>,
    pub paths: Vec<String>,
    pub symbols: Vec<String>,
    pub branch: Option<String>,
    pub fingerprint: Option<Fingerprint>,
    pub conditions: BTreeMap<String, String>,
    pub valid_from: Option<Timestamp>,
    pub valid_until: Option<Timestamp>,
}
impl Applicability {
    pub fn validate(&self) -> Result<()> {
        text(&self.repository, 1024)?;
        text(&self.worktree, 1024)?;
        unique(&self.roots, 32)?;
        unique(&self.paths, 64)?;
        unique(&self.symbols, 64)?;
        for path in &self.paths {
            relative_path(path)?;
        }
        for symbol in &self.symbols {
            text(symbol, 1024)?;
        }
        optional_text(&self.branch, 1024)?;
        if let Some(fingerprint) = &self.fingerprint {
            fingerprint.validate()?;
        }
        if self.conditions.len() > 32 {
            return Err(Error::Invalid("memory applicability conditions"));
        }
        for (key, value) in &self.conditions {
            text(key, 256)?;
            text(value, 1024)?;
        }
        if self
            .valid_from
            .zip(self.valid_until)
            .is_some_and(|(start, end)| start >= end)
        {
            return Err(Error::Invalid("memory validity interval"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Source,
    Configuration,
    ToolOutput,
    Verification,
    UserStatement,
    ModelInference,
    Patch,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRef {
    pub artifact: ArtifactId,
    pub sha256: String,
    pub range: Option<Range>,
    pub source: Option<Fingerprint>,
    pub verification: Option<VerificationId>,
    pub kind: EvidenceKind,
}
impl EvidenceRef {
    pub fn validate(&self) -> Result<()> {
        digest(&self.sha256)?;
        if self.range.as_ref().is_some_and(|r| r.start >= r.end) {
            return Err(Error::Invalid("memory evidence span"));
        }
        if let Some(source) = &self.source {
            source.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub id: ProposalId,
    pub command: CommandId,
    pub claim: ClaimId,
    pub scope: Scope,
    pub actor: ActorId,
    pub epochs: Epochs,
    pub registry_version: u32,
    pub extractor: String,
    pub output_key: String,
    pub origins: Vec<EventId>,
    pub subject: String,
    pub predicate: String,
    pub statement: String,
    pub value: ClaimValue,
    pub applicability: Applicability,
    pub evidence: Vec<EvidenceRef>,
    pub predecessor: Option<ClaimVersionId>,
    pub correction_reason: Option<String>,
    pub retention: String,
}
impl Proposal {
    /// Retention bounds are distinct from semantic gates: a rejected proposal
    /// may contain an invalid digest, predecessor, registry or command shape.
    pub fn validate_retained(&self) -> Result<()> {
        if self.origins.len() > 64
            || self.evidence.len() > MAX_EVIDENCE
            || self.applicability.roots.len() > 32
            || self.applicability.paths.len() > 64
            || self.applicability.symbols.len() > 64
            || self.applicability.conditions.len() > 32
        {
            return Err(Error::Invalid("retained memory collection bounds"));
        }
        let mut strings = vec![
            self.extractor.as_str(),
            &self.output_key,
            &self.subject,
            &self.predicate,
            &self.statement,
            &self.retention,
            &self.applicability.repository,
            &self.applicability.worktree,
        ];
        strings.extend(self.correction_reason.iter().map(String::as_str));
        strings.extend(self.applicability.branch.iter().map(String::as_str));
        strings.extend(self.applicability.paths.iter().map(String::as_str));
        strings.extend(self.applicability.symbols.iter().map(String::as_str));
        for (key, value) in &self.applicability.conditions {
            strings.extend([key.as_str(), value.as_str()]);
        }
        if let Some(fingerprint) = &self.applicability.fingerprint {
            strings.extend(fingerprint_strings(fingerprint));
        }
        for evidence in &self.evidence {
            strings.push(&evidence.sha256);
            if let Some(fingerprint) = &evidence.source {
                strings.extend(fingerprint_strings(fingerprint));
            }
        }
        match &self.value {
            ClaimValue::Command {
                argv, cwd, outcome, ..
            } => {
                if argv.len() > 64 {
                    return Err(Error::Invalid("retained command argument count"));
                }
                strings.extend(argv.iter().map(String::as_str));
                strings.push(cwd);
                if let Some(CheckOutcome::Failed { reason } | CheckOutcome::NotRun { reason }) =
                    outcome
                {
                    strings.push(reason);
                }
            }
            ClaimValue::ModuleRelationship { from, relation, to } => {
                strings.push(relation);
                for endpoint in [from, to] {
                    strings.extend([endpoint.path.as_str(), &endpoint.sha256]);
                    strings.extend(endpoint.symbol.iter().map(String::as_str));
                }
            }
            ClaimValue::Architecture {
                decision,
                rationale,
                ..
            } => strings.extend([decision.as_str(), rationale]),
            ClaimValue::EnvironmentConstraint {
                component,
                requirement,
                observed_value,
            } => {
                strings.extend([component.as_str(), requirement]);
                strings.extend(observed_value.iter().map(String::as_str));
            }
            ClaimValue::VerifiedFix {
                issue,
                before,
                after,
                ..
            } => {
                strings.push(issue);
                strings.extend(fingerprint_strings(before));
                strings.extend(fingerprint_strings(after));
            }
            ClaimValue::UserPreference { key, value, .. } => strings.extend([key.as_str(), value]),
        }
        if strings.iter().any(|s| s.len() > 32768)
            || strings.iter().map(|s| s.len()).sum::<usize>() > 256 * 1024
        {
            return Err(Error::Invalid("retained memory text bounds"));
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<()> {
        self.validate_retained()?;
        if self.registry_version != REGISTRY_VERSION {
            return Err(Error::Invalid("memory registry version"));
        }
        text(&self.extractor, 256)?;
        text(&self.output_key, 256)?;
        text(&self.subject, 1024)?;
        text(&self.predicate, 256)?;
        if self.subject != self.subject.trim() || self.predicate != self.predicate.trim() {
            return Err(Error::Invalid("memory subject/predicate normalization"));
        }
        text(&self.statement, 16384)?;
        text(&self.retention, 256)?;
        unique(&self.origins, 64)?;
        if self.origins.is_empty() || self.evidence.len() > MAX_EVIDENCE {
            return Err(Error::Invalid("memory origin or evidence count"));
        }
        unique(
            &self
                .evidence
                .iter()
                .map(|e| &e.artifact)
                .collect::<Vec<_>>(),
            MAX_EVIDENCE,
        )?;
        for evidence in &self.evidence {
            evidence.validate()?;
        }
        self.value.validate()?;
        self.applicability.validate()?;
        optional_text(&self.correction_reason, 4096)?;
        if self.predecessor.is_some() != self.correction_reason.is_some() {
            return Err(Error::Invalid("memory correction predecessor and reason"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Accepted,
    Disputed,
    Rejected,
    AwaitingReview,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    Verified,
    Observed,
    Inferred,
    Unverified,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Block,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub rule: String,
    pub severity: Severity,
    pub message: String,
    pub versions: Vec<ClaimVersionId>,
    pub evidence: Vec<ArtifactId>,
}
impl Finding {
    pub fn validate(&self) -> Result<()> {
        text(&self.rule, 256)?;
        text(&self.message, 4096)?;
        unique(&self.versions, 64)?;
        unique(&self.evidence, MAX_EVIDENCE)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resolution {
    pub outcome: Outcome,
    pub evidence_status: EvidenceStatus,
    pub findings: Vec<Finding>,
    pub conflicts: Vec<ClaimVersionId>,
    /// Only existing, authorized evidence goes here. Claimed missing/foreign
    /// IDs stay in the rejected proposal and never become store references.
    pub validated_evidence: Vec<ArtifactId>,
}
impl Resolution {
    pub fn validate(&self) -> Result<()> {
        if self.findings.len() > MAX_FINDINGS {
            return Err(Error::Invalid("memory findings count"));
        }
        for finding in &self.findings {
            finding.validate()?;
        }
        unique(&self.conflicts, 64)?;
        unique(&self.validated_evidence, MAX_EVIDENCE)?;
        if self.outcome == Outcome::Rejected && self.evidence_status == EvidenceStatus::Verified {
            return Err(Error::Invalid("rejected memory cannot be verified"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalRecord {
    pub document_type: DocumentType,
    pub schema_version: u32,
    pub id: ProposalId,
    pub scope: Scope,
    pub revision: Revision,
    pub proposal: Proposal,
    pub resolution: Resolution,
    pub payload_digest: String,
    pub recorded_at: Timestamp,
}
impl ProposalRecord {
    pub fn validate(&self) -> Result<()> {
        if self.document_type != DocumentType::Proposal {
            return Err(Error::Invalid("memory document type"));
        }
        schema(self.schema_version)?;
        if self.id != self.proposal.id
            || self.scope != self.proposal.scope
            || self.revision != Revision::ZERO
        {
            return Err(Error::Invalid("memory proposal record identity"));
        }
        if self.resolution.outcome == Outcome::Rejected {
            self.proposal.validate_retained()?;
        } else {
            self.proposal.validate()?;
        }
        self.resolution.validate()?;
        validated_subset(&self.proposal, &self.resolution)?;
        digest(&self.payload_digest)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Version {
    pub document_type: DocumentType,
    pub schema_version: u32,
    pub id: ClaimVersionId,
    pub scope: Scope,
    pub revision: Revision,
    pub proposal: Proposal,
    pub memory_seq: MemorySeq,
    pub canonical_watermark: Watermark,
    pub recorded_at: Timestamp,
    pub resolution: Resolution,
}
impl Version {
    pub fn validate(&self) -> Result<()> {
        if self.document_type != DocumentType::Version {
            return Err(Error::Invalid("memory document type"));
        }
        schema(self.schema_version)?;
        if self.scope != self.proposal.scope
            || self.revision != Revision::ZERO
            || self.memory_seq == MemorySeq::ZERO
            || self.canonical_watermark == Watermark::ZERO
            || !matches!(
                self.resolution.outcome,
                Outcome::Accepted | Outcome::Disputed
            )
            || self.proposal.predecessor.as_ref() == Some(&self.id)
        {
            return Err(Error::Invalid("memory version identity or resolution"));
        }
        self.proposal.validate()?;
        self.resolution.validate()?;
        validated_subset(&self.proposal, &self.resolution)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Head {
    pub document_type: DocumentType,
    pub schema_version: u32,
    pub id: ClaimId,
    pub scope: Scope,
    pub revision: Revision,
    pub current: Option<ClaimVersionId>,
    pub disputed: Vec<ClaimVersionId>,
}
impl Head {
    pub fn validate(&self) -> Result<()> {
        if self.document_type != DocumentType::Head {
            return Err(Error::Invalid("memory document type"));
        }
        schema(self.schema_version)?;
        unique(&self.disputed, 64)?;
        if self
            .current
            .as_ref()
            .is_some_and(|v| self.disputed.contains(v))
        {
            return Err(Error::Invalid("memory head accepted/disputed overlap"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryHead {
    pub document_type: DocumentType,
    pub schema_version: u32,
    pub id: WorkspaceId,
    pub workspace: WorkspaceId,
    pub revision: Revision,
    pub sequence: MemorySeq,
}
impl MemoryHead {
    pub fn validate(&self) -> Result<()> {
        if self.document_type != DocumentType::Sequence {
            return Err(Error::Invalid("memory document type"));
        }
        schema(self.schema_version)?;
        if self.id != self.workspace {
            return Err(Error::Scope);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexStatus {
    Pending,
    Ready,
    Failed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexIntent {
    pub document_type: DocumentType,
    pub schema_version: u32,
    pub id: IndexIntentId,
    pub scope: Scope,
    pub revision: Revision,
    pub transaction: TransactionId,
    pub versions: Vec<ClaimVersionId>,
    pub supersedes: Vec<ClaimVersionId>,
    pub memory_seq: MemorySeq,
    pub canonical_watermark: Watermark,
    pub status: IndexStatus,
}
impl IndexIntent {
    pub fn validate(&self) -> Result<()> {
        if self.document_type != DocumentType::IndexIntent {
            return Err(Error::Invalid("memory document type"));
        }
        schema(self.schema_version)?;
        unique(&self.versions, 64)?;
        unique(&self.supersedes, 64)?;
        if self.versions.is_empty()
            || self.memory_seq == MemorySeq::ZERO
            || self.canonical_watermark == Watermark::ZERO
            || self.versions.iter().any(|id| self.supersedes.contains(id))
        {
            return Err(Error::Invalid("memory index intent"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalResult {
    pub document_type: DocumentType,
    pub schema_version: u32,
    pub id: CommandId,
    pub scope: Scope,
    pub revision: Revision,
    pub proposal: ProposalId,
    pub payload_digest: String,
    pub transaction: TransactionId,
    pub version: Option<ClaimVersionId>,
    pub intent: Option<IndexIntentId>,
    pub resolution: Resolution,
    pub memory_seq: MemorySeq,
}
impl ProposalResult {
    pub fn validate(&self) -> Result<()> {
        if self.document_type != DocumentType::Result {
            return Err(Error::Invalid("memory document type"));
        }
        schema(self.schema_version)?;
        digest(&self.payload_digest)?;
        self.resolution.validate()?;
        let accepted = matches!(
            self.resolution.outcome,
            Outcome::Accepted | Outcome::Disputed
        );
        if self.revision != Revision::ZERO
            || self.version.is_some() != accepted
            || self.intent.is_some() != accepted
            || (accepted && self.memory_seq == MemorySeq::ZERO)
        {
            return Err(Error::Invalid("memory result identity or resolution"));
        }
        Ok(())
    }
}

fn schema(version: u32) -> Result<()> {
    if version == 1 {
        Ok(())
    } else {
        Err(Error::Invalid("memory schema version"))
    }
}
fn fingerprint_strings(fingerprint: &Fingerprint) -> [&str; 3] {
    [
        &fingerprint.repository,
        &fingerprint.buffers,
        &fingerprint.environment,
    ]
}
fn validated_subset(proposal: &Proposal, resolution: &Resolution) -> Result<()> {
    if resolution
        .validated_evidence
        .iter()
        .any(|id| !proposal.evidence.iter().any(|e| &e.artifact == id))
    {
        return Err(Error::Invalid("memory validated evidence outside proposal"));
    }
    Ok(())
}
fn text(value: &str, max: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > max || value.contains('\0') {
        return Err(Error::Invalid("bounded memory text"));
    }
    Ok(())
}
fn optional_text(value: &Option<String>, max: usize) -> Result<()> {
    if let Some(value) = value {
        text(value, max)?;
    }
    Ok(())
}
fn digest(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::Invalid("memory SHA-256"));
    }
    Ok(())
}
fn unique<T: Ord>(values: &[T], max: usize) -> Result<()> {
    if values.len() > max || values.iter().collect::<BTreeSet<_>>().len() != values.len() {
        return Err(Error::Invalid("memory list size or duplicates"));
    }
    Ok(())
}
/// Portable root-relative form; this does not authorize filesystem access.
fn relative_path(value: &str) -> Result<()> {
    text(value, 4096)?;
    if value == "." {
        return Ok(());
    }
    if value.starts_with('/')
        || value.contains(['\\', ':'])
        || value
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err(Error::Invalid("memory root-relative path"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proposal() -> Proposal {
        Proposal {
            id: ProposalId::new(),
            command: CommandId::new(),
            claim: ClaimId::new(),
            scope: Scope {
                workspace: WorkspaceId::new(),
                session: SessionId::new(),
                task: TaskId::new(),
            },
            actor: ActorId::new(),
            epochs: Epochs {
                authority: AuthorityRevision::ZERO,
                deletion: DeletionEpoch::ZERO,
                policy: PolicyRevision::ZERO,
            },
            registry_version: REGISTRY_VERSION,
            extractor: "deterministic/1".into(),
            output_key: "architecture-0".into(),
            origins: vec![EventId::new()],
            subject: "parser".into(),
            predicate: "architecture".into(),
            statement: "Parser isolates syntax".into(),
            value: ClaimValue::Architecture {
                decision: "Parser isolates syntax".into(),
                rationale: "Observed module boundaries".into(),
                inference: true,
            },
            applicability: Applicability {
                repository: "repo".into(),
                worktree: "main".into(),
                roots: vec![],
                paths: vec!["src/parser.rs".into()],
                symbols: vec![],
                branch: None,
                fingerprint: None,
                conditions: BTreeMap::new(),
                valid_from: None,
                valid_until: None,
            },
            evidence: vec![EvidenceRef {
                artifact: ArtifactId::new(),
                sha256: "a".repeat(64),
                range: None,
                source: None,
                verification: None,
                kind: EvidenceKind::Source,
            }],
            predecessor: None,
            correction_reason: None,
            retention: "workspace".into(),
        }
    }
    fn resolution() -> Resolution {
        Resolution {
            outcome: Outcome::Rejected,
            evidence_status: EvidenceStatus::Unverified,
            findings: vec![],
            conflicts: vec![],
            validated_evidence: vec![],
        }
    }

    #[test]
    fn tagged_records_roundtrip_and_reject_unknown_fields() {
        let proposal = proposal();
        let record = ProposalRecord {
            document_type: DocumentType::Proposal,
            schema_version: 1,
            id: proposal.id.clone(),
            scope: proposal.scope.clone(),
            revision: Revision::ZERO,
            proposal,
            resolution: resolution(),
            payload_digest: "b".repeat(64),
            recorded_at: Timestamp::new(1),
        };
        record.validate().unwrap();
        let mut json = serde_json::to_value(&record).unwrap();
        assert_eq!(json["document_type"], "vcp_memory_proposal_v1");
        assert_eq!(
            serde_json::from_value::<ProposalRecord>(json.clone()).unwrap(),
            record
        );
        json["authority_override"] = serde_json::json!(true);
        assert!(serde_json::from_value::<ProposalRecord>(json).is_err());
    }

    #[test]
    fn rejected_proposal_preserves_invalid_claimed_evidence_without_validating_it() {
        let mut proposal = proposal();
        proposal.evidence[0].sha256 = "not-a-digest".into();
        proposal.registry_version = 999;
        assert!(proposal.validate().is_err());
        let mut record = ProposalRecord {
            document_type: DocumentType::Proposal,
            schema_version: 1,
            id: proposal.id.clone(),
            scope: proposal.scope.clone(),
            revision: Revision::ZERO,
            proposal,
            resolution: resolution(),
            payload_digest: "b".repeat(64),
            recorded_at: Timestamp::new(1),
        };
        record.validate().unwrap();
        record.resolution.outcome = Outcome::Accepted;
        assert!(record.validate().is_err());
    }

    #[test]
    fn paths_predecessor_and_evidence_membership_are_checked() {
        let mut proposal = proposal();
        proposal.validate().unwrap();
        for path in [
            "../escape",
            "C:/escape",
            "\\\\server\\root",
            "/root",
            "src/../other",
            "src\\file",
        ] {
            proposal.applicability.paths = vec![path.into()];
            assert!(proposal.validate().is_err(), "accepted {path}");
        }
        proposal.applicability.paths = vec!["src/解析.rs".into()];
        proposal.predecessor = Some(ClaimVersionId::new());
        assert!(proposal.validate().is_err());
        proposal.correction_reason = Some("User correction".into());
        proposal.validate().unwrap();
        let mut resolved = resolution();
        resolved.validated_evidence = vec![ArtifactId::new()];
        assert!(validated_subset(&proposal, &resolved).is_err());
    }

    #[test]
    fn six_registry_classes_have_structured_values_and_preserve_argument_case() {
        let source = SourceEndpoint {
            root: RootId::new(),
            path: "src/lib.rs".into(),
            symbol: None,
            artifact: ArtifactId::new(),
            sha256: "a".repeat(64),
        };
        let fingerprint = Fingerprint {
            repository: "a".repeat(64),
            buffers: "b".repeat(64),
            environment: "c".repeat(64),
        };
        let values = [
            ClaimValue::Command {
                purpose: CommandPurpose::Test,
                argv: vec!["Tool.EXE".into(), "".into()],
                cwd: ".".into(),
                configuration: ArtifactId::new(),
                outcome: None,
                verification: None,
            },
            ClaimValue::ModuleRelationship {
                from: source.clone(),
                relation: "imports".into(),
                to: source,
            },
            ClaimValue::Architecture {
                decision: "isolate parser".into(),
                rationale: "source observation".into(),
                inference: true,
            },
            ClaimValue::EnvironmentConstraint {
                component: "rust".into(),
                requirement: "stable".into(),
                observed_value: None,
            },
            ClaimValue::VerifiedFix {
                issue: "parser panic".into(),
                patch: ArtifactId::new(),
                verification: VerificationId::new(),
                before: fingerprint.clone(),
                after: fingerprint,
            },
            ClaimValue::UserPreference {
                key: "format".into(),
                value: "plain".into(),
                explicit_origin: EventId::new(),
            },
        ];
        for value in &values {
            value.validate().unwrap();
        }
        let encoded = serde_json::to_string(&values[0]).unwrap();
        assert!(encoded.contains("Tool.EXE"));
        let mut invalid = values[0].clone();
        if let ClaimValue::Command { outcome, .. } = &mut invalid {
            *outcome = Some(CheckOutcome::Passed);
        }
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn retained_invalid_proposals_are_still_bounded() {
        let mut proposal = proposal();
        proposal.statement = "x".repeat(32769);
        assert!(proposal.validate_retained().is_err());
        let mut proposal = super::tests::proposal();
        proposal.evidence = vec![proposal.evidence[0].clone(); MAX_EVIDENCE + 1];
        assert!(proposal.validate_retained().is_err());
    }
}
