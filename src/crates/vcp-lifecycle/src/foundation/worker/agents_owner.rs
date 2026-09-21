// SPDX-License-Identifier: Apache-2.0
//! Trusted owner preparation around canonical graph admission. No provider loop.
use super::super::*;
use vcp_domain::{
    agents::WorkspaceReady,
    task::{Task, TaskState},
};
use vcp_repository::{
    dirty_snapshot::{CapturePolicy, WorkspaceSnapshot},
    worktree::{RegisteredRoot, Snapshotter, WorkspaceRegistration},
    Root, RootIdentity,
};
use vcp_store::contract::Collection;

/// Exact immutable inputs for a subsequent CreateChild command. Constructing
/// these does not allocate a child, dispatch it, or grant authority.
pub struct ChildWorkspaceInputs {
    pub snapshot: ArtifactId,
    pub snapshot_digest: String,
    pub registration: ArtifactId,
    pub registration_digest: String,
    pub isolated_root: RootId,
}
/// One-use launch ticket. Pending -> Running is durable before retained startup;
/// a lost ticket requires reconciliation rather than another automatic launch.
pub struct ChildStart {
    parent: ThreadId,
    generation: u64,
    binding: ThreadBinding,
    root: Root,
    controller: ControllerId,
    owner: OwnerEpoch,
    cleanup: LaunchCleanup,
}
/// A retained startup may fail or its owner future may be cancelled after the
/// durable Running transition. Only the unchanged, still-unbound task is paused.
struct LaunchCleanup {
    armed: bool,
    worker: worker::Worker,
    bindings: Arc<Mutex<HashMap<ThreadId, ThreadBinding>>>,
    scope: Scope,
    revision: Revision,
    controller: ControllerId,
    owner: OwnerEpoch,
}
impl Drop for LaunchCleanup {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let scope = self.scope.clone();
        let revision = self.revision;
        let controller = self.controller.clone();
        let owner = self.owner;
        let bindings = self.bindings.clone();
        // Owner loss has its own recovery pause. Failure here never releases
        // allocation, erases evidence, or changes an attached/live child.
        let _ = self.worker.run_cleanup(move |context| {
            if !context.owner_alive || context.engine.controller() != &controller || context.engine.owner_epoch() != owner
                || bindings.lock().map_err(|_| "binding lock poisoned")?.values().any(|binding| binding.scope == scope) {
                return Ok(());
            }
            let task: Task = context.engine.store().state().record(Collection::Task, scope.task.as_str(), &scope.workspace)?.decode()?;
            if task.scope != scope || task.revision != revision || task.state != vcp_domain::task::TaskState::Running { return Ok(()); }
            context.command(Command::Transition { next: vcp_domain::task::TaskState::Paused, reason: "retained child startup abandoned before owner attachment; explicit recovery required".into(), verification: None }, Some(scope.task), revision)?;
            Ok(())
        });
    }
}
impl ChildStart {
    pub fn workspace(&self) -> &Path {
        self.root.path()
    }
}
impl CanonicalHost {
    /// Cancel owner preparation as soon as its admission generation changes.
    pub(super) async fn child_preparation<T>(
        &self,
        parent: ThreadId,
        generation: u64,
        work: impl std::future::Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        tokio::pin!(work);
        loop {
            let changed = self.runtime.0.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            scheduler::check_generation(&self.runtime, parent, generation)?;
            tokio::select! {
                biased;
                _ = changed => {},
                result = &mut work => {
                    scheduler::check_generation(&self.runtime, parent, generation)?;
                    return result;
                }
            }
        }
    }
    fn pending_child_revision(&self, scope: &Scope, revision: Revision) -> Result<(), String> {
        let scope = scope.clone();
        self.worker.run(move |context| {
            let task: Task = context.engine.store().state()
                .record(Collection::Task, scope.task.as_str(), &scope.workspace)?.decode()?;
            if task.scope != scope || task.state != TaskState::Pending || task.revision != revision {
                return Err("child changed during materialization; partial workspace retained for reconciliation".into());
            }
            Ok(())
        })
    }
    /// Explicit stops notify immediately. The bounded timer also observes
    /// canonical changes made without a retained runtime notification.
    async fn child_materialization<T>(
        &self,
        parent: ThreadId,
        generation: u64,
        scope: &Scope,
        revision: Revision,
        work: impl std::future::Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        tokio::pin!(work);
        loop {
            let changed = self.runtime.0.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            scheduler::check_generation(&self.runtime, parent, generation)?;
            self.pending_child_revision(scope, revision)?;
            tokio::select! {
                biased;
                _ = changed => {},
                _ = tokio::time::sleep(std::time::Duration::from_millis(25)) => {},
                result = &mut work => {
                    scheduler::check_generation(&self.runtime, parent, generation)?;
                    self.pending_child_revision(scope, revision)?;
                    return result;
                }
            }
        }
    }
    pub async fn prepare_child_start(
        &self,
        parent: ThreadId,
        child: TaskId,
        snapshotter: &Snapshotter,
    ) -> Result<ChildStart, String> {
        let generation = scheduler::generation(&self.runtime, parent)?;
        let parent_binding = self.binding(parent)?;
        let checked = parent_binding.clone();
        let selected = child.clone();
        let (source, metadata_owner, root, snapshot, registration, revision) =
            self.worker.run(move |context| {
                context.can_start(&checked)?;
                let (graph, spec) = context
                    .child_assignment(&selected)?
                    .ok_or("child assignment missing")?;
                if spec.parent != checked.scope.task {
                    return Err("child parent differs".into());
                }
                let descriptor: ArtifactDescriptor = context
                    .engine
                    .store()
                    .state()
                    .record(
                        Collection::Artifact,
                        spec.snapshot.as_str(),
                        &checked.scope.workspace,
                    )?
                    .decode()?;
                if descriptor.state != CaptureState::Complete
                    || descriptor.sha256 != spec.snapshot_digest
                    || descriptor.length.get() > 128 * 1024 * 1024
                {
                    return Err("child snapshot identity or size differs".into());
                }
                let mut bytes = Vec::new();
                vcp_audit::history::History::read_artifact(
                    context.engine.store(),
                    &context.history_access(),
                    &spec.snapshot,
                    &mut bytes,
                )?;
                let snapshot: WorkspaceSnapshot = serde_json::from_slice(&bytes)?;
                let registration = context.child_registration(&selected, &graph, &spec)?;
                Ok((
                    context.task_root(&checked.scope.task)?,
                    context.tool_root()?,
                    context.task_root(&selected)?,
                    snapshot,
                    registration,
                    graph.revision,
                ))
            })?;
        let _ownership = self
            .scheduler
            .snapshot(child.clone(), root.identity.root.clone())?;
        self.child_preparation(parent, generation, async {
            snapshotter
                .verify_with_metadata(&source, &metadata_owner, &root, &snapshot, &registration)
                .await
                .map_err(|e| e.to_string())
        })
        .await?;
        let (binding, root, controller, owner, revision) = self.worker.run(move |context| {
            context.can_start(&parent_binding)?;
            let (graph, _) = context
                .child_assignment(&child)?
                .ok_or("child assignment missing")?;
            if graph.revision != revision {
                return Err("child graph changed during launch verification".into());
            }
            let task: Task = context
                .engine
                .store()
                .state()
                .record(
                    Collection::Task,
                    child.as_str(),
                    &parent_binding.scope.workspace,
                )?
                .decode()?;
            if task.parent.as_ref() != Some(&parent_binding.scope.task)
                || task.state != vcp_domain::task::TaskState::Pending
            {
                return Err("child launch requires the assigned pending child".into());
            }
            if !vcp_engine::agents::eligibility(
                context.engine.store().state(),
                &task,
                worker::now(),
                true,
            )?
            .is_empty()
            {
                return Err("child graph is not eligible for launch".into());
            }
            let root = context.task_root(&child)?;
            context.command(
                Command::Transition {
                    next: vcp_domain::task::TaskState::Running,
                    reason: "assigned child admitted for retained startup".into(),
                    verification: None,
                },
                Some(child),
                task.revision,
            )?;
            Ok((
                ThreadBinding {
                    scope: task.scope,
                    agent: AgentId::new(),
                    role: RequestRole::Child,
                },
                root,
                context.engine.controller().clone(),
                context.engine.owner_epoch(),
                task.revision.next()?,
            ))
        })?;
        let cleanup = LaunchCleanup {
            armed: true,
            worker: self.worker.clone(),
            bindings: self.bindings.clone(),
            scope: binding.scope.clone(),
            revision,
            controller: controller.clone(),
            owner,
        };
        Ok(ChildStart {
            parent,
            generation,
            binding,
            root,
            controller,
            owner,
            cleanup,
        })
    }
    pub fn attach_prepared_child(
        &self,
        mut ticket: ChildStart,
        thread: Arc<codex_core::CodexThread>,
    ) -> Result<ThreadId, String> {
        scheduler::check_generation(&self.runtime, ticket.parent, ticket.generation)?;
        let checked = ticket.binding.clone();
        let expected = ticket.root.clone();
        let controller = ticket.controller.clone();
        let owner = ticket.owner;
        self.worker.run(move |context| {
            if context.engine.controller() != &controller || context.engine.owner_epoch() != owner {
                return Err("child startup ticket belongs to a different canonical owner".into());
            }
            context.can_start(&checked)?;
            let root = context.task_root(&checked.scope.task)?;
            if root.identity != expected.identity
                || root.path() != expected.path()
                || root.hold(None, true)?.native_identity
                    != expected.hold(None, true)?.native_identity
            {
                return Err("child workspace changed during retained startup".into());
            }
            Ok(())
        })?;
        if thread
            .session_configured()
            .cwd
            .as_path()
            .canonicalize()
            .map_err(|e| e.to_string())?
            != ticket.root.path()
        {
            return Err("retained child started in a different workspace".into());
        }
        let mut bindings = self.bindings.lock().map_err(|_| "binding lock poisoned")?;
        if bindings
            .values()
            .any(|bound| bound.scope == ticket.binding.scope)
        {
            return Err("child already has a retained owner".into());
        }
        let revision = self
            .runtime
            .inspect(ticket.parent)
            .map_err(|e| format!("child parent: {e:?}"))?
            .revision;
        scheduler::check_generation(&self.runtime, ticket.parent, ticket.generation)?;
        let id = self
            .runtime
            .attach_child(ticket.parent, &revision, thread)
            .map_err(|e| format!("child attachment: {e:?}"))?;
        bindings.insert(id, ticket.binding);
        ticket.cleanup.armed = false;
        Ok(id)
    }
    pub async fn capture_child_workspace(
        &self,
        parent: ThreadId,
        child: TaskId,
        snapshotter: &Snapshotter,
        policy: &CapturePolicy,
        disposable_parent: &Root,
    ) -> Result<ChildWorkspaceInputs, String> {
        let generation = scheduler::generation(&self.runtime, parent)?;
        let binding = self.binding(parent)?;
        let checked = binding.clone();
        let (source, metadata_owner, parent_registration) = self.worker.run(move |context| {
            context.can_start(&checked)?;
            let root = context.task_root(&checked.scope.task)?;
            context.tool_read_access(
                &RootId::parse(context.config.workspace.as_str())?,
                "vcp_read",
            )?;
            let registration = context
                .child_assignment(&checked.scope.task)?
                .map(|(graph, spec)| context.child_registration(&checked.scope.task, &graph, &spec))
                .transpose()?;
            Ok((root, context.tool_root()?, registration))
        })?;
        if disposable_parent.identity.workspace != source.identity.workspace
            || disposable_parent.path().starts_with(source.path())
        {
            return Err(
                "disposable workspace directory must be independently registered outside source"
                    .into(),
            );
        }
        let _ownership = self
            .scheduler
            .snapshot(binding.scope.task.clone(), source.identity.root.clone())?;
        let snapshot = self
            .child_preparation(parent, generation, async {
                if let Some(registration) = &parent_registration {
                    snapshotter
                        .capture_registered(&source, &metadata_owner, registration, policy)
                        .await
                } else {
                    snapshotter.capture(&source, policy).await
                }
                .map_err(|e| e.to_string())
            })
            .await?;
        let registration = WorkspaceRegistration {
            owner: child.clone(),
            source: source.identity.clone(),
            child: RootIdentity {
                root: RootId::new(),
                worktree: format!("child-{child}"),
                ..source.identity.clone()
            },
            absolute_path: disposable_parent.path().join(child.as_str()),
            snapshot: snapshot.fingerprint.clone(),
            git_metadata_owner: snapshot.base_commit.as_ref().map(|_| RegisteredRoot {
                identity: metadata_owner.identity.clone(),
                absolute_path: metadata_owner.path().to_owned(),
            }),
        };
        if scheduler::generation(&self.runtime, parent)? != generation {
            return Err("parent changed during child snapshot".into());
        }
        let isolated_root = registration.child.root.clone();
        self.worker.run(move |context| {
            context.can_start(&binding)?;
            let snapshot = context.capture(
                &binding.scope,
                Channel::Evidence,
                &vcp_protocol::canonical_bytes(&snapshot)?,
                "child-workspace-snapshot/1",
            )?;
            let registration = context.capture(
                &binding.scope,
                Channel::Evidence,
                &vcp_protocol::canonical_bytes(&registration)?,
                "child-workspace-registration/1",
            )?;
            Ok(ChildWorkspaceInputs {
                snapshot: snapshot.spec.id,
                snapshot_digest: snapshot.sha256,
                registration: registration.spec.id,
                registration_digest: registration.sha256,
                isolated_root,
            })
        })
    }
    /// Graph creation and budget allocation must already be durable. A failure
    /// leaves the registered disposable path available for explicit recovery.
    pub async fn materialize_child_workspace(
        &self,
        parent: ThreadId,
        child: TaskId,
        snapshotter: &Snapshotter,
        disposable_parent: &Root,
    ) -> Result<(), String> {
        let generation = scheduler::generation(&self.runtime, parent)?;
        let binding = self.binding(parent)?;
        let checked = binding.clone();
        let selected = child.clone();
        let (
            source,
            metadata_owner,
            snapshot,
            registration,
            graph_revision,
            snapshot_digest,
            registration_digest,
            child_revision,
        ) = self.worker.run(move |context| {
            context.can_start(&checked)?;
            let target: Task = context
                .engine
                .store()
                .state()
                .record(
                    Collection::Task,
                    selected.as_str(),
                    &checked.scope.workspace,
                )?
                .decode()?;
            if target.state != TaskState::Pending {
                return Err("only a pending child can materialize its workspace".into());
            }
            let graph = vcp_engine::agents::graph(
                context.engine.store().state(),
                &checked.scope,
                &context.config.root_task,
            )?
            .ok_or("child graph missing")?;
            let spec = graph
                .children
                .get(&selected)
                .ok_or("child assignment missing")?;
            if spec.parent != checked.scope.task {
                return Err("child parent differs".into());
            }
            let parent: Task = context
                .engine
                .store()
                .state()
                .record(
                    Collection::Task,
                    checked.scope.task.as_str(),
                    &checked.scope.workspace,
                )?
                .decode()?;
            if !vcp_engine::agents::current_scope(
                context.engine.store().state(),
                &parent,
                spec,
                worker::now(),
            )? {
                return Err("child assignment is stale".into());
            }
            let read = |id: &ArtifactId, hash: &str, limit: u64| -> worker::Result<Vec<u8>> {
                let descriptor: ArtifactDescriptor = context
                    .engine
                    .store()
                    .state()
                    .record(Collection::Artifact, id.as_str(), &checked.scope.workspace)?
                    .decode()?;
                if descriptor.state != CaptureState::Complete
                    || descriptor.sha256 != hash
                    || descriptor.length.get() > limit
                {
                    return Err("child input identity or size differs".into());
                }
                let mut bytes = Vec::new();
                vcp_audit::history::History::read_artifact(
                    context.engine.store(),
                    &context.history_access(),
                    id,
                    &mut bytes,
                )?;
                Ok(bytes)
            };
            let snapshot: WorkspaceSnapshot = serde_json::from_slice(&read(
                &spec.snapshot,
                &spec.snapshot_digest,
                128 * 1024 * 1024,
            )?)?;
            let id = spec
                .registration
                .as_ref()
                .ok_or("child registration missing")?;
            let digest = spec
                .registration_digest
                .as_ref()
                .ok_or("child registration digest missing")?;
            let registration: WorkspaceRegistration =
                serde_json::from_slice(&read(id, digest, 64 * 1024)?)?;
            if registration.owner != selected
                || Some(&registration.child.root) != spec.isolated_root.as_ref()
                || registration.snapshot != snapshot.fingerprint
            {
                return Err("child workspace input mismatch".into());
            }
            Ok((
                context.task_root(&checked.scope.task)?,
                context.tool_root()?,
                snapshot,
                registration,
                graph.revision,
                spec.snapshot_digest.clone(),
                spec.registration_digest.clone(),
                target.revision,
            ))
        })?;
        let _ownership = self
            .scheduler
            .snapshot(binding.scope.task.clone(), source.identity.root.clone())?;
        let materialized = self
            .child_materialization(
                parent,
                generation,
                &Scope {
                    task: child.clone(),
                    ..binding.scope.clone()
                },
                child_revision,
                async {
                    snapshotter
                        .materialize_with_metadata(
                            &source,
                            &metadata_owner,
                            disposable_parent,
                            &snapshot,
                            &registration,
                        )
                        .await
                        .map_err(|e| e.to_string())
                },
            )
            .await?;
        let native_identity = materialized
            .root
            .hold(None, true)
            .map_err(|e| e.to_string())?
            .native_identity;
        if scheduler::generation(&self.runtime, parent)? != generation {
            return Err("parent changed during child materialization; workspace retained for reconciliation".into());
        }
        self.worker.run(move |context| {
            context.can_start(&binding)?;
            let target: Task = context
                .engine
                .store()
                .state()
                .record(Collection::Task, child.as_str(), &binding.scope.workspace)?
                .decode()?;
            if target.state != TaskState::Pending || target.revision != child_revision {
                return Err("child changed before workspace registration".into());
            }
            let policy = vcp_engine::policy::current(
                context.engine.store().state(),
                &context.config.workspace,
            )?
            .revision;
            let facts = vcp_engine::HostFacts {
                now: worker::now(),
                policy,
                may_execute: true,
                resume: None,
            };
            context
                .runtime
                .block_on(context.engine.record_child_workspace(
                    &binding.scope,
                    vcp_engine::agents::NativeWorkspaceEvidence {
                        child,
                        expected_graph: graph_revision,
                        ready: WorkspaceReady {
                            snapshot_digest,
                            registration_digest,
                            native_identity,
                        },
                    },
                    &context.access,
                    &facts,
                ))?;
            Ok(())
        })
    }
}
