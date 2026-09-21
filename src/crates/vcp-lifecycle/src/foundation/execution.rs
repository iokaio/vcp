// SPDX-License-Identifier: Apache-2.0
//! Native process capabilities remain owner-bound, one-use and non-serializable.
use super::*;
use std::{collections::BTreeMap, ffi::OsString};
use vcp_domain::effect::{Effect, EffectState};
use vcp_store::contract::Collection;
use vcp_tools::process::{Pins, Prepared, Profile, Request};
mod duplex;
pub use duplex::{DuplexIoLimits, DuplexProcess};

fn fence_process_owner(runtime: &Lifecycle) {
    if runtime.hold_owner().is_err() {
        // A subtree may already be draining, making another coordinated hold
        // Busy without holding its siblings. Unknown native ownership must still
        // prevent every new launch before the resource claim is released.
        let root_can_admit =
            runtime.0.state.lock().is_ok_and(|state| {
                state.attached && state.root.is_some_and(|root| !state.held(root))
            });
        if root_can_admit {
            let _ = runtime.lose_owner();
        }
    }
}

/// A launched process is not safely observed until its identity and Running
/// state are durable. Errors (including unwinding) fence queued dispatch before
/// the caller can release its resource claim.
pub(super) fn observe_process_start<T>(
    runtime: &Lifecycle,
    operation: impl FnOnce() -> Result<T, Box<dyn std::error::Error + Send + Sync>>,
) -> Result<T, Box<dyn std::error::Error + Send + Sync>> {
    struct Fence<'a> {
        runtime: &'a Lifecycle,
        observed: bool,
    }
    impl Drop for Fence<'_> {
        fn drop(&mut self) {
            if !self.observed {
                fence_process_owner(self.runtime);
            }
        }
    }
    let mut fence = Fence {
        runtime,
        observed: false,
    };
    let result = operation()?;
    fence.observed = true;
    Ok(result)
}
pub struct ProcessProposal {
    thread: ThreadId,
    binding: ThreadBinding,
    pub(super) prepared: Arc<Prepared>,
    controller: ControllerId,
    owner: OwnerEpoch,
    effect: ToolRunId,
    plan: ArtifactId,
    generation: u64,
    pub decision: vcp_policy::Decision,
    pub question: Option<ApprovalId>,
}
impl ProcessProposal {
    pub fn effect(&self) -> &ToolRunId {
        &self.effect
    }
    pub fn digest(&self) -> &str {
        self.prepared.authority().digest()
    }
}
pub struct PreparedProcess {
    host: ProcessHost,
    binding: ThreadBinding,
    effect: ToolRunId,
    execution: ExecutionId,
    plan: ArtifactId,
    process: Option<crate::process::Process>,
    stdout: Option<Arc<OutputCapture>>,
    stderr: Option<Arc<OutputCapture>>,
    prepared: Arc<Prepared>,
    _pins: Arc<Pins>,
    _conflict: EffectLease,
    finished: bool,
}
// Keep process guards independent of managed connection maps owned by the host.
// A full host clone here would form a cycle once a map owns its duplex process.
struct ProcessHost {
    runtime: Lifecycle,
    worker: worker::Worker,
}
impl From<&CanonicalHost> for ProcessHost {
    fn from(host: &CanonicalHost) -> Self {
        Self {
            runtime: host.runtime.clone(),
            worker: host.worker.clone(),
        }
    }
}
pub struct PreparedProcessOutcome {
    pub effect: ToolRunId,
    pub exit_code: Option<i32>,
    pub stdout: ArtifactDescriptor,
    pub stderr: ArtifactDescriptor,
    pub stdout_presentation: vcp_tools::process::output::Decoded,
    pub stderr_presentation: vcp_tools::process::output::Decoded,
    pub stdout_tail: Vec<u8>,
    pub stderr_tail: Vec<u8>,
    pub reason: Option<String>,
    pub evidence: ArtifactDescriptor,
}
impl CanonicalHost {
    /// Trusted configuration only. Profiles are deliberately absent after
    /// reopening; portable history cannot recreate native execution authority.
    pub fn configure_process_profile(&self, profile: Profile) -> Result<(), String> {
        self.worker
            .run(move |context| context.configure_process(profile))
    }
    pub fn prepare_process(
        &self,
        thread: ThreadId,
        request: Request,
    ) -> Result<ProcessProposal, String> {
        let generation = scheduler::generation(&self.runtime, thread)?;
        let binding = self.binding(thread)?;
        let scoped = binding.clone();
        let (prepared, controller, owner, effect, plan, decision, question) =
            self.worker.run(move |context| {
                let prepared = context.prepare_process(&scoped, request)?;
                let mut decision = context.process_decision(&scoped, &prepared)?;
                let _pins = if matches!(decision, vcp_policy::Decision::Deny { .. }) {
                    None
                } else {
                    let pins = prepared.pin()?;
                    decision = context.process_decision(&scoped, &prepared)?;
                    Some(pins)
                };
                let (effect, plan, decision, question) = context.propose_authority(
                    &scoped,
                    prepared.authority(),
                    &prepared.evidence()?,
                    decision,
                )?;
                Ok((
                    Arc::new(prepared),
                    context.engine.controller().clone(),
                    context.engine.owner_epoch(),
                    effect,
                    plan,
                    decision,
                    question,
                ))
            })?;
        Ok(ProcessProposal {
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
    pub fn dispatch_process(&self, ticket: ProcessProposal) -> Result<PreparedProcess, String> {
        if self.mcp_connections_present() {
            return Err("disconnect MCP processes before dispatching native processes".into());
        }
        let conflict = self
            .scheduler
            .try_acquire(ticket.prepared.authority().operation())?;
        self.dispatch_process_leased(ticket, conflict)
    }
    pub async fn schedule_process(
        &self,
        ticket: ProcessProposal,
    ) -> Result<PreparedProcess, String> {
        if self.mcp_connections_present() {
            return Err("disconnect MCP processes before scheduling native processes".into());
        }
        let mut queued = scheduler::QueuedEffect::new(self, &ticket.binding, &ticket.effect);
        let scheduled = self
            .schedule(
                ticket.prepared.authority().operation(),
                ticket.thread,
                ticket.generation,
            )
            .await;
        let conflict = match scheduled {
            Ok(conflict) => conflict,
            Err(error) => {
                self.cancel_queued_effect(ticket.binding, ticket.effect, error.clone())?;
                return Err(error);
            }
        };
        let result = self.dispatch_process_leased(ticket, conflict);
        if result.is_ok() {
            queued.dispatched();
        }
        result
    }
    fn dispatch_process_leased(
        &self,
        ticket: ProcessProposal,
        conflict: EffectLease,
    ) -> Result<PreparedProcess, String> {
        scheduler::check_generation(&self.runtime, ticket.thread, ticket.generation)?;
        let stdout = Arc::new(self.open_output(ticket.thread, Channel::Stdout)?);
        let stderr = Arc::new(self.open_output(ticket.thread, Channel::Stderr)?);
        let (out, err) = (stdout.clone(), stderr.clone());
        let binding = ticket.binding.clone();
        let prepared = ticket.prepared.clone();
        let effect = ticket.effect.clone();
        let plan = ticket.plan.clone();
        let controller = ticket.controller.clone();
        let owner = ticket.owner;
        let execution = ExecutionId::new();
        let run = execution.clone();
        let runtime = self.runtime.clone();
        let thread = ticket.thread;
        let generation = ticket.generation;
        // Launch on the caller's live reactor, never the store worker's
        // current-thread runtime (which is not an execution scheduler).
        let reactor =
            tokio::runtime::Handle::try_current().map_err(|_| "live process runtime required")?;
        let started=self.worker.run(move|context|{
            if context.engine.controller()!=&controller || context.engine.owner_epoch()!=owner{return Err("prepared process belongs to another owner".into());}
            if !matches!(context.process_preflight(&binding,&prepared)?,vcp_policy::Decision::Allow{..}){return Err("current process authority rejected before native revalidation".into());}
            let pins=Arc::new(prepared.pin()?);
            if !matches!(context.process_decision(&binding,&prepared)?,vcp_policy::Decision::Allow{..}){return Err("current process authority rejected".into());}
            context.tool_advance(&binding,&effect,EffectState::Authorized,None,vec![plan.clone()],"current process policy and native identities accepted")?;
            context.tool_advance(&binding,&effect,EffectState::DispatchRecorded,Some(run.clone()),vec![plan.clone()],"one owned native process dispatch recorded")?;
            observe_process_start(&runtime, || {
            let _entered=reactor.enter();
            let arguments:Vec<OsString>=prepared.arguments().iter().map(OsString::from).collect();
            let environment:BTreeMap<OsString,OsString>=prepared.environment().iter().map(|(k,v)|(k.into(),v.into())).collect();
            let op=prepared.authority().operation();
            let limits=crate::process::Limits{timeout:Duration::from_millis(op.timeout_ms.get()),output_bytes:op.output_bytes.get(),process_count:prepared.profile().process_count()};
            let process=if let Some(terminal)=prepared.profile().terminal() {
                runtime.spawn_pty_with_capture_generation(thread,&prepared.executable(),&arguments,&prepared.directory(),&environment,
                    codex_utils_pty::TerminalSize{rows:terminal.rows,cols:terminal.cols},prepared.input().map(str::to_owned),limits,
                    Arc::new(move|bytes|out.write(bytes).map_err(std::io::Error::other)),pins.clone(),Some(generation))?
            } else { runtime.spawn_bounded_process_with_capture_generation(thread,&prepared.executable(),&arguments,&prepared.directory(),&environment,64*1024,
                Some(Arc::new(move|bytes|out.write(bytes).map_err(std::io::Error::other))),
                Some(Arc::new(move|bytes|err.write(bytes).map_err(std::io::Error::other))),
                Some(limits),prepared.profile().mode()==vcp_tools::process::Mode::Cmd,Some(pins.clone()),Some(generation))? };
            let identity=context.capture(&binding.scope,Channel::Evidence,&vcp_protocol::canonical_bytes(&serde_json::json!({"execution":run,"process_id":process.id(),"process_identity":super::worker::recovery::process_identity(process.id()),"identity_authority":"owned process/job handles; PID is diagnostic only","job_processes":process.active_process_count()?}))?,"vcp-process-start-v1")?;
            context.tool_advance(&binding,&effect,EffectState::Running,Some(run),vec![plan,identity.spec.id],"native process launched with owned job membership before execution")?;
            Ok((process,pins))
            })
        });
        match started {
            Ok((process, pins)) => Ok(PreparedProcess {
                host: ProcessHost::from(self),
                binding: ticket.binding,
                effect: ticket.effect,
                execution,
                plan: ticket.plan,
                process: Some(process),
                stdout: Some(stdout),
                stderr: Some(stderr),
                prepared: ticket.prepared,
                _pins: pins,
                _conflict: conflict,
                finished: false,
            }),
            Err(error) => {
                // A timed-out worker callback may still reach the native launch
                // gate. Invalidate its generation before giving up this claim.
                if self.worker.fenced() {
                    fence_process_owner(&self.runtime);
                }
                let binding = ticket.binding;
                let effect = ticket.effect;
                let reason = error.clone();
                if self
                    .worker
                    .run_cleanup(move |context| {
                        let current: Effect = context
                            .engine
                            .store()
                            .state()
                            .record(
                                Collection::Effect,
                                effect.as_str(),
                                &binding.scope.workspace,
                            )?
                            .decode()?;
                        let next = match current.state {
                            EffectState::Proposed
                            | EffectState::Validated
                            | EffectState::Authorized => Some(EffectState::Cancelled),
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
                                current.execution,
                                current.observed_changes,
                                &reason,
                            )?;
                            if next == EffectState::OutcomeUnknown {
                                let task: vcp_domain::task::Task = context.engine.store().state()
                                    .record(Collection::Task, binding.scope.task.as_str(), &binding.scope.workspace)?.decode()?;
                                if task.state == vcp_domain::task::TaskState::Running {
                                    context.command(Command::Transition {
                                        next: vcp_domain::task::TaskState::Paused,
                                        reason: "native startup observation failed; reconcile before resume".into(),
                                        verification: None,
                                    }, Some(binding.scope.task.clone()), task.revision)?;
                                }
                            }
                        }
                        Ok(())
                    })
                    .is_err()
                {
                    self.worker.fence();
                }
                Err(error)
            }
        }
    }
}
impl PreparedProcess {
    pub fn id(&self) -> Option<u32> {
        self.process.as_ref().and_then(|p| p.id())
    }
    pub fn active_process_count(&self) -> Result<u32, String> {
        self.process
            .as_ref()
            .ok_or("process consumed")?
            .active_process_count()
            .map_err(|e| e.to_string())
    }
    pub async fn wait(mut self) -> Result<PreparedProcessOutcome, String> {
        let observed = self
            .process
            .take()
            .ok_or("process consumed")?
            .wait()
            .await
            .map_err(|e| e.to_string())?;
        self.finish_observed(observed, vec![])
    }
    fn finish_observed(
        &mut self,
        observed: crate::process::Outcome,
        additional_evidence: Vec<ArtifactId>,
    ) -> Result<PreparedProcessOutcome, String> {
        let partial = observed.stop_reason.is_some();
        let out = Arc::try_unwrap(self.stdout.take().unwrap())
            .map_err(|_| "stdout observer still active")?;
        let err = Arc::try_unwrap(self.stderr.take().unwrap())
            .map_err(|_| "stderr observer still active")?;
        let stdout = if partial {
            out.finish_partial()?
        } else {
            out.finish()?
        };
        let stderr = if partial {
            err.finish_partial()?
        } else {
            err.finish()?
        };
        let binding = self.binding.clone();
        let effect = self.effect.clone();
        let execution = self.execution.clone();
        let plan = self.plan.clone();
        let prepared = self.prepared.clone();
        let output = vec![stdout.spec.id.clone(), stderr.spec.id.clone()];
        let exit = observed.exit_code;
        let reason = observed.stop_reason.clone();
        let total = (observed.stdout.total, observed.stderr.total);
        let stdout_presentation = vcp_tools::process::output::decode(
            &observed.stdout.bytes,
            observed.stdout.total,
            prepared.profile().output_encoding(),
        );
        let stderr_presentation = vcp_tools::process::output::decode(
            &observed.stderr.bytes,
            observed.stderr.total,
            prepared.profile().output_encoding(),
        );
        let presentation =
            serde_json::json!({"stdout":stdout_presentation,"stderr":stderr_presentation});
        let evidence=self.host.worker.run_cleanup(move|context|{
            // Drained output is already observed work. A new filesystem scan
            // needs current read authority even while recording a late outcome.
            let sources=match context.process_decision(&binding,&prepared) {
                Ok(vcp_policy::Decision::Allow{..})=>match prepared.root().discover(&vcp_repository::discovery::Limits::default()) {
                    Ok(scan)=>serde_json::json!({"complete":scan.complete,"sources":scan.sources.iter().map(|s|&s.version).collect::<Vec<_>>(),"exclusions":scan.exclusions}),
                    Err(e)=>serde_json::json!({"complete":false,"error":e.to_string()})
                },
                _=>serde_json::json!({"complete":false,"reason":"current authority does not permit a fresh workspace observation"})
            };
            let evidence=context.capture(&binding.scope,Channel::Evidence,&vcp_protocol::canonical_bytes(&serde_json::json!({"schema_version":1,"effect":effect,"execution":execution,"exit_code":exit,"stop_reason":reason,"stdout_bytes":total.0,"stderr_bytes":total.1,"presentation":presentation,"output_complete":!partial,"owned_processes_remaining":0,"observed_workspace":sources,"external_effects":"opaque; reduced isolation does not inventory external filesystem/network effects"}))?,"vcp-process-outcome-v1")?;
            let mut receipts=vec![plan,evidence.spec.id.clone()];receipts.extend(output);receipts.extend(additional_evidence);
            let current:Effect=context.engine.store().state().record(Collection::Effect,effect.as_str(),&binding.scope.workspace)?.decode()?;
            receipts.extend(current.observed_changes);
            receipts.sort();receipts.dedup();
            context.command(Command::AdvanceEffect{id:effect,next:if exit==Some(0)&&!partial{EffectState::Succeeded}else{EffectState::Failed},reason:"native process and job quiescence observed; partial effects are retained".into(),execution:Some(execution),exit_code:exit,observed_changes:receipts},Some(binding.scope.task.clone()),current.revision)?;
            Ok(evidence)
        })?;
        self.finished = true;
        Ok(PreparedProcessOutcome {
            effect: self.effect.clone(),
            exit_code: observed.exit_code,
            stdout,
            stderr,
            stdout_presentation,
            stderr_presentation,
            stdout_tail: observed.stdout.bytes,
            stderr_tail: observed.stderr.bytes,
            reason: observed.stop_reason,
            evidence,
        })
    }
}
impl Drop for PreparedProcess {
    fn drop(&mut self) {
        if !self.finished {
            // A termination request is not a quiescence receipt. Fence queued
            // callbacks before this process's resource lease can be released.
            // Hold owns the native drain even when its waiter is dropped.
            fence_process_owner(&self.host.runtime);
            self.process.take();
            let binding = self.binding.clone();
            let effect = self.effect.clone();
            if self
                .host
                .worker
                .run_cleanup(move |context| {
                    let current: Effect = context
                        .engine
                        .store()
                        .state()
                        .record(
                            Collection::Effect,
                            effect.as_str(),
                            &binding.scope.workspace,
                        )?
                        .decode()?;
                    if matches!(
                        current.state,
                        EffectState::DispatchRecorded | EffectState::Running
                    ) {
                        context.tool_advance(
                            &binding,
                            &effect,
                            EffectState::OutcomeUnknown,
                            current.execution,
                            current.observed_changes,
                            "process observation interrupted; no automatic replay",
                        )?;
                        let task: vcp_domain::task::Task = context
                            .engine
                            .store()
                            .state()
                            .record(
                                Collection::Task,
                                binding.scope.task.as_str(),
                                &binding.scope.workspace,
                            )?
                            .decode()?;
                        if task.state == vcp_domain::task::TaskState::Running {
                            context.command(
                                Command::Transition {
                                    next: vcp_domain::task::TaskState::Paused,
                                    reason:
                                        "process observation interrupted; reconcile before resume"
                                            .into(),
                                    verification: None,
                                },
                                Some(binding.scope.task),
                                task.revision,
                            )?;
                        }
                    }
                    Ok(())
                })
                .is_err()
            {
                self.host.worker.fence();
            }
        }
    }
}
