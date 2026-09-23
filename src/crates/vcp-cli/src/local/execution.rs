// SPDX-License-Identifier: Apache-2.0
//! Trusted execution configuration carried only by the bounded private bootstrap.
use super::Role;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

// Intentionally neither Debug nor Clone: credentials are not diagnostics or
// connection grants. Attachments never contain this configuration.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Configuration {
    pub profile: PathBuf,
    pub provider_credential: String,
    #[serde(default)]
    pub credentials: BTreeMap<String, String>,
}

impl Configuration {
    pub(super) fn validate(&self, role: Role) -> Result<(), String> {
        if role != Role::Controller || !self.profile.is_absolute() {
            return Err("execution bootstrap requires controller role and absolute profile".into());
        }
        fn material(value: &str) -> bool {
            !value.is_empty() && value.len() <= 8192 && !value.chars().any(char::is_control)
        }
        if !material(&self.provider_credential)
            || self.credentials.len() > 16
            || self.credentials.iter().any(|(name, value)| {
                name.is_empty()
                    || name.len() > 128
                    || !name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                    || !material(value)
            })
        {
            return Err("execution bootstrap credential material rejected".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resumed_execution_ignores_prior_turn_terminal_events() {
        use codex_protocol::protocol::{Event, EventMsg, TurnAbortReason, TurnAbortedEvent};
        let aborted = |envelope: &str, turn: Option<&str>| Event {
            id: envelope.into(),
            msg: EventMsg::TurnAborted(TurnAbortedEvent {
                turn_id: turn.map(str::to_owned),
                reason: TurnAbortReason::Interrupted,
                started_at: None,
                completed_at: None,
                duration_ms: None,
            }),
        };
        assert!(!event_for_turn(&aborted("old", Some("old")), "resumed"));
        assert!(!event_for_turn(&aborted("old", None), "resumed"));
        assert!(!event_for_turn(&aborted("resumed", Some("old")), "resumed"));
        assert!(event_for_turn(
            &aborted("resumed", Some("resumed")),
            "resumed"
        ));
        assert!(event_for_turn(&aborted("resumed", None), "resumed"));
    }

    #[test]
    fn execution_configuration_is_private_bounded_and_controller_only() {
        let mut config = Configuration {
            profile: PathBuf::from("C:/trusted/profile.json"),
            provider_credential: "synthetic-cli-qualification".into(),
            credentials: BTreeMap::new(),
        };
        assert!(config.validate(Role::Controller).is_ok());
        assert!(config.validate(Role::Observer).is_err());
        config
            .credentials
            .insert("TOKEN".into(), "secret\r\nvalue".into());
        let error = config.validate(Role::Controller).unwrap_err();
        assert!(!error.contains("secret"));
        config.credentials.clear();
        config.profile = PathBuf::from("relative.json");
        assert!(config.validate(Role::Controller).is_err());
        config.profile = PathBuf::from("C:/trusted/profile.json");
        config.provider_credential = "x".repeat(8193);
        assert!(config.validate(Role::Controller).is_err());
    }
}

use std::sync::Arc;
use tokio::sync::Mutex;
use vcp_domain::{
    ids::*,
    task::{Task, TaskState},
    workspace::{Scope, Trust, Workspace},
};
use vcp_engine::{rpc::RpcHost, Access};
use vcp_lifecycle::foundation::{
    CanonicalHost, CanonicalOwner, Config, PublicConnection, PublicResumeAdmission,
    PublicResumeOutcome, PublicResumeTicket, ThreadBinding,
};
use vcp_protocol::{
    command::{Command, CommandReceipt},
    errors::{ApplicationError, Code, Retry},
    jsonrpc::RpcError,
    methods::{self, Call, ResultValue},
};
use vcp_store::contract::Collection;
mod start;

/// The helper process is the final containment boundary for a retained
/// constructor that cannot prove cancellation-safe cleanup. Deadline loss seals
/// admission immediately; the owned future keeps running through cleanup grace.
/// Only then may the dedicated server process exit, preserving crash recovery.
struct ProcessDeadline(std::sync::mpsc::Sender<()>);
impl ProcessDeadline {
    fn start(
        loss: Option<vcp_lifecycle::foundation::PublicConnectionLoss>,
    ) -> Result<Self, String> {
        let (finished, wait) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("local-execution-deadline".into())
            .spawn(move || {
                if !matches!(
                    wait.recv_timeout(std::time::Duration::from_secs(30)),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                ) {
                    return;
                }
                if let Some(loss) = loss {
                    // A blocked lifecycle mutex must not prevent the outer
                    // process deadline. Revocation owns its separate waiter.
                    if std::thread::Builder::new()
                        .name("local-execution-loss".into())
                        .spawn(move || loss.invalidate())
                        .is_err()
                    {
                        std::process::exit(1);
                    }
                }
                if matches!(
                    wait.recv_timeout(std::time::Duration::from_secs(5)),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                ) {
                    // This module is reachable only in the dedicated local helper;
                    // never return while an unjoined constructor could retain the writer.
                    std::process::exit(1);
                }
            })
            .map_err(|_| "local execution deadline unavailable")?;
        Ok(Self(finished))
    }
}
impl Drop for ProcessDeadline {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

struct State {
    session: Option<crate::session::Session>,
    pump: Option<tokio::task::JoinHandle<()>>,
    policy: Option<String>,
    configured: bool,
    deadline: Option<tokio::time::Instant>,
    closed: bool,
}

/// One server owns construction, submission and event consumption across all
/// attachments. A dropped response waiter never owns these operations.
pub(super) struct Supervisor {
    host: CanonicalHost,
    config: Config,
    data: PathBuf,
    configuration: Configuration,
    profile_digest: String,
    state: Mutex<State>,
}
impl Supervisor {
    pub(super) fn new(
        host: CanonicalHost,
        config: Config,
        data: PathBuf,
        configuration: Configuration,
    ) -> Result<Arc<Self>, String> {
        let path = crate::settings::local_path(
            &configuration.profile,
            std::path::Path::new(&config.binding.root),
        )?;
        let profile_digest =
            vcp_protocol::digest_bytes(&crate::settings::read_bounded(&path, 256 * 1024)?);
        Ok(Arc::new(Self {
            host,
            config,
            data,
            configuration,
            profile_digest,
            state: Mutex::new(State {
                session: None,
                pump: None,
                policy: None,
                configured: false,
                deadline: None,
                closed: false,
            }),
        }))
    }

    fn prepare_profile(
        &self,
        previous_policy: Option<&str>,
    ) -> Result<(crate::settings::PreparedProfile, String), String> {
        let workspace = std::path::Path::new(&self.config.binding.root);
        let path = crate::settings::local_path(&self.configuration.profile, workspace)?;
        let bytes = crate::settings::read_bounded(&path, 256 * 1024)?;
        if vcp_protocol::digest_bytes(&bytes) != self.profile_digest {
            return Err("execution profile changed; relaunch required".into());
        }
        let state = self.host.snapshot()?;
        let selected: Workspace = state
            .record(
                Collection::Workspace,
                self.config.workspace.as_str(),
                &self.config.workspace,
            )
            .and_then(|row| row.decode())
            .map_err(|_| "execution workspace unavailable")?;
        if selected.trust != Trust::Trusted || selected.binding != self.config.binding {
            return Err("execution workspace trust or binding changed".into());
        }
        let policy = vcp_engine::policy::optional(&state, &self.config.workspace)
            .map_err(|_| "execution policy unavailable")?
            .ok_or("execution requires an existing canonical policy")?;
        let pin = vcp_protocol::digest_bytes(
            &vcp_protocol::canonical_bytes(&policy).map_err(|_| "execution policy unavailable")?,
        );
        if previous_policy.is_some_and(|previous| previous != pin) {
            return Err("execution policy changed; relaunch required".into());
        }
        let profile: crate::settings::Profile =
            serde_json::from_slice(&bytes).map_err(|_| "invalid execution profile")?;
        if profile.version != 1
            || profile
                .workspace
                .canonicalize()
                .map_err(|_| "profile workspace unavailable")?
                != workspace
        {
            return Err("execution profile workspace differs from bound root".into());
        }
        let prepared = profile.prepare(policy.mode)?;
        let profile = &prepared.profile;
        if let Some(accepted) = self.retained_budget(&state)? {
            if accepted.budget.max_requests != profile.max_requests
                || accepted.budget.deadline_seconds != profile.deadline_seconds
                || accepted.budget.cap_micros.as_str() != self.config.cap.micros.get().to_string()
            {
                return Err("execution profile differs from original public run limits".into());
            }
        }
        let roots = std::iter::once(
            RootId::parse(self.config.workspace.as_str()).map_err(|_| "invalid execution root")?,
        )
        .chain(
            profile
                .processes
                .iter()
                .map(|process| RootId::parse(format!("exec-{}", process.name)))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "invalid process root")?,
        )
        .collect::<std::collections::BTreeSet<_>>();
        if self.config.price != profile.provider.price
            || self.config.input_ceiling != profile.provider.max_input
            || self.config.output_ceiling != profile.output_ceiling()?
            || self.config.max_transport_retries != profile.max_transport_retries
            || policy.automatic_effects != profile.automatic_effects
            || policy.workspace_roots != roots
        {
            return Err("execution profile differs from durable owner policy or model".into());
        }
        // Recheck bytes after validation too; an altered profile never silently
        // replaces the private bootstrap's selected profile.
        if vcp_protocol::digest_bytes(&crate::settings::read_bounded(&path, 256 * 1024)?)
            != self.profile_digest
        {
            return Err("execution profile changed during preparation".into());
        }
        Ok((prepared, pin))
    }

    fn retained_budget(
        &self,
        state: &vcp_store::contract::State,
    ) -> Result<Option<vcp_engine::public_start::RetainedStartBudget>, String> {
        if !state.records.contains_key(&vcp_store::contract::key(
            Collection::Task,
            self.config.root_task.as_str(),
        )) {
            return Ok(None);
        }
        vcp_engine::public_start::retained_start_budget(
            state,
            &Scope {
                workspace: self.config.workspace.clone(),
                session: self.config.session.clone(),
                task: self.config.root_task.clone(),
            },
        )
        .map_err(|_| "original run budget evidence unavailable".into())
    }

    fn execution_expiry(
        &self,
        profile: &crate::settings::Profile,
    ) -> Result<tokio::time::Instant, String> {
        let mut remaining = u64::from(profile.deadline_seconds) * 1000;
        if let Some(accepted) = self.retained_budget(&self.host.snapshot()?)? {
            let expires = accepted
                .accepted_at
                .get()
                .checked_add(u64::from(accepted.budget.deadline_seconds) * 1000)
                .ok_or("original run deadline overflow")?;
            remaining = remaining.min(expires.saturating_sub(crate::settings::now().get()));
        }
        if remaining == 0 {
            return Err("original run deadline elapsed".into());
        }
        Ok(tokio::time::Instant::now() + std::time::Duration::from_millis(remaining))
    }

    async fn resume(
        &self,
        connection: &mut PublicConnection,
        request: methods::SessionResume,
        current: &Access,
    ) -> Result<CommandReceipt, RpcError> {
        let mut state = self.state.lock().await;
        let operation = request.mutation.command_id.clone();
        if state.closed || request.task.as_str() != self.config.root_task.as_str() {
            return Err(failure(Code::PolicyDenied, Some(operation)));
        }
        let ticket = match connection.prepare_resume_rpc(request, current)? {
            PublicResumeAdmission::Replay(receipt) => return Ok(receipt),
            PublicResumeAdmission::Ready(ticket) => ticket,
        };
        self.resume_ready(&mut state, connection, ticket, current)
            .await
            .map_err(|_| failure(Code::PolicyDenied, Some(operation)))
    }

    async fn resume_ready(
        &self,
        state: &mut State,
        connection: &mut PublicConnection,
        mut ticket: PublicResumeTicket,
        current: &Access,
    ) -> Result<CommandReceipt, String> {
        if state
            .deadline
            .is_some_and(|deadline| tokio::time::Instant::now() >= deadline)
        {
            return Err("configured execution deadline elapsed; relaunch required".into());
        }
        // Construction and later retained submission acknowledgements both
        // remain owned and bounded; durable replay never enters this path.
        let _operation_deadline = ProcessDeadline::start(Some(connection.loss_signal()))?;
        // An old event owner must fully relinquish the queue before a new turn.
        if state.pump.as_ref().is_some_and(|pump| !pump.is_finished()) {
            return Err("previous execution owner is still draining".into());
        }
        if let Some(pump) = state.pump.take() {
            let _ = pump.await;
        }
        let (prepared, policy) = self.prepare_profile(state.policy.as_deref())?;
        let profile = if state.session.is_none() {
            let http = crate::mcp::prepare_http_with(&prepared.profile.mcp_http, |name| {
                self.configuration.credentials.get(name).cloned().ok_or(())
            })?;
            let credential = vcp_engine::capture::ProviderCredential::from_config(
                self.configuration.provider_credential.clone(),
            );
            let retained = crate::execution_profile::retained_config(
                &self.data,
                std::path::Path::new(&self.config.binding.root),
                &credential,
                &prepared.profile,
            )
            .await?;
            let profile = crate::execution_profile::install_host(
                &self.host,
                &self.config,
                prepared,
                http,
                |name| self.configuration.credentials.get(name).cloned().ok_or(()),
            )?;
            let startup = connection.authorize_resume_startup(&mut ticket, current)?;
            let binding = ThreadBinding {
                scope: ticket.scope().clone(),
                agent: AgentId::new(),
                role: vcp_domain::accounting::RequestRole::Main,
            };
            // The constructor remains owned through loss/shutdown; its ticket
            // rechecks the original connection before attachment and cleans up
            // failed attachment. Generic startup authorization stays closed.
            state.session = Some(
                crate::session::Session::start_public(
                    &self.host, retained, binding, connection, startup, current,
                )
                .await?,
            );
            profile
        } else {
            prepared.profile
        };
        state.policy = Some(policy);
        let session = state
            .session
            .as_ref()
            .ok_or("execution session unavailable")?;
        let thread = session.id;
        let scope = ticket.scope().clone();
        let execution = crate::execution::RetainedExecution::claim(&self.host, session, &scope)?;
        let receipt = match connection.resume_prepared(ticket, current)? {
            PublicResumeOutcome::Replay(receipt) => return Ok(receipt),
            PublicResumeOutcome::Accepted(receipt) => receipt,
        };
        if !state.configured {
            let expires = match self.execution_expiry(&profile) {
                Ok(expires) => expires,
                Err(_) => {
                    pause(&self.host, &scope);
                    return Ok(receipt);
                }
            };
            if crate::execution_profile::install_thread(&self.host, thread, &profile).is_err() {
                pause(&self.host, &scope);
                return Ok(receipt);
            }
            state.configured = true;
            state.deadline = Some(expires);
        }
        // Only a new durable acceptance can submit work. Canonical admission
        // independently rechecks controller loss between commit and submission.
        let turn = match execution.submit_identified().await {
            Ok(turn) => turn,
            Err(_) => {
                pause(&self.host, &scope);
                return Ok(receipt);
            }
        };
        let expires = state.deadline.ok_or("execution deadline unavailable")?;
        state.pump = Some(pump(self.host.clone(), scope, execution, turn, expires));
        Ok(receipt)
    }

    pub(super) async fn shutdown(&self, owner: CanonicalOwner) -> Result<(), String> {
        let _deadline = match ProcessDeadline::start(None) {
            Ok(deadline) => deadline,
            Err(_) => std::process::exit(1),
        };
        let mut state = self.state.lock().await;
        state.closed = true;
        if let Some(session) = &state.session {
            pause(&self.host, session.scope());
        }
        if let Some(pump) = state.pump.take() {
            let _ = pump.await;
        }
        // Keep the retained thread and manager alive while canonical close
        // interrupts and checkpoints them. Shutting the thread down first makes
        // the owner's final interruption fail and poisons restart evidence.
        let session = state.session.take();
        let closed = owner.close().await;
        let retained = if let Some(session) = session {
            session
                .thread
                .shutdown_and_wait()
                .await
                .map_err(|_| "retained execution shutdown failed".to_owned())
        } else {
            Ok(())
        };
        closed.and(retained)
    }
}

fn event_for_turn(event: &codex_protocol::protocol::Event, turn: &str) -> bool {
    use codex_protocol::protocol::EventMsg;
    event.id == turn
        && match &event.msg {
            EventMsg::TurnComplete(end) => end.turn_id == turn,
            EventMsg::TurnAborted(end) => end.turn_id.as_deref().is_none_or(|id| id == turn),
            _ => true,
        }
}

fn pump(
    host: CanonicalHost,
    scope: Scope,
    mut execution: crate::execution::RetainedExecution,
    turn: String,
    expires: tokio::time::Instant,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let deadline = tokio::time::sleep_until(expires);
        tokio::pin!(deadline);
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(100));
        loop {
            tokio::select! {
                _ = &mut deadline => { pause(&host, &scope); break; }
                _ = tick.tick() => {
                    match selected(&host, &scope) {
                        Ok(task) if task.state == TaskState::Running => {}
                        Ok(_) => break,
                        Err(_) => { pause(&host, &scope); break; }
                    }
                }
                event = execution.next_event() => match event {
                    Ok(event) if event_for_turn(&event, &turn) => match event.msg {
                        codex_protocol::protocol::EventMsg::TurnComplete(_) => {
                            if matches!(execution.complete(), Err(_) | Ok(crate::execution::Completion::Rejected(_))) { pause(&host, &scope); }
                            break;
                        }
                        codex_protocol::protocol::EventMsg::TurnAborted(_) | codex_protocol::protocol::EventMsg::Error(_) => { pause(&host, &scope); break; }
                        _ => {}
                    },
                    Ok(_) => {},
                    Err(_) => { pause(&host, &scope); break; }
                }
            }
        }
    })
}

fn selected(host: &CanonicalHost, scope: &Scope) -> Result<Task, String> {
    let task: Task = host
        .snapshot()?
        .record(Collection::Task, scope.task.as_str(), &scope.workspace)
        .and_then(|row| row.decode())
        .map_err(|_| "execution task unavailable")?;
    if task.scope != *scope {
        return Err("execution task scope changed".into());
    }
    Ok(task)
}
fn pause(host: &CanonicalHost, scope: &Scope) {
    // Fence runtime admission before reading a revision that may race canonical
    // progress. A failed pause commit retains the hold (or the worker fence).
    // Dropping its waiter does not cancel the owned interruption.
    drop(host.hold_execution());
    if let Ok(task) = selected(host, scope) {
        if !task.state.terminal() {
            if let Ok(command) = host.control_envelope(
                CommandId::new(),
                scope.task.clone(),
                task.revision,
                Command::Transition {
                    next: TaskState::Paused,
                    reason: "owned local execution stopped; explicit resume required".into(),
                    verification: None,
                },
            ) {
                let _ = host.stop(command);
            }
        }
    }
}
fn failure(code: Code, operation: Option<methods::Id>) -> RpcError {
    let retry = if code == Code::OutcomeUnknown {
        Retry::ReconcileOriginal
    } else {
        Retry::AfterRevalidation
    };
    ApplicationError {
        code,
        retry,
        operation,
        explanation: "local execution admission unavailable; inspect original command and task"
            .into(),
        reconciliation: None,
    }
    .into_rpc()
}

pub(super) struct Rpc {
    connection: Arc<Mutex<Option<PublicConnection>>>,
    supervisor: Option<Arc<Supervisor>>,
    methods: Vec<&'static str>,
}
impl Rpc {
    pub(super) fn new(connection: PublicConnection, supervisor: Option<Arc<Supervisor>>) -> Self {
        let methods = Call::METHODS
            .iter()
            .copied()
            .filter(|name| {
                connection.supported_methods().contains(name)
                    || (matches!(*name, "session/resume" | "turn/start") && supervisor.is_some())
            })
            .collect();
        Self {
            connection: Arc::new(Mutex::new(Some(connection))),
            supervisor,
            methods,
        }
    }
    pub(super) async fn disconnect(self) -> Result<(), String> {
        let connection = self
            .connection
            .lock()
            .await
            .take()
            .ok_or("connection already closed")?;
        connection.disconnect()?.wait().await.map(|_| ())
    }
}
impl RpcHost for Rpc {
    fn supported_methods(&self) -> &[&str] {
        &self.methods
    }
    fn authorize(&self, current: &Access) -> Result<(), RpcError> {
        self.connection
            .try_lock()
            .map_err(|_| failure(Code::ResourceLimit, None))?
            .as_ref()
            .ok_or_else(|| failure(Code::PolicyDenied, None))?
            .authorize(current)
    }
    async fn call(&mut self, call: Call, current: &Access) -> Result<ResultValue, RpcError> {
        let execution = match &call {
            Call::SessionResume(request) => Some((&request.scope, &request.mutation.command_id)),
            Call::TurnStart(request) => Some((&request.scope, &request.mutation.command_id)),
            _ => None,
        };
        if let (Some((scope, command)), Some(supervisor)) = (execution, &self.supervisor) {
            let request = call.clone();
            let current = current.clone();
            let command = command.clone();
            let scope = scope.clone();
            let connection = self.connection.clone();
            let supervisor = supervisor.clone();
            // The task owns its original connection guard through durable commit
            // and submission. Dropping this await only drops a response waiter.
            let operation = command.clone();
            tokio::spawn(async move {
                let mut locked = connection.lock().await;
                let connection = locked
                    .as_mut()
                    .ok_or_else(|| failure(Code::PolicyDenied, Some(command.clone())))?;
                match request {
                    Call::SessionResume(request) => {
                        supervisor.resume(connection, request, &current).await?;
                    }
                    Call::TurnStart(request) => {
                        supervisor.start(connection, request, &current).await?;
                    }
                    _ => return Err(RpcError::internal_error()),
                }
                connection
                    .call(
                        Call::CommandRead(methods::CommandRead {
                            scope,
                            command_id: command,
                        }),
                        &current,
                    )
                    .await
            })
            .await
            .map_err(|_| failure(Code::OutcomeUnknown, Some(operation)))?
        } else {
            self.connection
                .lock()
                .await
                .as_mut()
                .ok_or_else(|| failure(Code::PolicyDenied, call.command_id().cloned()))?
                .call(call, current)
                .await
        }
    }
}
