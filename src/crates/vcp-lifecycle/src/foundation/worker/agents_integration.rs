// SPDX-License-Identifier: Apache-2.0
//! Owner result collection and ordinary broker admission, never completion authority.
use super::super::*;
use vcp_domain::{
    accounting::{Attempt, Reservation, ReservationState},
    agents::ChildMode,
    effect::{Effect, EffectState},
    task::{Task, TaskState},
};
use vcp_repository::{
    dirty_snapshot::WorkspaceSnapshot,
    merge::{self, ChildPacket},
    worktree::Snapshotter,
};
use vcp_store::contract::Collection;

pub struct ChildIntegration {
    pub packet: ArtifactId,
    pub plan: ArtifactId,
    pub proposal: Option<ToolProposal>,
    pub rejection: Option<String>,
}

impl worker::Context {
    fn child_transcript_findings(
        &self,
        child: &TaskId,
    ) -> worker::Result<Vec<merge::ReviewFinding>> {
        let mut findings = Vec::new();
        // Canonical record order is deterministic, not a claim of recency. Keep
        // artifact references when full text exceeds the bounded preview.
        for row in self
            .engine
            .store()
            .state()
            .records
            .values()
            .filter(|row| {
                row.workspace == self.config.workspace
                    && row.collection == Collection::Artifact
                    && row.value["spec"]["scope"]["task"] == child.as_str()
                    && row.value["spec"]["channel"] == "child_transcript"
            })
            .take(8)
        {
            let descriptor: ArtifactDescriptor = row.decode()?;
            let mut note = format!("Untrusted unstamped child transcript; artifact={} sha256={}. No examined revision or defect is inferred. ", descriptor.spec.id, descriptor.sha256);
            if descriptor.state == CaptureState::Complete && descriptor.length.get() <= 64 * 1024 {
                let mut bytes = Vec::new();
                vcp_audit::history::History::read_artifact(
                    self.engine.store(),
                    &self.history_access(),
                    &descriptor.spec.id,
                    &mut bytes,
                )?;
                if let Ok(text) = std::str::from_utf8(&bytes) {
                    let mut end = text.len().min(4096);
                    while !text.is_char_boundary(end) {
                        end -= 1;
                    }
                    note.push_str(&text[..end]);
                    if end < text.len() {
                        note.push_str(" [preview truncated; inspect retained artifact]");
                    }
                } else {
                    note.push_str("Non-UTF-8 content; inspect retained artifact.");
                }
            } else {
                note.push_str("Preview unavailable or exceeds read bound; inspect artifact retention metadata.");
            }
            findings.push(note.into());
        }
        Ok(findings)
    }
    pub(super) fn integration_child_quiescent(&self, child: &TaskId) -> worker::Result<()> {
        let task: Task = self
            .engine
            .store()
            .state()
            .record(Collection::Task, child.as_str(), &self.config.workspace)?
            .decode()?;
        if !matches!(
            task.state,
            TaskState::Paused
                | TaskState::WaitingForInput
                | TaskState::Blocked
                | TaskState::Completed
        ) {
            return Err("child must be stopped or result-ready before integration".into());
        }
        for row in self
            .engine
            .store()
            .state()
            .records
            .values()
            .filter(|r| r.workspace == self.config.workspace)
        {
            if row.collection == Collection::Attempt {
                let attempt: Attempt = row.decode()?;
                if &attempt.scope.task == child
                    && matches!(
                        attempt.phase,
                        ReservationState::Created
                            | ReservationState::Submitted
                            | ReservationState::ReconciliationPending
                    )
                {
                    return Err("child has an active or unresolved attempt".into());
                }
            }
            if row.collection == Collection::Reservation {
                let reservation: Reservation = row.decode()?;
                if &reservation.scope.task == child
                    && (reservation.liability != Micros::ZERO
                        || matches!(
                            reservation.phase,
                            ReservationState::Created
                                | ReservationState::Submitted
                                | ReservationState::ReconciliationPending
                        ))
                {
                    return Err("child has an active or unresolved reservation".into());
                }
            }
            if row.collection == Collection::Effect {
                let effect: Effect = row.decode()?;
                if &effect.scope.task == child
                    && !matches!(
                        effect.state,
                        EffectState::Succeeded | EffectState::Failed | EffectState::Cancelled
                    )
                {
                    return Err("child has an active or unresolved effect".into());
                }
            }
        }
        Ok(())
    }

    pub(super) fn validate_integration_child(
        &self,
        binding: &ThreadBinding,
        prepared: &vcp_tools::Prepared,
    ) -> worker::Result<()> {
        let Some(child) = prepared.integration_child() else {
            return Ok(());
        };
        let (_, spec) = self
            .child_assignment(child)?
            .ok_or("integration child assignment missing")?;
        if spec.parent != binding.scope.task || spec.mode != ChildMode::IsolatedWrite {
            return Err("integration child belongs to a different parent or mode".into());
        }
        self.integration_child_quiescent(child)
    }
}

impl CanonicalHost {
    /// Read the latest retained review evidence under current artifact access.
    /// This is historical, untrusted evidence; it never dispatches or verifies.
    pub fn child_review_findings(
        &self,
        parent: ThreadId,
        child: TaskId,
    ) -> Result<serde_json::Value, String> {
        let binding = self.binding(parent)?;
        self.worker.run_cleanup(move |context| {
            let (graph, spec) = context.child_assignment_record(&child)?
                .ok_or("child assignment missing")?;
            if spec.parent != binding.scope.task {
                return Err("child belongs to a different parent".into());
            }
            let transcripts = context.child_transcript_findings(&child)?;
            let Some(result) = graph.results.get(&child).and_then(|results| results.last()) else {
                return Ok(serde_json::json!({"child":child,"findings":transcripts,"observation":"up to eight untrusted unstamped transcript previews in canonical record order; no retained review packet or correctness claim"}));
            };
            let descriptor: ArtifactDescriptor = context.engine.store().state()
                .record(Collection::Artifact, result.packet.as_str(), &binding.scope.workspace)?.decode()?;
            if descriptor.state != CaptureState::Complete || descriptor.length.get() > 1024 * 1024 {
                return Err("retained child review packet is unavailable or exceeds bounds".into());
            }
            let mut bytes = Vec::new();
            vcp_audit::history::History::read_artifact(context.engine.store(), &context.history_access(), &result.packet, &mut bytes)?;
            let packet: ChildPacket = serde_json::from_slice(&bytes)?;
            packet.validate_findings()?;
            Ok(serde_json::json!({"child":child,"packet":result.packet,"plan":result.plan,
                "base_fingerprint":packet.base_fingerprint,"current_fingerprint":packet.result_fingerprint,
                "findings":packet.findings,"transcripts":transcripts,
                "observation":"historical untrusted review evidence; current workspace not re-examined; no findings means no supported findings in examined scope, not guaranteed correctness"}))
        })
    }

    /// Observe the registered, quiescent child itself; users need not manufacture
    /// result fingerprints. The normal preparation and broker path still decide
    /// whether any proposal is admissible, and this method never applies it.
    pub async fn prepare_observed_child_integration(
        &self,
        parent: ThreadId,
        child: TaskId,
        snapshotter: &Snapshotter,
    ) -> Result<ChildIntegration, String> {
        let binding = self.binding(parent)?;
        let selected = child.clone();
        let findings = self.worker.run(move |context| {
            let (_, spec) = context
                .child_assignment_record(&selected)?
                .ok_or("child assignment missing")?;
            if spec.parent != binding.scope.task {
                return Err("child belongs to a different parent".into());
            }
            context.child_transcript_findings(&selected)
        })?;
        self.prepare_observed_child_review(parent, child, snapshotter, findings)
            .await
    }

    /// Explicitly attach bounded untrusted findings to a fresh owner observation.
    /// The caller supplies examined revisions; the owner never invents provenance
    /// for model prose and preparation rejects stale or out-of-scope claims.
    pub async fn prepare_observed_child_review(
        &self,
        parent: ThreadId,
        child: TaskId,
        snapshotter: &Snapshotter,
        findings: Vec<merge::ReviewFinding>,
    ) -> Result<ChildIntegration, String> {
        let generation = scheduler::generation(&self.runtime, parent)?;
        let binding = self.binding(parent)?;
        let scoped = binding.clone();
        let selected = child.clone();
        let (
            source,
            isolated,
            metadata_owner,
            registration,
            parent_registration,
            base,
            spec,
            revision,
        ) = self.worker.run(move |context| {
            context.can_start(&scoped)?;
            context.child_context_scope(&scoped)?;
            context.tool_read_access(
                &RootId::parse(context.config.workspace.as_str())?,
                "vcp_read",
            )?;
            let (graph, spec) = context
                .child_assignment(&selected)?
                .ok_or("child assignment missing")?;
            if spec.parent != scoped.scope.task {
                return Err("child belongs to a different parent".into());
            }
            context.integration_child_quiescent(&selected)?;
            let source = context.recovery_task_root(&scoped.scope.task)?;
            let isolated = context.recovery_task_root(&selected)?;
            let registration = context.child_registration(&selected, &graph, &spec)?;
            let parent_registration = context
                .child_assignment_record(&scoped.scope.task)?
                .map(|(g, s)| context.child_registration(&scoped.scope.task, &g, &s))
                .transpose()?;
            let descriptor: ArtifactDescriptor = context
                .engine
                .store()
                .state()
                .record(
                    Collection::Artifact,
                    spec.snapshot.as_str(),
                    &scoped.scope.workspace,
                )?
                .decode()?;
            if descriptor.state != CaptureState::Complete
                || descriptor.sha256 != spec.snapshot_digest
                || descriptor.length.get() > 128 * 1024 * 1024
            {
                return Err("child base artifact differs".into());
            }
            let mut bytes = Vec::new();
            vcp_audit::history::History::read_artifact(
                context.engine.store(),
                &context.history_access(),
                &spec.snapshot,
                &mut bytes,
            )?;
            let base: WorkspaceSnapshot = serde_json::from_slice(&bytes)?;
            Ok((
                source,
                isolated,
                context.tool_root()?,
                registration,
                parent_registration,
                base,
                spec,
                graph.revision,
            ))
        })?;
        let mut packet = {
            let _parent_claim = self
                .scheduler
                .snapshot(binding.scope.task.clone(), source.identity.root.clone())?;
            let _child_claim = self
                .scheduler
                .snapshot(child.clone(), isolated.identity.root.clone())?;
            self.child_preparation(parent, generation, async {
                merge::observe_packet(
                    snapshotter,
                    &merge::Inputs {
                        parent: &source,
                        child: &isolated,
                        metadata_owner: &metadata_owner,
                        parent_registration: parent_registration.as_ref(),
                        registration: &registration,
                        base: &base,
                        assignment: &spec,
                    },
                )
                .await
                .map_err(|e| e.to_string())
            })
            .await?
        };
        let selected = child.clone();
        self.worker.run(move |context| {
            context.can_start(&binding)?;
            context.integration_child_quiescent(&selected)?;
            let (graph, _) = context
                .child_assignment(&selected)?
                .ok_or("child assignment missing")?;
            if graph.revision != revision {
                return Err("child graph changed during result observation".into());
            }
            Ok(())
        })?;
        scheduler::check_generation(&self.runtime, parent, generation)?;
        packet.findings = findings;
        self.prepare_child_integration(parent, child, snapshotter, packet)
            .await
    }

    /// Collect untrusted findings even when no patch can be admitted. Applying a
    /// returned proposal still requires the normal current-parent broker checks.
    pub async fn prepare_child_integration(
        &self,
        parent: ThreadId,
        child: TaskId,
        snapshotter: &Snapshotter,
        packet: ChildPacket,
    ) -> Result<ChildIntegration, String> {
        let binding = self.binding(parent)?;
        let bytes = vcp_protocol::canonical_bytes(&packet).map_err(|e| e.to_string())?;
        if bytes.len() > 1024 * 1024 {
            return Err("child packet exceeds capture limit".into());
        }
        let scoped = binding.clone();
        let selected = child.clone();
        let packet_id = self.worker.run(move |context| {
            let (_, spec) = context
                .child_assignment_record(&selected)?
                .ok_or("child assignment missing")?;
            if spec.parent != scoped.scope.task {
                return Err("child belongs to a different parent".into());
            }
            Ok(context
                .capture(
                    &scoped.scope,
                    Channel::Evidence,
                    &bytes,
                    "child-result-packet/1",
                )?
                .spec
                .id)
        })?;
        let prepared = async {
            packet.validate_findings().map_err(|e| e.to_string())?;
            if packet.changed_paths.len() > 128 {
                return Err("child result packet exceeds field limits".to_owned());
            }
            let generation = scheduler::generation(&self.runtime, parent)?;
            let scoped = binding.clone();
            let selected = child.clone();
            let inputs = self.worker.run(move |context| {
                context.can_start(&scoped)?;
                let (graph, spec) = context
                    .child_assignment(&selected)?
                    .ok_or("child assignment missing")?;
                if spec.parent != scoped.scope.task {
                    return Err("child parent changed".into());
                }
                context.integration_child_quiescent(&selected)?;
                if spec.mode == ChildMode::IsolatedWrite {
                    context.tool_identity(&scoped, "vcp_patch")?;
                } else {
                    context.tool_read_access(
                        &RootId::parse(context.config.workspace.as_str())?,
                        "vcp_read",
                    )?;
                }
                context.child_context_scope(&scoped)?;
                // Current read policy and immutable native registration are both
                // required; expired execution stamps never enable a recovery write.
                let source = context.recovery_task_root(&scoped.scope.task)?;
                let isolated = context.recovery_task_root(&selected)?;
                let metadata_owner = context.tool_root()?;
                let registration = context.child_registration(&selected, &graph, &spec)?;
                let parent_registration = context
                    .child_assignment_record(&scoped.scope.task)?
                    .map(|(g, s)| context.child_registration(&scoped.scope.task, &g, &s))
                    .transpose()?;
                let descriptor: ArtifactDescriptor = context
                    .engine
                    .store()
                    .state()
                    .record(
                        Collection::Artifact,
                        spec.snapshot.as_str(),
                        &scoped.scope.workspace,
                    )?
                    .decode()?;
                if descriptor.state != CaptureState::Complete
                    || descriptor.sha256 != spec.snapshot_digest
                    || descriptor.length.get() > 128 * 1024 * 1024
                {
                    return Err("child base artifact differs".into());
                }
                let mut bytes = Vec::new();
                vcp_audit::history::History::read_artifact(
                    context.engine.store(),
                    &context.history_access(),
                    &spec.snapshot,
                    &mut bytes,
                )?;
                let base: WorkspaceSnapshot = serde_json::from_slice(&bytes)?;
                Ok(Some((
                    source,
                    isolated,
                    metadata_owner,
                    registration,
                    parent_registration,
                    base,
                    spec,
                    graph.revision,
                )))
            })?;
            let Some((
                source,
                isolated,
                metadata_owner,
                registration,
                parent_registration,
                base,
                spec,
                revision,
            )) = inputs
            else {
                return Ok((None, None));
            };
            let _parent_claim = self
                .scheduler
                .snapshot(binding.scope.task.clone(), source.identity.root.clone())?;
            let _child_claim = self
                .scheduler
                .snapshot(child.clone(), isolated.identity.root.clone())?;
            let mut plan = self
                .child_preparation(parent, generation, async {
                    merge::prepare(
                        snapshotter,
                        merge::Inputs {
                            parent: &source,
                            child: &isolated,
                            metadata_owner: &metadata_owner,
                            parent_registration: parent_registration.as_ref(),
                            registration: &registration,
                            base: &base,
                            assignment: &spec,
                        },
                        &packet,
                    )
                    .await
                    .map_err(|e| e.to_string())
                })
                .await?;
            if spec.mode == ChildMode::ReadOnly {
                if plan.rejection.is_none() {
                    plan.rejection = Some(
                        "read-only findings retained as untrusted evidence; no edit plan".into(),
                    );
                }
                plan.changes.clear();
                return Ok((Some(plan), None));
            }
            if !plan.ready() || plan.changes.is_empty() {
                return Ok((Some(plan), None));
            }
            let scoped = binding.clone();
            let selected = child.clone();
            let for_tool = plan.clone();
            let tool = self.worker.run(move |context| {
                context.integration_child_quiescent(&selected)?;
                let (graph, _) = context
                    .child_assignment(&selected)?
                    .ok_or("child assignment missing")?;
                if graph.revision != revision {
                    return Err("child graph changed during integration".into());
                }
                let identity = context.tool_identity(&scoped, "vcp_patch")?;
                Ok(Arc::new(vcp_tools::integration::prepare(
                    source,
                    metadata_owner,
                    identity,
                    selected,
                    for_tool,
                )?))
            });
            match tool {
                Ok(tool) => Ok((Some(plan), Some(tool))),
                Err(error) => {
                    let mut rejected = plan;
                    rejected.rejection = Some(error);
                    rejected.changes.clear();
                    Ok((Some(rejected), None))
                }
            }
        }
        .await;
        let (plan, tool, mut rejection) = match prepared {
            Ok((plan, tool)) => {
                let rejection = plan.as_ref().and_then(|p| p.rejection.clone()).or_else(|| {
                    plan.as_ref()
                        .filter(|p| !p.conflicts.is_empty())
                        .map(|_| "child integration has conflicts".into())
                });
                (plan, tool, rejection)
            }
            Err(error) => (None, None, Some(error)),
        };
        let evidence = serde_json::json!({"child":child,"packet":packet_id,"plan":plan,"rejection":rejection,"findings":packet.findings});
        let scope = binding.scope.clone();
        let mut plan_id = self.worker.run(move |context| {
            Ok(context
                .capture(
                    &scope,
                    Channel::Evidence,
                    &vcp_protocol::canonical_bytes(&evidence)?,
                    "child-integration-plan/1",
                )?
                .spec
                .id)
        })?;
        let proposal = match tool {
            Some(tool) => match self.propose_prepared_tool(parent, tool) {
                Ok(proposal) => Some(proposal),
                Err(error) => {
                    let scope = binding.scope.clone();
                    let evidence = serde_json::json!({"child":child,"packet":packet_id,"prepared_plan":plan_id,"rejection":error});
                    plan_id = self.worker.run(move |context| {
                        Ok(context
                            .capture(
                                &scope,
                                Channel::Evidence,
                                &vcp_protocol::canonical_bytes(&evidence)?,
                                "child-integration-admission/1",
                            )?
                            .spec
                            .id)
                    })?;
                    rejection = Some(error);
                    None
                }
            },
            None => None,
        };
        let result = vcp_domain::agents::ChildResultRef {
            packet: packet_id.clone(),
            plan: plan_id.clone(),
            effect: proposal.as_ref().map(|p| p.effect().clone()),
        };
        self.worker.run(move |context| {
            let (graph, _) = context
                .child_assignment_record(&child)?
                .ok_or("child assignment missing")?;
            let parent: Task = context
                .engine
                .store()
                .state()
                .record(
                    Collection::Task,
                    binding.scope.task.as_str(),
                    &binding.scope.workspace,
                )?
                .decode()?;
            context.command(
                Command::SubmitChildResult {
                    child,
                    result,
                    expected_graph: graph.revision,
                },
                Some(binding.scope.task),
                parent.revision,
            )?;
            Ok(())
        })?;
        Ok(ChildIntegration {
            packet: packet_id,
            plan: plan_id,
            proposal,
            rejection,
        })
    }
}
