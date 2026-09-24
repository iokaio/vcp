// SPDX-License-Identifier: Apache-2.0
//! Versioned lifecycle adapter. All executable effects use the ordinary broker.
pub mod adapters;
use super::*;
use vcp_extensions::hooks::{
    input::{HookInput, HookLimits},
    planner::{self, PlannedHook},
    receipt::{HookReceipt, HookStatus},
    registry::{FailurePolicy, HookDefinition, HookEvent},
    result::{validate_output, ArgumentRewrite},
};
use vcp_store::contract::{key, Collection};

fn artifact(identity: &str, kind: &str) -> Result<ArtifactId, String> {
    ArtifactId::parse(format!("hook-{kind}-{identity}")).map_err(|e| e.to_string())
}

pub(super) fn process_request(plan: &PlannedHook) -> Result<vcp_tools::process::Request, String> {
    let wire = vcp_extensions::hooks::runner::request(plan, HookLimits::default())
        .map_err(|e| e.to_string())?;
    Ok(vcp_tools::process::Request {
        profile: plan.definition.command.profile.clone(),
        arguments: wire.arguments,
        directory: plan.definition.command.working_directory.clone(),
        timeout_ms: plan.definition.timeout_ms,
        output_bytes: plan.definition.max_output_bytes as u64,
        input: None,
    })
}

/// Effects may change workspace contents; executable/profile input versions and
/// the authority that admitted the hook must remain current for publication.
pub(super) fn source_fence(
    prepared: &vcp_tools::process::Prepared,
) -> Result<serde_json::Value, String> {
    let evidence: serde_json::Value =
        serde_json::from_slice(&prepared.evidence().map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let op = prepared.authority().operation();
    Ok(
        serde_json::json!({"profile":evidence["profile"],"executable":evidence["executable"],
        "directory_identity":evidence["directory_identity"],"pinned_inputs":evidence["pinned_inputs"],
        "scope":op.scope,"actor":op.actor,"host":op.host,"binding":op.binding,
        "authority":op.authority,"policy":op.policy,"steering":op.steering}),
    )
}

/// Nonserializable one-use process capability. The serialized plan grants nothing.
pub struct HookProposal {
    process: ProcessProposal,
    plan: PlannedHook,
    thread: ThreadId,
    binding: ThreadBinding,
    #[cfg(feature = "qualification")]
    before_receipt: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
}
impl HookProposal {
    pub fn decision(&self) -> &vcp_policy::Decision {
        &self.process.decision
    }
    pub fn question(&self) -> Option<&ApprovalId> {
        self.process.question.as_ref()
    }
    pub fn effect(&self) -> &ToolRunId {
        self.process.effect()
    }
    #[cfg(feature = "qualification")]
    pub fn qualification_block_before_receipt(
        mut self,
        arrived: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    ) -> Self {
        self.before_receipt = Some((arrived, release));
        self
    }
}

#[derive(Clone, Debug)]
pub struct HookOutcome {
    pub receipt: HookReceipt,
    pub artifact: ArtifactId,
    /// The gate may inspect a prior decision, but must not publish context twice.
    pub duplicate: bool,
}

/// Inspection works while paused. Missing result means reconciliation, never rerun.
#[derive(Clone, Debug)]
pub struct HookRecovery {
    pub plan: serde_json::Value,
    pub receipt: Option<HookReceipt>,
    pub effect: vcp_domain::effect::Effect,
}

impl CanonicalHost {
    /// Trusted owner supplies only the authorized event projection. Canonical
    /// scope, root and steering are supplied here rather than accepted from hooks.
    pub fn hook_input(
        &self,
        thread: ThreadId,
        event: HookEvent,
        event_id: String,
        causation_id: String,
        depth: u32,
        artifact_refs: Vec<ArtifactId>,
        payload: serde_json::Value,
    ) -> Result<HookInput, String> {
        let binding = self.binding(thread)?;
        self.worker.run(move |context| {
            let identity = context.tool_identity(&binding, "vcp_exec")?;
            let input = HookInput {
                schema_version: 1,
                event,
                event_id,
                scope: binding.scope.clone(),
                root_task: context.config.root_task.clone(),
                steering_revision: identity.steering.get(),
                causation_id,
                depth,
                artifact_refs,
                payload,
            };
            input.validate(HookLimits::default())?;
            context.check_hook_input(&binding, &input)?;
            Ok(input)
        })
    }

    pub fn inspect_hook(
        &self,
        thread: ThreadId,
        identity: String,
    ) -> Result<Option<HookRecovery>, String> {
        let binding = self.binding(thread)?;
        let plan_id = artifact(&identity, "plan")?;
        let result_id = artifact(&identity, "result")?;
        self.worker.run(move |context| {
            let Some(bytes) = context.hook_artifact(&binding.scope, &plan_id, 1024 * 1024)? else {
                return Ok(None);
            };
            let plan: serde_json::Value = serde_json::from_slice(&bytes)?;
            let effect: ToolRunId = serde_json::from_value(plan["effect"].clone())?;
            let effect = context
                .engine
                .store()
                .state()
                .record(
                    Collection::Effect,
                    effect.as_str(),
                    &binding.scope.workspace,
                )?
                .decode()?;
            let receipt = context
                .hook_artifact(&binding.scope, &result_id, 2 * 1024 * 1024)?
                .map(|b| serde_json::from_slice(&b))
                .transpose()?;
            Ok(Some(HookRecovery {
                plan,
                receipt,
                effect,
            }))
        })
    }

    pub fn prepare_hook(
        &self,
        thread: ThreadId,
        plan: PlannedHook,
    ) -> Result<HookProposal, String> {
        plan.validate(HookLimits::default())
            .map_err(|e| e.to_string())?;
        let binding = self.binding(thread)?;
        let scoped = binding.clone();
        let checked = plan.clone();
        let id = artifact(&plan.identity, "plan")?;
        let plan_id = id.clone();
        self.worker.run(move |context| {
            context.check_hook_input(&scoped, &checked.input)?;
            if context
                .engine
                .store()
                .state()
                .records
                .contains_key(&key(Collection::Artifact, plan_id.as_str()))
            {
                return Err(
                    "duplicate hook delivery: inspect its durable receipt or reconcile; no replay"
                        .into(),
                );
            }
            Ok(())
        })?;
        let process = self.prepare_process(thread, process_request(&plan)?)?;
        let fence = source_fence(&process.prepared)?;
        let effect = process.effect().clone();
        let scoped = binding.clone();
        let recorded = plan.clone();
        if let Err(error) = self
            .worker
            .run(move |context| context.reserve_hook(&scoped, &recorded, id, &effect, fence))
        {
            self.cancel_queued_effect(
                binding.clone(),
                process.effect().clone(),
                "hook reservation rejected".into(),
            )?;
            return Err(error);
        }
        Ok(HookProposal {
            process,
            plan,
            thread,
            binding,
            #[cfg(feature = "qualification")]
            before_receipt: None,
        })
    }

    pub async fn dispatch_hook(&self, proposal: HookProposal) -> Result<HookOutcome, String> {
        let HookProposal {
            process,
            plan,
            thread,
            binding,
            #[cfg(feature = "qualification")]
            before_receipt,
        } = proposal;
        let checked = plan.clone();
        let scoped = binding.clone();
        if let Err(error) = self
            .worker
            .run(move |context| context.check_hook_input(&scoped, &checked.input))
        {
            self.cancel_queued_effect(
                binding.clone(),
                process.effect().clone(),
                "hook input changed before dispatch".into(),
            )?;
            return Err(error);
        }
        let effect = process.effect().clone();
        let bytes = vcp_extensions::hooks::runner::request(&plan, HookLimits::default())
            .map_err(|e| e.to_string())?
            .stdin;
        let ceiling = (plan.definition.max_output_bytes.max(bytes.len()) + 1).min(1024 * 1024);
        let mut child = self
            .schedule_duplex_process(
                process,
                DuplexIoLimits {
                    frame_bytes: ceiling,
                    queued_frames: 2,
                    input_bytes: bytes.len() as u64 + 1,
                    max_messages: 1,
                    stderr_bytes: Some(plan.definition.max_output_bytes as u64),
                },
            )
            .await?;
        let checked = plan.clone();
        let scoped = binding.clone();
        child
            .write_line_checked(&bytes, move |context| {
                context.check_hook_input(&scoped, &checked.input)
            })
            .await?;
        child.close_stdin();
        let outcome = child.wait().await?;
        let output_id = outcome.stdout.spec.id.clone();
        let scoped = binding.clone();
        let maximum = plan.definition.max_output_bytes as u64;
        let bytes = self
            .worker
            .run(move |context| context.hook_artifact(&scoped.scope, &output_id, maximum))?
            .ok_or("missing hook stdout artifact")?;
        let mut receipt = HookReceipt {
            schema_version: 1,
            identity: plan.identity.clone(),
            effect,
            status: HookStatus::Blocked,
            output: None,
            reason: None,
            evidence: vec![
                outcome.stdout.spec.id,
                outcome.stderr.spec.id,
                outcome.evidence.spec.id,
            ],
        };
        if outcome.exit_code != Some(0) || outcome.reason.is_some() {
            receipt.reason = Some(format!(
                "hook execution failed: exit={:?}, stop={:?}",
                outcome.exit_code, outcome.reason
            ));
            if plan.definition.failure_policy == FailurePolicy::Warn {
                receipt.status = HookStatus::Warning;
            }
            if bytes.iter().any(|b| !b.is_ascii_whitespace()) {
                match validate_output(&bytes, &plan) {
                    Err(error) => {
                        receipt.status = HookStatus::Blocked;
                        receipt.reason = Some(format!(
                            "invalid hook output, even though execution failed: {error}"
                        ));
                    }
                    Ok(output) if output.block => receipt.status = HookStatus::Blocked,
                    _ => (),
                }
            }
        } else {
            match validate_output(&bytes, &plan) {
                Ok(output) => {
                    if !output.block {
                        receipt.status = HookStatus::Validated;
                    }
                    receipt.output = Some(output);
                }
                Err(error) => receipt.reason = Some(format!("invalid hook output: {error}")),
            }
        }
        let result_id = artifact(&plan.identity, "result")?;
        #[cfg(feature = "qualification")]
        if let Some((arrived, release)) = before_receipt {
            arrived.notify_one();
            tokio::time::timeout(Duration::from_secs(60), release.notified())
                .await
                .map_err(|_| "hook qualification receipt barrier deadline")?;
        }
        let id = result_id.clone();
        let checked = plan.clone();
        let scoped = binding.clone();
        let receipt = self.worker.run(move |context| {
            // Keep late output as historical evidence, without publishing proposals.
            if let Err(error) = context.check_hook_result(&scoped, &checked) {
                receipt.status = HookStatus::Blocked;
                receipt.reason = Some(format!("late hook output requires revalidation: {error}"));
            }
            context.capture_hook(
                &scoped.scope,
                id,
                &vcp_protocol::canonical_bytes(&receipt)?,
                "vcp-hook-result-v1",
            )?;
            Ok(receipt)
        })?;
        let _ = thread;
        Ok(HookOutcome {
            receipt,
            artifact: result_id,
            duplicate: false,
        })
    }

    /// Sequential lifecycle adapter. Input and order are immutable for the batch;
    /// each rewrite must be applied through a fresh preparation before dispatch.
    pub async fn run_hooks(
        &self,
        thread: ThreadId,
        definitions: &[HookDefinition],
        input: HookInput,
    ) -> Result<Vec<HookOutcome>, String> {
        self.prune_pending_hooks()?;
        let plans =
            planner::plan(definitions, &input, HookLimits::default()).map_err(|e| e.to_string())?;
        let mut outcomes = Vec::new();
        for plan in plans {
            let identity = plan.identity.clone();
            if let Some(recovery) = self.inspect_hook(thread, identity.clone())? {
                if let Some(receipt) = recovery.receipt {
                    let scoped = self.binding(thread)?;
                    let checked = plan.clone();
                    self.worker
                        .run(move |context| context.check_hook_result(&scoped, &checked))?;
                    let blocked = receipt.status == HookStatus::Blocked;
                    outcomes.push(HookOutcome {
                        receipt,
                        artifact: artifact(&identity, "result")?,
                        duplicate: true,
                    });
                    if blocked {
                        break;
                    }
                    continue;
                }
            }
            let pending = self
                .hook_pending
                .lock()
                .map_err(|_| "hook pending lock poisoned")?
                .remove(&identity);
            let resumed_pending = pending.is_some();
            let mut ticket = match pending {
                Some(ticket) if ticket.thread == thread => ticket,
                Some(ticket) => {
                    self.retain_pending_hook(ticket)?;
                    return Err("hook proposal belongs to another live thread".into());
                }
                None => self.prepare_hook(thread, plan)?,
            };
            let prepared = ticket.process.prepared.clone();
            let scoped = ticket.binding.clone();
            let decision = match self
                .worker
                .run(move |context| context.process_decision(&scoped, &prepared))
            {
                Ok(decision) => decision,
                Err(error) => {
                    self.retain_pending_hook(ticket)?;
                    return Err(error);
                }
            };
            if !matches!(decision, vcp_policy::Decision::Allow { .. }) {
                let message = format!(
                    "hook requires current authorization: {decision:?}; approval={:?}; effect={}",
                    ticket.question(),
                    ticket.effect()
                );
                // WaitingForInput intentionally denies dispatch even while the
                // original approval question is still pending. Preserve that
                // live question; a denial without a question is cancelled below.
                self.retain_pending_hook(ticket)?;
                return Err(message);
            }
            if resumed_pending {
                let hook = ticket.plan.clone();
                if let Err(error) = self.refresh_pending_hook_generation(&mut ticket.process, hook)
                {
                    self.retain_pending_hook(ticket)?;
                    return Err(error);
                }
            }
            let outcome = self.dispatch_hook(ticket).await?;
            let blocked = outcome.receipt.status == HookStatus::Blocked;
            outcomes.push(outcome);
            if blocked {
                break;
            }
        }
        Ok(outcomes)
    }

    fn retain_pending_hook(&self, ticket: HookProposal) -> Result<(), String> {
        let mut pending = self
            .hook_pending
            .lock()
            .map_err(|_| "hook pending lock poisoned")?;
        if ticket.question().is_none() || pending.len() >= HookLimits::default().max_fanout {
            drop(pending);
            self.cancel_queued_effect(
                ticket.binding.clone(),
                ticket.effect().clone(),
                "hook cannot retain a denied or excess approval proposal".into(),
            )?;
            return Ok(());
        }
        pending.insert(ticket.plan.identity.clone(), ticket);
        Ok(())
    }

    fn prune_pending_hooks(&self) -> Result<(), String> {
        let mut pending = self
            .hook_pending
            .lock()
            .map_err(|_| "hook pending lock poisoned")?;
        let mut stale = Vec::new();
        for (identity, ticket) in pending.iter() {
            let binding = ticket.binding.clone();
            let prepared = ticket.process.prepared.clone();
            if !self
                .worker
                .run(move |context| context.hook_pending_current(&binding, &prepared))?
            {
                stale.push(identity.clone());
            }
        }
        for identity in stale {
            if let Some(ticket) = pending.remove(&identity) {
                self.cancel_queued_effect(
                    ticket.binding.clone(),
                    ticket.effect().clone(),
                    "pending hook authority or steering was superseded".into(),
                )?;
            }
        }
        Ok(())
    }

    fn hook_rewrite(
        &self,
        thread: ThreadId,
        outcome: &HookOutcome,
        original: &str,
    ) -> Result<ArgumentRewrite, String> {
        if outcome.receipt.status != HookStatus::Validated {
            return Err("hook did not validate".into());
        }
        let recovery = self
            .inspect_hook(thread, outcome.receipt.identity.clone())?
            .ok_or("hook receipt missing")?;
        if recovery.receipt.as_ref() != Some(&outcome.receipt) {
            return Err("hook receipt differs from canonical output".into());
        }
        let plan: PlannedHook =
            serde_json::from_value(recovery.plan["plan"].clone()).map_err(|e| e.to_string())?;
        let binding = self.binding(thread)?;
        self.worker
            .run(move |context| context.check_hook_result(&binding, &plan))?;
        let rewrite = outcome
            .receipt
            .output
            .as_ref()
            .and_then(|o| o.rewrite.clone())
            .ok_or("hook has no rewrite")?;
        if rewrite.original_digest != original {
            return Err("rewrite belongs to a different operation".into());
        }
        Ok(rewrite)
    }

    /// Consumes and cancels the original ticket. Approval never transfers.
    pub fn rewrite_hook_process(
        &self,
        thread: ThreadId,
        original: ProcessProposal,
        outcome: &HookOutcome,
    ) -> Result<ProcessProposal, String> {
        let request = (|| {
            let rewrite = self.hook_rewrite(thread, outcome, original.digest())?;
            if rewrite.tool != "vcp_exec" {
                return Err("hook process rewrite changed tool kind".into());
            }
            vcp_tools::process::Request::from_arguments(&rewrite.arguments.to_string())
                .map_err(|e| e.to_string())
        })();
        self.cancel_queued_effect(
            self.binding(thread)?,
            original.effect().clone(),
            "superseded by hook; fresh authorization required".into(),
        )?;
        self.prepare_process(thread, request?)
    }

    pub fn rewrite_hook_tool(
        &self,
        thread: ThreadId,
        original: ToolProposal,
        outcome: &HookOutcome,
    ) -> Result<ToolProposal, String> {
        let request = self
            .hook_rewrite(thread, outcome, original.digest())
            .and_then(|rewrite| {
                vcp_tools::Request::from_call(&rewrite.tool, &rewrite.arguments.to_string())
                    .map_err(|e| e.to_string())
            });
        self.cancel_queued_effect(
            self.binding(thread)?,
            original.effect().clone(),
            "superseded by hook; fresh authorization required".into(),
        )?;
        self.prepare_tool(thread, request?)
    }
}
