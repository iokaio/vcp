// SPDX-License-Identifier: Apache-2.0
//! Compare every logical catalog row against an independently semantically
//! replayed catalog. B-tree insertion shape is not a canonical history fact.
use super::*;

enum Values {
    Event,
    Blob,
    Metadata,
}
impl Catalog {
    pub(crate) async fn verify_workspace(
        &self,
        pages: &mut impl Pages,
        workspace: &WorkspaceId,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        let mut ordinal = 0u64;
        while ordinal < self.event_count() {
            check()?;
            let rows = self.event_page(pages, ordinal.checked_sub(1), 64).await?;
            if rows.is_empty() {
                return Err(Error::Corruption("archive scoped history incomplete"));
            }
            for row in rows {
                if &row.event.workspace != workspace {
                    return Err(Error::Access);
                }
                ordinal += 1;
            }
        }
        let mut after = None;
        loop {
            check()?;
            let rows = self.commands.page(pages, after.as_deref(), 64).await?;
            if rows.is_empty() {
                break;
            }
            for row in rows {
                let receipt: CommandReceipt =
                    read_object(pages, &serde_json::from_value(row.value)?).await?;
                if &receipt.workspace != workspace {
                    return Err(Error::Access);
                }
                after = Some(row.key);
            }
        }
        check()
    }
    /// `replayed` must come from complete replay of every retained original
    /// transition. This proves representation equivalence, never replay by itself.
    pub(crate) async fn verify_replayed_catalog(
        &self,
        source: &mut impl Pages,
        replayed: &Catalog,
        replay_pages: &mut impl Pages,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        replayed.validate()?;
        if self.watermark != replayed.watermark {
            return Err(Error::Corruption("archive replay catalog watermark"));
        }
        for (actual, expected, values) in [
            (&self.events, &replayed.events, Values::Event),
            (&self.identities, &replayed.identities, Values::Metadata),
            (&self.artifacts, &replayed.artifacts, Values::Metadata),
            (&self.commands, &replayed.commands, Values::Blob),
            (&self.transactions, &replayed.transactions, Values::Blob),
            (&self.groups, &replayed.groups, Values::Metadata),
        ] {
            if actual.count() != expected.count() {
                return Err(Error::Corruption("archive replay catalog count"));
            }
            let mut after = None;
            let mut count = 0u64;
            loop {
                check()?;
                let left = actual.page(source, after.as_deref(), 64).await?;
                let right = expected.page(replay_pages, after.as_deref(), 64).await?;
                if left.len() != right.len() {
                    return Err(Error::Corruption("archive replay catalog page"));
                }
                if left.is_empty() {
                    break;
                }
                for (left, right) in left.into_iter().zip(right) {
                    check()?;
                    if left.key != right.key {
                        return Err(Error::Corruption("archive replay catalog key"));
                    }
                    match values {
                        Values::Metadata => {
                            if left.value != right.value {
                                return Err(Error::Corruption("archive replay catalog metadata"));
                            }
                        }
                        Values::Event => {
                            let actual: EventRow = serde_json::from_value(left.value)?;
                            let expected: EventRow = serde_json::from_value(right.value)?;
                            if actual.id != expected.id {
                                return Err(Error::Corruption("archive replay event identity"));
                            }
                            verify_blob(source, &actual.blob, &expected.blob, check).await?;
                        }
                        Values::Blob => {
                            verify_blob(
                                source,
                                &serde_json::from_value(left.value)?,
                                &serde_json::from_value(right.value)?,
                                check,
                            )
                            .await?;
                        }
                    }
                    after = Some(left.key);
                    count = count
                        .checked_add(1)
                        .ok_or(Error::Limit("archive replay catalog count"))?;
                }
            }
            if count != actual.count() {
                return Err(Error::Corruption("archive replay catalog completeness"));
            }
        }
        check()
    }
}
async fn verify_blob(
    source: &mut impl Pages,
    actual: &Blob,
    expected: &Blob,
    check: &dyn Fn() -> Result<()>,
) -> Result<()> {
    actual.validate()?;
    expected.validate()?;
    if actual.bytes != expected.bytes
        || actual.sha256 != expected.sha256
        || actual.bytes > MAX_COMMIT_BYTES as u64
    {
        return Err(Error::Corruption("archive replay row commitment"));
    }
    let mut reader = history_blob::Reader::new(actual)?;
    loop {
        check()?;
        if reader.next(source).await?.is_none() {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "history_catalog_equivalence_tests.rs"]
mod tests;
