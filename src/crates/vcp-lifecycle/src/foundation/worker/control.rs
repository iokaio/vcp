// SPDX-License-Identifier: Apache-2.0
//! Presentation controls enter the existing owner and canonical command handler.
use super::super::*;
use vcp_domain::task::{Task, TaskState};
use vcp_protocol::command::CommandEnvelope;
use vcp_store::contract::Collection;

impl CanonicalHost {
    /// Pull durable events through the engine's authenticated, bounded cursor.
    /// Rendering never keeps an unbounded producer queue or accesses the store.
    pub fn subscribe_events(
        &self,
        after: SessionSeq,
        limit: u32,
    ) -> Result<vcp_protocol::subscription::Cursor, String> {
        self.worker.run_cleanup(move |context| {
            Ok(context
                .engine
                .subscribe(&context.access, after, limit, worker::now())?)
        })
    }

    pub fn events(
        &self,
        cursor: vcp_protocol::subscription::Cursor,
    ) -> Result<vcp_protocol::subscription::EventPage, String> {
        self.worker.run_cleanup(move |context| {
            Ok(context
                .engine
                .events(&context.access, &cursor, worker::now())?)
        })
    }

    pub fn unsubscribe_events(&self, snapshot: SnapshotId) -> Result<(), String> {
        self.worker.run_cleanup(move |context| {
            context.engine.unsubscribe(&snapshot);
            Ok(())
        })
    }

    /// Construct an envelope on the authenticated owner's input connection.
    /// Persist/reuse this envelope when retrying a command, including its ID.
    pub fn control_envelope(
        &self,
        id: CommandId,
        task: TaskId,
        expected: Revision,
        payload: Command,
    ) -> Result<CommandEnvelope, String> {
        self.worker.run_cleanup(move |context| {
            let current: Task = context
                .engine
                .store()
                .state()
                .record(Collection::Task, task.as_str(), &context.config.workspace)?
                .decode()?;
            Ok(CommandEnvelope {
                version: 1,
                id,
                workspace: current.scope.workspace,
                session: current.scope.session,
                task: Some(task),
                caller: context.config.actor.clone(),
                controller: context.engine.controller().clone(),
                owner_epoch: context.engine.owner_epoch(),
                expected,
                steering: current.steering,
                payload,
            })
        })
    }

    /// Pause/cancel fences the retained task and descendants before committing.
    /// The returned receipt acknowledges the durable state; quiescence remains
    /// separately observable in lifecycle/effect state while cancellation drains.
    /// Duplicate commands return their original receipt without another hold.
    pub fn stop(&self, command: CommandEnvelope) -> Result<CommandReceipt, String> {
        let runtime = self.runtime.clone();
        let bindings = self.bindings.clone();
        self.worker.run_cleanup(move |context| {
            command.validate_version()?;
            if vcp_protocol::canonical_bytes(&command)?.len()
                > vcp_protocol::version::MAX_COMMAND_BYTES
            {
                return Err("control command exceeds input limit".into());
            }
            context.engine.authorize(&context.access)?;
            if command.workspace != context.access.workspace
                || command.session != context.access.session
                || command.caller != context.access.actor
                || !context.access.write
            {
                return Err("control scope or caller denied".into());
            }
            let (next, reason) = match &command.payload {
                Command::Transition {
                    next,
                    reason,
                    verification: None,
                } if matches!(next, TaskState::Paused | TaskState::Cancelled) => (*next, reason),
                _ => return Err("stop accepts only pause or cancellation".into()),
            };
            if let Some(receipt) = context.engine.store().state().command(
                &command.workspace,
                &command.id,
                &command.digest()?,
            )? {
                return Ok(receipt);
            }
            if !context.owner_alive
                || command.controller != *context.engine.controller()
                || command.owner_epoch != context.engine.owner_epoch()
            {
                return Err("stale or closed control owner".into());
            }
            let task = command.task.as_ref().ok_or("control requires task ID")?;
            let current: Task = context
                .engine
                .store()
                .state()
                .record(Collection::Task, task.as_str(), &command.workspace)?
                .decode()?;
            let scope = Scope {
                workspace: command.workspace.clone(),
                session: command.session.clone(),
                task: task.clone(),
            };
            // Validate before touching the runtime, so a stale row cannot stop a
            // different revision of the task. The worker serializes this check.
            if current.state == TaskState::Paused && next == TaskState::Paused {
                if current.scope != scope
                    || current.revision != command.expected
                    || current.steering != command.steering
                    || reason.trim().is_empty()
                    || reason.len() > 4096
                {
                    return Err("stale or invalid repeated pause".into());
                }
            } else {
                current.transition(
                    &scope,
                    command.expected,
                    command.steering,
                    next,
                    EventId::new(),
                    reason.clone(),
                    None,
                    None,
                )?;
            }
            let threads: Vec<_> = bindings
                .lock()
                .map_err(|_| "binding lock poisoned")?
                .iter()
                .filter(|(_, binding)| binding.scope == scope)
                .map(|(id, _)| *id)
                .collect();
            if threads.is_empty() && current.state == TaskState::Running {
                return Err("running task has no reachable retained owner".into());
            }
            for thread in threads {
                let view = runtime
                    .inspect(thread)
                    .map_err(|e| format!("control owner: {e:?}"))?;
                if !view.local_hold {
                    // HoldWaiter cancellation does not cancel owned draining.
                    drop(
                        runtime
                            .hold(thread, &view.revision)
                            .map_err(|e| format!("control stop: {e:?}"))?,
                    );
                }
            }
            let receipt = context.runtime.block_on(context.engine.handle(
                command,
                &context.access,
                &vcp_engine::HostFacts::inspect(worker::now()),
            ))?;
            // Pending children have no retained thread to hold, but may have an
            // in-flight native workspace preparation owned by their parent.
            runtime.0.changed.notify_waiters();
            #[cfg(windows)]
            context.stop_coding_turns("explicit owner stop")?;
            Ok(receipt)
        })
    }
}
