// SPDX-License-Identifier: Apache-2.0
//! Sequential governed stdio MCP. A process lifetime is not a tool-call receipt.
use super::*;
use std::collections::BTreeSet;
use vcp_context::manifest::{Revisions, VerifiedContext};
use vcp_domain::effect::{Effect, EffectState};
use vcp_extensions::mcp as vcp_mcp;
use vcp_mcp::{
    client::{Client, Incoming},
    registration::Limits,
};
use vcp_store::contract::Collection;
pub(in crate::foundation) mod content;
use content::PreparedOperation;
pub mod remote;
pub mod remote_authority;
pub use remote::RemoteRegistration;
pub(crate) mod transport;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub name: String,
    pub process: vcp_tools::process::Request,
    pub allowed_tools: BTreeSet<String>,
    #[serde(default)]
    pub allowed_resources: BTreeSet<String>,
    #[serde(default)]
    pub allowed_prompts: BTreeSet<String>,
    pub limits: Limits,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    List {
        server: String,
    },
    Call {
        server: String,
        tool: String,
        identity_digest: String,
        arguments_json: String,
    },
    Resources {
        server: String,
    },
    ReadResource {
        server: String,
        uri: String,
        identity_digest: String,
    },
    Prompts {
        server: String,
    },
    GetPrompt {
        server: String,
        prompt: String,
        identity_digest: String,
        arguments_json: String,
    },
    ReadCached {
        server: String,
        artifact: ArtifactId,
    },
    Disconnect {
        server: String,
    },
}
impl Request {
    fn server(&self) -> &str {
        match self {
            Self::List { server }
            | Self::Call { server, .. }
            | Self::Resources { server }
            | Self::ReadResource { server, .. }
            | Self::Prompts { server }
            | Self::GetPrompt { server, .. }
            | Self::ReadCached { server, .. }
            | Self::Disconnect { server } => server,
        }
    }
}

#[derive(Clone)]
pub struct Provenance {
    pub(crate) context: Option<Arc<VerifiedContext>>,
    pub(super) memory: Option<memory_query::SendFence>,
    pub(crate) roots: Vec<vcp_repository::Root>,
    pub(crate) revisions: Option<Revisions>,
    pub(crate) owner_arguments: Option<String>,
}
impl Provenance {
    pub(super) fn from_context(
        context: VerifiedContext,
        roots: Vec<vcp_repository::Root>,
        memory: Option<memory_query::SendFence>,
    ) -> Self {
        Self {
            revisions: Some(context.sealed().manifest.revisions.clone()),
            context: Some(Arc::new(context)),
            memory,
            roots,
            owner_arguments: None,
        }
    }
    pub(crate) fn evidence(&self) -> Result<serde_json::Value, String> {
        Ok(
            serde_json::json!({"revisions":self.revisions,"context_manifest":self.context.as_ref().map(|c|c.sealed().manifest_digest()),
            "source_artifacts":self.context.as_ref().map(|c|c.sealed().manifest.included.iter().map(|p|&p.artifact).collect::<Vec<_>>()),
            "owner_input_sha256":self.owner_arguments.as_ref().map(|s|vcp_protocol::digest_bytes(s.as_bytes()))}),
        )
    }
}
#[derive(Default)]
pub(super) struct Connections {
    credentials: remote_authority::CredentialResolver,
    slots: Mutex<HashMap<(ThreadId, String), SharedSlot>>,
}
type SharedSlot = Arc<tokio::sync::Mutex<Slot>>;
#[derive(Default)]
struct Slot {
    remote: remote::RemoteSlot,
    startup: Option<PendingStartup>,
    connection: Option<Connection>,
    pending: Option<McpProposal>,
}
struct PendingStartup {
    ticket: ProcessProposal,
    unsent: scheduler::QueuedEffect,
}
impl Connections {
    pub(super) async fn shutdown(&self) -> Result<(), String> {
        let slots = self
            .slots
            .lock()
            .map_err(|_| "MCP map poisoned")?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for slot in slots {
            let mut slot = slot.lock().await;
            slot.remote.clear();
            slot.pending.take();
            slot.startup.take();
            if let Some(connection) = slot.connection.take() {
                stop_connection(connection).await;
            }
        }
        self.credentials.close().map_err(|e| e.to_string())?;
        Ok(())
    }
    pub(super) fn interrupt(&self) {
        if let Ok(slots) = self.slots.lock() {
            for slot in slots.values() {
                if let Ok(mut slot) = slot.try_lock() {
                    slot.remote.clear();
                    slot.connection.take();
                    slot.pending.take();
                    slot.startup.take();
                }
            }
        }
    }
}
struct Connection {
    process: DuplexProcess,
    client: Client,
    registration: vcp_mcp::registration::Registration,
    prepared: Arc<vcp_tools::process::Prepared>,
    evidence: ArtifactId,
    cache: std::collections::BTreeMap<ArtifactId, content::CachedResource>,
}
pub(super) struct ResumeProof {
    pub effect: ToolRunId,
    pub execution: ExecutionId,
    pub job: Arc<codex_utils_pty::JobObject>,
    pub process: Arc<vcp_tools::process::Prepared>,
    pub call: ToolRunId,
    pub authority: Arc<vcp_policy::Prepared>,
    pub approval: ApprovalId,
    pub provenance: Provenance,
    pub registration_digest: String,
    pub server: String,
    pub controller: ControllerId,
    pub owner: OwnerEpoch,
    pub generation: u64,
}
pub struct McpProposal {
    unsent: scheduler::QueuedEffect,
    thread: ThreadId,
    binding: ThreadBinding,
    generation: u64,
    controller: ControllerId,
    owner: OwnerEpoch,
    server: String,
    operation: PreparedOperation,
    authority: Arc<vcp_policy::Prepared>,
    process: Arc<vcp_tools::process::Prepared>,
    provenance: Provenance,
    effect: ToolRunId,
    plan: ArtifactId,
    request_key: String,
    pub decision: vcp_policy::Decision,
    pub question: Option<ApprovalId>,
}
pub(crate) struct ControlOutcome {
    pub value: serde_json::Value,
    pub artifacts: Vec<ArtifactId>,
}
impl ControlOutcome {
    fn plain(value: serde_json::Value) -> Self {
        Self {
            value,
            artifacts: vec![],
        }
    }
}
impl McpProposal {
    pub fn effect(&self) -> &ToolRunId {
        &self.effect
    }
    pub fn digest(&self) -> &str {
        self.authority.digest()
    }
}
impl CanonicalHost {
    pub(super) fn resume_mcp_approval(
        &self,
        thread: ThreadId,
        binding: ThreadBinding,
        expected: Revision,
        fingerprint: vcp_domain::verification::Fingerprint,
    ) -> Result<CommandReceipt, String> {
        let slots = self
            .mcp
            .slots
            .lock()
            .map_err(|_| "MCP map poisoned")?
            .iter()
            .map(|(key, slot)| (key.clone(), slot.clone()))
            .collect::<Vec<_>>();
        let mut guards = Vec::new();
        for (_, slot) in &slots {
            guards.push(
                slot.try_lock()
                    .map_err(|_| "MCP resume requires idle connections; a call is in flight")?,
            );
        }
        let mut proofs = Vec::new();
        for ((key, _), slot) in slots.iter().zip(&guards) {
            let Some(connection) = &slot.connection else {
                continue;
            };
            if key.0 != thread {
                return Err("another task owns a live MCP process".into());
            }
            let call = slot
                .pending
                .as_ref()
                .ok_or("MCP resume requires a pending exact call approval")?;
            call.operation.validate_client(&connection.client)?;
            let (effect, execution, job) = connection.process.resume_identity()?;
            proofs.push(ResumeProof {
                effect,
                execution,
                job,
                process: connection.prepared.clone(),
                call: call.effect.clone(),
                authority: call.authority.clone(),
                approval: call
                    .question
                    .clone()
                    .ok_or("MCP pending call lacks approval")?,
                provenance: call.provenance.clone(),
                registration_digest: call
                    .operation
                    .connection()
                    .ok_or("MCP connection absent")?
                    .registration_digest()
                    .to_owned(),
                server: call.server.clone(),
                controller: call.controller.clone(),
                owner: call.owner,
                generation: call.generation,
            });
        }
        if proofs.is_empty() {
            return self
                .worker
                .run(move |context| context.resume(&binding, expected, fingerprint));
        }
        let runtime = self.runtime.clone();
        // Guards remain held until the worker has consumed these ephemeral
        // observations; no protocol IO can begin during the resume decision.
        let result = self.worker.run(move |context| {
            context.resume_mcp_approval(&binding, thread, &runtime, expected, fingerprint, proofs)
        });
        drop(guards);
        result
    }
    #[cfg(feature = "qualification")]
    pub async fn qualification_prepare_mcp_context_call(
        &self,
        thread: ThreadId,
        request: Request,
        sealed: vcp_context::manifest::Sealed,
        roots: Vec<vcp_repository::Root>,
    ) -> Result<McpProposal, String> {
        let context = self
            .worker
            .run(move |context| context.verify_context(sealed))?;
        let provenance = Provenance::from_context(context, roots, None);
        let slot = self.mcp_slot(thread, request.server())?;
        let slot = slot.lock().await;
        self.mcp_prepare(thread, &request, provenance, &slot)
    }
    #[cfg(feature = "qualification")]
    pub async fn qualification_block_next_mcp_write(
        &self,
        thread: ThreadId,
        server: &str,
    ) -> Result<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>), String> {
        let slot = self.mcp_slot(thread, server)?;
        let mut slot = slot.lock().await;
        let connection = slot.connection.as_mut().ok_or("MCP connection absent")?;
        let arrived = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        connection
            .process
            .block_next_write(arrived.clone(), release.clone());
        Ok((arrived, release))
    }
    pub async fn prepare_mcp_call(
        &self,
        thread: ThreadId,
        request: Request,
    ) -> Result<McpProposal, String> {
        let provenance = Provenance {
            context: None,
            memory: None,
            roots: vec![],
            revisions: None,
            owner_arguments: Some(
                String::from_utf8(
                    vcp_protocol::canonical_bytes(&request).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?,
            ),
        };
        let slot = self.mcp_slot(thread, request.server())?;
        let slot = slot.lock().await;
        self.mcp_prepare(thread, &request, provenance, &slot)
    }
    pub async fn dispatch_mcp_call(
        &self,
        ticket: McpProposal,
    ) -> Result<serde_json::Value, String> {
        let slot = self.mcp_slot(ticket.thread, &ticket.server)?;
        let mut slot = slot.lock().await;
        self.mcp_dispatch(ticket, &mut slot)
            .await
            .map(|outcome| outcome.value)
    }
    async fn mcp_dispatch(
        &self,
        mut ticket: McpProposal,
        slot: &mut Slot,
    ) -> Result<ControlOutcome, String> {
        self.mcp_ticket_decision(&ticket).and_then(|decision| {
            if matches!(decision, vcp_policy::Decision::Allow { .. }) {
                Ok(())
            } else {
                Err("MCP call authority is not allowed".into())
            }
        })?;
        let mut connection = slot.connection.take().ok_or("MCP connection disappeared")?;
        let mut outbound = ticket.operation.begin_client(&mut connection.client)?;
        let execution = ExecutionId::new();
        let binding = ticket.binding.clone();
        let effect = ticket.effect.clone();
        let exec = execution.clone();
        let plan = ticket.plan.clone();
        let request_digest = outbound.digest().to_owned();
        let request_id = outbound.request_id().map(str::to_owned);
        let identity = ticket.operation.receipt_identity();
        self.worker.run(move|context| {
            let current:Effect=context.engine.store().state().record(Collection::Effect,effect.as_str(),&binding.scope.workspace)?.decode()?;
            if current.state!=EffectState::Validated || current.execution.is_some(){return Err("MCP call has already been admitted".into());}
            context.tool_advance(&binding,&effect,EffectState::Authorized,None,vec![plan.clone()],"current MCP call authority admitted")?;
            let intent=context.capture(&binding.scope,Channel::Evidence,&vcp_protocol::canonical_bytes(&serde_json::json!({"schema_version":1,"effect":effect,"execution":exec,"identity":identity,"request_id":request_id,"request_digest":request_digest,"delivery":"not yet observed; never replay to determine status"}))?,"vcp-mcp-call-intent-v1")?;
            context.tool_advance(&binding,&effect,EffectState::DispatchRecorded,Some(exec),vec![plan,intent.spec.id],"MCP call intent durable before protocol write")
        })?;
        ticket.unsent.dispatched();
        let mut guard = CallGuard {
            worker: self.worker.clone(),
            runtime: self.runtime.clone(),
            binding: ticket.binding.clone(),
            effect: ticket.effect.clone(),
            finished: false,
        };
        let binding = ticket.binding.clone();
        let process = ticket.process.clone();
        let authority = ticket.authority.clone();
        let provenance = ticket.provenance.clone();
        let server = ticket.server.clone();
        let registration = ticket
            .operation
            .connection()
            .ok_or("MCP connection absent")?
            .registration_digest()
            .to_owned();
        let effect = ticket.effect.clone();
        let exec = execution.clone();
        let deadline = tokio::time::Instant::now()
            + Duration::from_millis(connection.registration.limits.timeout_ms);
        let response: Result<content::Observed, String> = async {
            for _ in 0..connection.registration.limits.pages {
                let binding = binding.clone();
                let server = server.clone();
                let registration = registration.clone();
                let process = process.clone();
                let authority = authority.clone();
                let provenance = provenance.clone();
                let effect = effect.clone();
                let exec = exec.clone();
                transport::send_checked(
                    &mut connection.process,
                    &mut connection.client,
                    &outbound,
                    deadline,
                    move |context| {
                        if !matches!(
                            context.mcp_decision(
                                &binding,
                                &server,
                                &registration,
                                &process,
                                &authority,
                                &provenance
                            )?,
                            vcp_policy::Decision::Allow { .. }
                        ) {
                            return Err(
                                "MCP current source/call authority rejected before write".into()
                            );
                        }
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
                        if current.execution.as_ref() != Some(&exec)
                            || !matches!(
                                current.state,
                                EffectState::DispatchRecorded | EffectState::Running
                            )
                        {
                            return Err("MCP dispatch intent is no longer current".into());
                        }
                        if current.state == EffectState::Running {
                            return Ok(());
                        }
                        context.tool_advance(
                            &binding,
                            &effect,
                            EffectState::Running,
                            Some(exec),
                            current.observed_changes,
                            "MCP protocol write admitted; receipt not observed",
                        )
                    },
                )
                .await?;
                if let Some(observed) = content::observe(
                    next_response(&mut connection, deadline).await?,
                    &ticket.server,
                )? {
                    return Ok(observed);
                }
                outbound = ticket.operation.begin_client(&mut connection.client)?;
            }
            Err("MCP discovery page ceiling".into())
        }
        .await;
        let reply = match response {
            Ok(reply) => reply,
            Err(error) => {
                guard.unknown(&error)?;
                stop_connection(connection).await;
                return Ok(ControlOutcome::plain(
                    serde_json::json!({"effect":ticket.effect,"outcome":"unknown","reason":error,"replay":false}),
                ));
            }
        };
        let success = reply.success;
        let result = reply.value;
        let captured_result = if matches!(ticket.operation, PreparedOperation::Tool { .. }) {
            result["result"].clone()
        } else {
            result.clone()
        };
        let document = serde_json::json!({"schema_version":1,"effect":ticket.effect,"execution":execution,"identity":ticket.operation.receipt_identity(),"result":captured_result,"external_content":true,"grants_authority":false,"source":ticket.provenance.evidence()?});
        let binding = ticket.binding.clone();
        let effect = ticket.effect.clone();
        let captured = document.clone();
        let artifact = self.worker.run_cleanup(move |context| {
            // An already observed protocol reply belongs to its original call,
            // even if authority changed while it was in flight. It grants none.
            let artifact = context.capture(
                &binding.scope,
                Channel::Evidence,
                &vcp_protocol::canonical_bytes(&captured)?,
                "vcp-mcp-call-result-v1",
            )?;
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
            let mut evidence = current.observed_changes;
            evidence.push(artifact.spec.id.clone());
            context.tool_advance(
                &binding,
                &effect,
                if success {
                    EffectState::Succeeded
                } else {
                    EffectState::Failed
                },
                Some(execution),
                evidence,
                "MCP protocol result observed; server content remains untrusted",
            )?;
            Ok(artifact)
        })?;
        guard.finished = true;
        let cacheable = success
            && ticket
                .operation
                .resource()
                .is_some_and(|identity| connection.client.resource(identity).is_ok());
        if cacheable {
            if let Some(identity) = ticket.operation.resource() {
                while connection.cache.len() >= connection.registration.limits.tools as usize {
                    if let Some(key) = connection.cache.keys().next().cloned() {
                        connection.cache.remove(&key);
                    } else {
                        break;
                    }
                }
                connection.cache.insert(
                    artifact.spec.id.clone(),
                    content::CachedResource {
                        identity: identity.clone(),
                        scope: ticket.binding.scope.clone(),
                        artifact: artifact.spec.id.clone(),
                    },
                );
            }
        }
        slot.connection = Some(connection);
        let mut value = result.clone();
        value["effect"] = serde_json::json!(ticket.effect);
        value["outcome"] = serde_json::json!(if success { "succeeded" } else { "failed" });
        value["receipt"] = document;
        value["artifact"] = serde_json::json!(artifact.spec.id);
        value["external_content"] = true.into();
        value["grants_authority"] = false.into();
        value["cache_available"] = serde_json::json!(cacheable);
        Ok(ControlOutcome {
            value,
            artifacts: vec![artifact.spec.id],
        })
    }
    pub fn configure_mcp(&self, registration: Registration) -> Result<(), String> {
        self.worker
            .run(move |context| context.configure_mcp(registration))
    }
    fn mcp_slot(
        &self,
        thread: ThreadId,
        server: &str,
    ) -> Result<Arc<tokio::sync::Mutex<Slot>>, String> {
        if server.is_empty() || server.len() > 128 {
            return Err("invalid configured MCP server identity".into());
        }
        let mut slots = self
            .mcp
            .slots
            .lock()
            .map_err(|_| "MCP connection map poisoned")?;
        let key = (thread, server.to_owned());
        if !slots.contains_key(&key) && slots.len() >= 16 {
            return Err("MCP connection slot ceiling".into());
        }
        Ok(slots.entry(key).or_default().clone())
    }
    /// Explicit trusted owner command. Model wrappers use the separate captured
    /// context path, and cannot manufacture this literal-owner-input provenance.
    pub async fn mcp_control(
        &self,
        thread: ThreadId,
        request: Request,
    ) -> Result<serde_json::Value, String> {
        let provenance = Provenance {
            context: None,
            memory: None,
            roots: vec![],
            revisions: None,
            owner_arguments: Some(
                vcp_protocol::canonical_bytes(&request)
                    .map_err(|e| e.to_string())
                    .and_then(|b| String::from_utf8(b).map_err(|e| e.to_string()))?,
            ),
        };
        self.mcp_control_provenance(thread, request, provenance)
            .await
            .map(|outcome| outcome.value)
    }
    pub(crate) async fn mcp_control_provenance(
        &self,
        thread: ThreadId,
        request: Request,
        provenance: Provenance,
    ) -> Result<ControlOutcome, String> {
        let slot = self.mcp_slot(thread, request.server())?;
        let mut slot = slot.lock().await;
        let remote_server = request.server().to_owned();
        if self
            .worker
            .run(move |context| Ok(context.is_remote_mcp(&remote_server)))?
        {
            return self
                .remote_control(thread, request, provenance, &mut slot.remote)
                .await;
        }
        if matches!(
            &request,
            Request::Resources { .. } | Request::Prompts { .. }
        ) && slot.connection.is_none()
        {
            let setup = self
                .mcp_list(
                    thread,
                    request.server(),
                    &mut slot,
                    provenance.clone(),
                    true,
                )
                .await?;
            if slot.connection.is_none() {
                return Ok(setup);
            }
        }
        match request {
            Request::ReadCached { artifact, .. } => {
                self.mcp_cached(thread, &slot, &artifact, provenance)
            }
            Request::Disconnect { .. } => {
                slot.pending.take();
                slot.startup.take();
                if let Some(connection) = slot.connection.take() {
                    connection.process.terminate()?;
                    let receipt = connection.process.wait().await?;
                    Ok(ControlOutcome {
                        value: serde_json::json!({"disconnected":true,"process_effect":receipt.effect,"process_exit":receipt.exit_code}),
                        artifacts: vec![receipt.evidence.spec.id],
                    })
                } else {
                    Ok(ControlOutcome::plain(
                        serde_json::json!({"disconnected":true}),
                    ))
                }
            }
            Request::List { server } => {
                self.mcp_list(thread, &server, &mut slot, provenance, false)
                    .await
            }
            request @ (Request::Call { .. }
            | Request::Resources { .. }
            | Request::ReadResource { .. }
            | Request::Prompts { .. }
            | Request::GetPrompt { .. }) => {
                let request_key = vcp_protocol::digest_bytes(
                    &vcp_protocol::canonical_bytes(&request).map_err(|e| e.to_string())?,
                );
                let reused = slot
                    .pending
                    .as_ref()
                    .is_some_and(|p| p.request_key == request_key);
                let ticket = if slot
                    .pending
                    .as_ref()
                    .is_some_and(|p| p.request_key == request_key)
                {
                    slot.pending.take().ok_or("pending MCP call disappeared")?
                } else {
                    if slot.pending.is_some() {
                        return Err(
                            "resolve the pending MCP approval before preparing another call".into(),
                        );
                    }
                    self.mcp_prepare(thread, &request, provenance, &slot)?
                };
                let current = if reused {
                    self.mcp_ticket_decision(&ticket)?
                } else {
                    ticket.decision.clone()
                };
                if !matches!(current, vcp_policy::Decision::Allow { .. }) {
                    let value = serde_json::json!({"effect":ticket.effect,"decision":decision_value(&current),"question":ticket.question,"dispatched":false});
                    if matches!(current, vcp_policy::Decision::Question { .. }) {
                        slot.pending = Some(ticket);
                    }
                    return Ok(ControlOutcome::plain(value));
                }
                self.mcp_dispatch(ticket, &mut slot).await
            }
        }
    }
}

impl CanonicalHost {
    async fn mcp_list(
        &self,
        thread: ThreadId,
        server: &str,
        slot: &mut Slot,
        mut provenance: Provenance,
        initialize_only: bool,
    ) -> Result<ControlOutcome, String> {
        if slot.pending.is_some() {
            return Err("resolve pending MCP call before rediscovery".into());
        }
        let binding = self.binding(thread)?;
        let scoped = binding.clone();
        provenance = self.worker.run(move |context| {
            if provenance.revisions.is_none() {
                provenance.revisions = Some(context.context_revisions(&scoped)?);
            }
            context.validate_mcp_provenance(&scoped, &provenance)?;
            Ok(provenance)
        })?;
        let scoped = binding.clone();
        let name = server.to_owned();
        let (configured, registration) = self
            .worker
            .run(move |context| context.mcp_registration(&scoped, &name))?;
        if slot.connection.is_none() {
            let reused = slot.startup.is_some();
            let pending = match slot.startup.take() {
                Some(ticket) => ticket,
                None => {
                    let ticket = self.prepare_process(thread, configured.process.clone())?;
                    let unsent = scheduler::QueuedEffect::new(self, &binding, ticket.effect());
                    PendingStartup { ticket, unsent }
                }
            };
            let startup = &pending.ticket;
            let scoped = binding.clone();
            let prepared = startup.prepared.clone();
            let decision = if reused {
                self.worker
                    .run(move |context| context.process_decision(&scoped, &prepared))?
            } else {
                startup.decision.clone()
            };
            if !matches!(decision, vcp_policy::Decision::Allow { .. }) {
                let response = serde_json::json!({"server":server,"decision":decision_value(&decision),"question":startup.question,"effect":startup.effect(),"connected":false});
                if matches!(decision, vcp_policy::Decision::Question { .. }) {
                    slot.startup = Some(pending);
                }
                return Ok(ControlOutcome::plain(response));
            }
            let prepared = startup.prepared.clone();
            let PendingStartup {
                ticket: startup,
                mut unsent,
            } = pending;
            let process = self.dispatch_duplex_process(
                startup,
                DuplexIoLimits {
                    frame_bytes: registration.limits.frame_bytes as usize,
                    queued_frames: 8,
                    input_bytes: 8 * 1024 * 1024,
                    max_messages: 256,
                    stderr_bytes: Some(registration.limits.stderr_bytes),
                },
            )?;
            unsent.dispatched();
            let scoped = binding.clone();
            let document = registration.clone();
            let artifact = self.worker.run(move |context| {
                context.capture(
                    &scoped.scope,
                    Channel::Evidence,
                    &vcp_protocol::canonical_bytes(
                        &serde_json::json!({"schema_version":1,"registration":document}),
                    )?,
                    "vcp-mcp-registration-v1",
                )
            })?;
            let client = Client::new(registration.clone()).map_err(|e| e.to_string())?;
            let mut connection = Connection {
                process,
                client,
                registration: registration.clone(),
                prepared,
                evidence: artifact.spec.id,
                cache: Default::default(),
            };
            let deadline =
                tokio::time::Instant::now() + Duration::from_millis(registration.limits.timeout_ms);
            let initialize = connection.client.initialize().map_err(|e| e.to_string())?;
            let initialize_result = async {
                let scoped = binding.clone();
                let sources = provenance.clone();
                transport::send_checked(
                    &mut connection.process,
                    &mut connection.client,
                    &initialize,
                    deadline,
                    move |context| context.validate_mcp_provenance(&scoped, &sources),
                )
                .await?;
                match next_response(&mut connection, deadline).await? {
                    Incoming::Initialized { notification, .. } => {
                        transport::send(
                            &mut connection.process,
                            &mut connection.client,
                            &notification,
                            deadline,
                        )
                        .await
                    }
                    _ => Err("MCP initialization response rejected".into()),
                }
            }
            .await;
            if let Err(error) = initialize_result {
                stop_connection(connection).await;
                return Err(error);
            }
            slot.connection = Some(connection);
        }
        if initialize_only {
            return Ok(ControlOutcome::plain(serde_json::json!({"connected":true})));
        }
        let mut connection = slot.connection.take().ok_or("MCP connection absent")?;
        if connection
            .registration
            .digest()
            .map_err(|e| e.to_string())?
            != registration.digest().map_err(|e| e.to_string())?
        {
            stop_connection(connection).await;
            return Err("MCP registration changed; reconnect required".into());
        }
        let deadline =
            tokio::time::Instant::now() + Duration::from_millis(registration.limits.timeout_ms);
        let result=async {
            for _ in 0..registration.limits.pages {
                let outbound=connection.client.list_tools().map_err(|e|e.to_string())?;
                let scoped=binding.clone();let sources=provenance.clone();
                transport::send_checked(&mut connection.process,&mut connection.client,&outbound,deadline,move|context|context.validate_mcp_provenance(&scoped,&sources)).await?;
                match next_response(&mut connection,deadline).await? {
                    Incoming::DiscoveryPage{next:true}=>continue,
                    Incoming::DiscoveryComplete{tools,rejected}=> {
                        let metadata=tools.iter().map(|tool|Ok(serde_json::json!({"server":server,"tool":tool.name,"qualified_name":format!("{}::{}",server,tool.name),"identity":tool.identity,"identity_digest":tool.identity.digest().map_err(|e|e.to_string())?,"input_schema":tool.input_schema,"description":tool.description,"annotations_are_hints":true}))).collect::<Result<Vec<_>,String>>()?;
                        let document=serde_json::json!({"schema_version":1,"server":server,"connection":connection.client.connection(),"tools":metadata,"rejected":rejected,"registration_artifact":connection.evidence,"external_content":true});
                        let scoped=binding.clone();let captured=document.clone();
                        let artifact=self.worker.run(move|context|context.capture(&scoped.scope,Channel::Evidence,&vcp_protocol::canonical_bytes(&captured)?,"vcp-mcp-discovery-v1"))?;
                        return Ok(ControlOutcome{value:serde_json::json!({"connected":true,"catalog":document,"artifact":artifact.spec.id}),artifacts:vec![artifact.spec.id]});
                    }
                    _=>return Err("MCP discovery response rejected".into()),
                }
            }
            Err("MCP discovery page ceiling".into())
        }.await;
        match result {
            Ok(value) => {
                slot.connection = Some(connection);
                Ok(value)
            }
            Err(error) => {
                stop_connection(connection).await;
                Err(error)
            }
        }
    }
    fn mcp_prepare(
        &self,
        thread: ThreadId,
        request: &Request,
        mut provenance: Provenance,
        slot: &Slot,
    ) -> Result<McpProposal, String> {
        let server = request.server();
        let connection = slot
            .connection
            .as_ref()
            .ok_or("MCP server must be explicitly connected first")?;
        let operation = PreparedOperation::select_client(request, Some(&connection.client))?;
        let binding = self.binding(thread)?;
        let scoped = binding.clone();
        let process = connection.prepared.clone();
        let process_copy = process.clone();
        let operation_copy = operation.clone();
        let server_copy = server.to_owned();
        let (authority,provenance,controller,owner,effect,plan,decision,question)=self.worker.run(move|context| {
            if provenance.revisions.is_none(){provenance.revisions=Some(context.context_revisions(&scoped)?);}
            context.validate_mcp_provenance(&scoped,&provenance)?;
            let authority=context.prepare_mcp_authority(&scoped,&process_copy,&operation_copy,&provenance)?;
            let decision=context.mcp_decision(&scoped,&server_copy,operation_copy.connection().ok_or("MCP connection absent")?.registration_digest(),&process_copy,&authority,&provenance)?;
            let evidence=vcp_protocol::canonical_bytes(&serde_json::json!({"operation":authority.operation(),"source":provenance.evidence()?}))?;
            let (effect,plan,decision,question)=context.propose_authority(&scoped,&authority,&evidence,decision)?;
            Ok((Arc::new(authority),provenance,context.engine.controller().clone(),context.engine.owner_epoch(),effect,plan,decision,question))
        })?;
        Ok(McpProposal {
            unsent: scheduler::QueuedEffect::new(self, &binding, &effect),
            thread,
            binding,
            generation: scheduler::generation(&self.runtime, thread)?,
            controller,
            owner,
            server: server.to_owned(),
            operation,
            authority,
            process,
            provenance,
            effect,
            plan,
            request_key: vcp_protocol::digest_bytes(
                &vcp_protocol::canonical_bytes(request).map_err(|e| e.to_string())?,
            ),
            decision,
            question,
        })
    }
    fn mcp_ticket_decision(&self, ticket: &McpProposal) -> Result<vcp_policy::Decision, String> {
        scheduler::check_generation(&self.runtime, ticket.thread, ticket.generation)?;
        let binding = ticket.binding.clone();
        let process = ticket.process.clone();
        let authority = ticket.authority.clone();
        let provenance = ticket.provenance.clone();
        let server = ticket.server.clone();
        let registration = ticket
            .operation
            .connection()
            .ok_or("MCP connection absent")?
            .registration_digest()
            .to_owned();
        let controller = ticket.controller.clone();
        let owner = ticket.owner;
        self.worker.run(move |context| {
            if context.engine.controller() != &controller || context.engine.owner_epoch() != owner {
                return Err("MCP ticket belongs to another owner".into());
            }
            context.mcp_decision(
                &binding,
                &server,
                &registration,
                &process,
                &authority,
                &provenance,
            )
        })
    }
    pub async fn disconnect_mcp(&self) -> Result<(), String> {
        let slots = self
            .mcp
            .slots
            .lock()
            .map_err(|_| "MCP map poisoned")?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for slot in slots {
            let mut slot = slot.lock().await;
            slot.remote.clear();
            slot.pending.take();
            slot.startup.take();
            if let Some(connection) = slot.connection.take() {
                stop_connection(connection).await;
            }
        }
        Ok(())
    }
    pub fn mcp_connections_present(&self) -> bool {
        self.mcp.slots.lock().map_or(true, |slots| {
            slots.values().any(|slot| {
                slot.try_lock().map_or(true, |slot| {
                    slot.connection.is_some() || slot.remote.connected()
                })
            })
        })
    }
}
async fn next_response(
    connection: &mut Connection,
    deadline: tokio::time::Instant,
) -> Result<Incoming, String> {
    for _ in 0..32 {
        match transport::receive(&mut connection.process, &mut connection.client, deadline).await? {
            Incoming::ControlReply(outbound) => {
                transport::send(
                    &mut connection.process,
                    &mut connection.client,
                    &outbound,
                    deadline,
                )
                .await?
            }
            Incoming::Notification(vcp_mcp::client::Notification::ToolListChanged) => {
                return Err(
                    "MCP tool list changed; explicit reconnect and preparation required".into(),
                )
            }
            Incoming::Notification(_) => continue,
            incoming => return Ok(incoming),
        }
    }
    Err("MCP unsolicited message ceiling".into())
}
async fn stop_connection(mut connection: Connection) {
    connection.client.abandon();
    let _ = connection.process.terminate();
    let _ = connection.process.wait().await;
}
fn decision_value(decision: &vcp_policy::Decision) -> serde_json::Value {
    match decision {
        vcp_policy::Decision::Allow { origin, grant } => {
            serde_json::json!({"kind":"allow","origin":origin,"grant":grant})
        }
        vcp_policy::Decision::Deny { origin, reason } => {
            serde_json::json!({"kind":"deny","origin":origin,"reason":reason})
        }
        vcp_policy::Decision::Question { digest, reason } => {
            serde_json::json!({"kind":"question","digest":digest,"reason":reason})
        }
    }
}
struct CallGuard {
    worker: worker::Worker,
    runtime: Lifecycle,
    binding: ThreadBinding,
    effect: ToolRunId,
    finished: bool,
}
impl CallGuard {
    fn unknown(&mut self, reason: &str) -> Result<(), String> {
        let binding = self.binding.clone();
        let effect = self.effect.clone();
        let reason = reason.to_owned();
        self.worker.run_cleanup(move |context| {
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
                    &format!("MCP response not durably observed: {reason}; no automatic replay"),
                )?;
            }
            Ok(())
        })?;
        self.finished = true;
        Ok(())
    }
}
impl Drop for CallGuard {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.runtime.hold_owner();
            if self.unknown("call waiter interrupted").is_err() {
                self.worker.fence();
            }
        }
    }
}
