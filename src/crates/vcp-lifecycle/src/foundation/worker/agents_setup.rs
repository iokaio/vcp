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
        if !self.owner_alive || self.authority_pending || self.capture_admission_blocked() {
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
            let mut coding = context.parent_coding_config(&parent)?;
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
            // Parent operating instructions may describe process checks or
            // external tools that this isolated assignment cannot use. Explain
            // the actual child boundary before its first provider request.
            let child_verification = if coding.canonical_tools.contains("vcp_verify") {
                "vcp_verify with no assigned process checks can record child source evidence; it does not prove parent acceptance."
            } else {
                "Model verification is unavailable under the inherited tool ceiling; the host performs applicable source-integrity completion checks."
            };
            coding.operating.push_str(&format!(
                "\nIsolated child workflow: the root and all children share a total ceiling of {} model requests; this is not a fresh child allowance. Read the assignment and contract, gather the necessary source once, then act. Reuse complete unchanged source already in context; reread only missing ranges or changed files. Available tool schemas do not grant authority. Child process execution is unavailable, including vcp_exec and process checks through vcp_verify. Do not retry a denied process through another tool or discover unrelated MCP services to run it. Report required process checks as not run for the parent to execute against the integrated result. {} Before the final answer, inspect any edits, report the result and cite retained evidence.\nChild mode: {}.",
                coding.max_requests,
                child_verification,
                match spec.mode {
                    ChildMode::ReadOnly => "read only; inspect and report without editing",
                    ChildMode::IsolatedWrite if !coding.canonical_tools.contains("vcp_patch") => "isolated write assignment; the inherited ceiling has no patch tool, so report editing unavailable and return supported findings",
                    ChildMode::IsolatedWrite => "isolated write; modify only assigned write paths. Before editing, turn the stated contract into a short checklist, including input validation and boundary cases. Validate container types before reading properties. Distinguish omitted optional values from explicitly invalid values such as null; do not use nullish defaults when only omission permits a default. Use vcp_patch in its documented *** Begin Patch format with exact current context. Group related changes in one patch where practical, then read changed sources and check every contract item against the result. If a patch is rejected, use the error to correct its format or context instead of repeating it unchanged. Return the patch with unavailable process checks explicitly not run; an incomplete child verification does not require retries when only the parent can perform the remaining checks",
                },
            ));
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
