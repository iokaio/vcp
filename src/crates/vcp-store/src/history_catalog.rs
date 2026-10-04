// SPDX-License-Identifier: Apache-2.0
//! Exact retained history under a root held by the semantically validated owner.
//! Publishing a decoded catalog never substitutes for mandatory cold replay.
use crate::{
    contract::{command_key, PreparedTransition, Receipt, State, MAX_COMMIT_BYTES},
    history_blob::{self, Blob},
    history_index::{Entry, Pages, Root, Table},
    Error, Result,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use vcp_domain::{CommandId, EventId, SessionId, TransactionId, Watermark, WorkspaceId};
use vcp_protocol::{canonical_bytes, command::CommandReceipt, event::EventEnvelope};

const PAGE_ROWS: usize = 4096;
#[path = "history_catalog_encoding.rs"]
mod encoding;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Catalog {
    version: u32,
    watermark: Watermark,
    events: Root,
    identities: Root,
    commands: Root,
    transactions: Root,
    groups: Root,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EventRow {
    id: EventId,
    blob: Blob,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Group {
    first: u64,
    count: u64,
}

impl Catalog {
    pub(crate) fn watermark(&self) -> Watermark {
        self.watermark
    }
    pub(crate) fn event_count(&self) -> u64 {
        self.events.count()
    }

    /// A migration/replay adapter, not permission to trust a persisted head.
    /// Retains the exact redacted envelopes/receipts present in this generation.
    pub(crate) async fn from_validated_state(
        pages: &mut impl Pages,
        state: &State,
    ) -> Result<Self> {
        state.validate()?;
        let mut result = Self {
            version: 3,
            watermark: state.watermark,
            events: Root::empty(Table::EventOrdinal),
            identities: Root::empty(Table::EventIdentity),
            commands: Root::empty(Table::Command),
            transactions: Root::empty(Table::Transaction),
            groups: Root::empty(Table::Commit),
        };
        result.append_events(pages, &state.events).await?;
        for (key, receipt) in state.commands.iter() {
            if key != &command_key(&receipt.workspace, &receipt.command) {
                return Err(Error::Corruption("history command identity"));
            }
            result.commands = insert_object(&result.commands, pages, key.clone(), receipt).await?;
        }
        for (id, receipt) in state.transactions.iter() {
            if id != &receipt.transaction {
                return Err(Error::Corruption("history transaction identity"));
            }
            result.transactions =
                insert_object(&result.transactions, pages, id.to_string(), receipt).await?;
        }
        Ok(result)
    }

    /// Transitional complete-State adapter. The catalog is returned separately
    /// so failed writes cannot change the root currently held by the owner.
    /// Publication must atomically bind this candidate to the original commit.
    pub(crate) async fn append_validated_commit(
        &self,
        pages: &mut impl Pages,
        prepared: &PreparedTransition,
    ) -> Result<Self> {
        self.validate()?;
        let next = prepared.state();
        let commit = prepared.commit();
        if prepared.source_watermark() != self.watermark
            || prepared.source_events() != self.events.count()
            || next.watermark != self.watermark.next()?
            || commit.receipt.watermark != next.watermark
            || next.transactions.get(&commit.transaction.id) != Some(&commit.receipt)
            || (next.events.len() as u64) < self.events.count()
        {
            return Err(Error::Corruption("history catalog append identity"));
        }
        let appended = &next.events[self.events.count() as usize..];
        if appended.len() != commit.transaction.events.len()
            || appended
                .iter()
                .zip(&commit.transaction.events)
                .any(|(event, input)| {
                    event.watermark != next.watermark
                        || &event.event != input
                        || event.redaction.is_some()
                })
        {
            return Err(Error::Corruption("history catalog append events"));
        }
        let mut result = self.clone();
        result.watermark = next.watermark;
        result.append_events(pages, appended).await?;
        if let Some(receipt) = &commit.receipt.command {
            let key = command_key(&receipt.workspace, &receipt.command);
            if next.commands.get(&key) != Some(receipt)
                || receipt.transaction != commit.transaction.id
                || receipt.watermark != next.watermark
            {
                return Err(Error::Corruption("history catalog append command"));
            }
            result.commands = insert_object(&result.commands, pages, key, receipt).await?;
        }
        result.transactions = insert_object(
            &result.transactions,
            pages,
            commit.transaction.id.to_string(),
            &commit.receipt,
        )
        .await?;
        Ok(result)
    }

    fn validate(&self) -> Result<()> {
        if self.version != 3 {
            return Err(Error::Incompatible);
        }
        self.events.validate_table(Table::EventOrdinal)?;
        self.identities.validate_table(Table::EventIdentity)?;
        self.commands.validate_table(Table::Command)?;
        self.transactions.validate_table(Table::Transaction)?;
        self.groups.validate_table(Table::Commit)?;
        if self.events.count() != self.identities.count() {
            return Err(Error::Corruption("history identity count"));
        }
        Ok(())
    }

    async fn append_events(
        &mut self,
        pages: &mut impl Pages,
        events: &[EventEnvelope],
    ) -> Result<()> {
        let mut start = 0;
        while start < events.len() {
            let watermark = events[start].watermark;
            let count = events[start..].partition_point(|event| event.watermark == watermark);
            let first = self.events.count();
            for event in &events[start..start + count] {
                let ordinal = self.events.count();
                let bytes = canonical_bytes(event)?;
                if bytes.len() > MAX_COMMIT_BYTES {
                    return Err(Error::Limit("history event row"));
                }
                let blob = history_blob::write_bytes(pages, &bytes).await?;
                self.events = self
                    .events
                    .insert(
                        pages,
                        entry(
                            ordinal_key(ordinal),
                            &EventRow {
                                id: event.event.id.clone(),
                                blob,
                            },
                        )?,
                    )
                    .await?;
                self.identities = self
                    .identities
                    .insert(pages, entry(event.event.id.to_string(), &ordinal)?)
                    .await?;
            }
            self.groups = self
                .groups
                .insert(
                    pages,
                    entry(
                        ordinal_key(watermark.get()),
                        &Group {
                            first,
                            count: count as u64,
                        },
                    )?,
                )
                .await?;
            start += count;
        }
        Ok(())
    }

    /// Strict global order, capped by both requested rows and one commit's
    /// encoded byte bound. A short nonempty page need not be end-of-history.
    /// Scope filtering follows this proof and cannot conceal sibling events.
    pub(crate) async fn event_page(
        &self,
        pages: &mut impl Pages,
        after: Option<u64>,
        limit: usize,
    ) -> Result<Vec<EventEnvelope>> {
        self.validate()?;
        if limit == 0 || limit > PAGE_ROWS {
            return Err(Error::Limit("history catalog page"));
        }
        let first = match after {
            Some(value) => value
                .checked_add(1)
                .ok_or(Error::Limit("history ordinal"))?,
            None => 0,
        };
        if first > self.events.count() {
            return Err(Error::Conflict("history ordinal ahead of owner"));
        }
        let key = after.map(ordinal_key);
        let entries = self.events.page(pages, key.as_deref(), limit).await?;
        let expected = (self.events.count() - first).min(limit as u64) as usize;
        if entries.len() != expected {
            return Err(Error::Corruption("history page incomplete"));
        }
        let mut result = Vec::with_capacity(expected);
        let mut bytes = 0;
        for (offset, item) in entries.into_iter().enumerate() {
            if item.key != ordinal_key(first + offset as u64) {
                return Err(Error::Corruption("history ordinal gap"));
            }
            let row: EventRow = serde_json::from_value(item.value)?;
            if !result.is_empty() && row.blob.bytes > (MAX_COMMIT_BYTES - bytes) as u64 {
                break;
            }
            let event: EventEnvelope = read_object(pages, &row.blob).await?;
            if event.event.id != row.id || event.watermark > self.watermark {
                return Err(Error::Corruption("history event identity"));
            }
            bytes += row.blob.bytes as usize;
            result.push(event);
        }
        Ok(result)
    }

    pub(crate) async fn event(
        &self,
        pages: &mut impl Pages,
        id: &EventId,
    ) -> Result<Option<EventEnvelope>> {
        self.validate()?;
        let Some(row) = self.identities.get(pages, id.as_str()).await? else {
            return Ok(None);
        };
        let ordinal: u64 = serde_json::from_value(row.value)?;
        let mut events = self.event_page(pages, ordinal.checked_sub(1), 1).await?;
        let event = events
            .pop()
            .ok_or(Error::Corruption("history event locator missing"))?;
        if &event.event.id != id {
            return Err(Error::Corruption("history event locator identity"));
        }
        Ok(Some(event))
    }

    pub(crate) async fn event_at(
        &self,
        pages: &mut impl Pages,
        ordinal: u64,
    ) -> Result<Option<EventEnvelope>> {
        self.validate()?;
        if ordinal >= self.events.count() {
            return Ok(None);
        }
        Ok(self
            .event_page(pages, ordinal.checked_sub(1), 1)
            .await?
            .pop())
    }

    pub(crate) async fn command(
        &self,
        pages: &mut impl Pages,
        workspace: &WorkspaceId,
        command: &CommandId,
        digest: &str,
    ) -> Result<Option<CommandReceipt>> {
        let receipt = self
            .command_unchecked_meaning(pages, workspace, command)
            .await?;
        match receipt {
            Some(receipt) if receipt.digest != digest => {
                Err(Error::Conflict("command ID reused with different meaning"))
            }
            value => Ok(value),
        }
    }
    pub(crate) async fn command_unchecked_meaning(
        &self,
        pages: &mut impl Pages,
        workspace: &WorkspaceId,
        command: &CommandId,
    ) -> Result<Option<CommandReceipt>> {
        self.validate()?;
        let Some(item) = self
            .commands
            .get(pages, &command_key(workspace, command))
            .await?
        else {
            return Ok(None);
        };
        let receipt: CommandReceipt =
            read_object(pages, &serde_json::from_value(item.value)?).await?;
        if &receipt.workspace != workspace
            || &receipt.command != command
            || receipt.watermark > self.watermark
        {
            return Err(Error::Corruption("history command locator identity"));
        }
        Ok(Some(receipt))
    }

    pub(crate) async fn scoped_command(
        &self,
        pages: &mut impl Pages,
        workspace: &WorkspaceId,
        session: &SessionId,
        command: &CommandId,
    ) -> Result<Option<CommandReceipt>> {
        let Some(receipt) = self
            .command_unchecked_meaning(pages, workspace, command)
            .await?
        else {
            return Ok(None);
        };
        let Some(item) = self
            .groups
            .get(pages, &ordinal_key(receipt.watermark.get()))
            .await?
        else {
            return Ok(None);
        };
        let group: Group = serde_json::from_value(item.value)?;
        let end = group
            .first
            .checked_add(group.count)
            .ok_or(Error::Corruption("history group overflow"))?;
        if group.count == 0 || end > self.events.count() {
            return Err(Error::Corruption("history group extent"));
        }
        let mut at = group.first;
        let mut found = false;
        while at < end {
            let rows = self
                .event_page(
                    pages,
                    at.checked_sub(1),
                    (end - at).min(PAGE_ROWS as u64) as usize,
                )
                .await?;
            for event in &rows {
                if event.watermark != receipt.watermark {
                    return Err(Error::Corruption("history group watermark"));
                }
                if &event.event.correlation == command {
                    if &event.event.workspace != workspace || &event.event.session != session {
                        return Err(Error::Access);
                    }
                    found = true;
                }
            }
            at += rows.len() as u64;
        }
        Ok(found.then_some(receipt))
    }

    pub(crate) async fn transaction(
        &self,
        pages: &mut impl Pages,
        id: &TransactionId,
    ) -> Result<Option<Receipt>> {
        self.validate()?;
        let Some(item) = self.transactions.get(pages, id.as_str()).await? else {
            return Ok(None);
        };
        let receipt: Receipt = read_object(pages, &serde_json::from_value(item.value)?).await?;
        if &receipt.transaction != id || receipt.watermark > self.watermark {
            return Err(Error::Corruption("history transaction locator identity"));
        }
        Ok(Some(receipt))
    }
}

#[path = "history_catalog_verify.rs"]
mod verify;

fn ordinal_key(value: u64) -> String {
    format!("{value:016x}")
}
fn entry(key: String, value: &impl Serialize) -> Result<Entry> {
    Ok(Entry {
        key,
        value: serde_json::to_value(value)?,
    })
}
async fn insert_object(
    root: &Root,
    pages: &mut impl Pages,
    key: String,
    value: &impl Serialize,
) -> Result<Root> {
    let bytes = canonical_bytes(value)?;
    if bytes.len() > MAX_COMMIT_BYTES {
        return Err(Error::Limit("history row"));
    }
    let blob = history_blob::write_bytes(pages, &bytes).await?;
    root.insert(pages, entry(key, &blob)?).await
}
async fn read_object<T: DeserializeOwned + Serialize>(
    pages: &mut impl Pages,
    blob: &Blob,
) -> Result<T> {
    let bytes = history_blob::read_bounded(pages, blob, MAX_COMMIT_BYTES).await?;
    let row: T = serde_json::from_slice(&bytes)?;
    if canonical_bytes(&row)? != bytes {
        return Err(Error::Corruption("history row encoding"));
    }
    Ok(row)
}

#[cfg(test)]
#[path = "history_catalog_tests.rs"]
mod tests;
