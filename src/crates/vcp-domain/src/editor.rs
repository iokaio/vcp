// SPDX-License-Identifier: Apache-2.0
//! Hash-only canonical editor preparation and per-file receipts. Draft bytes are not records.
use crate::{workspace::Scope, *};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
// Version 2 deliberately fails the pre-editor store's generic Projection-v1
// fallback. An older engine must not open a store and erase this buffer fence.
pub const CHANGE: &str = "vcp_editor_change_v2";
pub const BUFFERS: &str = "vcp_editor_buffers_v2";
pub const SCHEMA_VERSION: u32 = 2;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub id: String,
    pub host: String,
    pub open_id: String,
    pub uri: String,
    pub path: String,
    pub version: Revision,
    pub content_sha256: String,
    pub dirty: bool,
    pub root: RootId,
    pub disk_sha256: String,
    pub disk_fingerprint: String,
    pub eol: String,
    pub encoding: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileState {
    Prepared,
    Dispatched,
    Applied,
    Rejected,
    Unknown,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    pub effect: ToolRunId,
    pub observation: Observation,
    pub after_sha256: String,
    pub edits_digest: String,
    pub operation_digest: String,
    pub state: FileState,
    pub execution: Option<ExecutionId>,
    pub observed: Option<Observation>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeSet {
    pub document_type: String,
    pub schema_version: u32,
    pub id: String,
    pub scope: Scope,
    pub revision: Revision,
    pub actor: ActorId,
    pub connection: ControllerId,
    pub generation: String,
    pub root: RootId,
    pub host: HostId,
    pub binding: Revision,
    pub authority: AuthorityRevision,
    pub policy: PolicyRevision,
    pub steering: SteeringRevision,
    pub files: Vec<File>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BufferFile {
    pub observation: Observation,
    pub uncertain: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BufferState {
    pub document_type: String,
    pub schema_version: u32,
    pub id: String,
    pub scope: Scope,
    pub revision: Revision,
    pub files: BTreeMap<String, BufferFile>,
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl Observation {
    pub fn validate(&self) -> Result<()> {
        for id in [&self.id, &self.host, &self.open_id] {
            TaskId::parse(id.clone())?;
        }
        if self.uri.is_empty()
            || self.uri.len() > 32768
            || self.uri.contains('\0')
            || self.path.is_empty()
            || self.path.len() > 32768
            || self.path.contains('\0')
            || !hash(&self.content_sha256)
            || !hash(&self.disk_sha256)
            || !hash(&self.disk_fingerprint)
            || !matches!(self.eol.as_str(), "lf" | "crlf")
            || !matches!(
                self.encoding.as_str(),
                "unknown" | "utf8" | "utf8_bom" | "utf16_le" | "utf16_be"
            )
        {
            return Err(Error::Invalid("editor observation"));
        }
        Ok(())
    }
}
impl ChangeSet {
    pub fn validate(&self) -> Result<()> {
        TaskId::parse(self.id.clone())?;
        TaskId::parse(self.generation.clone())?;
        if self.document_type != CHANGE
            || self.schema_version != SCHEMA_VERSION
            || self.files.is_empty()
            || self.files.len() > 16
        {
            return Err(Error::Invalid("editor change"));
        }
        let mut paths = std::collections::BTreeSet::new();
        let mut effects = std::collections::BTreeSet::new();
        for file in &self.files {
            file.observation.validate()?;
            if !paths.insert(&file.observation.path)
                || !effects.insert(&file.effect)
                || file.observation.root != self.root
                || !hash(&file.after_sha256)
                || !hash(&file.edits_digest)
                || !hash(&file.operation_digest)
                || (file.state == FileState::Prepared && file.execution.is_some())
                || (!matches!(file.state, FileState::Prepared | FileState::Rejected)
                    && file.execution.is_none())
            {
                return Err(Error::Invalid("editor prepared file"));
            }
            if let Some(observed) = &file.observed {
                observed.validate()?;
                if observed.root != self.root
                    || observed.path != file.observation.path
                    || observed.uri != file.observation.uri
                {
                    return Err(Error::Scope);
                }
            }
            if matches!(file.state, FileState::Applied | FileState::Rejected)
                && file.observed.is_none()
            {
                return Err(Error::Invalid("editor receipt missing observation"));
            }
        }
        Ok(())
    }
}
impl BufferState {
    pub fn validate(&self) -> Result<()> {
        TaskId::parse(self.id.clone())?;
        if self.document_type != BUFFERS
            || self.schema_version != SCHEMA_VERSION
            || self.files.len() > 16
        {
            return Err(Error::Invalid("editor buffer tracking limit"));
        }
        for (path, file) in &self.files {
            file.observation.validate()?;
            if path != &file.observation.path {
                return Err(Error::Invalid("editor buffer path"));
            }
        }
        Ok(())
    }
    pub fn unverified(&self) -> bool {
        // A last clean observation is not a live guarantee. Disk-only checks
        // cannot certify tracked editor documents, including after disconnection.
        !self.files.is_empty()
    }
}
