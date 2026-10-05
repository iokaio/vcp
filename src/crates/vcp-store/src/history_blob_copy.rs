// SPDX-License-Identifier: Apache-2.0
//! A private immutable destination deduplicates content hashes on disk. No
//! history-wide descriptor or seen-digest map is retained by this adapter.
use super::*;

pub(crate) struct CopyingPages<'a, S, D> {
    pub(crate) source: &'a mut S,
    pub(crate) destination: &'a mut D,
    pub(crate) check: &'a dyn Fn() -> Result<()>,
}
impl<S: Pages, D: Pages> Pages for CopyingPages<'_, S, D> {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        (self.check)()?;
        let bytes = self.source.read(digest, limit).await?;
        if bytes.len() > limit || digest_bytes(&bytes) != digest {
            return Err(Error::Corruption("archive copied object commitment"));
        }
        self.destination.write(digest, &bytes).await?;
        (self.check)()?;
        Ok(bytes)
    }
    async fn write(&mut self, _: &str, _: &[u8]) -> Result<()> {
        Err(Error::Access)
    }
}
impl Blob {
    /// Root and payload pages may be written tentatively, but success requires
    /// every ordered chunk and the final whole-stream length/hash to validate.
    pub(crate) async fn copy_objects(
        &self,
        source: &mut impl Pages,
        destination: &mut impl Pages,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        let mut pages = CopyingPages {
            source,
            destination,
            check,
        };
        let mut reader = Reader::new(self)?;
        while reader.next(&mut pages).await?.is_some() {}
        check()
    }
}
