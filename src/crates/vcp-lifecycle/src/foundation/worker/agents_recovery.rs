// SPDX-License-Identifier: Apache-2.0
//! Reattach registered child owners while preserving their current working state.
use super::super::*;
use vcp_domain::{
    agents::ChildSpec,
    task::{Task, TaskState},
};
use vcp_repository::{
    dirty_snapshot::{CapturePolicy, WorkspaceSnapshot},
    worktree::Snapshotter,
    Root,
};
use vcp_store::contract::Collection;

pub struct ChildRecovery {
    pub evidence: ArtifactId,
    pub blocked: Option<String>,
    pub ticket: Option<ChildRecoveryTicket>,
}
pub struct ChildRecoveryTicket {
    parent: ThreadId,
    generation: u64,
    binding: ThreadBinding,
    task_revision: Revision,
    root: Root,
    native_identity: String,
    startup_authorized: bool,
    controller: ControllerId,
    owner: OwnerEpoch,
}
impl ChildRecoveryTicket {
    pub fn workspace(&self) -> &Path {
        self.root.path()
    }
}

impl worker::Context {
    fn recovery_child_snapshot(&self, spec: &ChildSpec) -> worker::Result<WorkspaceSnapshot> {
        let descriptor: ArtifactDescriptor = self
            .engine
            .store()
            .state()
            .record(
                Collection::Artifact,
                spec.snapshot.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        if descriptor.state != CaptureState::Complete
            || descriptor.sha256 != spec.snapshot_digest
            || descriptor.length.get() > 128 * 1024 * 1024
        {
            return Err("registered child base evidence differs".into());
        }
        let mut bytes = Vec::new();
        vcp_audit::history::History::read_artifact(
            self.engine.store(),
            &self.history_access(),
            &spec.snapshot,
            &mut bytes,
        )?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}

impl CanonicalHost {
    fn recovery_generation(&self, parent: ThreadId) -> Result<u64, String> {
        let state = self
            .runtime
            .0
            .state
            .lock()
            .map_err(|_| "lifecycle poisoned")?;
        if !state.attached || state.sealing {
            return Err("recovery owner unavailable".into());
        }
        Ok(state
            .entries
            .get(&parent)
            .ok_or("recovery parent is not attached")?
            .admission_generation)
    }
    /// Observe and reconcile while held. Native registration, not old snapshot
    /// content, identifies the workspace; legitimate child edits are preserved.
    pub async fn prepare_child_recovery(
        &self,
        parent: ThreadId,
        child: TaskId,
        snapshotter: &Snapshotter,
    ) -> Result<ChildRecovery, String> {
        let parent_binding = self.binding(parent)?;
        let selected = child.clone();
        let checked = parent_binding.clone();
        let scope = self.worker.run(move |context| {
            let (_, spec) = context
                .child_assignment_record(&selected)?
                .ok_or("child assignment missing")?;
            if spec.parent != checked.scope.task {
                return Err("recovery child belongs to another parent".into());
            }
            let task: Task = context
                .engine
                .store()
                .state()
                .record(
                    Collection::Task,
                    selected.as_str(),
                    &checked.scope.workspace,
                )?
                .decode()?;
            Ok(task.scope)
        })?;
        let result: Result<ChildRecoveryTicket, String> = async {
            {
                let state = self
                    .runtime
                    .0
                    .state
                    .lock()
                    .map_err(|_| "lifecycle poisoned")?;
                if state.root.is_none_or(|root| !state.held(root)) {
                    return Err("pause the retained root before child recovery".into());
                }
            }
            self.reconcile_effects()?;
            let generation = self.recovery_generation(parent)?;
            let selected = child.clone();
            let (root, metadata_owner, registration, policy, task) =
                self.worker.run(move |context| {
                    let (graph, spec) = context
                        .child_assignment(&selected)?
                        .ok_or("child assignment missing")?;
                    let task: Task = context
                        .engine
                        .store()
                        .state()
                        .record(
                            Collection::Task,
                            selected.as_str(),
                            &context.config.workspace,
                        )?
                        .decode()?;
                    if task.state != TaskState::Paused {
                        return Err(
                            "recovery attachment requires an explicitly paused child".into()
                        );
                    }
                    context.integration_child_quiescent(&selected)?;
                    let root = context.recovery_task_root(&selected)?;
                    let registration = context.child_registration(&selected, &graph, &spec)?;
                    let base = context.recovery_child_snapshot(&spec)?;
                    let mut names: std::collections::BTreeSet<_> =
                        base.files.iter().map(|f| f.path.clone()).collect();
                    let scan = root.discover(&vcp_repository::discovery::Limits::default())?;
                    if !scan.complete {
                        return Err("recovery workspace inventory is incomplete".into());
                    }
                    names.extend(
                        scan.sources
                            .iter()
                            .map(|source| source.version.path.clone()),
                    );
                    names.remove(".vcp-child-owner");
                    Ok((
                        root,
                        context.tool_root()?,
                        registration,
                        CapturePolicy {
                            untracked: names,
                            ..Default::default()
                        },
                        task,
                    ))
                })?;
            let _claim = self
                .scheduler
                .snapshot(child.clone(), root.identity.root.clone())?;
            let observed = {
                let observation =
                    snapshotter.capture_registered(&root, &metadata_owner, &registration, &policy);
                tokio::pin!(observation);
                loop {
                    let changed = self.runtime.0.changed.notified();
                    tokio::pin!(changed);
                    changed.as_mut().enable();
                    if self.recovery_generation(parent)? != generation {
                        return Err("recovery owner changed during observation".into());
                    }
                    tokio::select! {
                        biased;
                        _ = changed => {},
                        result = &mut observation => break result.map_err(|e| e.to_string())?,
                    }
                }
            };
            if self.recovery_generation(parent)? != generation {
                return Err("recovery owner changed during observation".into());
            }
            let native_identity = root
                .hold(None, true)
                .map_err(|e| e.to_string())?
                .native_identity;
            let observed_scope = scope.clone();
            let selected = child.clone();
            let (controller, owner) = self.worker.run(move |context| {
                context
                    .child_assignment(&selected)?
                    .ok_or("child assignment missing")?;
                context.integration_child_quiescent(&selected)?;
                context.capture(
                    &observed_scope,
                    Channel::Evidence,
                    &vcp_protocol::canonical_bytes(&observed)?,
                    "child-recovery-observation/1",
                )?;
                Ok((
                    context.engine.controller().clone(),
                    context.engine.owner_epoch(),
                ))
            })?;
            Ok(ChildRecoveryTicket {
                parent,
                generation,
                binding: ThreadBinding {
                    scope: task.scope,
                    agent: AgentId::new(),
                    role: RequestRole::Child,
                },
                task_revision: task.revision,
                root,
                native_identity,
                startup_authorized: false,
                controller,
                owner,
            })
        }
        .await;
        let blocked = result.as_ref().err().cloned();
        let diagnostic = serde_json::json!({"child":child,"blocked":blocked,"status":if blocked.is_some(){"blocked"}else{"registered_workspace_observed; explicit attachment and resume required"}});
        let evidence = self.worker.run(move |context| {
            Ok(context
                .capture(
                    &scope,
                    Channel::Evidence,
                    &vcp_protocol::canonical_bytes(&diagnostic)?,
                    "child-recovery-status/1",
                )?
                .spec
                .id)
        })?;
        Ok(ChildRecovery {
            evidence,
            blocked,
            ticket: result.ok(),
        })
    }
    /// Authorize only retained startup in the exact registered directory. Turn
    /// admission remains closed until the child is attached held and resumed.
    pub fn authorize_child_recovery_startup(
        &self,
        ticket: &mut ChildRecoveryTicket,
    ) -> Result<(), String> {
        if ticket.startup_authorized {
            return Err("recovery ticket startup was already authorized".into());
        }
        self.validate_child_recovery_ticket(ticket)?;
        let mut state = self
            .runtime
            .0
            .state
            .lock()
            .map_err(|_| "lifecycle poisoned")?;
        if !state.attached
            || state.sealing
            || state
                .entries
                .get(&ticket.parent)
                .is_none_or(|entry| entry.admission_generation != ticket.generation)
        {
            return Err("recovery startup owner changed".into());
        }
        let grant = (ticket.root.path().to_path_buf(), None);
        if state.startups.contains(&grant) {
            return Err("recovery startup already authorized".into());
        }
        state.startups.push(grant);
        ticket.startup_authorized = true;
        Ok(())
    }
    fn validate_child_recovery_ticket(&self, ticket: &ChildRecoveryTicket) -> Result<(), String> {
        if self.recovery_generation(ticket.parent)? != ticket.generation {
            return Err("recovery ticket owner changed".into());
        }
        let binding = ticket.binding.clone();
        let expected = ticket.task_revision;
        let identity = ticket.native_identity.clone();
        let root = ticket.root.clone();
        let controller = ticket.controller.clone();
        let owner = ticket.owner;
        self.worker.run(move |context| {
            if context.engine.controller() != &controller || context.engine.owner_epoch() != owner {
                return Err("recovery ticket belongs to a different canonical owner".into());
            }
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
            if task.state != TaskState::Paused || task.revision != expected {
                return Err("recovery child state changed".into());
            }
            context
                .child_assignment(&binding.scope.task)?
                .ok_or("child assignment missing")?;
            context.integration_child_quiescent(&binding.scope.task)?;
            let current = context.recovery_task_root(&binding.scope.task)?;
            if current.identity != root.identity
                || current.path() != root.path()
                || current.hold(None, true)?.native_identity != identity
            {
                return Err("registered child workspace moved or changed".into());
            }
            Ok(())
        })
    }
    pub async fn attach_recovered_child(
        &self,
        ticket: ChildRecoveryTicket,
        thread: Arc<codex_core::CodexThread>,
    ) -> Result<ThreadId, String> {
        if !ticket.startup_authorized {
            return Err("recovery startup was not authorized by this ticket".into());
        }
        self.validate_child_recovery_ticket(&ticket)?;
        if thread
            .session_configured()
            .cwd
            .as_path()
            .canonicalize()
            .map_err(|e| e.to_string())?
            != ticket.root.path()
        {
            return Err("recovered retained child has a different workspace".into());
        }
        let id = thread.session_configured().thread_id;
        {
            let mut bindings = self.bindings.lock().map_err(|_| "binding lock poisoned")?;
            if bindings
                .values()
                .any(|binding| binding.scope == ticket.binding.scope)
                || bindings.contains_key(&id)
            {
                return Err("child already has a retained owner".into());
            }
            let mut state = self
                .runtime
                .0
                .state
                .lock()
                .map_err(|_| "lifecycle poisoned")?;
            if !state.attached
                || state.sealing
                || state
                    .entries
                    .get(&ticket.parent)
                    .is_none_or(|entry| entry.admission_generation != ticket.generation)
                || state.entries.contains_key(&id)
            {
                return Err("recovery attachment owner changed".into());
            }
            state
                .advance()
                .map_err(|e| format!("recovery attachment: {e:?}"))?;
            state.entries.insert(
                id,
                crate::Entry {
                    thread: Arc::downgrade(&thread),
                    parent: Some(ticket.parent),
                    held: true,
                    interrupted: false,
                    interruption_error: None,
                    starts: 0,
                    admission_generation: 0,
                },
            );
            state
                .checkpoint()
                .map_err(|e| format!("recovery checkpoint: {e:?}"))?;
            bindings.insert(id, ticket.binding);
        }
        // No model work can enter while held. Drain retained startup tasks so
        // explicit resume has the same interruption proof as ordinary pauses.
        crate::HoldWaiter(self.runtime.interrupt_owned(vec![id], false))
            .wait()
            .await
            .map_err(|e| format!("recovery interruption: {e:?}"))?;
        Ok(id)
    }
}
