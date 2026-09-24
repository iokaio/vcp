// SPDX-License-Identifier: Apache-2.0
use super::{
    planner::PlannedHook,
    registry::{FailurePolicy, HookEvent},
    Error, Result,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextProposal {
    pub text: String,
    pub artifact_refs: Vec<vcp_domain::ArtifactId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArgumentRewrite {
    pub original_digest: String,
    pub tool: String,
    pub arguments: serde_json::Value,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HookOutput {
    pub schema_version: u32,
    pub findings: Vec<String>,
    pub context: Option<ContextProposal>,
    pub rewrite: Option<ArgumentRewrite>,
    /// A successful validator can explicitly block the affected action.
    pub block: bool,
}
pub fn validate_output(bytes: &[u8], planned: &PlannedHook) -> Result<HookOutput> {
    if bytes.len() > planned.definition.max_output_bytes {
        return Err(Error::Limit("output bytes"));
    }
    let output: HookOutput = serde_json::from_slice(bytes)?;
    if output.schema_version != 1 {
        return Err(Error::Invalid("output schema version"));
    }
    if output.block && (output.context.is_some() || output.rewrite.is_some()) {
        return Err(Error::Invalid("blocked result cannot publish proposals"));
    }
    if let Some(context) = &output.context {
        if context
            .artifact_refs
            .iter()
            .any(|r| !planned.input.artifact_refs.contains(r))
        {
            return Err(Error::Invalid("ungranted output artifact"));
        }
    }
    if let Some(rewrite) = &output.rewrite {
        if planned.input.event != HookEvent::BeforeToolAuthorization
            || !super::hash(&rewrite.original_digest)
            || rewrite.tool.is_empty()
            || !rewrite.arguments.is_object()
            || planned
                .input
                .payload
                .get("operation_digest")
                .and_then(|v| v.as_str())
                != Some(rewrite.original_digest.as_str())
        {
            return Err(Error::Invalid("argument rewrite"));
        }
    }
    Ok(output)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureDisposition {
    Block,
    Warn,
}
/// Parsing/schema/security failures always block. Only ordinary execution failures
/// may use the configured optional-notification warning policy.
pub fn failure_disposition(policy: FailurePolicy, validation_failure: bool) -> FailureDisposition {
    if validation_failure || policy == FailurePolicy::Block {
        FailureDisposition::Block
    } else {
        FailureDisposition::Warn
    }
}
