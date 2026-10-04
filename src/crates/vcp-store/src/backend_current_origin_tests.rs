// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    history_index::{io::Files, Pages},
    private_paths::Directory,
    StoreDiagnostics,
};
use vcp_domain::{CommandId, EventId, TransactionId, Watermark};
#[path = "../tests/common/mod.rs"]
mod common;

struct ReadOnly<'a, P>(&'a mut P);
impl<P: Pages> Pages for ReadOnly<'_, P> {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        self.0.read(digest, limit).await
    }
    async fn write(&mut self, _: &str, _: &[u8]) -> Result<()> {
        panic!("cold replay attempted a page write")
    }
}
fn reopen(root: &Path) -> Journal {
    Journal {
        file: OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join("canonical.frames"))
            .unwrap(),
        root: root.to_owned(),
        chain: "0".repeat(64),
        initial_chain: "0".repeat(64),
        base_watermark: Watermark::ZERO,
        write_budget: None,
    }
}
fn next(state: &State, index: u64) -> Transaction {
    let mut tx = common::initial();
    tx.id = TransactionId::parse(format!("migration-{index}")).unwrap();
    tx.expected_watermark = state.watermark;
    tx.mutations.clear();
    tx.events[0].id = EventId::parse(format!("migration-event-{index}")).unwrap();
    tx.events[0].correlation = CommandId::parse(format!("migration-command-{index}")).unwrap();
    tx.command.as_mut().unwrap().command = tx.events[0].correlation.clone();
    tx.command.as_mut().unwrap().digest = format!("{index:064x}");
    tx
}
#[tokio::test]
async fn migrated_files_cold_open_replays_prefix_suffix_and_checks_tip_checkpoint_and_original_bytes(
) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("canonical");
    let forbidden = temp.path().join("workspace");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&forbidden).unwrap();
    fs::create_dir(root.join("history")).unwrap();
    let directory = Directory::open(&root.join("history"), &[forbidden]).unwrap();
    let mut pages = Files::new(&directory);
    let (mut backend, empty) = Backend::open(&root, BackendKind::Files).await.unwrap();
    let (mut expected, first) = empty.prepare_reference(&common::initial()).unwrap();
    let pretty = serde_json::to_vec_pretty(&first).unwrap();
    let Backend::Files(journal) = &mut backend else {
        unreachable!()
    };
    journal
        .append(&pretty, expected.watermark, &|_| {})
        .unwrap();
    // Checkpoint's canonical chain is intentionally sensitive to pretty native
    // bytes under the existing contract; use a canonical fixture for that check.
    let prefix = fs::read(root.join("canonical.frames")).unwrap();
    let mut reader = backend
        .history(&root, &expected, Watermark::ZERO)
        .await
        .unwrap();
    let (origin, digest, mut owner) = Origin::stage(&mut pages, &expected, None, &mut reader)
        .await
        .unwrap();
    reader.close().await.unwrap();
    assert_eq!(origin.watermark(), expected.watermark);
    for index in 1..=2 {
        let tx = next(&expected, index);
        let crate::durable_owner::Outcome::Prepared(prepared) =
            owner.prepare(&mut pages, &tx).await.unwrap()
        else {
            panic!("prepared")
        };
        let payload = serde_json::to_vec_pretty(prepared.commit()).unwrap();
        let (advanced, publication) =
            crate::history_publication::stage(&mut pages, &owner, &prepared, &payload)
                .await
                .unwrap();
        let Backend::Files(journal) = &mut backend else {
            unreachable!()
        };
        journal
            .append_current(
                &payload,
                prepared.commit().receipt.watermark,
                &publication,
                &|_| {},
            )
            .unwrap();
        expected = expected.prepare_reference(&tx).unwrap().0;
        owner = advanced;
    }
    let identity = owner.identity().to_owned();
    drop(owner);
    backend.close().await.unwrap();
    let bytes = fs::read(root.join("canonical.frames")).unwrap();
    assert_eq!(&bytes[..prefix.len()], prefix);
    let origin = Origin::load(&mut pages, &digest).await.unwrap();
    let mut readonly = ReadOnly(&mut pages);
    let mut journal = reopen(&root);
    let mut diagnostics = StoreDiagnostics::new(BackendKind::Files);
    let reopened = journal
        .replay_current_all(&mut readonly, &origin, None, &mut diagnostics)
        .await
        .unwrap();
    assert_eq!(reopened.identity(), identity);
    assert_eq!(
        reopened.semantic().current(),
        &crate::CurrentState::from_state(&expected)
    );
    assert_eq!(diagnostics.replayed_commits, 3);
    reopened
        .semantic()
        .catalog()
        .verify_replayed_state(&mut readonly, &expected)
        .await
        .unwrap();
    drop(journal);
    // A partial unacknowledged append is quarantined, preserving every original
    // byte; the admitted owner remains exactly the last complete transition.
    let mut file = OpenOptions::new()
        .append(true)
        .open(root.join("canonical.frames"))
        .unwrap();
    file.write_all(b"VCPJ0003").unwrap();
    drop(file);
    let mut journal = reopen(&root);
    assert_eq!(
        journal
            .replay_current_all(
                &mut readonly,
                &origin,
                None,
                &mut StoreDiagnostics::new(BackendKind::Files)
            )
            .await
            .unwrap()
            .identity(),
        identity
    );
    drop(journal);
    assert_eq!(fs::read(root.join("canonical.frames")).unwrap(), bytes);
    // Truncating an acknowledged frame fails rather than treating it as a tail.
    fs::write(root.join("canonical.frames"), &bytes[..bytes.len() - 1]).unwrap();
    let mut journal = reopen(&root);
    assert!(journal
        .replay_current_all(
            &mut readonly,
            &origin,
            None,
            &mut StoreDiagnostics::new(BackendKind::Files)
        )
        .await
        .is_err());
    drop(journal);
    fs::write(root.join("canonical.frames"), &bytes).unwrap();
    // A resealed descriptor with the same cut but changed prefix commitment is
    // rejected after full replay; neither roots nor a digest are replay trust.
    let original = readonly.read(&digest, 16 * 1024).await.unwrap();
    let mut forged: serde_json::Value = serde_json::from_slice(&original).unwrap();
    forged["legacy_sha256"] = serde_json::Value::String("f".repeat(64));
    let forged: Origin = serde_json::from_value(forged).unwrap();
    let mut journal = reopen(&root);
    assert!(matches!(
        journal
            .replay_current_all(
                &mut readonly,
                &forged,
                None,
                &mut StoreDiagnostics::new(BackendKind::Files)
            )
            .await,
        Err(Error::Corruption("history origin legacy state differs"))
    ));
    // Native and indexed JSON spellings must agree even when parsed Commit and
    // all semantic receipts remain exactly equal.
    assert!(matches!(
        origin
            .verify_original(&mut readonly, &first, &canonical_bytes(&first).unwrap())
            .await,
        Err(Error::Corruption("history origin native payload differs"))
    ));
}

#[tokio::test]
async fn migrated_files_cold_open_preserves_legacy_checkpoint_validation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("canonical");
    let forbidden = temp.path().join("workspace");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&forbidden).unwrap();
    fs::create_dir(root.join("history")).unwrap();
    let directory = Directory::open(&root.join("history"), &[forbidden]).unwrap();
    let mut pages = Files::new(&directory);
    let (mut backend, empty) = Backend::open(&root, BackendKind::Files).await.unwrap();
    let (state, commit) = empty.prepare_reference(&common::initial()).unwrap();
    backend.append(&commit, &state, &|_| {}).await.unwrap();
    backend.checkpoint(&state).unwrap();
    let mut reader = backend
        .history(&root, &state, Watermark::ZERO)
        .await
        .unwrap();
    let (origin, _, _) = Origin::stage(&mut pages, &state, None, &mut reader)
        .await
        .unwrap();
    reader.close().await.unwrap();
    backend.close().await.unwrap();
    let mut journal = reopen(&root);
    let mut diagnostics = StoreDiagnostics::new(BackendKind::Files);
    journal
        .replay_current_all(&mut pages, &origin, None, &mut diagnostics)
        .await
        .unwrap();
    assert_eq!(diagnostics.checkpoint_state_comparisons, 1);
    drop(journal);
    let path = root.join("checkpoint-00000000000000000001.active");
    let mut seal: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    seal["chain"] = serde_json::Value::String("e".repeat(64));
    fs::write(&path, canonical_bytes(&seal).unwrap()).unwrap();
    let mut journal = reopen(&root);
    assert!(matches!(
        journal
            .replay_current_all(
                &mut pages,
                &origin,
                None,
                &mut StoreDiagnostics::new(BackendKind::Files)
            )
            .await,
        Err(Error::Corruption("checkpoint journal boundary"))
    ));
}

#[tokio::test]
async fn migrated_sqlite_cold_open_checks_every_commit_publication_and_materialized_row() {
    use crate::history_index::io::Sqlite;
    let temp = tempfile::tempdir().unwrap();
    let (mut backend, empty) = Backend::open(temp.path(), BackendKind::Sqlite)
        .await
        .unwrap();
    let (mut expected, commit) = empty.prepare_reference(&common::initial()).unwrap();
    backend.append(&commit, &expected, &|_| {}).await.unwrap();
    let mut reader = backend
        .history(temp.path(), &expected, Watermark::ZERO)
        .await
        .unwrap();
    let Backend::Sqlite(db) = &mut backend else {
        unreachable!()
    };
    initialize_sqlite(db).await.unwrap();
    let (_, digest, mut owner) = Origin::stage(&mut Sqlite::new(db), &expected, None, &mut reader)
        .await
        .unwrap();
    reader.close().await.unwrap();
    for index in 1..=2 {
        let tx = next(&expected, index);
        let crate::durable_owner::Outcome::Prepared(prepared) =
            owner.prepare(&mut Sqlite::new(db), &tx).await.unwrap()
        else {
            panic!("prepared")
        };
        let payload = serde_json::to_vec_pretty(prepared.commit()).unwrap();
        owner = append_sqlite(db, &owner, &prepared, &payload, &|_| {})
            .await
            .unwrap();
        expected = expected.prepare_reference(&tx).unwrap().0;
    }
    let identity = owner.identity().to_owned();
    drop(owner);
    backend.close().await.unwrap();
    let options = SqliteConnectOptions::new()
        .filename(temp.path().join("canonical.sqlite"))
        .create_if_missing(false)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .foreign_keys(true)
        .busy_timeout(Duration::from_millis(100));
    let mut db = SqliteConnection::connect_with(&options).await.unwrap();
    let origin = Origin::load(&mut Sqlite::new(&mut db), &digest)
        .await
        .unwrap();
    let mut diagnostics = StoreDiagnostics::new(BackendKind::Sqlite);
    let owner = replay_sqlite_all(&mut db, &origin, None, &mut diagnostics)
        .await
        .unwrap();
    assert_eq!(owner.identity(), identity);
    assert_eq!(
        owner.semantic().current(),
        &crate::CurrentState::from_state(&expected)
    );
    assert_eq!(diagnostics.replayed_commits, 3);
    owner
        .semantic()
        .catalog()
        .verify_replayed_state(&mut Sqlite::new(&mut db), &expected)
        .await
        .unwrap();
    // The native read verifier never manufactures a missing root and checks all
    // original bodies as well as every final materialized row and receipt.
    for statement in [
        "DELETE FROM history_publications WHERE watermark=2",
        "UPDATE history_publications SET digest=printf('%064d',0) WHERE watermark=2",
        "UPDATE commits SET payload=x'7b7d' WHERE watermark=2",
        "UPDATE records SET digest=printf('%064d',0) WHERE key=(SELECT min(key) FROM records)",
        "UPDATE events SET payload=x'7b7d' WHERE id='created'",
        "UPDATE commands SET payload=x'7b7d' WHERE id=(SELECT min(id) FROM commands)",
        "DELETE FROM events WHERE id='created'",
    ] {
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query(statement).execute(&mut db).await.unwrap();
        assert!(
            replay_sqlite_all(
                &mut db,
                &origin,
                None,
                &mut StoreDiagnostics::new(BackendKind::Sqlite)
            )
            .await
            .is_err(),
            "{statement}"
        );
        sqlx::query("ROLLBACK").execute(&mut db).await.unwrap();
    }
    assert_eq!(
        replay_sqlite_all(
            &mut db,
            &origin,
            None,
            &mut StoreDiagnostics::new(BackendKind::Sqlite)
        )
        .await
        .unwrap()
        .identity(),
        identity
    );
    db.close().await.unwrap();
}

#[tokio::test]
async fn migrated_files_origin_accepts_empty_prefix_and_retained_replay_base_without_invented_commits(
) {
    for retained in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        let forbidden = temp.path().join("workspace");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&forbidden).unwrap();
        fs::create_dir(root.join("history")).unwrap();
        let directory = Directory::open(&root.join("history"), &[forbidden]).unwrap();
        let mut pages = Files::new(&directory);
        if retained {
            let (state, _) = State::default()
                .prepare_reference(&common::initial())
                .unwrap();
            crate::replay_base::ReplayBase::write(&root, &state, &state, &[]).unwrap();
        }
        let base = crate::replay_base::ReplayBase::load(&root).unwrap();
        let (backend, state) = Backend::open(&root, BackendKind::Files).await.unwrap();
        let mut reader = backend
            .history(&root, &state, state.watermark)
            .await
            .unwrap();
        let (origin, _, owner) = Origin::stage(&mut pages, &state, base.as_ref(), &mut reader)
            .await
            .unwrap();
        reader.close().await.unwrap();
        backend.close().await.unwrap();
        assert_eq!(owner.originals().root().count(), 0);
        let mut journal = reopen(&root);
        journal.base_watermark = state.watermark;
        journal.initial_chain = base
            .as_ref()
            .map(|base| base.chain().unwrap())
            .unwrap_or_else(|| "0".repeat(64));
        let reopened = journal
            .replay_current_all(
                &mut pages,
                &origin,
                base.as_ref(),
                &mut StoreDiagnostics::new(BackendKind::Files),
            )
            .await
            .unwrap();
        assert_eq!(owner.identity(), reopened.identity());
        if retained {
            assert!(reopened
                .originals()
                .get(&mut pages, Watermark::new(1))
                .await
                .is_err());
        }
    }
}
