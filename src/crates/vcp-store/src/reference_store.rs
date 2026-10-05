// SPDX-License-Identifier: Apache-2.0
//! Explicit resident archival/reference adapter for tests and offline oracles.
//! The live Store never implements this trait. Overrides (including panic-on-
//! State bounded-reader fixtures) are forwarded verbatim by the blanket impl.
use super::*;

#[allow(async_fn_in_trait)]
pub trait ReferenceStore {
    fn state(&self) -> &State;
    /// Borrow only the current projection; callers cannot accidentally scan history.
    fn current(&self) -> crate::CurrentStateView<'_> {
        crate::CurrentStateView::from(self.state())
    }
    /// Owner-known global event count. Readers keep the same serialized owner
    /// and current watermark throughout a request; this is not a scoped count.
    async fn history_event_count(&self) -> Result<u64> {
        u64::try_from(self.state().events.len()).map_err(|_| Error::Limit("history ordinal"))
    }
    /// Unfiltered, contiguous global ordinals after the exclusive zero-based
    /// boundary (None starts at zero). Pages may split a transaction and are
    /// bounded by 4096 rows and MAX_COMMIT_BYTES of canonical row bytes.
    /// A short nonempty page can be byte-bound; only an empty page means end.
    /// These validated resident-State defaults are migration adapters; durable
    /// implementations must authenticate every row under the owner's root.
    async fn history_events(&self, after: Option<u64>, limit: usize) -> Result<Vec<EventEnvelope>> {
        if limit == 0 || limit > 4096 {
            return Err(Error::Limit("history page"));
        }
        let first = after
            .map_or(Some(0), |value| value.checked_add(1))
            .ok_or(Error::Limit("history ordinal"))?;
        let first = usize::try_from(first).map_err(|_| Error::Limit("history ordinal"))?;
        let events = &self.state().events;
        if first > events.len() {
            return Err(Error::Conflict("history ordinal ahead of owner"));
        }
        let mut rows = Vec::new();
        let mut bytes = 0usize;
        for event in events.iter().skip(first).take(limit) {
            let size = encoded_len(event)?;
            if size > MAX_COMMIT_BYTES {
                return Err(Error::Limit("history event row"));
            }
            if size > MAX_COMMIT_BYTES - bytes {
                break;
            }
            bytes += size;
            rows.push(event.clone());
        }
        Ok(rows)
    }
    /// Complete command receipt facts in canonical command-key order. The
    /// exclusive key cursor and byte/row bounds permit streaming source
    /// commitments without deriving receipt existence from retained events.
    async fn history_commands(
        &self,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<(String, CommandReceipt)>> {
        if limit == 0 || limit > 4096 {
            return Err(Error::Limit("command history page"));
        }
        let mut rows = Vec::new();
        let mut bytes = 0usize;
        for (key, receipt) in self
            .state()
            .commands
            .iter()
            .filter(|(key, _)| after.is_none_or(|after| key.as_str() > after))
            .take(limit)
        {
            let size = encoded_len(&(key, receipt))?;
            if size > MAX_COMMIT_BYTES {
                return Err(Error::Limit("command history row"));
            }
            if size > MAX_COMMIT_BYTES - bytes {
                break;
            }
            bytes += size;
            rows.push((key.clone(), receipt.clone()));
        }
        Ok(rows)
    }
    /// Complete ordered artifact-reference index under the current owner root.
    /// Returns each matching envelope once, with its global ordinal, regardless
    /// of kind, schema, redaction, or session. Cursor is exclusive; empty proves
    /// exhaustion. Durable implementations must authenticate index completeness,
    /// not merely return matching mutable database rows. Bounds match history_events.
    async fn history_artifact_events(
        &self,
        workspace: &WorkspaceId,
        artifact: &ArtifactId,
        after: Option<u64>,
        limit: usize,
    ) -> Result<Vec<(u64, EventEnvelope)>> {
        if limit == 0 || limit > 4096 {
            return Err(Error::Limit("artifact history page"));
        }
        let first = after
            .map_or(Some(0), |value| value.checked_add(1))
            .ok_or(Error::Limit("history ordinal"))?;
        let first = usize::try_from(first).map_err(|_| Error::Limit("history ordinal"))?;
        let events = &self.state().events;
        if first > events.len() {
            return Err(Error::Conflict("history ordinal ahead of owner"));
        }
        let mut rows = Vec::new();
        let mut bytes = 0usize;
        for (ordinal, event) in events
            .iter()
            .enumerate()
            .skip(first)
            .filter(|(_, event)| {
                &event.event.workspace == workspace && event.event.artifacts.contains(artifact)
            })
            .take(limit)
        {
            let size = encoded_len(event)?;
            if size > MAX_COMMIT_BYTES {
                return Err(Error::Limit("history event row"));
            }
            if size > MAX_COMMIT_BYTES - bytes {
                break;
            }
            bytes += size;
            rows.push((
                u64::try_from(ordinal).map_err(|_| Error::Limit("history ordinal"))?,
                event.clone(),
            ));
        }
        Ok(rows)
    }
    /// Authenticated exact global ordinal, or absence at/beyond the owner's end.
    async fn history_event_at(&self, ordinal: u64) -> Result<Option<EventEnvelope>> {
        let ordinal = usize::try_from(ordinal).map_err(|_| Error::Limit("history ordinal"))?;
        let event = self.state().events.get(ordinal);
        if event
            .map(encoded_len)
            .transpose()?
            .is_some_and(|size| size > MAX_COMMIT_BYTES)
        {
            return Err(Error::Limit("history event row"));
        }
        Ok(event.cloned())
    }

    /// Exact identity lookup in validated canonical history. Event identities
    /// are unique; absence is distinct from a failed authenticated read.
    async fn history_event(&self, id: &EventId) -> Result<Option<EventEnvelope>> {
        let event = self.state().events.iter().find(|row| &row.event.id == id);
        if event
            .map(encoded_len)
            .transpose()?
            .is_some_and(|size| size > MAX_COMMIT_BYTES)
        {
            return Err(Error::Limit("history event row"));
        }
        Ok(event.cloned())
    }
    /// Authenticated canonical fact lookup only. Callers must separately prove
    /// command meaning and disclosure scope; this is not replay authorization.
    /// The resident adapter is replaced by the admitted catalog before eviction.
    async fn command_receipt_by_id(
        &self,
        workspace: &WorkspaceId,
        command: &CommandId,
    ) -> Result<Option<CommandReceipt>> {
        Ok(self
            .state()
            .commands
            .get(&command_key(workspace, command))
            .cloned())
    }
    /// Exact command-meaning replay remains available after event retention.
    /// This resident-state adapter is replaced by authenticated durable lookup
    /// before history eviction; an unbound mutable database row is insufficient.
    async fn command_receipt(
        &self,
        workspace: &WorkspaceId,
        command: &CommandId,
        digest: &str,
    ) -> Result<Option<CommandReceipt>> {
        self.state().command(workspace, command, digest)
    }
    /// Public queries also require a nonempty complete correlation group proving
    /// session scope. Scope-prefiltered pages cannot establish this proof.
    async fn scoped_command_receipt(
        &self,
        workspace: &WorkspaceId,
        session: &SessionId,
        command: &CommandId,
    ) -> Result<Option<CommandReceipt>> {
        let state = self.state();
        let Some(receipt) = state.commands.get(&command_key(workspace, command)) else {
            return Ok(None);
        };
        if &receipt.workspace != workspace || &receipt.command != command {
            return Err(Error::Access);
        }
        let mut found = false;
        for event in state.events.iter().filter(|event| {
            event.watermark == receipt.watermark && &event.event.correlation == command
        }) {
            if &event.event.workspace != workspace || &event.event.session != session {
                return Err(Error::Access);
            }
            found = true;
        }
        Ok(found.then(|| receipt.clone()))
    }
    /// Exact durable transaction identity, including receipt-only retries.
    /// Durable implementations authenticate this lookup under the owner cut.
    async fn transaction_receipt(&self, id: &TransactionId) -> Result<Option<Receipt>> {
        let state = self.state();
        let Some(receipt) = state.transactions.get(id) else {
            return Ok(None);
        };
        if &receipt.transaction != id || receipt.watermark > state.watermark {
            return Err(Error::Corruption("history transaction locator identity"));
        }
        if encoded_len(receipt)? > MAX_COMMIT_BYTES {
            return Err(Error::Limit("history transaction row"));
        }
        Ok(Some(receipt.clone()))
    }
    async fn archive_state(&self) -> Result<State> {
        Ok(self.state().clone())
    }
    async fn transact(&mut self, transaction: Transaction) -> Result<Receipt>;
}

impl<T: ReferenceStore> CanonicalStore for T {
    fn current(&self) -> crate::CurrentStateView<'_> {
        ReferenceStore::current(self)
    }
    async fn history_event_count(&self) -> Result<u64> {
        ReferenceStore::history_event_count(self).await
    }
    async fn history_events(&self, after: Option<u64>, limit: usize) -> Result<Vec<EventEnvelope>> {
        ReferenceStore::history_events(self, after, limit).await
    }
    async fn history_commands(
        &self,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<(String, CommandReceipt)>> {
        ReferenceStore::history_commands(self, after, limit).await
    }
    async fn history_artifact_events(
        &self,
        workspace: &WorkspaceId,
        artifact: &ArtifactId,
        after: Option<u64>,
        limit: usize,
    ) -> Result<Vec<(u64, EventEnvelope)>> {
        ReferenceStore::history_artifact_events(self, workspace, artifact, after, limit).await
    }
    async fn history_event_at(&self, ordinal: u64) -> Result<Option<EventEnvelope>> {
        ReferenceStore::history_event_at(self, ordinal).await
    }
    async fn history_event(&self, id: &EventId) -> Result<Option<EventEnvelope>> {
        ReferenceStore::history_event(self, id).await
    }
    async fn command_receipt_by_id(
        &self,
        workspace: &WorkspaceId,
        command: &CommandId,
    ) -> Result<Option<CommandReceipt>> {
        ReferenceStore::command_receipt_by_id(self, workspace, command).await
    }
    async fn command_receipt(
        &self,
        workspace: &WorkspaceId,
        command: &CommandId,
        digest: &str,
    ) -> Result<Option<CommandReceipt>> {
        ReferenceStore::command_receipt(self, workspace, command, digest).await
    }
    async fn scoped_command_receipt(
        &self,
        workspace: &WorkspaceId,
        session: &SessionId,
        command: &CommandId,
    ) -> Result<Option<CommandReceipt>> {
        ReferenceStore::scoped_command_receipt(self, workspace, session, command).await
    }
    async fn transaction_receipt(&self, id: &TransactionId) -> Result<Option<Receipt>> {
        ReferenceStore::transaction_receipt(self, id).await
    }
    async fn transact(&mut self, transaction: Transaction) -> Result<Receipt> {
        ReferenceStore::transact(self, transaction).await
    }
    async fn archive_state(&self) -> Result<State> {
        ReferenceStore::archive_state(self).await
    }
}
