// SPDX-License-Identifier: Apache-2.0
//! Durable result data. Authority and external-effect state remain in the broker.
use super::result::HookOutput;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookStatus {
    Validated,
    Blocked,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HookReceipt {
    pub schema_version: u32,
    pub identity: String,
    pub effect: vcp_domain::ToolRunId,
    pub status: HookStatus,
    pub output: Option<HookOutput>,
    pub reason: Option<String>,
    pub evidence: Vec<vcp_domain::ArtifactId>,
}
