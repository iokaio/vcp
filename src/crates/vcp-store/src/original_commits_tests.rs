// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::contract::State;
use std::collections::BTreeMap;
use vcp_domain::{EventId, TransactionId};
#[path = "../tests/common/mod.rs"]
mod common;

#[derive(Default, Clone)]
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
            .ok_or(Error::Corruption("original fixture missing page"))?;
        if bytes.len() > limit {
            return Err(Error::Limit("original fixture page"));
        }
        Ok(bytes.clone())
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        self.writes += 1;
        if self.fail_at.is_some_and(|at| self.writes >= at) {
            return Err(Error::Unavailable("original fixture write"));
        }
        if let Some(existing) = self.objects.get(digest) {
            assert_eq!(existing, bytes);
        } else {
            self.objects.insert(digest.into(), bytes.into());
        }
        Ok(())
    }
}

#[tokio::test]
async fn exact_original_bytes_survive_multiple_index_pages_and_replay() {
    let mut pages = Memory::default();
    let mut originals = OriginalCommits::from_validated_base(&mut pages, None)
        .await
        .unwrap();
    let mut state = State::default();
    let mut expected = Vec::new();
    for i in 0..70 {
        let mut transaction = common::initial();
        if i > 0 {
            transaction.id = TransactionId::parse(format!("original-{i}")).unwrap();
            transaction.expected_watermark = state.watermark;
            transaction.command = None;
            transaction.mutations.clear();
            transaction.events[0].id = EventId::parse(format!("original-event-{i}")).unwrap();
            transaction.events[0].data = serde_json::json!({"text": "Unicode: λ \"\n", "index": i});
        }
        let (next, commit) = state.prepare(&transaction).unwrap();
        let bytes = if i % 2 == 0 {
            serde_json::to_vec_pretty(&commit).unwrap()
        } else {
            canonical_bytes(&commit).unwrap()
        };
        originals = originals
            .append_verified(&mut pages, &bytes, &commit)
            .await
            .unwrap();
        expected.push(bytes);
        state = next;
    }
    let descriptor = canonical_bytes(&originals).unwrap();
    assert!(
        descriptor.len() < 2048,
        "descriptor must not grow with original body count"
    );
    let reopened: OriginalCommits = serde_json::from_slice(&descriptor).unwrap();
    let mut replay = State::default();
    for (i, bytes) in expected.iter().enumerate() {
        let row = reopened
            .get(&mut pages, Watermark::new(i as u64 + 1))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&row.bytes, bytes);
        let (next, recomputed) = replay.prepare(&row.commit.transaction).unwrap();
        assert_eq!(recomputed, row.commit);
        replay = next;
    }
    assert_eq!(state, replay);
    assert!(reopened
        .get(&mut pages, Watermark::new(71))
        .await
        .unwrap()
        .is_none());
    assert!(reopened.get(&mut pages, Watermark::ZERO).await.is_err());
}

#[tokio::test]
async fn source_mismatch_bad_domain_corruption_and_partial_writes_preserve_prior_root() {
    let mut pages = Memory::default();
    let genesis = OriginalCommits::from_validated_base(&mut pages, None)
        .await
        .unwrap();
    let (state, commit) = State::default().prepare(&common::initial()).unwrap();
    let bytes = canonical_bytes(&commit).unwrap();
    let mut wrong = commit.clone();
    wrong.receipt.digest = "b".repeat(64);
    assert!(genesis
        .append_verified(&mut pages, &bytes, &wrong)
        .await
        .is_err());
    assert_eq!(pages.writes, 0);
    pages.fail_at = Some(2);
    assert!(genesis
        .append_verified(&mut pages, &bytes, &commit)
        .await
        .is_err());
    assert_eq!(genesis.watermark(), Watermark::ZERO);
    assert_eq!(genesis.root().count(), 0);
    pages.fail_at = None;
    let next = genesis
        .append_verified(&mut pages, &bytes, &commit)
        .await
        .unwrap();
    assert_eq!(
        next.get(&mut pages, state.watermark)
            .await
            .unwrap()
            .unwrap()
            .bytes,
        bytes
    );
    let writes = pages.writes;
    assert!(next
        .append_verified(&mut pages, &bytes, &commit)
        .await
        .is_err());
    assert_eq!(pages.writes, writes);
    let mut descriptor = serde_json::to_value(&next).unwrap();
    descriptor["rows"]["table"] = serde_json::json!("commit");
    assert!(serde_json::from_value::<OriginalCommits>(descriptor)
        .unwrap()
        .validate()
        .is_err());
    for data in pages.objects.values_mut() {
        data[0] ^= 1;
    }
    assert!(next.get(&mut pages, state.watermark).await.is_err());
}

#[tokio::test]
async fn retained_base_preserves_exact_prefix_evidence_without_invented_bodies() {
    let state = State::default().prepare(&common::initial()).unwrap().0;
    let temp = tempfile::tempdir().unwrap();
    ReplayBase::write(temp.path(), &state, &state, &[]).unwrap();
    let base = ReplayBase::load(temp.path()).unwrap().unwrap();
    let bytes = std::fs::read(temp.path().join("replay-base.json")).unwrap();
    let mut pages = Memory::default();
    let originals = OriginalCommits::from_validated_base(&mut pages, Some((&base, &bytes)))
        .await
        .unwrap();
    assert_eq!(originals.base_watermark(), state.watermark);
    assert_eq!(originals.watermark(), state.watermark);
    assert_eq!(
        history_blob::read_bounded(
            &mut pages,
            originals.base_blob().unwrap(),
            MAX_STATE_BYTES + 1024 * 1024
        )
        .await
        .unwrap(),
        bytes
    );
    assert!(matches!(
        originals.get(&mut pages, state.watermark).await,
        Err(Error::Conflict(
            "original commit precedes retained boundary"
        ))
    ));
    assert!(
        OriginalCommits::from_validated_base(&mut pages, Some((&base, b"different")))
            .await
            .is_err()
    );
    let mut tx = common::initial();
    tx.id = TransactionId::new();
    tx.expected_watermark = state.watermark;
    tx.command = None;
    tx.events.clear();
    tx.mutations.clear();
    let (_, commit) = state.prepare(&tx).unwrap();
    let original = canonical_bytes(&commit).unwrap();
    let next = originals
        .append_verified(&mut pages, &original, &commit)
        .await
        .unwrap();
    assert_eq!(next.root().count(), 1);
    assert_eq!(
        next.get(&mut pages, commit.receipt.watermark)
            .await
            .unwrap()
            .unwrap()
            .bytes,
        original
    );
}
