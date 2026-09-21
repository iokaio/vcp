// SPDX-License-Identifier: Apache-2.0
//! Configure isolated child loops without copying parent conversation history.
use super::super::*;
use vcp_domain::{
    agents::ChildMode,
    task::{Task, TaskState},
};
use vcp_store::contract::Collection;

impl worker::Context {
    pub(super) fn task_has_streams(&self, binding: &ThreadBinding) -> worker::Result<bool> {
        for id in self.streams.keys() {
            let attempt: vcp_domain::accounting::Attempt = self
                .engine
                .store()
                .state()
                .record(Collection::Attempt, id.as_str(), &binding.scope.workspace)?
                .decode()?;
            if attempt.scope == binding.scope {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub(super) fn child_held_setup_access(&self, binding: &ThreadBinding) -> worker::Result<()> {
        self.validate_binding(binding)?;
        if !self.owner_alive || self.authority_pending || self.interrupted_capture {
            return Err("child setup owner or capture recovery is unavailable".into());
        }
        self.child_assignment(&binding.scope.task)?
            .ok_or("held setup requires a registered child")?;
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
        if task.state != TaskState::Paused {
            return Err("held child setup requires paused state".into());
        }
        self.integration_child_quiescent(&binding.scope.task)?;
        self.child_context_scope(binding)?;
        self.recovery_task_root(&binding.scope.task)?;
        Ok(())
    }
}
impl CanonicalHost {
    /// Clone trusted setup only. A recovered child stays locally held/paused;
    /// fresh context and history remain scoped to its own canonical task.
    pub fn configure_child_from_parent(
        &self,
        parent: ThreadId,
        child: ThreadId,
    ) -> Result<(), String> {
        let parent = self.binding(parent)?;
        let binding = self.binding(child)?;
        let view = self
            .runtime
            .inspect(child)
            .map_err(|e| format!("child setup owner: {e:?}"))?;
        if !view.owner_attached {
            return Err("child setup requires retained ownership".into());
        }
        let held = view.local_hold;
        self.worker.run(move |context| {
            let (_, spec) = context
                .child_assignment(&binding.scope.task)?
                .ok_or("child assignment missing")?;
            if spec.parent != parent.scope.task
                || parent.scope.workspace != binding.scope.workspace
                || parent.scope.session != binding.scope.session
            {
                return Err("child setup parent differs".into());
            }
            if held {
                context.child_held_setup_access(&binding)?;
            } else {
                context.can_start(&binding)?;
            }
            let coding = context.parent_coding_config(&parent)?;
            // The cumulative request/deadline configuration is a root ceiling.
            // The child's independent deadline is checked by child_assignment.
            let mut verification = context.parent_verification_config(&parent)?;
            let task: Task = context
                .engine
                .store()
                .state()
                .record(
                    Collection::Task,
                    binding.scope.task.as_str(),
                    &binding.scope.workspace,
                )?
                .decode()?;
            if spec.mode == ChildMode::ReadOnly {
                if !task.required_checks.is_empty() {
                    return Err("read-only child declares process checks".into());
                }
                verification.requirements.clear();
            } else {
                verification.requirements.retain(|r| {
                    task.required_checks
                        .contains(&format!("{}#test", r.manifest))
                });
                if task.required_checks.iter().any(|check| {
                    !verification
                        .requirements
                        .iter()
                        .any(|r| *check == format!("{}#test", r.manifest))
                }) {
                    return Err("child check is absent from trusted parent setup".into());
                }
            }
            if !context.verification.contains_key(&binding.scope.task) {
                context.configure_verification_setup(&binding, verification, held)?;
            }
            if !context.coding.contains_key(&binding.scope.task) {
                context.configure_coding_setup(&binding, coding, held)?;
            }
            Ok(())
        })
    }
}
