// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::portable_snapshot::wire::IndexedPages;
use std::collections::BTreeMap;
use vcp_domain::{EventId, TaskId, TransactionId};
#[path = "../tests/common/mod.rs"]
mod common;
#[derive(Default)]
struct Memory(BTreeMap<String, Vec<u8>>);
impl Pages for Memory {
    async fn read(&mut self, key: &str, limit: usize) -> Result<Vec<u8>> {
        let bytes = self
            .0
            .get(key)
            .ok_or(Error::Corruption("canonical fixture page missing"))?;
        if bytes.len() > limit {
            return Err(Error::Limit("canonical fixture page"));
        }
        Ok(bytes.clone())
    }
    async fn write(&mut self, key: &str, bytes: &[u8]) -> Result<()> {
        if let Some(old) = self.0.insert(key.into(), bytes.into()) {
            assert_eq!(old, bytes);
        }
        Ok(())
    }
}
async fn fixture(legacy: bool) -> (Memory, DurableOwner, State) {
    let mut state = State::default();
    let temp = tempfile::tempdir().unwrap();
    let (base, bytes) = if legacy {
        state = state.prepare(&common::initial()).unwrap().0;
        ReplayBase::write(temp.path(), &state, &state, &[]).unwrap();
        let bytes = std::fs::read(temp.path().join("replay-base.json")).unwrap();
        (Some(ReplayBase::decode(&bytes).unwrap()), Some(bytes))
    } else {
        (None, None)
    };
    let mut pages = Memory::default();
    let catalog = Catalog::from_validated_state(&mut pages, &state)
        .await
        .unwrap();
    let originals =
        OriginalCommits::from_validated_base(&mut pages, base.as_ref().zip(bytes.as_deref()))
            .await
            .unwrap();
    let mut owner =
        DurableOwner::from_replayed(&mut pages, &state, base.as_ref(), catalog, originals)
            .await
            .unwrap();
    for index in 0..3 {
        let mut tx = common::initial();
        if state.watermark != Watermark::ZERO {
            tx.id = TransactionId::new();
            tx.expected_watermark = state.watermark;
            tx.mutations.clear();
            tx.command = None;
            tx.events[0].id = EventId::new();
            tx.events[0].data = serde_json::json!({"index":index,"text":"é漢字\n\\"});
        }
        let (next, expected) = state.prepare(&tx).unwrap();
        let Outcome::Prepared(prepared) = owner.prepare(&mut pages, &tx).await.unwrap() else {
            panic!("new transition")
        };
        assert_eq!(prepared.commit(), &expected);
        owner = owner
            .advance(
                &mut pages,
                &prepared,
                &serde_json::to_vec_pretty(&expected).unwrap(),
            )
            .await
            .unwrap();
        state = next;
    }
    (pages, owner, state)
}

#[tokio::test]
async fn canonical_component_captures_and_fully_replays_exact_genesis_and_retained_history() {
    for legacy in [false, true] {
        let (mut source, owner, expected) = fixture(legacy).await;
        let mut staged = Memory::default();
        let mut indexed = IndexedPages::new(&mut staged);
        let captured = Canonical::capture(
            &owner,
            &mut source,
            &mut indexed,
            &common::workspace().id,
            &|| Ok(()),
        )
        .await
        .unwrap();
        let objects = indexed.finish().unwrap();
        assert!(objects.count() > 0);
        let root = canonical_bytes(&captured).unwrap();
        assert!(root.len() < 128 * 1024);
        let captured: Canonical = serde_json::from_slice(&root).unwrap();
        assert_eq!(
            captured.current.sha256,
            crate::CurrentState::from_state(&expected)
                .projection_digest()
                .unwrap()
        );
        source.0.clear();
        drop(owner);
        let mut rebuilt = Memory::default();
        let restored = captured
            .replay(&mut staged, &mut rebuilt, &|| Ok(()))
            .await
            .unwrap();
        assert_eq!(
            restored.semantic().current(),
            &crate::CurrentState::from_state(&expected)
        );
        assert_eq!(
            restored
                .semantic()
                .catalog()
                .legacy_digest(&mut rebuilt, restored.semantic().current().into())
                .await
                .unwrap(),
            crate::legacy_state_stream::digest(&expected).unwrap()
        );
        if legacy {
            assert!(restored
                .originals()
                .get(&mut rebuilt, Watermark::new(1))
                .await
                .is_err());
        }
        for watermark in captured.originals.base_watermark().get() + 1..=captured.watermark.get() {
            let watermark = Watermark::new(watermark);
            assert_eq!(
                captured
                    .originals
                    .get(&mut staged, watermark)
                    .await
                    .unwrap()
                    .unwrap()
                    .bytes,
                restored
                    .originals()
                    .get(&mut rebuilt, watermark)
                    .await
                    .unwrap()
                    .unwrap()
                    .bytes
            );
        }
    }
}

#[tokio::test]
async fn valid_claimed_head_never_hides_invalid_interior_transition_or_missing_pages() {
    let (mut source, owner, _) = fixture(false).await;
    let mut staged = Memory::default();
    let mut captured = Canonical::capture(
        &owner,
        &mut source,
        &mut staged,
        &common::workspace().id,
        &|| Ok(()),
    )
    .await
    .unwrap();
    let mut forged = OriginalCommits::from_validated_base(&mut staged, None)
        .await
        .unwrap();
    for ordinal in 1..=captured.watermark.get() {
        let mut row = captured
            .originals
            .get(&mut staged, Watermark::new(ordinal))
            .await
            .unwrap()
            .unwrap();
        if ordinal == 2 {
            row.commit.transaction.events[0].task = Some(TaskId::parse("missing-task").unwrap());
            row.commit.receipt.digest =
                vcp_protocol::digest_bytes(&canonical_bytes(&row.commit.transaction).unwrap());
        }
        forged = forged
            .append_verified(
                &mut staged,
                &canonical_bytes(&row.commit).unwrap(),
                &row.commit,
            )
            .await
            .unwrap();
    }
    // This models authenticated-but-untrusted archive metadata. The unchanged
    // claimed final current/catalog cannot excuse an invalid retained interior.
    captured.originals = forged;
    let error = captured
        .replay(&mut staged, &mut Memory::default(), &|| Ok(()))
        .await
        .err()
        .unwrap();
    assert!(error.to_string().contains("record not found"), "{error}");
    staged.0.clear();
    assert!(captured
        .replay(&mut staged, &mut Memory::default(), &|| Ok(()))
        .await
        .is_err());
    assert!(captured
        .replay(&mut source, &mut Memory::default(), &|| Err(
            Error::Unavailable("cancelled")
        ))
        .await
        .is_err());
}
