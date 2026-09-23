// SPDX-License-Identifier: Apache-2.0
//! Shared read-side logical suppression, independent of physical erasure.
use crate::{ids::*, revision::*};
use serde::{Deserialize, Serialize};

/// P5 owns selection, protected references and purge. Reads must honor these
/// logical ranges/artifact exclusions before a physical rewrite completes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionMask {
    pub schema_version: u32,
    pub workspace: WorkspaceId,
    pub session: SessionId,
    pub first: SessionSeq,
    pub last: SessionSeq,
    pub artifacts: Vec<ArtifactId>,
    pub deletion: DeletionEpoch,
    pub reason: String,
}
impl RetentionMask {
    pub fn validate(&self) -> crate::Result<()> {
        if self.schema_version != 1 || self.first > self.last || self.reason.trim().is_empty() {
            return Err(crate::Error::Invalid("retention mask"));
        }
        Ok(())
    }
}
