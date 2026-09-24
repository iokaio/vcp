// SPDX-License-Identifier: Apache-2.0
//! Trusted lifecycle registration and the retained native-tool integration.
use super::*;
use serde_json::{json, Value};

impl CanonicalHost {
    async fn publish_lifecycle_outcomes(
        &self,
        thread: ThreadId,
        outcomes: Vec<HookOutcome>,
    ) -> Result<(), String> {
        if outcomes.is_empty() {
            return Ok(());
        }
        gate_decision(&outcomes)?;
        let binding = self.binding(thread)?;
        self.worker
            .run(move |context| context.publish_hook_context(&binding, outcomes))
    }
    /// Owner-triggered session/task admission before the first retained turn.
    /// Stable event identities make explicit resume inspect prior receipts.
    pub async fn start_lifecycle_hooks(&self, thread: ThreadId) -> Result<(), String> {
        let binding = self.binding(thread)?;
        for (event, id) in [
            (
                HookEvent::SessionStart,
                format!("session-{}", binding.scope.session),
            ),
            (HookEvent::TaskStart, format!("task-{}", binding.scope.task)),
        ] {
            let outcomes = self
                .gate_event(thread, event, id.clone(), id, 0, vec![], json!({}))
                .await?;
            self.publish_lifecycle_outcomes(thread, outcomes).await?;
        }
        Ok(())
    }

    /// Completion gate runs while the task still permits dispatch. Canonical
    /// completion subsequently revalidates verification and quiescence.
    pub async fn complete_lifecycle_hooks(
        &self,
        thread: ThreadId,
    ) -> Result<Vec<HookOutcome>, String> {
        let binding = self.binding(thread)?;
        let id = format!("complete-{}", binding.scope.task);
        let outcomes = self
            .gate_event(
                thread,
                HookEvent::TaskCompletion,
                id.clone(),
                id,
                0,
                vec![],
                json!({}),
            )
            .await?;
        gate_decision(&outcomes)?;
        Ok(outcomes)
    }

    /// Await executable gates outside the store worker, before canonical context
    /// assembly and its pure portable-compaction projection.
    pub(crate) async fn model_lifecycle_hooks(&self, thread: ThreadId) -> Result<(), String> {
        if !self.has_model_hooks(thread)? {
            return Ok(());
        }
        let binding = self.binding(thread)?;
        let scoped = binding.clone();
        let Some((identity, compact)) = self
            .worker
            .run(move |context| context.coding_hook_boundary(&scoped))?
        else {
            return Ok(());
        };
        let mut all_outcomes = Vec::new();
        for event in [
            HookEvent::BeforeContextAssembly,
            HookEvent::BeforeCompaction,
        ] {
            if event == HookEvent::BeforeCompaction && !compact {
                continue;
            }
            let id = format!("context-{identity}");
            let outcomes = self
                .gate_event(
                    thread,
                    event,
                    id.clone(),
                    id,
                    0,
                    vec![],
                    json!({"context_identity":identity}),
                )
                .await?;
            all_outcomes.extend(outcomes.clone());
            self.publish_lifecycle_outcomes(thread, outcomes).await?;
        }
        self.worker.run(move |context| {
            context.arm_coding_hook_gate(&binding, identity, compact, all_outcomes)
        })
    }
    pub(crate) fn has_model_hooks(&self, thread: ThreadId) -> Result<bool, String> {
        Ok(self
            .hook_registry
            .lock()
            .map_err(|_| "hook registry lock poisoned")?
            .get(&thread)
            .is_some_and(|definitions| {
                definitions.iter().any(|definition| {
                    matches!(
                        definition.event,
                        HookEvent::BeforeContextAssembly | HookEvent::BeforeCompaction
                    )
                })
            }))
    }
    pub(crate) fn has_event_hooks(
        &self,
        thread: ThreadId,
        event: HookEvent,
    ) -> Result<bool, String> {
        Ok(self
            .hook_registry
            .lock()
            .map_err(|_| "hook registry lock poisoned")?
            .get(&thread)
            .is_some_and(|definitions| definitions.iter().any(|d| d.event == event)))
    }
    pub(crate) fn has_tool_hooks(&self, thread: ThreadId) -> Result<bool, String> {
        self.has_event_hooks(thread, HookEvent::BeforeToolAuthorization)
    }
    /// Prepare an identity without proposing authority, run the gate, then ask
    /// policy exactly once for the final operation (including any rewrite).
    pub async fn prepare_gated_process(
        &self,
        thread: ThreadId,
        request: vcp_tools::process::Request,
        event_id: String,
    ) -> Result<(ProcessProposal, Vec<HookOutcome>), String> {
        if !self.has_tool_hooks(thread)? {
            return Ok((self.prepare_process(thread, request)?, vec![]));
        }
        let binding = self.binding(thread)?;
        let initial = request.clone();
        let digest = self.worker.run(move |context| {
            Ok(context
                .prepare_process(&binding, initial)?
                .authority()
                .digest()
                .to_owned())
        })?;
        let outcomes = self
            .gate_event(
                thread,
                HookEvent::BeforeToolAuthorization,
                event_id.clone(),
                event_id,
                0,
                vec![],
                json!({"tool":"vcp_exec","arguments":request,"operation_digest":digest}),
            )
            .await?;
        gate_decision(&outcomes)?;
        let final_request = match outcomes
            .iter()
            .find_map(|o| o.receipt.output.as_ref().and_then(|o| o.rewrite.as_ref()))
        {
            Some(rewrite) => {
                if rewrite.tool != "vcp_exec" || rewrite.original_digest != digest {
                    return Err("hook rewrite changed original identity or process kind".into());
                }
                vcp_tools::process::Request::from_arguments(&rewrite.arguments.to_string())
                    .map_err(|e| e.to_string())?
            }
            None => request,
        };
        let proposal = self.prepare_process_with_hooks(thread, final_request, outcomes.clone())?;
        let rewritten = outcomes.iter().any(|o| {
            o.receipt
                .output
                .as_ref()
                .is_some_and(|o| o.rewrite.is_some())
        });
        if !outcomes.is_empty() && !rewritten && proposal.digest() != digest {
            self.cancel_queued_effect(
                self.binding(thread)?,
                proposal.effect().clone(),
                "hook input resources changed before final preparation".into(),
            )?;
            return Err(
                "operation identity changed after hook validation; requires refreshed hook input"
                    .into(),
            );
        }
        Ok((proposal, outcomes))
    }

    pub async fn prepare_gated_tool(
        &self,
        thread: ThreadId,
        request: vcp_tools::Request,
        arguments: Value,
        event_id: String,
    ) -> Result<(ToolProposal, Vec<HookOutcome>), String> {
        if !self.has_tool_hooks(thread)? {
            return Ok((self.prepare_tool(thread, request)?, vec![]));
        }
        let binding = self.binding(thread)?;
        let initial = request.clone();
        let tool = request.tool().to_owned();
        let digest = self.worker.run(move |context| {
            context.child_tool_request(&binding, &initial)?;
            let identity = context.tool_identity(&binding, initial.tool())?;
            let prepared = vcp_tools::prepare(
                context.task_root(&binding.scope.task)?,
                identity,
                initial,
                ByteCount::new(1024 * 1024),
            )?;
            Ok(prepared.authority().digest().to_owned())
        })?;
        let outcomes = self
            .gate_event(
                thread,
                HookEvent::BeforeToolAuthorization,
                event_id.clone(),
                event_id,
                0,
                vec![],
                json!({"tool":tool,"arguments":arguments,"operation_digest":digest}),
            )
            .await?;
        gate_decision(&outcomes)?;
        let final_request = match outcomes
            .iter()
            .find_map(|o| o.receipt.output.as_ref().and_then(|o| o.rewrite.as_ref()))
        {
            Some(rewrite) => {
                if rewrite.original_digest != digest {
                    return Err("hook rewrite changed original identity".into());
                }
                vcp_tools::Request::from_call(&rewrite.tool, &rewrite.arguments.to_string())
                    .map_err(|e| e.to_string())?
            }
            None => request,
        };
        let proposal = self.prepare_tool_with_hooks(thread, final_request, outcomes.clone())?;
        let rewritten = outcomes.iter().any(|o| {
            o.receipt
                .output
                .as_ref()
                .is_some_and(|o| o.rewrite.is_some())
        });
        if !outcomes.is_empty() && !rewritten && proposal.digest() != digest {
            self.cancel_queued_effect(
                self.binding(thread)?,
                proposal.effect().clone(),
                "hook input resources changed before final preparation".into(),
            )?;
            return Err(
                "operation identity changed after hook validation; requires refreshed hook input"
                    .into(),
            );
        }
        Ok((proposal, outcomes))
    }

    /// Owner setup only. Registrations cannot be supplied by a model tool or
    /// silently replaced midway through a live thread.
    pub fn configure_hooks(
        &self,
        thread: ThreadId,
        definitions: Vec<HookDefinition>,
    ) -> Result<(), String> {
        for event in [
            HookEvent::SessionStart,
            HookEvent::TaskStart,
            HookEvent::BeforeContextAssembly,
            HookEvent::BeforeToolAuthorization,
            HookEvent::AfterToolCompletion,
            HookEvent::BeforeCompaction,
            HookEvent::AfterVerification,
            HookEvent::TaskCompletion,
        ] {
            let input = self.hook_input(
                thread,
                event,
                "registry-validation".into(),
                "owner-configuration".into(),
                0,
                vec![],
                json!({}),
            )?;
            planner::plan(&definitions, &input, HookLimits::default())
                .map_err(|e| e.to_string())?;
        }
        let mut registry = self
            .hook_registry
            .lock()
            .map_err(|_| "hook registry lock poisoned")?;
        if registry.contains_key(&thread) {
            return Err("hooks already configured for this thread".into());
        }
        registry.insert(thread, definitions);
        drop(registry);
        let required = self.has_model_hooks(thread)?;
        let binding = self.binding(thread)?;
        self.worker
            .run(move |context| context.require_coding_hook_gate(&binding, required))
    }

    /// Async owner adapter for all eight reserved lifecycle points. A blocked
    /// result must prevent the affected action; the tool gates below enforce it.
    pub async fn gate_event(
        &self,
        thread: ThreadId,
        event: HookEvent,
        event_id: String,
        causation_id: String,
        depth: u32,
        artifacts: Vec<ArtifactId>,
        payload: Value,
    ) -> Result<Vec<HookOutcome>, String> {
        let definitions = self
            .hook_registry
            .lock()
            .map_err(|_| "hook registry lock poisoned")?
            .get(&thread)
            .cloned()
            .unwrap_or_default();
        if !definitions.iter().any(|d| d.event == event) {
            return Ok(vec![]);
        }
        let input = self.hook_input(
            thread,
            event,
            event_id,
            causation_id,
            depth,
            artifacts,
            payload,
        )?;
        self.run_hooks(thread, &definitions, input).await
    }
}

fn gate_decision(outcomes: &[HookOutcome]) -> Result<(), String> {
    if let Some(blocked) = outcomes
        .iter()
        .find(|o| o.receipt.status == HookStatus::Blocked)
    {
        return Err(format!(
            "hook blocked operation; receipt={}; reason={:?}",
            blocked.artifact, blocked.receipt.reason
        ));
    }
    if outcomes
        .iter()
        .filter(|o| {
            o.receipt
                .output
                .as_ref()
                .is_some_and(|o| o.rewrite.is_some())
        })
        .count()
        > 1
    {
        return Err(
            "multiple rewrites of one immutable hook input are ambiguous; operation blocked".into(),
        );
    }
    Ok(())
}

/// Attributed, receipt-linked untrusted proposals; repeated delivery does not
/// republish context or findings. No output is promoted to operating instructions.
pub fn presentation(outcomes: &[HookOutcome]) -> Value {
    Value::Array(outcomes.iter().map(|outcome| json!({
        "hook_identity":outcome.receipt.identity,"receipt":outcome.artifact,
        "status":outcome.receipt.status,"reason":outcome.receipt.reason,
        "duplicate":outcome.duplicate,"trust":"attributed hook proposal; not authority",
        "proposal": if !outcome.duplicate && outcome.receipt.status == HookStatus::Validated {
            outcome.receipt.output.as_ref().map(|output| json!({"findings":output.findings,"context":output.context}))
        } else {None}
    })).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vcp_extensions::hooks::result::{ArgumentRewrite, ContextProposal, HookOutput};

    fn outcome() -> HookOutcome {
        HookOutcome {
            receipt: HookReceipt {
                schema_version: 1,
                identity: "a".repeat(64),
                effect: ToolRunId::new(),
                status: HookStatus::Validated,
                output: Some(HookOutput {
                    schema_version: 1,
                    findings: vec!["finding".into()],
                    context: Some(ContextProposal {
                        text: "untrusted context".into(),
                        artifact_refs: vec![],
                    }),
                    rewrite: None,
                    block: false,
                }),
                reason: None,
                evidence: vec![],
            },
            artifact: ArtifactId::new(),
            duplicate: false,
        }
    }
    #[test]
    fn repeated_or_blocked_results_never_republish_proposals() {
        let mut item = outcome();
        assert_eq!(
            presentation(&[item.clone()])[0]["proposal"]["context"]["text"],
            "untrusted context"
        );
        item.duplicate = true;
        assert!(presentation(&[item.clone()])[0]["proposal"].is_null());
        item.duplicate = false;
        item.receipt.status = HookStatus::Blocked;
        assert!(presentation(&[item.clone()])[0]["proposal"].is_null());
        assert!(gate_decision(&[item]).is_err());
    }
    #[test]
    fn conflicting_rewrites_block_before_any_rewrite_is_applied() {
        let mut item = outcome();
        item.receipt.output.as_mut().unwrap().rewrite = Some(ArgumentRewrite {
            original_digest: "b".repeat(64),
            tool: "vcp_exec".into(),
            arguments: json!({}),
        });
        assert!(gate_decision(&[item.clone()]).is_ok());
        assert!(gate_decision(&[item.clone(), item]).is_err());
    }
}
