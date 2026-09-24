// SPDX-License-Identifier: Apache-2.0
//! Hook receipts are canonical artifacts, never a second execution journal.
use super::*;
use vcp_extensions::hooks::{input::HookInput, planner::PlannedHook};

impl Context {
    /// Recheck the complete batch in the same worker operation that proposes the
    /// final effect. A later hook cannot invalidate an earlier rewrite silently.
    pub fn check_hook_outcomes(
        &self,
        binding: &ThreadBinding,
        outcomes: &[crate::foundation::hooks::HookOutcome],
    ) -> Result<()> {
        for outcome in outcomes {
            let bytes = self
                .hook_artifact(&binding.scope, &outcome.artifact, 2 * 1024 * 1024)?
                .ok_or("hook result missing at final authorization")?;
            let receipt: vcp_extensions::hooks::receipt::HookReceipt =
                serde_json::from_slice(&bytes)?;
            if receipt != outcome.receipt
                || receipt.status == vcp_extensions::hooks::receipt::HookStatus::Blocked
            {
                return Err("hook result blocked or changed before final authorization".into());
            }
            let id = ArtifactId::parse(format!("hook-plan-{}", receipt.identity))?;
            let bytes = self
                .hook_artifact(&binding.scope, &id, 1024 * 1024)?
                .ok_or("hook reservation missing at final authorization")?;
            let reserved: serde_json::Value = serde_json::from_slice(&bytes)?;
            let plan: PlannedHook = serde_json::from_value(reserved["plan"].clone())?;
            plan.validate(vcp_extensions::hooks::input::HookLimits::default())?;
            self.check_hook_result(binding, &plan)?;
        }
        Ok(())
    }
    pub fn hook_pending_current(
        &self,
        binding: &ThreadBinding,
        prepared: &vcp_tools::process::Prepared,
    ) -> Result<bool> {
        let op = prepared.authority().operation();
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
        let workspace: Workspace = self
            .engine
            .store()
            .state()
            .record(
                Collection::Workspace,
                binding.scope.workspace.as_str(),
                &binding.scope.workspace,
            )?
            .decode()?;
        let policy =
            vcp_engine::policy::current(self.engine.store().state(), &binding.scope.workspace)?;
        Ok(self.owner_alive
            && !task.state.terminal()
            && task.steering == op.steering
            && workspace.authority == op.authority
            && workspace.binding.revision == op.binding
            && policy.revision == op.policy
            && self
                .process_profiles
                .get(prepared.profile().name())
                .map(|p| p.digest())
                .transpose()?
                == Some(prepared.profile().digest()?))
    }
    pub fn check_hook_result(&self, binding: &ThreadBinding, plan: &PlannedHook) -> Result<()> {
        self.check_hook_input(binding, &plan.input)?;
        let id = ArtifactId::parse(format!("hook-plan-{}", plan.identity))?;
        let bytes = self
            .hook_artifact(&binding.scope, &id, 1024 * 1024)?
            .ok_or("hook reservation missing")?;
        let reserved: serde_json::Value = serde_json::from_slice(&bytes)?;
        let current = self.prepare_process(binding, super::super::hooks::process_request(plan)?)?;
        if reserved["source_fence"] != super::super::hooks::source_fence(&current)? {
            return Err("hook executable, profile inputs, binding or authority changed; result requires revalidation, not replay".into());
        }
        Ok(())
    }
    pub fn check_hook_input(&self, binding: &ThreadBinding, input: &HookInput) -> Result<()> {
        self.can_start(binding)?;
        self.tool_identity(binding, "vcp_exec")?;
        self.check_hook_projection(binding, input)
    }

    fn check_hook_projection(&self, binding: &ThreadBinding, input: &HookInput) -> Result<()> {
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
        if input.scope != binding.scope
            || input.root_task != self.config.root_task
            || input.steering_revision != task.steering.get()
        {
            return Err("hook scope or steering changed; create a fresh lifecycle input".into());
        }
        for id in &input.artifact_refs {
            let descriptor: ArtifactDescriptor = self
                .engine
                .store()
                .state()
                .record(Collection::Artifact, id.as_str(), &binding.scope.workspace)?
                .decode()?;
            if descriptor.spec.scope != binding.scope || descriptor.state != CaptureState::Complete
            {
                return Err("hook artifact outside the current task or not complete".into());
            }
            // Apply retention/export fences before disclosing even a reference.
            vcp_audit::history::History::read_artifact(
                self.engine.store(),
                &self.history_access(),
                id,
                std::io::sink(),
            )?;
        }
        Ok(())
    }

    pub fn hook_artifact(
        &self,
        scope: &Scope,
        id: &ArtifactId,
        limit: u64,
    ) -> Result<Option<Vec<u8>>> {
        let Some(row) = self
            .engine
            .store()
            .state()
            .records
            .get(&key(Collection::Artifact, id.as_str()))
        else {
            return Ok(None);
        };
        let descriptor: ArtifactDescriptor = row.decode()?;
        if descriptor.spec.scope != *scope || descriptor.length.get() > limit {
            return Err("hook artifact scope or byte ceiling".into());
        }
        let mut bytes = Vec::new();
        vcp_audit::history::History::read_artifact(
            self.engine.store(),
            &self.history_access(),
            id,
            &mut bytes,
        )?;
        Ok(Some(bytes))
    }

    pub fn capture_hook(
        &mut self,
        scope: &Scope,
        id: ArtifactId,
        bytes: &[u8],
        schema: &str,
    ) -> Result<ArtifactDescriptor> {
        if self
            .engine
            .store()
            .state()
            .records
            .contains_key(&key(Collection::Artifact, id.as_str()))
        {
            return Err(
                "hook identity already recorded; inspect/reconcile instead of replay".into(),
            );
        }
        let mut spec = self.spec(scope, Channel::Evidence, schema);
        spec.id = id;
        spec.source = "vcp-hook".into();
        let mut writer = self.engine.store().spool().create(spec)?;
        for chunk in bytes.chunks(vcp_store::artifact::CHUNK_BYTES) {
            writer.write_chunk(chunk)?;
        }
        let descriptor = writer.finalize()?;
        drop(writer);
        self.command(
            Command::AttachArtifact {
                descriptor: descriptor.clone(),
            },
            Some(scope.task.clone()),
            Revision::ZERO,
        )?;
        Ok(descriptor)
    }

    pub fn reserve_hook(
        &mut self,
        binding: &ThreadBinding,
        plan: &PlannedHook,
        id: ArtifactId,
        effect: &ToolRunId,
        source_fence: serde_json::Value,
    ) -> Result<()> {
        // Preparation may have just asked for approval and moved the task to
        // WaitingForInput. Recording that pending input grants no dispatch.
        // Dispatch separately requires can_start and current process authority.
        self.check_hook_projection(binding, &plan.input)?;
        let current: vcp_domain::effect::Effect = self
            .engine
            .store()
            .state()
            .record(
                Collection::Effect,
                effect.as_str(),
                &binding.scope.workspace,
            )?
            .decode()?;
        if current.scope != binding.scope || current.steering.get() != plan.input.steering_revision
        {
            return Err("hook effect scope changed before input capture".into());
        }
        let profile = self
            .process_profiles
            .get(&plan.definition.command.profile)
            .ok_or("hook process profile is not configured")?;
        if profile.mode() != vcp_tools::process::Mode::Direct || profile.terminal().is_some() {
            return Err("hook requires a direct nonterminal profile".into());
        }
        self.capture_hook(&binding.scope, id, &canonical_bytes(&serde_json::json!({
                "schema_version":1,"plan":plan,"effect":effect,"source_fence":source_fence,
            "recovery":"reserved once; inspect the ordinary effect journal, never automatically replay"
        }))?, "vcp-hook-plan-v1")?;
        Ok(())
    }
}
