// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::collections::BTreeMap;

#[derive(Default)]
struct MemoryPages {
    pages: BTreeMap<String, Vec<u8>>,
    reads: usize,
    writes: usize,
    fail_write: bool,
}
impl Pages for MemoryPages {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        self.reads += 1;
        let bytes = self
            .pages
            .get(digest)
            .ok_or(Error::Corruption("missing test page"))?;
        if bytes.len() > limit {
            return Err(Error::Limit("test page read"));
        }
        Ok(bytes.clone())
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        if self.fail_write {
            return Err(Error::Unavailable("synthetic index write"));
        }
        self.writes += 1;
        if let Some(prior) = self.pages.get(digest) {
            if prior != bytes {
                return Err(Error::Corruption("index page replacement"));
            }
        } else {
            self.pages.insert(digest.into(), bytes.into());
        }
        Ok(())
    }
}
fn entry(index: usize) -> Entry {
    Entry {
        key: format!("{:020}", index * 2),
        value: serde_json::json!({"commit_watermark":(index+1).to_string(),"event_ordinal":"0"}),
    }
}

#[tokio::test]
async fn persistent_index_matches_ordered_reference_and_pins_old_roots() {
    let mut pages = MemoryPages::default();
    let mut root = Root::empty(Table::EventOrdinal);
    let mut reference = BTreeMap::new();
    let mut snapshots = Vec::new();
    // Exceed FANOUT² so branch splitting and a second branch level are
    // exercised regardless of leaf occupancy under this permutation.
    for index in 0..1103 {
        let row = entry((index * 97) % 1103);
        reference.insert(row.key.clone(), row.clone());
        root = root.insert(&mut pages, row).await.unwrap();
        assert_eq!(root.count(), reference.len() as u64);
        if index % 100 == 0 {
            snapshots.push((root.clone(), reference.clone()));
        }
    }
    assert!(root.head.as_ref().unwrap().height >= 2);
    for row in reference.values() {
        pages.reads = 0;
        assert_eq!(
            root.get(&mut pages, &row.key).await.unwrap(),
            Some(row.clone())
        );
        assert!(pages.reads <= usize::from(root.head.as_ref().unwrap().height) + 1);
    }
    assert!(root
        .get(&mut pages, "00000000000000000001")
        .await
        .unwrap()
        .is_none());
    for (root, expected) in snapshots.into_iter().chain([(root, reference)]) {
        let mut collected = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let rows = root.page(&mut pages, after.as_deref(), 7).await.unwrap();
            if rows.is_empty() {
                break;
            }
            assert!(rows.len() <= 7);
            after = Some(rows.last().unwrap().key.clone());
            collected.extend(rows);
        }
        assert_eq!(collected, expected.into_values().collect::<Vec<_>>());
    }
}

#[tokio::test]
async fn owner_root_rejects_changed_missing_cross_table_and_resealed_pages() {
    let mut pages = MemoryPages::default();
    let root = Root::empty(Table::Transaction)
        .insert(&mut pages, entry(0))
        .await
        .unwrap();
    let link = root.head.as_ref().unwrap();
    let original = pages.pages[&link.digest].clone();
    let mut altered: Page = serde_json::from_slice(&original).unwrap();
    let Body::Leaf(rows) = &mut altered.body else {
        unreachable!()
    };
    rows[0].value = serde_json::json!({"commit_watermark":"forged"});
    let resealed = canonical_bytes(&altered).unwrap();
    pages
        .pages
        .insert(digest_bytes(&resealed), resealed.clone());
    pages.pages.insert(link.digest.clone(), resealed);
    assert!(matches!(
        root.get(&mut pages, &entry(0).key).await,
        Err(Error::Corruption("history index page digest"))
    ));
    pages.pages.remove(&link.digest);
    assert!(root.get(&mut pages, &entry(0).key).await.is_err());
    pages.pages.insert(link.digest.clone(), original);
    let cross_table = Root {
        table: Table::Command,
        ..root.clone()
    };
    assert!(matches!(
        cross_table.get(&mut pages, &entry(0).key).await,
        Err(Error::Corruption("history index page domain"))
    ));
    let mut wrong_count = root.clone();
    wrong_count.head.as_mut().unwrap().count += 1;
    assert!(matches!(
        wrong_count.get(&mut pages, &entry(0).key).await,
        Err(Error::Corruption("history index page commitment"))
    ));
}

#[tokio::test]
async fn malformed_pages_bounds_duplicate_identity_and_failed_publication_fail_closed() {
    let mut pages = MemoryPages::default();
    let root = Root::empty(Table::Commit)
        .insert(&mut pages, entry(0))
        .await
        .unwrap();
    let writes = pages.writes;
    assert_eq!(root.insert(&mut pages, entry(0)).await.unwrap(), root);
    assert_eq!(pages.writes, writes);
    let mut changed = entry(0);
    changed.value = serde_json::json!({"changed":true});
    assert!(root.insert(&mut pages, changed).await.is_err());
    pages.fail_write = true;
    assert!(root.insert(&mut pages, entry(1)).await.is_err());
    pages.fail_write = false;
    assert_eq!(
        root.get(&mut pages, &entry(0).key).await.unwrap(),
        Some(entry(0))
    );
    assert_eq!(root.count(), 1);
    assert!(root.page(&mut pages, None, 0).await.is_err());
    assert!(root
        .page(&mut pages, None, MAX_READ_ROWS + 1)
        .await
        .is_err());
    assert!(root.get(&mut pages, "../invalid\n").await.is_err());
    let mut oversized = entry(1);
    oversized.value = serde_json::json!("x".repeat(MAX_VALUE_BYTES + 1));
    assert!(root.insert(&mut pages, oversized).await.is_err());
    let invalid_bodies = [
        Body::Leaf(vec![]),
        Body::Leaf(vec![entry(1), entry(0)]),
        Body::Leaf(vec![entry(0), entry(0)]),
        Body::Branch(vec![]),
        Body::Branch(vec![root.head.clone().unwrap()]),
        Body::Branch(vec![root.head.clone().unwrap(), root.head.clone().unwrap()]),
    ];
    for body in invalid_bodies {
        assert!(root.save(&mut pages, body).await.is_err());
    }
}

async fn native_corpus(pages: &mut impl Pages) -> Root {
    let mut root = Root::empty(Table::Command);
    for index in (0..85).rev() {
        root = root.insert(pages, entry(index)).await.unwrap();
    }
    for index in 0..85 {
        assert_eq!(
            root.get(pages, &entry(index).key).await.unwrap(),
            Some(entry(index))
        );
    }
    assert_eq!(
        root.page(pages, Some(&entry(30).key), 17).await.unwrap(),
        (31..48).map(entry).collect::<Vec<_>>()
    );
    root
}

#[tokio::test]
async fn file_pages_reopen_under_pinned_directory_and_reject_mutable_replacements() {
    let temp = tempfile::tempdir().unwrap();
    let pages_path = temp.path().join("pages");
    let forbidden = temp.path().join("workspace");
    std::fs::create_dir(&pages_path).unwrap();
    std::fs::create_dir(&forbidden).unwrap();
    let directory =
        crate::private_paths::Directory::open(&pages_path, &[forbidden.clone()]).unwrap();
    let mut pages = io::Files::new(&directory);
    let root = native_corpus(&mut pages).await;
    let trusted_root = root.clone();
    drop(pages);
    drop(directory);
    let directory = crate::private_paths::Directory::open(&pages_path, &[forbidden]).unwrap();
    let mut pages = io::Files::new(&directory);
    assert_eq!(
        trusted_root.page(&mut pages, None, 128).await.unwrap(),
        (0..85).map(entry).collect::<Vec<_>>()
    );
    let head = trusted_root.head.as_ref().unwrap();
    let original = pages.read(&head.digest, MAX_PAGE_BYTES).await.unwrap();
    pages.write(&head.digest, &original).await.unwrap();
    let path = pages_path.join(format!("{}.json", head.digest));
    let mut corrupt = original.clone();
    corrupt[0] ^= 1;
    std::fs::write(&path, corrupt).unwrap();
    assert!(trusted_root.get(&mut pages, &entry(0).key).await.is_err());
    assert!(
        pages.write(&head.digest, &original).await.is_err(),
        "immutable write cannot repair/overwrite conflicting retained bytes"
    );
    assert!(pages.read("../escaped", MAX_PAGE_BYTES).await.is_err());
}

#[tokio::test]
async fn sqlite_pages_use_existing_connection_and_refuse_oversized_or_changed_blobs() {
    use sqlx::Connection;
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("canonical.sqlite");
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&database)
        .create_if_missing(true);
    let mut connection = sqlx::SqliteConnection::connect_with(&options)
        .await
        .unwrap();
    io::Sqlite::initialize(&mut connection).await.unwrap();
    let root = native_corpus(&mut io::Sqlite::new(&mut connection)).await;
    connection.close().await.unwrap();
    let options = options.create_if_missing(false);
    let mut connection = sqlx::SqliteConnection::connect_with(&options)
        .await
        .unwrap();
    assert_eq!(
        root.page(&mut io::Sqlite::new(&mut connection), None, 128)
            .await
            .unwrap(),
        (0..85).map(entry).collect::<Vec<_>>()
    );
    let head = root.head.as_ref().unwrap();
    let original = io::Sqlite::new(&mut connection)
        .read(&head.digest, MAX_PAGE_BYTES)
        .await
        .unwrap();
    sqlx::query("UPDATE history_index_pages SET payload=zeroblob(?) WHERE digest=?")
        .bind((MAX_PAGE_BYTES + 1) as i64)
        .bind(&head.digest)
        .execute(&mut connection)
        .await
        .unwrap();
    assert!(matches!(
        root.get(&mut io::Sqlite::new(&mut connection), &entry(0).key)
            .await,
        Err(Error::Corruption("history index page storage bound"))
    ));
    assert!(io::Sqlite::new(&mut connection)
        .write(&head.digest, &original)
        .await
        .is_err());
    sqlx::query("UPDATE history_index_pages SET payload='not a blob' WHERE digest=?")
        .bind(&head.digest)
        .execute(&mut connection)
        .await
        .unwrap();
    assert!(root
        .get(&mut io::Sqlite::new(&mut connection), &entry(0).key)
        .await
        .is_err());
    connection.close().await.unwrap();
}
