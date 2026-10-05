// SPDX-License-Identifier: Apache-2.0
//! A renewed lease may extend the available history, but never this drain's cut.
use super::*;
use vcp_domain::ids::SnapshotId;

pub(super) trait Source {
    fn subscribe(&self, after: SessionSeq, limit: u32) -> Result<Cursor, String>;
    fn page(&self, cursor: Cursor) -> Result<EventPage, String>;
    fn unsubscribe(&self, snapshot: SnapshotId) -> Result<(), String>;
}

impl Source for CanonicalHost {
    fn subscribe(&self, after: SessionSeq, limit: u32) -> Result<Cursor, String> {
        self.subscribe_events(after, limit)
    }
    fn page(&self, cursor: Cursor) -> Result<EventPage, String> {
        self.events(cursor)
    }
    fn unsubscribe(&self, snapshot: SnapshotId) -> Result<(), String> {
        self.unsubscribe_events(snapshot)
    }
}

pub(super) struct Drain {
    cursor: Cursor,
    end: SessionSeq,
    renewed_after: Option<SessionSeq>,
}

impl Drain {
    pub(super) fn new(cursor: Cursor) -> Self {
        Self {
            end: cursor.end,
            cursor,
            renewed_after: None,
        }
    }
    pub(super) fn end(&self) -> SessionSeq {
        self.end
    }
    pub(super) fn snapshot(&self) -> SnapshotId {
        self.cursor.snapshot.clone()
    }
    pub(super) fn advance(&mut self, cursor: Cursor) {
        self.cursor = cursor;
    }

    pub(super) fn page(&mut self, source: &impl Source) -> Result<EventPage, String> {
        let page = source.page(self.cursor.clone())?;
        if !matches!(
            page,
            EventPage::Gap {
                reason: GapReason::SnapshotExpired,
                ..
            }
        ) {
            return Ok(page);
        }
        // Retry only expiry, and only once without acknowledged page progress.
        // Re-authentication proves the original sequence prefix is still visible;
        // engine paging continues to reject unavailable/non-contiguous history.
        if self.renewed_after == Some(self.cursor.after) {
            return Ok(page);
        }
        let previous = self.cursor.clone();
        source.unsubscribe(previous.snapshot.clone())?;
        self.cursor = source.subscribe(previous.after, previous.limit)?;
        self.renewed_after = Some(previous.after);
        let reason = if self.cursor.workspace != previous.workspace
            || self.cursor.session != previous.session
            || self.cursor.authority != previous.authority
        {
            Some(GapReason::ScopeChanged)
        } else if self.cursor.deletion != previous.deletion {
            Some(GapReason::RetentionChanged)
        } else if self.cursor.version != previous.version
            || self.cursor.after != previous.after
            || self.cursor.limit != previous.limit
            || self.cursor.watermark < previous.watermark
            || self.cursor.end < previous.end
        {
            Some(GapReason::CursorChanged)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Ok(EventPage::Gap {
                reason,
                restart_from_snapshot: true,
            });
        }
        source.page(self.cursor.clone())
    }
}
