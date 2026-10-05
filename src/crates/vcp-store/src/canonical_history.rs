// SPDX-License-Identifier: Apache-2.0
//! Read-only canonical projections at one admitted cut. Implementations retain
//! the native owner or snapshot pin; this interface grants no mutation authority.
use crate::{contract::CanonicalStore, CurrentStateView, Error, Result, Snapshot};
use vcp_domain::{CommandId, EventId, WorkspaceId};
use vcp_protocol::{command::CommandReceipt, event::EventEnvelope};

#[allow(async_fn_in_trait)]
pub trait CanonicalHistory {
    fn current(&self) -> CurrentStateView<'_>;
    async fn history_event_count(&self) -> Result<u64>;
    /// Global ordinals, exclusive `after`, and the canonical row/byte bounds.
    /// A short nonempty page is a byte boundary, not evidence of exhaustion.
    async fn history_events(&self, after: Option<u64>, limit: usize) -> Result<Vec<EventEnvelope>>;
    async fn history_event_at(&self, ordinal: u64) -> Result<Option<EventEnvelope>>;
    async fn history_event(&self, id: &EventId) -> Result<Option<EventEnvelope>>;
    async fn command_receipt(
        &self,
        workspace: &WorkspaceId,
        command: &CommandId,
        digest: &str,
    ) -> Result<Option<CommandReceipt>>;
}

impl<T: CanonicalStore + ?Sized> CanonicalHistory for T {
    fn current(&self) -> CurrentStateView<'_> {
        CanonicalStore::current(self)
    }
    async fn history_event_count(&self) -> Result<u64> {
        CanonicalStore::history_event_count(self).await
    }
    async fn history_events(&self, after: Option<u64>, limit: usize) -> Result<Vec<EventEnvelope>> {
        CanonicalStore::history_events(self, after, limit).await
    }
    async fn history_event_at(&self, ordinal: u64) -> Result<Option<EventEnvelope>> {
        CanonicalStore::history_event_at(self, ordinal).await
    }
    async fn history_event(&self, id: &EventId) -> Result<Option<EventEnvelope>> {
        CanonicalStore::history_event(self, id).await
    }
    async fn command_receipt(
        &self,
        workspace: &WorkspaceId,
        command: &CommandId,
        digest: &str,
    ) -> Result<Option<CommandReceipt>> {
        CanonicalStore::command_receipt(self, workspace, command, digest).await
    }
}

impl CanonicalHistory for Snapshot {
    fn current(&self) -> CurrentStateView<'_> {
        Snapshot::current(self)
    }
    async fn history_event_count(&self) -> Result<u64> {
        Snapshot::history_event_count(self).await
    }
    async fn history_events(&self, after: Option<u64>, limit: usize) -> Result<Vec<EventEnvelope>> {
        Snapshot::history_events(self, after, limit).await
    }
    async fn history_event_at(&self, ordinal: u64) -> Result<Option<EventEnvelope>> {
        Snapshot::history_event_at(self, ordinal).await
    }
    async fn history_event(&self, id: &EventId) -> Result<Option<EventEnvelope>> {
        Snapshot::history_event(self, id).await
    }
    async fn command_receipt(
        &self,
        workspace: &WorkspaceId,
        command: &CommandId,
        digest: &str,
    ) -> Result<Option<CommandReceipt>> {
        let receipt = self.command_receipt_by_id(workspace, command).await?;
        if receipt
            .as_ref()
            .is_some_and(|receipt| receipt.digest != digest)
        {
            return Err(Error::Conflict("command ID reused with different meaning"));
        }
        Ok(receipt)
    }
}
