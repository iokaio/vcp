// SPDX-License-Identifier: Apache-2.0
//! Exact owner-local byte accounting, rebuilt rather than trusted on load.
use super::{encoded_len, State};
use crate::{Error, Result};
use serde::Serialize;
use std::collections::BTreeSet;
use vcp_domain::Watermark;

#[derive(Clone, Copy, Debug)]
pub(crate) struct StateSize {
    watermark: Watermark,
    bytes: usize,
}
impl StateSize {
    /// Exact archival-size accounting from a typed proposal. It never builds
    /// an incomplete State or materializes retained historical payloads.
    pub(crate) fn next_proposal(
        self,
        prior: &State,
        next: &super::current_preparation::ProposedTransition,
    ) -> Result<Self> {
        if self.watermark != prior.watermark {
            return Err(Error::Corruption("state size watermark"));
        }
        let mut removed = encoded_len(&prior.watermark)?;
        let mut added = encoded_len(&next.current.watermark)?;
        for key in &next.touched {
            if let Some(value) = prior.records.get(key) {
                removed += entry_len(key, value)?;
            }
            if let Some(value) = next.current.records.get(key) {
                added += entry_len(key, value)?;
            }
        }
        removed += commas(prior.records.len());
        added += commas(next.current.records.len());
        for event in &next.events {
            added += encoded_len(event)?;
        }
        removed += commas(prior.events.len());
        added += commas(prior.events.len() + next.events.len());
        if let Some(command) = &next.receipt.command {
            added += entry_len(
                &super::command_key(&command.workspace, &command.command),
                command,
            )?;
        }
        removed += commas(prior.commands.len());
        added += commas(prior.commands.len() + usize::from(next.receipt.command.is_some()));
        added += entry_len(&next.receipt.transaction, &next.receipt)?;
        removed += commas(prior.transactions.len());
        added += commas(prior.transactions.len() + 1);
        let sessions = next
            .events
            .iter()
            .map(|event| &event.event.session)
            .collect::<BTreeSet<_>>();
        for session in sessions {
            if let Some(sequence) = prior.sequences.get(session) {
                removed += entry_len(session, sequence)?;
            }
            let sequence = next
                .current
                .sequences
                .get(session)
                .ok_or(Error::Corruption("state size session"))?;
            added += entry_len(session, sequence)?;
        }
        removed += commas(prior.sequences.len());
        added += commas(next.current.sequences.len());
        let bytes = self
            .bytes
            .checked_sub(removed)
            .and_then(|bytes| bytes.checked_add(added))
            .ok_or(Error::Corruption("state size arithmetic"))?;
        Ok(Self {
            watermark: next.current.watermark,
            bytes,
        })
    }

    pub(crate) fn measure(state: &State) -> Result<Self> {
        Ok(Self {
            watermark: state.watermark,
            bytes: encoded_len(state)?,
        })
    }
    pub(crate) fn bytes(self) -> usize {
        self.bytes
    }

    /// `next` holds only newly appended events and receipts. Records and
    /// sequences are complete candidate maps; preparation supplies `touched`.
    #[cfg(test)]
    pub(super) fn next(
        self,
        prior: &State,
        next: &State,
        touched: &BTreeSet<String>,
    ) -> Result<Self> {
        if self.watermark != prior.watermark {
            return Err(Error::Corruption("state size watermark"));
        }
        let mut removed = encoded_len(&prior.watermark)?;
        let mut added = encoded_len(&next.watermark)?;
        for key in touched {
            if let Some(value) = prior.records.get(key) {
                removed += entry_len(key, value)?;
            }
            if let Some(value) = next.records.get(key) {
                added += entry_len(key, value)?;
            }
        }
        removed += commas(prior.records.len());
        added += commas(next.records.len());
        for event in &next.events {
            added += encoded_len(event)?;
        }
        removed += commas(prior.events.len());
        added += commas(prior.events.len() + next.events.len());
        for (key, value) in &next.commands {
            added += entry_len(key, value)?;
        }
        removed += commas(prior.commands.len());
        added += commas(prior.commands.len() + next.commands.len());
        for (key, value) in &next.transactions {
            added += entry_len(key, value)?;
        }
        removed += commas(prior.transactions.len());
        added += commas(prior.transactions.len() + next.transactions.len());
        let sessions = next
            .events
            .iter()
            .map(|event| &event.event.session)
            .collect::<BTreeSet<_>>();
        for session in sessions {
            if let Some(sequence) = prior.sequences.get(session) {
                removed += entry_len(session, sequence)?;
            }
            let sequence = next
                .sequences
                .get(session)
                .ok_or(Error::Corruption("state size session"))?;
            added += entry_len(session, sequence)?;
        }
        removed += commas(prior.sequences.len());
        added += commas(next.sequences.len());
        let bytes = self
            .bytes
            .checked_sub(removed)
            .and_then(|n| n.checked_add(added))
            .ok_or(Error::Corruption("state size arithmetic"))?;
        Ok(Self {
            watermark: next.watermark,
            bytes,
        })
    }
}
fn commas(length: usize) -> usize {
    length.saturating_sub(1)
}
fn entry_len(key: &impl Serialize, value: &impl Serialize) -> Result<usize> {
    // State map keys serialize as JSON strings. Colons are one byte; braces
    // and struct field names remain unchanged between states.
    Ok(encoded_len(key)? + 1 + encoded_len(value)?)
}
