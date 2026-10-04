// SPDX-License-Identifier: Apache-2.0
//! Test reader supplies bounded history independently of the current projection.
use super::*;
use std::{cell::Cell, collections::BTreeMap, sync::Arc};
use vcp_protocol::event::EventEnvelope;
use vcp_store::{contract::Receipt, CurrentState, CurrentStateView};

pub(super) struct Paged {
    current: CurrentState,
    events: Vec<EventEnvelope>,
    commands: BTreeMap<String, CommandReceipt>,
    pub(super) pages: Cell<usize>,
    pub(super) fail: bool,
}
impl Paged {
    pub(super) fn new(state: &State, current: &Arc<CurrentState>) -> Self {
        let mut current = current.as_ref().clone();
        current.records = state.records.clone();
        current.sequences = state.sequences.clone();
        current.watermark = state.watermark;
        Self {
            current,
            events: state.events.to_vec(),
            commands: state
                .commands
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            pages: Cell::new(0),
            fail: false,
        }
    }
}
impl CanonicalStore for Paged {
    fn state(&self) -> &State {
        panic!("proof attempted whole-State access")
    }
    fn current(&self) -> CurrentStateView<'_> {
        (&self.current).into()
    }
    async fn history_event_count(&self) -> vcp_store::Result<u64> {
        Ok(self.events.len() as u64)
    }
    async fn history_events(
        &self,
        after: Option<u64>,
        limit: usize,
    ) -> vcp_store::Result<Vec<EventEnvelope>> {
        self.pages.set(self.pages.get() + 1);
        if self.fail {
            return Err(vcp_store::Error::Corruption("injected history failure"));
        }
        let first = after.map_or(0, |value| value + 1) as usize;
        Ok(self
            .events
            .iter()
            .skip(first)
            .take(limit.min(2))
            .cloned()
            .collect())
    }
    async fn command_receipt(
        &self,
        workspace: &WorkspaceId,
        command: &CommandId,
        digest: &str,
    ) -> vcp_store::Result<Option<CommandReceipt>> {
        match self
            .commands
            .get(&vcp_store::contract::command_key(workspace, command))
        {
            Some(receipt) if receipt.digest == digest => Ok(Some(receipt.clone())),
            Some(_) => Err(vcp_store::Error::Conflict("command meaning differs")),
            None => Ok(None),
        }
    }
    async fn transact(&mut self, _: Transaction) -> vcp_store::Result<Receipt> {
        panic!("proof attempted a mutation")
    }
}
