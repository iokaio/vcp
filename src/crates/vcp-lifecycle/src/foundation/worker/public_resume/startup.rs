// SPDX-License-Identifier: Apache-2.0
//! A constructor-specific startup gate: generic admission stays held until the
//! exact authorized constructor attaches. Old completed constructors cannot win.
use super::*;
use codex_extension_api::{
    HostModelPurpose, HostWorkAdmission, HostWorkKind, HostWorkPermit, ToolName,
};
use std::path::{Path, PathBuf};
use vcp_engine::controller::ControllerToken;

pub struct PublicResumeStartup {
    grant: Arc<Grant>,
}
impl PublicResumeStartup {
    /// Install this in the retained constructor's extension registry in place
    /// of the ordinary host work-admission adapter. Do not call generic startup
    /// authorization. Model and tool admission still delegate to the host.
    pub fn work_admission(&self) -> Arc<dyn HostWorkAdmission> {
        self.grant.clone()
    }
}
impl Drop for PublicResumeStartup {
    fn drop(&mut self) {
        if let Ok(mut state) = self.grant.host.runtime.0.state.lock() {
            if state
                .root_startup
                .as_ref()
                .is_some_and(|nonce| Arc::ptr_eq(nonce, &self.grant.nonce))
            {
                state.root_startup = None;
                // Keep generic startup held. A new explicit resume must mint
                // another grant after all constructor permits have drained.
            }
        }
    }
}

#[derive(Clone)]
enum Intent {
    Resume(SessionResume),
    #[cfg(windows)]
    Start {
        request: vcp_protocol::methods::TurnStart,
        receipt: CommandReceipt,
    },
}

struct Grant {
    host: super::super::super::CanonicalHost,
    access: Access,
    connection: ControllerId,
    token: ControllerToken,
    nonce: Arc<()>,
    scope: vcp_domain::workspace::Scope,
    workspace: PathBuf,
    consumed: AtomicBool,
    attached: AtomicBool,
    request: Intent,
}
impl std::fmt::Debug for Grant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PublicResumeStartupAdmission")
    }
}

pub(in crate::foundation::worker) fn available(state: &crate::State) -> bool {
    state.attached
        && !state.sealing
        && !state.owner_sealing
        && state.root.is_none()
        && state.entries.is_empty()
        && state.startups_in_flight == 0
        && state.startups.is_empty()
        && state.root_hold_error.is_none()
        && state
            .commands
            .iter()
            .all(|command| command.result.is_some())
        && state.work.iter().all(|work| work.receipt.is_some())
}

fn check(
    context: &Context,
    request: &Intent,
    access: &Access,
    connection: &ControllerId,
    token: &ControllerToken,
) -> Result<()> {
    context.check_public_controller(access, connection, token)?;
    if context.capture_admission_blocked() {
        return Err("canonical host fenced".into());
    }
    if context.authority_pending {
        return Err("authority change is stopping work".into());
    }
    #[cfg(windows)]
    if let Intent::Start { request, receipt } = request {
        context.check_accepted_start(request, receipt, access, connection, token)?;
        return Ok(());
    }
    let Intent::Resume(request) = request else {
        return Err("unsupported constructor intent".into());
    };
    let facts = HostFacts {
        now: now(),
        policy: PolicyRevision::ZERO,
        resume: None,
        may_execute: context.owner_alive,
    };
    if matches!(
        context.engine.prepare_controlled_public(
            Call::SessionResume(request.clone()),
            access,
            &facts,
            connection,
            token
        )?,
        PublicAdmission::Replay(_)
    ) {
        return Err("resume already accepted; constructor cannot repeat".into());
    }
    #[cfg(windows)]
    context.check_public_start_budget()?;
    Ok(())
}

impl PublicConnection {
    /// Mint one root constructor capability only for an explicit resume whose
    /// original controller is current. Acquire/reconnect never call this path.
    pub fn authorize_resume_startup(
        &self,
        ticket: &mut PublicResumeTicket,
        current: &Access,
    ) -> std::result::Result<PublicResumeStartup, String> {
        let _resume = self
            .host
            .public_resume
            .lock()
            .map_err(|_| "public resume lock poisoned")?;
        self.check_resume_ticket(ticket, current)?;
        if ticket.startup_used {
            return Err("resume startup ticket already used".into());
        }
        let fresh = match self.prepare_resume_inner(ticket.request(), current)? {
            PublicResumeAdmission::Replay(_) => {
                return Err("resume already accepted; startup is not authorized".into());
            }
            PublicResumeAdmission::Ready(fresh) => fresh,
        };
        let host = self.host.clone();
        let runtime = host.runtime.clone();
        let bindings = host.bindings.clone();
        let scheduler = host.scheduler.clone();
        let nonce = Arc::new(());
        let reserved = nonce.clone();
        let scope = fresh.task.scope.clone();
        let requested_scope = scope.clone();
        let (access, connection, token) = match &fresh.commit {
            ResumeCommit::Public {
                access,
                connection,
                token,
                ..
            } => (access.clone(), connection.clone(), token.clone()),
            _ => unreachable!(),
        };
        let workspace = host.worker.run(move |context| {
            if context.recheck_resume(&fresh.commit)?.is_some() {
                return Err("resume already accepted".into());
            }
            if requested_scope.task != context.config.root_task
                || requested_scope.session != context.config.session
                || requested_scope.workspace != context.config.workspace
                || scheduler.busy()
                || !bindings
                    .lock()
                    .map_err(|_| "binding lock poisoned")?
                    .is_empty()
            {
                return Err("resume startup requires a drained configured root".into());
            }
            let workspace = PathBuf::from(&context.config.binding.root).canonicalize()?;
            let mut state = runtime.0.state.lock().map_err(|_| "lifecycle poisoned")?;
            // Loss publishes its marker before waiting for this lifecycle lock.
            context.recheck_resume(&fresh.commit)?;
            if !available(&state) || state.root_startup.is_some() {
                return Err("resume startup is not drained".into());
            }
            state.advance().map_err(|e| format!("{e:?}"))?;
            state.root_admission_held = true;
            state.root_startup = Some(reserved);
            state.checkpoint().map_err(|e| format!("{e:?}"))?;
            Ok(workspace)
        })?;
        ticket.startup_used = true;
        Ok(PublicResumeStartup {
            grant: Arc::new(Grant {
                host,
                access,
                connection,
                token,
                nonce,
                scope,
                workspace,
                consumed: AtomicBool::new(false),
                attached: AtomicBool::new(false),
                request: Intent::Resume(ticket.request()),
            }),
        })
    }

    /// Consume the constructor capability and bind only that returned thread.
    /// On failure the caller must shut down the unbound retained thread.
    pub fn attach_resume_root(
        &self,
        startup: PublicResumeStartup,
        thread: Arc<codex_core::CodexThread>,
        binding: ThreadBinding,
        current: &Access,
    ) -> std::result::Result<ThreadId, String> {
        let (_, access, connection, token) = self.rpc_context(current)?;
        let grant = startup.grant.clone();
        if binding.scope != grant.scope
            || access.authority != grant.access.authority
            || access.actor != grant.access.actor
            || access.workspace != grant.access.workspace
            || access.session != grant.access.session
            || connection != grant.connection
            || !token.is_some_and(|token| {
                token.generation() == grant.token.generation()
                    && token.revision() == grant.token.revision()
            })
        {
            return Err("resume constructor authority changed".into());
        }
        let host = self.host.clone();
        let bindings = host.bindings.clone();
        host.worker.run(move |context| {
            context.validate_binding(&binding)?;
            let mut state = grant
                .host
                .runtime
                .0
                .state
                .lock()
                .map_err(|_| "lifecycle poisoned")?;
            check(
                context,
                &grant.request,
                &access,
                &grant.connection,
                &grant.token,
            )?;
            if !available(&state)
                || !grant.consumed.load(Ordering::SeqCst)
                || !state
                    .root_startup
                    .as_ref()
                    .is_some_and(|nonce| Arc::ptr_eq(nonce, &grant.nonce))
            {
                return Err("resume constructor was revoked or has not drained".into());
            }
            let mut bindings = bindings.lock().map_err(|_| "binding lock poisoned")?;
            if !bindings.is_empty() {
                return Err("resume root already bound".into());
            }
            state.advance().map_err(|e| format!("{e:?}"))?;
            let id = thread.session_configured().thread_id;
            state.root = Some(id);
            state.root_startup = None;
            state.root_admission_held = false;
            state.entries.insert(
                id,
                crate::Entry {
                    thread: Arc::downgrade(&thread),
                    parent: None,
                    held: true,
                    interrupted: true,
                    interruption_error: None,
                    starts: 0,
                    admission_generation: 0,
                },
            );
            state.checkpoint().map_err(|e| format!("{e:?}"))?;
            bindings.insert(id, binding);
            grant.attached.store(true, Ordering::SeqCst);
            Ok(id)
        })
    }
}

impl HostWorkAdmission for Grant {
    fn prepare_model(
        &self,
        thread: ThreadId,
        purpose: HostModelPurpose,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = std::result::Result<(), String>> + Send + '_>,
    > {
        self.host.prepare_model(thread, purpose)
    }
    fn requires_completed_response(&self) -> bool {
        self.host.requires_completed_response()
    }
    fn admit_model(
        &self,
        thread: ThreadId,
        body: &mut serde_json::Value,
        purpose: HostModelPurpose,
    ) -> std::result::Result<Box<dyn HostWorkPermit>, String> {
        self.host.admit_model(thread, body, purpose)
    }
    fn admit_tool(
        &self,
        thread: ThreadId,
        call_id: &str,
        name: &ToolName,
    ) -> std::result::Result<Box<dyn HostWorkPermit>, String> {
        self.host.admit_tool(thread, call_id, name)
    }
    fn admit(
        &self,
        thread: ThreadId,
        kind: HostWorkKind,
        label: &str,
    ) -> std::result::Result<Box<dyn HostWorkPermit>, String> {
        self.host.admit(thread, kind, label)
    }
    fn admit_startup(
        &self,
        workspace: &Path,
        resumed: Option<ThreadId>,
    ) -> std::result::Result<Box<dyn Send>, String> {
        if self.attached.load(Ordering::SeqCst) {
            return self.host.admit_startup(workspace, resumed);
        }
        let workspace = workspace
            .canonicalize()
            .map_err(|_| "startup workspace unavailable")?;
        if resumed.is_some() || workspace != self.workspace {
            return Err("resume startup scope mismatch".into());
        }
        let runtime = self.host.runtime.clone();
        let access = self.access.clone();
        let connection = self.connection.clone();
        let token = self.token.clone();
        let nonce = self.nonce.clone();
        let request = self.request.clone();
        if self.consumed.swap(true, Ordering::SeqCst) {
            return Err("resume constructor already admitted".into());
        }
        self.host.worker.run(move |context| {
            let mut state = runtime.0.state.lock().map_err(|_| "lifecycle poisoned")?;
            check(context, &request, &access, &connection, &token)?;
            if context.authority_pending
                || !available(&state)
                || !state
                    .root_startup
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, &nonce))
            {
                return Err("resume constructor revoked".into());
            }
            state.startups_in_flight = state
                .startups_in_flight
                .checked_add(1)
                .ok_or("startup overflow")?;
            drop(state);
            Ok(Box::new(crate::StartupPermit(runtime)) as Box<dyn Send>)
        })
    }
}

#[cfg(windows)]
pub(in crate::foundation::worker) fn accepted_start(
    connection: &PublicConnection,
    request: vcp_protocol::methods::TurnStart,
    receipt: CommandReceipt,
    access: Access,
    identity: ControllerId,
    token: ControllerToken,
) -> std::result::Result<PublicResumeStartup, String> {
    let host = connection.host.clone();
    let runtime = host.runtime.clone();
    let bindings = host.bindings.clone();
    let scheduler = host.scheduler.clone();
    let nonce = Arc::new(());
    let reserved = nonce.clone();
    let request_check = request.clone();
    let receipt_check = receipt.clone();
    let access_check = access.clone();
    let identity_check = identity.clone();
    let token_check = token.clone();
    let (scope, workspace) = host.worker.run(move |context| {
        let proof = context.check_accepted_start(
            &request_check,
            &receipt_check,
            &access_check,
            &identity_check,
            &token_check,
        )?;
        if scheduler.busy()
            || !bindings
                .lock()
                .map_err(|_| "binding lock poisoned")?
                .is_empty()
        {
            return Err("start requires a drained configured root".into());
        }
        let workspace = PathBuf::from(&context.config.binding.root).canonicalize()?;
        let mut state = runtime.0.state.lock().map_err(|_| "lifecycle poisoned")?;
        context.check_accepted_start(
            &request_check,
            &receipt_check,
            &access_check,
            &identity_check,
            &token_check,
        )?;
        if !available(&state) || state.root_startup.is_some() {
            return Err("start constructor is not drained".into());
        }
        state.advance().map_err(|e| format!("{e:?}"))?;
        state.root_admission_held = true;
        state.root_startup = Some(reserved);
        state.checkpoint().map_err(|e| format!("{e:?}"))?;
        Ok((proof.task.scope, workspace))
    })?;
    Ok(PublicResumeStartup {
        grant: Arc::new(Grant {
            host,
            access,
            connection: identity,
            token,
            nonce,
            scope,
            workspace,
            consumed: AtomicBool::new(false),
            attached: AtomicBool::new(false),
            request: Intent::Start { request, receipt },
        }),
    })
}
