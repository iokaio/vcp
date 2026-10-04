// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{history_index::io::Files, private_paths::Directory, StoreDiagnostics};
use vcp_domain::{CommandId, EventId, TransactionId};
#[path = "../tests/common/mod.rs"]
mod common;

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    directory: Directory,
    backend: Backend,
    origin: Origin,
    owner: DurableOwner,
}
impl Fixture {
    async fn new(pretty: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("history")).unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir(&workspace).unwrap();
        let directory = Directory::open(&root.join("history"), &[workspace]).unwrap();
        let mut pages = Files::new(&directory);
        let (mut backend, empty) = Backend::open(&root, BackendKind::Files).await.unwrap();
        let (state, commit) = empty.prepare_reference(&common::initial()).unwrap();
        let bytes = if pretty {
            serde_json::to_vec_pretty(&commit).unwrap()
        } else {
            canonical_bytes(&commit).unwrap()
        };
        let Backend::Files(journal) = &mut backend else {
            unreachable!()
        };
        journal.append(&bytes, state.watermark, &|_| {}).unwrap();
        let mut reader = backend
            .history(&root, &state, Watermark::ZERO)
            .await
            .unwrap();
        let (origin, _, owner) = Origin::stage(&mut pages, &state, None, &mut reader)
            .await
            .unwrap();
        reader.close().await.unwrap();
        Self {
            _temp: temp,
            root,
            directory,
            backend,
            origin,
            owner,
        }
    }

    async fn append(&mut self, index: u64) {
        let mut tx = common::initial();
        tx.id = TransactionId::parse(format!("checkpoint-{index}")).unwrap();
        tx.expected_watermark = self.owner.semantic().current().watermark;
        tx.mutations.clear();
        tx.events[0].id = EventId::parse(format!("checkpoint-event-{index}")).unwrap();
        tx.events[0].correlation = CommandId::parse(format!("checkpoint-command-{index}")).unwrap();
        tx.command.as_mut().unwrap().command = tx.events[0].correlation.clone();
        tx.command.as_mut().unwrap().digest = format!("{index:064x}");
        let mut pages = Files::new(&self.directory);
        let crate::durable_owner::Outcome::Prepared(prepared) =
            self.owner.prepare(&mut pages, &tx).await.unwrap()
        else {
            panic!("prepared");
        };
        let bytes = canonical_bytes(prepared.commit()).unwrap();
        let (next, publication) =
            crate::history_publication::stage(&mut pages, &self.owner, &prepared, &bytes)
                .await
                .unwrap();
        let Backend::Files(journal) = &mut self.backend else {
            unreachable!()
        };
        journal
            .append_current(
                &bytes,
                next.semantic().current().watermark,
                &publication,
                &|_| {},
            )
            .unwrap();
        self.owner = next;
    }

    fn checkpoint(&mut self) {
        self.backend.checkpoint_current(&self.owner).unwrap();
    }

    async fn replay(&self) -> Result<(DurableOwner, StoreDiagnostics)> {
        let mut journal = Journal {
            file: OpenOptions::new()
                .read(true)
                .write(true)
                .open(self.root.join("canonical.frames"))?,
            root: self.root.clone(),
            chain: "0".repeat(64),
            initial_chain: "0".repeat(64),
            base_watermark: Watermark::ZERO,
            write_budget: None,
        };
        let mut pages = Files::new(&self.directory);
        let mut diagnostics = StoreDiagnostics::new(BackendKind::Files);
        let owner = journal
            .replay_current_all(&mut pages, &self.origin, None, &mut diagnostics)
            .await?;
        Ok((owner, diagnostics))
    }

    fn descriptor(&self) -> PathBuf {
        self.root
            .join(name(self.owner.semantic().current().watermark, "json"))
    }
    fn seal(&self) -> PathBuf {
        self.root
            .join(name(self.owner.semantic().current().watermark, "active"))
    }

    fn replace_descriptor(&self, edit: impl FnOnce(&mut serde_json::Value)) {
        let mut descriptor: serde_json::Value =
            serde_json::from_slice(&fs::read(self.descriptor()).unwrap()).unwrap();
        edit(&mut descriptor);
        let bytes = canonical_bytes(&descriptor).unwrap();
        fs::write(self.descriptor(), &bytes).unwrap();
        let mut seal: serde_json::Value =
            serde_json::from_slice(&fs::read(self.seal()).unwrap()).unwrap();
        seal["sha256"] = digest_bytes(&bytes).into();
        fs::write(self.seal(), canonical_bytes(&seal).unwrap()).unwrap();
    }
}

#[tokio::test]
async fn current_checkpoint_repeat_origin_and_suffix_still_replay_every_commit() {
    let mut fixture = Fixture::new(true).await;
    fixture.checkpoint();
    let original = fs::read(fixture.descriptor()).unwrap();
    let original_seal = fs::read(fixture.seal()).unwrap();
    fixture.checkpoint();
    assert_eq!(fs::read(fixture.descriptor()).unwrap(), original);
    assert_eq!(fs::read(fixture.seal()).unwrap(), original_seal);
    let (owner, diagnostics) = fixture.replay().await.unwrap();
    assert_eq!(owner.identity(), fixture.owner.identity());
    assert_eq!(diagnostics.replayed_commits, 1);
    assert_eq!(diagnostics.checkpoint_state_comparisons, 1);
    fixture.append(2).await;
    fixture.checkpoint();
    fixture.append(3).await;
    let (owner, diagnostics) = fixture.replay().await.unwrap();
    assert_eq!(owner.identity(), fixture.owner.identity());
    assert_eq!(diagnostics.replayed_commits, 3);
    assert_eq!(diagnostics.checkpoint_state_comparisons, 1);
}

#[tokio::test]
async fn current_checkpoint_rejects_validly_resealed_wrong_identity_projection_chain_and_extent() {
    for field in ["owner", "current", "chain", "end"] {
        let mut fixture = Fixture::new(false).await;
        fixture.checkpoint();
        fixture.replace_descriptor(|descriptor| {
            descriptor[field] = if field == "end" {
                "0".into()
            } else {
                "f".repeat(64).into()
            }
        });
        assert!(fixture.replay().await.is_err(), "{field}");
        let bytes = fs::read(fixture.descriptor()).unwrap();
        assert!(fixture.backend.checkpoint_current(&fixture.owner).is_err());
        assert_eq!(fs::read(fixture.descriptor()).unwrap(), bytes);
    }
}

#[tokio::test]
async fn current_checkpoint_binds_original_spelling_even_with_same_current_projection() {
    let mut canonical = Fixture::new(false).await;
    let pretty = Fixture::new(true).await;
    assert_eq!(
        canonical.owner.semantic().current(),
        pretty.owner.semantic().current()
    );
    assert_ne!(canonical.owner.identity(), pretty.owner.identity());
    canonical.checkpoint();
    fs::copy(canonical.descriptor(), pretty.descriptor()).unwrap();
    fs::copy(canonical.seal(), pretty.seal()).unwrap();
    assert!(pretty.replay().await.is_err());
}

#[tokio::test]
async fn current_checkpoint_unpublished_descriptor_is_ignored_but_bad_active_seal_fails_closed() {
    let mut fixture = Fixture::new(false).await;
    fs::write(fixture.descriptor(), b"unfinished descriptor").unwrap();
    assert_eq!(
        fixture
            .replay()
            .await
            .unwrap()
            .1
            .checkpoint_state_comparisons,
        0
    );
    // No replacement of an unpublished collision is permitted.
    assert!(fixture.backend.checkpoint_current(&fixture.owner).is_err());
    fs::remove_file(fixture.descriptor()).unwrap();
    fixture.checkpoint();
    let seal = fs::read(fixture.seal()).unwrap();
    fs::write(fixture.seal(), &seal[..seal.len() / 2]).unwrap();
    assert!(fixture.replay().await.is_err());
    assert!(fixture.backend.checkpoint_current(&fixture.owner).is_err());
    fs::write(fixture.seal(), seal).unwrap();
    fs::write(fixture.descriptor(), b"corrupted descriptor").unwrap();
    assert!(fixture.replay().await.is_err());
}

#[tokio::test]
async fn current_checkpoint_ahead_of_history_or_missing_history_pages_cannot_authorize_open() {
    let mut fixture = Fixture::new(false).await;
    fixture.checkpoint();
    let mut seal: Seal = serde_json::from_slice(&fs::read(fixture.seal()).unwrap()).unwrap();
    let mut descriptor: CurrentCheckpoint =
        serde_json::from_slice(&fs::read(fixture.descriptor()).unwrap()).unwrap();
    descriptor.watermark = Watermark::new(10);
    seal.watermark = descriptor.watermark;
    seal.file = name(descriptor.watermark, "json");
    let bytes = canonical_bytes(&descriptor).unwrap();
    seal.sha256 = digest_bytes(&bytes);
    fs::write(fixture.root.join(&seal.file), bytes).unwrap();
    let future_seal = fixture.root.join(name(descriptor.watermark, "active"));
    fs::write(&future_seal, canonical_bytes(&seal).unwrap()).unwrap();
    assert!(fixture.replay().await.is_err());
    fs::remove_file(future_seal).unwrap();
    // A valid checkpoint cannot replace any original/history page required by
    // semantic replay. Remove only fixture-owned immutable page files.
    for entry in fs::read_dir(fixture.root.join("history")).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            fs::remove_file(path).unwrap();
        }
    }
    assert!(fixture.replay().await.is_err());
}

#[tokio::test]
async fn current_checkpoint_protects_original_journal_extent_without_relying_on_tip() {
    let mut fixture = Fixture::new(false).await;
    fixture.append(2).await;
    fixture.checkpoint();
    fs::remove_file(fixture.root.join("commit-00000000000000000002.tip")).unwrap();
    let path = fixture.root.join("canonical.frames");
    let length = fs::metadata(&path).unwrap().len();
    OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(length - 1)
        .unwrap();
    assert!(fixture.replay().await.is_err());
    assert_eq!(fs::metadata(&path).unwrap().len(), length - 1);
}

#[tokio::test]
async fn current_checkpoint_coexists_with_legacy_prefix_checkpoint_and_sqlite_remains_noop() {
    let mut fixture = Fixture::new(false).await;
    let state = fixture
        .owner
        .archive_state(&mut Files::new(&fixture.directory))
        .await
        .unwrap();
    fixture.backend.checkpoint(&state).unwrap();
    let legacy = fs::read(fixture.root.join("checkpoint-00000000000000000001.active")).unwrap();
    fixture.checkpoint();
    fixture.append(2).await;
    fixture.checkpoint();
    let (owner, diagnostics) = fixture.replay().await.unwrap();
    assert_eq!(owner.identity(), fixture.owner.identity());
    assert_eq!(diagnostics.replayed_commits, 2);
    assert_eq!(diagnostics.checkpoint_state_comparisons, 2);
    assert_eq!(
        fs::read(fixture.root.join("checkpoint-00000000000000000001.active")).unwrap(),
        legacy
    );

    let temp = tempfile::tempdir().unwrap();
    let (mut sqlite, _) = Backend::open(temp.path(), BackendKind::Sqlite)
        .await
        .unwrap();
    sqlite.checkpoint_current(&fixture.owner).unwrap();
    assert!(!fs::read_dir(temp.path()).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with("history-checkpoint-")));
    sqlite.close().await.unwrap();
}
