// SPDX-License-Identifier: Apache-2.0
//! Authenticated versioned archive evidence for the existing restore pipeline.
use super::*;
use crate::{vault_crypto::ManifestEnvelope, CurrentStateView};
pub(crate) enum Data {
    Legacy {
        archive: Archive,
        proof: VerifiedRestore,
    },
    Stream(Box<stream::ValidatedStream>),
}
/// Authenticated transport and full semantic archive evidence. Decoded status
/// or extracted objects cannot construct this capability or advance trust.
pub struct Validated {
    pub(crate) data: Data,
    pub(super) operation: CommandId,
    pub(super) trust_revision: u64,
    pub(super) trust_digest: String,
}
impl Validated {
    /// Explicit bounded archival DTO for administrative/reference consumers.
    /// Large streamed archives remain usable through current/history import.
    pub async fn archive_state(&self) -> Result<crate::contract::State> {
        match &self.data {
            Data::Legacy { archive, .. } => Ok(archive.state().clone()),
            Data::Stream(value) => {
                value
                    .restored
                    .owner
                    .archive_state(&mut crate::history_index::io::Files::new(&value.replayed))
                    .await
            }
        }
    }
    pub fn archive_bytes(&self) -> Result<u64> {
        match &self.data {
            Data::Legacy { proof, .. } => {
                proof
                    .restored()
                    .payloads
                    .values()
                    .try_fold(0u64, |count, bytes| {
                        count
                            .checked_add(bytes.len() as u64)
                            .ok_or(Error::Limit("archive size"))
                    })
            }
            Data::Stream(value) => Ok(value.proof.manifest().payload.bytes),
        }
    }
    pub fn current(&self) -> CurrentStateView<'_> {
        match &self.data {
            Data::Legacy { archive, .. } => archive.state().into(),
            Data::Stream(value) => value.restored.owner.semantic().current().into(),
        }
    }
    pub fn inputs(&self) -> &crate::snapshot_inputs::Inputs {
        match &self.data {
            Data::Legacy { archive, .. } => archive.inputs(),
            Data::Stream(value) => &value.restored.inputs,
        }
    }
    pub fn coverage(&self) -> &crate::snapshot_inputs::Coverage {
        match &self.data {
            Data::Legacy { archive, .. } => archive.coverage(),
            Data::Stream(value) => &value.restored.coverage,
        }
    }
    pub fn manifest(&self) -> ManifestEnvelope {
        match &self.data {
            Data::Legacy { proof, .. } => proof.restored().manifest.clone().into(),
            Data::Stream(value) => value.proof.manifest().clone().into(),
        }
    }
    pub fn advance_trust(&self, trust: &mut LocalTrust, expected: u64) -> Result<()> {
        if expected != self.trust_revision
            || digest_bytes(&canonical_bytes(trust.configuration())?) != self.trust_digest
        {
            return Err(Error::Conflict("restore trust changed after validation"));
        }
        match &self.data {
            Data::Legacy { proof, .. } => trust.advance_after_restore(proof, expected),
            Data::Stream(value) => trust.advance_after_stream_restore(&value.proof, expected),
        }
    }
}
