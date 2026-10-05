// SPDX-License-Identifier: Apache-2.0
//! Bounded canonical history reads. Authorization remains with the caller.
use crate::{
    contract::{encoded_len, CanonicalStore, MAX_COMMIT_BYTES},
    Error, Result, Store,
};
use vcp_domain::{workspace::Scope, CommandId, SessionId, Watermark, WorkspaceId};
use vcp_protocol::{command::CommandReceipt, event::EventEnvelope};
const MAX_PAGE: usize = 4096;
pub struct EventHistoryPage {
    pub source_watermark: Watermark,
    pub events: Vec<EventEnvelope>,
    pub scanned: usize,
    pub cutoff: Option<Watermark>,
    pub has_more: bool,
}
pub struct CommandHistoryPage {
    pub source_watermark: Watermark,
    pub receipts: Vec<CommandReceipt>,
    pub next: Option<CommandId>,
}
impl Store {
    async fn event_lower_bound(&self, watermark: Watermark, inclusive: bool) -> Result<u64> {
        let mut low = 0;
        let mut high = self.history_event_count().await?;
        while low < high {
            let mid = low + (high - low) / 2;
            let row = self
                .history_event_at(mid)
                .await?
                .ok_or(Error::Corruption("history ordinal missing"))?;
            if row.watermark < watermark || (!inclusive && row.watermark == watermark) {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        Ok(low)
    }
    /// Exact retained receipt group, with the original workspace/session filter.
    /// No correlation or presentation policy is silently added by this reader.
    pub async fn receipt_events(
        &self,
        workspace: &WorkspaceId,
        session: &SessionId,
        receipt: &CommandReceipt,
    ) -> Result<std::vec::IntoIter<EventEnvelope>> {
        if &receipt.workspace != workspace
            || self
                .command_receipt_by_id(workspace, &receipt.command)
                .await?
                .as_ref()
                != Some(receipt)
        {
            return Err(Error::Conflict(
                "history receipt differs from canonical owner",
            ));
        }
        let start = self.event_lower_bound(receipt.watermark, true).await?;
        let end = self.event_lower_bound(receipt.watermark, false).await?;
        let rows = self.event_range(start, end).await?;
        Ok(rows
            .into_iter()
            .filter(|row| &row.event.workspace == workspace && &row.event.session == session)
            .collect::<Vec<_>>()
            .into_iter())
    }
    async fn event_range(&self, start: u64, end: u64) -> Result<Vec<EventEnvelope>> {
        if end < start || end - start > MAX_PAGE as u64 {
            return Err(Error::Limit(
                "history event transaction exceeds bounded page",
            ));
        }
        let mut at = start;
        let mut rows = Vec::new();
        let mut bytes = 0usize;
        while at < end {
            let page = self
                .history_events(
                    at.checked_sub(1),
                    usize::try_from(end - at).map_err(|_| Error::Limit("history ordinal"))?,
                )
                .await?;
            if page.is_empty() || page.len() as u64 > end - at {
                return Err(Error::Corruption("history range incomplete"));
            }
            for row in page {
                bytes = bytes
                    .checked_add(encoded_len(&row)?)
                    .ok_or(Error::Limit("history page bytes"))?;
                // A whole transaction includes envelope metadata beyond the
                // original bounded commit body; it must never be clipped.
                if bytes > 2 * MAX_COMMIT_BYTES {
                    return Err(Error::Limit("history page bytes"));
                }
                rows.push(row);
                at += 1;
            }
        }
        Ok(rows)
    }
    pub async fn event_history_page(
        &self,
        scope: &Scope,
        after: Watermark,
        target_rows: usize,
    ) -> Result<EventHistoryPage> {
        if target_rows == 0 || target_rows > MAX_PAGE {
            return Err(Error::Limit("history page rows"));
        }
        let source_watermark = self.current().watermark;
        if after > source_watermark {
            return Err(Error::Conflict("history cursor ahead of owner"));
        }
        let start = self.event_lower_bound(after, false).await?;
        let total = self.history_event_count().await?;
        let mut end = start;
        let mut rows = Vec::new();
        let mut bytes = 0usize;
        while end < total && rows.len() < target_rows {
            let watermark = self
                .history_event_at(end)
                .await?
                .ok_or(Error::Corruption("history ordinal missing"))?
                .watermark;
            let group_end = self.event_lower_bound(watermark, false).await?;
            if !rows.is_empty() && group_end - start > MAX_PAGE as u64 {
                break;
            }
            let group = self.event_range(end, group_end).await?;
            let group_bytes = group.iter().try_fold(0usize, |sum, row| {
                sum.checked_add(encoded_len(row)?)
                    .ok_or(Error::Limit("history page bytes"))
            })?;
            if !rows.is_empty() && group_bytes > MAX_COMMIT_BYTES.saturating_sub(bytes) {
                break;
            }
            bytes = bytes
                .checked_add(group_bytes)
                .ok_or(Error::Limit("history page bytes"))?;
            rows.extend(group);
            end = group_end;
        }
        let scanned = rows.len();
        let cutoff = rows.last().map(|row| row.watermark);
        Ok(EventHistoryPage {
            source_watermark,
            scanned,
            cutoff,
            has_more: end < total,
            events: rows
                .into_iter()
                .filter(|row| {
                    row.event.workspace == scope.workspace
                        && row.event.session == scope.session
                        && row.event.task.as_ref() == Some(&scope.task)
                })
                .collect(),
        })
    }
    pub async fn command_history_page(
        &self,
        workspace: &WorkspaceId,
        source_watermark: Watermark,
        after: Option<&CommandId>,
        limit: usize,
    ) -> Result<CommandHistoryPage> {
        if limit == 0 || limit > MAX_PAGE {
            return Err(Error::Limit("history page rows"));
        }
        if source_watermark != self.current().watermark {
            return Err(Error::Conflict(
                "history source changed; restart pagination",
            ));
        }
        let lower = after.map_or_else(
            || format!("{workspace}:"),
            |id| crate::contract::command_key(workspace, id),
        );
        let upper = format!("{workspace};");
        let mut cursor = lower;
        let mut receipts = Vec::new();
        let mut bytes = 0usize;
        let mut more = false;
        'pages: loop {
            let rows = self.history_commands(Some(&cursor), limit.min(64)).await?;
            if rows.is_empty() {
                break;
            }
            for (key, receipt) in rows {
                if key >= upper {
                    break 'pages;
                }
                if &receipt.workspace != workspace {
                    return Err(Error::Corruption("history receipt workspace"));
                }
                let size = encoded_len(&receipt)?;
                if receipts.len() == limit || size > MAX_COMMIT_BYTES - bytes {
                    more = true;
                    break 'pages;
                }
                bytes += size;
                cursor = key;
                receipts.push(receipt);
            }
        }
        let next = if more {
            receipts.last().map(|row| row.command.clone())
        } else {
            None
        };
        Ok(CommandHistoryPage {
            source_watermark,
            receipts,
            next,
        })
    }
}
