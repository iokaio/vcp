// SPDX-License-Identifier: Apache-2.0
//! Native file broker. Tickets cannot be deserialized or recreated from model
//! content. One canonical worker owns policy, dispatch intents and receipts.
use super::*;
use vcp_domain::effect::EffectState;
pub struct ToolProposal {
    thread: ThreadId,
    binding: ThreadBinding,
    prepared: Arc<vcp_tools::Prepared>,
    controller: ControllerId,
    owner: OwnerEpoch,
    effect: ToolRunId,
    plan: ArtifactId,
    generation: u64,
    pub decision: vcp_policy::Decision,
    pub question: Option<ApprovalId>,
}
impl ToolProposal {
    pub fn effect(&self) -> &ToolRunId {
        &self.effect
    }
    pub fn digest(&self) -> &str {
        self.prepared.authority().digest()
    }
}
pub struct ToolOutcome {
    pub effect: ToolRunId,
    pub result: serde_json::Value,
    pub evidence: ArtifactDescriptor,
}
impl CanonicalHost {
    pub fn prepare_tool(
        &self,
        thread: ThreadId,
        request: vcp_tools::Request,
    ) -> Result<ToolProposal, String> {
        let generation = scheduler::generation(&self.runtime, thread)?;
        let binding = self.binding(thread)?;
        let scoped = binding.clone();
        let (prepared, controller, owner) = self.worker.run(move |context| {
            context.child_tool_request(&scoped, &request)?;
            let identity = context.tool_identity(&scoped, request.tool())?;
            let prepared = vcp_tools::prepare(
                context.task_root(&scoped.scope.task)?,
                identity,
                request,
                ByteCount::new(1024 * 1024),
            )?;
            Ok((
                Arc::new(prepared),
                context.engine.controller().clone(),
                context.engine.owner_epoch(),
            ))
        })?;
        let scoped = binding.clone();
        let proposed = prepared.clone();
        let (effect, plan, decision, question) = self
            .worker
            .run(move |context| context.tool_propose(&scoped, &proposed))?;
        Ok(ToolProposal {
            thread,
            binding,
            prepared,
            controller,
            owner,
            effect,
            plan,
            generation,
            decision,
            question,
        })
    }
    /// Owner-only prepared edits enter the same policy, intent and receipt path
    /// as ordinary native tools. The caller supplies no execution authority.
    pub(in crate::foundation) fn propose_prepared_tool(
        &self,
        thread: ThreadId,
        prepared: Arc<vcp_tools::Prepared>,
    ) -> Result<ToolProposal, String> {
        let generation = scheduler::generation(&self.runtime, thread)?;
        let binding = self.binding(thread)?;
        if prepared.authority().operation().scope != binding.scope {
            return Err("prepared edit belongs to a different task".into());
        }
        let scoped = binding.clone();
        let proposed = prepared.clone();
        let (controller, owner, effect, plan, decision, question) =
            self.worker.run(move |context| {
                let (effect, plan, decision, question) =
                    context.tool_propose(&scoped, &proposed)?;
                Ok((
                    context.engine.controller().clone(),
                    context.engine.owner_epoch(),
                    effect,
                    plan,
                    decision,
                    question,
                ))
            })?;
        Ok(ToolProposal {
            thread,
            binding,
            prepared,
            controller,
            owner,
            effect,
            plan,
            generation,
            decision,
            question,
        })
    }
    /// Consumes one owner-bound ticket. Every file gets a fresh native version
    /// check and current authority check, then a durable intent before mutation.
    pub fn dispatch_tool(&self, ticket: ToolProposal) -> Result<ToolOutcome, String> {
        if self.mcp_connections_present() {
            return Err("disconnect MCP processes before dispatching native tools".into());
        }
        let lease = self
            .scheduler
            .try_acquire(ticket.prepared.authority().operation())?;
        self.dispatch_tool_leased(ticket, lease)
    }
    /// Wait without blocking the retained reactor. Queued work is fenced by
    /// owner generation; native identity and authority are rechecked at dispatch.
    pub async fn schedule_tool(&self, ticket: ToolProposal) -> Result<ToolOutcome, String> {
        if self.mcp_connections_present() {
            return Err("disconnect MCP processes before scheduling native tools".into());
        }
        let mut queued = scheduler::QueuedEffect::new(self, &ticket.binding, &ticket.effect);
        let scheduled = self
            .schedule(
                ticket.prepared.authority().operation(),
                ticket.thread,
                ticket.generation,
            )
            .await;
        let lease = match scheduled {
            Ok(lease) => lease,
            Err(error) => {
                self.cancel_queued_effect(ticket.binding, ticket.effect, error.clone())?;
                return Err(error);
            }
        };
        let host = self.clone();
        tokio::task::spawn_blocking(move || {
            let result = host.dispatch_tool_leased(ticket, lease);
            if result.is_ok() {
                queued.dispatched();
            }
            result
        })
        .await
        .map_err(|error| format!("scheduled tool worker failed: {error}"))?
    }
    fn dispatch_tool_leased(
        &self,
        ticket: ToolProposal,
        _lease: EffectLease,
    ) -> Result<ToolOutcome, String> {
        scheduler::check_generation(&self.runtime, ticket.thread, ticket.generation)?;
        let mut permit = HostWorkAdmission::admit(
            &self.runtime,
            ticket.thread,
            HostWorkKind::Tool,
            "prepared-native-file",
        )?;
        let prepared = ticket.prepared.clone();
        let binding = ticket.binding.clone();
        let effect = ticket.effect.clone();
        let plan = ticket.plan.clone();
        let controller = ticket.controller.clone();
        let owner = ticket.owner;
        let execution = ExecutionId::new();
        let run = execution.clone();
        let admitted = self.worker.run(move |context| {
            if context.engine.controller() != &controller || context.engine.owner_epoch() != owner {
                return Err("prepared ticket belongs to another owner".into());
            }
            if !matches!(
                context.tool_preflight(&binding, &prepared)?,
                vcp_policy::Decision::Allow { .. }
            ) {
                return Err("current tool authority rejected before native revalidation".into());
            }
            prepared.revalidate()?;
            match context.tool_decision(&binding, &prepared)? {
                vcp_policy::Decision::Allow { origin, .. } => {
                    context.tool_advance(
                        &binding,
                        &effect,
                        EffectState::Authorized,
                        None,
                        vec![plan.clone()],
                        &origin,
                    )?;
                    context.tool_advance(
                        &binding,
                        &effect,
                        EffectState::DispatchRecorded,
                        Some(run.clone()),
                        vec![plan.clone()],
                        "broker admitted one immutable prepared execution",
                    )?;
                    context.tool_advance(
                        &binding,
                        &effect,
                        EffectState::Running,
                        Some(run),
                        vec![plan],
                        "broker started native file execution",
                    )?;
                    Ok(())
                }
                other => Err(format!("tool authority rejected: {other:?}").into()),
            }
        });
        if let Err(error) = admitted {
            let binding = ticket.binding.clone();
            let effect = ticket.effect.clone();
            let reason = error.clone();
            let plan = ticket.plan.clone();
            let reconciled = self.worker.run_cleanup(move |context| {
                use vcp_store::contract::Collection;
                let row: vcp_domain::effect::Effect = context
                    .engine
                    .store()
                    .state()
                    .record(
                        Collection::Effect,
                        effect.as_str(),
                        &binding.scope.workspace,
                    )?
                    .decode()?;
                let next = match row.state {
                    EffectState::Proposed | EffectState::Validated | EffectState::Authorized => {
                        Some(EffectState::Cancelled)
                    }
                    EffectState::DispatchRecorded | EffectState::Running => {
                        Some(EffectState::OutcomeUnknown)
                    }
                    _ => None,
                };
                if let Some(next) = next {
                    context.tool_advance(
                        &binding,
                        &effect,
                        next,
                        row.execution,
                        vec![plan],
                        &reason,
                    )?;
                }
                Ok(())
            });
            if reconciled.is_err() {
                self.worker.fence();
            }
            permit.complete()?;
            return Err(error);
        }
        let mut receipts = vec![ticket.plan.clone()];
        let mut observations = vec![];
        let mut failure = None;
        let mut unknown = false;
        let started = std::time::Instant::now();
        for change in ticket.prepared.changes() {
            if started.elapsed()
                > Duration::from_millis(ticket.prepared.authority().operation().timeout_ms.get())
            {
                failure = Some("prepared operation deadline elapsed".to_owned());
                break;
            }
            let prepared = ticket.prepared.clone();
            let binding = ticket.binding.clone();
            let change = change.clone();
            let runtime = self.runtime.clone();
            let thread = ticket.thread;
            let generation = ticket.generation;
            let effect = ticket.effect.clone();
            let execution = execution.clone();
            let outcome=self.worker.run(move|context|{
                let state=runtime.0.state.lock().map_err(|_|"poisoned lifecycle")?;
                if !state.attached || state.held(thread) || !state.admission_current(thread, generation) {return Err("file dispatch is paused, superseded or unowned".into());}
                if !matches!(context.tool_decision(&binding,&prepared)?,vcp_policy::Decision::Allow{..}) {return Err("tool authority changed before file dispatch".into());}
                let _index = prepared.hold_index()?;
                // All paths were validated before the first write. Repeat the
                // current file's identity check under deny-write/delete sharing.
                let target=prepared.root().mutation_target(&change.probes[0])?;
                let intent=context.capture(&binding.scope,Channel::Evidence,&vcp_protocol::canonical_bytes(&serde_json::json!({"schema_version":1,"kind":"file_dispatch","effect":effect,"execution":execution,"change":change}))?,"vcp-file-intent-v1")?;
                let observation=target.apply(change.after.as_deref(),change.rename_to.as_deref());
                drop(state);
                let receipt=context.capture(&binding.scope,Channel::Evidence,&vcp_protocol::canonical_bytes(&serde_json::json!({"schema_version":1,"effect":effect,"execution":execution,"observation":observation}))?,"vcp-file-outcome-v1")?;
                Ok((observation,intent.spec.id,receipt.spec.id))
            });
            match outcome {
                Ok((observation, intent, receipt)) => {
                    receipts.extend([intent, receipt]);
                    if !observation.complete {
                        unknown = observation.changed || observation.staging_path.is_some();
                        failure = observation.error.clone();
                    }
                    observations.push(observation);
                    if failure.is_some() {
                        break;
                    }
                }
                Err(error) => {
                    failure = Some(error);
                    unknown = true;
                    break;
                }
            }
        }
        let prepared = ticket.prepared.clone();
        let binding = ticket.binding.clone();
        let effect = ticket.effect.clone();
        let result = if prepared.changes().is_empty() {
            prepared.proposed_result().clone()
        } else {
            serde_json::json!({"complete":failure.is_none(),"files":observations,"error":failure})
        };
        let result_copy = result.clone();
        let finished = self.worker.run_cleanup(move |context| {
            // Read results are released only if the observed inputs and current
            // authority still match after preparation/admission.
            if prepared.changes().is_empty() {
                prepared.revalidate()?;
                if !matches!(
                    context.tool_decision(&binding, &prepared)?,
                    vcp_policy::Decision::Allow { .. }
                ) {
                    return Err("read authority changed".into());
                }
            }
            let output = context.capture(
                &binding.scope,
                Channel::Evidence,
                &vcp_protocol::canonical_bytes(&result_copy)?,
                "vcp-tool-result-v1",
            )?;
            receipts.push(output.spec.id.clone());
            let next = if unknown {
                EffectState::OutcomeUnknown
            } else if failure.is_some() {
                EffectState::Failed
            } else {
                EffectState::Succeeded
            };
            context.tool_advance(
                &binding,
                &effect,
                next,
                Some(execution),
                receipts,
                "broker observed bounded file results; no automatic replay",
            )?;
            Ok(output)
        });
        let evidence = match finished {
            Ok(output) => output,
            Err(error) => {
                self.worker.fence();
                return Err(error);
            }
        };
        permit.complete()?;
        Ok(ToolOutcome {
            effect: ticket.effect,
            result,
            evidence,
        })
    }
}
