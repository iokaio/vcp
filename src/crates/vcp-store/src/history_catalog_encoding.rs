// SPDX-License-Identifier: Apache-2.0
//! Exact legacy commitment from current records and an owner-validated catalog.
//! No complete State is manufactured. Row bytes retain their original canonical
//! encoding; the caller binds both inputs to the same admitted generation.
use super::*;
use crate::{
    history_blob::{Reader, Writer},
    CurrentStateView,
};
use sha2::{Digest, Sha256};
#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod common;

trait Sink {
    async fn write(&mut self, bytes: &[u8]) -> Result<()>;
}
struct HashSink(Sha256);
struct BoundedSink {
    bytes: Vec<u8>,
    limit: usize,
}
impl Sink for BoundedSink {
    async fn write(&mut self, bytes: &[u8]) -> Result<()> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|length| length > self.limit)
        {
            return Err(Error::Limit("explicit archival State bytes"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
}
impl Sink for HashSink {
    async fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.0.update(bytes);
        Ok(())
    }
}
struct BlobSink<'a, P> {
    pages: &'a mut P,
    writer: Writer,
}
impl<P: Pages> Sink for BlobSink<'_, P> {
    async fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.writer.push(self.pages, bytes).await
    }
}

impl Catalog {
    /// Explicit legacy archival materialization only. Live current/history
    /// consumers must use authenticated bounded reads. Exceeding this legacy
    /// DTO bound requires the streaming neutral archive, not a partial State.
    pub(crate) async fn legacy_bytes(
        &self,
        pages: &mut impl Pages,
        current: CurrentStateView<'_>,
        limit: usize,
    ) -> Result<Vec<u8>> {
        if limit == 0 || limit > crate::contract::MAX_STATE_BYTES {
            return Err(Error::Limit("explicit archival State bytes"));
        }
        let mut sink = BoundedSink {
            bytes: Vec::new(),
            limit,
        };
        self.write_legacy(pages, current, &mut sink).await?;
        Ok(sink.bytes)
    }
    pub(crate) async fn legacy_digest(
        &self,
        pages: &mut impl Pages,
        current: CurrentStateView<'_>,
    ) -> Result<String> {
        let mut sink = HashSink(Sha256::new());
        self.write_legacy(pages, current, &mut sink).await?;
        Ok(format!("{:x}", sink.0.finalize()))
    }

    /// Export bytes to a separate private object destination. Only a successful
    /// return yields the complete descriptor; interrupted staging grants nothing.
    pub(crate) async fn legacy_blob(
        &self,
        source: &mut impl Pages,
        current: CurrentStateView<'_>,
        destination: &mut impl Pages,
    ) -> Result<Blob> {
        let mut sink = BlobSink {
            pages: destination,
            writer: Writer::new(),
        };
        self.write_legacy(source, current, &mut sink).await?;
        sink.writer.finish(sink.pages).await
    }

    async fn write_legacy(
        &self,
        pages: &mut impl Pages,
        current: CurrentStateView<'_>,
        sink: &mut impl Sink,
    ) -> Result<()> {
        self.validate()?;
        if self.watermark != current.watermark {
            return Err(Error::Conflict("history encoding watermark differs"));
        }
        sink.write(b"{\"commands\":").await?;
        stored_map(&self.commands, pages, sink).await?;
        sink.write(b",\"events\":[").await?;
        let mut after = None;
        let mut count = 0u64;
        loop {
            let rows = self.events.page(pages, after.as_deref(), 64).await?;
            if rows.is_empty() {
                break;
            }
            for row in rows {
                if row.key != ordinal_key(count) {
                    return Err(Error::Corruption("history encoding event order"));
                }
                if count != 0 {
                    sink.write(b",").await?;
                }
                let event: EventRow = serde_json::from_value(row.value)?;
                object(pages, &event.blob, sink).await?;
                after = Some(row.key);
                count = count
                    .checked_add(1)
                    .ok_or(Error::Limit("history encoding count"))?;
            }
        }
        if count != self.events.count() {
            return Err(Error::Corruption("history encoding event count"));
        }
        sink.write(b"],\"records\":").await?;
        current_map(sink, current.records.iter()).await?;
        sink.write(b",\"sequences\":").await?;
        current_map(sink, current.sequences.iter()).await?;
        sink.write(b",\"transactions\":").await?;
        stored_map(&self.transactions, pages, sink).await?;
        sink.write(b",\"watermark\":").await?;
        sink.write(&canonical_bytes(&current.watermark)?).await?;
        sink.write(b"}").await
    }
}

async fn object(pages: &mut impl Pages, blob: &Blob, sink: &mut impl Sink) -> Result<()> {
    if blob.bytes > MAX_COMMIT_BYTES as u64 {
        return Err(Error::Limit("history encoding row"));
    }
    let mut reader = Reader::new(blob)?;
    while let Some(bytes) = reader.next(pages).await? {
        sink.write(&bytes).await?;
    }
    Ok(())
}
async fn stored_map(root: &Root, pages: &mut impl Pages, sink: &mut impl Sink) -> Result<()> {
    sink.write(b"{").await?;
    let mut after = None;
    let mut count = 0u64;
    loop {
        let rows = root.page(pages, after.as_deref(), 64).await?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            if count != 0 {
                sink.write(b",").await?;
            }
            sink.write(&canonical_bytes(&row.key)?).await?;
            sink.write(b":").await?;
            let blob: Blob = serde_json::from_value(row.value)?;
            object(pages, &blob, sink).await?;
            after = Some(row.key);
            count = count
                .checked_add(1)
                .ok_or(Error::Limit("history encoding count"))?;
        }
    }
    if count != root.count() {
        return Err(Error::Corruption("history encoding map count"));
    }
    sink.write(b"}").await
}
async fn current_map<'a, K: Serialize + 'a, V: Serialize + 'a>(
    sink: &mut impl Sink,
    entries: impl Iterator<Item = (&'a K, &'a V)>,
) -> Result<()> {
    sink.write(b"{").await?;
    for (index, (key, value)) in entries.enumerate() {
        if index != 0 {
            sink.write(b",").await?;
        }
        sink.write(&canonical_bytes(key)?).await?;
        sink.write(b":").await?;
        sink.write(&canonical_bytes(value)?).await?;
    }
    sink.write(b"}").await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    #[derive(Default)]
    struct Memory(BTreeMap<String, Vec<u8>>);
    impl Pages for Memory {
        async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
            let bytes = self
                .0
                .get(digest)
                .ok_or(Error::Corruption("missing encoding object"))?;
            if bytes.len() > limit {
                return Err(Error::Limit("encoding object"));
            }
            Ok(bytes.clone())
        }
        async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
            self.0.insert(digest.into(), bytes.into());
            Ok(())
        }
    }
    #[tokio::test]
    async fn streamed_catalog_encoding_matches_exact_complete_state_bytes_and_digest() {
        let (mut state, _) = State::default().prepare(&common::initial()).unwrap();
        for index in 0..67 {
            if index > 0 {
                let mut transaction = common::initial();
                transaction.id = TransactionId::parse(format!("encoding-{index}")).unwrap();
                transaction.expected_watermark = state.watermark;
                transaction.mutations.clear();
                transaction.events[0].id = EventId::parse(format!("encoding-{index}")).unwrap();
                transaction.events[0].data =
                    serde_json::json!({"unicode":"é漢字","escaped":"\"\\\n","number":"1.00"});
                transaction.command = None;
                state = state.prepare(&transaction).unwrap().0;
            }
            if ![0, 1, 66].contains(&index) {
                continue;
            }
            let mut source = Memory::default();
            let catalog = Catalog::from_validated_state(&mut source, &state)
                .await
                .unwrap();
            let current = CurrentStateView::from(&state);
            let expected = crate::legacy_state_stream::bytes(&state).unwrap();
            let pages_before = source.0.len();
            assert_eq!(
                catalog
                    .legacy_bytes(&mut source, current, expected.len())
                    .await
                    .unwrap(),
                expected
            );
            assert!(matches!(
                catalog
                    .legacy_bytes(&mut source, current, expected.len() - 1)
                    .await,
                Err(Error::Limit("explicit archival State bytes"))
            ));
            assert_eq!(
                source.0.len(),
                pages_before,
                "archive read cannot create canonical objects"
            );
            assert_eq!(
                catalog.legacy_digest(&mut source, current).await.unwrap(),
                vcp_protocol::digest_bytes(&expected)
            );
            let mut destination = Memory::default();
            let blob = catalog
                .legacy_blob(&mut source, current, &mut destination)
                .await
                .unwrap();
            assert_eq!(
                history_blob::read_bounded(&mut destination, &blob, expected.len())
                    .await
                    .unwrap(),
                expected
            );
            let mut wrong = current;
            wrong.watermark = wrong.watermark.next().unwrap();
            assert!(catalog.legacy_digest(&mut source, wrong).await.is_err());
            source.0.clear();
            assert!(catalog
                .legacy_blob(&mut source, current, &mut destination)
                .await
                .is_err());
        }
    }
}
