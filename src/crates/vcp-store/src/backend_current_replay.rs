// SPDX-License-Identifier: Apache-2.0
//! Full semantic replay over native authenticated history pages. Deterministic
//! candidate page writes are compared with already-published bytes, never used
//! to repair missing or corrupt objects during cold-open verification.
use super::*;
use crate::{contract::current_transition, history_index::Pages, journal_frame::Frame};

struct ComparePages<'a, P> {
    source: &'a mut P,
}
impl<P: Pages> Pages for ComparePages<'_, P> {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        self.source.read(digest, limit).await
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        let original = self.source.read(digest, bytes.len()).await?;
        if original != bytes {
            return Err(Error::Corruption("replayed history object differs"));
        }
        Ok(())
    }
}
pub(crate) async fn replay_current(
    pages: &mut impl Pages,
    source: &AdmittedCut,
    frame: &Frame,
) -> Result<AdmittedCut> {
    let publication = frame
        .publication
        .as_ref()
        .ok_or(Error::Corruption("current frame publication missing"))?;
    let commit: Commit = serde_json::from_slice(&frame.payload)?;
    if commit.version != FORMAT_VERSION {
        return Err(Error::Incompatible);
    }
    if commit.receipt.watermark != source.current().watermark.next()? {
        return Err(Error::Corruption("replayed current watermark"));
    }
    let mut pages = ComparePages { source: pages };
    let current_transition::Outcome::Prepared(prepared) =
        current_transition::prepare(&mut pages, source, &commit.transaction).await?
    else {
        return Err(Error::Corruption("duplicate current replay transaction"));
    };
    if prepared.commit() != &commit {
        return Err(Error::Corruption("replayed receipt differs"));
    }
    let next = source.advance(&mut pages, &prepared).await?;
    crate::history_publication::verify(
        &mut pages,
        publication,
        source,
        &prepared,
        &next,
        &frame.payload,
    )
    .await?;
    Ok(next)
}
