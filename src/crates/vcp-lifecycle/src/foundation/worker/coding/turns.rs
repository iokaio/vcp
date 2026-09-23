// SPDX-License-Identifier: Apache-2.0
//! Canonical stages of an explicitly submitted retained coding turn.
use super::*;
mod identity;

impl Context {
    pub fn begin_coding_turn(&mut self, binding: &ThreadBinding, input: String) -> Result<TurnId> {
        self.begin_coding_turn_identified(binding, input, TurnId::new())
    }

    pub fn begin_coding_turn_identified(
        &mut self,
        binding: &ThreadBinding,
        input: String,
        id: TurnId,
    ) -> Result<TurnId> {
        self.can_start(binding)?;
        if input.trim().is_empty() || input.len() > 65_536 || input.contains('\0') {
            return Err("turn input must contain bounded UTF-8 text".into());
        }
        identity::require_unused(self.engine.store().state(), &id)?;
        let previous = self
            .coding
            .get(&binding.scope.task)
            .ok_or("coding setup missing")?
            .turn
            .clone();
        if let Some(id) = previous {
            let previous: Turn = self
                .engine
                .store()
                .state()
                .record(Collection::Turn, id.as_str(), &binding.scope.workspace)?
                .decode()?;
            if !matches!(
                previous.state,
                TurnState::Completed
                    | TurnState::Failed
                    | TurnState::Cancelled
                    | TurnState::Paused
                    | TurnState::WaitingForInput
                    | TurnState::Blocked
                    | TurnState::BudgetExhausted
            ) {
                return Err("a canonical coding turn is already active".into());
            }
        }
        let task: Task = self
            .engine
            .store()
            .state()
            .record(
                Collection::Task,
                binding.scope.task.as_str(),
                &binding.scope.workspace,
            )?
            .decode()?;
        let trigger = self.capture(
            &binding.scope,
            Channel::Evidence,
            input.as_bytes(),
            "coding-turn-input/1",
        )?;
        self.command(
            Command::StartTurn {
                id: id.clone(),
                trigger: trigger.spec.id,
            },
            Some(binding.scope.task.clone()),
            task.revision,
        )?;
        self.coding
            .get_mut(&binding.scope.task)
            .ok_or("coding setup missing")?
            .turn = Some(id.clone());
        Ok(id)
    }

    pub(crate) fn coding_stage(
        &mut self,
        binding: &ThreadBinding,
        next: TurnState,
        reason: &str,
    ) -> Result<()> {
        let Some(id) = self
            .coding
            .get(&binding.scope.task)
            .and_then(|state| state.turn.clone())
        else {
            return Ok(());
        };
        let current: Turn = self
            .engine
            .store()
            .state()
            .record(Collection::Turn, id.as_str(), &binding.scope.workspace)?
            .decode()?;
        if current.state == next {
            return Ok(());
        }
        // Qualified retries are attempts within the same model-request stage.
        // Their individual context, reservation and request records remain
        // canonical; a retry is not a fabricated tool-execution transition.
        if current.state == TurnState::RequestingModel
            && matches!(
                next,
                TurnState::AssemblingContext | TurnState::ReservingBudget
            )
            && self
                .provider
                .as_ref()
                .is_some_and(|provider| provider.retries.contains_key(&binding.scope.task))
        {
            return Ok(());
        }
        self.command(
            Command::AdvanceTurn {
                id,
                next,
                reason: reason.into(),
            },
            Some(binding.scope.task.clone()),
            current.revision,
        )?;
        Ok(())
    }

    pub(crate) fn stop_coding_turns(&mut self, reason: &str) -> Result<()> {
        let turns: Vec<Turn> = self
            .engine
            .store()
            .state()
            .records
            .values()
            .filter(|row| {
                row.collection == Collection::Turn && row.workspace == self.config.workspace
            })
            .map(Record::decode)
            .collect::<std::result::Result<_, vcp_store::Error>>()?;
        for turn in turns {
            if turn.scope.session != self.config.session {
                continue;
            }
            if matches!(
                turn.state,
                TurnState::Completed | TurnState::Failed | TurnState::Cancelled
            ) {
                continue;
            }
            let task: Task = self
                .engine
                .store()
                .state()
                .record(
                    Collection::Task,
                    turn.scope.task.as_str(),
                    &turn.scope.workspace,
                )?
                .decode()?;
            if turn.steering != task.steering {
                continue;
            }
            // Keep independent stop causes visible even when the enclosing
            // task is subsequently cancelled. Cancellation is already durable
            // on the task; it must not erase exhaustion or required input.
            if matches!(
                turn.state,
                TurnState::BudgetExhausted | TurnState::Blocked | TurnState::WaitingForInput
            ) {
                continue;
            }
            let next = match task.state {
                TaskState::Cancelled if turn.state == TurnState::Cancelling => TurnState::Cancelled,
                TaskState::Cancelled => TurnState::Cancelling,
                TaskState::Paused => TurnState::Paused,
                TaskState::WaitingForInput => TurnState::WaitingForInput,
                TaskState::Blocked => TurnState::Blocked,
                TaskState::Failed => TurnState::Failed,
                _ => continue,
            };
            if turn.state == next {
                continue;
            }
            self.command(
                Command::AdvanceTurn {
                    id: turn.id.clone(),
                    next,
                    reason: reason.into(),
                },
                Some(turn.scope.task.clone()),
                turn.revision,
            )?;
            if next == TurnState::Cancelling {
                self.command(
                    Command::AdvanceTurn {
                        id: turn.id,
                        next: TurnState::Cancelled,
                        reason: reason.into(),
                    },
                    Some(turn.scope.task),
                    turn.revision.next()?,
                )?;
            }
        }
        Ok(())
    }
}
