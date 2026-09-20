// SPDX-License-Identifier: Apache-2.0
//! Canonical remote MCP operations. A logical HTTP session is not a process lease.
use super::remote_authority::{AuthorityPin, CredentialLease, CredentialMaterial, RemoteProfile};
use super::*;
use crate::remote_transport::{self as wire, trust::TrustSnapshot};
use std::collections::BTreeSet;
use vcp_mcp::{
    client::Outbound,
    http::{Decoder, Session},
};

#[derive(Clone)]
pub struct RemoteRegistration {
    pub(in crate::foundation) profile: RemoteProfile,
    pub(in crate::foundation) allowed_tools: BTreeSet<String>,
    allowed_resources: BTreeSet<String>,
    allowed_prompts: BTreeSet<String>,
    pub(in crate::foundation) limits: vcp_mcp::registration::Limits,
    pub(in crate::foundation) trust: Arc<TrustSnapshot>,
}
impl RemoteRegistration {
    pub fn new(
        profile: RemoteProfile,
        allowed_tools: BTreeSet<String>,
        limits: vcp_mcp::registration::Limits,
    ) -> Result<Self, String> {
        Ok(Self {
            profile,
            allowed_tools,
            allowed_resources: Default::default(),
            allowed_prompts: Default::default(),
            limits,
            trust: Arc::new(TrustSnapshot::native()?),
        })
    }
    #[cfg(feature = "qualification")]
    pub fn fixture_roots(
        profile: RemoteProfile,
        allowed_tools: BTreeSet<String>,
        limits: vcp_mcp::registration::Limits,
        roots: Vec<Vec<u8>>,
    ) -> Result<Self, String> {
        Ok(Self {
            profile,
            allowed_tools,
            allowed_resources: Default::default(),
            allowed_prompts: Default::default(),
            limits,
            trust: Arc::new(TrustSnapshot::fixture_roots(roots)?),
        })
    }
    pub fn with_content(
        mut self,
        resources: BTreeSet<String>,
        prompts: BTreeSet<String>,
    ) -> Result<Self, String> {
        self.allowed_resources = resources;
        self.allowed_prompts = prompts;
        self.pure()?;
        Ok(self)
    }
    pub fn profile(&self) -> &RemoteProfile {
        &self.profile
    }
    pub(in crate::foundation) fn pure(
        &self,
    ) -> Result<vcp_mcp::registration::Registration, String> {
        if self.limits.timeout_ms > 120_000 {
            return Err("remote MCP timeout exceeds the 120-second transport ceiling".into());
        }
        let digest = vcp_protocol::digest_bytes(
            &vcp_protocol::canonical_bytes(
                &serde_json::json!({"profile":self.profile.digest(),"trust":self.trust.digest()}),
            )
            .map_err(|e| e.to_string())?,
        );
        let value = vcp_mcp::registration::Registration {
            id: self.profile.config().server.clone(),
            revision: self.profile.config().revision,
            scope: vcp_domain::policy::GrantScope::Workspace {
                workspace: self.profile.config().workspace.clone(),
            },
            transport: vcp_mcp::registration::Transport::StreamableHttp {
                remote_profile: self.profile.config().server.clone(),
                resolved_digest: digest,
            },
            auth_refs: self
                .profile
                .config()
                .credential_ref
                .iter()
                .cloned()
                .collect(),
            allowed_tools: self.allowed_tools.clone(),
            allowed_resources: self.allowed_resources.clone(),
            allowed_prompts: self.allowed_prompts.clone(),
            trusted_effects: Default::default(),
            limits: self.limits.clone(),
            capabilities: Default::default(),
        };
        value.validate().map_err(|e| e.to_string())?;
        Ok(value)
    }
}
#[derive(Default)]
pub(super) struct RemoteSlot {
    connection: Option<RemoteConnection>,
    pending: Option<RemoteProposal>,
}
impl RemoteSlot {
    pub(super) fn clear(&mut self) {
        self.pending.take();
        self.connection.take();
    }
    pub(super) fn connected(&self) -> bool {
        self.connection.is_some()
    }
}
struct RemoteConnection {
    session: Session,
    registration: RemoteRegistration,
    evidence: ArtifactId,
    credential_revision: Option<Revision>,
    cache: std::collections::BTreeMap<ArtifactId, content::CachedResource>,
}
pub struct RemoteProposal {
    #[cfg(feature = "qualification")]
    before_receipt: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
    #[cfg(feature = "qualification")]
    before_http: Option<(bool, Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
    unsent: scheduler::QueuedEffect,
    thread: ThreadId,
    binding: ThreadBinding,
    generation: u64,
    server: String,
    registration: RemoteRegistration,
    registration_digest: String,
    authority: Arc<vcp_policy::Prepared>,
    provenance: Provenance,
    credential: Option<CredentialLease>,
    pin: AuthorityPin,
    operation: PreparedOperation,
    effect: ToolRunId,
    plan: ArtifactId,
    request_key: String,
    pub decision: vcp_policy::Decision,
    pub question: Option<ApprovalId>,
}
impl RemoteProposal {
    /// Stop after a successful validated reply, before durable receipt capture.
    /// Cancellation retains the ordinary unfinished-call recovery semantics.
    #[cfg(feature = "qualification")]
    pub fn qualification_block_before_receipt(
        mut self,
        arrived: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    ) -> Self {
        self.before_receipt = Some((arrived, release));
        self
    }

    #[cfg(feature = "qualification")]
    pub fn qualification_block_before_http(
        &mut self,
        after_source_check: bool,
    ) -> (Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>) {
        let arrived = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        self.before_http = Some((after_source_check, arrived.clone(), release.clone()));
        (arrived, release)
    }

    pub fn effect(&self) -> &ToolRunId {
        &self.effect
    }
}
impl CanonicalHost {
    pub fn configure_mcp_remote(&self, registration: RemoteRegistration) -> Result<(), String> {
        self.worker
            .run(move |context| context.configure_mcp_remote(registration))
    }
    pub fn install_mcp_credential(
        &self,
        server: &str,
        expected: Option<Revision>,
        material: CredentialMaterial,
        expires_at: Timestamp,
    ) -> Result<Revision, String> {
        let server = server.to_owned();
        let connections = self.mcp.clone();
        let runtime = self.runtime.clone();
        self.worker.run(move |context| {
            let (registration, pin) = context.remote_mcp_setup(&server)?;
            let revision = connections.credentials.install(
                &registration.profile,
                pin,
                expected,
                material,
                expires_at,
                timestamp(),
            )?;
            if expected.is_some() {
                runtime.close_all_remote_sockets();
            }
            Ok(revision)
        })
    }
    pub fn revoke_mcp_credential(
        &self,
        server: &str,
        expected: Revision,
    ) -> Result<Revision, String> {
        let server = server.to_owned();
        let connections = self.mcp.clone();
        let runtime = self.runtime.clone();
        self.worker.run(move |context| {
            let (registration, _) = context.remote_mcp_setup(&server)?;
            let revision = connections
                .credentials
                .revoke(&registration.profile, expected)?;
            runtime.close_all_remote_sockets();
            Ok(revision)
        })
    }
}

fn timestamp() -> Timestamp {
    Timestamp::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|v| v.as_millis().min(u64::MAX as u128) as u64)
            .unwrap_or(u64::MAX),
    )
}
impl CanonicalHost {
    pub async fn prepare_remote_mcp_call(
        &self,
        thread: ThreadId,
        request: Request,
    ) -> Result<RemoteProposal, String> {
        if matches!(
            request,
            Request::Disconnect { .. } | Request::ReadCached { .. }
        ) {
            return Err("remote MCP call required".into());
        }
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
        let shared = self.mcp_slot(thread, request.server())?;
        let slot = shared.lock().await;
        self.remote_prepare(thread, &request, provenance, &slot.remote)
    }
    pub async fn dispatch_remote_mcp_call(
        &self,
        ticket: RemoteProposal,
    ) -> Result<serde_json::Value, String> {
        let shared = self.mcp_slot(ticket.thread, &ticket.server)?;
        let mut slot = shared.lock().await;
        self.remote_dispatch(ticket, &mut slot.remote)
            .await
            .map(|value| value.value)
    }
    pub(super) async fn remote_control(
        &self,
        thread: ThreadId,
        request: Request,
        provenance: Provenance,
        slot: &mut RemoteSlot,
    ) -> Result<ControlOutcome, String> {
        if let Request::ReadCached { artifact, .. } = &request {
            return self.remote_cached(thread, slot, artifact, provenance);
        }
        if matches!(request, Request::Disconnect { .. }) {
            slot.clear();
            return Ok(ControlOutcome::plain(
                serde_json::json!({"disconnected":true,"remote_session_deleted":false}),
            ));
        }
        let key = vcp_protocol::digest_bytes(
            &vcp_protocol::canonical_bytes(&request).map_err(|e| e.to_string())?,
        );
        let reused = slot.pending.as_ref().is_some_and(|p| p.request_key == key);
        let ticket = if reused {
            slot.pending
                .take()
                .ok_or("remote pending ticket disappeared")?
        } else {
            if slot.pending.is_some() {
                return Err("resolve pending remote MCP approval first".into());
            }
            self.remote_prepare(thread, &request, provenance, slot)?
        };
        let decision = if reused {
            self.remote_ticket_decision(&ticket)?
        } else {
            ticket.decision.clone()
        };
        if !matches!(decision, vcp_policy::Decision::Allow { .. }) {
            let value = serde_json::json!({"server":ticket.server,"effect":ticket.effect,"decision":decision_value(&decision),"question":ticket.question,"dispatched":false,"connected":slot.connected()});
            if matches!(decision, vcp_policy::Decision::Question { .. }) {
                slot.pending = Some(ticket);
            }
            return Ok(ControlOutcome::plain(value));
        }
        self.remote_dispatch(ticket, slot).await
    }
    fn remote_prepare(
        &self,
        thread: ThreadId,
        request: &Request,
        mut provenance: Provenance,
        slot: &RemoteSlot,
    ) -> Result<RemoteProposal, String> {
        let operation = PreparedOperation::select_session(
            request,
            slot.connection.as_ref().map(|v| &v.session),
        )?;
        let binding = self.binding(thread)?;
        let scoped = binding.clone();
        let server = request.server().to_owned();
        let remote = server.clone();
        let request_value = serde_json::to_value(request).map_err(|e| e.to_string())?;
        let checked = operation.arguments().cloned();
        let previous_credential = slot.connection.as_ref().map(|c| c.credential_revision);
        let connections = self.mcp.clone();
        let mut session_secrets = Vec::new();
        if let Some(connection) = &slot.connection {
            connection.session.visit_sensitive_values(|value| {
                session_secrets.push(zeroize::Zeroizing::new(value.to_owned()))
            });
        }
        let (registration,registration_digest,credential,pin,provenance,operation,authority,effect,plan,decision,question)=self.worker.run(move|context|{
            if provenance.revisions.is_none(){provenance.revisions=Some(context.context_revisions(&scoped)?);}
            context.validate_mcp_provenance(&scoped,&provenance)?;
            let (registration,pin)=context.remote_mcp_setup(&remote)?;
            let credential=if registration.profile.config().credential_ref.is_some(){Some(connections.credentials.resolve_current(&registration.profile,&pin,timestamp())?)}else{None};
            let mut operation=operation;
            if operation.discovery() && previous_credential.is_some_and(|prior|prior!=credential.as_ref().map(CredentialLease::revision)) {operation.reset_discovery_connection();}
            let ids=operation.evidence();
            let registration_digest=registration.pure()?.digest()?;
            let args=serde_json::json!({"request":request_value,"identity":ids,"checked_arguments":checked.as_ref().map(|a|a.digest()),"registration":registration_digest,"trust":registration.trust.digest(),"credential_revision":credential.as_ref().map(CredentialLease::revision),"source":provenance.evidence()?});
            if let Some(lease)=&credential {if lease.contains_secret(&vcp_protocol::canonical_bytes(&args)?)? {return Err("credential-bearing operation arguments rejected".into());}}
            for secret in &session_secrets {
                let filter=super::remote_authority::CaptureSecret::new(secret)?;
                screen_session_json(&filter,&vcp_protocol::canonical_bytes(&args)?)?;
                if let Some(value)=&checked {screen_session_json(&filter,value.canonical_bytes())?;}
            }
            if let (Some(lease),Some(checked))=(&credential,&checked) {if lease.contains_secret(checked.canonical_bytes())? {return Err("credential-bearing checked arguments rejected".into());}}
            let authority=context.prepare_remote_mcp_authority(&scoped,&registration,args,&provenance)?;
            // The completed operation adds the endpoint and canonical host identity.
            // Screen those fields before policy evaluation or durable proposal capture.
            let operation_bytes=vcp_protocol::canonical_bytes(authority.operation())?;
            if let Some(lease)=&credential {
                if lease.contains_secret(&operation_bytes)? {return Err("sensitive remote MCP operation rejected".into());}
            }
            for secret in &session_secrets {
                screen_session_json(&super::remote_authority::CaptureSecret::new(secret)?,&operation_bytes)?;
            }
            let decision=context.remote_mcp_decision(&scoped,&remote,&registration_digest,&authority,&provenance)?;
            let evidence=vcp_protocol::canonical_bytes(&serde_json::json!({"operation":authority.operation(),"source":provenance.evidence()?}))?;
            let (effect,plan,decision,question)=context.propose_authority(&scoped,&authority,&evidence,decision)?;
            Ok((registration,registration_digest,credential,pin,provenance,operation,Arc::new(authority),effect,plan,decision,question))
        })?;
        let unsent = scheduler::QueuedEffect::new(self, &binding, &effect);
        if !operation.discovery()
            && slot.connection.as_ref().is_some_and(|connection| {
                connection.credential_revision != credential.as_ref().map(CredentialLease::revision)
            })
        {
            return Err(
                "remote credential changed; disconnect and list before preparing a call".into(),
            );
        }
        Ok(RemoteProposal {
            #[cfg(feature = "qualification")]
            before_receipt: None,
            #[cfg(feature = "qualification")]
            before_http: None,
            unsent,
            thread,
            binding,
            generation: scheduler::generation(&self.runtime, thread)?,
            server,
            registration,
            registration_digest,
            authority,
            provenance,
            credential,
            pin,
            operation,
            effect,
            plan,
            request_key: vcp_protocol::digest_bytes(
                &vcp_protocol::canonical_bytes(request).map_err(|e| e.to_string())?,
            ),
            decision,
            question,
        })
    }
    fn remote_ticket_decision(
        &self,
        ticket: &RemoteProposal,
    ) -> Result<vcp_policy::Decision, String> {
        scheduler::check_generation(&self.runtime, ticket.thread, ticket.generation)?;
        let binding = ticket.binding.clone();
        let server = ticket.server.clone();
        let digest = ticket.registration_digest.clone();
        let authority = ticket.authority.clone();
        let provenance = ticket.provenance.clone();
        let credential = ticket.credential.clone();
        let expected = ticket.pin.clone();
        self.worker.run(move |context| {
            let (registration, current) = context.remote_mcp_setup(&server)?;
            if current != expected {
                return Err("remote MCP authority owner changed".into());
            }
            if let Some(lease) = &credential {
                lease.validate_authority(&registration.profile, &current, timestamp())?;
            }
            context.remote_mcp_decision(&binding, &server, &digest, &authority, &provenance)
        })
    }
}
struct WireRun {
    execution: ExecutionId,
    sequence: u32,
    artifacts: Vec<ArtifactId>,
    deadline: tokio::time::Instant,
    usable: bool,
    sensitive: Vec<zeroize::Zeroizing<String>>,
    body_bytes: usize,
    controls: u32,
}
impl CanonicalHost {
    async fn remote_dispatch(
        &self,
        mut ticket: RemoteProposal,
        slot: &mut RemoteSlot,
    ) -> Result<ControlOutcome, String> {
        if !matches!(
            self.remote_ticket_decision(&ticket)?,
            vcp_policy::Decision::Allow { .. }
        ) {
            return Err("remote MCP authority is not allowed".into());
        }
        if !ticket.operation.discovery() {
            ticket.operation.validate_session(
                &slot
                    .connection
                    .as_ref()
                    .ok_or("remote MCP connection disappeared")?
                    .session,
            )?;
        }
        let _claim = self.scheduler.try_acquire(ticket.authority.operation())?;
        let mut permit = codex_extension_api::HostWorkAdmission::admit(
            &self.runtime,
            ticket.thread,
            codex_extension_api::HostWorkKind::Tool,
            "governed MCP HTTP",
        )?;
        let execution = ExecutionId::new();
        let binding = ticket.binding.clone();
        let effect = ticket.effect.clone();
        let run = execution.clone();
        let plan = ticket.plan.clone();
        self.worker.run(move |context| {
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
            if current.state != EffectState::Validated || current.execution.is_some() {
                return Err("remote MCP operation already admitted".into());
            }
            context.tool_advance(
                &binding,
                &effect,
                EffectState::Authorized,
                None,
                vec![plan.clone()],
                "remote MCP current authority admitted",
            )?;
            context.tool_advance(
                &binding,
                &effect,
                EffectState::DispatchRecorded,
                Some(run),
                vec![plan],
                "remote MCP intent recorded before DNS or socket connection",
            )
        })?;
        ticket.unsent.dispatched();
        let mut guard = CallGuard {
            worker: self.worker.clone(),
            runtime: self.runtime.clone(),
            binding: ticket.binding.clone(),
            effect: ticket.effect.clone(),
            finished: false,
        };
        let mut wire_run = WireRun {
            execution: execution.clone(),
            sequence: 0,
            usable: true,
            sensitive: vec![],
            body_bytes: 0,
            controls: 0,
            artifacts: vec![],
            deadline: tokio::time::Instant::now()
                + Duration::from_millis(ticket.registration.limits.timeout_ms),
        };
        if ticket.operation.discovery()
            && slot.connection.as_ref().is_some_and(|connection| {
                connection.credential_revision
                    != ticket.credential.as_ref().map(CredentialLease::revision)
            })
        {
            slot.connection.take();
        }
        let mut connection = match slot.connection.take() {
            Some(connection) => {
                if connection
                    .registration
                    .pure()?
                    .digest()
                    .map_err(|e| e.to_string())?
                    != ticket.registration_digest
                {
                    return Err("remote registration changed; reconnect required".into());
                }
                connection
            }
            None => {
                if !ticket.operation.discovery() {
                    return Err("remote MCP connection disappeared".into());
                }
                RemoteConnection {
                    session: Session::new(ticket.registration.pure()?)
                        .map_err(|e| e.to_string())?,
                    registration: ticket.registration.clone(),
                    evidence: ticket.plan.clone(),
                    credential_revision: ticket.credential.as_ref().map(CredentialLease::revision),
                    cache: Default::default(),
                }
            }
        };
        let result: Result<(serde_json::Value, bool), String> = async {
            if connection.session.connection().is_none() {
                let initialize = connection.session.initialize().map_err(|e| e.to_string())?;
                match self
                    .remote_exchange(&ticket, &mut connection.session, initialize, &mut wire_run)
                    .await?
                {
                    Some(Incoming::Initialized { notification, .. }) => {
                        self.remote_exchange(
                            &ticket,
                            &mut connection.session,
                            notification,
                            &mut wire_run,
                        )
                        .await?;
                    }
                    _ => return Err("remote MCP initialization response rejected".into()),
                }
            }
            for _ in 0..ticket.registration.limits.pages {
                let outbound = ticket.operation.begin_session(&mut connection.session)?;
                let incoming = self
                    .remote_exchange(&ticket, &mut connection.session, outbound, &mut wire_run)
                    .await?
                    .ok_or("remote MCP response absent")?;
                if let Some(observed) = content::observe(incoming, &ticket.server)? {
                    let mut value = observed.value;
                    if ticket.operation.discovery() {
                        value["catalog"]["schema_version"] = 1.into();
                        value["catalog"]["server"] = ticket.server.clone().into();
                        value["catalog"]["connection"] =
                            serde_json::json!(connection.session.connection());
                        value["catalog"]["registration_artifact"] =
                            serde_json::json!(connection.evidence);
                    }
                    let safe = match sanitized_json(
                        &ticket,
                        &wire_run,
                        &vcp_protocol::canonical_bytes(&value).map_err(|e| e.to_string())?,
                    ) {
                        Ok(bytes) => serde_json::from_slice(&bytes)
                            .map_err(|_| "remote result sanitization rejected")?,
                        Err(_) if !ticket.operation.discovery() => {
                            serde_json::json!({"omitted":"sensitive result","observed_reply":true})
                        }
                        Err(_) => return Err("sensitive discovery identity rejected".into()),
                    };
                    if ticket.operation.discovery() && safe != value {
                        return Err("sensitive discovery identity rejected".into());
                    }
                    return Ok((safe, observed.success));
                }
            }
            Err("remote MCP discovery page ceiling".into())
        }
        .await;
        let (value, success) = match result {
            Ok(value) => value,
            Err(_) => {
                connection.session.abort();
                guard.unknown("remote MCP exchange failed; no automatic replay")?;
                permit.complete()?;
                return Ok(ControlOutcome {
                    value: serde_json::json!({"effect":ticket.effect,"outcome":"unknown","replay":false,"reason":"remote MCP exchange failed"}),
                    artifacts: wire_run.artifacts,
                });
            }
        };
        #[cfg(feature = "qualification")]
        if success {
            if let Some((arrived, release)) = ticket.before_receipt.take() {
                arrived.notify_one();
                tokio::time::timeout_at(wire_run.deadline, release.notified())
                    .await
                    .map_err(|_| "MCP qualification receipt barrier deadline")?;
            }
        }
        let document = serde_json::json!({"schema_version":1,"effect":ticket.effect,"execution":execution,"identity":ticket.operation.receipt_identity(),"result":value,"source":ticket.provenance.evidence()?,"wire_artifacts":wire_run.artifacts});
        let binding = ticket.binding.clone();
        let effect = ticket.effect.clone();
        let captured = document.clone();
        let exec = execution.clone();
        let artifact = self.worker.run_cleanup(move |context| {
            let artifact = context.capture(
                &binding.scope,
                Channel::Evidence,
                &vcp_protocol::canonical_bytes(&captured)?,
                "vcp-mcp-http-result-v1",
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
                Some(exec),
                evidence,
                "remote MCP protocol result observed",
            )?;
            Ok(artifact)
        })?;
        guard.finished = true;
        permit.complete()?;
        let cacheable = success
            && wire_run.usable
            && ticket
                .operation
                .resource()
                .is_some_and(|identity| connection.session.resource(identity).is_ok());
        if wire_run.usable {
            if cacheable {
                if let Some(identity) = ticket.operation.resource() {
                    while connection.cache.len() >= ticket.registration.limits.tools as usize {
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
        }
        let mut value = value;
        value["external_content"] = true.into();
        value["grants_authority"] = false.into();
        value["cache_available"] = serde_json::json!(cacheable);
        value["effect"] = serde_json::json!(ticket.effect);
        value["outcome"] = serde_json::json!(if success { "succeeded" } else { "failed" });
        value["artifact"] = serde_json::json!(artifact.spec.id);
        if !ticket.operation.discovery() {
            value["receipt"] = document;
        }
        wire_run.artifacts.push(artifact.spec.id);
        Ok(ControlOutcome {
            value,
            artifacts: wire_run.artifacts,
        })
    }
    async fn remote_exchange(
        &self,
        ticket: &RemoteProposal,
        session: &mut Session,
        outbound: Outbound,
        run: &mut WireRun,
    ) -> Result<Option<Incoming>, String> {
        if run.sequence >= 128 {
            return Err("remote MCP wire attempt ceiling".into());
        }
        run.sequence += 1;
        let wire_sequence = run.sequence;
        let exchange_id = session
            .begin_exchange(&outbound)
            .map_err(|e| e.to_string())?;
        session
            .validate_send(&exchange_id, &outbound)
            .map_err(|e| e.to_string())?;
        let mut headers = http::HeaderMap::new();
        session
            .headers(&exchange_id)
            .map_err(|e| e.to_string())?
            .visit(|name, value| {
                if let (Ok(name), Ok(value)) = (
                    http::header::HeaderName::from_bytes(name.as_bytes()),
                    http::HeaderValue::from_str(value),
                ) {
                    headers.insert(name, value);
                }
            });
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            http::header::ACCEPT,
            http::HeaderValue::from_static("application/json, text/event-stream"),
        );
        if let Some(lease) = &ticket.credential {
            let header = lease
                .with_bearer(timestamp(), |value| {
                    http::HeaderValue::from_str(&format!("Bearer {value}"))
                })
                .map_err(|e| e.to_string())?
                .map_err(|_| "remote credential header rejected")?;
            headers.insert(http::header::AUTHORIZATION, header);
        }
        remember_session(session, run);
        self.remote_admit_wire(ticket, run, &outbound)?;
        let endpoint = url::Url::parse(ticket.registration.profile.endpoint())
            .map_err(|_| "remote endpoint rejected")?;
        let host = match endpoint.host().ok_or("remote endpoint host absent")? {
            url::Host::Domain(host) => host.to_owned(),
            url::Host::Ipv4(ip) => ip.to_string(),
            url::Host::Ipv6(ip) => ip.to_string(),
        };
        let port = endpoint
            .port_or_known_default()
            .ok_or("remote port absent")?;
        let mut addresses =
            tokio::time::timeout_at(run.deadline, tokio::net::lookup_host((host.as_str(), port)))
                .await
                .map_err(|_| "remote DNS deadline")?
                .map_err(|_| "remote DNS failed")?
                .collect::<Vec<_>>();
        addresses.sort();
        addresses.dedup();
        let address = *addresses.first().ok_or("remote DNS empty")?;
        let authority = if matches!(endpoint.host(), Some(url::Host::Ipv6(_))) {
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        };
        let tls = wire::Tls {
            name: rustls::pki_types::ServerName::try_from(host.to_owned())
                .map_err(|_| "remote TLS name rejected")?,
            trust: ticket.registration.trust.clone(),
        };
        let request = wire::Outbound {
            address,
            authority,
            path: endpoint.path().into(),
            tls: Some(tls),
            headers,
            body: outbound.bytes().to_vec(),
            limits: wire::Limits {
                request_bytes: 1024 * 1024,
                response_bytes: (ticket.registration.limits.total_discovery_bytes as usize)
                    .saturating_sub(run.body_bytes)
                    .max(1),
                wire_bytes: 32 * 1024 * 1024,
                header_bytes: 16 * 1024,
                header_count: 32,
                deadline: run.deadline,
            },
        };
        if !matches!(
            self.remote_ticket_decision(ticket)?,
            vcp_policy::Decision::Allow { .. }
        ) {
            return Err("remote authority changed after DNS".into());
        }
        let mut exchange = wire::Exchange::start(
            self.runtime.clone(),
            ticket.thread,
            ticket.generation,
            request,
            ticket.credential.clone(),
        )
        .await
        .map_err(|_| "remote HTTP connection failed")?;
        #[cfg(feature = "qualification")]
        wait_qualification_barrier(ticket, run, false).await?;
        // Exchange::start completes TCP/TLS but never polls its HTTP driver.
        // Native file/skill changes during setup must be checked before payload.
        if !matches!(
            self.remote_ticket_decision(ticket)?,
            vcp_policy::Decision::Allow { .. }
        ) {
            return Err("remote source changed during TLS setup".into());
        }
        #[cfg(feature = "qualification")]
        wait_qualification_barrier(ticket, run, true).await?;
        let mut decoder: Option<Decoder> = None;
        let mut reply = None;

        macro_rules! observed {
            ($result:expr) => {
                match $result {
                    Ok(value) => value,
                    Err(_) => {
                        session.abort();
                        run.usable = false;
                        if reply.is_some() {
                            return Ok(reply.take());
                        }
                        return Err("remote protocol observation rejected".into());
                    }
                }
            };
        }

        let result: Result<Option<Incoming>, String> = async {
            loop {
                let event = match exchange.next().await {
                    Ok(event) => event,
                    Err(_) => {
                        session.abort();
                        run.usable = false;
                        if reply.is_some() {
                            return Ok(reply.take());
                        }
                        return Err("remote HTTP observation failed".into());
                    }
                };
                match event {
                    wire::Event::Written(written) => {
                        if !exchange.owns(&written) || written.body_digest() != outbound.digest() {
                            return Err("remote HTTP write observation mismatch".into());
                        }
                        session
                            .written(&exchange_id, &outbound)
                            .map_err(|e| e.to_string())?;
                        self.remote_observe_wire(ticket, run, wire_sequence, serde_json::json!({
                            "kind": "request_fully_flushed", "digest": written.body_digest()
                        }))?;
                    }
                    wire::Event::Head { status, headers } => {
                        let session_header = single_header(&headers, "mcp-session-id")?;
                        let content_type = single_header(&headers, "content-type")?;
                        remember_session(session, run);
                        if let Some(value) = session_header {
                            remember_secret(run, value);
                        }
                        let response_to = session
                            .head(&exchange_id, status.as_u16(), session_header)
                            .map_err(|e| e.to_string())?;
                        decoder = Some(
                            Decoder::new(
                                response_to,
                                status.as_u16(),
                                content_type,
                                vcp_mcp::http::Limits {
                                    body_bytes: (ticket.registration.limits.total_discovery_bytes
                                        as usize)
                                        .saturating_sub(run.body_bytes)
                                        .max(1),
                                    frame_bytes: (ticket.registration.limits.frame_bytes as usize)
                                        .min(
                                            (ticket.registration.limits.total_discovery_bytes
                                                as usize)
                                                .saturating_sub(run.body_bytes)
                                                .max(1),
                                        ),
                                    ..Default::default()
                                },
                                std::time::Instant::now(),
                                run.deadline.into_std(),
                            )
                            .map_err(|e| e.to_string())?,
                        );
                    }
                    wire::Event::Chunk(bytes) => {
                        run.body_bytes = run
                            .body_bytes
                            .checked_add(bytes.len())
                            .ok_or("remote response byte overflow")?;
                        if run.body_bytes
                            > ticket.registration.limits.total_discovery_bytes as usize
                        {
                            session.abort();
                            run.usable = false;
                            if reply.is_some() {
                                return Ok(reply.take());
                            }
                            return Err("remote operation output ceiling".into());
                        }

                        let mut at = 0;
                        while at < bytes.len() {
                            let step = match decoder
                                .as_mut()
                                .ok_or("remote HTTP head absent")?
                                .push(&bytes[at..], std::time::Instant::now())
                            {
                                Ok(step) => step,
                                Err(_) => {
                                    session.abort();
                                    run.usable = false;
                                    if reply.is_some() {
                                        return Ok(reply.take());
                                    }
                                    return Err("remote HTTP framing rejected".into());
                                }
                            };
                            if step.consumed == 0 {
                                return Err("remote HTTP decoder made no progress".into());
                            }
                            at += step.consumed;
                            if let Some(frame) = step.frame {
                                if let Some(raw) = frame.json_bytes() {
                                    let incoming = observed!(session.receive(&exchange_id, raw));
                                    remember_session(session, run);
                                    self.remote_capture_frame(ticket, run, raw)?;
                                    match incoming {
                                        Incoming::ControlReply(control) => {
                                            run.controls += 1;
                                            if run.controls > 32 {
                                                return Err("remote callback ceiling".into());
                                            }
                                            Box::pin(
                                                self.remote_exchange(ticket, session, control, run),
                                            )
                                            .await?;
                                        }
                                        Incoming::Notification(_) => (),
                                        other => reply = Some(other),
                                    }
                                }
                            }
                        }
                    }
                    wire::Event::End { sent_bytes, received_bytes } => {
                    self.remote_observe_wire(ticket,run,wire_sequence,serde_json::json!({"kind":"response_stream_closed","tcp_sent_bytes":sent_bytes,"tcp_received_bytes":received_bytes}))?;
                        let end = observed!(decoder
                            .take()
                            .ok_or("remote HTTP response absent")?
                            .finish(std::time::Instant::now()));
                        if end.discarded_partial_event {
                            run.usable = false;
                        }
                        if let Some(frame) = end.frame {
                            if let Some(raw) = frame.json_bytes() {
                                let incoming = observed!(session.receive(&exchange_id, raw));
                                remember_session(session, run);
                                self.remote_capture_frame(ticket, run, raw)?;
                                match incoming {
                                    Incoming::ControlReply(control) => {
                                        run.controls += 1;
                                        if run.controls > 32 {
                                            return Err("remote callback ceiling".into());
                                        }
                                        Box::pin(
                                            self.remote_exchange(ticket, session, control, run),
                                        )
                                        .await?;
                                    }
                                    Incoming::Notification(_) => (),
                                    other => reply = Some(other),
                                }
                            }
                        }
                        observed!(session.finish(&exchange_id, &outbound, end.summary.body_bytes));
                        return Ok(reply.take());
                    }
                }
            }
        }
        .await;
        if result.is_err() && reply.is_some() {
            session.abort();
            run.usable = false;
            return Ok(reply.take());
        }
        result
    }
}
fn single_header<'a>(headers: &'a http::HeaderMap, name: &str) -> Result<Option<&'a str>, String> {
    let values = headers.get_all(name);
    let mut iter = values.iter();
    let first = iter.next();
    if iter.next().is_some() {
        return Err("duplicate remote HTTP control header".into());
    }
    first
        .map(|value| {
            value
                .to_str()
                .map_err(|_| "remote HTTP control header rejected".into())
        })
        .transpose()
}
impl CanonicalHost {
    fn remote_admit_wire(
        &self,
        ticket: &RemoteProposal,
        run: &mut WireRun,
        outbound: &Outbound,
    ) -> Result<(), String> {
        if !matches!(
            self.remote_ticket_decision(ticket)?,
            vcp_policy::Decision::Allow { .. }
        ) {
            return Err("remote current wire authority denied".into());
        }
        let body = sanitized_json(ticket, run, outbound.bytes())
            .map_err(|_| "remote request capture rejected")?;
        let binding = ticket.binding.clone();
        let effect = ticket.effect.clone();
        let execution = run.execution.clone();
        let sequence = run.sequence;
        let digest = outbound.digest().to_owned();

        let source = ticket.provenance.clone();
        let server = ticket.server.clone();
        let registration = ticket.registration_digest.clone();
        let authority = ticket.authority.clone();
        let credential = ticket.credential.clone();
        let expected = ticket.pin.clone();
        let artifact=self.worker.run(move|context|{
            let (configured,pin)=context.remote_mcp_setup(&server)?;
            if pin!=expected{return Err("remote HTTP owner pin changed".into());}
            if let Some(lease)=credential{lease.validate_authority(&configured.profile,&pin,timestamp())?;}
            if !matches!(context.remote_mcp_decision(&binding,&server,&registration,&authority,&source)?,vcp_policy::Decision::Allow{..}) {return Err("remote wire source or authority changed".into());}
            let current:Effect=context.engine.store().state().record(Collection::Effect,effect.as_str(),&binding.scope.workspace)?.decode()?;
            if current.execution.as_ref()!=Some(&execution) || !matches!(current.state,EffectState::DispatchRecorded|EffectState::Running){return Err("remote HTTP parent intent is not current".into());}
            let document=serde_json::json!({"schema_version":1,"effect":effect,"execution":execution,"sequence":sequence,"request_digest":digest,"body":serde_json::from_slice::<serde_json::Value>(&body)?,"source":source.evidence()?,"delivery":"intent only; not a remote receipt"});
            let artifact=context.capture(&binding.scope,Channel::Evidence,&vcp_protocol::canonical_bytes(&document)?,"vcp-mcp-http-wire-intent-v1")?;
            let mut evidence=current.observed_changes;evidence.push(artifact.spec.id.clone());
            if current.state == EffectState::DispatchRecorded {
            context.tool_advance(&binding,&effect,EffectState::Running,Some(execution),evidence,"remote HTTP wire attempt admitted under parent operation claim")?;
            }
            Ok(artifact.spec.id)
        })?;
        run.artifacts.push(artifact);
        Ok(())
    }
    fn remote_capture_frame(
        &self,
        ticket: &RemoteProposal,
        run: &mut WireRun,
        raw: &[u8],
    ) -> Result<(), String> {
        let bytes = sanitized_json(ticket, run, raw).unwrap_or_else(|_| {
            br#"{"omitted":"sensitive or malformed external content"}"#.to_vec()
        });
        let binding = ticket.binding.clone();
        let artifact = self.worker.run_cleanup(move |context| {
            context.capture(
                &binding.scope,
                Channel::Evidence,
                &bytes,
                "vcp-mcp-http-observed-frame-v1",
            )
        })?;
        run.artifacts.push(artifact.spec.id);
        Ok(())
    }
}

fn remember_secret(run: &mut WireRun, value: &str) {
    if !run
        .sensitive
        .iter()
        .any(|existing| existing.as_str() == value)
    {
        run.sensitive
            .push(zeroize::Zeroizing::new(value.to_owned()));
    }
}
fn remember_session(session: &Session, run: &mut WireRun) {
    session.visit_sensitive_values(|value| remember_secret(run, value));
}
fn sanitized_json(ticket: &RemoteProposal, run: &WireRun, raw: &[u8]) -> Result<Vec<u8>, String> {
    let mut safe = if let Some(lease) = &ticket.credential {
        lease.sanitize_json(raw).map_err(|e| e.to_string())?
    } else {
        raw.to_vec()
    };
    for secret in &run.sensitive {
        safe = super::remote_authority::CaptureSecret::new(secret)
            .map_err(|e| e.to_string())?
            .sanitize_json(&safe)
            .map_err(|e| e.to_string())?;
    }
    if let Some(lease) = &ticket.credential {
        if lease.contains_secret(&safe).map_err(|e| e.to_string())? {
            return Err("sensitive capture rejected".into());
        }
    }
    for secret in &run.sensitive {
        if super::remote_authority::CaptureSecret::new(secret)
            .map_err(|e| e.to_string())?
            .contains_secret(&safe)
            .map_err(|e| e.to_string())?
        {
            return Err("sensitive capture rejected".into());
        }
    }
    Ok(safe)
}

#[cfg(feature = "qualification")]
async fn wait_qualification_barrier(
    ticket: &RemoteProposal,
    run: &WireRun,
    after: bool,
) -> Result<(), String> {
    if run.sequence == 1 {
        if let Some((selected, arrived, release)) = &ticket.before_http {
            if *selected == after {
                arrived.notify_one();
                tokio::time::timeout_at(run.deadline, release.notified())
                    .await
                    .map_err(|_| "remote qualification barrier deadline")?;
            }
        }
    }
    Ok(())
}
impl CanonicalHost {
    #[cfg(feature = "qualification")]
    pub async fn qualification_prepare_remote_mcp_context_call(
        &self,
        thread: ThreadId,
        request: Request,
        sealed: vcp_context::manifest::Sealed,
        roots: Vec<vcp_repository::Root>,
    ) -> Result<RemoteProposal, String> {
        let context = self
            .worker
            .run(move |context| context.verify_context(sealed))?;
        let provenance = Provenance::from_context(context, roots, None);
        let shared = self.mcp_slot(thread, request.server())?;
        let slot = shared.lock().await;
        self.remote_prepare(thread, &request, provenance, &slot.remote)
    }
}

impl CanonicalHost {
    fn remote_observe_wire(
        &self,
        ticket: &RemoteProposal,
        run: &mut WireRun,
        sequence: u32,
        observation: serde_json::Value,
    ) -> Result<(), String> {
        let binding = ticket.binding.clone();
        let effect = ticket.effect.clone();
        let execution = run.execution.clone();
        let artifact=self.worker.run_cleanup(move|context|context.capture(&binding.scope,Channel::Evidence,&vcp_protocol::canonical_bytes(&serde_json::json!({"schema_version":1,"effect":effect,"execution":execution,"sequence":sequence,"observation":observation,"remote_effect_receipt":false}))?,"vcp-mcp-http-wire-observation-v1"))?;
        run.artifacts.push(artifact.spec.id);
        Ok(())
    }
}

impl CanonicalHost {
    fn remote_cached(
        &self,
        thread: ThreadId,
        slot: &RemoteSlot,
        artifact: &ArtifactId,
        mut provenance: Provenance,
    ) -> Result<ControlOutcome, String> {
        let connection = slot
            .connection
            .as_ref()
            .ok_or("remote MCP cache requires a current connection")?;
        let entry = connection
            .cache
            .get(artifact)
            .ok_or("remote MCP cached resource unavailable")?
            .clone();
        connection
            .session
            .resource(&entry.identity)
            .map_err(|e| e.to_string())?;
        let binding = self.binding(thread)?;
        if entry.scope != binding.scope {
            return Err("remote MCP cached resource scope rejected".into());
        }
        let registration = connection.registration.clone();
        let expected = registration.pure()?.digest().map_err(|e| e.to_string())?;
        let server = registration.profile.config().server.clone();
        let credential_revision = connection.credential_revision;
        let resolver = self.mcp.clone();
        self.worker.run(move |context| {
            if provenance.revisions.is_none() {
                provenance.revisions = Some(context.context_revisions(&binding)?);
            }
            context.validate_mcp_provenance(&binding, &provenance)?;
            let (current, pin) = context.remote_mcp_setup(&server)?;
            if current.pure()?.digest()? != expected
                || !current.allowed_resources.contains(entry.identity.key())
            {
                return Err("remote MCP cached resource registration changed".into());
            }
            let lease = if current.profile.config().credential_ref.is_some() {
                Some(
                    resolver
                        .credentials
                        .resolve_current(&current.profile, &pin, timestamp())?,
                )
            } else {
                None
            };
            if lease.as_ref().map(CredentialLease::revision) != credential_revision {
                return Err("remote MCP cached resource credential changed".into());
            }
            context.read_mcp_cache(&binding, &entry.artifact)
        })
    }
}

// Admission examines complete trusted JSON and the checked argument tree separately.
// The outer operation contains JSON strings; it cannot replace argument validation.
fn screen_session_json(
    filter: &super::remote_authority::CaptureSecret<'_>,
    bytes: &[u8],
) -> Result<(), String> {
    let rejected = || "sensitive remote MCP operation rejected".to_owned();
    if filter.contains_secret(bytes).map_err(|_| rejected())? {
        return Err(rejected());
    }
    let safe = filter.sanitize_json(bytes).map_err(|_| rejected())?;
    let original: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| rejected())?;
    let sanitized: serde_json::Value = serde_json::from_slice(&safe).map_err(|_| rejected())?;
    if original != sanitized {
        return Err(rejected());
    }
    Ok(())
}
