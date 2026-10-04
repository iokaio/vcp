// SPDX-License-Identifier: Apache-2.0
//! Reachability transfer of an already admitted catalog. This copies its exact
//! indexed rows; it cannot manufacture original commit bodies or replay proof.
use super::*;

impl Catalog {
    pub(crate) async fn copy_objects(
        &self,
        source: &mut impl Pages,
        destination: &mut impl Pages,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        for root in [
            &self.events,
            &self.identities,
            &self.commands,
            &self.transactions,
            &self.groups,
        ] {
            root.copy_pages(source, destination, check).await?;
        }
        for (root, event_rows) in [
            (&self.events, true),
            (&self.commands, false),
            (&self.transactions, false),
        ] {
            let mut after = None;
            let mut count = 0u64;
            loop {
                check()?;
                let rows = root.page(source, after.as_deref(), 64).await?;
                if rows.is_empty() {
                    break;
                }
                for row in rows {
                    let blob = if event_rows {
                        let row: EventRow = serde_json::from_value(row.value.clone())?;
                        row.blob
                    } else {
                        serde_json::from_value(row.value.clone())?
                    };
                    blob.copy_objects(source, destination, check).await?;
                    after = Some(row.key);
                    count = count
                        .checked_add(1)
                        .ok_or(Error::Limit("archive catalog rows"))?;
                }
            }
            if count != root.count() {
                return Err(Error::Corruption("archive catalog row count"));
            }
        }
        check()
    }
}

#[cfg(test)]
#[path = "history_copy_tests.rs"]
mod tests;
