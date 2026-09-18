// SPDX-License-Identifier: Apache-2.0
use crate::{Access, Engine, Error, Result};
use vcp_domain::{ids::*, revision::*, workspace::Workspace};
use vcp_protocol::subscription::*;
use vcp_store::contract::{CanonicalStore, Collection};
pub(crate) const MAX_SUBSCRIPTIONS: usize = 16;
impl<S: CanonicalStore> Engine<S> {
    pub fn subscribe(
        &mut self,
        access: &Access,
        after: SessionSeq,
        limit: u32,
        now: Timestamp,
    ) -> Result<Cursor> {
        self.authorize(access)?;
        if limit == 0 || limit as usize > vcp_protocol::version::MAX_PAGE_EVENTS {
            return Err(vcp_protocol::version::Error::Limit.into());
        }
        self.subscriptions
            .retain(|_, cursor| cursor.expires_at > now);
        if self.subscriptions.len() >= MAX_SUBSCRIPTIONS {
            return Err(vcp_protocol::version::Error::Limit.into());
        }
        let state = self.store().state();
        let workspace: Workspace = state
            .record(
                Collection::Workspace,
                access.workspace.as_str(),
                &access.workspace,
            )?
            .decode()?;
        let end = state
            .sequences
            .get(&access.session)
            .copied()
            .unwrap_or_default();
        if after > end {
            return Err(Error::Target);
        }
        let cursor = Cursor {
            version: 1,
            snapshot: SnapshotId::new(),
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            after,
            end,
            watermark: state.watermark,
            authority: workspace.authority,
            deletion: workspace.deletion,
            expires_at: Timestamp::new(
                now.get()
                    .checked_add(60_000)
                    .ok_or(vcp_domain::Error::Overflow)?,
            ),
            limit,
        };
        self.subscriptions
            .insert(cursor.snapshot.clone(), cursor.clone());
        Ok(cursor)
    }
    /// Pull-based backpressure: no producer queue is allocated for a slow or
    /// disconnected consumer. Final results remain in canonical ordered events.
    pub fn events(&self, access: &Access, cursor: &Cursor, now: Timestamp) -> Result<EventPage> {
        self.authorize(access)?;
        let gap = |reason| {
            Ok(EventPage::Gap {
                reason,
                restart_from_snapshot: true,
            })
        };
        if cursor.workspace != access.workspace || cursor.session != access.session {
            return Err(Error::Access);
        }
        let Some(original) = self.subscriptions.get(&cursor.snapshot) else {
            return gap(GapReason::SnapshotExpired);
        };
        if cursor.workspace != original.workspace || cursor.session != original.session {
            return gap(GapReason::ScopeChanged);
        }
        if now >= original.expires_at {
            return gap(GapReason::SnapshotExpired);
        }
        if cursor.version != 1
            || cursor.end != original.end
            || cursor.watermark != original.watermark
            || cursor.authority != original.authority
            || cursor.deletion != original.deletion
            || cursor.expires_at != original.expires_at
            || cursor.limit != original.limit
            || cursor.after > cursor.end
            || cursor.after < original.after
        {
            return gap(GapReason::CursorChanged);
        }
        let state = self.store().state();
        let workspace: Workspace = state
            .record(
                Collection::Workspace,
                access.workspace.as_str(),
                &access.workspace,
            )?
            .decode()?;
        if cursor.authority != workspace.authority {
            return gap(GapReason::ScopeChanged);
        }
        if cursor.deletion != workspace.deletion {
            return gap(GapReason::RetentionChanged);
        }
        let events = state
            .events
            .iter()
            .filter(|e| {
                e.event.workspace == access.workspace
                    && e.event.session == access.session
                    && e.sequence > cursor.after
                    && e.sequence <= cursor.end
                    && e.watermark <= cursor.watermark
            })
            .take(cursor.limit as usize)
            .cloned()
            .collect::<Vec<_>>();
        let mut expected = cursor.after;
        for event in &events {
            expected = expected.next()?;
            if event.sequence != expected {
                return gap(GapReason::SequenceUnavailable);
            }
        }
        if events.is_empty() && cursor.after < cursor.end {
            return gap(GapReason::SequenceUnavailable);
        }
        let mut next_cursor = cursor.clone();
        next_cursor.after = expected;
        Ok(EventPage::Events {
            events,
            at_end: next_cursor.after == next_cursor.end,
            next_cursor,
            snapshot_watermark: cursor.watermark,
        })
    }
    pub fn unsubscribe(&mut self, id: &SnapshotId) {
        self.subscriptions.remove(id);
    }
}
