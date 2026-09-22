// SPDX-License-Identifier: Apache-2.0
//! Child workspace and broker ceilings read only canonical graph assignments.
use super::*;
use vcp_domain::{agents::*, policy::*};
use vcp_repository::{worktree::WorkspaceRegistration, Root};

impl Context {
    pub(super) fn child_model_scope(&self, binding: &ThreadBinding, model: &str) -> Result<()> {
        if self
            .child_assignment(&binding.scope.task)?
            .is_some_and(|(_, spec)| spec.model_policy != model)
        {
            return Err("selected model exceeds the child's fixed model assignment".into());
        }
        Ok(())
    }
    pub(super) fn child_context_scope(&self, binding: &ThreadBinding) -> Result<()> {
        let Some((_, spec)) = self.child_assignment(&binding.scope.task)? else {
            return Ok(());
        };
        let root = RootId::parse(self.config.workspace.as_str())?;
        if !spec
            .paths
            .iter()
            .any(|path| path.root == root && path.path.is_empty())
        {
            return Err("full-snapshot context requires an explicit snapshot read scope".into());
        }
        Ok(())
    }
    pub(super) fn child_process_scope(&self, binding: &ThreadBinding) -> Result<()> {
        let Some((_, spec)) = self.child_assignment(&binding.scope.task)? else {
            return Ok(());
        };
        let root = RootId::parse(self.config.workspace.as_str())?;
        if spec.mode != ChildMode::IsolatedWrite
            || !spec.effects.contains(&EffectClass::Execute)
            || !spec
                .paths
                .iter()
                .any(|path| path.root == root && path.path.is_empty() && path.write)
        {
            return Err("process preparation requires whole isolated-workspace write scope".into());
        }
        // The current native adapter advertises JobTree/environment controls,
        // but no WorkspaceFilesystem enforcement. A cwd and a copied tree do
        // not prevent an opaque process from writing an absolute parent path.
        Err("isolated child processes require qualified WorkspaceFilesystem enforcement".into())
    }
    pub(super) fn child_assignment_record(
        &self,
        task: &TaskId,
    ) -> Result<Option<(TaskGraph, ChildSpec)>> {
        if task == &self.config.root_task {
            return Ok(None);
        }
        let Some(row) = self.engine.store().state().records.get(&key(
            Collection::Projection,
            &graph_id(&self.config.root_task),
        )) else {
            // Existing manually bound task fixtures retain their P1 semantics;
            // the graph admission path always persists a graph before binding.
            return Ok(None);
        };
        let graph: TaskGraph = row.decode()?;
        graph.validate()?;
        if graph.scope.workspace != self.config.workspace
            || graph.scope.session != self.config.session
            || graph.scope.task != self.config.root_task
        {
            return Err("child graph belongs to a different owner scope".into());
        }
        let spec = graph
            .children
            .get(task)
            .ok_or("child has no graph assignment")?
            .clone();
        Ok(Some((graph, spec)))
    }
    pub(super) fn child_assignment(&self, task: &TaskId) -> Result<Option<(TaskGraph, ChildSpec)>> {
        let Some((graph, spec)) = self.child_assignment_record(task)? else {
            return Ok(None);
        };
        if graph.cleanup.contains_key(task) {
            return Err("child workspace is leased for cleanup; only cleanup reconciliation and retained history are available".into());
        }
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
        let parent: Task = self
            .engine
            .store()
            .state()
            .record(
                Collection::Task,
                spec.parent.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        let policy =
            vcp_engine::policy::current(self.engine.store().state(), &self.config.workspace)?;
        if spec.actor != self.config.actor
            || spec.authority != workspace.authority
            || spec.policy != policy.revision
            || spec.binding != workspace.binding.revision
            || spec.parent_steering != parent.steering
            || spec.deadline <= now()
        {
            return Err("child assignment requires current parent, authority and deadline".into());
        }
        if !vcp_engine::agents::current_scope(self.engine.store().state(), &parent, &spec, now())? {
            return Err("child grant ceiling is no longer current".into());
        }
        Ok(Some((graph, spec)))
    }
    pub(super) fn child_registration(
        &self,
        task: &TaskId,
        graph: &TaskGraph,
        spec: &ChildSpec,
    ) -> Result<WorkspaceRegistration> {
        let ready = graph
            .ready
            .get(task)
            .ok_or("child workspace has not been qualified")?;
        let id = spec
            .registration
            .as_ref()
            .ok_or("child requires a registered isolated workspace")?;
        let descriptor: ArtifactDescriptor = self
            .engine
            .store()
            .state()
            .record(Collection::Artifact, id.as_str(), &self.config.workspace)?
            .decode()?;
        if descriptor.state != CaptureState::Complete
            || Some(&descriptor.sha256) != spec.registration_digest.as_ref()
            || ready.registration_digest != spec.registration_digest
            || ready.snapshot_digest != spec.snapshot_digest
            || descriptor.length.get() > 64 * 1024
        {
            return Err("child workspace registration evidence changed".into());
        }
        let mut bytes = Vec::new();
        vcp_audit::history::History::read_artifact(
            self.engine.store(),
            &self.history_access(),
            id,
            &mut bytes,
        )?;
        let registration: WorkspaceRegistration = serde_json::from_slice(&bytes)?;
        if registration.owner != *task
            || registration.child.workspace != self.config.workspace
            || registration.child.binding != self.config.binding.revision
            || Some(&registration.child.root) != spec.isolated_root.as_ref()
            || !registration.absolute_path.is_absolute()
        {
            return Err("child workspace binding differs from assignment".into());
        }
        Ok(registration)
    }
    pub(in crate::foundation) fn task_root(&self, task: &TaskId) -> Result<Root> {
        let Some((graph, spec)) = self.child_assignment(task)? else {
            return self.tool_root();
        };
        self.registered_child_root(task, &graph, &spec)
    }
    fn registered_child_root(
        &self,
        task: &TaskId,
        graph: &TaskGraph,
        spec: &ChildSpec,
    ) -> Result<Root> {
        let registration = self.child_registration(task, graph, spec)?;
        let root = Root::open(registration.child, &registration.absolute_path)?;
        if root.hold(None, true)?.native_identity
            != graph
                .ready
                .get(task)
                .ok_or("child readiness missing")?
                .native_identity
        {
            return Err("child workspace moved or replaced; explicit recovery required".into());
        }
        Ok(root)
    }
    /// Read-only reconciliation follows immutable ownership, not an expired
    /// execution grant or superseded objective. It cannot authorize dispatch.
    pub(super) fn recovery_task_root(&self, task: &TaskId) -> Result<Root> {
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
        if workspace.trust != Trust::Trusted || workspace.binding != self.config.binding {
            return Err("current trusted binding is required for recovery reads".into());
        }
        self.tool_read_access(&RootId::parse(self.config.workspace.as_str())?, "vcp_patch")?;
        let Some((graph, spec)) = self.child_assignment_record(task)? else {
            return self.tool_root();
        };
        // Keep current source and projected child read denials authoritative.
        let root = self.registered_child_root(task, &graph, &spec)?;
        self.tool_read_access(&root.identity.root, "vcp_patch")?;
        Ok(root)
    }
    pub(in crate::foundation) fn child_tool_request(
        &self,
        binding: &ThreadBinding,
        request: &vcp_tools::Request,
    ) -> Result<()> {
        let Some((_, spec)) = self.child_assignment(&binding.scope.task)? else {
            return Ok(());
        };
        let root = RootId::parse(self.config.workspace.as_str())?;
        let check = |path: &str, write: bool| -> Result<()> {
            if !path.is_empty() {
                vcp_repository::path::relative(std::path::Path::new(path))?;
            }
            if write
                && path
                    .split('/')
                    .any(|part| part.eq_ignore_ascii_case(".vcp-child-owner"))
            {
                return Err("child ownership metadata is not editable".into());
            }
            let requested = ChildPath {
                root: root.clone(),
                path: path.into(),
                write,
            };
            if !spec.paths.iter().any(|p| p.covers(&requested))
                || (write && spec.mode == ChildMode::ReadOnly)
            {
                return Err("tool path exceeds child assignment".into());
            }
            Ok(())
        };
        match request {
            vcp_tools::Request::Read { path, .. } => check(path, false),
            // Search/list expose directory names or scan the snapshot. Require
            // the whole snapshot read ceiling before observing any entries.
            vcp_tools::Request::List { .. } | vcp_tools::Request::Search { .. } => check("", false),
            vcp_tools::Request::Patch { patch } => {
                if patch.len() > 256 * 1024 {
                    return Err("child patch exceeds input ceiling".into());
                }
                for hunk in codex_apply_patch::parse_patch(patch)?.hunks {
                    match hunk {
                        codex_apply_patch::Hunk::AddFile { path, .. }
                        | codex_apply_patch::Hunk::DeleteFile { path } => {
                            check(path.to_str().ok_or("non-Unicode child path")?, true)?
                        }
                        codex_apply_patch::Hunk::UpdateFile {
                            path, move_path, ..
                        } => {
                            check(path.to_str().ok_or("non-Unicode child path")?, true)?;
                            if let Some(path) = move_path {
                                check(path.to_str().ok_or("non-Unicode child path")?, true)?;
                            }
                        }
                    }
                }
                Ok(())
            }
        }
    }
    pub(super) fn child_operation_scope(
        &self,
        binding: &ThreadBinding,
        operation: &Operation,
    ) -> Result<()> {
        let Some((_, spec)) = self.child_assignment(&binding.scope.task)? else {
            return Ok(());
        };
        if matches!(operation.invocation, Invocation::Process { .. }) {
            self.child_process_scope(binding)?;
        }
        if !operation.effects.is_subset(&spec.effects) {
            return Err("operation effects exceed child assignment".into());
        }
        let source = RootId::parse(self.config.workspace.as_str())?;
        let isolated = spec
            .isolated_root
            .as_ref()
            .ok_or("child isolated root missing")?;
        for resource in &operation.resources {
            if &resource.root != isolated {
                if !resource.write
                    && matches!(operation.invocation, Invocation::Process { .. })
                    && self.process_profiles.values().any(|profile| {
                        profile.executable_root_id().ok().as_ref() == Some(&resource.root)
                    })
                {
                    continue;
                }
                return Err("child operation accesses an unrelated root".into());
            }
            let requested = ChildPath {
                root: source.clone(),
                path: resource.path.clone(),
                write: resource.write,
            };
            if !spec.paths.iter().any(|p| p.covers(&requested)) {
                return Err("operation resource exceeds child assignment".into());
            }
        }
        // A process cannot be constrained by a declared filename write set.
        // Until a narrower OS capability exists, only whole-copy assignments
        // may execute; read-only helpers never acquire process capability.
        if !matches!(operation.invocation, Invocation::Local)
            && (!spec
                .paths
                .iter()
                .any(|p| p.root == source && p.path.is_empty() && p.write)
                || spec.mode != ChildMode::IsolatedWrite)
        {
            return Err("child process requires whole isolated-workspace write scope".into());
        }
        Ok(())
    }
    pub(super) fn child_policy(
        &self,
        binding: &ThreadBinding,
    ) -> Result<Option<(Policy, Vec<Grant>, Vec<Denial>)>> {
        let Some((_, spec)) = self.child_assignment(&binding.scope.task)? else {
            return Ok(None);
        };
        let source = RootId::parse(self.config.workspace.as_str())?;
        let isolated = spec.isolated_root.as_ref().ok_or("child root missing")?;
        let mut policy =
            vcp_engine::policy::current(self.engine.store().state(), &self.config.workspace)?;
        if policy.workspace_roots.contains(&source) {
            policy.workspace_roots.insert(isolated.clone());
        }
        let project_denials = |rules: &mut Vec<Denial>| {
            for rule in rules {
                if rule.roots.contains(&source) {
                    rule.roots.insert(isolated.clone());
                }
            }
        };
        project_denials(&mut policy.denials);
        let mut host_denials = self.config.host_tool_denials.clone();
        project_denials(&mut host_denials);
        let mut grants = Vec::new();
        let child_root = self.task_root(&binding.scope.task)?;
        let source_root = self.tool_root()?;
        let parent: Task = self
            .engine
            .store()
            .state()
            .record(
                Collection::Task,
                spec.parent.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        for mut grant in
            vcp_engine::policy::grants(self.engine.store().state(), &self.config.workspace)?
        {
            if spec.grants.get(&grant.id) != Some(&grant.revision) {
                continue;
            }
            if !vcp_engine::agents::inherited_grant(
                self.engine.store().state(),
                &parent,
                &grant,
                now(),
            )? {
                continue;
            }
            let GrantTarget::Configured {
                roots, invocation, ..
            } = &mut grant.target
            else {
                continue;
            };
            if !roots.contains(&source) {
                continue;
            }
            roots.insert(isolated.clone());
            if let Invocation::Process { directory, .. } = invocation {
                let relative = std::path::Path::new(directory)
                    .strip_prefix(source_root.path())
                    .map_err(|_| "inherited process directory is outside source workspace")?;
                *directory = child_root
                    .path()
                    .join(relative)
                    .to_str()
                    .ok_or("non-Unicode child process directory")?
                    .into();
            }
            if let GrantScope::Task { scope } = &mut grant.scope {
                scope.task = binding.scope.task.clone();
            }
            grants.push(grant);
        }
        Ok(Some((policy, grants, host_denials)))
    }
}
