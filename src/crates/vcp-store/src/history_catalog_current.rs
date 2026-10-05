// SPDX-License-Identifier: Apache-2.0
//! Qualified current preparation supplies only new rows. No archival State is
//! built and no durable object becomes authoritative before owner publication.
use super::*;
use crate::{admitted_history::AdmittedCut, contract::current_transition::PreparedCurrent};

impl Catalog {
    pub(crate) async fn append_current(
        &self,
        pages: &mut impl Pages,
        source: &AdmittedCut,
        prepared: &PreparedCurrent,
    ) -> Result<Self> {
        if prepared.source_identity() != source.identity()
            || canonical_bytes(self)? != canonical_bytes(source.catalog())?
        {
            return Err(Error::Conflict("prepared history source differs"));
        }
        let proposed = prepared.proposed();
        let commit = prepared.commit();
        if proposed.current.watermark != self.watermark.next()?
            || commit.receipt != proposed.receipt
            || commit.receipt.watermark != proposed.current.watermark
            || commit.transaction.expected_watermark != self.watermark
            || commit.transaction.id != proposed.receipt.transaction
            || proposed.events.len() != commit.transaction.events.len()
            || proposed
                .events
                .iter()
                .zip(&commit.transaction.events)
                .any(|(row, input)| {
                    row.version != 1
                        || row.watermark != proposed.current.watermark
                        || &row.event != input
                        || row.redaction.is_some()
                })
        {
            return Err(Error::Corruption("current history append identity"));
        }
        let mut result = self.clone();
        result.watermark = proposed.current.watermark;
        result.append_events(pages, &proposed.events).await?;
        if let Some(receipt) = &commit.receipt.command {
            result.commands = insert_object(
                &result.commands,
                pages,
                command_key(&receipt.workspace, &receipt.command),
                receipt,
            )
            .await?;
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
}
