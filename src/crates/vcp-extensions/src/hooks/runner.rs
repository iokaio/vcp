// SPDX-License-Identifier: Apache-2.0
//! Pure wire projection; the lifecycle adapter exclusively owns process dispatch.
use super::{input::HookLimits, planner::PlannedHook, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HookRequest {
    /// One bounded JSON document; the broker adds newline transport framing.
    pub stdin: Vec<u8>,
    /// Binds the complete plan (including scoped input) to process authorization.
    pub digest: String,
    /// Profile arguments plus an authorization-bound input digest.
    pub arguments: Vec<String>,
}
pub fn request(planned: &PlannedHook, limits: HookLimits) -> Result<HookRequest> {
    planned.validate(limits)?;
    let stdin = vcp_protocol::canonical_bytes(planned)?;
    let digest = vcp_protocol::digest_bytes(&stdin);
    let mut arguments = planned.definition.command.arguments.clone();
    arguments.extend(["--vcp-hook-input-sha256".into(), digest.clone()]);
    Ok(HookRequest {
        stdin,
        digest,
        arguments,
    })
}
