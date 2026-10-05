// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::history_index::{Entry, Root};

#[derive(Default)]
struct Memory {
    rows: BTreeMap<String, Vec<u8>>,
    reads: usize,
    fail_read: bool,
    fail_write: bool,
}
impl Pages for Memory {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        self.reads += 1;
        if self.fail_read {
            return Err(Error::Unavailable("memo physical read"));
        }
        let bytes = self.rows.get(digest).ok_or(Error::Corruption("missing"))?;
        if bytes.len() > limit {
            return Err(Error::Limit("memo physical read"));
        }
        Ok(bytes.clone())
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
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
    }
    memory.fail_read = true;
    let mut session = ReadSession::new(&mut memory);
    assert!(matches!(
        root.page(&mut session, None, 80).await,
        Err(Error::Unavailable("memo physical read"))
    ));
    assert!(session.verified.entries.is_empty());
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
    session.verified = VerifiedPages {
        entries: BTreeMap::new(),
        bytes: MAX_BYTES,
    };
    root.load(&mut session, root.head.as_ref().unwrap())
        .await
        .unwrap();
    assert!(session.verified.entries.is_empty());
    assert_eq!(session.verified.bytes, MAX_BYTES);
}
