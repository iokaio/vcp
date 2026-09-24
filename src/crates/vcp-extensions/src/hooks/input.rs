// SPDX-License-Identifier: Apache-2.0
use super::{registry::HookEvent, Error, Result};
use serde::{Deserialize, Serialize};
use vcp_domain::{workspace::Scope, ArtifactId, TaskId};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HookInput {
    pub schema_version: u32,
    pub event: HookEvent,
    pub event_id: String,
    pub scope: Scope,
    pub root_task: TaskId,
    pub steering_revision: u64,
    pub causation_id: String,
    pub depth: u32,
    pub artifact_refs: Vec<ArtifactId>,
    /// Authorized projection supplied by the lifecycle adapter, never ambient context.
    pub payload: serde_json::Value,
}
#[derive(Clone, Copy, Debug)]
pub struct HookLimits {
    pub max_depth: u32,
    pub max_fanout: usize,
    pub max_input_bytes: usize,
}
impl Default for HookLimits {
    fn default() -> Self {
        Self {
            max_depth: 8,
            max_fanout: 32,
            max_input_bytes: 65_536,
        }
    }
}
impl HookInput {
    pub fn validate(&self, limits: HookLimits) -> Result<()> {
        if limits.max_depth > 32
            || limits.max_fanout > 128
            || limits.max_input_bytes > 1_048_576
            || limits.max_input_bytes == 0
        {
            return Err(Error::Limit("trusted input ceilings"));
        }
        if self.schema_version != 1
            || self.event_id.is_empty()
            || self.causation_id.is_empty()
            || self.event_id.len() > 128
            || self.causation_id.len() > 128
        {
            return Err(Error::Invalid("event identity or schema version"));
        }
        if self.depth > limits.max_depth {
            return Err(Error::Limit("causation depth"));
        }
        let unique: std::collections::BTreeSet<_> = self.artifact_refs.iter().collect();
        if unique.len() != self.artifact_refs.len() {
            return Err(Error::Invalid("artifact references"));
        }
        if serde_json::to_vec(self)?.len() > limits.max_input_bytes {
            return Err(Error::Limit("input bytes"));
        }
        Ok(())
    }
    pub fn identity(&self) -> Result<String> {
        super::digest(self)
    }
}
