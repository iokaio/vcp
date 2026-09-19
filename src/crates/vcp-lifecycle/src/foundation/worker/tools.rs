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
        self.tool_read_access(&RootId::parse(workspace.id.as_str())?, tool)?;
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
    pub fn tool_read_access(&self, root: &RootId, tool: &str) -> Result<()> {
        let policy =
            vcp_engine::policy::current(self.engine.store().state(), &self.config.workspace)?;
        // Preparation also observes source bytes. A read-denied root cannot
        // become readable by asking to prepare a write. Path-specific read
        // denials conservatively hold preparation until a narrower root exists.
        if self
            .config
            .host_tool_denials
            .iter()
            .chain(policy.denials.iter())
            .any(|r| {
                (r.effects.is_empty() || r.effects.contains(&EffectClass::Read))
                    && (r.roots.is_empty() || r.roots.contains(root))
                    && r.tool.as_deref().is_none_or(|name| name == tool)
            })
        {
            return Err("trusted read denial prevents tool preparation".into());
        }
        Ok(())
    }
    /// Canonical-only admission before native reads. A successful preflight is
    /// never sufficient for dispatch: the broker still revalidates native state.
    pub fn tool_preflight(
        &self,
        binding: &ThreadBinding,
        prepared: &vcp_tools::Prepared,
    ) -> Result<vcp_policy::Decision> {
        if self
            .coding_remaining()
            .is_some_and(|remaining| remaining.is_zero())
        {
            return Err("canonical coding deadline elapsed before tool dispatch".into());
        }
        let decision = self.authority_decision(
            binding,
            prepared.authority(),
            &BTreeSet::from([RootId::parse(self.config.workspace.as_str())?]),
            true,
            &BTreeSet::from([Isolation::PathContainment, Isolation::OutputLimit]),
        )?;
        if !matches!(decision, vcp_policy::Decision::Deny { .. }) {
            self.tool_identity(binding, &prepared.authority().operation().tool)?;
        }
        Ok(decision)
    }
    pub fn tool_decision(
        &self,
        binding: &ThreadBinding,
        prepared: &vcp_tools::Prepared,
    ) -> Result<vcp_policy::Decision> {
        let preflight = self.tool_preflight(binding, prepared)?;
        if matches!(preflight, vcp_policy::Decision::Deny { .. }) {
            return Ok(preflight);
        }
        let root = self.tool_root()?;
        self.authority_decision(
            binding,
            prepared.authority(),
            &BTreeSet::from([root.identity.root.clone()]),
            root.identity == prepared.root().identity && root.path() == prepared.root().path(),
            &BTreeSet::from([Isolation::PathContainment, Isolation::OutputLimit]),
        )
    }
    pub fn authority_decision(
        &self,
        binding: &ThreadBinding,
        prepared: &vcp_policy::Prepared,
        roots: &BTreeSet<RootId>,
        resources_current: bool,
        isolation: &BTreeSet<Isolation>,
    ) -> Result<vcp_policy::Decision> {
        let state = self.engine.store().state();
        if prepared.operation().effects != BTreeSet::from([EffectClass::Read])
            && state
                .records
                .values()
                .filter(|row| row.collection == Collection::Effect)
                .filter_map(|row| row.decode::<Effect>().ok())
                .any(|effect| {
                    effect.scope.workspace == binding.scope.workspace
                        && effect.state == EffectState::OutcomeUnknown
                })
        {
            return Ok(vcp_policy::Decision::Deny {
                origin: "effect reconciliation".into(),
                reason:
                    "an uncertain workspace effect requires reconciliation before another mutation"
                        .into(),
            });
        }
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
        Ok(vcp_engine::policy::evaluate(
            state,
            prepared,
            &vcp_policy::Facts {
                workspace: &workspace,
                scope: &binding.scope,
                actor: &self.config.actor,
                steering: task.steering,
                policy: policy.revision,
                now: now(),
                owner_current: self.owner_alive,
                task_running: self.can_start(binding).is_ok(),
                resources_current,
                registered_roots: roots,
                isolation,
                host_denials: &self.config.host_tool_denials,
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
        let mut decision = self.tool_decision(binding, prepared)?;
        if !matches!(decision, vcp_policy::Decision::Deny { .. }) {
            prepared.revalidate()?;
            decision = self.tool_decision(binding, prepared)?;
        }
        self.propose_authority(
            binding,
            prepared.authority(),
            &prepared.evidence()?,
            decision,
        )
    }
    pub fn propose_authority(
        &mut self,
        binding: &ThreadBinding,
        prepared: &vcp_policy::Prepared,
        evidence: &[u8],
        decision: vcp_policy::Decision,
    ) -> Result<(
        ToolRunId,
        ArtifactId,
        vcp_policy::Decision,
        Option<ApprovalId>,
    )> {
        self.can_start(binding)?;
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
            &canonical_bytes(&serde_json::json!({
                "schema": "vcp-prepared-tool/2",
                "controller": self.engine.controller(),
                "owner": self.engine.owner_epoch(),
                "host_tool_denials": self.config.host_tool_denials,
                "prepared": serde_json::from_slice::<serde_json::Value>(evidence)?,
            }))?,
            "vcp-prepared-tool-v2",
        )?;
        let effect = ToolRunId::new();
        self.command(
            Command::ProposeEffect {
                id: effect.clone(),
                operation_digest: prepared.digest().into(),
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
            "registered tool recorded prepared identity and current policy decision",
        )?;
        let mut question = None;
        match &decision {
            vcp_policy::Decision::Question { .. } => {
                let operation = prepared.operation();
                let id = ApprovalId::new();
                self.command(
                    Command::Ask {
                        approval: Approval {
                            id: id.clone(),
                            scope: binding.scope.clone(),
                            effect: effect.clone(),
                            effect_revision: Revision::new(1),
                            steering: task.steering,
                            operation_digest: prepared.digest().into(),
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
