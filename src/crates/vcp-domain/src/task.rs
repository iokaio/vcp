// SPDX-License-Identifier: Apache-2.0
use crate::{verification::*, workspace::Scope, *};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Pending,
    Running,
    WaitingForInput,
    Blocked,
    Paused,
    Completed,
    Failed,
    Cancelled,
}
impl TaskState {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Objective {
    pub text: String,
    pub constraints: Vec<String>,
    pub acceptance: Vec<String>,
    pub source: EventId,
    pub steering: SteeringRevision,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub scope: Scope,
    pub root: TaskId,
    pub parent: Option<TaskId>,
    pub fork_origin: Option<TaskId>,
    pub revision: Revision,
    pub steering: SteeringRevision,
    pub objectives: Vec<Objective>,
    pub state: TaskState,
    pub fingerprint: Fingerprint,
    pub editing: bool,
    pub required_checks: Vec<String>,
    pub cause: EventId,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redaction: Option<crate::redaction::ContentRedaction>,
}

/// Produced by trusted host revalidation, not deserialized from a user command.
pub struct ResumeEvidence {
    pub workspace_current: bool,
    pub policy_current: bool,
    pub budget_current: bool,
    pub effects_reconciled: bool,
    pub owner_current: bool,
}
impl ResumeEvidence {
    pub fn valid(&self) -> bool {
        self.workspace_current
            && self.policy_current
            && self.budget_current
            && self.effects_reconciled
            && self.owner_current
    }
}
impl Task {
    pub fn validate(&self) -> Result<()> {
        self.fingerprint.validate()?;
        if let Some(redaction) = &self.redaction {
            redaction.validate()?;
            if !self.state.terminal()
                || !self.objectives.is_empty()
                || !self.required_checks.is_empty()
                || !self.reason.is_empty()
                || (self.parent.is_none() && self.root != self.scope.task)
                || self.parent.as_ref() == Some(&self.scope.task)
            {
                return Err(Error::Invalid("purged terminal task"));
            }
            return Ok(());
        }
        if self.objectives.is_empty()
            || self.reason.trim().is_empty()
            || self
                .objectives
                .iter()
                .any(|o| o.text.trim().is_empty() || o.text.len() > 65536)
            || self.objectives.last().unwrap().steering != self.steering
            || self
                .objectives
                .windows(2)
                .any(|w| w[0].steering >= w[1].steering)
            || (self.parent.is_none() && self.root != self.scope.task)
            || self.parent.as_ref() == Some(&self.scope.task)
        {
            return Err(Error::Invalid("task"));
        }
        Ok(())
    }
    pub fn can_dispatch(
        &self,
        scope: &Scope,
        steering: SteeringRevision,
        ancestors_running: bool,
    ) -> bool {
        &self.scope == scope
            && self.state == TaskState::Running
            && self.steering == steering
            && ancestors_running
    }
    pub fn transition(
        &self,
        scope: &Scope,
        expected: Revision,
        steering: SteeringRevision,
        next: TaskState,
        cause: EventId,
        reason: String,
        verification: Option<&Verification>,
        resume: Option<&ResumeEvidence>,
    ) -> Result<Self> {
        if &self.scope != scope {
            return Err(Error::Scope);
        }
        if self.redaction.is_some() {
            return Err(Error::Transition);
        }
        if self.revision != expected {
            return Err(Error::Stale);
        }
        if self.steering != steering {
            return Err(Error::Steering);
        }
        if reason.trim().is_empty() || reason.len() > 4096 {
            return Err(Error::Invalid("transition reason"));
        }
        if self.state.terminal() || self.state == next {
            return Err(Error::Transition);
        }
        if next == TaskState::Completed {
            if self.state != TaskState::Running
                || !verification.is_some_and(|v| {
                    v.applies(scope, steering, &self.fingerprint)
                        && v.satisfies(&self.required_checks, self.editing)
                })
            {
                return Err(Error::Evidence);
            }
        } else if next == TaskState::Running {
            if self.state != TaskState::Pending && !resume.is_some_and(ResumeEvidence::valid) {
                return Err(Error::Revalidation);
            }
        } else if next == TaskState::Pending {
            return Err(Error::Transition);
        }
        let mut result = self.clone();
        result.revision = expected.next()?;
        result.state = next;
        result.cause = cause;
        result.reason = reason;
        Ok(result)
    }
    pub fn steer(&self, expected: Revision, mut objective: Objective) -> Result<Self> {
        if self.redaction.is_some() {
            return Err(Error::Transition);
        }
        if self.revision != expected {
            return Err(Error::Stale);
        }
        if self.state.terminal() {
            return Err(Error::Transition);
        }
        let mut next = self.clone();
        next.revision = expected.next()?;
        next.steering = self.steering.next()?;
        objective.steering = next.steering;
        next.cause = objective.source.clone();
        next.objectives.push(objective);
        next.validate()?;
        Ok(next)
    }
    pub fn observe_fingerprint(
        &self,
        expected: Revision,
        fingerprint: Fingerprint,
        cause: EventId,
    ) -> Result<Self> {
        if self.redaction.is_some() {
            return Err(Error::Transition);
        }
        if self.revision != expected {
            return Err(Error::Stale);
        }
        fingerprint.validate()?;
        let mut next = self.clone();
        next.revision = expected.next()?;
        next.fingerprint = fingerprint;
        next.cause = cause;
        Ok(next)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnState {
    Queued,
    AssemblingContext,
    ReservingBudget,
    RequestingModel,
    ProcessingResponse,
    AwaitingApproval,
    ExecutingTools,
    Verifying,
    Completed,
    WaitingForInput,
    Paused,
    Blocked,
    BudgetExhausted,
    Failed,
    Cancelling,
    Cancelled,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Turn {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redaction: Option<crate::redaction::ContentRedaction>,
    pub id: TurnId,
    pub scope: Scope,
    pub revision: Revision,
    pub steering: SteeringRevision,
    pub state: TurnState,
    pub trigger: ArtifactId,
    pub cause: EventId,
    pub reason: String,
}
impl Turn {
    pub fn transition(
        &self,
        expected: Revision,
        current_steering: SteeringRevision,
        next: TurnState,
        cause: EventId,
        reason: String,
        resume: Option<&ResumeEvidence>,
    ) -> Result<Self> {
        use TurnState::*;
        if self.redaction.is_some() {
            return Err(Error::Transition);
        }
        if self.revision != expected {
            return Err(Error::Stale);
        }
        if self.steering != current_steering {
            return Err(Error::Steering);
        }
        if reason.trim().is_empty() {
            return Err(Error::Invalid("reason"));
        }
        let suspended = matches!(
            self.state,
            WaitingForInput | Paused | Blocked | BudgetExhausted
        );
        let legal = match (self.state, next) {
            (Completed | Failed | Cancelled, _) => false,
            (Cancelling, Cancelled) => true,
            (Cancelling, _) => false,
            (_, Paused | Blocked | BudgetExhausted | Failed | Cancelling | WaitingForInput) => {
                self.state != next
            }
            (Queued, AssemblingContext)
            | (AssemblingContext, ReservingBudget)
            | (ReservingBudget, RequestingModel)
            | (RequestingModel, ProcessingResponse)
            | (ProcessingResponse, AwaitingApproval | ExecutingTools | Verifying)
            | (AwaitingApproval, ExecutingTools)
            | (ExecutingTools, AssemblingContext)
            | (Verifying, Completed) => true,
            (_, AssemblingContext) if suspended => resume.is_some_and(ResumeEvidence::valid),
            _ => false,
        };
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
