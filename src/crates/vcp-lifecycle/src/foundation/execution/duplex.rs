// SPDX-License-Identifier: Apache-2.0
//! Owned duplex process lifetime, not MCP call authority. The exclusive process
//! claim remains held until observed shutdown. Production protocol writes stay
//! internal until an adapter supplies per-message intent and source fences.
use super::*;
use crate::process::duplex::{Duplex, DuplexLimits};

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct DuplexIoLimits {
    pub frame_bytes: usize,
    pub queued_frames: usize,
    pub input_bytes: u64,
    pub max_messages: u32,
}
impl DuplexIoLimits {
    fn validate(self) -> Result<(), String> {
        if self.frame_bytes == 0
            || self.frame_bytes > 1024 * 1024
            || self.queued_frames == 0
            || self.queued_frames > 64
            || self.input_bytes == 0
            || self.input_bytes > 8 * 1024 * 1024
            || self.max_messages == 0
            || self.max_messages > 256
        {
            return Err("duplex IO bounds rejected".into());
        }
        Ok(())
    }
}

pub struct DuplexProcess {
    // Drop the pipe owner before the canonical guard releases its fenced claim.
    process: Option<Duplex>,
    lifetime: PreparedProcess,
    thread: ThreadId,
    generation: u64,
    controller: ControllerId,
    owner: OwnerEpoch,
    limits: DuplexIoLimits,
    messages: u32,
    input_bytes: u64,
    inputs: Vec<ArtifactId>,
}

impl CanonicalHost {
    pub fn dispatch_duplex_process(
        &self,
        ticket: ProcessProposal,
        limits: DuplexIoLimits,
    ) -> Result<DuplexProcess, String> {
        limits.validate()?;
        let conflict = self
            .scheduler
            .try_acquire(ticket.prepared.authority().operation())?;
        self.dispatch_duplex_leased(ticket, limits, conflict)
    }
    pub async fn schedule_duplex_process(
        &self,
        ticket: ProcessProposal,
        limits: DuplexIoLimits,
    ) -> Result<DuplexProcess, String> {
        limits.validate()?;
        let mut queued = scheduler::QueuedEffect::new(self, &ticket.binding, &ticket.effect);
        let conflict = match self
            .schedule(
                ticket.prepared.authority().operation(),
                ticket.thread,
                ticket.generation,
            )
            .await
        {
            Ok(claim) => claim,
            Err(error) => {
                self.cancel_queued_effect(ticket.binding, ticket.effect, error.clone())?;
                return Err(error);
            }
        };
        let result = self.dispatch_duplex_leased(ticket, limits, conflict);
        if result.is_ok() {
            queued.dispatched();
        }
        result
    }
    fn dispatch_duplex_leased(
        &self,
        ticket: ProcessProposal,
        io: DuplexIoLimits,
        conflict: EffectLease,
    ) -> Result<DuplexProcess, String> {
        if ticket.prepared.profile().mode() != vcp_tools::process::Mode::Direct
            || ticket.prepared.profile().terminal().is_some()
            || ticket.prepared.input().is_some()
        {
            return Err(
                "duplex requires a direct non-terminal profile without prepared stdin".into(),
            );
        }
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
        let reactor =
            tokio::runtime::Handle::try_current().map_err(|_| "live process runtime required")?;
        let started = self.worker.run(move |context| {
            if context.engine.controller() != &controller || context.engine.owner_epoch() != owner { return Err("duplex process belongs to another owner".into()); }
            if !matches!(context.process_preflight(&binding, &prepared)?, vcp_policy::Decision::Allow { .. }) { return Err("current duplex startup authority rejected".into()); }
            let pins = Arc::new(prepared.pin()?);
            if !matches!(context.process_decision(&binding, &prepared)?, vcp_policy::Decision::Allow { .. }) { return Err("current duplex startup identities rejected".into()); }
            let configuration = context.capture(&binding.scope, Channel::Evidence,
                &vcp_protocol::canonical_bytes(&serde_json::json!({"schema_version":1,"effect":effect,"execution":run,"operation_digest":prepared.authority().digest(),"io":io,"authority":"process startup only; protocol calls require separate admission"}))?, "vcp-duplex-lifetime-v1")?;
            let evidence = vec![plan.clone(), configuration.spec.id.clone()];
            context.tool_advance(&binding, &effect, EffectState::Authorized, None, evidence.clone(), "duplex startup authority and native identities accepted")?;
            context.tool_advance(&binding, &effect, EffectState::DispatchRecorded, Some(run.clone()), evidence.clone(), "one duplex process launch durably recorded")?;
            observe_process_start(&runtime, || {
                let _entered = reactor.enter();
                let arguments = prepared.arguments().iter().map(OsString::from).collect::<Vec<_>>();
                let environment = prepared.environment().iter().map(|(k,v)| (k.into(),v.into())).collect::<BTreeMap<OsString,OsString>>();
                let op = prepared.authority().operation();
                let process = runtime.spawn_duplex_process_generation(thread, &prepared.executable(), &arguments, &prepared.directory(), &environment, 64 * 1024,
                    Some(Arc::new(move |bytes| out.write(bytes).map_err(std::io::Error::other))),
                    Some(Arc::new(move |bytes| err.write(bytes).map_err(std::io::Error::other))),
                    DuplexLimits { process: crate::process::Limits { timeout: Duration::from_millis(op.timeout_ms.get()), output_bytes: op.output_bytes.get(), process_count: prepared.profile().process_count() }, frame_bytes: io.frame_bytes, queued_frames: io.queued_frames, input_bytes: io.input_bytes }, Some(pins.clone()), Some(generation))?;
                let identity = context.capture(&binding.scope, Channel::Evidence,
                    &vcp_protocol::canonical_bytes(&serde_json::json!({"execution":run,"process_id":process.id(),"process_identity":super::super::worker::recovery::process_identity(process.id()),"identity_authority":"owned duplex process/job handles; PID is diagnostic only","job_processes":process.active_process_count()?}))?, "vcp-process-start-v1")?;
                let mut evidence = evidence; evidence.push(identity.spec.id);
                context.tool_advance(&binding, &effect, EffectState::Running, Some(run), evidence, "duplex connection process running; individual protocol calls are not authorized")?;
                Ok((process,pins,configuration.spec.id))
            })
        });
        let (process, pins, configuration) = match started {
            Ok(value) => value,
            Err(error) => {
                if self.worker.fenced() {
                    fence_process_owner(&self.runtime);
                }
                let binding = ticket.binding;
                let effect = ticket.effect;
                let reason = error.clone();
                if self.worker.run_cleanup(move |context| {
                    let current: Effect = context.engine.store().state().record(Collection::Effect, effect.as_str(), &binding.scope.workspace)?.decode()?;
                    let next = match current.state {
                        EffectState::Proposed | EffectState::Validated | EffectState::Authorized => Some(EffectState::Cancelled),
                        EffectState::DispatchRecorded | EffectState::Running => Some(EffectState::OutcomeUnknown), _ => None,
                    };
                    if let Some(next) = next {
                        context.tool_advance(&binding, &effect, next, current.execution, current.observed_changes, &reason)?;
                        if next == EffectState::OutcomeUnknown {
                            let task: vcp_domain::task::Task = context.engine.store().state().record(Collection::Task, binding.scope.task.as_str(), &binding.scope.workspace)?.decode()?;
                            if task.state == vcp_domain::task::TaskState::Running {
                                context.command(Command::Transition { next: vcp_domain::task::TaskState::Paused, reason: "duplex startup observation failed; reconcile before resume".into(), verification: None }, Some(binding.scope.task.clone()), task.revision)?;
                            }
                        }
                    }
                    Ok(())
                }).is_err() { self.worker.fence(); }
                return Err(error);
            }
        };
        Ok(DuplexProcess {
            process: Some(process),
            thread,
            generation,
            controller: ticket.controller,
            owner: ticket.owner,
            limits: io,
            messages: 0,
            input_bytes: 0,
            inputs: vec![configuration],
            lifetime: PreparedProcess {
                host: self.clone(),
                binding: ticket.binding,
                effect: ticket.effect,
                execution,
                plan: ticket.plan,
                process: None,
                stdout: Some(stdout),
                stderr: Some(stderr),
                prepared: ticket.prepared,
                _pins: pins,
                _conflict: conflict,
                finished: false,
            },
        })
    }
}

impl DuplexProcess {
    pub fn id(&self) -> Option<u32> {
        self.process.as_ref().and_then(Duplex::id)
    }
    pub fn effect(&self) -> &ToolRunId {
        &self.lifetime.effect
    }
    fn current_io_authority(&self) -> Result<(), String> {
        scheduler::check_generation(&self.lifetime.host.runtime, self.thread, self.generation)?;
        let binding = self.lifetime.binding.clone();
        let prepared = self.lifetime.prepared.clone();
        let controller = self.controller.clone();
        let owner = self.owner;
        self.lifetime.host.worker.run(move |context| {
            if context.engine.controller() != &controller
                || context.engine.owner_epoch() != owner
                || !matches!(
                    context.process_preflight(&binding, &prepared)?,
                    vcp_policy::Decision::Allow { .. }
                )
                || !matches!(
                    context.process_decision(&binding, &prepared)?,
                    vcp_policy::Decision::Allow { .. }
                )
            {
                return Err("current duplex IO authority rejected".into());
            }
            Ok(())
        })
    }
    pub async fn read_line(&mut self) -> Result<Option<Vec<u8>>, String> {
        self.current_io_authority()?;
        let line = self
            .process
            .as_mut()
            .ok_or("duplex consumed")?
            .read_line()
            .await
            .map_err(|e| e.to_string())?;
        self.current_io_authority()?;
        Ok(line)
    }
    /// Internal transport IO only. An MCP adapter must first admit its own
    /// canonical call and revalidate every referenced source; this check grants
    /// no per-message authority and capture is not an authorization receipt.
    /// Authentication-bearing messages must use a separately sanitized boundary,
    /// never this raw protocol-input capture. Recovery must not infer delivery
    /// from an input artifact: a cancelled write can leave absent/partial bytes.
    #[allow(dead_code)] // Production caller belongs to the following MCP increment.
    pub(crate) async fn write_line(&mut self, bytes: &[u8]) -> Result<(), String> {
        if bytes.is_empty()
            || bytes.len() > self.limits.frame_bytes
            || bytes.contains(&b'\n')
            || bytes.contains(&b'\r')
            || self.messages >= self.limits.max_messages
        {
            return Err("duplex input frame or message limit".into());
        }
        let next_bytes = self
            .input_bytes
            .checked_add(bytes.len() as u64 + 1)
            .ok_or("duplex input overflow")?;
        if next_bytes > self.limits.input_bytes {
            return Err("duplex input lifetime limit".into());
        }
        scheduler::check_generation(&self.lifetime.host.runtime, self.thread, self.generation)?;
        let binding = self.lifetime.binding.clone();
        let prepared = self.lifetime.prepared.clone();
        let effect = self.lifetime.effect.clone();
        let execution = self.lifetime.execution.clone();
        let controller = self.controller.clone();
        let owner = self.owner;
        let sequence = self.messages + 1;
        let bytes = bytes.to_vec();
        let capture = bytes.clone();
        let artifacts = self.lifetime.host.worker.run(move |context| {
            if context.engine.controller() != &controller || context.engine.owner_epoch() != owner { return Err("duplex input belongs to another owner".into()); }
            if !matches!(context.process_preflight(&binding, &prepared)?, vcp_policy::Decision::Allow { .. })
                || !matches!(context.process_decision(&binding, &prepared)?, vcp_policy::Decision::Allow { .. }) { return Err("current duplex input process authority rejected".into()); }
            let current: Effect = context.engine.store().state().record(Collection::Effect, effect.as_str(), &binding.scope.workspace)?.decode()?;
            if current.state != EffectState::Running || current.execution.as_ref() != Some(&execution) { return Err("duplex process is not the current running execution".into()); }
            let body = context.capture(&binding.scope, Channel::Evidence, &capture, "vcp-duplex-input-bytes-v1")?;
            let intent = context.capture(&binding.scope, Channel::Evidence,
                &vcp_protocol::canonical_bytes(&serde_json::json!({"schema_version":1,"effect":effect,"execution":execution,"sequence":sequence,"body":body.spec.id,"sha256":vcp_protocol::digest_bytes(&capture),"bytes":capture.len(),"framing":"LF appended","observation":"captured before write; partial/absent delivery remains possible"}))?, "vcp-duplex-input-v1")?;
            Ok(vec![body.spec.id,intent.spec.id])
        })?;
        self.inputs.extend(artifacts);
        self.messages = sequence;
        self.input_bytes = next_bytes;
        self.process
            .as_mut()
            .ok_or("duplex consumed")?
            .write_line(&bytes)
            .await
            .map_err(|e| e.to_string())
    }
    /// Native qualification seam only; unavailable in normal builds.
    #[cfg(feature = "qualification")]
    pub async fn qualification_write_line(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.write_line(bytes).await
    }
    pub fn close_stdin(&mut self) {
        if let Some(process) = &mut self.process {
            process.close_stdin();
        }
    }
    pub fn terminate(&self) -> Result<(), String> {
        self.process
            .as_ref()
            .ok_or("duplex consumed")?
            .terminate()
            .map_err(|e| e.to_string())
    }
    /// Receipt describes connection process exit, never success of a protocol call.
    pub async fn wait(mut self) -> Result<PreparedProcessOutcome, String> {
        let observed = self
            .process
            .take()
            .ok_or("duplex consumed")?
            .wait()
            .await
            .map_err(|e| e.to_string())?;
        self.lifetime
            .finish_observed(observed, std::mem::take(&mut self.inputs))
    }
}
