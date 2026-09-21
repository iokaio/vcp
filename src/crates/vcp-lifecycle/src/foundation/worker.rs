// SPDX-License-Identifier: Apache-2.0
mod authority;
#[cfg(windows)]
mod backup_checkpoint;
#[cfg(windows)]
mod coding;
#[cfg(feature = "qualification")]
mod conformance;
#[cfg(windows)]
mod console;
mod control;
#[cfg(windows)]
pub(super) mod decision;
#[cfg(windows)]
mod escalation;
#[cfg(windows)]
mod execution;
#[cfg(windows)]
mod mcp;
mod memory;
#[cfg(windows)]
mod memory_query;
mod provider;
mod reasoning;
pub(super) mod recovery;
mod retention_policy;
mod routing;
#[cfg(windows)]
mod skills;
#[cfg(windows)]
mod tools;
#[cfg(windows)]
mod verification;
use super::{Config, ThreadBinding};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use vcp_domain::{accounting::*, artifact::*, ids::*, revision::*, task::*, workspace::*};
use vcp_engine::{Access, Engine, HostFacts};
use vcp_protocol::{canonical_bytes, command::*};
use vcp_store::{
    artifact::{ArtifactWriter, LocalWriter},
    contract::*,
    Store,
};
type Failure = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, Failure>;
type Job = Box<dyn FnOnce(&mut Context) + Send>;
struct Inner {
    thread_id: std::thread::ThreadId,
    sender: Mutex<Option<SyncSender<Job>>>,
    thread: Mutex<Option<JoinHandle<std::result::Result<(), String>>>>,
    fenced: AtomicBool,
}
impl Drop for Inner {
    fn drop(&mut self) {
        self.sender.get_mut().unwrap().take();
        if let Some(thread) = self.thread.get_mut().unwrap().take() {
            match thread.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => eprintln!("canonical store shutdown failed: {error}"),
                Err(_) => eprintln!("canonical store worker panicked during shutdown"),
            }
        }
    }
}
#[derive(Clone)]
pub struct Worker(Arc<Inner>);
impl Worker {
    pub fn open(config: Config, expected: Option<Revision>) -> std::result::Result<Self, String> {
        let (tx, rx) = mpsc::sync_channel::<Job>(32);
        let (ready, ready_rx) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("vcp-canonical-store".into())
            .spawn(move || match Context::open_selected(config, expected) {
                Ok(mut context) => {
                    let _ = ready.send(Ok(()));
                    while let Ok(job) = rx.recv() {
                        job(&mut context);
                    }
                    context.close().map_err(|error| error.to_string())
                }
                Err(error) => {
                    let _ = ready.send(Err(error.to_string()));
                    Err(error.to_string())
                }
            })
            .map_err(|error| error.to_string())?;
        ready_rx.recv().map_err(|_| "canonical worker stopped")??;
        let thread_id = thread.thread().id();
        Ok(Self(Arc::new(Inner {
            thread_id,
            sender: Mutex::new(Some(tx)),
            thread: Mutex::new(Some(thread)),
            fenced: AtomicBool::new(false),
        })))
    }
    pub fn fence(&self) {
        self.0.fenced.store(true, Ordering::SeqCst);
    }
    pub fn fenced(&self) -> bool {
        self.0.fenced.load(Ordering::SeqCst)
    }
    pub fn run<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut Context) -> Result<T> + Send + 'static,
    ) -> std::result::Result<T, String> {
        if self.fenced() {
            return Err("canonical capture/admission fenced; reopen required".into());
        }
        self.run_cleanup(operation)
    }
    pub fn run_cleanup<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut Context) -> Result<T> + Send + 'static,
    ) -> std::result::Result<T, String> {
        if std::thread::current().id() == self.0.thread_id {
            self.fence();
            #[cfg(debug_assertions)]
            eprintln!("canonical worker rejected synchronous self-reentry");
            return Err("canonical worker cannot synchronously reenter itself".into());
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let job: Job = Box::new(move |context| {
            let result = operation(context).map_err(|error| error.to_string());
            let _ = tx.send((result, context.interrupted_capture));
        });
        self.0
            .sender
            .lock()
            .map_err(|_| "canonical queue poisoned")?
            .as_ref()
            .ok_or("canonical owner closed")?
            .try_send(job)
            .map_err(|_| "canonical queue unavailable or full")?;
        match rx.recv_timeout(Duration::from_secs(30)) {
            Ok((result, fenced)) => {
                if fenced {
                    self.fence();
                }
                result
            }
            Err(_) => {
                self.fence();
                Err("canonical operation outcome unknown; reopen required".into())
            }
        }
    }
}
pub struct Context {
    pub engine: Engine<Store>,
    pub runtime: tokio::runtime::Runtime,
    pub config: Config,
    access: Access,
    streams: HashMap<AttemptId, LocalWriter>,
    outputs: HashMap<ArtifactId, (Scope, LocalWriter)>,
    interrupted_capture: bool,
    owner_alive: bool,
    authority_pending: bool,
    provider_required: bool,
    provider: Option<provider::Provider>,
    routing: Option<routing::Runtime>,
    #[cfg(windows)]
    decisions: decision::Runtime,
    #[cfg(windows)]
    skills: Option<skills::Runtime>,
    #[cfg(windows)]
    coding: HashMap<TaskId, coding::Loop>,
    #[cfg(windows)]
    process_profiles: HashMap<String, vcp_tools::process::Profile>,
    #[cfg(windows)]
    mcp: mcp::State,
    #[cfg(windows)]
    verification: HashMap<TaskId, verification::Setup>,
}
fn now() -> Timestamp {
    Timestamp::new(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(u64::MAX as u128) as u64,
    )
}
fn has_unpriced_media(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Array(values) => values.iter().any(has_unpriced_media),
        serde_json::Value::Object(values) => {
            matches!(
                values.get("type").and_then(|value| value.as_str()),
                Some(
                    "input_image"
                        | "image_url"
                        | "input_audio"
                        | "output_audio"
                        | "audio"
                        | "input_video"
                        | "video"
                        | "input_file"
                        | "file"
                )
            ) || values.values().any(has_unpriced_media)
        }
        _ => false,
    }
}
impl Context {
    fn close(self) -> Result<()> {
        self.runtime.block_on(self.engine.into_store().close())?;
        Ok(())
    }

    #[cfg(test)]
    fn open(config: Config) -> Result<Self> {
        Self::open_selected(config, None)
    }
    fn open_selected(config: Config, expected: Option<Revision>) -> Result<Self> {
        vcp_policy::validate_host_denials(&config.host_tool_denials)?;
        if config.input_ceiling.get() == 0 || config.output_ceiling.get() == 0 {
            return Err("provider ceilings required".into());
        }
        if config.max_transport_retries > 2 {
            return Err("transport retry ceiling cannot exceed two".into());
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let store = runtime.block_on(Store::open_with_artifact_limit(
            &config.canonical_root,
            config.backend,
            &[],
            config.artifact_limit.get(),
        ))?;
        if let Some(expected) = expected {
            let task: Task = store
                .state()
                .record(
                    Collection::Task,
                    config.root_task.as_str(),
                    &config.workspace,
                )?
                .decode()?;
            if task.revision != expected
                || task.scope.session != config.session
                || task.parent.is_some()
            {
                return Err("task changed since selection; refresh workspace discovery".into());
            }
        }
        let engine = Engine::new(store)?;
        let access = Access {
            actor: config.actor.clone(),
            workspace: config.workspace.clone(),
            session: config.session.clone(),
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            bootstrap: true,
        };
        let interrupted_capture = !engine.store().spool().unfinished()?.is_empty();
        let provider_required = engine
            .store()
            .state()
            .records
            .values()
            .filter(|row| row.collection == Collection::Artifact)
            .filter_map(|row| row.decode::<ArtifactDescriptor>().ok())
            .any(|row| row.spec.schema == "openrouter-provider-configuration/1");
        let mut context = Self {
            engine,
            runtime,
            config,
            access,
            streams: HashMap::new(),
            outputs: HashMap::new(),
            interrupted_capture,
            owner_alive: true,
            authority_pending: false,
            provider_required,
            provider: None,
            routing: None,
            #[cfg(windows)]
            decisions: decision::Runtime::default(),
            #[cfg(windows)]
            skills: None,
            #[cfg(windows)]
            coding: HashMap::new(),
            #[cfg(windows)]
            process_profiles: HashMap::new(),
            #[cfg(windows)]
            mcp: mcp::State::default(),
            #[cfg(windows)]
            verification: HashMap::new(),
        };
        if context.engine.store().state().records.is_empty() {
            context.command(
                Command::Initialize {
                    binding: context.config.binding.clone(),
                },
                None,
                Revision::ZERO,
            )?;
        } else {
            let workspace: Workspace = context
                .engine
                .store()
                .state()
                .record(
                    Collection::Workspace,
                    context.config.workspace.as_str(),
                    &context.config.workspace,
                )?
                .decode()?;
            if workspace.binding != context.config.binding {
                return Err("workspace binding requires explicit revalidation".into());
            }
            context.access.authority = workspace.authority;
            // Resume never repeats a previously submitted HTTP request.
            let attempts: Vec<Attempt> = context
                .engine
                .store()
                .state()
                .records
                .values()
                .filter(|r| r.collection == Collection::Attempt)
                .map(Record::decode)
                .collect::<std::result::Result<_, _>>()?;
            for attempt in attempts {
                if attempt.phase == ReservationState::Submitted {
                    let actor = context.actor();
                    context.runtime.block_on(vcp_budget::hold_uncertain(
                        context.engine.store_mut(),
                        &attempt.id,
                        &attempt.scope,
                        &actor,
                        "owner reopened after possible send",
                    ))?;
                }
            }
            context.reconcile_effects()?;
            let tasks: Vec<Task> = context
                .engine
                .store()
                .state()
                .records
                .values()
                .filter(|r| r.collection == Collection::Task)
                .map(Record::decode)
                .collect::<std::result::Result<_, _>>()?;
            for task in tasks {
                if matches!(task.state, TaskState::Pending | TaskState::Running) {
                    context
                        .recovery_pause(task, "owning host reopened; deliberate resume required")?;
                }
            }
        }
        #[cfg(windows)]
        context.stop_coding_turns("owner recovered canonical turn")?;
        context.apply_startup_retention_policy()?;
        Ok(context)
    }
    fn actor(&self) -> vcp_budget::Actor {
        vcp_budget::Actor {
            id: self.config.actor.clone(),
            now: now(),
        }
    }
    pub fn observe_usage(&mut self, observation: UsageObservation) -> Result<Settlement> {
        let actor = self.actor();
        Ok(self.runtime.block_on(vcp_budget::observe(
            self.engine.store_mut(),
            observation,
            &actor,
        ))?)
    }
    pub fn history_access(&self) -> vcp_audit::history::Access {
        vcp_audit::history::Access {
            workspace: self.access.workspace.clone(),
            authority: self.access.authority,
            read: true,
            tasks: None,
        }
    }
    pub fn command(
        &mut self,
        payload: Command,
        task: Option<TaskId>,
        expected: Revision,
    ) -> Result<CommandReceipt> {
        self.command_with_resume(payload, task, expected, None)
    }
    fn command_with_resume(
        &mut self,
        payload: Command,
        task: Option<TaskId>,
        expected: Revision,
        resume: Option<ResumeEvidence>,
    ) -> Result<CommandReceipt> {
        // Yield capture/tool hot paths to interactive work. Their durable
        // observations are picked up at the next task/turn/check boundary.
        let maintain_memory = matches!(
            payload,
            Command::Transition { .. }
                | Command::RecordVerification { .. }
                | Command::AdvanceTurn {
                    next: TurnState::Verifying | TurnState::Completed,
                    ..
                }
        );
        let steering = task
            .as_ref()
            .and_then(|id| {
                self.engine
                    .store()
                    .state()
                    .record(Collection::Task, id.as_str(), &self.config.workspace)
                    .ok()
            })
            .and_then(|row| row.decode::<Task>().ok())
            .map_or(SteeringRevision::ZERO, |task| task.steering);
        let envelope = CommandEnvelope {
            version: 1,
            id: CommandId::new(),
            workspace: self.config.workspace.clone(),
            session: self.config.session.clone(),
            task,
            caller: self.config.actor.clone(),
            controller: self.engine.controller().clone(),
            owner_epoch: self.engine.owner_epoch(),
            expected,
            steering,
            payload,
        };
        let host = HostFacts {
            now: now(),
            policy: vcp_engine::policy::optional(
                self.engine.store().state(),
                &self.config.workspace,
            )?
            .map_or(PolicyRevision::ZERO, |policy| policy.revision),
            resume,
            may_execute: self.owner_alive,
        };
        let receipt = self
            .runtime
            .block_on(self.engine.handle(envelope, &self.access, &host))?;
        // This trusted owner follows its own acknowledged authority change.
        // Old external credentials and prepared operations retain stale epochs.
        let workspace: Workspace = self
            .engine
            .store()
            .state()
            .record(
                Collection::Workspace,
                self.config.workspace.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        self.access.authority = workspace.authority;
        if maintain_memory {
            self.memory_after_command();
        }
        Ok(receipt)
    }
    pub fn can_start(&self, binding: &ThreadBinding) -> Result<()> {
        if self.authority_pending {
            return Err("authority change is stopping work".into());
        }
        if !self.owner_alive {
            return Err("canonical owner is closed".into());
        }
        self.validate_binding(binding)?;
        let mut id = Some(binding.scope.task.clone());
        while let Some(current) = id {
            let task: Task = self
                .engine
                .store()
                .state()
                .record(Collection::Task, current.as_str(), &binding.scope.workspace)?
                .decode()?;
            if task.state != TaskState::Running {
                return Err("canonical task or ancestor is held".into());
            }
            id = task.parent;
        }
        Ok(())
    }
    pub fn resume(
        &mut self,
        binding: &ThreadBinding,
        expected: Revision,
        fingerprint: vcp_domain::verification::Fingerprint,
    ) -> Result<CommandReceipt> {
        self.resume_checked(binding, expected, fingerprint, &[], || Ok(()))
    }
    fn resume_checked<T>(
        &mut self,
        binding: &ThreadBinding,
        expected: Revision,
        fingerprint: vcp_domain::verification::Fingerprint,
        idle_owned: &[(ToolRunId, ExecutionId)],
        final_check: impl FnOnce() -> Result<T>,
    ) -> Result<CommandReceipt> {
        if self.authority_pending {
            return Err("authority change is stopping work".into());
        }
        self.validate_binding(binding)?;
        if self.interrupted_capture {
            return Err("capture recovery incomplete".into());
        }
        let task: Task = self
            .engine
            .store()
            .state()
            .record(
                Collection::Task,
                binding.scope.task.as_str(),
                &binding.scope.workspace,
            )?
            .decode()?;
        if task.fingerprint != fingerprint {
            return Err("repository fingerprint changed; refresh canonical task first".into());
        }
        self.revalidate_resume_environment(binding)?;
        let budget_current = vcp_budget::ledger(self.engine.store().state(), &binding.scope)
            .map(|ledger| {
                !ledger.overrun
                    && ledger
                        .settled
                        .get()
                        .saturating_add(ledger.active.get())
                        .saturating_add(ledger.unresolved.get())
                        < ledger.cap.get()
            })
            .unwrap_or(true);
        let effects_reconciled = !self
            .engine
            .store()
            .state()
            .records
            .values()
            .filter(|row| row.collection == Collection::Effect)
            .map(Record::decode::<vcp_domain::effect::Effect>)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .iter()
            .any(|effect| {
                if effect.state == vcp_domain::effect::EffectState::Running
                    && idle_owned.iter().any(|(id, execution)| {
                        id == &effect.id && effect.execution.as_ref() == Some(execution)
                    })
                {
                    return false;
                }
                matches!(
                    effect.state,
                    vcp_domain::effect::EffectState::DispatchRecorded
                        | vcp_domain::effect::EffectState::Running
                        | vcp_domain::effect::EffectState::OutcomeUnknown
                )
            });
        let _resume_guard = final_check()?;
        self.command_with_resume(
            Command::Transition {
                next: TaskState::Running,
                reason: "explicit host resume after current binding, budget and effect checks"
                    .into(),
                verification: None,
            },
            Some(task.scope.task),
            expected,
            Some(ResumeEvidence {
                workspace_current: true,
                policy_current: true,
                budget_current,
                effects_reconciled,
                owner_current: true,
            }),
        )
    }
    pub fn validate_binding(&self, binding: &ThreadBinding) -> Result<()> {
        let workspace: Workspace = self
            .engine
            .store()
            .state()
            .record(
                Collection::Workspace,
                self.config.workspace.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        if workspace.authority != self.access.authority || workspace.binding != self.config.binding
        {
            return Err("workspace authority changed; rebind retained host".into());
        }
        let task: Task = self
            .engine
            .store()
            .state()
            .record(
                Collection::Task,
                binding.scope.task.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        if task.scope != binding.scope || task.root != self.config.root_task {
            return Err("thread binding differs from canonical root".into());
        }
        Ok(())
    }
    fn spec(&self, scope: &Scope, channel: Channel, schema: &str) -> ArtifactSpec {
        ArtifactSpec {
            id: ArtifactId::new(),
            scope: scope.clone(),
            media_type: "application/octet-stream".into(),
            schema: schema.into(),
            source: "retained-codex".into(),
            channel,
            retention: "full-work-history".into(),
            omissions: vec![Omission::AuthenticationHeaders, Omission::RecoveryMaterial],
        }
    }
    pub fn capture(
        &mut self,
        scope: &Scope,
        channel: Channel,
        bytes: &[u8],
        schema: &str,
    ) -> Result<ArtifactDescriptor> {
        let mut writer = self
            .engine
            .store()
            .spool()
            .create(self.spec(scope, channel, schema))?;
        for chunk in bytes.chunks(vcp_store::artifact::CHUNK_BYTES) {
            writer.write_chunk(chunk)?;
        }
        let descriptor = writer.finalize()?;
        drop(writer);
        self.command(
            Command::AttachArtifact {
                descriptor: descriptor.clone(),
            },
            Some(scope.task.clone()),
            Revision::ZERO,
        )?;
        Ok(descriptor)
    }
    pub fn admit(
        &mut self,
        binding: &ThreadBinding,
        mut body: serde_json::Value,
    ) -> Result<(
        AttemptId,
        serde_json::Value,
        Option<std::time::Instant>,
        u32,
    )> {
        if !self.owner_alive {
            return Err("canonical owner is closed".into());
        }
        self.validate_binding(binding)?;
        if self.interrupted_capture {
            return Err("unfinished capture requires reconciliation".into());
        }
        let provider_request = if self.provider_required {
            Some(self.admit_context(binding, &body)?)
        } else {
            None
        };
        if let Some(prepared) = &provider_request {
            body = prepared.body.clone();
        }
        let price = provider_request.as_ref().map_or_else(
            || self.config.price.clone(),
            |prepared| prepared.snapshot.price.clone(),
        );
        let selected_input_ceiling = if provider_request
            .as_ref()
            .is_some_and(|prepared| prepared.routing.is_some())
        {
            self.current_input_ceiling()?
        } else {
            self.config.input_ceiling
        };
        let input_ceiling = provider_request
            .as_ref()
            .map_or(selected_input_ceiling, |prepared| {
                Units::new(
                    selected_input_ceiling
                        .get()
                        .min(prepared.snapshot.max_input.get()),
                )
            });
        if body["model"].as_str() != Some(&price.model) {
            return Err("request model differs from admitted price/capability".into());
        }
        if !body.is_object() {
            return Err("request body must be an object".into());
        }
        let output_ceiling = provider_request
            .as_ref()
            .map_or(self.config.output_ceiling, |prepared| {
                prepared.output_ceiling
            });
        if output_ceiling == Units::ZERO || output_ceiling > self.config.output_ceiling {
            return Err("request output exceeds trusted host ceiling".into());
        }
        body["max_output_tokens"] = serde_json::json!(output_ceiling.get());
        if has_unpriced_media(&body["input"]) {
            return Err("multimodal request needs a qualified price/capability adapter".into());
        }
        // P1 qualifies text/function requests. Unknown native provider tools or
        // multimodal charge categories need the P2 capability adapter.
        if body
            .get("tools")
            .and_then(|v| v.as_array())
            .is_some_and(|tools| {
                tools
                    .iter()
                    .any(|tool| !matches!(tool["type"].as_str(), Some("function" | "custom")))
            })
        {
            return Err("unpriced provider tool".into());
        }
        let bytes = canonical_bytes(&body)?;
        if bytes.len() as u64 > input_ceiling.get() {
            return Err("request exceeds qualified text input byte ceiling".into());
        }
        let scope = &binding.scope;
        let actor = self.actor();
        self.initialize_root_budget()?;
        let capture = (|| -> Result<ArtifactDescriptor> {
            let mut writer = self.engine.store().spool().create(self.spec(
                scope,
                Channel::RequestBody,
                "responses-request/1",
            ))?;
            for chunk in bytes.chunks(vcp_store::artifact::CHUNK_BYTES) {
                writer.write_chunk(chunk)?;
            }
            Ok(writer.finalize()?)
        })();
        let descriptor = match capture {
            Ok(descriptor) => descriptor,
            Err(error) => {
                self.interrupted_capture = true;
                let _ = self.pause_root("request capture failed before send");
                return Err(error);
            }
        };
        let root = vcp_budget::ledger(self.engine.store().state(), scope)?;
        let task: Task = self
            .engine
            .store()
            .state()
            .record(Collection::Task, scope.task.as_str(), &scope.workspace)?
            .decode()?;
        // Reserve all possible input/cache partitions conservatively. Prices
        // retain their original identity; the bound intentionally overestimates.
        let reservation_input =
            provider_request
                .as_ref()
                .map_or(self.config.input_ceiling, |prepared| {
                    prepared
                        .snapshot
                        .reservation_input(Units::new(bytes.len() as u64))
                });
        let bounds = Usage {
            input: Units::new(
                reservation_input
                    .get()
                    .checked_mul(3)
                    .ok_or("input ceiling overflow")?,
            ),
            cache_read: reservation_input,
            cache_write: reservation_input,
            output: output_ceiling,
            requests: Units::new(1),
            ..Default::default()
        };
        let quote = vcp_budget::arithmetic::quote(price, bounds, actor.now)?;
        let input = vcp_budget::Admission {
            transaction: TransactionId::new(),
            attempt: AttemptId::new(),
            reservation: ReservationId::new(),
            scope: scope.clone(),
            agent: binding.agent.clone(),
            role: binding.role,
            request: descriptor.spec.id.clone(),
            request_digest: descriptor.sha256.clone(),
            quote,
            previous: {
                let retry = self
                    .provider
                    .as_ref()
                    .and_then(|p| p.retries.get(&binding.scope.task))
                    .map(|r| r.predecessor.clone());
                #[cfg(windows)]
                let retry = retry.or_else(|| {
                    provider_request
                        .as_ref()
                        .and_then(|prepared| prepared.escalation.as_ref())
                        .map(|pending| pending.plan.previous_attempt.clone())
                });
                retry
            },
            expected_ledger: root.revision,
            policy: root.policy,
            steering: task.steering,
            draw_protected: binding.role == RequestRole::Verification,
            now: actor.now,
        };
        #[cfg(windows)]
        self.coding_stage(
            binding,
            TurnState::ReservingBudget,
            "reserving captured request budget",
        )?;
        let reservation = self.runtime.block_on(vcp_budget::reserve_captured(
            self.engine.store_mut(),
            input,
            descriptor,
            &actor,
        ));
        let attempt = match reservation {
            Ok(attempt) => attempt,
            Err(error) => {
                #[cfg(windows)]
                if matches!(error, vcp_budget::Error::Exhausted(_)) {
                    self.coding_stage(
                        binding,
                        TurnState::BudgetExhausted,
                        "canonical budget admission denied",
                    )?;
                }
                return Err(error.into());
            }
        };
        if let Some(decision) = provider_request
            .as_ref()
            .and_then(|prepared| prepared.routing.as_ref())
        {
            if let Err(error) = self.record_routing_attempt(
                binding,
                decision,
                &vcp_protocol::digest_bytes(&bytes),
                attempt.id.clone(),
            ) {
                self.runtime.block_on(vcp_budget::release_before_send(
                    self.engine.store_mut(),
                    &attempt.id,
                    scope,
                    &actor,
                ))?;
                return Err(error);
            }
        }
        #[cfg(windows)]
        if let Some(pending) = provider_request
            .as_ref()
            .and_then(|prepared| prepared.escalation.as_ref())
        {
            let access = self.routing_access();
            let handoff = pending
                .handoff
                .as_ref()
                .ok_or("escalation handoff missing after preparation")?;
            if let Err(error) =
                self.runtime
                    .block_on(crate::foundation::routing_state::record_escalation(
                        self.engine.store_mut(),
                        &access,
                        scope,
                        &pending.plan,
                        handoff,
                        attempt.id.clone(),
                        now(),
                    ))
            {
                self.runtime.block_on(vcp_budget::release_before_send(
                    self.engine.store_mut(),
                    &attempt.id,
                    scope,
                    &actor,
                ))?;
                return Err(error.into());
            }
        }
        let response = self.engine.store().spool().create(self.spec(
            scope,
            Channel::Response,
            "responses-sse-observed-through-terminal/1",
        ));
        let response = match response {
            Ok(writer) => writer,
            Err(error) => {
                self.interrupted_capture = true;
                // No send intent exists yet: this is positive local no-send
                // evidence, so it must not become an ambiguous liability.
                self.runtime.block_on(vcp_budget::release_before_send(
                    self.engine.store_mut(),
                    &attempt.id,
                    scope,
                    &actor,
                ))?;
                let _ = self.pause_root("response capture could not open before send");
                return Err(error.into());
            }
        };
        #[cfg(windows)]
        self.coding_stage(
            binding,
            TurnState::RequestingModel,
            "captured request reserved for model admission",
        )?;
        #[cfg(windows)]
        if let Err(error) = self.validate_memory_send(binding) {
            // The source fence won canonical ordering before SendIntent. This
            // is positive no-send evidence, so release rather than charge an
            // uncertain provider liability. No transport permit escapes.
            self.runtime.block_on(vcp_budget::release_before_send(
                self.engine.store_mut(),
                &attempt.id,
                scope,
                &actor,
            ))?;
            return Err(error);
        }
        if let Err(error) = self.runtime.block_on(vcp_budget::submit(
            self.engine.store_mut(),
            &attempt.id,
            scope,
            attempt.revision,
            &actor,
        )) {
            self.interrupted_capture = true;
            let _ = self.pause_root("send admission durability failed");
            return Err(error.into());
        }
        self.streams.insert(attempt.id.clone(), response);
        if let Some(prepared) = provider_request {
            self.provider
                .as_mut()
                .ok_or("provider configuration lost")?
                .streams
                .insert(attempt.id.clone(), prepared.stream);
        }
        let deadline = self
            .provider
            .as_ref()
            .map(|provider| std::time::Instant::now() + provider.timeout);
        #[cfg(windows)]
        let deadline = match (deadline, self.coding_remaining()) {
            (Some(provider), Some(remaining)) => {
                Some(provider.min(std::time::Instant::now() + remaining))
            }
            (other, _) => other,
        };
        let retry = self
            .provider
            .as_mut()
            .and_then(|p| p.retries.remove(&binding.scope.task));
        let (deadline, retries) = retry.map_or((deadline, 0), |r| (Some(r.deadline), r.count));
        #[cfg(windows)]
        self.seed_decision_shadow(binding, &attempt);
        Ok((attempt.id, body, deadline, retries))
    }
    pub fn response_chunk(&mut self, attempt: &AttemptId, bytes: &[u8]) -> Result<()> {
        self.response_error_chunk(attempt, bytes)?;
        if self.provider_required {
            self.provider
                .as_mut()
                .ok_or("provider configuration missing")?
                .streams
                .get_mut(attempt)
                .ok_or("provider stream missing")?
                .push(bytes)?;
        }
        Ok(())
    }
    pub fn response_error_chunk(&mut self, attempt: &AttemptId, bytes: &[u8]) -> Result<()> {
        for chunk in bytes.chunks(vcp_store::artifact::CHUNK_BYTES) {
            if let Err(error) = self
                .streams
                .get_mut(attempt)
                .ok_or("response capture missing")?
                .write_chunk(chunk)
            {
                let _ = self.pause_root("response capture failed");
                return Err(error.into());
            }
        }
        Ok(())
    }
    fn pause_root(&mut self, reason: &str) -> Result<()> {
        let task: Task = self
            .engine
            .store()
            .state()
            .record(
                Collection::Task,
                self.config.root_task.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        if task.state == TaskState::Running {
            self.command(
                Command::Transition {
                    next: TaskState::Paused,
                    reason: reason.into(),
                    verification: None,
                },
                Some(task.scope.task),
                task.revision,
            )?;
        }
        #[cfg(windows)]
        self.stop_coding_turns(reason)?;
        Ok(())
    }
    pub fn initialize_root_budget(&mut self) -> Result<()> {
        if !self.owner_alive {
            return Err("budget initialization requires live owner".into());
        }
        let state = self.engine.store().state();
        if state
            .record(
                Collection::Ledger,
                self.config.root_task.as_str(),
                &self.config.workspace,
            )
            .is_ok()
        {
            return Ok(());
        }
        let root: Task = state
            .record(
                Collection::Task,
                self.config.root_task.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        if root.scope.session != self.config.session || root.parent.is_some() {
            return Err("root budget scope denied".into());
        }
        let actor = self.actor();
        self.runtime.block_on(vcp_budget::initialize(
            self.engine.store_mut(),
            root.scope,
            self.config.cap.clone(),
            self.config.protected,
            None,
            &actor,
        ))?;
        Ok(())
    }
    pub fn pause_all(&mut self, reason: &str) -> Result<()> {
        self.owner_alive = false;
        #[cfg(windows)]
        self.decisions.credentials.close()?;
        let tasks: Vec<Task> = self
            .engine
            .store()
            .state()
            .records
            .values()
            .filter(|row| row.collection == Collection::Task)
            .map(Record::decode)
            .collect::<std::result::Result<_, _>>()?;
        for task in tasks {
            if matches!(task.state, TaskState::Pending | TaskState::Running) {
                self.recovery_pause(task, reason)?;
            }
        }
        #[cfg(windows)]
        self.stop_coding_turns(reason)?;
        Ok(())
    }
    /// The exclusive store owner recovers every session, including one selected
    /// before a crash. External command access remains bound to its session.
    fn recovery_pause(&mut self, task: Task, reason: &str) -> Result<()> {
        if task.scope.workspace != self.config.workspace {
            return Err("recovery workspace denied".into());
        }
        let configured = std::mem::replace(&mut self.config.session, task.scope.session.clone());
        let access = std::mem::replace(&mut self.access.session, task.scope.session.clone());
        let result = (|| {
            self.command(
                Command::Transition {
                    next: TaskState::Paused,
                    reason: reason.into(),
                    verification: None,
                },
                Some(task.scope.task),
                task.revision,
            )?;
            #[cfg(windows)]
            self.stop_coding_turns(reason)?;
            Ok(())
        })();
        self.config.session = configured;
        self.access.session = access;
        result
    }
    pub fn open_output(&mut self, scope: &Scope, channel: Channel) -> Result<ArtifactId> {
        if !matches!(
            channel,
            Channel::Stdout | Channel::Stderr | Channel::ChildTranscript
        ) {
            return Err("invalid streaming output channel".into());
        }
        let spec = self.spec(scope, channel, "retained-full-output/1");
        let id = spec.id.clone();
        let writer = self.engine.store().spool().create(spec)?;
        self.outputs.insert(id.clone(), (scope.clone(), writer));
        Ok(id)
    }
    pub fn output_chunk(&mut self, id: &ArtifactId, bytes: &[u8]) -> Result<()> {
        if let Err(error) = self
            .outputs
            .get_mut(id)
            .ok_or("output capture missing")?
            .1
            .write_chunk(bytes)
        {
            let _ = self.pause_root("tool capture failed");
            return Err(error.into());
        }
        Ok(())
    }
    pub fn finish_output(&mut self, id: &ArtifactId, abort: bool) -> Result<ArtifactDescriptor> {
        let (scope, mut writer) = self.outputs.remove(id).ok_or("output capture missing")?;
        let descriptor = if abort {
            writer.abort()?
        } else {
            writer.finalize()?
        };
        drop(writer);
        self.command(
            Command::AttachArtifact {
                descriptor: descriptor.clone(),
            },
            Some(scope.task),
            Revision::ZERO,
        )?;
        Ok(descriptor)
    }
    pub fn complete(
        &mut self,
        binding: &ThreadBinding,
        attempt: &AttemptId,
        usage: Option<codex_protocol::protocol::TokenUsage>,
        response_id: String,
    ) -> Result<()> {
        if self.provider_required {
            return self.complete_provider(binding, attempt, &response_id);
        }
        let mut writer = self
            .streams
            .remove(attempt)
            .ok_or("response capture missing")?;
        let descriptor = writer.finalize()?;
        drop(writer);
        self.command(
            Command::AttachArtifact {
                descriptor: descriptor.clone(),
            },
            Some(binding.scope.task.clone()),
            Revision::ZERO,
        )?;
        let Some(usage) = usage else {
            return self.unknown(binding, attempt, "terminal response omitted usage");
        };
        let unit = |value: i64| -> Result<Units> { Ok(Units::new(u64::try_from(value)?)) };
        let normalized = Usage {
            input: unit(usage.input_tokens)?,
            cache_read: unit(usage.cached_input_tokens)?,
            cache_write: unit(usage.cache_write_input_tokens)?,
            output: unit(usage.output_tokens)?,
            reasoning: unit(usage.reasoning_output_tokens)?,
            requests: Units::new(1),
            provider_tools: Units::ZERO,
        };
        let admitted = vcp_budget::attempt(
            self.engine.store().state(),
            attempt,
            &binding.scope.workspace,
        )?;
        let actual = vcp_budget::arithmetic::quote(
            admitted.quote.price.clone(),
            normalized,
            Timestamp::ZERO,
        )?;
        let observation = UsageObservation {
            id: ObservationId::new(),
            scope: binding.scope.clone(),
            attempt: attempt.clone(),
            provider_request: response_id,
            mode: UsageMode::Cumulative {
                version: Units::new(1),
            },
            amount: actual.amount,
            final_usage: true,
            raw: descriptor.spec.id,
            correction: None,
        };
        let actor = self.actor();
        self.runtime.block_on(vcp_budget::observe(
            self.engine.store_mut(),
            observation,
            &actor,
        ))?;
        Ok(())
    }
    pub fn unknown(
        &mut self,
        binding: &ThreadBinding,
        attempt: &AttemptId,
        reason: &str,
    ) -> Result<()> {
        self.retain_unknown(binding, attempt, reason, true)
    }
    pub(super) fn retain_unknown(
        &mut self,
        binding: &ThreadBinding,
        attempt: &AttemptId,
        reason: &str,
        pause: bool,
    ) -> Result<()> {
        // Cancellation may arrive after raw final usage but before the retained
        // completion callback. Keep that observation rather than losing a known
        // charge. Parsing failure still preserves an unknown liability.
        let terminal = self
            .provider
            .as_ref()
            .and_then(|p| p.streams.get(attempt))
            .and_then(|p| p.terminal_identity())
            .map(str::to_owned);
        if let Some(response_id) = terminal {
            return self.complete_provider(binding, attempt, &response_id);
        }
        if let Some(provider) = &mut self.provider {
            provider.streams.remove(attempt);
        }
        if let Some(mut writer) = self.streams.remove(attempt) {
            let descriptor = writer.abort()?;
            drop(writer);
            self.command(
                Command::AttachArtifact { descriptor },
                Some(binding.scope.task.clone()),
                Revision::ZERO,
            )?;
        }
        let actor = self.actor();
        self.runtime.block_on(vcp_budget::hold_uncertain(
            self.engine.store_mut(),
            attempt,
            &binding.scope,
            &actor,
            reason,
        ))?;
        if pause {
            self.pause_root("provider outcome requires accounting reconciliation")?;
        }
        Ok(())
    }
}
