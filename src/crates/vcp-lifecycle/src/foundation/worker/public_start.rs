// SPDX-License-Identifier: Apache-2.0
//! Fresh durable run acceptance is distinct from permission to construct or send.
use super::public_connection::PublicConnection;
use super::public_resume::{startup, PublicResumeStartup};
use super::*;
use codex_extension_api::HostWorkAdmission;
use codex_protocol::ThreadId;
use vcp_engine::{
    controller::ControllerToken,
    public::PublicError,
    public_start::{AcceptedPublicStart, PreparedPublicStart, StartFacts},
};
use vcp_protocol::{jsonrpc::RpcError, methods::TurnStart};

pub enum PublicStartAdmission {
    Replay(CommandReceipt),
    Ready(PublicStartPreparation),
}
pub struct PublicStartPreparation {
    prepared: PreparedPublicStart,
    access: Access,
    connection: ControllerId,
    token: ControllerToken,
}
pub enum PublicStartOutcome {
    Replay(CommandReceipt),
    Accepted {
        receipt: CommandReceipt,
        ticket: PublicStartTicket,
    },
}
/// Only this process's fresh Accepted result creates this one-use capability.
/// A durable receipt or reconnect cannot reconstruct it.
pub struct PublicStartTicket {
    request: TurnStart,
    receipt: CommandReceipt,
    access: Access,
    connection: ControllerId,
    token: ControllerToken,
    scope: Scope,
    startup_used: bool,
    attached: Arc<Mutex<Option<ThreadId>>>,
}
impl PublicStartTicket {
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
}
pub struct PublicStartStartup {
    startup: PublicResumeStartup,
    attached: Arc<Mutex<Option<ThreadId>>>,
}
impl PublicStartStartup {
    pub fn work_admission(&self) -> Arc<dyn HostWorkAdmission> {
        self.startup.work_admission()
    }
}

impl PublicConnection {
    pub fn prepare_start_rpc(
        &self,
        request: TurnStart,
        current: &Access,
    ) -> std::result::Result<PublicStartAdmission, RpcError> {
        let operation = Some(request.mutation.command_id.clone());
        let error = |e| vcp_engine::rpc::public_error(e, operation.clone(), false);
        let (host, access, connection, token) = self
            .rpc_context(current)
            .map_err(|_| error(PublicError::Access))?;
        let token = token.ok_or_else(|| error(PublicError::Access))?;
        let runtime = host.runtime.clone();
        let bindings = host.bindings.clone();
        let scheduler = host.scheduler.clone();
        let result = host
            .worker
            .run(move |context| {
                Ok(
                    (|| -> std::result::Result<PublicStartAdmission, PublicError> {
                        context
                            .check_public_controller(&access, &connection, &token)
                            .map_err(|_| PublicError::Access)?;
                        let prepared = match context.engine.prepare_public_start(
                            request,
                            &access,
                            &connection,
                            &token,
                        )? {
                            vcp_engine::public_start::PublicStartAdmission::Replay(receipt) => {
                                return Ok(PublicStartAdmission::Replay(receipt))
                            }
                            vcp_engine::public_start::PublicStartAdmission::Ready(prepared) => {
                                prepared
                            }
                        };
                        context.check_start_selection(prepared.request())?;
                        let state = runtime
                            .0
                            .state
                            .lock()
                            .map_err(|_| PublicError::OutcomeUnknown)?;
                        if !startup::available(&state)
                            || state.root_startup.is_some()
                            || scheduler.busy()
                            || !bindings
                                .lock()
                                .map_err(|_| PublicError::OutcomeUnknown)?
                                .is_empty()
                        {
                            return Err(PublicError::StaleState);
                        }
                        Ok(PublicStartAdmission::Ready(PublicStartPreparation {
                            prepared,
                            access,
                            connection,
                            token,
                        }))
                    })(),
                )
            })
            .map_err(|_| error(PublicError::OutcomeUnknown))?;
        result.map_err(error)
    }

    /// Observe native disk state inside the trusted owner. Profile requirements
    /// are trusted configuration; the public request cannot supply fingerprint,
    /// protected reserve, editing authority, checks or policy observations.
    pub fn accept_start(
        &self,
        preparation: PublicStartPreparation,
        current: &Access,
        editing: bool,
        required_checks: Vec<String>,
    ) -> std::result::Result<PublicStartOutcome, RpcError> {
        let operation = Some(preparation.prepared.request().mutation.command_id.clone());
        let error = |e| vcp_engine::rpc::public_error(e, operation.clone(), false);
        let _serial = self
            .host
            .public_resume
            .lock()
            .map_err(|_| error(PublicError::OutcomeUnknown))?;
        self.check_start_identity(
            &preparation.access,
            &preparation.connection,
            &preparation.token,
            current,
        )
        .map_err(|_| error(PublicError::Access))?;
        let host = self.host.clone();
        let runtime = host.runtime.clone();
        let bindings = host.bindings.clone();
        let scheduler = host.scheduler.clone();
        let attempted = Arc::new(AtomicBool::new(false));
        let committing = attempted.clone();
        let result = host.worker.run(move |context| {
            Ok(
                (|| -> std::result::Result<PublicStartOutcome, PublicError> {
                    let PublicStartPreparation {
                        prepared,
                        access,
                        connection,
                        token,
                    } = preparation;
                    context
                        .check_public_controller(&access, &connection, &token)
                        .map_err(|_| PublicError::Access)?;
                    let request = prepared.request().clone();
                    let prepared = match context.engine.prepare_public_start(
                        request.clone(),
                        &access,
                        &connection,
                        &token,
                    )? {
                        vcp_engine::public_start::PublicStartAdmission::Replay(receipt) => {
                            return Ok(PublicStartOutcome::Replay(receipt))
                        }
                        vcp_engine::public_start::PublicStartAdmission::Ready(prepared) => prepared,
                    };
                    context.check_start_selection(&request)?;
                    let root = context.tool_root().map_err(|_| PublicError::Unavailable)?;
                    let _root_pin = root
                        .hold(None, true)
                        .map_err(|_| PublicError::Unavailable)?;
                    let observed = context
                        .runtime
                        .block_on(root.observe(None, &Default::default()))
                        .map_err(|_| PublicError::Unavailable)?;
                    if !observed.manifest.bounded_scan_complete {
                        return Err(PublicError::Unavailable);
                    }
                    let facts = StartFacts {
                        fingerprint: vcp_domain::verification::Fingerprint {
                            repository: observed.digest,
                            buffers: vcp_protocol::digest_bytes(b""),
                            environment: vcp_protocol::digest_bytes(
                                b"vcp-cli-explicit-user-profile/1",
                            ),
                        },
                        editing,
                        required_checks,
                        protected: context.config.protected,
                        policy: vcp_engine::policy::optional(
                            context.engine.store().state(),
                            &access.workspace,
                        )
                        .map_err(|_| PublicError::Unavailable)?
                        .map_or(PolicyRevision::ZERO, |p| p.revision),
                    };
                    let scope = Scope {
                        workspace: access.workspace.clone(),
                        session: access.session.clone(),
                        task: TaskId::parse(request.task.as_str())
                            .map_err(|_| PublicError::InvalidParameters)?,
                    };
                    // A newly accepted task does not yet exist, so the ordinary
                    // capture helper's AttachArtifact command cannot be used here.
                    let mut writer = context
                        .engine
                        .store()
                        .spool()
                        .create(context.spec(&scope, Channel::Evidence, "coding-turn-input/1"))
                        .map_err(|_| PublicError::Unavailable)?;
                    for chunk in request
                        .objective
                        .as_bytes()
                        .chunks(vcp_store::artifact::CHUNK_BYTES)
                    {
                        writer
                            .write_chunk(chunk)
                            .map_err(|_| PublicError::Unavailable)?;
                    }
                    let trigger = writer.finalize().map_err(|_| PublicError::Unavailable)?;
                    drop(writer);
                    let mut state = runtime
                        .0
                        .state
                        .lock()
                        .map_err(|_| PublicError::OutcomeUnknown)?;
                    context
                        .check_public_controller(&access, &connection, &token)
                        .map_err(|_| PublicError::Access)?;
                    context.check_start_selection(&request)?;
                    if !startup::available(&state)
                        || state.root_startup.is_some()
                        || scheduler.busy()
                        || !bindings
                            .lock()
                            .map_err(|_| PublicError::OutcomeUnknown)?
                            .is_empty()
                    {
                        return Err(PublicError::StaleState);
                    }
                    state.advance().map_err(|_| PublicError::OutcomeUnknown)?;
                    state.root_admission_held = true;
                    state
                        .checkpoint()
                        .map_err(|_| PublicError::OutcomeUnknown)?;
                    committing.store(true, Ordering::SeqCst);
                    let outcome = context
                        .runtime
                        .block_on(context.engine.commit_public_start(
                            prepared,
                            &access,
                            &facts,
                            &trigger,
                            now(),
                        ))?;
                    match outcome {
                        vcp_engine::public_start::PublicStartOutcome::Replay(receipt) => {
                            Ok(PublicStartOutcome::Replay(receipt))
                        }
                        vcp_engine::public_start::PublicStartOutcome::Accepted(receipt) => {
                            Ok(PublicStartOutcome::Accepted {
                                receipt: receipt.clone(),
                                ticket: PublicStartTicket {
                                    request,
                                    receipt,
                                    access,
                                    connection,
                                    token,
                                    scope,
                                    startup_used: false,
                                    attached: Arc::new(Mutex::new(None)),
                                },
                            })
                        }
                    }
                })(),
            )
        });
        let result = result
            .map_err(|_| error(PublicError::OutcomeUnknown))
            .and_then(|result| result.map_err(error));
        if result.is_err() && attempted.load(Ordering::SeqCst) {
            host.worker.fence();
            drop(host.runtime.hold_owner());
        }
        result
    }

    fn check_start_identity(
        &self,
        original: &Access,
        identity: &ControllerId,
        original_token: &ControllerToken,
        current: &Access,
    ) -> std::result::Result<(), String> {
        let (_, access, connection, token) = self.rpc_context(current)?;
        if connection != *identity
            || access.actor != original.actor
            || access.workspace != original.workspace
            || access.session != original.session
            || access.authority != original.authority
            || !token.is_some_and(|token| {
                token.generation() == original_token.generation()
                    && token.revision() == original_token.revision()
            })
        {
            return Err("start ticket authority changed".into());
        }
        Ok(())
    }

    pub fn authorize_start_startup(
        &self,
        ticket: &mut PublicStartTicket,
        current: &Access,
    ) -> std::result::Result<PublicStartStartup, String> {
        let _serial = self
            .host
            .public_resume
            .lock()
            .map_err(|_| "public admission lock poisoned")?;
        self.check_start_identity(&ticket.access, &ticket.connection, &ticket.token, current)?;
        if ticket.startup_used {
            return Err("start constructor ticket already used".into());
        }
        let startup = startup::accepted_start(
            self,
            ticket.request.clone(),
            ticket.receipt.clone(),
            ticket.access.clone(),
            ticket.connection.clone(),
            ticket.token.clone(),
        )?;
        ticket.startup_used = true;
        Ok(PublicStartStartup {
            startup,
            attached: ticket.attached.clone(),
        })
    }

    pub fn attach_start_root(
        &self,
        startup: PublicStartStartup,
        thread: Arc<codex_core::CodexThread>,
        binding: ThreadBinding,
        current: &Access,
    ) -> std::result::Result<ThreadId, String> {
        let id = self.attach_resume_root(startup.startup, thread, binding, current)?;
        *startup
            .attached
            .lock()
            .map_err(|_| "start attachment poisoned")? = Some(id);
        Ok(id)
    }

    /// Consume fresh acceptance authority. This only activates the accepted
    /// canonical task; the retained submission still belongs to the supervisor.
    pub fn activate_start(
        &self,
        ticket: PublicStartTicket,
        current: &Access,
    ) -> std::result::Result<CommandReceipt, String> {
        let _serial = self
            .host
            .public_resume
            .lock()
            .map_err(|_| "public admission lock poisoned")?;
        self.check_start_identity(&ticket.access, &ticket.connection, &ticket.token, current)?;
        let thread = ticket
            .attached
            .lock()
            .map_err(|_| "start attachment poisoned")?
            .ok_or("start constructor has not attached")?;
        let host = self.host.clone();
        let runtime = host.runtime.clone();
        let bindings = host.bindings.clone();
        let attempted = Arc::new(AtomicBool::new(false));
        let committing = attempted.clone();
        let result = host.worker.run(move |context| {
            let proof = context.check_accepted_start(
                &ticket.request,
                &ticket.receipt,
                &ticket.access,
                &ticket.connection,
                &ticket.token,
            )?;
            let binding = bindings
                .lock()
                .map_err(|_| "binding lock poisoned")?
                .get(&thread)
                .cloned()
                .ok_or("start binding lost")?;
            if binding.scope != proof.task.scope {
                return Err("start binding scope mismatch".into());
            }
            context.validate_binding(&binding)?;
            let view = runtime.inspect(thread).map_err(|e| format!("{e:?}"))?;
            if !view.owner_attached || view.inherited_hold || view.interruption_error.is_some() {
                return Err("start owner is not ready".into());
            }
            if view.local_hold {
                runtime
                    .resume(thread, &view.revision)
                    .map_err(|e| format!("{e:?}"))?;
                committing.store(true, Ordering::SeqCst);
            }
            let state = runtime.0.state.lock().map_err(|_| "lifecycle poisoned")?;
            context.check_accepted_start(
                &ticket.request,
                &ticket.receipt,
                &ticket.access,
                &ticket.connection,
                &ticket.token,
            )?;
            if !state.attached
                || state.sealing
                || state.root != Some(thread)
                || state.held(thread)
                || state.startups_in_flight != 0
                || state.entries.values().any(|entry| entry.starts != 0)
                || state.work.iter().any(|work| work.receipt.is_none())
            {
                return Err("start owner changed before activation".into());
            }
            committing.store(true, Ordering::SeqCst);
            context.command(
                Command::Transition {
                    next: TaskState::Running,
                    reason: "fresh public run activation".into(),
                    verification: None,
                },
                Some(proof.task.scope.task),
                proof.task.revision,
            )?;
            drop(state);
            Ok(ticket.receipt)
        });
        if result.is_err() && attempted.load(Ordering::SeqCst) {
            host.worker.fence();
            drop(host.runtime.hold_owner());
        }
        result
    }
}

impl Context {
    fn check_start_selection(&self, request: &TurnStart) -> std::result::Result<(), PublicError> {
        if request.task.as_str() != self.config.root_task.as_str()
            || request.scope.workspace.as_str() != self.config.workspace.as_str()
            || request.scope.session.as_str() != self.config.session.as_str()
            || request.budget.cap_micros.as_str() != self.config.cap.micros.get().to_string()
            || request.budget.currency != vcp_protocol::methods::Currency::Usd
            || self.config.cap.currency.code() != "USD"
        {
            return Err(PublicError::InvalidParameters);
        }
        if !self.owner_alive || self.authority_pending || self.capture_admission_blocked() {
            return Err(PublicError::StaleState);
        }
        Ok(())
    }
    pub(super) fn check_accepted_start(
        &self,
        request: &TurnStart,
        receipt: &CommandReceipt,
        access: &Access,
        connection: &ControllerId,
        token: &ControllerToken,
    ) -> Result<AcceptedPublicStart> {
        self.check_public_controller(access, connection, token)?;
        self.check_start_selection(request)?;
        let accepted = self
            .engine
            .check_accepted_public_start(request, receipt, access, connection, token)?;
        self.check_public_start_budget()?;
        Ok(accepted)
    }
}
