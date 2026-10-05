// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::collections::BTreeMap;
#[path = "../tests/common/mod.rs"]
mod common;
#[derive(Default, Clone)]
struct Memory(BTreeMap<String, Vec<u8>>);
impl Pages for Memory {
    async fn read(&mut self, key: &str, limit: usize) -> Result<Vec<u8>> {
        let value = self
            .0
            .get(key)
            .ok_or(Error::Corruption("equivalence page missing"))?;
        if value.len() > limit {
            return Err(Error::Limit("equivalence page"));
        }
        Ok(value.clone())
    }
    async fn write(&mut self, key: &str, bytes: &[u8]) -> Result<()> {
        if let Some(old) = self.0.insert(key.into(), bytes.into()) {
            assert_eq!(old, bytes);
        }
        Ok(())
    }
}
#[tokio::test]
async fn equivalent_catalogs_accept_different_index_shapes_and_reject_wrong_logical_rows() {
    let mut initial = common::initial();
    let first = initial.events[0].clone();
    initial.events = (0..70)
        .map(|index| {
            let mut row = first.clone();
            row.id = EventId::parse(format!("equivalence-{index:03}")).unwrap();
            row
        })
        .collect();
    let state = State::default().prepare(&initial).unwrap().0;
    let mut reference = Memory::default();
    let expected = Catalog::from_validated_state(&mut reference, &state)
        .await
        .unwrap();
    let mut source = reference.clone();
    let mut actual = expected.clone();
    let entries = expected
        .identities
        .page(&mut reference, None, 100)
        .await
        .unwrap();
    actual.identities = Root::empty(Table::EventIdentity);
    for row in entries.iter().rev() {
        actual.identities = actual
            .identities
            .insert(&mut source, row.clone())
            .await
            .unwrap();
    }
    assert_ne!(
        canonical_bytes(&actual.identities).unwrap(),
        canonical_bytes(&expected.identities).unwrap()
    );
    actual
        .verify_replayed_catalog(&mut source, &expected, &mut reference, &|| Ok(()))
        .await
        .unwrap();
    let mut bad = actual.clone();
    bad.identities = Root::empty(Table::EventIdentity);
    for (index, mut row) in entries.into_iter().enumerate() {
        if index == 0 {
            row.value = serde_json::json!(69);
        }
        bad.identities = bad.identities.insert(&mut source, row).await.unwrap();
    }
    assert!(bad
        .verify_replayed_catalog(&mut source, &expected, &mut reference, &|| Ok(()))
        .await
        .is_err());
    let mut bad = actual.clone();
    bad.watermark = bad.watermark.next().unwrap();
    assert!(bad
        .verify_replayed_catalog(&mut source, &expected, &mut reference, &|| Ok(()))
        .await
        .is_err());
    for bytes in source.0.values_mut() {
        bytes[0] ^= 1;
    }
    assert!(actual
        .verify_replayed_catalog(&mut source, &expected, &mut reference, &|| Ok(()))
        .await
        .is_err());
}
