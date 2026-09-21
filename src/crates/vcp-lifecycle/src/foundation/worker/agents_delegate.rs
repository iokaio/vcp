// SPDX-License-Identifier: Apache-2.0
//! Deliberate owner delegation; request data never supplies authority stamps.
use super::super::*;
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{
    agents::*,
    policy::EffectClass,
    task::{Objective, Task},
    verification::Fingerprint,
};
use vcp_repository::{dirty_snapshot::CapturePolicy, worktree::Snapshotter, Root};
use vcp_store::contract::Collection;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DelegationRequest {
    pub objective: String,
    pub acceptance: Vec<String>,
    pub mode: ChildMode,
    pub write_paths: BTreeSet<String>,
    pub untracked_inputs: BTreeSet<String>,
    pub allocation: Micros,
    pub deadline: Timestamp,
    pub required_checks: Vec<String>,
}
impl DelegationRequest {
    fn validate(&self) -> Result<(), String> {
        if self.objective.trim().is_empty()
            || self.objective.len() > 32_768
            || self.acceptance.is_empty()
            || self.acceptance.len() > 64
            || self
                .acceptance
                .iter()
                .chain(&self.required_checks)
                .any(|s| s.trim().is_empty() || s.len() > 4096)
            || self.required_checks.len() > 64
            || self.write_paths.len() > 127
            || self.untracked_inputs.len() > 4096
            || self.allocation == Micros::ZERO
            || self.deadline <= worker::now()
            || (self.mode == ChildMode::ReadOnly && !self.write_paths.is_empty())
            || (self.mode == ChildMode::IsolatedWrite && self.write_paths.is_empty())
        {
            return Err(
                "invalid bounded child objective, acceptance, paths, allocation or deadline".into(),
            );
        }
        for path in self.write_paths.iter().chain(&self.untracked_inputs) {
            if path.is_empty()
                || vcp_repository::path::relative(Path::new(path)).map_err(|e| e.to_string())?
                    != *path
                || path.split('/').any(|part| {
                    part.eq_ignore_ascii_case(".git")
                        || part.eq_ignore_ascii_case(".vcp-child-owner")
                })
            {
                return Err("delegation inputs must be exact relative source paths".into());
            }
        }
        Ok(())
    }
}
impl CanonicalHost {
    /// Persist an assignment and its shared allocation, then qualify its isolated
    /// workspace. This does not start a provider or grant executable effects.
    pub async fn delegate_child(
        &self,
        parent: ThreadId,
        request: DelegationRequest,
        snapshotter: &Snapshotter,
        disposable_parent: &Root,
    ) -> Result<TaskId, String> {
        request.validate()?;
        let generation = scheduler::generation(&self.runtime, parent)?;
        let binding = self.binding(parent)?;
        let checked = binding.clone();
        let requested = request.clone();
        let (parent_revision, parent_steering, model_policy, paths) =
            self.worker.run(move |context| {
                context.can_start(&checked)?;
                context.child_context_scope(&checked)?;
                let task: Task = context
                    .engine
                    .store()
                    .state()
                    .record(
                        Collection::Task,
                        checked.scope.task.as_str(),
                        &checked.scope.workspace,
                    )?
                    .decode()?;
                let provider = context
                    .provider
                    .as_ref()
                    .ok_or("delegation requires the qualified owner model")?;
                if provider.snapshot.valid_until <= worker::now() {
                    return Err("owner model qualification expired".into());
                }
                let source_root = RootId::parse(context.config.workspace.as_str())?;
                let mut paths = vec![ChildPath {
                    root: source_root.clone(),
                    path: String::new(),
                    write: false,
                }];
                paths.extend(requested.write_paths.iter().map(|path| ChildPath {
                    root: source_root.clone(),
                    path: path.clone(),
                    write: true,
                }));
                if let Some((_, ceiling)) = context.child_assignment(&checked.scope.task)? {
                    if paths
                        .iter()
                        .any(|path| !ceiling.paths.iter().any(|allowed| allowed.covers(path)))
                        || requested.deadline > ceiling.deadline
                        || requested.allocation > ceiling.allocation
                        || ceiling.model_policy != provider.snapshot.compatibility.model
                        || (requested.mode == ChildMode::IsolatedWrite
                            && ceiling.mode == ChildMode::ReadOnly)
                    {
                        return Err("delegation exceeds the current parent assignment".into());
                    }
                }
                Ok((
                    task.revision,
                    task.steering,
                    provider.snapshot.compatibility.model.clone(),
                    paths,
                ))
            })?;
        let child = TaskId::new();
        let inputs = self
            .capture_child_workspace(
                parent,
                child.clone(),
                snapshotter,
                &CapturePolicy {
                    untracked: request.untracked_inputs.clone(),
                    required: request.untracked_inputs.clone(),
                    ..Default::default()
                },
                disposable_parent,
            )
            .await?;
        scheduler::check_generation(&self.runtime, parent, generation)?;
        let selected = child.clone();
        self.worker.run(move |context| {
            context.can_start(&binding)?;
            context.initialize_root_budget()?;
            let state = context.engine.store().state();
            let task: Task = state
                .record(
                    Collection::Task,
                    binding.scope.task.as_str(),
                    &binding.scope.workspace,
                )?
                .decode()?;
            if task.revision != parent_revision || task.steering != parent_steering {
                return Err("parent changed during delegation capture".into());
            }
            let provider = context
                .provider
                .as_ref()
                .ok_or("owner model configuration missing")?;
            if provider.snapshot.valid_until <= worker::now()
                || provider.snapshot.compatibility.model != model_policy
            {
                return Err("owner model changed during delegation capture".into());
            }
            let workspace: Workspace = state
                .record(
                    Collection::Workspace,
                    binding.scope.workspace.as_str(),
                    &binding.scope.workspace,
                )?
                .decode()?;
            let policy = vcp_engine::policy::current(state, &binding.scope.workspace)?;
            let graph = vcp_engine::agents::graph(state, &binding.scope, &task.root)?;
            let ledger = vcp_budget::ledger(
                state,
                &Scope {
                    task: task.root.clone(),
                    ..binding.scope.clone()
                },
            )?;
            let spec = ChildSpec {
                parent: binding.scope.task.clone(),
                actor: context.config.actor.clone(),
                parent_steering,
                dependencies: BTreeSet::new(),
                mode: request.mode,
                role: "bounded development child".into(),
                model_policy,
                paths,
                effects: if request.mode == ChildMode::ReadOnly {
                    BTreeSet::from([EffectClass::Read])
                } else {
                    BTreeSet::from([EffectClass::Read, EffectClass::Write])
                },
                authority: workspace.authority,
                policy: policy.revision,
                binding: workspace.binding.revision,
                grants: BTreeMap::new(),
                allocation: request.allocation,
                deadline: request.deadline,
                snapshot: inputs.snapshot,
                snapshot_digest: inputs.snapshot_digest.clone(),
                registration: Some(inputs.registration),
                registration_digest: Some(inputs.registration_digest),
                isolated_root: Some(inputs.isolated_root),
            };
            let fingerprint = Fingerprint {
                repository: inputs.snapshot_digest,
                buffers: task.fingerprint.buffers.clone(),
                environment: task.fingerprint.environment.clone(),
            };
            context.command(
                Command::CreateChild {
                    id: selected,
                    objective: Objective {
                        text: request.objective,
                        constraints: vec![
                            "Use only assigned source paths; executable effects are not delegated."
                                .into(),
                        ],
                        acceptance: request.acceptance,
                        source: task.cause.clone(),
                        steering: SteeringRevision::ZERO,
                    },
                    fingerprint,
                    required_checks: request.required_checks,
                    spec,
                    limits: graph.as_ref().map(|g| g.limits.clone()).unwrap_or_default(),
                    expected_graph: graph.as_ref().map(|g| g.revision),
                    expected_ledger: ledger.revision,
                },
                Some(binding.scope.task),
                task.revision,
            )?;
            Ok(())
        })?;
        self.materialize_child_workspace(parent, child.clone(), snapshotter, disposable_parent).await
            .map_err(|error| format!("child {child} remains registered and pending; workspace preparation failed: {error}"))?;
        Ok(child)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> DelegationRequest {
        DelegationRequest {
            objective: "Review error handling in the captured parser".into(),
            acceptance: vec!["Report concrete malformed-input cases with source paths".into()],
            mode: ChildMode::ReadOnly,
            write_paths: BTreeSet::new(),
            untracked_inputs: BTreeSet::from(["src/parser.rs".into()]),
            allocation: Micros::new(100),
            deadline: Timestamp::new(u64::MAX),
            required_checks: vec![],
        }
    }

    #[test]
    fn delegation_requires_useful_bounded_work_and_exact_scoped_inputs() {
        let base = request();
        assert!(base.validate().is_ok());
        for path in [
            "../outside",
            "C:/outside",
            "src/../outside",
            "src\\parser.rs",
            ".git/config",
            ".vcp-child-owner",
            "",
        ] {
            let mut invalid = base.clone();
            invalid.untracked_inputs = BTreeSet::from([path.into()]);
            assert!(invalid.validate().is_err(), "{path}");
        }
        let mut invalid = base.clone();
        invalid.objective = " ".into();
        assert!(invalid.validate().is_err());
        invalid = base.clone();
        invalid.acceptance.clear();
        assert!(invalid.validate().is_err());
        invalid = base.clone();
        invalid.write_paths.insert("src/parser.rs".into());
        assert!(invalid.validate().is_err());
        invalid.mode = ChildMode::IsolatedWrite;
        assert!(invalid.validate().is_ok());
        invalid.write_paths.clear();
        assert!(invalid.validate().is_err());
        invalid = base;
        invalid.deadline = Timestamp::new(0);
        assert!(invalid.validate().is_err());
    }
}
