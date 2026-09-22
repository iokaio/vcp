// SPDX-License-Identifier: Apache-2.0
//! Deliberate cleanup under an irreversible canonical lease and retained results.
use super::super::*;
use vcp_domain::{
    agents::{ChildCleanup, ChildSpec, TaskGraph},
    task::Task,
};
use vcp_protocol::canonical_bytes;
use vcp_repository::{
    cleanup::{CleanupIntent, CleanupReceipt},
    dirty_snapshot::WorkspaceSnapshot,
    merge,
    worktree::Snapshotter,
    Root, RootIdentity,
};
use vcp_store::contract::Collection;

pub struct ChildCleanupPreview {
    parent: ThreadId,
    child: TaskId,
    generation: u64,
    graph_revision: Revision,
    task_revision: Revision,
    intent: CleanupIntent,
    result: WorkspaceSnapshot,
    rejected_edits: bool,
}
impl ChildCleanupPreview {
    pub fn summary(&self) -> serde_json::Value {
        serde_json::json!({"child":self.child,"path":self.intent.absolute_path,
            "entries":self.intent.entries.len(),"retained_result_fingerprint":self.result.fingerprint,
            "rejected_edits":self.rejected_edits,"status":"preview; no deletion or lease published",
            "git_metadata":"retained; no Git pruning"})
    }
}
impl worker::Context {
    fn cleanup_eligible(
        &self,
        binding: &ThreadBinding,
        child: &TaskId,
    ) -> worker::Result<(TaskGraph, ChildSpec, Task)> {
        self.can_start(binding)?;
        let (graph, spec) = self
            .child_assignment_record(child)?
            .ok_or("child assignment missing")?;
        self.tool_identity(binding, "vcp_cleanup")?;
        let policy =
            vcp_engine::policy::current(self.engine.store().state(), &binding.scope.workspace)?;
        let source_root = RootId::parse(binding.scope.workspace.as_str())?;
        let local_edit = policy.mode == vcp_domain::policy::Autonomy::Workspace
            || (policy.mode == vcp_domain::policy::Autonomy::Autonomous
                && policy
                    .automatic_effects
                    .contains(&vcp_domain::policy::EffectClass::Read)
                && policy
                    .automatic_effects
                    .contains(&vcp_domain::policy::EffectClass::Write));
        if !policy.workspace_roots.contains(&source_root)
            || !local_edit
            || self
                .config
                .host_tool_denials
                .iter()
                .chain(&policy.denials)
                .any(|rule| {
                    (rule.effects.is_empty()
                        || rule
                            .effects
                            .contains(&vcp_domain::policy::EffectClass::Write))
                        && (rule.roots.is_empty()
                            || rule.roots.contains(&source_root)
                            || spec
                                .isolated_root
                                .as_ref()
                                .is_some_and(|r| rule.roots.contains(r)))
                        && rule
                            .tool
                            .as_deref()
                            .is_none_or(|name| matches!(name, "vcp_cleanup" | "vcp_patch"))
                })
        {
            return Err("cleanup requires current root write authority without applicable host or policy denials".into());
        }
        if spec.parent != binding.scope.task {
            return Err("cleanup child belongs to another parent".into());
        }
        let state = self.engine.store().state();
        let task: Task = state
            .record(Collection::Task, child.as_str(), &binding.scope.workspace)?
            .decode()?;
        if !task.state.terminal() {
            return Err("cleanup requires a completed, failed or cancelled child; preserve paused recovery work".into());
        }
        // A descendant still refers to the original native source/metadata owner.
        if graph.children.iter().any(|(id, other)| {
            (other.parent == *child || other.dependencies.contains(child))
                && graph.cleanup.get(id).is_none_or(|c| c.receipt.is_none())
        }) {
            return Err("cleanup is retained by a dependent child or recovery workspace".into());
        }
        for row in state
            .records
            .values()
            .filter(|r| r.workspace == binding.scope.workspace)
        {
            if row.collection == Collection::Attempt {
                let attempt: vcp_domain::accounting::Attempt = row.decode()?;
                if attempt.scope.session == binding.scope.session
                    && matches!(
                        attempt.phase,
                        vcp_domain::accounting::ReservationState::Created
                            | vcp_domain::accounting::ReservationState::Submitted
                            | vcp_domain::accounting::ReservationState::ReconciliationPending
                    )
                {
                    return Err("cleanup waits for active or uncertain attempts".into());
                }
            }
            if row.collection == Collection::Reservation {
                let reservation: vcp_domain::accounting::Reservation = row.decode()?;
                if reservation.root == task.root
                    && (reservation.liability != Micros::ZERO
                        || matches!(
                            reservation.phase,
                            vcp_domain::accounting::ReservationState::Created
                                | vcp_domain::accounting::ReservationState::Submitted
                                | vcp_domain::accounting::ReservationState::ReconciliationPending
                        ))
                {
                    return Err(
                        "cleanup waits for root reservations and unknown liabilities to settle"
                            .into(),
                    );
                }
            }
            if row.collection == Collection::Effect {
                let effect: vcp_domain::effect::Effect = row.decode()?;
                if effect.scope.session == binding.scope.session
                    && !matches!(
                        effect.state,
                        vcp_domain::effect::EffectState::Succeeded
                            | vcp_domain::effect::EffectState::Failed
                            | vcp_domain::effect::EffectState::Cancelled
                    )
                {
                    return Err(
                        "cleanup waits for active or unknown effects and integration receipts"
                            .into(),
                    );
                }
            }
        }
        Ok((graph, spec, task))
    }
    fn cleanup_artifact<T: serde::de::DeserializeOwned>(
        &self,
        id: &ArtifactId,
        scope: &Scope,
    ) -> worker::Result<T> {
        let descriptor: ArtifactDescriptor = self
            .engine
            .store()
            .state()
            .record(Collection::Artifact, id.as_str(), &scope.workspace)?
            .decode()?;
        if descriptor.spec.scope != *scope
            || descriptor.state != CaptureState::Complete
            || descriptor.length.get() > 128 * 1024 * 1024
        {
            return Err("cleanup evidence is unavailable, outside scope or exceeds bounds".into());
        }
        let mut bytes = Vec::new();
        vcp_audit::history::History::read_artifact(
            self.engine.store(),
            &self.history_access(),
            id,
            &mut bytes,
        )?;
        Ok(serde_json::from_slice(&bytes)?)
    }
    fn publish_cleanup(
        &mut self,
        binding: &ThreadBinding,
        child: TaskId,
        revision: Revision,
        cleanup: ChildCleanup,
    ) -> worker::Result<()> {
        let facts = vcp_engine::HostFacts {
            now: worker::now(),
            policy: vcp_engine::policy::current(
                self.engine.store().state(),
                &self.config.workspace,
            )?
            .revision,
            may_execute: true,
            resume: None,
        };
        self.runtime.block_on(self.engine.record_child_cleanup(
            &binding.scope,
            vcp_engine::agents::NativeCleanupEvidence {
                child,
                expected_graph: revision,
                cleanup,
            },
            &self.access,
            &facts,
        ))?;
        Ok(())
    }
}
impl CanonicalHost {
    fn record_child_cleanup_failure(
        &self,
        binding: ThreadBinding,
        child: TaskId,
        mut reason: String,
    ) -> Result<(), String> {
        if reason.len() > 2048 {
            let mut end = 2048;
            while !reason.is_char_boundary(end) {
                end -= 1;
            }
            reason.truncate(end);
        }
        self.worker.run_cleanup(move |context| {
            let (graph, spec) = context
                .child_assignment_record(&child)?
                .ok_or("cleanup graph missing")?;
            if spec.parent != binding.scope.task {
                return Err("cleanup parent differs".into());
            }
            let mut cleanup = graph
                .cleanup
                .get(&child)
                .ok_or("cleanup intent missing")?
                .clone();
            if cleanup.receipt.is_some() {
                return Ok(());
            }
            if cleanup.diagnostics.len() >= 128 {
                return Err(
                    "cleanup diagnostic capacity reached; prior 128 failures retained".into(),
                );
            }
            let scope = Scope {
                task: child.clone(),
                ..binding.scope.clone()
            };
            let evidence = context.capture(
                &scope,
                Channel::Evidence,
                &canonical_bytes(
                    &serde_json::json!({"child":child,"intent":cleanup.intent,"reason":reason}),
                )?,
                "child-cleanup-failure/1",
            )?;
            cleanup
                .diagnostics
                .push(vcp_domain::agents::ChildCleanupDiagnostic {
                    artifact: evidence.spec.id,
                    reason,
                });
            context.publish_cleanup(&binding, child, graph.revision, cleanup)
        })
    }
    fn cleanup_unbound(&self, child: &TaskId) -> Result<(), String> {
        if self
            .bindings
            .lock()
            .map_err(|_| "binding lock poisoned")?
            .values()
            .any(|b| b.scope.task == *child)
        {
            return Err("child still has a retained runtime owner; cancel it, close the owner and reopen before cleanup".into());
        }
        Ok(())
    }
    pub async fn prepare_child_cleanup(
        &self,
        parent: ThreadId,
        child: TaskId,
        snapshotter: &Snapshotter,
        reject_edits: bool,
    ) -> Result<ChildCleanupPreview, String> {
        self.cleanup_unbound(&child)?;
        let generation = scheduler::generation(&self.runtime, parent)?;
        let binding = self.binding(parent)?;
        let selected = child.clone();
        let (graph,spec,task,source,isolated,metadata,registration,parent_registration,base)=self.worker.run(move|context|{
            let (graph,spec,task)=context.cleanup_eligible(&binding,&selected)?;
            if graph.cleanup.contains_key(&selected){return Err("cleanup already has a durable intent; use reconcile, never prepare a replacement".into());}
            let registration=context.child_registration(&selected,&graph,&spec)?;
            let base:WorkspaceSnapshot=context.cleanup_artifact(&spec.snapshot,&binding.scope)?;
            let parent_registration=context.child_assignment_record(&binding.scope.task)?.map(|(g,s)|context.child_registration(&binding.scope.task,&g,&s)).transpose()?;
            Ok((graph,spec,task,context.recovery_task_root(&binding.scope.task)?,context.recovery_task_root(&selected)?,context.tool_root()?,registration,parent_registration,base))
        })?;
        let _source_claim = self
            .scheduler
            .snapshot(spec.parent.clone(), source.identity.root.clone())?;
        let _child_claim = self
            .scheduler
            .snapshot(child.clone(), isolated.identity.root.clone())?;
        let checked = self.binding(parent)?;
        let selected = child.clone();
        self.worker
            .run(move |context| context.cleanup_eligible(&checked, &selected).map(|_| ()))?;
        let inputs = merge::Inputs {
            parent: &source,
            child: &isolated,
            metadata_owner: &metadata,
            parent_registration: parent_registration.as_ref(),
            registration: &registration,
            base: &base,
            assignment: &spec,
        };
        let result = merge::observe_result(snapshotter, &inputs)
            .await
            .map_err(|e| e.to_string())?;
        let changed = result.files != base.files;
        if changed && !reject_edits {
            return Err("child has changed files; retain/integrate them first or explicitly preview with --reject-edits".into());
        }
        let parent_path = registration
            .absolute_path
            .parent()
            .ok_or("registered disposable parent missing")?;
        let disposable = Root::open(
            RootIdentity {
                root: RootId::new(),
                worktree: "cleanup-disposable-parent".into(),
                ..registration.child.clone()
            },
            parent_path,
        )
        .map_err(|e| e.to_string())?;
        let intent = snapshotter
            .prepare_cleanup(
                &disposable,
                &isolated,
                registration.git_metadata_owner.as_ref().map(|_| &metadata),
                &registration,
                &graph.ready[&child].native_identity,
            )
            .map_err(|e| e.to_string())?;
        // Both observations must name the same retained file bytes. Metadata is
        // retained separately by Git and the canonical registration/intent.
        for entry in intent
            .entries
            .iter()
            .filter(|e| !e.directory && e.path != ".git" && e.path != ".vcp-child-owner")
        {
            let bytes = result
                .files
                .iter()
                .find(|f| f.path == entry.path)
                .and_then(|f| f.working.as_ref())
                .ok_or("cleanup inventory contains unretained output")?;
            if entry.sha256.as_deref() != Some(vcp_protocol::digest_bytes(bytes).as_str()) {
                return Err("child changed between retention and cleanup preview".into());
            }
        }
        scheduler::check_generation(&self.runtime, parent, generation)?;
        Ok(ChildCleanupPreview {
            parent,
            child,
            generation,
            graph_revision: graph.revision,
            task_revision: task.revision,
            intent,
            result,
            rejected_edits: changed && reject_edits,
        })
    }
    pub fn apply_child_cleanup(
        &self,
        preview: ChildCleanupPreview,
    ) -> Result<CleanupReceipt, String> {
        scheduler::check_generation(&self.runtime, preview.parent, preview.generation)?;
        self.cleanup_unbound(&preview.child)?;
        let binding = self.binding(preview.parent)?;
        let child = preview.child.clone();
        let selected = child.clone();
        self.worker.run(move |context| {
            let (graph, _, task) = context.cleanup_eligible(&binding, &selected)?;
            if graph.revision != preview.graph_revision
                || task.revision != preview.task_revision
                || graph.cleanup.contains_key(&selected)
            {
                return Err("cleanup preview is stale".into());
            }
            let result = context.capture(
                &task.scope,
                Channel::Evidence,
                &canonical_bytes(&preview.result)?,
                "child-cleanup-retained-result/1",
            )?;
            let intent = context.capture(
                &task.scope,
                Channel::Evidence,
                &canonical_bytes(&preview.intent)?,
                "child-cleanup-intent/1",
            )?;
            context.publish_cleanup(
                &binding,
                selected,
                graph.revision,
                ChildCleanup {
                    intent: intent.spec.id,
                    retained_result: result.spec.id,
                    rejected_edits: preview.rejected_edits,
                    receipt: None,
                    diagnostics: vec![],
                },
            )
        })?;
        self.reconcile_child_cleanup(preview.parent, child)
    }
    pub fn reconcile_child_cleanup(
        &self,
        parent: ThreadId,
        child: TaskId,
    ) -> Result<CleanupReceipt, String> {
        self.reconcile_child_cleanup_inner(parent, child, || {})
    }
    #[cfg(feature = "qualification")]
    pub fn qualification_cleanup_before_claim(
        &self,
        parent: ThreadId,
        child: TaskId,
        before_claim: impl FnOnce(),
    ) -> Result<CleanupReceipt, String> {
        self.reconcile_child_cleanup_inner(parent, child, before_claim)
    }
    fn reconcile_child_cleanup_inner(
        &self,
        parent: ThreadId,
        child: TaskId,
        before_claim: impl FnOnce(),
    ) -> Result<CleanupReceipt, String> {
        self.cleanup_unbound(&child)?;
        let generation = scheduler::generation(&self.runtime, parent)?;
        let binding = self.binding(parent)?;
        let selected = child.clone();
        let root = self.worker.run(move |context| {
            let (_, spec) = context
                .child_assignment_record(&selected)?
                .ok_or("child assignment missing")?;
            spec.isolated_root
                .ok_or("cleanup requires isolated root".into())
        })?;
        before_claim();
        let _claim = self.scheduler.snapshot(child.clone(), root)?;
        let checked = binding.clone();
        let selected = child.clone();
        let (intent, complete) = self.worker.run(move |context| {
            let (graph, _, task) = context.cleanup_eligible(&checked, &selected)?;
            let cleanup = graph
                .cleanup
                .get(&selected)
                .ok_or("cleanup has no durable intent; preview first")?;
            let intent: CleanupIntent = context.cleanup_artifact(&cleanup.intent, &task.scope)?;
            let complete = cleanup
                .receipt
                .as_ref()
                .map(|id| context.cleanup_artifact::<CleanupReceipt>(id, &task.scope))
                .transpose()?;
            Ok((intent, complete))
        })?;
        if let Some(receipt) = complete {
            return Ok(receipt);
        }
        let removal = (|| -> Result<CleanupReceipt, String> {
            let disposable = Root::open(
                intent.disposable_parent.identity.clone(),
                &intent.disposable_parent.absolute_path,
            )
            .map_err(|e| e.to_string())?;
            let metadata = intent
                .registration
                .git_metadata_owner
                .as_ref()
                .map(|owner| Root::open(owner.identity.clone(), &owner.absolute_path))
                .transpose()
                .map_err(|e| e.to_string())?;
            scheduler::check_generation(&self.runtime, parent, generation)?;
            vcp_repository::cleanup::remove_cleanup_with_gate(
                &disposable,
                metadata.as_ref(),
                &intent,
                || {
                    let state =
                        self.runtime.0.state.lock().map_err(|_| {
                            vcp_repository::Error::Scope("lifecycle poisoned".into())
                        })?;
                    if self.worker.fenced()
                        || !state.attached
                        || state.held(parent)
                        || !state.admission_current(parent, generation)
                    {
                        return Err(vcp_repository::Error::Scope(
                            "cleanup paused, superseded or unowned; reconcile the retained intent"
                                .into(),
                        ));
                    }
                    Ok(state)
                },
            )
            .map_err(|e| {
                format!(
                    "cleanup incomplete; durable intent retained for explicit reconciliation: {e}"
                )
            })
        })();
        let receipt = match removal {
            Ok(receipt) => receipt,
            Err(reason) => {
                self.record_child_cleanup_failure(binding.clone(), child.clone(), reason.clone())
                    .map_err(|recording| {
                        format!("{reason}; diagnostic publication failed: {recording}")
                    })?;
                return Err(reason);
            }
        };
        let result = receipt.clone();
        // Publishing a receipt is reconciliation only; a late pause may prevent
        // this acknowledgement, but it cannot discard or replace the intent.
        self.worker.run(move |context| {
            let (graph, _, task) = context.cleanup_eligible(&binding, &child)?;
            let mut cleanup = graph
                .cleanup
                .get(&child)
                .ok_or("cleanup intent missing")?
                .clone();
            let evidence = context.capture(
                &task.scope,
                Channel::Evidence,
                &canonical_bytes(&result)?,
                "child-cleanup-receipt/1",
            )?;
            cleanup.receipt = Some(evidence.spec.id);
            context.publish_cleanup(&binding, child, graph.revision, cleanup)
        })?;
        Ok(receipt)
    }
}
