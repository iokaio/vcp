// SPDX-License-Identifier: Apache-2.0
//! Full comparison against mandatory semantic replay; persisted roots alone
//! never provide permission to bypass earlier transition validation.
use super::*;

impl Catalog {
    /// The caller must supply the result of full canonical semantic replay.
    /// This proves all persisted catalog rows and lookup/group identities match
    /// that result. It does not establish the history's semantics by itself.
    pub(crate) async fn verify_replayed_state(
        &self,
        pages: &mut impl Pages,
        replayed: &State,
    ) -> Result<()> {
        self.validate()?;
        if self.watermark != replayed.watermark
            || self.events.count() != replayed.events.len() as u64
            || self.commands.count() != replayed.commands.len() as u64
            || self.transactions.count() != replayed.transactions.len() as u64
        {
            return Err(Error::Corruption("history catalog replay counts"));
        }
        let mut ordinal = 0u64;
        let mut artifact_references = 0u64;
        while ordinal < self.events.count() {
            let rows = self
                .event_page(pages, ordinal.checked_sub(1), PAGE_ROWS)
                .await?;
            if rows.is_empty() {
                return Err(Error::Corruption("history catalog replay incomplete"));
            }
            for row in rows {
                let expected = &replayed.events[ordinal as usize];
                if &row != expected {
                    return Err(Error::Corruption("history catalog replay event"));
                }
                let id = self
                    .identities
                    .get(pages, row.event.id.as_str())
                    .await?
                    .ok_or(Error::Corruption("history catalog replay identity missing"))?;
                if serde_json::from_value::<u64>(id.value)? != ordinal {
                    return Err(Error::Corruption("history catalog replay ordinal"));
                }
                artifact_references = artifact_references
                    .checked_add(
                        self.verify_artifact_references(pages, &row, ordinal)
                            .await?,
                    )
                    .ok_or(Error::Limit("history artifact references"))?;
                ordinal += 1;
            }
        }
        if self.artifacts.count() != artifact_references {
            return Err(Error::Corruption("history artifact reference count"));
        }
        let mut first = 0usize;
        let mut groups = 0u64;
        while first < replayed.events.len() {
            let watermark = replayed.events[first].watermark;
            let count =
                replayed.events[first..].partition_point(|event| event.watermark == watermark);
            let entry = self
                .groups
                .get(pages, &ordinal_key(watermark.get()))
                .await?
                .ok_or(Error::Corruption("history catalog replay group missing"))?;
            let group: Group = serde_json::from_value(entry.value)?;
            if group.first != first as u64 || group.count != count as u64 {
                return Err(Error::Corruption("history catalog replay group extent"));
            }
            groups += 1;
            first += count;
        }
        if self.groups.count() != groups {
            return Err(Error::Corruption("history catalog replay group count"));
        }
        for (key, receipt) in replayed.commands.iter() {
            if key != &command_key(&receipt.workspace, &receipt.command)
                || self
                    .command_unchecked_meaning(pages, &receipt.workspace, &receipt.command)
                    .await?
                    .as_ref()
                    != Some(receipt)
            {
                return Err(Error::Corruption("history catalog replay command"));
            }
        }
        for (id, receipt) in replayed.transactions.iter() {
            if self.transaction(pages, id).await?.as_ref() != Some(receipt) {
                return Err(Error::Corruption("history catalog replay transaction"));
            }
        }
        Ok(())
    }
}
