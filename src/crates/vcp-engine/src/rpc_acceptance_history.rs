// SPDX-License-Identifier: Apache-2.0
//! Bounded global history traversal for receipt projection proofs.
use vcp_domain::revision::Watermark;
use vcp_protocol::event::EventEnvelope;
use vcp_store::{contract::CanonicalStore, Error, Result};

pub(super) struct Pages {
    watermark: Watermark,
    end: u64,
    next: u64,
}
impl Pages {
    pub(super) async fn open<S: CanonicalStore>(store: &S) -> Result<Self> {
        let watermark = store.current().watermark;
        let end = store.history_event_count().await?;
        Ok(Self {
            watermark,
            end,
            next: 0,
        })
    }
    pub(super) async fn next<S: CanonicalStore>(
        &mut self,
        store: &S,
    ) -> Result<Option<Vec<EventEnvelope>>> {
        if store.current().watermark != self.watermark {
            return Err(Error::Conflict("receipt history owner changed"));
        }
        if self.next == self.end {
            return Ok(None);
        }
        let limit = (self.end - self.next).min(4096) as usize;
        let rows = store
            .history_events(self.next.checked_sub(1), limit)
            .await?;
        if rows.is_empty()
            || rows.len() > limit
            || rows.iter().any(|event| event.watermark > self.watermark)
        {
            return Err(Error::Corruption("receipt history page"));
        }
        if store.current().watermark != self.watermark {
            return Err(Error::Conflict("receipt history owner changed"));
        }
        self.next += rows.len() as u64;
        Ok(Some(rows))
    }
}
