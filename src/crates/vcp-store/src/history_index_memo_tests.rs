// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::history_index::{Entry, Root};

#[derive(Default)]
struct Memory {
    rows: BTreeMap<String, Vec<u8>>,
    reads: usize,
    fail_read: bool,
    fail_write: bool,
    ignore_limit: bool,
    pending_write: bool,
}
impl Pages for Memory {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        self.reads += 1;
        if self.fail_read {
            return Err(Error::Unavailable("memo physical read"));
        }
        let bytes = self.rows.get(digest).ok_or(Error::Corruption("missing"))?;
        if !self.ignore_limit && bytes.len() > limit {
            return Err(Error::Limit("memo physical read"));
        }
        Ok(bytes.clone())
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        if self.pending_write {
            std::future::pending::<()>().await;
        }
        if self.fail_write {
            return Err(Error::Unavailable("memo physical write"));
        }
        self.rows.insert(digest.to_owned(), bytes.to_vec());
        Ok(())
    }
}
async fn fixture(memory: &mut Memory, count: usize) -> Root {
    let mut root = Root::empty(Table::EventOrdinal);
    for index in 0..count {
        root = root
            .insert(
                memory,
                Entry {
                    key: format!("{index:08}"),
                    value: index.into(),
                },
            )
            .await
            .unwrap();
    }
    memory.reads = 0;
    root
}

#[tokio::test]
async fn repeated_traversal_reuses_verified_pages_but_next_operation_reads_again() {
    let mut memory = Memory::default();
    let root = fixture(&mut memory, 80).await;
    {
        let mut session = ReadSession::new(&mut memory);
        let first = root.page(&mut session, None, 80).await.unwrap();
        let reads = session.inner.reads;
        assert!(reads > 1);
        assert_eq!(root.page(&mut session, None, 80).await.unwrap(), first);
        assert_eq!(session.inner.reads, reads);
        let observed = session.observations();
        assert_eq!(observed.physical_started, reads as u64);
        assert_eq!(observed.physical_completed, reads as u64);
        assert_eq!(observed.physical_failed, 0);
        assert!(observed.physical_bytes > 0);
        assert!(observed.index_hits > 0);
        assert_eq!(observed.index_misses, reads as u64);
        assert_eq!(observed.index_admitted, reads as u64);
        assert_eq!(observed.payload_hits, 0);
    }
    memory.fail_read = true;
    let mut session = ReadSession::new(&mut memory);
    assert!(matches!(
        root.page(&mut session, None, 80).await,
        Err(Error::Unavailable("memo physical read"))
    ));
    assert!(session.verified.entries.is_empty());
    assert_eq!(session.observations().physical_started, 1);
    assert_eq!(session.observations().physical_failed, 1);
    assert_eq!(session.observations().physical_bytes, 0);
}

#[tokio::test]
async fn hits_recheck_table_and_complete_link_commitment() {
    let mut memory = Memory::default();
    let root = fixture(&mut memory, 1).await;
    let link = root.head.clone().unwrap();
    let mut session = ReadSession::new(&mut memory);
    root.load(&mut session, &link).await.unwrap();
    let mut other_table = root.clone();
    other_table.table = Table::Command;
    assert!(matches!(
        other_table.load(&mut session, &link).await,
        Err(Error::Corruption("history index page domain"))
    ));
    for changed in 0..4 {
        let mut different = link.clone();
        match changed {
            0 => different.count += 1,
            1 => different.first = "0000000".into(),
            2 => different.last = "00000001".into(),
            _ => different.height += 1,
        }
        assert!(root.load(&mut session, &different).await.is_err());
    }
    assert_eq!(session.inner.reads, 1);
    assert_eq!(session.observations().index_rejected_hits, 5);
}

#[tokio::test]
async fn corruption_is_not_memoized_and_failed_write_discards_observations() {
    let mut memory = Memory::default();
    let root = fixture(&mut memory, 1).await;
    let link = root.head.clone().unwrap();
    let original = memory.rows[&link.digest].clone();
    memory.rows.insert(link.digest.clone(), b"corrupt".to_vec());
    {
        let mut session = ReadSession::new(&mut memory);
        assert!(root.load(&mut session, &link).await.is_err());
        assert!(session.verified.entries.is_empty());
        session.inner.rows.insert(link.digest.clone(), original);
        root.load(&mut session, &link).await.unwrap();
        session.inner.fail_write = true;
        assert!(session.write(&link.digest, b"unused").await.is_err());
        assert!(session.verified.entries.is_empty());
        session.inner.fail_read = true;
        assert!(matches!(
            root.load(&mut session, &link).await,
            Err(Error::Unavailable("memo physical read"))
        ));
    }
    memory.fail_read = false;
    memory
        .rows
        .insert(link.digest.clone(), b"later corruption".to_vec());
    assert!(root
        .load(&mut ReadSession::new(&mut memory), &link)
        .await
        .is_err());
}

#[tokio::test]
async fn memo_limits_do_not_limit_valid_history_or_skip_uncached_reads() {
    let mut memory = Memory::default();
    let mut roots = Vec::new();
    for index in 0..MAX_ENTRIES + 1 {
        roots.push(
            Root::empty(Table::EventOrdinal)
                .insert(
                    &mut memory,
                    Entry {
                        key: format!("{index:08}"),
                        value: index.into(),
                    },
                )
                .await
                .unwrap(),
        );
    }
    let mut session = ReadSession::new(&mut memory);
    for root in &roots {
        root.load(&mut session, root.head.as_ref().unwrap())
            .await
            .unwrap();
    }
    assert_eq!(session.verified.entries.len(), MAX_ENTRIES);
    assert!(session.verified.bytes <= MAX_BYTES);
    let root = roots.last().unwrap();
    let reads = session.inner.reads;
    root.load(&mut session, root.head.as_ref().unwrap())
        .await
        .unwrap();
    assert_eq!(session.inner.reads, reads + 1);
    assert_eq!(session.observations().index_refused_entries, 2);
    session.verified = VerifiedPages {
        entries: BTreeMap::new(),
        bytes: MAX_BYTES,
        ..Default::default()
    };
    root.load(&mut session, root.head.as_ref().unwrap())
        .await
        .unwrap();
    assert!(session.verified.entries.is_empty());
    assert_eq!(session.verified.bytes, MAX_BYTES);
    assert_eq!(session.observations().index_refused_bytes, 1);
}

#[tokio::test]
async fn repeated_blob_passes_reuse_bytes_but_new_operation_rechecks_storage() {
    let mut memory = Memory::default();
    let original = vec![b'a'; crate::history_blob::CHUNK_BYTES + 13];
    let blob = crate::history_blob::write_bytes(&mut memory, &original)
        .await
        .unwrap();
    {
        let mut session = ReadSession::new(&mut memory);
        assert_eq!(
            crate::history_blob::read_bounded(&mut session, &blob, original.len())
                .await
                .unwrap(),
            original
        );
        let reads = session.inner.reads;
        for _ in 0..2 {
            assert_eq!(
                crate::history_blob::read_bounded(&mut session, &blob, original.len())
                    .await
                    .unwrap(),
                original
            );
        }
        assert_eq!(session.inner.reads, reads);
        assert_eq!(session.chunks.len(), 2);
    }
    let key = vcp_protocol::digest_bytes(&original[..crate::history_blob::CHUNK_BYTES]);
    memory.rows.insert(key, b"later corruption".to_vec());
    assert!(crate::history_blob::read_bounded(
        &mut ReadSession::new(&mut memory),
        &blob,
        original.len()
    )
    .await
    .is_err());
}

#[tokio::test]
async fn chunk_hits_preserve_limits_and_failed_writes_invalidate_bytes() {
    let mut memory = Memory::default();
    let bytes = b"physical payload".to_vec();
    let digest = vcp_protocol::digest_bytes(&bytes);
    memory.rows.insert(digest.clone(), bytes.clone());
    let mut session = ReadSession::new(&mut memory);
    assert_eq!(session.read(&digest, 64).await.unwrap(), bytes);
    assert!(matches!(
        session.read(&digest, 1).await,
        Err(Error::Limit("memo physical read"))
    ));
    assert_eq!(session.inner.reads, 2);
    session.inner.fail_write = true;
    assert!(session.write(&digest, &bytes).await.is_err());
    assert!(session.chunks.is_empty());
    assert_eq!(session.chunk_bytes, 0);
    session.inner.fail_read = true;
    assert!(session.read(&digest, 64).await.is_err());
    assert!(session.chunks.is_empty());
}

#[tokio::test]
async fn invalid_physical_bytes_and_failed_reads_never_enter_chunk_memo() {
    let mut memory = Memory::default();
    let correct = b"correct".to_vec();
    let digest = vcp_protocol::digest_bytes(&correct);
    memory.rows.insert(digest.clone(), b"wrong".to_vec());
    let mut session = ReadSession::new(&mut memory);
    assert_eq!(session.read(&digest, 64).await.unwrap(), b"wrong");
    assert!(session.chunks.is_empty());
    session.inner.rows.insert(digest.clone(), correct.clone());
    session.inner.fail_read = true;
    assert!(session.read(&digest, 64).await.is_err());
    assert!(session.chunks.is_empty());
    session.inner.fail_read = false;
    assert!(session.read(&digest, 1).await.is_err());
    assert!(session.chunks.is_empty());
    session.inner.ignore_limit = true;
    assert_eq!(session.read(&digest, 1).await.unwrap(), correct);
    assert!(session.chunks.is_empty());
    assert_eq!(session.read(&digest, 64).await.unwrap(), correct);
    assert_eq!(session.chunks.len(), 1);
}

#[tokio::test]
async fn cancelled_write_discards_verified_payload_observations() {
    let mut memory = Memory::default();
    let bytes = b"observed payload".to_vec();
    let digest = vcp_protocol::digest_bytes(&bytes);
    memory.rows.insert(digest.clone(), bytes.clone());
    let mut session = ReadSession::new(&mut memory);
    session.read(&digest, 64).await.unwrap();
    session.inner.pending_write = true;
    {
        let mut writing = Box::pin(session.write(&digest, &bytes));
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(writing.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
    }
    assert!(session.chunks.is_empty());
    assert_eq!(session.chunk_bytes, 0);
    session.inner.fail_read = true;
    assert!(session.read(&digest, 64).await.is_err());
}

#[tokio::test]
async fn chunk_memo_bounds_fall_back_without_limiting_valid_history() {
    let mut memory = Memory::default();
    for index in 0..MAX_ENTRIES + 1 {
        let bytes = index.to_le_bytes().to_vec();
        memory
            .rows
            .insert(vcp_protocol::digest_bytes(&bytes), bytes);
    }
    let keys: Vec<_> = memory.rows.keys().cloned().collect();
    let mut session = ReadSession::new(&mut memory);
    for key in &keys {
        session.read(key, 64).await.unwrap();
    }
    assert_eq!(session.chunks.len(), MAX_ENTRIES);
    let reads = session.inner.reads;
    session.read(keys.last().unwrap(), 64).await.unwrap();
    assert_eq!(session.inner.reads, reads + 1);
    session.chunks.clear();
    session.chunk_bytes = MAX_BYTES;
    session.read(&keys[0], 64).await.unwrap();
    assert!(session.chunks.is_empty());
    assert_eq!(session.chunk_bytes, MAX_BYTES);
    assert_eq!(session.inner.reads, reads + 2);
    assert_eq!(session.observations().payload_admitted, MAX_ENTRIES as u64);
    assert_eq!(session.observations().payload_refused_entries, 2);
    assert_eq!(session.observations().payload_refused_bytes, 1);
}

#[tokio::test]
async fn read_counters_preserve_totals_across_write_invalidation_and_errors() {
    let mut memory = Memory::default();
    let root = fixture(&mut memory, 1).await;
    let bytes = b"bounded payload".to_vec();
    let digest = vcp_protocol::digest_bytes(&bytes);
    memory.rows.insert(digest.clone(), bytes.clone());
    let mut session = ReadSession::new(&mut memory);
    root.page(&mut session, None, 1).await.unwrap();
    session.read(&digest, 64).await.unwrap();
    let before = session.observations();
    session.read(&digest, 64).await.unwrap();
    let hit = session.observations().since(before);
    assert_eq!(hit.payload_hits, 1);
    assert_eq!(hit.physical_started, 0);
    session.inner.fail_write = true;
    assert!(session.write(&digest, &bytes).await.is_err());
    assert_eq!(
        session.observations().physical_started,
        before.physical_started
    );
    assert_eq!(session.observations().index_admitted, before.index_admitted);
    session.inner.fail_read = true;
    assert!(session.read(&digest, 64).await.is_err());
    let failure = session.observations().since(before);
    assert_eq!(failure.physical_started, 1);
    assert_eq!(failure.physical_completed, 1);
    assert_eq!(failure.physical_failed, 1);
    assert_eq!(failure.physical_bytes, 0);
}

#[test]
fn history_read_diagnostics_decode_legacy_defaults_and_saturate() {
    let old: crate::HistoryReadPhases = serde_json::from_str("{}").unwrap();
    assert_eq!(old, crate::HistoryReadPhases::default());
    let mut counts: HistoryReads = serde_json::from_str(r#"{"physical_started":1}"#).unwrap();
    assert_eq!(counts.physical_failed, 0);
    counts.add(HistoryReads {
        physical_started: u64::MAX,
        ..Default::default()
    });
    assert_eq!(counts.physical_started, u64::MAX);
    assert_eq!(
        serde_json::from_value::<HistoryReads>(serde_json::to_value(counts).unwrap()).unwrap(),
        counts
    );
}
