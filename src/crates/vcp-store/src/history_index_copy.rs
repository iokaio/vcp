// SPDX-License-Identifier: Apache-2.0
//! Copy only pages reachable through an admitted root, checking the same parent
//! commitments as ordinary queries. Destination remains private until its owner
//! completes semantic validation and publishes a separate root capability.
use super::*;

impl Root {
    pub(crate) async fn copy_pages(
        &self,
        source: &mut impl Pages,
        destination: &mut impl Pages,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        let mut pending: Vec<Link> = self.head.iter().cloned().collect();
        while let Some(link) = pending.pop() {
            check()?;
            let page = self.load(source, &link).await?;
            // load proves these canonical bytes are identical to the source,
            // including the parent's type/range/count/height commitment.
            let bytes = canonical_bytes(&page)?;
            destination.write(&link.digest, &bytes).await?;
            if let Body::Branch(children) = page.body {
                pending.extend(children.into_iter().rev());
            }
            // Height decreases at each authenticated edge. This stack bound is
            // independent of total historical rows or persisted old roots.
            if pending.len() > FANOUT * (usize::from(MAX_HEIGHT) + 1) {
                return Err(Error::Corruption("archive index traversal depth"));
            }
        }
        check()
    }
}
