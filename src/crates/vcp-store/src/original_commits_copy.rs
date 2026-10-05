// SPDX-License-Identifier: Apache-2.0
//! Preserve exact retained replay bytes and unavailable-prefix evidence. A copy
//! is never semantic replay or permission to publish a canonical owner.
use super::*;

impl OriginalCommits {
    pub(crate) async fn copy_objects(
        &self,
        source: &mut impl Pages,
        destination: &mut impl Pages,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        if let Some(base) = self.base_blob() {
            base.copy_objects(source, destination, check).await?;
        }
        self.root().copy_pages(source, destination, check).await?;
        let mut pages = history_blob::copy::CopyingPages {
            source,
            destination,
            check,
        };
        let mut watermark = self.base_watermark();
        while watermark < self.watermark() {
            check()?;
            watermark = watermark.next()?;
            // get checks every exact ordinal, row transaction identity and the
            // existing per-commit byte limit, preserving accepted JSON spelling.
            // The adapter copies only the immutable pages read by that proof.
            self.get(&mut pages, watermark)
                .await?
                .ok_or(Error::Corruption("archive original commit missing"))?;
        }
        check()
    }
}

#[cfg(test)]
#[path = "original_commits_copy_tests.rs"]
mod tests;
