// SPDX-License-Identifier: Apache-2.0
//! Canonical publication identity; component bytes remain rebuildable derivatives.
use crate::{workspace::Scope, *};
use serde::{Deserialize, Serialize};

pub const GENERATION: &str = "vcp_search_generation_v1";
pub const ACTIVE: &str = "vcp_search_active_v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Generation {
    pub document_type: String,
    pub schema_version: u32,
    pub id: GenerationId,
    pub scope: Scope,
    pub revision: Revision,
    pub transaction: TransactionId,
    pub previous: Option<GenerationId>,
    pub canonical_watermark: Watermark,
    pub memory_seq: MemorySeq,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
    pub inventory_digest: String,
    pub inventory_checksum: String,
    pub lexical_schema: u32,
    pub tokenizer: String,
    pub lexical_checksum: String,
    pub embedding_specification: String,
    pub vector_checksum: Option<String>,
    /// Complete coverage of an inventory with no eligible searchable records.
    pub empty_complete: bool,
    pub vector_deficits: Vec<String>,
    pub covered_intents: Vec<IndexIntentId>,
}
impl Generation {
    pub fn validate(&self) -> Result<()> {
        if self.document_type != GENERATION
            || self.schema_version != 1
            || self.revision != Revision::ZERO
            || self.id.as_str() == self.scope.workspace.as_str()
            || self.previous.as_ref() == Some(&self.id)
            || self.lexical_schema != 1
            || self.tokenizer.is_empty()
            || self.tokenizer.len() > 256
            || self.vector_deficits.len() > 16384
            || self.covered_intents.len() > 4096
            || [
                &self.inventory_digest,
                &self.inventory_checksum,
                &self.lexical_checksum,
                &self.embedding_specification,
            ]
            .iter()
            .any(|hash| !crate::accounting::valid_hash(hash))
            || self
                .vector_checksum
                .as_ref()
                .is_some_and(|hash| !crate::accounting::valid_hash(hash))
            || self
                .vector_deficits
                .iter()
                .any(|id| !crate::accounting::valid_hash(id))
            || self
                .vector_deficits
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.vector_deficits.len()
            || self
                .covered_intents
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.covered_intents.len()
            || (self.empty_complete
                && (self.vector_checksum.is_some() || !self.vector_deficits.is_empty()))
            || ((!self.empty_complete
                && (self.vector_checksum.is_none() || !self.vector_deficits.is_empty()))
                && (!self.covered_intents.is_empty() || self.memory_seq != MemorySeq::ZERO))
        {
            return Err(Error::Invalid("search generation manifest"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Active {
    pub document_type: String,
    pub schema_version: u32,
    pub id: WorkspaceId,
    pub workspace: WorkspaceId,
    pub revision: Revision,
    pub generation: GenerationId,
    pub transaction: TransactionId,
}
impl Active {
    pub fn validate(&self) -> Result<()> {
        if self.document_type != ACTIVE || self.schema_version != 1 || self.id != self.workspace {
            return Err(Error::Invalid("active search generation"));
        }
        Ok(())
    }
}
