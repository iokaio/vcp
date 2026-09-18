// SPDX-License-Identifier: Apache-2.0
//! Private CLI control delivery to the existing owner. No second scheduler or
//! journal writer. A command whose receipt is missing after a crash is inspectable
//! but never automatically replayed.
use super::{Error, Lifecycle, Revision, View};
use codex_protocol::ThreadId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Pause,
    Resume,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommandRecord {
    pub id: String,
    pub thread: ThreadId,
    pub action: Action,
    pub result: Option<Result<(), Error>>,
}

impl Lifecycle {
    /// The owning CLI supplies a stable command id and its last observed revision.
    /// Resume requires the host's current workspace/authority validation. Cached
    /// acknowledgements do not repeat transitions, even across owner recovery.
    pub async fn control(
        &self,
        command_id: &str,
        thread: ThreadId,
        expected: &Revision,
        action: Action,
        revalidate: impl FnOnce() -> Result<(), Error>,
    ) -> Result<View, Error> {
        if command_id.is_empty() || command_id.len() > 256 {
            return Err(Error::InvalidCommand);
        }
        let revision = {
            let mut state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
            if let Some(previous) = state
                .commands
                .iter()
                .find(|command| command.id == command_id)
            {
                if previous.thread != thread || previous.action != action {
                    return Err(Error::CommandConflict);
                }
                previous.result.ok_or(Error::PendingCommand)??;
                drop(state);
                return self.inspect(thread);
            }
            state.check(expected)?;
            if !state.entries.contains_key(&thread) {
                return Err(Error::UnknownThread);
            }
            state.advance()?;
            state.commands.push(CommandRecord {
                id: command_id.into(),
                thread,
                action,
                result: None,
            });
            state.checkpoint()?;
            state.revision()
        };
        let result = match action {
            Action::Pause => match self.hold(thread, &revision) {
                Ok(waiter) => waiter.wait().await,
                Err(error) => Err(error),
            },
            Action::Resume => revalidate().and_then(|_| self.resume(thread, &revision)),
        };
        {
            let mut state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
            state.advance()?;
            state
                .commands
                .iter_mut()
                .find(|command| command.id == command_id)
                .unwrap()
                .result = Some(result);
            state.checkpoint()?;
        }
        result?;
        self.inspect(thread)
    }
}
