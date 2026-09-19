// SPDX-License-Identifier: Apache-2.0
//! Exit evidence is read from one immutable canonical snapshot.
use crate::exit_status::Conditions;
use vcp_domain::{
    accounting::{Attempt, ReservationState},
    effect::{Effect, EffectState},
    task::{Task, TaskState, Turn, TurnState},
    workspace::Scope,
};
use vcp_lifecycle::foundation::CanonicalHost;
use vcp_protocol::command::{Approval, ApprovalState, CommandReceipt};
use vcp_store::contract::Collection;

pub struct Outcome {
    pub task: Task,
    pub conditions: Conditions,
    pub receipt: CommandReceipt,
    pub approvals: Vec<Approval>,
}

impl Outcome {
    /// Do not infer budget denials, required input, or successful verification
    /// from diagnostic text. Historical turn failures belong to their original
    /// turn; only the latest turn contributes its terminal condition.
    pub fn read(host: &CanonicalHost, scope: &Scope) -> Result<Self, String> {
        let state = host.snapshot()?;
        let task: Task = state
            .record(Collection::Task, scope.task.as_str(), &scope.workspace)
            .and_then(|row| row.decode())
            .map_err(|e| e.to_string())?;
        if task.scope != *scope {
            return Err("task scope denied".into());
        }
        let tasks: Vec<Task> = state
            .records
            .values()
            .filter(|row| row.collection == Collection::Task && row.workspace == scope.workspace)
            .map(|row| row.decode().map_err(|e| e.to_string()))
            .collect::<Result<_, _>>()?;
        let mut included = std::collections::BTreeSet::from([scope.task.clone()]);
        loop {
            let before = included.len();
            for child in &tasks {
                if child.scope.session == scope.session
                    && child
                        .parent
                        .as_ref()
                        .is_some_and(|parent| included.contains(parent))
                {
                    included.insert(child.scope.task.clone());
                }
            }
            if before == included.len() {
                break;
            }
        }
        let event = state
            .events
            .iter()
            .find(|event| {
                event.event.id == task.cause
                    && event.event.workspace == scope.workspace
                    && event.event.session == scope.session
                    && event.event.task.as_ref() == Some(&scope.task)
            })
            .ok_or("task has no durable cause")?;
        let receipt = state
            .commands
            .values()
            .find(|receipt| {
                receipt.command == event.event.correlation && receipt.workspace == scope.workspace
            })
            .cloned()
            .ok_or("task cause has no command receipt")?;
        let mut conditions = Conditions {
            cancelled: task.state == TaskState::Cancelled,
            required_input: task.state == TaskState::WaitingForInput,
            incomplete: matches!(task.state, TaskState::Failed | TaskState::Blocked),
            durably_paused: task.state == TaskState::Paused,
            completed: task.state == TaskState::Completed,
            ..Conditions::default()
        };
        let mut approvals = Vec::new();
        let mut turns = Vec::new();
        for record in state
            .records
            .values()
            .filter(|row| row.workspace == scope.workspace)
        {
            match record.collection {
                Collection::Effect => {
                    let effect: Effect = record.decode().map_err(|e| e.to_string())?;
                    if effect.scope.session == scope.session
                        && included.contains(&effect.scope.task)
                    {
                        conditions.unresolved_effect |= matches!(
                            effect.state,
                            EffectState::DispatchRecorded
                                | EffectState::Running
                                | EffectState::OutcomeUnknown
                        );
                    }
                }
                Collection::Attempt => {
                    let attempt: Attempt = record.decode().map_err(|e| e.to_string())?;
                    if attempt.scope.session == scope.session
                        && included.contains(&attempt.scope.task)
                    {
                        conditions.unresolved_effect |= matches!(
                            attempt.phase,
                            ReservationState::Submitted | ReservationState::ReconciliationPending
                        );
                    }
                }
                Collection::Approval => {
                    let approval: Approval = record.decode().map_err(|e| e.to_string())?;
                    if approval.scope.session == scope.session
                        && included.contains(&approval.scope.task)
                        && approval.state == ApprovalState::Pending
                    {
                        approvals.push(approval);
                        conditions.required_input = true;
                    }
                }
                Collection::Turn => {
                    let turn: Turn = record.decode().map_err(|e| e.to_string())?;
                    if turn.scope == *scope && turn.steering == task.steering {
                        turns.push(turn);
                    }
                }
                _ => {}
            }
        }
        let latest = turns
            .iter()
            .filter_map(|turn| {
                state
                    .events
                    .iter()
                    .find(|event| event.event.id == turn.cause)
                    .map(|event| (event.watermark, turn))
            })
            .max_by_key(|(watermark, _)| *watermark);
        if let Some((_, turn)) = latest {
            conditions.budget_exhausted = turn.state == TurnState::BudgetExhausted;
            conditions.required_input |= turn.state == TurnState::WaitingForInput;
            conditions.incomplete |= matches!(turn.state, TurnState::Failed | TurnState::Blocked);
        }
        // Current failed verification is distinct from a saved pause. An old
        // report cannot override later checks or newly steered acceptance.
        for event in state.events.iter().rev().filter(|event| {
            event.event.workspace == scope.workspace
                && event.event.session == scope.session
                && event.event.task.as_ref() == Some(&scope.task)
                && event.event.kind == vcp_protocol::event::EventKind::VerificationRecorded
        }) {
            let facts = event.event.data["facts"]
                .as_array()
                .ok_or("verification event facts missing")?;
            let id = facts
                .iter()
                .find(|fact| fact["collection"] == "verification")
                .and_then(|fact| fact["id"].as_str())
                .ok_or("verification event reference missing")?;
            let report: vcp_domain::verification::Verification = state
                .record(Collection::Verification, id, &scope.workspace)
                .and_then(|row| row.decode())
                .map_err(|e| e.to_string())?;
            if report.applies(scope, task.steering, &task.fingerprint) {
                conditions.incomplete |= !report.satisfies(&task.required_checks, task.editing);
                break;
            }
        }
        Ok(Self {
            task,
            conditions,
            receipt,
            approvals,
        })
    }
}
