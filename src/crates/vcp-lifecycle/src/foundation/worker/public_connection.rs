// SPDX-License-Identifier: Apache-2.0
//! Authenticated connection ownership over the existing host and writer.
use super::*;
use crate::foundation::CanonicalHost;
use vcp_domain::controller::Reason;
use vcp_engine::{
    controller::ControllerToken,
    query::{Query, QueryResult},
};

pub(super) struct CurrentController {
    access: Access,
    connection: ControllerId,
    token: ControllerToken,
    connected: Arc<AtomicBool>,
}

/// Not clonable: dropping the controlling connection owns its pause and release.
/// Access is supplied by trusted local authentication, never request parameters.
pub struct PublicConnection {
    host: CanonicalHost,
    access: Access,
    connection: ControllerId,
    token: Option<ControllerToken>,
    reactor: tokio::runtime::Handle,
    closed: bool,
    connected: Arc<AtomicBool>,
}

pub struct PublicDisconnect(
    tokio::sync::oneshot::Receiver<std::result::Result<Option<CommandReceipt>, String>>,
);
impl PublicDisconnect {
    /// Dropping the waiter does not cancel owned pause/draining/release.
    pub async fn wait(self) -> std::result::Result<Option<CommandReceipt>, String> {
        self.0
            .await
            .map_err(|_| "public owner release outcome unknown")?
    }
}

impl CanonicalHost {
    pub(in crate::foundation) fn public_startup_admission(
        &self,
    ) -> std::result::Result<(), String> {
        self.worker.run(|context| context.check_public_owner())
    }

    /// Select public ownership before dispatch. Attaching cannot silently take
    /// over a running private CLI owner. Once selected, an absent lease blocks
    /// new work even though the server's CanonicalOwner remains alive.
    pub fn public_connection(
        &self,
        current: Access,
    ) -> std::result::Result<PublicConnection, String> {
        let reactor = tokio::runtime::Handle::try_current()
            .map_err(|_| "public connection requires a live runtime")?;
        let checked = current.clone();
        let runtime = self.runtime.clone();
        let scheduler = self.scheduler.clone();
        self.worker.run(move |context| {
            if checked.workspace != context.config.workspace
                || checked.session != context.config.session
            {
                return Err("public connection differs from host scope".into());
            }
            context.engine.query(
                &checked,
                &Query::Session {
                    session: checked.session.clone(),
                },
            )?;
            if !context.public_mode {
                let state = runtime.0.state.lock().map_err(|_| "lifecycle poisoned")?;
                if !state.attached
                    || state.sealing
                    || state.startups_in_flight != 0
                    || state.entries.values().any(|entry| entry.starts != 0)
                    || state.work.iter().any(|work| work.receipt.is_none())
                    || scheduler.busy()
                {
                    return Err("public ownership requires a quiescent private host".into());
                }
                for row in context
                    .engine
                    .store()
                    .state()
                    .records
                    .values()
                    .filter(|row| row.collection == Collection::Task)
                {
                    let task: Task = row.decode()?;
                    if task.scope.session == checked.session && task.state == TaskState::Running {
                        return Err(
                            "public ownership cannot replace a running private owner".into()
                        );
                    }
                }
                context.public_mode = true;
            }
            Ok(())
        })?;
        Ok(PublicConnection {
            host: self.clone(),
            access: current,
            connection: ControllerId::new(),
            token: None,
            reactor,
            closed: false,
            connected: Arc::new(AtomicBool::new(true)),
        })
    }
}

impl PublicConnection {
    pub fn acquire(
        &mut self,
        command: CommandId,
        expected: Option<Revision>,
    ) -> std::result::Result<CommandReceipt, String> {
        if self.closed || !self.connected.load(Ordering::SeqCst) {
            return Err("public connection closed".into());
        }
        let access = self.access.clone();
        let connection = self.connection.clone();
        let connected = self.connected.clone();
        let identity = self.host.public_identity.clone();
        let (receipt, token) = self.host.worker.run(move |context| {
            if !connected.load(Ordering::SeqCst) {
                return Err("public connection closed".into());
            }
            if !context.owner_alive || context.authority_pending {
                return Err("public owner is unavailable".into());
            }
            let receipt = context.runtime.block_on(context.engine.acquire_controller(
                &access,
                &connection,
                command,
                expected,
                now(),
            ))?;
            let token = match context.engine.controller_token(&access, &connection) {
                Ok(token) => Some(token),
                Err(vcp_engine::controller::ControllerError::Stale) => None,
                Err(error) => return Err(error.into()),
            };
            if let Some(token) = &token {
                *identity.lock().map_err(|_| "public identity poisoned")? =
                    Some((connection.clone(), token.generation()));
                context.public_controller = Some(CurrentController {
                    access,
                    connection,
                    token: token.clone(),
                    connected,
                });
            }
            Ok((receipt, token))
        })?;
        self.token = token;
        Ok(receipt)
    }

    pub fn controller_token(&self) -> std::result::Result<ControllerToken, String> {
        let (_, access, connection, token) = self.rpc_context(&self.access)?;
        let token = token.ok_or("public connection is an observer")?;
        let checked = token.clone();
        self.host.worker.run(move |context| {
            context.public_authorize(&access, &connection, Some(&checked), true)
        })?;
        Ok(token)
    }

    pub fn query(&self, query: Query) -> std::result::Result<QueryResult, String> {
        let (_, access, connection, token) = self.rpc_context(&self.access)?;
        self.host.worker.run_cleanup(move |context| {
            context.public_authorize(&access, &connection, token.as_ref(), false)?;
            Ok(context.engine.query(&access, &query)?)
        })
    }

    /// Internal RPC bridge only. A refreshed read authority cannot refresh an
    /// old controller token or expand this connection's original role ceiling.
    pub(super) fn rpc_context(
        &self,
        current: &Access,
    ) -> std::result::Result<(CanonicalHost, Access, ControllerId, Option<ControllerToken>), String>
    {
        if self.closed
            || !self.connected.load(Ordering::SeqCst)
            || current.actor != self.access.actor
            || current.workspace != self.access.workspace
            || current.session != self.access.session
            || current.read && !self.access.read
            || current.write && !self.access.write
            || current.bootstrap
        {
            return Err("public connection scope or role changed".into());
        }
        let access = current.clone();
        let connection = self.connection.clone();
        let token = self.token.clone();
        self.host.worker.run_cleanup(move |context| {
            context.public_authorize(&access, &connection, token.as_ref(), false)
        })?;
        Ok((
            self.host.clone(),
            current.clone(),
            self.connection.clone(),
            self.token.clone(),
        ))
    }

    /// Own interruption, startup draining and canonical release. If the first
    /// retained root is still being constructed, reopening the host is required
    /// before another construction: acquisition alone never rearms attachment.
    pub fn disconnect(mut self) -> std::result::Result<PublicDisconnect, String> {
        self.closed = true;
        let result = self.begin_disconnect();
        if result.is_err() {
            self.host.worker.fence();
        }
        result
    }

    fn begin_disconnect(&self) -> std::result::Result<PublicDisconnect, String> {
        self.connected.store(false, Ordering::SeqCst);
        let (reply, receiver) = tokio::sync::oneshot::channel();
        let Some(original) = self.token.clone() else {
            let _ = reply.send(Ok(None));
            return Ok(PublicDisconnect(receiver));
        };
        let access = self.access.clone();
        let connection = self.connection.clone();
        let runtime = self.host.runtime.clone();
        // Never wait for the store while holding identity. Interruption is
        // admitted before the bounded writer queue, including queue failure.
        let waiter = {
            let identity = self
                .host
                .public_identity
                .lock()
                .map_err(|_| "public identity poisoned")?;
            if identity.as_ref() != Some(&(connection.clone(), original.generation())) {
                let _ = reply.send(Ok(None));
                return Ok(PublicDisconnect(receiver));
            }
            Some(
                runtime
                    .hold_owner()
                    .map_err(|error| format!("public owner stop: {error:?}"))?,
            )
        };
        let selected = connection.clone();
        let preparation = self.host.worker.run_cleanup(move |context| {
            // Fresh authority is exclusively for cleanup. Compare ownership
            // before touching lifecycle state, so late old disconnects are inert.
            let fresh = context.cleanup_access(&access)?;
            let lease = context.engine.controller_lease(&fresh)?;
            let Some(lease) = lease else {
                return Ok(None);
            };
            let Some(holder) = lease.holder else {
                return Ok(None);
            };
            if holder.connection != selected
                || holder.actor != access.actor
                || holder.process_owner != *context.engine.controller()
                || holder.owner_epoch != context.engine.owner_epoch()
                || lease.generation != original.generation()
            {
                return Ok(None);
            }
            let token = context.engine.controller_token(&fresh, &selected)?;
            context.public_controller = None;
            context.pause_public_session(&fresh)?;
            Ok(Some(token))
        });
        let prepared = match preparation {
            Ok(prepared) => prepared,
            Err(error) => {
                self.host.worker.fence();
                return Err(error);
            }
        };
        let Some(token) = prepared else {
            let _ = reply.send(Ok(None));
            return Ok(PublicDisconnect(receiver));
        };
        let worker = self.host.worker.clone();
        let original_access = self.access.clone();
        let scheduler = self.host.scheduler.clone();
        let deadline = self.host.runtime.0.deadline;
        let identity = self.host.public_identity.clone();
        let guard = CleanupGuard {
            worker: worker.clone(),
            complete: false,
        };
        drop(self.reactor.spawn(async move {
            let mut guard = guard;
            let result = async {
                if let Some(waiter) = waiter {
                    waiter
                        .wait()
                        .await
                        .map_err(|error| format!("public owner drain: {error:?}"))?;
                }
                tokio::time::timeout(deadline, async {
                    while scheduler.busy() {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                })
                .await
                .map_err(|_| "public scheduler drain timed out")?;
                worker.run_cleanup(move |context| {
                    let fresh = context.cleanup_access(&original_access)?;
                    let current = context.engine.controller_token(&fresh, &connection)?;
                    if current.generation() != token.generation()
                        || current.revision() != token.revision()
                    {
                        return Err("public owner changed during cleanup".into());
                    }
                    let receipt = context.runtime.block_on(context.engine.release_controller(
                        &fresh,
                        &connection,
                        CommandId::new(),
                        &current,
                        current.revision(),
                        Reason::ConnectionLost,
                        now(),
                    ))?;
                    let mut identity = identity.lock().map_err(|_| "public identity poisoned")?;
                    if identity.as_ref() == Some(&(connection.clone(), current.generation())) {
                        *identity = None;
                    }
                    Ok(Some(receipt))
                })
            }
            .await;
            guard.complete = result.is_ok();
            let _ = reply.send(result);
        }));
        Ok(PublicDisconnect(receiver))
    }
}

impl Drop for PublicConnection {
    fn drop(&mut self) {
        if !self.closed {
            self.closed = true;
            if self.begin_disconnect().is_err() {
                self.host.worker.fence();
            }
        }
    }
}

struct CleanupGuard {
    worker: Worker,
    complete: bool,
}
impl Drop for CleanupGuard {
    fn drop(&mut self) {
        if !self.complete {
            self.worker.fence();
        }
    }
}

impl Context {
    /// Rechecked after owned asynchronous drains as well as before admission.
    /// Pending authority work may call this; connection loss may not.
    pub(super) fn check_public_controller(
        &self,
        access: &Access,
        connection: &ControllerId,
        token: &ControllerToken,
    ) -> Result<()> {
        if !self.public_mode || !self.owner_alive {
            return Err("public host unavailable".into());
        }
        let owner = self
            .public_controller
            .as_ref()
            .ok_or("public controller absent")?;
        if !owner.connected.load(Ordering::SeqCst)
            || owner.connection != *connection
            || owner.token.generation() != token.generation()
            || owner.token.revision() != token.revision()
        {
            return Err("public controlling connection lost or replaced".into());
        }
        self.engine.check_controller(access, connection, token)?;
        Ok(())
    }

    pub(super) fn check_public_owner(&self) -> Result<()> {
        if !self.public_mode {
            return Ok(());
        }
        let owner = self
            .public_controller
            .as_ref()
            .ok_or("public controller absent; explicit acquisition required")?;
        self.check_public_controller(&owner.access, &owner.connection, &owner.token)?;
        Ok(())
    }

    pub(super) fn public_authorize(
        &self,
        access: &Access,
        connection: &ControllerId,
        token: Option<&ControllerToken>,
        write: bool,
    ) -> Result<()> {
        if !self.public_mode
            || !self.owner_alive
            || access.workspace != self.config.workspace
            || access.session != self.config.session
        {
            return Err("public host unavailable or scope changed".into());
        }
        self.engine.query(
            access,
            &Query::Session {
                session: access.session.clone(),
            },
        )?;
        if write {
            let token = token.ok_or("public controller token required")?;
            self.check_public_controller(access, connection, token)?;
            if self.public_controller.as_ref().is_none_or(|owner| {
                owner.connection != *connection || owner.token.generation() != token.generation()
            }) {
                return Err("public connection is not the active host owner".into());
            }
        }
        Ok(())
    }

    fn cleanup_access(&self, previous: &Access) -> Result<Access> {
        if previous.workspace != self.config.workspace
            || previous.session != self.config.session
            || !previous.write
        {
            return Err("public cleanup scope denied".into());
        }
        let workspace: Workspace = self
            .engine
            .store()
            .state()
            .record(
                Collection::Workspace,
                previous.workspace.as_str(),
                &previous.workspace,
            )?
            .decode()?;
        Ok(Access {
            actor: previous.actor.clone(),
            workspace: previous.workspace.clone(),
            session: previous.session.clone(),
            authority: workspace.authority,
            read: true,
            write: true,
            bootstrap: false,
        })
    }

    fn pause_public_session(&mut self, access: &Access) -> Result<()> {
        // Refresh only trusted host cleanup authority, never a client's stored
        // credential/token. The absent public_controller still fences dispatch.
        self.access.authority = access.authority;
        let tasks: Vec<Task> = self
            .engine
            .store()
            .state()
            .records
            .values()
            .filter(|row| row.collection == Collection::Task && row.workspace == access.workspace)
            .map(Record::decode)
            .collect::<std::result::Result<_, _>>()?;
        for task in tasks {
            if task.scope.session == access.session
                && !task.state.terminal()
                && task.state != TaskState::Paused
            {
                self.recovery_pause(
                    task,
                    "public controlling connection lost; explicit resume required",
                )?;
            }
        }
        #[cfg(windows)]
        self.stop_coding_turns("public controlling connection lost")?;
        Ok(())
    }
}
