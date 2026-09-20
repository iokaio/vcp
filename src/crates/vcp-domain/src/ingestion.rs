// SPDX-License-Identifier: Apache-2.0
//! Durable extraction progress. Queue state never grants execution authority.
use crate::{workspace::Scope, *};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractorSpec {
    pub name: String,
    pub version: u32,
    /// Canonical EventKind names selected by the trusted adapter.
    pub event_kinds: Vec<String>,
}
impl ExtractorSpec {
    pub fn identity(&self) -> String {
        format!("{}/{}", self.name, self.version)
    }
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty()
            || self.name.len() > 128
            || self.name.contains(['\0', '\r', '\n', '/'])
            || self.version == 0
            || self.event_kinds.is_empty()
            || self.event_kinds.len() > 32
            || self.event_kinds.iter().any(|v| {
                v.is_empty()
                    || v.len() > 64
                    || !v.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
            })
            || self
                .event_kinds
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.event_kinds.len()
        {
            return Err(Error::Invalid("ingestion extractor specification"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentType {
    #[serde(rename = "vcp_ingestion_cursor_v1")]
    Cursor,
    #[serde(rename = "vcp_ingestion_job_v1")]
    Job,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    pub document_type: DocumentType,
    pub schema_version: u32,
    pub id: CommandId,
    pub scope: Scope,
    pub revision: Revision,
    pub extractor: ExtractorSpec,
    /// Exclusive offset in the append-only canonical event stream.
    pub after: Units,
    pub scanned_through: Watermark,
}
impl Cursor {
    pub fn validate(&self) -> Result<()> {
        if self.document_type != DocumentType::Cursor || self.schema_version != 1 {
            return Err(Error::Invalid("ingestion cursor schema"));
        }
        self.extractor.validate()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Pending,
    Leased,
    Deferred,
    Failed,
    Completed,
    Cancelled,
}
impl JobState {
    pub fn finished(self) -> bool {
        matches!(self, Self::Failed | Self::Completed | Self::Cancelled)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lease {
    pub token: CommandId,
    pub owner: ActorId,
    pub expires_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub document_type: DocumentType,
    pub schema_version: u32,
    pub id: CommandId,
    /// Actual origin task, which can be a child of the cursor's root.
    pub scope: Scope,
    pub root: TaskId,
    pub revision: Revision,
    pub cursor: CommandId,
    pub extractor: ExtractorSpec,
    pub origin: EventId,
    pub origin_watermark: Watermark,
    pub state: JobState,
    pub attempts: Units,
    pub max_attempts: Units,
    pub lease: Option<Lease>,
    pub not_before: Timestamp,
    pub last_failure: Option<String>,
    /// Committed P5-01 results; a replay reuses their stable proposal identities.
    pub results: Vec<CommandId>,
    /// Explicit unsupported/unavailable/no-claim outcome when extraction emits none.
    pub finding: Option<String>,
}
impl Job {
    pub fn validate(&self) -> Result<()> {
        if self.document_type != DocumentType::Job
            || self.schema_version != 1
            || self.max_attempts == Units::ZERO
            || self.max_attempts.get() > 16
            || self.attempts > self.max_attempts
            || (self.state == JobState::Leased) != self.lease.is_some()
            || self.results.len() > 64
            || self
                .results
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.results.len()
            || (self.state == JobState::Completed
                && self.results.is_empty()
                && self.finding.is_none())
            || [self.last_failure.as_ref(), self.finding.as_ref()]
                .into_iter()
                .flatten()
                .any(|s| s.is_empty() || s.len() > 4096 || s.contains('\0'))
        {
            return Err(Error::Invalid("ingestion job state"));
        }
        self.extractor.validate()
    }
}
