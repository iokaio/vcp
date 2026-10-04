// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::collections::BTreeMap;
use vcp_domain::TransactionId;
#[path = "../tests/common/mod.rs"]
mod common;
#[derive(Default)]
struct Memory {
    objects: BTreeMap<String, Vec<u8>>,
    writes: usize,
    fail_at: Option<usize>,
}
impl Pages for Memory {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        let bytes = self
            .objects
            .get(digest)
            .ok_or(Error::Corruption("owner fixture missing"))?;
        if bytes.len() > limit {
            return Err(Error::Limit("owner fixture object"));
        }
        Ok(bytes.clone())
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        self.writes += 1;
        if self.fail_at.is_some_and(|n| self.writes >= n) {
            return Err(Error::Unavailable("owner fixture write"));
        }
        if let Some(prior) = self.objects.get(digest) {
            assert_eq!(prior, bytes);
        } else {
            self.objects.insert(digest.into(), bytes.into());
        }
        Ok(())
    }
}
async fn owner(pages: &mut Memory, state: &State, commit: &Commit, pretty: bool) -> DurableOwner {
    let original = if pretty {
        serde_json::to_vec_pretty(commit).unwrap()
    } else {
        canonical_bytes(commit).unwrap()
    };
    let originals = OriginalCommits::from_validated_base(pages, None)
        .await
        .unwrap()
        .append_verified(pages, &original, commit)
        .await
        .unwrap();
    let catalog = Catalog::from_validated_state(pages, state).await.unwrap();
    DurableOwner::from_replayed(pages, state, None, catalog, originals)
        .await
        .unwrap()
}
fn next(state: &State) -> Transaction {
    let mut transaction = common::initial();
    transaction.id = TransactionId::new();
    transaction.expected_watermark = state.watermark;
    transaction.mutations.clear();
    transaction.events.clear();
    transaction.command = None;
    transaction
}
#[tokio::test]
async fn joint_identity_rejects_certificate_for_different_original_spelling() {
    let (state, commit) = State::default().prepare(&common::initial()).unwrap();
    let mut pages = Memory::default();
    let first = owner(&mut pages, &state, &commit, false).await;
    let second = owner(&mut pages, &state, &commit, true).await;
    assert_eq!(first.semantic().identity(), second.semantic().identity());
    assert_ne!(first.identity(), second.identity());
    let tx = next(&state);
    let Outcome::Prepared(prepared) = first.prepare(&mut pages, &tx).await.unwrap() else {
        panic!("new commit");
    };
    let payload = canonical_bytes(prepared.commit()).unwrap();
    let writes = pages.writes;
    assert!(matches!(
        second.advance(&mut pages, &prepared, &payload).await,
        Err(Error::Conflict("prepared durable source differs"))
    ));
    assert_eq!(pages.writes, writes);
    let advanced = first
        .advance(&mut pages, &prepared, &payload)
        .await
        .unwrap();
    let (expected, expected_commit) = state.prepare(&tx).unwrap();
    advanced
        .semantic()
        .catalog()
        .verify_replayed_state(&mut pages, &expected)
        .await
        .unwrap();
    assert_eq!(
        advanced
            .originals()
            .get(&mut pages, expected.watermark)
            .await
            .unwrap()
            .unwrap()
            .bytes,
        payload
    );
    assert_eq!(prepared.commit(), &expected_commit);
    assert!(
        matches!(advanced.prepare(&mut pages, &tx).await.unwrap(), Outcome::Duplicate(ref value) if value == &expected_commit.receipt)
    );
    assert_eq!(first.semantic().current().watermark, state.watermark);
}
#[tokio::test]
async fn joint_admission_rejects_foreign_originals_and_partial_writes_do_not_advance_owner() {
    let (state, commit) = State::default().prepare(&common::initial()).unwrap();
    let mut pages = Memory::default();
    let admitted = owner(&mut pages, &state, &commit, false).await;
    let mut foreign_tx = common::initial();
    foreign_tx.events[0].data = serde_json::json!({"foreign": true});
    let (_, foreign) = State::default().prepare(&foreign_tx).unwrap();
    let originals = OriginalCommits::from_validated_base(&mut pages, None)
        .await
        .unwrap()
        .append_verified(&mut pages, &canonical_bytes(&foreign).unwrap(), &foreign)
        .await
        .unwrap();
    assert!(DurableOwner::from_replayed(
        &mut pages,
        &state,
        None,
        admitted.semantic().catalog().clone(),
        originals
    )
    .await
    .is_err());
    let Outcome::Prepared(prepared) = admitted.prepare(&mut pages, &next(&state)).await.unwrap()
    else {
        panic!("new commit");
    };
    let identity = admitted.identity().to_owned();
    pages.fail_at = Some(pages.writes + 2);
    assert!(admitted
        .advance(
            &mut pages,
            &prepared,
            &canonical_bytes(prepared.commit()).unwrap()
        )
        .await
        .is_err());
    pages.fail_at = None;
    assert_eq!(admitted.identity(), identity);
    assert_eq!(admitted.originals().watermark(), state.watermark);
    admitted
        .semantic()
        .catalog()
        .verify_replayed_state(&mut pages, &state)
        .await
        .unwrap();
    assert!(admitted
        .advance(
            &mut pages,
            &prepared,
            &canonical_bytes(prepared.commit()).unwrap()
        )
        .await
        .is_ok());
}
#[tokio::test]
async fn retained_boundary_must_match_loaded_base_and_preserves_prefix_receipts() {
    let state = State::default().prepare(&common::initial()).unwrap().0;
    let temp = tempfile::tempdir().unwrap();
    ReplayBase::write(temp.path(), &state, &state, &[]).unwrap();
    let base = ReplayBase::load(temp.path()).unwrap().unwrap();
    let original = std::fs::read(temp.path().join("replay-base.json")).unwrap();
    let mut pages = Memory::default();
    let originals = OriginalCommits::from_validated_base(&mut pages, Some((&base, &original)))
        .await
        .unwrap();
    let catalog = Catalog::from_validated_state(&mut pages, &state)
        .await
        .unwrap();
    assert!(DurableOwner::from_replayed(
        &mut pages,
        &state,
        None,
        catalog.clone(),
        originals.clone()
    )
    .await
    .is_err());
    let mut other = base.clone();
    other.source_digest = "c".repeat(64);
    assert!(DurableOwner::from_replayed(
        &mut pages,
        &state,
        Some(&other),
        catalog.clone(),
        originals.clone()
    )
    .await
    .is_err());
    let admitted = DurableOwner::from_replayed(&mut pages, &state, Some(&base), catalog, originals)
        .await
        .unwrap();
    assert_eq!(admitted.originals().base_watermark(), state.watermark);
    assert!(admitted
        .originals()
        .get(&mut pages, state.watermark)
        .await
        .is_err());
}
