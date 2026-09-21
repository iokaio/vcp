// SPDX-License-Identifier: Apache-2.0
use super::*;

impl Context {
    pub(super) fn current_reasoning_effort(&self) -> Result<Option<vcp_models::reasoning::Effort>> {
        Ok(self
            .current_routing_policy()?
            .and_then(|policy| policy.reasoning_effort))
    }
}
