// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::collections::BTreeMap;
#[path = "../tests/common/mod.rs"]
mod common;

#[derive(Default)]
struct Memory {
    rows: BTreeMap<String, Vec<u8>>,
    fail_write: bool,
    fail_read: bool,
    block_write: bool,
    block_read: bool,
    largest_write: usize,
}
impl Pages for Memory {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        if self.block_read {
            return std::future::pending().await;
        }
        if self.fail_read {
            return Err(Error::Unavailable("synthetic read"));
        }
        let bytes = self
            .rows
            .get(digest)
            .ok_or(Error::Corruption("synthetic missing object"))?;
        if bytes.len() > limit {
            return Err(Error::Limit("synthetic object"));
        }
        Ok(bytes.clone())
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        if self.block_write {
            return std::future::pending().await;
        }
        if self.fail_write {
            return Err(Error::Unavailable("synthetic write"));
        }
        assert_eq!(digest_bytes(bytes), digest);
        self.largest_write = self.largest_write.max(bytes.len());
        if let Some(prior) = self.rows.get(digest) {
            assert_eq!(prior, bytes);
        }
        self.rows.insert(digest.into(), bytes.into());
        Ok(())
    }
}

#[tokio::test]
async fn chunk_boundaries_empty_stream_and_exact_legacy_bytes_round_trip() {
    let (state, _) = crate::contract::State::default()
        .prepare(&common::initial())
        .unwrap();
    let canonical = crate::legacy_state_stream::bytes(&state).unwrap();
    for bytes in [
        vec![],
        vec![7],
        vec![11; CHUNK_BYTES],
        vec![13; CHUNK_BYTES * 2 + 7],
        canonical,
    ] {
        let mut pages = Memory::default();
        let mut writer = Writer::new();
        for part in bytes.chunks(8191) {
            writer.push(&mut pages, part).await.unwrap();
        }
        let blob = writer.finish(&mut pages).await.unwrap();
        assert_eq!(blob.sha256, digest_bytes(&bytes));
        assert_eq!(blob.bytes, bytes.len() as u64);
        assert!(vcp_protocol::canonical_bytes(&blob).unwrap().len() < 2048);
        let decoded: Blob =
            serde_json::from_slice(&vcp_protocol::canonical_bytes(&blob).unwrap()).unwrap();
        assert_eq!(
            read_bounded(&mut pages, &decoded, bytes.len())
                .await
                .unwrap(),
            bytes
        );
        assert!(pages.largest_write <= CHUNK_BYTES);
        if !bytes.is_empty() {
            assert!(read_bounded(&mut pages, &blob, bytes.len() - 1)
                .await
                .is_err());
        }
    }
}

#[tokio::test]
async fn reader_rejects_missing_changed_reordered_or_short_content_and_wrong_root() {
    let bytes = vec![19; CHUNK_BYTES + 3];
    let mut pages = Memory::default();
    let blob = write_bytes(&mut pages, &bytes).await.unwrap();
    let mut bad = blob.clone();
    bad.index = Root::empty(Table::EventOrdinal);
    assert!(Reader::new(&bad).is_err());
    bad = blob.clone();
    bad.bytes += CHUNK_BYTES as u64;
    assert!(Reader::new(&bad).is_err());
    bad = blob.clone();
    bad.sha256 = "a".repeat(64);
    assert!(read_bounded(&mut pages, &bad, bytes.len()).await.is_err());
    let original = pages
        .rows
        .get(&digest_bytes(&bytes[..CHUNK_BYTES]))
        .unwrap()
        .clone();
    pages.rows.get_mut(&digest_bytes(&original)).unwrap()[0] ^= 1;
    let mut reader = Reader::new(&blob).unwrap();
    assert!(reader.next(&mut pages).await.is_err());
    pages.rows.insert(digest_bytes(&original), original);
    assert!(
        reader.next(&mut pages).await.is_err(),
        "reader cannot recover after a failed proof"
    );
    let mut wrong_order = Root::empty(Table::ArchiveChunks);
    for (index, chunk) in bytes.chunks(CHUNK_BYTES).rev().enumerate() {
        wrong_order = wrong_order
            .insert(
                &mut pages,
                Entry {
                    key: format!("{index:016x}"),
                    value: serde_json::json!({"bytes":chunk.len(),"sha256":digest_bytes(chunk)}),
                },
            )
            .await
            .unwrap();
    }
    bad = blob.clone();
    bad.index = wrong_order;
    assert!(read_bounded(&mut pages, &bad, bytes.len()).await.is_err());
    pages.rows.remove(&digest_bytes(&bytes[..CHUNK_BYTES]));
    assert!(read_bounded(&mut pages, &blob, bytes.len()).await.is_err());
}

#[tokio::test]
async fn failed_or_cancelled_io_never_yields_a_complete_descriptor_or_reader() {
    use std::time::Duration;
    let bytes = vec![29; CHUNK_BYTES];
    for blocked in [false, true] {
        let mut pages = Memory {
            fail_write: !blocked,
            block_write: blocked,
            ..Default::default()
        };
        let mut writer = Writer::new();
        let result =
            tokio::time::timeout(Duration::from_millis(10), writer.push(&mut pages, &bytes)).await;
        assert!(result.is_err() || result.unwrap().is_err());
        pages.block_write = false;
        pages.fail_write = false;
        assert!(writer.finish(&mut pages).await.is_err());
        let blob = write_bytes(&mut pages, &bytes).await.unwrap();
        let mut reader = Reader::new(&blob).unwrap();
        pages.fail_read = !blocked;
        pages.block_read = blocked;
        let result = tokio::time::timeout(Duration::from_millis(10), reader.next(&mut pages)).await;
        assert!(result.is_err() || result.unwrap().is_err());
        pages.fail_read = false;
        pages.block_read = false;
        assert!(reader.next(&mut pages).await.is_err());
    }
}

#[tokio::test]
async fn history_larger_than_legacy_state_limit_streams_with_one_chunk_buffer() {
    let mut pages = Memory::default();
    let chunk = vec![37; CHUNK_BYTES];
    let mut writer = Writer::new();
    let count = crate::contract::MAX_STATE_BYTES / CHUNK_BYTES + 1;
    let mut expected = Sha256::new();
    for _ in 0..count {
        writer.push(&mut pages, &chunk).await.unwrap();
        assert!(writer.pending.capacity() <= CHUNK_BYTES);
        expected.update(&chunk);
    }
    let blob = writer.finish(&mut pages).await.unwrap();
    assert!(blob.bytes > crate::contract::MAX_STATE_BYTES as u64);
    assert_eq!(blob.sha256, format!("{:x}", expected.finalize()));
    assert!(
        read_bounded(&mut pages, &blob, crate::contract::MAX_STATE_BYTES)
            .await
            .is_err()
    );
    let mut reader = Reader::new(&blob).unwrap();
    let mut observed = 0u64;
    while let Some(bytes) = reader.next(&mut pages).await.unwrap() {
        assert_eq!(bytes, chunk);
        observed += bytes.len() as u64;
    }
    assert_eq!(observed, blob.bytes);
    assert!(reader.complete);
    assert!(reader.next(&mut pages).await.unwrap().is_none());
}
