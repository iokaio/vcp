// SPDX-License-Identifier: Apache-2.0
//! Native process capabilities remain owner-bound, one-use and non-serializable.
use super::*;
use std::{collections::BTreeMap, ffi::OsString};
use vcp_domain::effect::{Effect, EffectState};
use vcp_store::contract::Collection;
use vcp_tools::process::{Pins, Prepared, Profile, Request};
pub struct ProcessProposal {
    thread: ThreadId,
    binding: ThreadBinding,
    prepared: Arc<Prepared>,
    controller: ControllerId,
    owner: OwnerEpoch,
    effect: ToolRunId,
    plan: ArtifactId,
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
    host: CanonicalHost,
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
pub struct PreparedProcessOutcome {
    pub effect: ToolRunId,
    pub exit_code: Option<i32>,
    pub stdout: ArtifactDescriptor,
    pub stderr: ArtifactDescriptor,
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
        let binding = self.binding(thread)?;
        let scoped = binding.clone();
        let (prepared, controller, owner, effect, plan, decision, question) =
            self.worker.run(move |context| {
                let prepared = context.prepare_process(&scoped, request)?;
                let _pins = prepared.pin()?;
                let decision = context.process_decision(&scoped, &prepared)?;
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
            decision,
            question,
        })
    }
    pub fn dispatch_process(&self, ticket: ProcessProposal) -> Result<PreparedProcess, String> {
        let conflict = EffectLease::acquire(&self.tool_conflict)?;
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
        // Launch on the caller's live reactor, never the store worker's
        // current-thread runtime (which is not an execution scheduler).
        let reactor =
            tokio::runtime::Handle::try_current().map_err(|_| "live process runtime required")?;
        let started=self.worker.run(move|context|{
            if context.engine.controller()!=&controller || context.engine.owner_epoch()!=owner{return Err("prepared process belongs to another owner".into());}
            let pins=Arc::new(prepared.pin()?);
            if !matches!(context.process_decision(&binding,&prepared)?,vcp_policy::Decision::Allow{..}){return Err("current process authority rejected".into());}
            context.tool_advance(&binding,&effect,EffectState::Authorized,None,vec![plan.clone()],"current process policy and native identities accepted")?;
            context.tool_advance(&binding,&effect,EffectState::DispatchRecorded,Some(run.clone()),vec![plan.clone()],"one owned native process dispatch recorded")?;
            let _entered=reactor.enter();
            let arguments:Vec<OsString>=prepared.arguments().iter().map(OsString::from).collect();
            let environment:BTreeMap<OsString,OsString>=prepared.environment().iter().map(|(k,v)|(k.into(),v.into())).collect();
            let op=prepared.authority().operation();
            let limits=crate::process::Limits{timeout:Duration::from_millis(op.timeout_ms.get()),output_bytes:op.output_bytes.get(),process_count:prepared.profile().process_count()};
            let process=if let Some(terminal)=prepared.profile().terminal() {
                runtime.spawn_pty_with_capture(thread,&prepared.executable(),&arguments,&prepared.directory(),&environment,
                    codex_utils_pty::TerminalSize{rows:terminal.rows,cols:terminal.cols},prepared.input().map(str::to_owned),limits,
                    Arc::new(move|bytes|out.write(bytes).map_err(std::io::Error::other)),pins.clone())?
            } else { runtime.spawn_bounded_process_with_capture(thread,&prepared.executable(),&arguments,&prepared.directory(),&environment,64*1024,
                Some(Arc::new(move|bytes|out.write(bytes).map_err(std::io::Error::other))),
                Some(Arc::new(move|bytes|err.write(bytes).map_err(std::io::Error::other))),
                Some(limits),prepared.profile().mode()==vcp_tools::process::Mode::Cmd,Some(pins.clone()))? };
            let identity=context.capture(&binding.scope,Channel::Evidence,&vcp_protocol::canonical_bytes(&serde_json::json!({"execution":run,"process_id":process.id(),"identity_authority":"owned process/job handles; PID is diagnostic only","job_processes":process.active_process_count()?}))?,"vcp-process-start-v1")?;
            context.tool_advance(&binding,&effect,EffectState::Running,Some(run),vec![plan,identity.spec.id],"native process launched with owned job membership before execution")?;
            Ok((process,pins))
        });
        match started {
            Ok((process, pins)) => Ok(PreparedProcess {
                host: self.clone(),
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
        let evidence=self.host.worker.run_cleanup(move|context|{
            let discovery=prepared.root().discover(&vcp_repository::discovery::Limits::default());
            let sources=match discovery{Ok(scan)=>serde_json::json!({"complete":scan.complete,"sources":scan.sources.iter().map(|s|&s.version).collect::<Vec<_>>(),"exclusions":scan.exclusions}),Err(e)=>serde_json::json!({"complete":false,"error":e.to_string()})};
            let evidence=context.capture(&binding.scope,Channel::Evidence,&vcp_protocol::canonical_bytes(&serde_json::json!({"schema_version":1,"effect":effect,"execution":execution,"exit_code":exit,"stop_reason":reason,"stdout_bytes":total.0,"stderr_bytes":total.1,"output_complete":!partial,"owned_processes_remaining":0,"observed_workspace":sources,"external_effects":"opaque; reduced isolation does not inventory external filesystem/network effects"}))?,"vcp-process-outcome-v1")?;
            let mut receipts=vec![plan,evidence.spec.id.clone()];receipts.extend(output);
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
