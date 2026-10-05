// SPDX-License-Identifier: Apache-2.0
//! Trusted read-only native snapshot. No archival DTO and no wire authorization.
use super::*;
use vcp_protocol::event::EventEnvelope;

pub struct HistoryPage {
    pub watermark: Watermark,
    pub count: u64,
    pub events: Vec<EventEnvelope>,
}

/// Current records and authenticated history refer to one pinned owner cut.
/// Later appends cannot change this reader; dropping it releases native pins.
pub struct HistoryReader {
    snapshot: Arc<vcp_store::Snapshot>,
    worker: worker::Worker,
}

impl CanonicalHost {
    pub fn retained_start_budget(
        &self,
        scope: Scope,
    ) -> Result<Option<vcp_engine::public_start::RetainedStartBudget>, String> {
        self.worker.run_cleanup(move |context| {
            let store = context.engine.store();
            if !store
                .current()
                .records
                .contains_key(&vcp_store::contract::key(
                    vcp_store::contract::Collection::Task,
                    scope.task.as_str(),
                ))
            {
                return Ok(None);
            }
            context
                .runtime
                .block_on(vcp_engine::public_start::retained_start_budget_store(
                    store, &scope,
                ))
                .map_err(|_| "original run budget evidence unavailable".into())
        })
    }
    pub fn history_reader(&self) -> Result<HistoryReader, String> {
        let snapshot = self
            .worker
            .run_cleanup(|context| Ok(Arc::new(context.engine.store().snapshot()?)))?;
        Ok(HistoryReader {
            snapshot,
            worker: self.worker.clone(),
        })
    }
}

impl HistoryReader {
    pub fn current(&self) -> vcp_store::CurrentStateView<'_> {
        self.snapshot.current()
    }

    /// Native row/byte bounds apply. A short nonempty page is not EOF.
    pub fn page(&self, after: Option<u64>, limit: usize) -> Result<HistoryPage, String> {
        let snapshot = Arc::clone(&self.snapshot);
        self.worker.run_cleanup(move |context| {
            let watermark = snapshot.current().watermark;
            let count = context.runtime.block_on(snapshot.history_event_count())?;
            let events = context
                .runtime
                .block_on(snapshot.history_events(after, limit))?;
            if events.iter().any(|event| event.watermark > watermark) {
                return Err("snapshot history exceeded its cut".into());
            }
            Ok(HistoryPage {
                watermark,
                count,
                events,
            })
        })
    }

    pub fn event(&self, id: EventId) -> Result<Option<EventEnvelope>, String> {
        let snapshot = Arc::clone(&self.snapshot);
        self.worker.run_cleanup(move |context| {
            let event = context.runtime.block_on(snapshot.history_event(&id))?;
            if event.as_ref().is_some_and(|event| {
                event.event.id != id || event.watermark > snapshot.current().watermark
            }) {
                return Err("snapshot event identity mismatch".into());
            }
            Ok(event)
        })
    }

    pub fn event_at(&self, ordinal: u64) -> Result<Option<EventEnvelope>, String> {
        let snapshot = Arc::clone(&self.snapshot);
        self.worker.run_cleanup(move |context| {
            let event = context
                .runtime
                .block_on(snapshot.history_event_at(ordinal))?;
            if event
                .as_ref()
                .is_some_and(|event| event.watermark > snapshot.current().watermark)
            {
                return Err("snapshot event exceeded its cut".into());
            }
            Ok(event)
        })
    }

    /// Identity lookup only; callers preserve scope and command-meaning checks.
    pub fn command(
        &self,
        workspace: WorkspaceId,
        command: CommandId,
    ) -> Result<Option<CommandReceipt>, String> {
        let snapshot = Arc::clone(&self.snapshot);
        self.worker.run_cleanup(move |context| {
            let receipt = context
                .runtime
                .block_on(snapshot.command_receipt_by_id(&workspace, &command))?;
            if receipt.as_ref().is_some_and(|receipt| {
                receipt.command != command
                    || receipt.workspace != workspace
                    || receipt.watermark > snapshot.current().watermark
            }) {
                return Err("snapshot receipt identity mismatch".into());
            }
            Ok(receipt)
        })
    }
}
