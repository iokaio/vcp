// SPDX-License-Identifier: Apache-2.0
//! Workspace aggregate source identity; never a task-scoped permission grant.
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const SOURCES: &str = "vcp_optimization_forecast_sources_v1";
pub const REDACTED: &str = "vcp_optimization_redacted_forecast_sources_v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sources {
    pub document_type: String,
    pub schema_version: u32,
    pub id: String,
    pub workspace: WorkspaceId,
    pub revision: Revision,
    pub report: String,
    pub artifact: ArtifactId,
    pub artifact_digest: String,
    pub source_tasks: BTreeSet<TaskId>,
    pub source_records: BTreeSet<String>,
    pub source_events: BTreeSet<EventId>,
    pub sources_digest: String,
}
impl Sources {
    pub fn validate(&self) -> Result<()> {
        if self.document_type != SOURCES
            || self.schema_version != 1
            || self.revision != Revision::ZERO
            || self.id.is_empty()
            || self.id.len() > 128
            || self.report.is_empty()
            || self.report.len() > 128
            || self.source_tasks.is_empty()
            || self.source_tasks.len() > 4096
            || self.source_records.len() > 16_384
            || self.source_events.len() > 8192
            || self
                .source_records
                .iter()
                .any(|r| r.is_empty() || r.len() > 256)
            || !accounting::valid_hash(&self.artifact_digest)
            || !accounting::valid_hash(&self.sources_digest)
        {
            return Err(Error::Invalid("forecast source manifest"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedactedSources {
    pub document_type: String,
    pub schema_version: u32,
    pub id: String,
    pub workspace: WorkspaceId,
    pub revision: Revision,
    pub deletion: DeletionEpoch,
    pub original_digest: String,
}
impl RedactedSources {
    pub fn validate(&self) -> Result<()> {
        if self.document_type != REDACTED
            || self.schema_version != 1
            || self.id.is_empty()
            || self.id.len() > 128
        {
            return Err(Error::Invalid("redacted forecast identity"));
        }
        redaction::ContentRedaction {
            deletion: self.deletion,
            original_digest: self.original_digest.clone(),
        }
        .validate()
    }
}
