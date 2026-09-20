// SPDX-License-Identifier: Apache-2.0
//! Explicit unavailable payload identities. These are never normal claim values.
use crate::{memory::Outcome, workspace::Scope, *};
use serde::{Deserialize, Serialize};

pub const PROPOSAL: &str = "vcp_memory_redacted_proposal_v1";
pub const VERSION: &str = "vcp_memory_redacted_version_v1";
pub const RESULT: &str = "vcp_memory_redacted_result_v1";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentRedaction {
    pub deletion: DeletionEpoch,
    pub original_digest: String,
}
impl ContentRedaction {
    pub fn validate(&self) -> Result<()> {
        if self.deletion == DeletionEpoch::ZERO
            || !crate::accounting::valid_hash(&self.original_digest)
        {
            return Err(Error::Invalid("explicit content redaction"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sources {
    pub origins: Vec<EventId>,
    pub artifacts: Vec<ArtifactId>,
    pub verifications: Vec<VerificationId>,
    pub versions: Vec<ClaimVersionId>,
}
impl Sources {
    pub fn validate(&self) -> Result<()> {
        fn unique<T: Ord>(values: &[T]) -> bool {
            values.len() <= 128
                && values
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    == values.len()
        }
        if !unique(&self.origins)
            || !unique(&self.artifacts)
            || !unique(&self.verifications)
            || !unique(&self.versions)
        {
            return Err(Error::Invalid("redacted source identities"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedactedProposal {
    pub document_type: String,
    pub schema_version: u32,
    pub id: ProposalId,
    pub scope: Scope,
    pub revision: Revision,
    pub deletion: DeletionEpoch,
    pub original_digest: String,
    pub payload_digest: String,
    pub command: CommandId,
    pub claim: ClaimId,
    pub actor: ActorId,
    pub sources: Sources,
    pub predecessor: Option<ClaimVersionId>,
    pub outcome: Outcome,
    pub recorded_at: Timestamp,
    /// Hashes of extractor/output-key/origin tuples prevent post-purge identity reuse.
    pub origin_output_keys: Vec<String>,
    /// Stable ingestion ownership remains provable without retaining extractor text.
    pub extractor_digest: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedactedVersion {
    pub document_type: String,
    pub schema_version: u32,
    pub id: ClaimVersionId,
    pub scope: Scope,
    pub revision: Revision,
    pub deletion: DeletionEpoch,
    pub original_digest: String,
    pub proposal: ProposalId,
    pub claim: ClaimId,
    pub predecessor: Option<ClaimVersionId>,
    pub sources: Sources,
    pub outcome: Outcome,
    pub memory_seq: MemorySeq,
    pub canonical_watermark: Watermark,
    pub recorded_at: Timestamp,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedactedResult {
    pub document_type: String,
    pub schema_version: u32,
    pub id: CommandId,
    pub scope: Scope,
    pub revision: Revision,
    pub deletion: DeletionEpoch,
    pub original_digest: String,
    pub proposal: ProposalId,
    pub payload_digest: String,
    pub transaction: TransactionId,
    pub version: Option<ClaimVersionId>,
    pub intent: Option<IndexIntentId>,
    pub outcome: Outcome,
    pub memory_seq: MemorySeq,
}
fn header(
    tag: &str,
    expected: &str,
    version: u32,
    revision: Revision,
    deletion: DeletionEpoch,
    digest: &str,
) -> Result<()> {
    if tag != expected
        || version != 1
        || revision != Revision::ZERO
        || deletion == DeletionEpoch::ZERO
        || !crate::accounting::valid_hash(digest)
    {
        return Err(Error::Invalid("redacted entity header"));
    }
    Ok(())
}
impl RedactedProposal {
    pub fn validate(&self) -> Result<()> {
        header(
            &self.document_type,
            PROPOSAL,
            self.schema_version,
            self.revision,
            self.deletion,
            &self.original_digest,
        )?;
        self.sources.validate()?;
        if !crate::accounting::valid_hash(&self.payload_digest)
            || !crate::accounting::valid_hash(&self.extractor_digest)
            || self.origin_output_keys.len() > 128
            || self
                .origin_output_keys
                .iter()
                .any(|hash| !crate::accounting::valid_hash(hash))
            || self
                .origin_output_keys
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.origin_output_keys.len()
        {
            return Err(Error::Invalid("redacted proposal digests"));
        }
        Ok(())
    }
}
impl RedactedVersion {
    pub fn validate(&self) -> Result<()> {
        header(
            &self.document_type,
            VERSION,
            self.schema_version,
            self.revision,
            self.deletion,
            &self.original_digest,
        )?;
        self.sources.validate()?;
        if self.memory_seq == MemorySeq::ZERO
            || self.canonical_watermark == Watermark::ZERO
            || !matches!(self.outcome, Outcome::Accepted | Outcome::Disputed)
        {
            return Err(Error::Invalid("redacted version chronology"));
        }
        Ok(())
    }
}
impl RedactedResult {
    pub fn validate(&self) -> Result<()> {
        header(
            &self.document_type,
            RESULT,
            self.schema_version,
            self.revision,
            self.deletion,
            &self.original_digest,
        )?;
        let accepted = matches!(self.outcome, Outcome::Accepted | Outcome::Disputed);
        if !crate::accounting::valid_hash(&self.payload_digest)
            || accepted != self.version.is_some()
            || accepted != self.intent.is_some()
            || (accepted && self.memory_seq == MemorySeq::ZERO)
        {
            return Err(Error::Invalid("redacted result identity"));
        }
        Ok(())
    }
}
