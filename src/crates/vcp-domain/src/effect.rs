// SPDX-License-Identifier: Apache-2.0
use crate::{workspace::Scope, *};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectState {
    Proposed,
    Validated,
    Authorized,
    DispatchRecorded,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    OutcomeUnknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Effect {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redaction: Option<crate::redaction::ContentRedaction>,
    pub id: ToolRunId,
    pub scope: Scope,
    pub revision: Revision,
    pub steering: SteeringRevision,
    pub state: EffectState,
    pub operation_digest: String,
    pub execution: Option<ExecutionId>,
    pub exit_code: Option<i32>,
    pub observed_changes: Vec<ArtifactId>,
    pub cause: EventId,
    pub reason: String,
}
impl Effect {
    pub fn transition(
        &self,
        expected: Revision,
        steering: SteeringRevision,
        next: EffectState,
        cause: EventId,
        reason: String,
    ) -> Result<Self> {
        use EffectState::*;
        if self.redaction.is_some() {
            return Err(Error::Transition);
        }
        if self.revision != expected {
            return Err(Error::Stale);
        }
        // Late outcomes may reconcile old work but cannot authorize a fresh dispatch.
        if matches!(next, Validated | Authorized | DispatchRecorded | Running)
            && self.steering != steering
        {
            return Err(Error::Steering);
        }
        if reason.trim().is_empty() {
            return Err(Error::Invalid("reason"));
        }
        let legal = matches!(
            (self.state, next),
            (Proposed, Validated)
                | (Validated, Authorized)
                | (Authorized, DispatchRecorded)
                | (DispatchRecorded, Running | OutcomeUnknown)
                | (Running, Succeeded | Failed | Cancelled | OutcomeUnknown)
                | (OutcomeUnknown, Succeeded | Failed | Cancelled)
                | (Proposed | Validated | Authorized, Cancelled)
        );
        if !legal {
            return Err(Error::Transition);
        }
        let mut result = self.clone();
        result.revision = expected.next()?;
        result.state = next;
        result.cause = cause;
        result.reason = reason;
        Ok(result)
    }
}
