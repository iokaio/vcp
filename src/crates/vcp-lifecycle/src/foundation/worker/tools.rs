// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::collections::BTreeSet;
use vcp_domain::{effect::*, policy::*};
impl Context {
    pub fn tool_root(&self) -> Result<vcp_repository::Root> {
        let workspace: Workspace = self
            .engine
            .store()
            .state()
            .record(
                Collection::Workspace,
                self.config.workspace.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        Ok(vcp_repository::Root::open(
            vcp_repository::RootIdentity {
                workspace: workspace.id.clone(),
                root: RootId::parse(workspace.id.as_str())?,
                repository: workspace.binding.repository,
                worktree: workspace.binding.worktree,
                binding: workspace.binding.revision,
            },
            std::path::Path::new(&workspace.binding.root),
        )?)
    }
    pub fn tool_identity(
        &self,
        binding: &ThreadBinding,
        tool: &str,
    ) -> Result<vcp_tools::Identity> {
        self.can_start(binding)?;
        let workspace: Workspace = self
            .engine
            .store()
            .state()
            .record(
                Collection::Workspace,
                self.config.workspace.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
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
        let policy = vcp_engine::policy::current(self.engine.store().state(), &workspace.id)?;
        if workspace.trust != Trust::Trusted {
            return Err("workspace trust is required before tool preparation reads".into());
        }
        let root = RootId::parse(workspace.id.as_str())?;
        // Preparation also observes source bytes. A read-denied root cannot
        // become readable by asking to prepare a write. Path-specific read
        // denials conservatively hold preparation until a narrower root exists.
        if policy.denials.iter().any(|r| {
            (r.effects.is_empty() || r.effects.contains(&EffectClass::Read))
                && (r.roots.is_empty() || r.roots.contains(&root))
                && r.tool.as_deref().is_none_or(|name| name == tool)
        }) {
            return Err("trusted read denial prevents tool preparation".into());
        }
        Ok(vcp_tools::Identity {
            scope: binding.scope.clone(),
            actor: self.config.actor.clone(),
            host: workspace.binding.host,
            binding: workspace.binding.revision,
            authority: workspace.authority,
            steering: task.steering,
            policy: policy.revision,
        })
    }
    pub fn tool_decision(
        &self,
        binding: &ThreadBinding,
        prepared: &vcp_tools::Prepared,
    ) -> Result<vcp_policy::Decision> {
        let state = self.engine.store().state();
        let workspace: Workspace = state
            .record(
                Collection::Workspace,
                self.config.workspace.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        let task: Task = state
            .record(
                Collection::Task,
                binding.scope.task.as_str(),
                &binding.scope.workspace,
            )?
            .decode()?;
        let policy = vcp_engine::policy::current(state, &workspace.id)?;
        let root = self.tool_root()?;
        let roots = BTreeSet::from([root.identity.root.clone()]);
        let isolation = BTreeSet::from([Isolation::PathContainment, Isolation::OutputLimit]);
        Ok(vcp_engine::policy::evaluate(
            state,
            prepared.authority(),
            &vcp_policy::Facts {
                workspace: &workspace,
                scope: &binding.scope,
                actor: &self.config.actor,
                steering: task.steering,
                policy: policy.revision,
                now: now(),
                owner_current: self.owner_alive,
                task_running: self.can_start(binding).is_ok(),
                resources_current: root.identity == prepared.root().identity
                    && root.path() == prepared.root().path(),
                registered_roots: &roots,
                isolation: &isolation,
                host_denials: &[],
            },
        )?)
    }
    pub fn tool_advance(
        &mut self,
        binding: &ThreadBinding,
        id: &ToolRunId,
        next: EffectState,
        execution: Option<ExecutionId>,
        artifacts: Vec<ArtifactId>,
        reason: &str,
    ) -> Result<()> {
        let current: Effect = self
            .engine
            .store()
            .state()
            .record(Collection::Effect, id.as_str(), &binding.scope.workspace)?
            .decode()?;
        self.command(
            Command::AdvanceEffect {
                id: id.clone(),
                next,
                reason: reason.into(),
                execution,
                exit_code: None,
                observed_changes: artifacts,
            },
            Some(binding.scope.task.clone()),
            current.revision,
        )?;
        Ok(())
    }
    pub fn tool_propose(
        &mut self,
        binding: &ThreadBinding,
        prepared: &vcp_tools::Prepared,
    ) -> Result<(
        ToolRunId,
        ArtifactId,
        vcp_policy::Decision,
        Option<ApprovalId>,
    )> {
        self.can_start(binding)?;
        prepared.revalidate()?;
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
        let plan = self.capture(
            &binding.scope,
            Channel::Evidence,
            &prepared.evidence()?,
            "vcp-prepared-tool-v1",
        )?;
        let effect = ToolRunId::new();
        self.command(
            Command::ProposeEffect {
                id: effect.clone(),
                operation_digest: prepared.authority().digest().into(),
            },
            Some(binding.scope.task.clone()),
            task.revision,
        )?;
        self.tool_advance(
            binding,
            &effect,
            EffectState::Validated,
            None,
            vec![plan.spec.id.clone()],
            "registered tool validated exact arguments and native sources",
        )?;
        let decision = self.tool_decision(binding, prepared)?;
        let mut question = None;
        match &decision {
            vcp_policy::Decision::Question { .. } => {
                let operation = prepared.authority().operation();
                let id = ApprovalId::new();
                self.command(
                    Command::Ask {
                        approval: Approval {
                            id: id.clone(),
                            scope: binding.scope.clone(),
                            effect: effect.clone(),
                            effect_revision: Revision::new(1),
                            steering: task.steering,
                            operation_digest: prepared.authority().digest().into(),
                            actor: self.config.actor.clone(),
                            policy: operation.policy,
                            expires_at: Timestamp::new(now().get().saturating_add(300_000)),
                            state: ApprovalState::Pending,
                            revision: Revision::ZERO,
                            controller: Some(self.engine.controller().clone()),
                            owner_epoch: Some(self.engine.owner_epoch()),
                            authority: Some(operation.authority),
                            binding: Some(operation.binding),
                        },
                    },
                    Some(binding.scope.task.clone()),
                    Revision::new(1),
                )?;
                question = Some(id);
            }
            vcp_policy::Decision::Deny { reason, .. } => self.tool_advance(
                binding,
                &effect,
                EffectState::Cancelled,
                None,
                vec![plan.spec.id.clone()],
                reason,
            )?,
            _ => (),
        }
        Ok((effect, plan.spec.id, decision, question))
    }
}
