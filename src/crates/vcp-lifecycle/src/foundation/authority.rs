// SPDX-License-Identifier: Apache-2.0
//! Authority changes drain the existing owner; they never create another scheduler.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct AuthorityChange(tokio::sync::oneshot::Receiver<Result<CommandReceipt, String>>);
impl AuthorityChange {
    /// Dropping this waiter does not cancel the owned stop/commit operation.
    pub async fn wait(self) -> Result<CommandReceipt, String> {
        self.0
            .await
            .map_err(|_| "authority change outcome unknown")?
    }
}
pub(super) fn changes_authority(command: &Command) -> bool {
    matches!(
        command,
        Command::SetPolicy { .. }
            | Command::SetWorkspaceTrust { .. }
            | Command::SetGrant { .. }
            | Command::Rebind { .. }
            | Command::Steer { .. }
    )
}
impl CanonicalHost {
    pub(super) fn command_checked(
        &self,
        command: Command,
        task: Option<TaskId>,
        expected: Revision,
    ) -> Result<CommandReceipt, String> {
        let runtime = self.runtime.clone();
        let conflict = self.tool_conflict.clone();
        self.worker.run(move |context| {
            context.check_external_command(&command)?;
            if changes_authority(&command) {
                // Keep worker -> lifecycle lock order. Admission cannot acquire a
                // permit between this quiescence check and the durable mutation.
                let state = runtime.0.state.lock().map_err(|_| "lifecycle poisoned")?;
                if !state.attached
                    || state.sealing
                    || state.startups_in_flight != 0
                    || state.entries.values().any(|entry| entry.starts != 0)
                    || state.work.iter().any(|work| work.receipt.is_none())
                    || conflict.load(Ordering::SeqCst)
                {
                    return Err("active work requires coordinated change_authority".into());
                }
                context.command(command, task, expected)
            } else {
                context.command(command, task, expected)
            }
        })
    }

    /// Fence and pause the whole owning tree, drain native/retained work, then
    /// commit the revision-checked command. Tasks stay paused for deliberate resume.
    pub fn change_authority(
        &self,
        command: Command,
        task: Option<TaskId>,
        expected: Revision,
    ) -> Result<AuthorityChange, String> {
        let reactor =
            tokio::runtime::Handle::try_current().map_err(|_| "live owner runtime required")?;
        let runtime = self.runtime.clone();
        let requested = command.clone();
        let selected = task.clone();
        let started = Arc::new(AtomicBool::new(false));
        let marker = started.clone();
        let preparation = self.worker.run(move |context| {
            context.validate_authority_change(&requested, &selected, expected)?;
            context.mark_authority_pending();
            marker.store(true, Ordering::SeqCst);
            let waiter = runtime
                .hold_owner()
                .map_err(|error| format!("authority stop: {error:?}"))?;
            let adjusted = context.pause_for_authority(&requested, &selected, expected)?;
            Ok((waiter, adjusted))
        });
        let (waiter, expected) = match preparation {
            Ok(value) => value,
            Err(error) => {
                if started.load(Ordering::SeqCst) {
                    self.worker.fence();
                }
                return Err(error);
            }
        };
        let worker = self.worker.clone();
        let (reply, receiver) = tokio::sync::oneshot::channel();
        drop(reactor.spawn(async move {
            let result = match waiter.wait().await {
                Ok(()) => worker
                    .run(move |context| context.finish_authority_change(command, task, expected)),
                Err(error) => {
                    if worker
                        .run_cleanup(|context| {
                            context.clear_authority_pending();
                            Ok(())
                        })
                        .is_err()
                    {
                        worker.fence();
                    }
                    Err(format!("authority change not applied: {error:?}"))
                }
            };
            let _ = reply.send(result);
        }));
        Ok(AuthorityChange(receiver))
    }
}
