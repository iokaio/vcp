// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::contract::State;
use std::{cell::Cell, collections::BTreeMap};
#[path = "../tests/common/mod.rs"]
mod common;
#[derive(Default, Clone)]
struct Memory {
    objects: BTreeMap<String, Vec<u8>>,
    fail: bool,
}
impl Pages for Memory {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        let bytes = self
            .objects
            .get(digest)
            .ok_or(Error::Corruption("original copy missing page"))?;
        if bytes.len() > limit {
            return Err(Error::Limit("original copy page"));
        }
        Ok(bytes.clone())
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        if self.fail {
            return Err(Error::Unavailable("original copy destination"));
        }
        assert!(bytes.len() <= 128 * 1024);
        if let Some(prior) = self.objects.insert(digest.into(), bytes.into()) {
            assert_eq!(prior, bytes);
        }
        Ok(())
    }
}
async fn fixture(
    legacy: bool,
) -> (
    Memory,
    OriginalCommits,
    State,
    Vec<Vec<u8>>,
    Option<Vec<u8>>,
) {
    let mut source = Memory::default();
    let mut state = State::default();
    let retained = tempfile::tempdir().unwrap();
    let (base, base_bytes) = if legacy {
        state = state.prepare(&common::initial()).unwrap().0;
        ReplayBase::write(retained.path(), &state, &state, &[]).unwrap();
        (
            ReplayBase::load(retained.path()).unwrap(),
            Some(std::fs::read(retained.path().join("replay-base.json")).unwrap()),
        )
    } else {
        (None, None)
    };
    let mut original =
        OriginalCommits::from_validated_base(&mut source, base.as_ref().zip(base_bytes.as_deref()))
            .await
            .unwrap();
    let mut expected = Vec::new();
    for index in 0..35 {
        let mut tx = common::initial();
        if state.watermark != Watermark::ZERO {
            tx.id = TransactionId::new();
            tx.command = None;
            tx.mutations.clear();
            tx.expected_watermark = state.watermark;
            tx.events[0].id = vcp_domain::EventId::new();
        }
        let (next, commit) = state.prepare(&tx).unwrap();
        let bytes = if index % 2 == 0 {
            serde_json::to_vec_pretty(&commit).unwrap()
        } else {
            canonical_bytes(&commit).unwrap()
        };
        original = original
            .append_verified(&mut source, &bytes, &commit)
            .await
            .unwrap();
        expected.push(bytes);
        state = next;
    }
    (source, original, state, expected, base_bytes)
}

#[tokio::test]
async fn exact_genesis_and_retained_boundary_copy_remain_independently_replayable() {
    for legacy in [false, true] {
        let (mut source, original, expected_state, bytes, base_bytes) = fixture(legacy).await;
        let mut copied = Memory::default();
        original
            .copy_objects(&mut source, &mut copied, &|| Ok(()))
            .await
            .unwrap();
        assert!(
            copied.objects.len() < source.objects.len(),
            "old persistent index generations excluded"
        );
        source.objects.clear();
        let mut replay = if let Some(blob) = original.base_blob() {
            let copied_base =
                history_blob::read_bounded(&mut copied, blob, MAX_STATE_BYTES + 1024 * 1024)
                    .await
                    .unwrap();
            assert_eq!(Some(&copied_base), base_bytes.as_ref());
            let base: ReplayBase = serde_json::from_slice(&copied_base).unwrap();
            assert!(
                original
                    .get(&mut copied, base.state.watermark)
                    .await
                    .is_err(),
                "unavailable prefix is not invented"
            );
            base.state
        } else {
            State::default()
        };
        for expected in bytes {
            let row = original
                .get(&mut copied, replay.watermark.next().unwrap())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(row.bytes, expected, "original JSON spelling preserved");
            let (next, commit) = replay.prepare(&row.commit.transaction).unwrap();
            assert_eq!(commit, row.commit);
            replay = next;
        }
        assert_eq!(replay, expected_state);
    }
}

#[tokio::test]
async fn forged_original_identity_failed_destination_and_cancelled_copy_reject() {
    let (source, original, _, _, _) = fixture(false).await;
    let mut working = source.clone();
    let mut failed = Memory {
        fail: true,
        ..Default::default()
    };
    assert!(original
        .copy_objects(&mut working, &mut failed, &|| Ok(()))
        .await
        .is_err());
    let calls = Cell::new(0);
    assert!(original
        .copy_objects(&mut working, &mut Memory::default(), &|| {
            calls.set(calls.get() + 1);
            if calls.get() == 5 {
                Err(Error::Unavailable("copy cancelled"))
            } else {
                Ok(())
            }
        })
        .await
        .is_err());
    let mut forged = original.clone();
    let mut rows = Root::empty(Table::CommitPayload);
    for mut entry in original.rows.page(&mut working, None, 64).await.unwrap() {
        entry.value["transaction"] = serde_json::json!("wrong-transaction");
        rows = rows.insert(&mut working, entry).await.unwrap();
    }
    forged.rows = rows;
    assert!(
        forged
            .copy_objects(&mut working, &mut Memory::default(), &|| Ok(()))
            .await
            .is_err(),
        "valid chunk hashes cannot replace row-to-commit identity checks"
    );
    for bytes in working.objects.values_mut() {
        bytes[0] ^= 1;
    }
    assert!(original
        .copy_objects(&mut working, &mut Memory::default(), &|| Ok(()))
        .await
        .is_err());
    working.objects.clear();
    assert!(original
        .copy_objects(&mut working, &mut Memory::default(), &|| Ok(()))
        .await
        .is_err());
    // Failed destinations and consumers have not changed source descriptors.
    let mut restored = source;
    assert!(original
        .get(&mut restored, Watermark::new(1))
        .await
        .unwrap()
        .is_some());
}
