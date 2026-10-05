// SPDX-License-Identifier: Apache-2.0
//! Async historical obligations resolved against one admitted, pinned cut.
//! The callback is pure; unavailable data suspends it until exact authenticated
//! evidence is available, and physical read failures poison the resolver.
use crate::{
    admitted_history::AdmittedCut,
    contract::{current_preparation::PreparationHistory, Receipt},
    fork_contract::ForkHistory,
    historical_facts::{EventFact, EventFacts, TransactionFacts},
    history_catalog::Catalog,
    history_index::Pages,
    CurrentStateView, Error, Result,
};
use std::collections::BTreeMap;
use vcp_domain::{CommandId, EventId, SessionId, TransactionId, Watermark, WorkspaceId};
use vcp_protocol::event::EventEnvelope;

#[derive(Debug)]
struct Pending;
impl std::fmt::Display for Pending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("internal historical obligation")
    }
}
impl std::error::Error for Pending {}
fn pending(error: &Error) -> bool {
    matches!(error, Error::Io(error) if error.get_ref().is_some_and(|inner| inner.is::<Pending>()))
}
enum Need {
    Fact(EventId),
    Envelope(EventId),
    Receipt(TransactionId),
    Watermark(TransactionId),
    Command(WorkspaceId, CommandId),
    Session(SessionId),
    SourceDigest,
}
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ResolutionDiagnostics {
    pub(crate) passes: u64,
    pub(crate) resolutions: u64,
    pub(crate) event_reads: u64,
    pub(crate) transaction_reads: u64,
    pub(crate) command_reads: u64,
    pub(crate) session_pages: u64,
    pub(crate) session_rows: u64,
    pub(crate) digest_passes: u64,
}
pub(crate) struct ResolvedHistory<'a> {
    catalog: &'a Catalog,
    source: CurrentStateView<'a>,
    facts: BTreeMap<EventId, Option<EventFact>>,
    envelopes: BTreeMap<EventId, Option<EventEnvelope>>,
    receipts: BTreeMap<TransactionId, Option<Receipt>>,
    watermarks: BTreeMap<TransactionId, Option<Watermark>>,
    commands: BTreeMap<(WorkspaceId, CommandId), bool>,
    sessions: BTreeMap<SessionId, bool>,
    digest: Option<String>,
    need: Option<Need>,
    poisoned: bool,
    diagnostics: ResolutionDiagnostics,
}
impl<'a> ResolvedHistory<'a> {
    // The private admitted pair prevents accidental same-watermark mixing of
    // independently loaded current records and historical roots.
    pub(crate) fn new(cut: &'a AdmittedCut) -> Self {
        Self {
            catalog: cut.catalog(),
            source: cut.current().into(),
            facts: BTreeMap::new(),
            envelopes: BTreeMap::new(),
            receipts: BTreeMap::new(),
            watermarks: BTreeMap::new(),
            commands: BTreeMap::new(),
            sessions: BTreeMap::new(),
            digest: None,
            need: None,
            poisoned: false,
            diagnostics: ResolutionDiagnostics::default(),
        }
    }
    pub(crate) fn diagnostics(&self) -> ResolutionDiagnostics {
        self.diagnostics
    }
    // Callback must never publish, dispatch tools or mutate source. Cancellation
    // and physical read failures poison the resolver before any tentative reuse.
    pub(crate) async fn run<T>(
        &mut self,
        pages: &mut impl Pages,
        mut check: impl FnMut(&mut Self) -> Result<T>,
    ) -> Result<T> {
        if self.poisoned {
            return Err(Error::Unavailable("historical resolver failed"));
        }
        self.poisoned = true;
        loop {
            self.need = None;
            self.diagnostics.passes = self.diagnostics.passes.saturating_add(1);
            match check(self) {
                Ok(value) if self.need.is_none() => {
                    self.poisoned = false;
                    return Ok(value);
                }
                Ok(_) => return Err(Error::Corruption("historical obligation ignored")),
                Err(error) if pending(&error) => {
                    let need = self
                        .need
                        .take()
                        .ok_or(Error::Corruption("historical obligation missing"))?;
                    self.resolve(pages, need).await?;
                }
                Err(error) => {
                    // A callback that swallowed/replaced a pending obligation
                    // must not permit this resolver to validate another result.
                    if self.need.is_none() {
                        self.poisoned = false;
                    }
                    return Err(error);
                }
            }
        }
    }
    fn request<T>(&mut self, need: Need) -> Result<T> {
        if self.need.is_none() {
            self.need = Some(need);
        }
        Err(Error::Io(std::io::Error::other(Pending)))
    }
    async fn resolve(&mut self, pages: &mut impl Pages, need: Need) -> Result<()> {
        self.diagnostics.resolutions = self.diagnostics.resolutions.saturating_add(1);
        match need {
            Need::Fact(id) => {
                self.diagnostics.event_reads = self.diagnostics.event_reads.saturating_add(1);
                let row = self.catalog.event(pages, &id).await?;
                self.facts.insert(id, row.as_ref().map(EventFact::from));
            }
            Need::Envelope(id) => {
                self.diagnostics.event_reads = self.diagnostics.event_reads.saturating_add(1);
                let row = self.catalog.event(pages, &id).await?;
                self.facts
                    .insert(id.clone(), row.as_ref().map(EventFact::from));
                self.envelopes.insert(id, row);
            }
            Need::Receipt(id) => {
                self.diagnostics.transaction_reads =
                    self.diagnostics.transaction_reads.saturating_add(1);
                let row = self.catalog.transaction(pages, &id).await?;
                self.watermarks
                    .insert(id.clone(), row.as_ref().map(|r| r.watermark));
                self.receipts.insert(id, row);
            }
            Need::Watermark(id) => {
                self.diagnostics.transaction_reads =
                    self.diagnostics.transaction_reads.saturating_add(1);
                let row = self.catalog.transaction(pages, &id).await?;
                self.watermarks.insert(id, row.map(|r| r.watermark));
            }
            Need::Command(workspace, command) => {
                self.diagnostics.command_reads = self.diagnostics.command_reads.saturating_add(1);
                let value = self
                    .catalog
                    .command_unchecked_meaning(pages, &workspace, &command)
                    .await?
                    .is_some();
                self.commands.insert((workspace, command), value);
            }
            Need::Session(session) => {
                let mut next = 0u64;
                let mut found = false;
                while next < self.catalog.event_count() {
                    self.diagnostics.session_pages =
                        self.diagnostics.session_pages.saturating_add(1);
                    let rows = self
                        .catalog
                        .event_page(pages, next.checked_sub(1), 4096)
                        .await?;
                    if rows.is_empty() {
                        return Err(Error::Corruption("historical session scan incomplete"));
                    }
                    self.diagnostics.session_rows = self
                        .diagnostics
                        .session_rows
                        .saturating_add(rows.len() as u64);
                    if rows.iter().any(|row| row.event.session == session) {
                        found = true;
                        break;
                    }
                    next += rows.len() as u64;
                }
                self.sessions.insert(session, found);
            }
            Need::SourceDigest => {
                self.diagnostics.digest_passes = self.diagnostics.digest_passes.saturating_add(1);
                self.digest = Some(self.catalog.legacy_digest(pages, self.source).await?);
            }
        }
        Ok(())
    }
}
impl EventFacts for ResolvedHistory<'_> {
    fn last(&mut self, id: &EventId) -> Result<Option<EventFact>> {
        match self.facts.get(id) {
            Some(v) => Ok(v.clone()),
            None => self.request(Need::Fact(id.clone())),
        }
    }
    fn any(&mut self, id: &EventId, predicate: &dyn Fn(&EventFact) -> bool) -> Result<bool> {
        // The source catalog has unique IDs proved by mandatory validation.
        Ok(self.last(id)?.as_ref().is_some_and(predicate))
    }
}
impl TransactionFacts for ResolvedHistory<'_> {
    fn watermark(&mut self, id: &TransactionId) -> Result<Option<Watermark>> {
        match self.watermarks.get(id) {
            Some(v) => Ok(*v),
            None => self.request(Need::Watermark(id.clone())),
        }
    }
}
impl ForkHistory for ResolvedHistory<'_> {
    fn has_session(&mut self, id: &SessionId) -> Result<bool> {
        match self.sessions.get(id) {
            Some(v) => Ok(*v),
            None => self.request(Need::Session(id.clone())),
        }
    }
    fn any_event(
        &mut self,
        id: &EventId,
        predicate: &dyn Fn(&EventEnvelope) -> bool,
    ) -> Result<bool> {
        match self.envelopes.get(id) {
            Some(v) => Ok(v.as_ref().is_some_and(predicate)),
            None => self.request(Need::Envelope(id.clone())),
        }
    }
}
impl PreparationHistory for ResolvedHistory<'_> {
    fn transaction_receipt(&mut self, id: &TransactionId) -> Result<Option<Receipt>> {
        match self.receipts.get(id) {
            Some(v) => Ok(v.clone()),
            None => self.request(Need::Receipt(id.clone())),
        }
    }
    fn command_present(&mut self, workspace: &WorkspaceId, command: &CommandId) -> Result<bool> {
        match self.commands.get(&(workspace.clone(), command.clone())) {
            Some(v) => Ok(*v),
            None => self.request(Need::Command(workspace.clone(), command.clone())),
        }
    }
    fn source_digest(&mut self) -> Result<String> {
        match &self.digest {
            Some(v) => Ok(v.clone()),
            None => self.request(Need::SourceDigest),
        }
    }
}
#[cfg(test)]
#[path = "resolved_history_tests.rs"]
mod tests;

/// Candidate predicates see source history followed by the actual proposed
/// append. Preparation/fork predicates deliberately have no overlay adapter:
/// duplicate admission and prior publication evidence must use the source cut.
pub(crate) struct CandidateHistory<'a, 'b, 'source> {
    source: &'a mut ResolvedHistory<'source>,
    proposed: &'b crate::contract::current_preparation::ProposedTransition,
}
impl<'a, 'b, 'source> CandidateHistory<'a, 'b, 'source> {
    pub(crate) fn new(
        source: &'a mut ResolvedHistory<'source>,
        proposed: &'b crate::contract::current_preparation::ProposedTransition,
    ) -> Self {
        Self { source, proposed }
    }
}
impl EventFacts for CandidateHistory<'_, '_, '_> {
    fn last(&mut self, id: &EventId) -> Result<Option<EventFact>> {
        match self
            .proposed
            .events
            .iter()
            .rev()
            .find(|row| &row.event.id == id)
        {
            Some(row) => Ok(Some(EventFact::from(row))),
            None => self.source.last(id),
        }
    }
    fn any(&mut self, id: &EventId, predicate: &dyn Fn(&EventFact) -> bool) -> Result<bool> {
        if self.source.any(id, predicate)? {
            return Ok(true);
        }
        Ok(self
            .proposed
            .events
            .iter()
            .filter(|row| &row.event.id == id)
            .any(|row| predicate(&EventFact::from(row))))
    }
}
impl TransactionFacts for CandidateHistory<'_, '_, '_> {
    fn watermark(&mut self, id: &TransactionId) -> Result<Option<Watermark>> {
        if id == &self.proposed.receipt.transaction {
            Ok(Some(self.proposed.receipt.watermark))
        } else {
            self.source.watermark(id)
        }
    }
}
