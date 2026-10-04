// SPDX-License-Identifier: Apache-2.0
//! Bounded neutral-history/2 object packing. This transports data only: root
//! closure, full semantic replay and destination authority remain mandatory.
//! All Pages and output arguments must belong to private pinned staging.
use crate::{
    history_index::{Entry, Pages, Root, Table},
    vault_crypto::{stream::RootDescriptor, Object},
    Error, Result,
};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use vcp_protocol::digest_bytes;

const MAGIC: &[u8; 8] = b"VCPNEU02";
const MAX_OBJECT: usize = 128 * 1024;
pub(crate) const FORMAT: &str = "vcp-neutral-history/2";

/// Only a fixed authenticated staging-index root is resident. Index pages are
/// written directly to backing, never recursively included in their own set.
pub(crate) struct IndexedPages<'a, P> {
    backing: &'a mut P,
    root: Root,
    failed: bool,
}
impl<'a, P: Pages> IndexedPages<'a, P> {
    pub(crate) fn new(backing: &'a mut P) -> Self {
        Self {
            backing,
            root: Root::empty(Table::ArchiveObjects),
            failed: false,
        }
    }
    pub(crate) fn finish(self) -> Result<ObjectSet> {
        if self.failed {
            return Err(Error::Unavailable("archive object staging failed"));
        }
        Ok(ObjectSet { root: self.root })
    }
}
impl<P: Pages> Pages for IndexedPages<'_, P> {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        if self.failed {
            return Err(Error::Unavailable("archive object staging failed"));
        }
        self.failed = true;
        let bytes = self.backing.read(digest, limit).await?;
        if bytes.len() > limit || digest_bytes(&bytes) != digest {
            return Err(Error::Corruption("archive staged read integrity"));
        }
        self.failed = false;
        Ok(bytes)
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        if self.failed {
            return Err(Error::Unavailable("archive object staging failed"));
        }
        self.failed = true;
        if bytes.is_empty() || bytes.len() > MAX_OBJECT || digest_bytes(bytes) != digest {
            return Err(Error::Corruption("archive object shape or commitment"));
        }
        self.backing.write(digest, bytes).await?;
        self.root = self
            .root
            .insert(
                self.backing,
                Entry {
                    key: digest.to_owned(),
                    value: serde_json::to_value(Object {
                        bytes: bytes.len() as u64,
                        sha256: digest.to_owned(),
                    })?,
                },
            )
            .await?;
        self.failed = false;
        Ok(())
    }
}
pub(crate) struct ObjectSet {
    root: Root,
}
impl ObjectSet {
    pub(crate) fn count(&self) -> u64 {
        self.root.count()
    }
}

struct Output<'a, W> {
    inner: &'a mut W,
    digest: Sha256,
    bytes: u64,
    limit: u64,
}
impl<W: Write> Output<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len() as u64)
            .filter(|count| *count <= self.limit)
            .ok_or(Error::Limit("neutral wire bytes"))?;
        self.inner.write_all(bytes)?;
        self.digest.update(bytes);
        Ok(())
    }
}
/// Private output may contain a tentative prefix on failure. Only successful
/// return supplies the complete payload commitment accepted by signed-age/2.
pub(crate) async fn write(
    pages: &mut impl Pages,
    set: &ObjectSet,
    root: &[u8],
    output: &mut impl Write,
    limit: u64,
    check: &dyn Fn() -> Result<()>,
) -> Result<(RootDescriptor, Object)> {
    check()?;
    if root.is_empty() || root.len() > MAX_OBJECT {
        return Err(Error::Limit("neutral root descriptor"));
    }
    set.root.validate_table(Table::ArchiveObjects)?;
    let mut output = Output {
        inner: output,
        digest: Sha256::new(),
        bytes: 0,
        limit,
    };
    output.write(MAGIC)?;
    output.write(&(root.len() as u32).to_be_bytes())?;
    output.write(root)?;
    output.write(&set.count().to_be_bytes())?;
    let mut after = None;
    let mut count = 0u64;
    loop {
        check()?;
        let entries = set.root.page(pages, after.as_deref(), 64).await?;
        if entries.is_empty() {
            break;
        }
        for entry in entries {
            check()?;
            let descriptor: Object = serde_json::from_value(entry.value)?;
            if descriptor.sha256 != entry.key
                || descriptor.bytes == 0
                || descriptor.bytes > MAX_OBJECT as u64
            {
                return Err(Error::Corruption("neutral object descriptor"));
            }
            let bytes = pages.read(&entry.key, MAX_OBJECT).await?;
            if bytes.len() as u64 != descriptor.bytes || digest_bytes(&bytes) != entry.key {
                return Err(Error::Corruption("neutral object content"));
            }
            // Lowercase SHA-256 names are fixed width and have no path meaning.
            if !hash(&entry.key) {
                return Err(Error::Corruption("neutral object key"));
            }
            output.write(entry.key.as_bytes())?;
            output.write(&(bytes.len() as u32).to_be_bytes())?;
            output.write(&bytes)?;
            after = Some(entry.key);
            count = count
                .checked_add(1)
                .ok_or(Error::Limit("neutral object count"))?;
        }
    }
    if count != set.count() {
        return Err(Error::Corruption("neutral object count differs"));
    }
    output.write(&count.to_be_bytes())?;
    check()?;
    Ok((
        RootDescriptor {
            format: FORMAT.into(),
            sha256: digest_bytes(root),
            descriptor_bytes: root.len() as u64,
        },
        Object {
            bytes: output.bytes,
            sha256: format!("{:x}", output.digest.finalize()),
        },
    ))
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
struct Input<R> {
    inner: R,
    bytes: u64,
    digest: Sha256,
    expected: Object,
}
impl<R: Read> Input<R> {
    fn bytes(&mut self, output: &mut [u8]) -> Result<()> {
        self.bytes = self
            .bytes
            .checked_add(output.len() as u64)
            .filter(|bytes| *bytes <= self.expected.bytes)
            .ok_or(Error::Corruption("neutral wire exceeds commitment"))?;
        self.inner.read_exact(output)?;
        self.digest.update(output);
        Ok(())
    }
    fn fixed<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut value = [0; N];
        self.bytes(&mut value)?;
        Ok(value)
    }
    fn finish(mut self) -> Result<()> {
        if self.inner.read(&mut [0; 1])? != 0
            || self.bytes != self.expected.bytes
            || format!("{:x}", self.digest.finalize()) != self.expected.sha256
        {
            return Err(Error::Corruption("neutral wire terminal commitment"));
        }
        Ok(())
    }
}
/// Integrity-qualified data, deliberately not a semantically admitted archive.
/// A validated writer signature cannot replace replay of every retained commit.
pub(crate) struct UntrustedObjects {
    pub(crate) root: Vec<u8>,
    pub(crate) objects: ObjectSet,
}
pub(crate) async fn read(
    input: impl Read,
    expected_root: &RootDescriptor,
    expected_payload: &Object,
    destination: &mut impl Pages,
    check: &dyn Fn() -> Result<()>,
) -> Result<UntrustedObjects> {
    check()?;
    if expected_root.format != FORMAT {
        return Err(Error::Incompatible);
    }
    if expected_root.descriptor_bytes == 0
        || expected_root.descriptor_bytes > MAX_OBJECT as u64
        || !hash(&expected_root.sha256)
        || !hash(&expected_payload.sha256)
    {
        return Err(Error::Corruption("neutral wire commitments"));
    }
    let mut input = Input {
        inner: input,
        bytes: 0,
        digest: Sha256::new(),
        expected: expected_payload.clone(),
    };
    if &input.fixed::<8>()? != MAGIC {
        return Err(Error::Incompatible);
    }
    let root_length = u32::from_be_bytes(input.fixed()?) as usize;
    if root_length as u64 != expected_root.descriptor_bytes {
        return Err(Error::Corruption("neutral root length"));
    }
    let mut root = vec![0; root_length];
    input.bytes(&mut root)?;
    if digest_bytes(&root) != expected_root.sha256 {
        return Err(Error::Corruption("neutral root commitment"));
    }
    let count = u64::from_be_bytes(input.fixed()?);
    let mut destination = IndexedPages::new(destination);
    let mut previous: Option<String> = None;
    let mut buffer = vec![0; MAX_OBJECT];
    for _ in 0..count {
        check()?;
        let key = String::from_utf8(input.fixed::<64>()?.to_vec())
            .map_err(|_| Error::Corruption("neutral object key"))?;
        if !hash(&key) || previous.as_ref().is_some_and(|old| old >= &key) {
            return Err(Error::Corruption("neutral object order or duplicate"));
        }
        let length = u32::from_be_bytes(input.fixed()?) as usize;
        if length == 0 || length > MAX_OBJECT {
            return Err(Error::Corruption("neutral object length"));
        }
        input.bytes(&mut buffer[..length])?;
        destination.write(&key, &buffer[..length]).await?;
        previous = Some(key);
    }
    if u64::from_be_bytes(input.fixed()?) != count {
        return Err(Error::Corruption("neutral wire object footer"));
    }
    input.finish()?;
    check()?;
    let objects = destination.finish()?;
    if objects.count() != count {
        return Err(Error::Corruption("neutral staged object count"));
    }
    Ok(UntrustedObjects { root, objects })
}

#[cfg(test)]
#[path = "portable_snapshot_wire_tests.rs"]
mod tests;
