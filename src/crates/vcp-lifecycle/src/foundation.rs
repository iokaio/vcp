// SPDX-License-Identifier: Apache-2.0
//! Canonical persistence adapter for the retained controller. The worker only
//! serializes storage operations; scheduling and interruption remain in Codex.
mod authority;
pub mod backup;
#[cfg(windows)]
pub mod backup_checkpoint;
pub mod backup_manager;
pub mod backup_run;
pub mod canonical_tools;
pub mod history_retention;
pub mod execution_diagnostics;
#[cfg(windows)]
pub mod restore_search;
#[cfg(windows)]
pub mod restore_workspace;
mod scheduler;
use scheduler::{EffectLease, Scheduler};
#[cfg(windows)]
pub mod coding;
pub mod conformance;
#[cfg(windows)]
pub mod decision;
#[cfg(windows)]
mod execution;
#[cfg(windows)]
pub mod hooks;
#[cfg(windows)]
pub mod local_memory;
#[cfg(windows)]
pub mod mcp;
#[cfg(windows)]
pub mod memory_inspection;
#[cfg(windows)]
pub mod memory_publication;
#[cfg(windows)]
pub mod memory_query;
#[cfg(windows)]
pub mod memory_query_resources;
#[cfg(windows)]
pub mod memory_vectors;
#[cfg(feature = "qualification")]
pub mod model_dispatch_qualification;
#[cfg(windows)]
pub mod observers;
pub mod openrouter;
#[cfg(windows)]
mod process;
#[cfg(windows)]
mod provider_pacing;
pub mod reconciliation;
pub mod routing;
pub mod routing_state;
pub mod skills;
#[cfg(windows)]
mod tools;
#[cfg(windows)]
pub mod verification;
mod worker;
use crate::{Lifecycle, OwnerLease};
pub use authority::AuthorityChange;
use codex_extension_api::{
    HostModelPurpose, HostResponseCapture, HostWorkAdmission, HostWorkKind, HostWorkPermit,
    TurnStartAdmission,
};
use codex_protocol::{protocol::TokenUsage, ThreadId};
#[cfg(windows)]
pub use execution::{
    DuplexIoLimits, DuplexProcess, PreparedProcess, PreparedProcessOutcome, ProcessProposal,
};
#[cfg(windows)]
pub use process::{CanonicalProcess, ProcessOutcome};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
#[cfg(all(windows, feature = "qualification"))]
pub use tools::FileDispatchPoint;
#[cfg(windows)]
pub use tools::{ToolOutcome, ToolProposal};
use vcp_domain::{accounting::*, artifact::*, ids::*, revision::*, workspace::*};
use vcp_protocol::command::{Command, CommandReceipt};
use vcp_store::{contract::State, BackendKind};
#[cfg(windows)]
pub use worker::agents_cleanup::ChildCleanupPreview;
#[cfg(windows)]
pub use worker::agents_delegate::{DelegationRequest, HelperTemplate};
#[cfg(windows)]
pub use worker::agents_integration::ChildIntegration;
#[cfg(windows)]
pub use worker::agents_owner::{ChildStart, ChildWorkspaceInputs};
#[cfg(windows)]
pub use worker::agents_recovery::{ChildRecovery, ChildRecoveryTicket};
pub use worker::public_connection::{PublicConnection, PublicConnectionLoss, PublicDisconnect};
pub use worker::public_resume::{
    PublicResumeAdmission, PublicResumeOutcome, PublicResumeStartup, PublicResumeTicket,
};

#[cfg(windows)]
pub use worker::public_start::{
    PublicStartAdmission, PublicStartOutcome, PublicStartPreparation, PublicStartStartup,
    PublicStartTicket,
};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub canonical_root: PathBuf,
    pub backend: BackendKind,
    pub workspace: WorkspaceId,
    pub session: SessionId,
    pub binding: Binding,
    pub actor: ActorId,
    pub root_task: TaskId,
    pub cap: vcp_domain::accounting::MonetaryLimit,
    pub protected: Micros,
    pub price: PriceSnapshot,
    pub input_ceiling: Units,
    pub output_ceiling: Units,
    /// Trusted retry ceiling; legacy owner configurations retain two retries.
    #[serde(default = "default_max_transport_retries")]
    pub max_transport_retries: u32,
    pub artifact_limit: ByteCount,
    /// Explicit trusted host ceilings for native tools. Immutable for this
    /// owner; neither user policy nor restored history can replace these rules.
    pub host_tool_denials: Vec<vcp_domain::policy::Denial>,
}
pub fn default_max_transport_retries() -> u32 {
    2
}

#[derive(Clone)]
pub struct ThreadBinding {
    pub scope: Scope,
    pub agent: AgentId,
    pub role: RequestRole,
}
#[derive(Clone)]
pub struct CanonicalHost {
    receipt_source: Arc<Mutex<Option<Arc<reconciliation::ReceiptRuntime>>>>,
    #[cfg(windows)]
    provider_pacing: Arc<Mutex<Option<(provider_pacing::Gate, Duration)>>>,
    local_memory_only: bool,
    memory_owner_alive: Arc<std::sync::atomic::AtomicBool>,
    runtime: Lifecycle,
    worker: worker::Worker,
    bindings: Arc<Mutex<HashMap<ThreadId, ThreadBinding>>>,
    scheduler: Arc<Scheduler>,
    public_identity: Arc<Mutex<Option<(ControllerId, Revision)>>>,
    #[cfg(windows)]
    hook_pending: Arc<Mutex<HashMap<String, hooks::HookProposal>>>,
    #[cfg(windows)]
    hook_registry:
        Arc<Mutex<HashMap<ThreadId, Vec<vcp_extensions::hooks::registry::HookDefinition>>>>,
    public_resume: Arc<Mutex<()>>,
    backup: Arc<Mutex<Option<backup_manager::Loaded>>>,
    #[cfg(windows)]
    mcp: Arc<mcp::Connections>,
}
pub struct CanonicalOwner {
    memory_owner_alive: Arc<std::sync::atomic::AtomicBool>,
    runtime: Option<OwnerLease>,
    worker: worker::Worker,
    #[cfg(windows)]
    mcp: Arc<mcp::Connections>,
}
impl CanonicalOwner {
    pub async fn close(mut self) -> Result<(), String> {
        self.memory_owner_alive
            .store(false, std::sync::atomic::Ordering::Release);
        self.worker
            .run_cleanup(|context| context.pause_all("owning host closed"))?;
        if let Some(owner) = self.runtime.take() {
            owner.close().await.map_err(|error| format!("{error:?}"))?;
        }
        #[cfg(windows)]
        self.mcp.shutdown().await?;
        Ok(())
    }
}
impl Drop for CanonicalOwner {
    fn drop(&mut self) {
        self.memory_owner_alive
            .store(false, std::sync::atomic::Ordering::Release);
        self.runtime.take();
        #[cfg(windows)]
        self.mcp.interrupt();
        if self
            .worker
            .run_cleanup(|context| context.pause_all("owning host dropped"))
            .is_err()
        {
            self.worker.fence();
        }
    }
}
impl std::fmt::Debug for CanonicalHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CanonicalHost").finish_non_exhaustive()
    }
}
pub struct OutputCapture {
    worker: worker::Worker,
    id: ArtifactId,
    finished: bool,
}
impl OutputCapture {
    pub fn write(&self, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() > vcp_store::artifact::CHUNK_BYTES {
            return Err("output chunk exceeds bounded capture buffer".into());
        }
        let id = self.id.clone();
        let bytes = bytes.to_vec();
        self.worker
            .run(move |context| context.output_chunk(&id, &bytes))
            .inspect_err(|_| self.worker.fence())
    }
    pub fn finish(mut self) -> Result<ArtifactDescriptor, String> {
        let id = self.id.clone();
        let result = self
            .worker
            .run(move |context| context.finish_output(&id, false));
        if result.is_ok() {
            self.finished = true;
        } else {
            self.worker.fence();
        }
        result
    }
    pub fn finish_partial(mut self) -> Result<ArtifactDescriptor, String> {
        let id = self.id.clone();
        let result = self
            .worker
            .run_cleanup(move |context| context.finish_output(&id, true));
        if result.is_ok() {
            self.finished = true;
        } else {
            self.worker.fence();
        }
        result
    }
}
impl Drop for OutputCapture {
    fn drop(&mut self) {
        if !self.finished {
            let id = self.id.clone();
            if self
                .worker
                .run_cleanup(move |context| context.finish_output(&id, true))
                .is_err()
            {
                self.worker.fence();
            }
        }
    }
}
impl CanonicalHost {
    /// Read-only phase counters from this owner's already validated store.
    pub fn store_diagnostics(&self) -> Result<vcp_store::StoreDiagnostics, String> {
        self.worker
            .run_cleanup(|context| Ok(context.engine.store().diagnostics().clone()))
    }
    pub fn execution_diagnostics(&self, scope: Scope) -> Result<execution_diagnostics::Snapshot, String> {
        self.worker.run_cleanup(move |context| {
            let task: vcp_domain::task::Task = context.engine.store().state().record(vcp_store::contract::Collection::Task, scope.task.as_str(), &context.config.workspace)?.decode()?;
            if task.scope != scope || task.redaction.is_some() { return Err("diagnostic scope unavailable".into()); }
            Ok(context.diagnostics.snapshot().for_scope(&scope))
        })
    }
    fn diagnostic_span(&self, thread: ThreadId, phase: execution_diagnostics::Phase) -> Result<execution_diagnostics::Span, String> {
        let binding = self.binding(thread)?;
        self.worker.run_cleanup(move |context| Ok(context.begin_diagnostic(&binding, phase, None)))
    }
    /// Persist the explicitly admitted root cap before a CLI can detach or
    /// select another task. Reopening never replaces an existing ledger cap.
    pub fn initialize_root_budget(&self) -> Result<(), String> {
        self.worker.run(|context| context.initialize_root_budget())
    }
    /// Record this intentional execution's effective constraints and executable
    /// identity. Historical accepted requests remain unchanged.
    pub fn configure_execution_constraints(
        &self,
        deadline: vcp_domain::Limit<Timestamp>,
    ) -> Result<(), String> {
        self.worker
            .run(move |context| context.configure_execution_constraints(deadline))
    }
    /// Enable the explicit OpenRouter contract for every retained request.
    /// Reopening an enabled store requires fresh configuration and context.
    pub fn configure_provider(
        &self,
        snapshot: vcp_models::catalog::Snapshot,
        raw_catalog: Vec<u8>,
    ) -> Result<(), String> {
        self.configure_provider_with_timeout(snapshot, raw_catalog, Duration::from_secs(120))
    }
    pub fn configure_provider_with_timeout(
        &self,
        snapshot: vcp_models::catalog::Snapshot,
        raw_catalog: Vec<u8>,
        timeout: Duration,
    ) -> Result<(), String> {
        self.worker
            .run(move |context| context.configure_provider(snapshot, raw_catalog, timeout))
    }
    pub fn context_revisions(
        &self,
        id: ThreadId,
    ) -> Result<vcp_context::manifest::Revisions, String> {
        let binding = self.binding(id)?;
        self.worker
            .run(move |context| context.context_revisions(&binding))
    }
    /// The host re-resolves captured bytes under current canonical access; a
    /// serialized manifest alone cannot become a transport capability.
    pub fn prepare_context(
        &self,
        id: ThreadId,
        sealed: vcp_context::manifest::Sealed,
        schemas: serde_json::Value,
        roots: Vec<vcp_repository::Root>,
    ) -> Result<(), String> {
        let binding = self.binding(id)?;
        self.worker
            .run(move |context| context.prepare_context(&binding, sealed, schemas, roots))
    }
    pub fn open(config: Config) -> Result<(Self, CanonicalOwner), String> {
        Self::open_selected(config, None)
    }
    /// Validate a discovery selection under the exclusive store lock before
    /// recovery can change its revision. The same lock is retained by the owner.
    pub fn open_selected(
        config: Config,
        expected: Option<Revision>,
    ) -> Result<(Self, CanonicalOwner), String> {
        let worker = worker::Worker::open(config, expected)?;
        Self::from_worker(worker)
    }
    /// Transfer the already validated selection store without releasing its
    /// exclusive lock or replaying history. Recovery uses the normal owner path.
    pub fn open_owned_selected(
        config: Config,
        store: vcp_store::Store,
        expected: Revision,
    ) -> Result<(Self, CanonicalOwner), String> {
        Self::from_worker(worker::Worker::open_owned_selected(
            config, store, expected,
        )?)
    }
    fn from_worker(worker: worker::Worker) -> Result<(Self, CanonicalOwner), String> {
        // Recovery may fence startup on an acknowledged aborted response. Replay
        // already captured authoritative receipts under the exclusive owner
        // before binding a retained thread; registration cannot precede this
        // settlement. This performs no metadata GET or provider submission.
        worker.run_cleanup(|context| {
            context.replay_provider_receipts()?;
            Ok(())
        })?;
        let (runtime, owner) = Lifecycle::new(Duration::from_secs(5));
        let memory_owner_alive = Arc::new(std::sync::atomic::AtomicBool::new(true));
        #[cfg(windows)]
        let mcp = Arc::new(mcp::Connections::default());
        let owner = CanonicalOwner {
            memory_owner_alive: memory_owner_alive.clone(),
            runtime: Some(owner),
            worker: worker.clone(),
            #[cfg(windows)]
            mcp: mcp.clone(),
        };
        Ok((
            Self {
                receipt_source: Arc::new(Mutex::new(None)),
                #[cfg(windows)]
                provider_pacing: Arc::new(Mutex::new(None)),
                local_memory_only: false,
                memory_owner_alive,
                runtime,
                worker,
                bindings: Arc::new(Mutex::new(HashMap::new())),
                scheduler: Arc::new(Scheduler::default()),
                public_identity: Arc::new(Mutex::new(None)),
                #[cfg(windows)]
                hook_pending: Arc::new(Mutex::new(HashMap::new())),
                #[cfg(windows)]
                hook_registry: Arc::new(Mutex::new(HashMap::new())),
                public_resume: Arc::new(Mutex::new(())),
                #[cfg(windows)]
                mcp,
                backup: Arc::new(Mutex::new(None)),
            },
            owner,
        ))
    }
    pub fn lifecycle(&self) -> &Lifecycle {
        &self.runtime
    }
    pub fn snapshot(&self) -> Result<State, String> {
        self.worker
            .run_cleanup(|context| Ok(context.engine.store().state().clone()))
    }
    /// Current records only. This read does not dispatch work or copy retained
    /// event/receipt payloads; its watermark is not an admission capability.
    pub fn current_state(&self) -> Result<Arc<vcp_store::CurrentState>, String> {
        self.worker
            .run_cleanup(|context| Ok(context.engine.store().current_state()))
    }
    /// Explicit bounded local maintenance, never called by read-only inspection.
    pub fn maintain_memory(&self) -> Result<vcp_memory::runner::Progress, String> {
        self.worker.run(|context| context.memory_step())
    }
    pub fn memory_status(&self) -> Result<vcp_memory::runner::Status, String> {
        self.worker.run_cleanup(|context| {
            Ok(vcp_memory::runner::status(
                context.engine.store(),
                &context.memory_access(),
            )?)
        })
    }
    /// Promote an already captured, accounted Memory-role gateway response.
    /// Validation and governed writes stay under the canonical owner fence.
    pub fn promote_memory_response(
        &self,
        extraction: vcp_memory::extraction::ExtractionContext,
    ) -> Result<Vec<vcp_memory::repository::MemoryCommit>, String> {
        self.worker
            .run(move |context| context.promote_memory_response(extraction))
    }
    pub fn observe_usage(&self, observation: UsageObservation) -> Result<Settlement, String> {
        self.worker
            .run_cleanup(move |context| context.observe_usage(observation))
    }
    pub fn unfinished_captures(&self) -> Result<Vec<ArtifactDescriptor>, String> {
        self.worker
            .run_cleanup(|context| Ok(context.engine.store().spool().unfinished()?))
    }
    pub fn command(
        &self,
        command: Command,
        task: Option<TaskId>,
        expected: Revision,
    ) -> Result<CommandReceipt, String> {
        self.command_checked(command, task, expected)
    }
    pub fn register(&self, id: ThreadId, binding: ThreadBinding) -> Result<(), String> {
        let checked = binding.clone();
        self.worker.run(move |context| {
            if context.capture_admission_blocked() {
                return Err("canonical capture/admission fenced; reopen required".into());
            }
            context.validate_binding(&checked)?;
            Ok(())
        })?;
        let mut bindings = self.bindings.lock().map_err(|_| "binding lock poisoned")?;
        if bindings.contains_key(&id) {
            return Err("retained thread already bound".into());
        }
        bindings.insert(id, binding);
        Ok(())
    }
    fn binding(&self, id: ThreadId) -> Result<ThreadBinding, String> {
        self.bindings
            .lock()
            .map_err(|_| "binding lock poisoned")?
            .get(&id)
            .cloned()
            .ok_or_else(|| "retained thread has no canonical scope".into())
    }
    pub fn inspect(
        &self,
        query: vcp_audit::inspection::InspectionQuery,
    ) -> Result<vcp_audit::inspection::InspectionPage, String> {
        self.worker.run_cleanup(move |context| {
            Ok(context.runtime.block_on(vcp_audit::inspection::inspect(
                context.engine.store(),
                &context.history_access(),
                &query,
            ))?)
        })
    }
    pub fn read_artifact(&self, id: ArtifactId) -> Result<Vec<u8>, String> {
        self.worker.run_cleanup(move |context| {
            let mut bytes = Vec::new();
            context
                .runtime
                .block_on(vcp_audit::history::History::read_artifact(
                    context.engine.store(),
                    &context.history_access(),
                    &id,
                    &mut bytes,
                ))?;
            Ok(bytes)
        })
    }

    /// Reporting-only proof that a pending charge belongs to a validated complete
    /// response. Missing evidence never authorizes execution or resolves billing.
    pub fn completed_financial_uncertainty(&self, attempt: Attempt) -> Result<bool, String> {
        self.worker
            .run_cleanup(move |context| context.completed_financial_uncertainty(&attempt))
    }
    pub fn project(&self) -> Result<vcp_audit::projection::View, String> {
        self.worker.run(|context| {
            let workspace = context.config.workspace.clone();
            Ok(context.runtime.block_on(vcp_audit::projection::publish(
                context.engine.store_mut(),
                &workspace,
                2,
            ))?)
        })
    }
    /// Qualification and host adapters share the same full-output capture path.
    pub fn capture(
        &self,
        id: ThreadId,
        channel: Channel,
        bytes: Vec<u8>,
    ) -> Result<ArtifactDescriptor, String> {
        let binding = self.binding(id)?;
        self.worker.run(move |context| {
            context.capture(&binding.scope, channel, &bytes, "retained-output/1")
        })
    }
    pub fn open_output(&self, id: ThreadId, channel: Channel) -> Result<OutputCapture, String> {
        let binding = self.binding(id)?;
        let id = self
            .worker
            .run(move |context| context.open_output(&binding.scope, channel))?;
        Ok(OutputCapture {
            worker: self.worker.clone(),
            id,
            finished: false,
        })
    }
    pub fn resume(
        &self,
        id: ThreadId,
        expected: Revision,
        fingerprint: vcp_domain::verification::Fingerprint,
    ) -> Result<CommandReceipt, String> {
        let binding = self.binding(id)?;
        let view = self
            .runtime
            .inspect(id)
            .map_err(|error| format!("{error:?}"))?;
        if !view.owner_attached || view.local_hold || view.inherited_hold {
            return Err("retained owner is not ready for canonical resume".into());
        }
        #[cfg(windows)]
        if self.mcp_connections_present() {
            return self.resume_mcp_approval(id, binding, expected, fingerprint);
        }
        self.worker
            .run(move |context| context.resume(&binding, expected, fingerprint))
    }
}
impl TurnStartAdmission for CanonicalHost {
    fn admit_turn_start(&self) -> Option<Box<dyn Send>> {
        None
    }
    fn admit_continuation_start(&self) -> Option<Box<dyn Send>> {
        None
    }
    fn admit_turn_start_for_thread(&self, id: ThreadId) -> Option<Box<dyn Send>> {
        if self.worker.fenced() {
            return None;
        }
        let binding = self.binding(id).ok()?;
        self.worker
            .run(move |context| context.can_start(&binding))
            .ok()?;
        self.runtime.admit_turn_start_for_thread(id)
    }
    fn admit_continuation_start_for_thread(&self, id: ThreadId) -> Option<Box<dyn Send>> {
        self.admit_turn_start_for_thread(id)
    }
}
struct ModelPermit {
    diagnostic: Option<execution_diagnostics::Span>,
    #[cfg(windows)]
    provider_slot: Option<provider_pacing::Slot>,
    host: CanonicalHost,
    thread: ThreadId,
    purpose: HostModelPurpose,
    generation: u64,
    retries: u32,
    retry_scheduled: bool,
    worker: worker::Worker,
    binding: ThreadBinding,
    attempt: AttemptId,
    runtime: Box<dyn HostWorkPermit>,
    finished: bool,
    deadline: Option<std::time::Instant>,
}
impl HostWorkPermit for ModelPermit {
    fn response_generation_identity(&mut self, identity: &str) -> Result<(), String> {
        let attempt = self.attempt.clone();
        let identity = identity.to_owned();
        self.worker
            .run(move |context| context.record_failed_generation_header(&attempt, identity))
    }
    fn retry_delay(
        &mut self,
        failure: codex_extension_api::HostModelFailure,
        retry_after_ms: Option<u64>,
    ) -> Result<Option<Duration>, String> {
        if self.finished
            || self.host.runtime.admission_generation(self.thread).ok() != Some(self.generation)
        {
            return Ok(None);
        }
        if let Some(span) = self.diagnostic.take() { span.finish(&Err::<(), ()>(())); }
        let deadline = self.deadline;
        let http_status = match &failure {
            codex_extension_api::HostModelFailure::Http(status) => Some(*status),
            _ => None,
        };
        let failure = match failure {
            codex_extension_api::HostModelFailure::Http(status) => {
                vcp_models::retry::http_failure(status)
            }
            codex_extension_api::HostModelFailure::Timeout => vcp_models::retry::Failure::Timeout,
            codex_extension_api::HostModelFailure::Transport => {
                vcp_models::retry::Failure::Transient
            }
            codex_extension_api::HostModelFailure::Protocol => vcp_models::retry::Failure::Protocol,
        };
        #[cfg(windows)]
        let cooldown = {
            let attempt = self.attempt.clone();
            let binding = self.binding.clone();
            let (source, route) = self.worker.run(move |context| {
                Ok((
                    context.provider_limit_source(&attempt),
                    context.provider_attempt_endpoint(&binding, &attempt)?,
                ))
            })?;
            if let Some(slot) = self.provider_slot.as_mut() {
                slot.bind_failed_route(route)?;
            }
            self.provider_slot.as_ref().map_or(Ok(()), |slot| {
                slot.failed(
                    failure,
                    source,
                    Duration::from_millis(
                        retry_after_ms
                            .filter(|ms| *ms != u64::MAX)
                            .unwrap_or(5_000)
                            .max(5_000),
                    ),
                )
            })
        };
        // Availability leases and canonical billing uncertainty are independent.
        #[cfg(windows)]
        self.provider_slot.take();
        let binding = self.binding.clone();
        let attempt = self.attempt.clone();
        let count = self.retries;
        let delay = self.worker.run(move |context| {
            context.schedule_retry(
                &binding,
                attempt,
                count,
                deadline,
                failure,
                http_status,
                retry_after_ms,
            )
        })?;
        if delay.is_some() {
            self.runtime.complete()?;
            self.finished = true;
            self.retry_scheduled = true;
        }
        // Preserve the actual HTTP failure and pending-retry cleanup even if
        // coordination could not persist. Such an error denies another send.
        #[cfg(windows)]
        cooldown?;
        Ok(delay)
    }
    fn retry_current(&self) -> bool {
        if !self.retry_scheduled
            || self.host.runtime.admission_generation(self.thread).ok() != Some(self.generation)
        {
            return false;
        }
        let binding = self.binding.clone();
        let attempt = self.attempt.clone();
        self.worker
            .run(move |context| context.retry_current(&binding, &attempt))
            .is_ok()
    }
    fn admit_retry(
        &mut self,
        body: &mut serde_json::Value,
    ) -> Result<Box<dyn HostWorkPermit>, String> {
        if !self.retry_current() {
            return Err("provider retry cancelled by current owner/context/deadline".into());
        }
        self.host.admit_model(self.thread, body, self.purpose)
    }
    fn admit_retry_async<'a>(
        &'a mut self,
        body: &'a mut serde_json::Value,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Box<dyn HostWorkPermit>, String>> + Send + 'a>,
    > {
        Box::pin(async move {
            if !self.retry_current() {
                return Err("provider retry cancelled by current owner/context/deadline".into());
            }
            #[cfg(windows)]
            {
                let _ = self.host.reconcile_pending(self.thread).await?;
                let slot = self
                    .host
                    .acquire_provider_slot(self.thread, Some(self.purpose), self.deadline)
                    .await?;
                if !self.retry_current() {
                    return Err("provider retry cancelled by current owner/context/deadline".into());
                }
                return self
                    .host
                    .admit_model_with_slot(self.thread, body, self.purpose, slot);
            }
            #[cfg(not(windows))]
            self.admit_retry(body)
        })
    }
    fn response_deadline(&self) -> Option<std::time::Instant> {
        self.deadline
    }
    fn complete(&mut self) -> Result<(), String> {
        Err("model completion requires response identity and usage".into())
    }
    fn response_capture(&self) -> Option<HostResponseCapture> {
        let worker = self.worker.clone();
        let attempt = self.attempt.clone();
        Some(Arc::new(move |bytes| {
            for chunk in bytes.chunks(vcp_store::artifact::CHUNK_BYTES) {
                let bytes = chunk.to_vec();
                let attempt = attempt.clone();
                worker
                    .run(move |context| context.response_chunk(&attempt, &bytes))
                    .inspect_err(|_| worker.fence())?;
            }
            Ok(())
        }))
    }
    fn response_error_capture(&self) -> Option<HostResponseCapture> {
        let worker = self.worker.clone();
        let attempt = self.attempt.clone();
        Some(Arc::new(move |bytes| {
            // Qualify metadata from the complete bounded HTTP body, never from
            // artifact chunks that could hide an invalid prefix or suffix.
            let limit_source = vcp_models::retry::error_limit_source(bytes);
            for chunk in bytes.chunks(vcp_store::artifact::CHUNK_BYTES) {
                let bytes = chunk.to_vec();
                let attempt = attempt.clone();
                worker
                    .run(move |context| context.response_error_chunk(&attempt, &bytes))
                    .inspect_err(|_| worker.fence())?;
            }
            let attempt = attempt.clone();
            worker
                .run(move |context| context.response_http_error_source(&attempt, limit_source))
                .inspect_err(|_| worker.fence())?;
            Ok(())
        }))
    }
    fn complete_model_response(
        &mut self,
        usage: Option<&TokenUsage>,
        response_id: &str,
    ) -> Result<(), String> {
        #[cfg(windows)]
        let scheduling_usage = usage.and_then(|usage| {
            Some(Usage {
                input: vcp_domain::Units::new(u64::try_from(usage.input_tokens).ok()?),
                output: vcp_domain::Units::new(u64::try_from(usage.output_tokens).ok()?),
                cache_read: vcp_domain::Units::new(u64::try_from(usage.cached_input_tokens).ok()?),
                cache_write: vcp_domain::Units::new(
                    u64::try_from(usage.cache_write_input_tokens).ok()?,
                ),
                requests: vcp_domain::Units::new(1),
                ..Usage::default()
            })
        });
        let usage = usage.cloned();
        let response_id = response_id.to_owned();
        let attempt = self.attempt.clone();
        let binding = self.binding.clone();
        let result = self.worker
            .run(move |context| context.complete(&binding, &attempt, usage, response_id))
            .inspect_err(|_| self.worker.fence());
        if let Some(span) = self.diagnostic.take() { span.finish(&result); }
        result?;
        self.runtime.complete()?;
        self.finished = true;
        #[cfg(windows)]
        {
            if let Some(slot) = &self.provider_slot {
                slot.succeeded(scheduling_usage.as_ref())?;
            }
            self.provider_slot.take();
        }
        Ok(())
    }
}
impl Drop for ModelPermit {
    fn drop(&mut self) {
        if self.retry_scheduled {
            let attempt = self.attempt.clone();
            let binding = self.binding.clone();
            if self
                .worker
                .run_cleanup(move |context| context.cancel_retry(&binding, &attempt))
                .is_err()
            {
                self.worker.fence();
            }
        }
        if !self.finished {
            let attempt = self.attempt.clone();
            let binding = self.binding.clone();
            let runtime = self.host.runtime.clone();
            let thread = self.thread;
            if self
                .worker
                .run_cleanup(move |context| {
                    // Only an independently held, still-owned child may keep
                    // this expected interruption local. Canonical stop and root
                    // state are checked again on the accounting worker.
                    let independently_held = runtime.inspect(thread).is_ok_and(|view| {
                        view.owner_attached && view.local_hold && !view.inherited_hold
                    });
                    context.incomplete_retained_response(&binding, &attempt, independently_held)
                })
                // Dropping the response permit terminates this local producer.
                // Its missing provider receipt remains an uncertain canonical
                // liability; that is distinct from live retained work. Record
                // producer termination only after that liability is durable.
                .and_then(|()| self.runtime.complete())
                .is_err()
            {
                self.worker.fence();
            }
        }
    }
}
impl HostWorkAdmission for CanonicalHost {
    fn prepare_model(
        &self,
        thread: ThreadId,
        purpose: HostModelPurpose,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async move {
            #[cfg(windows)]
            if purpose == HostModelPurpose::Turn {
                let _ = self.poll_observers(thread).await;
                self.model_lifecycle_hooks(thread).await?;
            }
            #[cfg(not(windows))]
            let _ = (thread, purpose);
            Ok(())
        })
    }
    #[cfg(windows)]
    fn admit_tool(
        &self,
        thread: ThreadId,
        call_id: &str,
        name: &codex_extension_api::ToolName,
    ) -> Result<Box<dyn HostWorkPermit>, String> {
        let binding = self.binding(thread)?;
        let call_id = call_id.to_owned();
        let name = name.clone();
        self.worker
            .run(move |context| context.admit_coding_tool(&binding, &call_id, &name))?;
        HostWorkAdmission::admit(
            &self.runtime,
            thread,
            HostWorkKind::Tool,
            "canonical-coding-wrapper",
        )
    }
    fn requires_completed_response(&self) -> bool {
        true
    }
    fn admit_startup(
        &self,
        workspace: &Path,
        resumed: Option<ThreadId>,
    ) -> Result<Box<dyn Send>, String> {
        if self.worker.fenced() {
            return Err("canonical host fenced".into());
        }
        self.public_startup_admission()?;
        self.runtime.admit_startup(workspace, resumed)
    }
    fn admit(
        &self,
        _thread: ThreadId,
        _kind: HostWorkKind,
        _label: &str,
    ) -> Result<Box<dyn HostWorkPermit>, String> {
        Err("canonical host requires captured model body or prepared tool authority".into())
    }
    fn admit_model(
        &self,
        thread: ThreadId,
        body: &mut serde_json::Value,
        purpose: HostModelPurpose,
    ) -> Result<Box<dyn HostWorkPermit>, String> {
        #[cfg(windows)]
        {
            if self
                .provider_pacing
                .lock()
                .map_err(|_| "provider pacing poisoned")?
                .is_some()
            {
                return Err("configured provider pacing requires async admission".into());
            }
            self.admit_model_with_slot(thread, body, purpose, None)
        }
        #[cfg(not(windows))]
        self.admit_model_unpaced(thread, body, purpose)
    }
    fn admit_model_async<'a>(
        &'a self,
        thread: ThreadId,
        body: &'a mut serde_json::Value,
        purpose: HostModelPurpose,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Box<dyn HostWorkPermit>, String>> + Send + 'a>,
    > {
        Box::pin(async move {
            #[cfg(windows)]
            {
                let deadline = if self
                    .provider_pacing
                    .lock()
                    .map_err(|_| "provider pacing poisoned")?
                    .is_some()
                {
                    let binding = self.binding(thread)?;
                    let remaining = self
                        .worker
                        .run(move |context| context.provider_queue_remaining(&binding))?;
                    remaining.map(|remaining| std::time::Instant::now() + remaining)
                } else {
                    None
                };
                let _ = self.reconcile_pending(thread).await?;
                let slot = self
                    .acquire_provider_slot(thread, Some(purpose), deadline)
                    .await?;
                return self.admit_model_with_slot(thread, body, purpose, slot);
            }
            #[cfg(not(windows))]
            self.admit_model(thread, body, purpose)
        })
    }
}
impl CanonicalHost {
    #[cfg(windows)]
    pub fn configure_provider_pacing(
        &self,
        root: std::path::PathBuf,
        timeout: Duration,
    ) -> Result<(), String> {
        let gate = provider_pacing::Gate::new(root)?;
        let mut configured = self
            .provider_pacing
            .lock()
            .map_err(|_| "provider pacing poisoned")?;
        if configured.is_some() {
            return Err("provider pacing is already configured".into());
        }
        *configured = Some((gate, timeout));
        Ok(())
    }
    #[cfg(windows)]
    async fn acquire_provider_slot(
        &self,
        thread: ThreadId,
        purpose: Option<HostModelPurpose>,
        deadline: Option<std::time::Instant>,
    ) -> Result<Option<provider_pacing::Slot>, String> {
        let configured = self
            .provider_pacing
            .lock()
            .map_err(|_| "provider pacing poisoned")?
            .clone();
        let Some((gate, timeout)) = configured else {
            return Ok(None);
        };
        let mut binding = self.binding(thread)?;
        if purpose == Some(HostModelPurpose::Compaction) {
            binding.role = RequestRole::Compaction;
        }
        let generation = self
            .runtime
            .admission_generation(thread)
            .map_err(|e| format!("{e:?}"))?;
        let remaining = self.worker.run({
            let binding = binding.clone();
            move |context| context.provider_queue_remaining(&binding)
        })?;
        let queue_deadline =
            remaining.map(|remaining| std::time::Instant::now() + timeout.min(remaining));
        let deadline = match (deadline, queue_deadline) {
            (Some(one), Some(two)) => Some(one.min(two)),
            (one, two) => one.or(two),
        };
        let routes = if purpose.is_some() {
            self.worker.run({
                let binding = binding.clone();
                move |context| context.prepare_rotation_routes(&binding)
            })?
        } else {
            None
        };
        let current = || {
            self.runtime.admission_generation(thread).ok() == Some(generation)
                && self
                    .worker
                    .run({
                        let binding = binding.clone();
                        move |context| context.can_start(&binding)
                    })
                    .is_ok()
        };
        let queued = std::time::Instant::now();
        let mut slot = if let Some(mut routes) = routes {
            routes.deadline = deadline;
            loop {
                let sweep = std::time::Instant::now() + Duration::from_secs(2);
                let sweep_deadline = deadline.map_or(sweep, |deadline| sweep.min(deadline));
                match gate.acquire_routes(&routes, sweep_deadline, &current).await {
                    Ok(mut slot) => {
                        slot.deadline = deadline;
                        break slot;
                    }
                    Err(error)
                        if error == "provider rotation deadline expired before submission"
                            && deadline.is_none_or(|deadline| std::time::Instant::now() < deadline) =>
                    {
                        let _ = self.reconcile_pending(thread).await?;
                    }
                    Err(error) => return Err(error),
                }
            }
        } else {
            gate.acquire(deadline, current).await?
        };
        slot.queued_for(queued.elapsed())?;
        if let Some(selection) = slot.selection() {
            self.worker
                .run(move |context| context.select_rotation_route(&binding, selection))?;
        }
        Ok(Some(slot))
    }
    #[cfg(windows)]
    fn admit_model_with_slot(
        &self,
        thread: ThreadId,
        body: &mut serde_json::Value,
        purpose: HostModelPurpose,
        slot: Option<provider_pacing::Slot>,
    ) -> Result<Box<dyn HostWorkPermit>, String> {
        self.admit_model_inner(thread, body, purpose, slot)
    }
    #[cfg(not(windows))]
    fn admit_model_unpaced(
        &self,
        thread: ThreadId,
        body: &mut serde_json::Value,
        purpose: HostModelPurpose,
    ) -> Result<Box<dyn HostWorkPermit>, String> {
        self.admit_model_inner(thread, body, purpose)
    }
    fn admit_model_inner(
        &self,
        thread: ThreadId,
        body: &mut serde_json::Value,
        purpose: HostModelPurpose,
        #[cfg(windows)] slot: Option<provider_pacing::Slot>,
    ) -> Result<Box<dyn HostWorkPermit>, String> {
        let mut binding = self.binding(thread)?;
        let generation = self
            .runtime
            .admission_generation(thread)
            .map_err(|e| format!("{e:?}"))?;
        match purpose {
            HostModelPurpose::Compaction => binding.role = RequestRole::Compaction,
            HostModelPurpose::Memory => {
                return Err("memory inference requires governed local adapter".into());
            }
            HostModelPurpose::Turn => {}
        }
        let mut runtime = HostWorkAdmission::admit(
            &self.runtime,
            thread,
            HostWorkKind::Model,
            "canonical-responses",
        )?;
        let input = body.clone();
        let admitted = binding.clone();
        #[cfg(windows)]
        let queue_deadline = slot.as_ref().and_then(|slot| slot.deadline);
        #[cfg(not(windows))]
        let queue_deadline = None;
        let admission = self.worker.run(move |context| {
            context.set_provider_queue_deadline(&admitted, queue_deadline)?;
            context.admit(&admitted, input)
        });
        let (attempt, prepared, mut deadline, retries) = match admission {
            Ok(value) => value,
            Err(error) => {
                runtime.complete()?;
                return Err(error);
            }
        };
        #[cfg(windows)]
        if let Some(slot) = &slot {
            slot.mark_admitted();
            if let Some(queued) = slot.deadline {
                deadline = Some(deadline.map_or(queued, |value| value.min(queued)));
            }
        }
        *body = prepared;
        let diagnostic_binding = binding.clone();
        let diagnostic_attempt = attempt.clone();
        let diagnostic = self.worker.run_cleanup(move |context| Ok(context.begin_diagnostic(&diagnostic_binding, execution_diagnostics::Phase::ProviderExchange, Some(diagnostic_attempt)))).ok();
        Ok(Box::new(ModelPermit {
            diagnostic,
            #[cfg(windows)]
            provider_slot: slot,
            host: self.clone(),
            thread,
            purpose,
            generation,
            retries,
            retry_scheduled: false,
            worker: self.worker.clone(),
            binding,
            attempt,
            runtime,
            finished: false,
            deadline,
        }))
    }
}
