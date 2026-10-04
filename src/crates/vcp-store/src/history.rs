// SPDX-License-Identifier: Apache-2.0
//! Bounded canonical history reads. Callers retain authorization and redaction policy.
use crate::{Error, Result, Store};
use std::ops::Bound::{Excluded, Included};
use vcp_domain::{workspace::Scope, CommandId, SessionId, Watermark, WorkspaceId};
use vcp_protocol::{command::CommandReceipt, event::EventEnvelope};

const MAX_PAGE: usize = 4096;

/// A borrowed page from the validated owner. Empty scoped rows may still have
/// a cutoff: unrelated history was scanned and the consumer must advance it.
pub struct EventHistoryPage<'a> {
    pub source_watermark: Watermark,
    pub events: Vec<&'a EventEnvelope>,
    pub scanned: usize,
    pub cutoff: Option<Watermark>,
    pub has_more: bool,
}

/// Receipt identity is workspace-scoped. Receipts do not contain a session or
/// task identity, so this API never claims finer-grained authorization.
pub struct CommandHistoryPage<'a> {
    pub source_watermark: Watermark,
    pub receipts: Vec<&'a CommandReceipt>,
    pub next: Option<CommandId>,
}

impl Store {
    /// Locate one canonical receipt's event group through the existing ordered
    /// watermark index. The group retains its transaction's validated storage
    /// bounds and is never clipped or copied. Actor/correlation authorization
    /// remains the caller's responsibility, as for all raw history reads.
    pub fn receipt_events<'a>(
        &'a self,
        workspace: &'a WorkspaceId,
        session: &'a SessionId,
        receipt: &CommandReceipt,
    ) -> Result<impl Iterator<Item = &'a EventEnvelope> + 'a> {
        let state = self.state();
        if &receipt.workspace != workspace
            || state
                .commands
                .get(&crate::contract::command_key(workspace, &receipt.command))
                != Some(receipt)
        {
            return Err(Error::Conflict(
                "history receipt differs from canonical owner",
            ));
        }
        let start = state
            .events
            .partition_point(|event| event.watermark < receipt.watermark);
        let end = start
            + state.events[start..].partition_point(|event| event.watermark == receipt.watermark);
        Ok(state.events[start..end].iter().filter(move |event| {
            &event.event.workspace == workspace && &event.event.session == session
        }))
    }

    /// Seek the existing watermark ordering without scanning older history.
    /// A page never splits a transaction, whose bounded extension is 4096 rows.
    /// Raw envelopes retain redaction markers; public presentation must still
    /// apply current access and retention masks under the owner's serialization.
    pub fn event_history_page(
        &self,
        scope: &Scope,
        after: Watermark,
        target_rows: usize,
    ) -> Result<EventHistoryPage<'_>> {
        if target_rows == 0 || target_rows > MAX_PAGE {
            return Err(Error::Limit("history page rows"));
        }
        let state = self.state();
        if after > state.watermark {
            return Err(Error::Conflict("history cursor ahead of owner"));
        }
        let start = state
            .events
            .partition_point(|event| event.watermark <= after);
        let mut end = start.saturating_add(target_rows).min(state.events.len());
        while end < state.events.len()
            && end > start
            && state.events[end].watermark == state.events[end - 1].watermark
            && end - start < MAX_PAGE
        {
            end += 1;
        }
        if end < state.events.len()
            && end > start
            && state.events[end].watermark == state.events[end - 1].watermark
        {
            return Err(Error::Limit(
                "history event transaction exceeds bounded page",
            ));
        }
        let scanned = &state.events[start..end];
        Ok(EventHistoryPage {
            source_watermark: state.watermark,
            events: scanned
                .iter()
                .filter(|event| {
                    event.event.workspace == scope.workspace
                        && event.event.session == scope.session
                        && event.event.task.as_ref() == Some(&scope.task)
                })
                .collect(),
            scanned: scanned.len(),
            cutoff: scanned.last().map(|event| event.watermark),
            has_more: end < state.events.len(),
        })
    }

    /// Range over the existing workspace/command-key index. Pin all pages to
    /// one source watermark so newly inserted IDs cannot fall behind a cursor.
    pub fn command_history_page(
        &self,
        workspace: &WorkspaceId,
        source_watermark: Watermark,
        after: Option<&CommandId>,
        limit: usize,
    ) -> Result<CommandHistoryPage<'_>> {
        if limit == 0 || limit > MAX_PAGE {
            return Err(Error::Limit("history page rows"));
        }
        let state = self.state();
        if source_watermark != state.watermark {
            return Err(Error::Conflict(
                "history source changed; restart pagination",
            ));
        }
        let lower = after.map_or_else(
            || Included(format!("{workspace}:")),
            |id| Excluded(crate::contract::command_key(workspace, id)),
        );
        let mut rows = state
            .commands
            .range((lower, Excluded(format!("{workspace};"))));
        let receipts: Vec<_> = rows
            .by_ref()
            .take(limit)
            .map(|(_, receipt)| receipt)
            .collect();
        if receipts
            .iter()
            .any(|receipt| &receipt.workspace != workspace)
        {
            return Err(Error::Corruption("history receipt workspace"));
        }
        let next = rows
            .next()
            .and_then(|_| receipts.last().map(|receipt| receipt.command.clone()));
        Ok(CommandHistoryPage {
            source_watermark,
            receipts,
            next,
        })
    }
}
