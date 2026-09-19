// SPDX-License-Identifier: Apache-2.0
//! Canonical persistence adapter for the retained controller. The worker only
//! serializes storage operations; scheduling and interruption remain in Codex.
mod authority;
mod scheduler;
use scheduler::{EffectLease, Scheduler};
#[cfg(windows)]
pub mod coding;
#[cfg(windows)]
mod execution;
pub mod openrouter;
#[cfg(windows)]
mod process;
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
pub use execution::{PreparedProcess, PreparedProcessOutcome, ProcessProposal};
#[cfg(windows)]
pub use process::{CanonicalProcess, ProcessOutcome};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
#[cfg(windows)]
pub use tools::{ToolOutcome, ToolProposal};
use vcp_domain::{accounting::*, artifact::*, ids::*, revision::*, workspace::*};
use vcp_protocol::command::{Command, CommandReceipt};
use vcp_store::{contract::State, BackendKind};

#[derive(Clone)]
pub struct Config {
    pub canonical_root: PathBuf,
    pub backend: BackendKind,
    pub workspace: WorkspaceId,
    pub session: SessionId,
    pub binding: Binding,
    pub actor: ActorId,
    pub root_task: TaskId,
    pub cap: Money,
    pub protected: Micros,
    pub price: PriceSnapshot,
    pub input_ceiling: Units,
    pub output_ceiling: Units,
    pub artifact_limit: ByteCount,
    /// Explicit trusted host ceilings for native tools. Immutable for this
    /// owner; neither user policy nor restored history can replace these rules.
    pub host_tool_denials: Vec<vcp_domain::policy::Denial>,
}
#[derive(Clone)]
pub struct ThreadBinding {
    pub scope: Scope,
    pub agent: AgentId,
    pub role: RequestRole,
}
#[derive(Clone)]
pub struct CanonicalHost {
    runtime: Lifecycle,
    worker: worker::Worker,
    bindings: Arc<Mutex<HashMap<ThreadId, ThreadBinding>>>,
    scheduler: Arc<Scheduler>,
}
pub struct CanonicalOwner {
    runtime: Option<OwnerLease>,
    worker: worker::Worker,
}
impl CanonicalOwner {
    pub async fn close(mut self) -> Result<(), String> {
        self.worker
            .run_cleanup(|context| context.pause_all("owning host closed"))?;
        if let Some(owner) = self.runtime.take() {
            owner.close().await.map_err(|error| format!("{error:?}"))?;
        }
        Ok(())
    }
}
impl Drop for CanonicalOwner {
    fn drop(&mut self) {
        self.runtime.take();
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
        let worker = worker::Worker::open(config)?;
        let (runtime, owner) = Lifecycle::new(Duration::from_secs(5));
        let owner = CanonicalOwner {
            runtime: Some(owner),
            worker: worker.clone(),
        };
        Ok((
            Self {
                runtime,
                worker,
                bindings: Arc::new(Mutex::new(HashMap::new())),
                scheduler: Arc::new(Scheduler::default()),
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
    pub fn read_artifact(&self, id: ArtifactId) -> Result<Vec<u8>, String> {
        self.worker.run_cleanup(move |context| {
            let mut bytes = Vec::new();
            vcp_audit::history::History::read_artifact(
                context.engine.store(),
                &context.history_access(),
                &id,
                &mut bytes,
            )?;
            Ok(bytes)
        })
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
        let Some(deadline) = self.deadline else {
            return Ok(None);
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
        let binding = self.binding.clone();
        let attempt = self.attempt.clone();
        let count = self.retries;
        let delay = self.worker.run(move |context| {
            context.schedule_retry(&binding, attempt, count, deadline, failure, retry_after_ms)
        })?;
        if delay.is_some() {
            self.runtime.complete()?;
            self.finished = true;
            self.retry_scheduled = true;
        }
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
            for chunk in bytes.chunks(vcp_store::artifact::CHUNK_BYTES) {
                let bytes = chunk.to_vec();
                let attempt = attempt.clone();
                worker
                    .run(move |context| context.response_error_chunk(&attempt, &bytes))
                    .inspect_err(|_| worker.fence())?;
            }
            Ok(())
        }))
    }
    fn complete_model_response(
        &mut self,
        usage: Option<&TokenUsage>,
        response_id: &str,
    ) -> Result<(), String> {
        let usage = usage.cloned();
        let response_id = response_id.to_owned();
        let attempt = self.attempt.clone();
        let binding = self.binding.clone();
        self.worker
            .run(move |context| context.complete(&binding, &attempt, usage, response_id))
            .inspect_err(|_| self.worker.fence())?;
        self.runtime.complete()?;
        self.finished = true;
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
            if self
                .worker
                .run_cleanup(move |context| {
                    context.unknown(&binding, &attempt, "retained response did not complete")
                })
                .is_err()
            {
                self.worker.fence();
            }
        }
    }
}
impl HostWorkAdmission for CanonicalHost {
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
        let mut binding = self.binding(thread)?;
        let generation = self
            .runtime
            .admission_generation(thread)
            .map_err(|e| format!("{e:?}"))?;
        match purpose {
            HostModelPurpose::Compaction => binding.role = RequestRole::Compaction,
            HostModelPurpose::Memory => {
                return Err("memory inference requires governed local adapter".into())
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
        let admission = self
            .worker
            .run(move |context| context.admit(&admitted, input));
        let (attempt, prepared, deadline, retries) = match admission {
            Ok(value) => value,
            Err(error) => {
                runtime.complete()?;
                return Err(error);
            }
        };
        *body = prepared;
        Ok(Box::new(ModelPermit {
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
