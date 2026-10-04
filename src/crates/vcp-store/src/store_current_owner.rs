// SPDX-License-Identifier: Apache-2.0
//! Replacement Store ownership methods under qualification before the public
//! API cut. No complete historical State is retained or implicitly reconstructed.
use super::*;
use crate::{durable_owner::Outcome, store_history_reader::ReadPages};
use vcp_domain::{ArtifactId, CommandId, EventId, SessionId, TransactionId, WorkspaceId};
use vcp_protocol::{command::CommandReceipt, event::EventEnvelope};

macro_rules! read {
    ($owner:ident, $pages:ident, $request:expr) => {{
        $owner.ensure_healthy()?;
        let mut $pages = ReadPages::from_locked(&$owner._lock, $owner.kind())?;
        let result = $request.await;
        let closed = $pages.close().await;
        match result {
            Err(error) => Err(error),
            Ok(value) => closed.map(|()| value),
        }
    }};
}
impl Opened {
    pub(crate) fn canonical_lock(&self) -> &crate::canonical_lock::CanonicalLock {
        &self._lock
    }
    pub(crate) fn snapshot(
        &self,
    ) -> Result<crate::store::current_migration::snapshot::PinnedDurableSnapshot> {
        crate::store::current_migration::snapshot::PinnedDurableSnapshot::from_opened(self)
    }
    pub(crate) fn kind(&self) -> BackendKind {
        match &self.backend {
            Backend::Files(_) => BackendKind::Files,
            Backend::Sqlite(_) => BackendKind::Sqlite,
        }
    }
    pub(crate) fn ensure_healthy(&self) -> Result<()> {
        if self.poisoned {
            return Err(Error::Unavailable("reopen after indeterminate commit"));
        }
        Ok(())
    }
    pub(crate) fn current(&self) -> crate::CurrentStateView<'_> {
        self.owner.semantic().current().into()
    }
    pub(crate) fn current_state(&self) -> std::sync::Arc<crate::CurrentState> {
        std::sync::Arc::clone(
            self.current
                .get_or_init(|| std::sync::Arc::new(self.owner.semantic().current().clone())),
        )
    }
    pub(crate) async fn archive_state(&self) -> Result<State> {
        read!(self, pages, self.owner.archive_state(&mut pages))
    }
    pub(crate) async fn logical_digest(&self) -> Result<String> {
        read!(
            self,
            pages,
            self.owner
                .semantic()
                .catalog()
                .legacy_digest(&mut pages, self.current())
        )
    }
    pub(crate) async fn history_event_count(&self) -> Result<u64> {
        self.ensure_healthy()?;
        Ok(self.owner.semantic().catalog().event_count())
    }
    pub(crate) async fn history_events(
        &self,
        after: Option<u64>,
        limit: usize,
    ) -> Result<Vec<EventEnvelope>> {
        read!(
            self,
            pages,
            self.owner
                .semantic()
                .catalog()
                .event_page(&mut pages, after, limit)
        )
    }
    pub(crate) async fn history_event_at(&self, ordinal: u64) -> Result<Option<EventEnvelope>> {
        read!(
            self,
            pages,
            self.owner
                .semantic()
                .catalog()
                .event_at(&mut pages, ordinal)
        )
    }
    pub(crate) async fn history_event(&self, id: &EventId) -> Result<Option<EventEnvelope>> {
        read!(
            self,
            pages,
            self.owner.semantic().catalog().event(&mut pages, id)
        )
    }
    pub(crate) async fn history_artifact_events(
        &self,
        workspace: &WorkspaceId,
        artifact: &ArtifactId,
        after: Option<u64>,
        limit: usize,
    ) -> Result<Vec<(u64, EventEnvelope)>> {
        read!(
            self,
            pages,
            self.owner
                .semantic()
                .catalog()
                .artifact_events(&mut pages, workspace, artifact, after, limit)
        )
    }
    pub(crate) async fn history_commands(
        &self,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<(String, CommandReceipt)>> {
        read!(
            self,
            pages,
            self.owner
                .semantic()
                .catalog()
                .command_page(&mut pages, after, limit)
        )
    }
    pub(crate) async fn command_receipt_by_id(
        &self,
        workspace: &WorkspaceId,
        command: &CommandId,
    ) -> Result<Option<CommandReceipt>> {
        read!(
            self,
            pages,
            self.owner
                .semantic()
                .catalog()
                .command_unchecked_meaning(&mut pages, workspace, command)
        )
    }
    pub(crate) async fn command_receipt(
        &self,
        workspace: &WorkspaceId,
        command: &CommandId,
        digest: &str,
    ) -> Result<Option<CommandReceipt>> {
        read!(
            self,
            pages,
            self.owner
                .semantic()
                .catalog()
                .command(&mut pages, workspace, command, digest)
        )
    }
    pub(crate) async fn scoped_command_receipt(
        &self,
        workspace: &WorkspaceId,
        session: &SessionId,
        command: &CommandId,
    ) -> Result<Option<CommandReceipt>> {
        read!(
            self,
            pages,
            self.owner
                .semantic()
                .catalog()
                .scoped_command(&mut pages, workspace, session, command)
        )
    }
    pub(crate) async fn transaction_receipt(&self, id: &TransactionId) -> Result<Option<Receipt>> {
        read!(
            self,
            pages,
            self.owner.semantic().catalog().transaction(&mut pages, id)
        )
    }
    pub(crate) async fn transact(
        &mut self,
        transaction: Transaction,
        observe: &impl Fn(Barrier),
    ) -> Result<Receipt> {
        self.ensure_healthy()?;
        let preparation_started = Instant::now();
        let outcome = async {
            match &mut self.backend {
                Backend::Files(_) => {
                    let directory = self
                        .pages
                        .as_ref()
                        .ok_or(Error::Corruption("history page directory"))?;
                    self.owner
                        .prepare(&mut io::Files::new(directory), &transaction)
                        .await
                }
                Backend::Sqlite(db) => {
                    self.owner
                        .prepare(&mut io::Sqlite::new(db), &transaction)
                        .await
                }
            }
        }
        .await;
        self.diagnostics
            .preparation
            .record(preparation_started, outcome.is_ok());
        let prepared = match outcome? {
            Outcome::Duplicate(receipt) => {
                self.diagnostics.duplicate_transactions =
                    self.diagnostics.duplicate_transactions.saturating_add(1);
                return Ok(receipt);
            }
            Outcome::Prepared(prepared) => prepared,
        };
        for mutation in &transaction.mutations {
            if let Mutation::Put { record, .. } = mutation {
                if record.collection == Collection::Artifact {
                    self.spool.verify(&record.decode()?)?;
                }
            }
        }
        observe(Barrier::Prepared);
        let payload = canonical_bytes(prepared.commit())?;
        let receipt = prepared.commit().receipt.clone();
        self.poisoned = true;
        let append_started = Instant::now();
        let appended: Result<DurableOwner> = async {
            match &mut self.backend {
                Backend::Files(journal) => {
                    let directory = self
                        .pages
                        .as_ref()
                        .ok_or(Error::Corruption("history page directory"))?;
                    let (next, publication) = crate::history_publication::stage(
                        &mut io::Files::new(directory),
                        &self.owner,
                        &prepared,
                        &payload,
                    )
                    .await?;
                    journal.append_current(&payload, receipt.watermark, &publication, observe)?;
                    Ok(next)
                }
                Backend::Sqlite(db) => {
                    append_sqlite(db, &self.owner, &prepared, &payload, observe).await
                }
            }
        }
        .await;
        self.diagnostics
            .append
            .record(append_started, appended.is_ok());
        self.owner = std::sync::Arc::new(appended?);
        self.current.take();
        self.diagnostics.current_watermark = self.current().watermark.get();
        self.poisoned = false;
        observe(Barrier::BeforeReply);
        Ok(receipt)
    }
}
