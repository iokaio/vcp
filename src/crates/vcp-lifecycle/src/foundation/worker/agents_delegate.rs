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
    #[serde(default = "default_role")]
    pub role: String,
    #[serde(default = "default_read_paths")]
    pub read_paths: BTreeSet<String>,
    #[serde(default)]
    pub helper: Option<HelperTemplate>,
    pub objective: String,
    pub acceptance: Vec<String>,
    pub mode: ChildMode,
    pub write_paths: BTreeSet<String>,
    pub untracked_inputs: BTreeSet<String>,
    pub allocation: Micros,
    pub deadline: Timestamp,
    pub required_checks: Vec<String>,
}
fn default_role() -> String {
    "bounded development child".into()
}
fn default_read_paths() -> BTreeSet<String> {
    BTreeSet::from([String::new()])
}

/// Versioned convenience instructions, never authority or a model selection.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelperTemplate {
    pub name: String,
    pub revision: u32,
}
impl HelperTemplate {
    pub const REVISION: u32 = 2;
    pub fn guidance(&self) -> Result<&'static str, String> {
        if self.revision != Self::REVISION {
            return Err("helper template revision is stale; select the current template".into());
        }
        match self.name.as_str() {
            "explore" => Ok("Explore the assigned read scope. Return relevant source paths and line ranges, supporting evidence and a short synthesis. Report missing inputs and uncertainty. Do not execute processes or edit files."),
            "review" => Ok(concat!(
                "Review the assigned read scope. First read the affected contract and the supplied current/base sources. ",
                "Trace each candidate trigger through both versions before assigning causality: introduced means the current version fails where the base did not; pre_existing means the same defect is present in both. ",
                "Use unknown only when comparison evidence is unavailable or inconclusive, and explain the missing evidence. ",
                "Check that each trigger belongs to the contract's supported input domain. Unspecified behavior for unsupported inputs is not a demonstrated defect; keep such suggestions separate from findings. ",
                "Inspect benign neighboring behavior as a control and do not invent findings to fill a quota. ",
                "Report supported findings with location, concrete input, expected and actual behavior, consequence, evidence and uncertainty. ",
                "Cite the contract and both examined source versions using readable paths/ranges and retained evidence artifact IDs when available. ",
                "Distinguish static reasoning from executed reproduction; do not claim checks ran without their results. ",
                "State the examined scope and remaining uncertainty. No findings means no supported findings in this scope, not guaranteed correctness. Do not execute processes or edit files."
            )),
            _ => Err("unknown helper template; choose explore or review".into()),
        }
    }
}
impl DelegationRequest {
    fn validate(&self) -> Result<(), String> {
        if self.role.trim().is_empty()
            || self.role.len() > 128
            || self.read_paths.is_empty()
            || self.read_paths.len() + self.write_paths.len() > 128
            || self.objective.trim().is_empty()
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
        if let Some(helper) = &self.helper {
            helper.guidance()?;
            if self.mode != ChildMode::ReadOnly
                || self.role != helper.name
                || !self.required_checks.is_empty()
            {
                return Err(
                    "helper templates require their named read-only role with no process checks"
                        .into(),
                );
            }
        }
        // Context assembly currently consumes the complete qualified snapshot.
        // A narrow tool scope must not implicitly authorize broader model input.
        if !self.read_paths.contains("") {
            return Err("child context requires whole-snapshot read scope; narrower context assembly is not yet qualified".into());
        }
        for path in self
            .write_paths
            .iter()
            .chain(&self.untracked_inputs)
            .chain(self.read_paths.iter().filter(|p| !p.is_empty()))
        {
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
                let mut paths: Vec<_> = requested
                    .read_paths
                    .iter()
                    .map(|path| ChildPath {
                        root: source_root.clone(),
                        path: path.clone(),
                        write: false,
                    })
                    .collect();
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
                role: request.role,
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
            let mut constraints = vec![
                "Use only assigned source paths; executable effects are not delegated.".into(),
            ];
            let mut objective = request.objective;
            if let Some(helper) = request.helper {
                let guidance = helper.guidance()?;
                constraints.push(format!(
                    "helper-template:{}@{}: {guidance}",
                    helper.name, helper.revision
                ));
                objective.push_str("\n\n");
                objective.push_str(guidance);
            }
            context.command(
                Command::CreateChild {
                    id: selected,
                    objective: Objective {
                        text: objective,
                        constraints,
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
            role: default_role(),
            read_paths: default_read_paths(),
            helper: None,
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
    fn helper_templates_reject_stale_versions_and_expanded_authority() {
        for name in ["explore", "review"] {
            let mut child = request();
            child.role = name.into();
            child.helper = Some(HelperTemplate {
                name: name.into(),
                revision: HelperTemplate::REVISION,
            });
            assert!(child.validate().is_ok());
            child.read_paths = BTreeSet::from(["src".into()]);
            assert!(child.validate().is_err());
            child.read_paths = default_read_paths();
            for stale in [0, 1] {
                child.helper.as_mut().unwrap().revision = stale;
                assert!(child.validate().is_err(), "stale helper revision {stale}");
            }
            child.helper.as_mut().unwrap().revision = HelperTemplate::REVISION;
            child.required_checks.push("process#test".into());
            assert!(child.validate().is_err());
            child.required_checks.clear();
            child.mode = ChildMode::IsolatedWrite;
            child.write_paths.insert("src/parser.rs".into());
            assert!(child.validate().is_err());
        }
        let mut custom = request();
        custom.role = "project-specific researcher".into();
        assert!(custom.validate().is_ok());
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
