// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::journal_frame::{self, ReadFrame};
use vcp_domain::Watermark;
#[path = "../tests/common/mod.rs"]
mod common;

fn journal(root: &Path) -> Journal {
    Journal {
        file: OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(root.join("canonical.frames"))
            .unwrap(),
        root: root.to_owned(),
        chain: "0".repeat(64),
        base_watermark: Watermark::ZERO,
        initial_chain: "0".repeat(64),
        write_budget: None,
    }
}

#[test]
fn native_current_frame_binds_publication_preserves_legacy_bytes_and_rejects_tampering() {
    let temp = tempfile::tempdir().unwrap();
    let mut journal = journal(temp.path());
    // Deliberately noncanonical JSON spelling: a reader must preserve bytes,
    // never deserialize/re-encode an original retained commit body.
    let original = b"{ \"retained\" : \"exact body\" }";
    journal
        .append(original, Watermark::new(1), &|_| {})
        .unwrap();
    let old = fs::read(temp.path().join("canonical.frames")).unwrap();
    let old_chain = journal.chain.clone();
    let publication = "a".repeat(64);
    let next = b"{\"next\":true}";
    journal
        .append_current(next, Watermark::new(2), &publication, &|_| {})
        .unwrap();
    let bytes = fs::read(temp.path().join("canonical.frames")).unwrap();
    assert_eq!(&bytes[..old.len()], old);
    let ReadFrame::Complete(first) =
        journal_frame::read(&mut journal.file, 0, &"0".repeat(64)).unwrap()
    else {
        panic!("original frame")
    };
    assert_eq!(first.payload, original);
    assert!(first.publication.is_none());
    let ReadFrame::Complete(second) =
        journal_frame::read(&mut journal.file, first.end, &first.chain).unwrap()
    else {
        panic!("new frame")
    };
    assert_eq!(second.payload, next);
    assert_eq!(second.publication.as_deref(), Some(publication.as_str()));
    assert_eq!(second.chain, journal.chain);
    let tip: serde_json::Value = serde_json::from_slice(
        &fs::read(temp.path().join("commit-00000000000000000002.tip")).unwrap(),
    )
    .unwrap();
    assert_eq!(tip["publication"], publication);
    assert_eq!(tip["chain"], second.chain);
    assert_eq!(tip["end"], second.end.to_string());
    assert!(matches!(
        journal_frame::read(&mut journal.file, second.end, &second.chain).unwrap(),
        ReadFrame::End
    ));
    for relative in [
        16,
        80,
        journal_frame::CURRENT_HEADER,
        journal_frame::CURRENT_HEADER + next.len(),
        bytes.len() - old.len() - 1,
    ] {
        let mut altered = bytes.clone();
        altered[old.len() + relative] ^= 1;
        fs::write(temp.path().join("corrupt.frames"), altered).unwrap();
        let mut file = File::open(temp.path().join("corrupt.frames")).unwrap();
        assert!(journal_frame::read(&mut file, old.len() as u64, &old_chain).is_err());
    }
}

#[test]
fn every_partial_native_current_append_is_uncommitted_and_preserves_original_prefix() {
    let payload = b"{\"new\":true}";
    let publication = "b".repeat(64);
    let extent = journal_frame::CURRENT_HEADER + payload.len() + 72;
    for budget in 0..extent {
        let temp = tempfile::tempdir().unwrap();
        let mut journal = journal(temp.path());
        journal
            .append(b"{\"original\":true}", Watermark::new(1), &|_| {})
            .unwrap();
        let prefix = fs::read(temp.path().join("canonical.frames")).unwrap();
        let chain = journal.chain.clone();
        journal.write_budget = Some(budget);
        assert!(journal
            .append_current(payload, Watermark::new(2), &publication, &|_| {})
            .is_err());
        let bytes = fs::read(temp.path().join("canonical.frames")).unwrap();
        assert_eq!(&bytes[..prefix.len()], prefix);
        assert!(!temp.path().join("commit-00000000000000000002.tip").exists());
        assert!(matches!(
            journal_frame::read(&mut journal.file, prefix.len() as u64, &chain).unwrap(),
            ReadFrame::End | ReadFrame::Incomplete
        ));
    }
}

#[tokio::test]
async fn sqlite_current_publication_is_atomic_with_original_commit_and_every_materialized_row() {
    let temp = tempfile::tempdir().unwrap();
    let (mut backend, source) = Backend::open(temp.path(), BackendKind::Sqlite)
        .await
        .unwrap();
    let Backend::Sqlite(db) = &mut backend else {
        unreachable!()
    };
    initialize_sqlite(db).await.unwrap();
    let cut = {
        let mut pages = crate::history_index::io::Sqlite::new(db);
        let catalog = crate::history_catalog::Catalog::from_validated_state(&mut pages, &source)
            .await
            .unwrap();
        let originals =
            crate::original_commits::OriginalCommits::from_validated_base(&mut pages, None)
                .await
                .unwrap();
        DurableOwner::from_replayed(&mut pages, &source, None, catalog, originals)
            .await
            .unwrap()
    };
    let transaction = common::initial();
    let expected = source.prepare_reference(&transaction).unwrap();
    let prepared = {
        let mut pages = crate::history_index::io::Sqlite::new(db);
        let crate::durable_owner::Outcome::Prepared(prepared) =
            cut.prepare(&mut pages, &transaction).await.unwrap()
        else {
            panic!("initial transition")
        };
        prepared
    };
    let original_payload = serde_json::to_vec_pretty(prepared.commit()).unwrap();
    let prior_pages: i64 = sqlx::query_scalar("SELECT count(*) FROM history_index_pages")
        .fetch_one(&mut *db)
        .await
        .unwrap();
    // Fail at the final publication-row insert, after catalog pages and all
    // normal canonical rows have been tentatively written in the same native tx.
    sqlx::query("CREATE TRIGGER fail_current_publication BEFORE INSERT ON history_publications BEGIN SELECT RAISE(ABORT, 'injected publication failure'); END").execute(&mut *db).await.unwrap();
    assert!(
        append_sqlite(db, &cut, &prepared, &original_payload, &|_| {})
            .await
            .is_err()
    );
    for (table, query) in [
        ("commits", "SELECT count(*) FROM commits"),
        ("records", "SELECT count(*) FROM records"),
        ("events", "SELECT count(*) FROM events"),
        ("commands", "SELECT count(*) FROM commands"),
        (
            "history_publications",
            "SELECT count(*) FROM history_publications",
        ),
    ] {
        let count: i64 = sqlx::query_scalar(query).fetch_one(&mut *db).await.unwrap();
        assert_eq!(count, 0, "rollback left rows in {table}");
    }
    let after_pages: i64 = sqlx::query_scalar("SELECT count(*) FROM history_index_pages")
        .fetch_one(&mut *db)
        .await
        .unwrap();
    assert_eq!(after_pages, prior_pages);
    assert_eq!(cut.semantic().current().watermark, Watermark::ZERO);
    sqlx::query("DROP TRIGGER fail_current_publication")
        .execute(&mut *db)
        .await
        .unwrap();
    let next = append_sqlite(db, &cut, &prepared, &original_payload, &|_| {})
        .await
        .unwrap();
    let payload: Vec<u8> = sqlx::query_scalar("SELECT payload FROM commits WHERE watermark=1")
        .fetch_one(&mut *db)
        .await
        .unwrap();
    assert_eq!(payload, original_payload);
    let publication: String =
        sqlx::query_scalar("SELECT digest FROM history_publications WHERE watermark=1")
            .fetch_one(&mut *db)
            .await
            .unwrap();
    {
        let mut pages = crate::history_index::io::Sqlite::new(db);
        crate::history_publication::verify(
            &mut pages,
            &publication,
            &cut,
            &prepared,
            &next,
            &payload,
        )
        .await
        .unwrap();
        next.semantic()
            .catalog()
            .verify_replayed_state(&mut pages, &expected.0)
            .await
            .unwrap();
    }
    backend.verify_materialized(&expected.0).await.unwrap();
    backend.close().await.unwrap();
    // Existing original Commit semantics remain independently replayable. This
    // legacy backend oracle does not qualify layout-3 Store open/activation.
    let (backend, replayed) = Backend::open(temp.path(), BackendKind::Sqlite)
        .await
        .unwrap();
    assert_eq!(replayed, expected.0);
    backend.close().await.unwrap();
}

#[tokio::test]
async fn native_files_cold_replay_checks_each_transition_without_reconstructing_suffix_state() {
    use crate::{durable_owner, history_index::Pages};
    use vcp_domain::{CommandId, EventId, TransactionId};
    struct ReadOnly<'a, P>(&'a mut P);
    impl<P: Pages> Pages for ReadOnly<'_, P> {
        async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
            self.0.read(digest, limit).await
        }
        async fn write(&mut self, _: &str, _: &[u8]) -> Result<()> {
            panic!("cold semantic replay must never repair durable objects")
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("canonical");
    let forbidden = temp.path().join("workspace");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&forbidden).unwrap();
    let page_path = root.join("history");
    fs::create_dir(&page_path).unwrap();
    let directory = crate::private_paths::Directory::open(&page_path, &[forbidden]).unwrap();
    let mut pages = crate::history_index::io::Files::new(&directory);
    let mut journal = journal(&root);
    let (legacy, initial) = State::default()
        .prepare_reference(&common::initial())
        .unwrap();
    journal
        .append(
            &canonical_bytes(&initial).unwrap(),
            legacy.watermark,
            &|_| {},
        )
        .unwrap();
    let catalog = crate::history_catalog::Catalog::from_validated_state(&mut pages, &legacy)
        .await
        .unwrap();
    let originals = crate::original_commits::OriginalCommits::from_validated_base(&mut pages, None)
        .await
        .unwrap()
        .append_verified(&mut pages, &canonical_bytes(&initial).unwrap(), &initial)
        .await
        .unwrap();
    let origin = canonical_bytes(&(&catalog, &originals)).unwrap();
    immutable_file(&root.join("history-origin.json"), &origin).unwrap();
    let mut cut = DurableOwner::from_replayed(&mut pages, &legacy, None, catalog, originals)
        .await
        .unwrap();
    let mut expected = legacy;
    for index in 0..2 {
        let mut transaction = common::initial();
        transaction.id = TransactionId::parse(format!("native-replay-{index}")).unwrap();
        transaction.expected_watermark = cut.semantic().current().watermark;
        transaction.mutations.clear();
        transaction.events[0].id = EventId::parse(format!("native-event-{index}")).unwrap();
        transaction.events[0].correlation =
            CommandId::parse(format!("native-command-{index}")).unwrap();
        let command = transaction.command.as_mut().unwrap();
        command.command = transaction.events[0].correlation.clone();
        command.digest = format!("{index:064x}");
        let durable_owner::Outcome::Prepared(prepared) =
            cut.prepare(&mut pages, &transaction).await.unwrap()
        else {
            panic!("new transition")
        };
        let payload = serde_json::to_vec_pretty(prepared.commit()).unwrap();
        let (next, publication) =
            crate::history_publication::stage(&mut pages, &cut, &prepared, &payload)
                .await
                .unwrap();
        journal
            .append_current(
                &payload,
                prepared.commit().receipt.watermark,
                &publication,
                &|_| {},
            )
            .unwrap();
        expected = expected.prepare_reference(&transaction).unwrap().0;
        cut = next;
    }
    drop(journal);
    drop(cut);
    // Reopen native bytes, semantically replay the exact old prefix, jointly
    // authenticate its root, and drop that complete DTO before the new suffix.
    let mut file = File::open(root.join("canonical.frames")).unwrap();
    let ReadFrame::Complete(first) = journal_frame::read(&mut file, 0, &"0".repeat(64)).unwrap()
    else {
        panic!("legacy prefix")
    };
    let initial: Commit = serde_json::from_slice(&first.payload).unwrap();
    let mut legacy = State::default();
    legacy.replay(&initial).unwrap();
    let (catalog, originals): (
        crate::history_catalog::Catalog,
        crate::original_commits::OriginalCommits,
    ) = serde_json::from_slice(&fs::read(root.join("history-origin.json")).unwrap()).unwrap();
    assert_eq!(
        originals
            .get(&mut pages, initial.receipt.watermark)
            .await
            .unwrap()
            .unwrap()
            .bytes,
        first.payload
    );
    let mut read_only = ReadOnly(&mut pages);
    let mut cut = DurableOwner::from_replayed(&mut read_only, &legacy, None, catalog, originals)
        .await
        .unwrap();
    drop(legacy);
    let ReadFrame::Complete(second) =
        journal_frame::read(&mut file, first.end, &first.chain).unwrap()
    else {
        panic!("current suffix")
    };
    // A forged interior Commit still fails full semantics, even when a later
    // valid frame exists. This simulates a recomputed outer checksum; checksums
    // alone are deliberately insufficient for canonical admission.
    let mut invalid: Commit = serde_json::from_slice(&second.payload).unwrap();
    invalid.transaction.events[0].id = EventId::parse("created").unwrap();
    let corrupted = journal_frame::Frame {
        payload: canonical_bytes(&invalid).unwrap(),
        publication: second.publication.clone(),
        chain: second.chain.clone(),
        end: second.end,
    };
    assert!(matches!(
        replay_current(&mut read_only, &cut, &corrupted).await,
        Err(Error::Corruption("event identity"))
    ));
    let after_second = replay_current(&mut read_only, &cut, &second).await.unwrap();
    let ReadFrame::Complete(third) =
        journal_frame::read(&mut file, second.end, &second.chain).unwrap()
    else {
        panic!("final suffix")
    };
    cut = replay_current(&mut read_only, &after_second, &third)
        .await
        .unwrap();
    assert_eq!(
        cut.semantic().current(),
        &crate::CurrentState::from_state(&expected)
    );
    cut.semantic()
        .catalog()
        .verify_replayed_state(&mut read_only, &expected)
        .await
        .unwrap();
    // Missing a published object cannot be silently regenerated on cold open.
    let publication = third.publication.as_ref().unwrap();
    let path = directory.path.join(format!("{publication}.json"));
    let mut bytes = fs::read(&path).unwrap();
    bytes[0] ^= 1;
    fs::write(&path, bytes).unwrap();
    assert!(replay_current(&mut read_only, &after_second, &third)
        .await
        .is_err());
}

#[tokio::test]
async fn original_cursor_returns_exact_native_payload_spelling_on_both_backends() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut backend, empty) = Backend::open(temp.path(), kind).await.unwrap();
        let (state, commit) = empty.prepare_reference(&common::initial()).unwrap();
        let original = serde_json::to_vec_pretty(&commit).unwrap();
        assert_ne!(original, canonical_bytes(&commit).unwrap());
        match &mut backend {
            Backend::Files(journal) => journal.append(&original, state.watermark, &|_| {}).unwrap(),
            Backend::Sqlite(_) => {
                backend.append(&commit, &state, &|_| {}).await.unwrap();
                let Backend::Sqlite(db) = &mut backend else {
                    unreachable!()
                };
                sqlx::query("UPDATE commits SET payload=?,digest=? WHERE watermark=1")
                    .bind(&original)
                    .bind(digest_bytes(&original))
                    .execute(db)
                    .await
                    .unwrap();
            }
        }
        let mut reader = backend
            .history(temp.path(), &state, Watermark::ZERO)
            .await
            .unwrap();
        let row = reader.next_original().await.unwrap().unwrap();
        assert_eq!(row.bytes, original);
        assert_eq!(row.commit, commit);
        assert!(reader.next_original().await.unwrap().is_none());
        reader.close().await.unwrap();
        backend.close().await.unwrap();
        let (backend, replayed) = Backend::open(temp.path(), kind).await.unwrap();
        assert_eq!(replayed, state);
        backend.close().await.unwrap();
    }
}
