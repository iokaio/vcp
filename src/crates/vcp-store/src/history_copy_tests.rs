// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{cell::Cell, collections::BTreeMap};
#[path = "../tests/common/mod.rs"]
mod common;

#[derive(Default)]
struct Memory {
    objects: BTreeMap<String, Vec<u8>>,
    maximum: usize,
    fail_write: bool,
}
impl Pages for Memory {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        let bytes = self
            .objects
            .get(digest)
            .ok_or(Error::Corruption("copy source missing"))?;
        if bytes.len() > limit {
            return Err(Error::Limit("copy source object"));
        }
        self.maximum = self.maximum.max(bytes.len());
        Ok(bytes.clone())
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        if self.fail_write {
            return Err(Error::Unavailable("copy destination unavailable"));
        }
        assert!(bytes.len() <= 128 * 1024);
        assert_eq!(vcp_protocol::digest_bytes(bytes), digest);
        if let Some(prior) = self.objects.insert(digest.into(), bytes.into()) {
            assert_eq!(prior, bytes);
        }
        Ok(())
    }
}

#[tokio::test]
async fn reachable_copy_preserves_all_catalog_queries_without_unreachable_old_pages() {
    let mut initial = common::initial();
    let event = initial.events[0].clone();
    initial.events = (0..70)
        .map(|i| {
            let mut row = event.clone();
            row.id = EventId::parse(format!("copy-{i}")).unwrap();
            row.data = serde_json::json!({"payload":"x".repeat(1600)});
            row
        })
        .collect();
    let state = State::default().prepare(&initial).unwrap().0;
    let mut source = Memory::default();
    let catalog = Catalog::from_validated_state(&mut source, &state)
        .await
        .unwrap();
    let original_pages = source.objects.len();
    let mut destination = Memory::default();
    catalog
        .copy_objects(&mut source, &mut destination, &|| Ok(()))
        .await
        .unwrap();
    assert!(
        destination.objects.len() < original_pages,
        "persistent insertion's unreachable prior index roots are excluded"
    );
    let copied_pages = destination.objects.len();
    catalog
        .copy_objects(&mut source, &mut destination, &|| Ok(()))
        .await
        .unwrap();
    assert_eq!(
        copied_pages,
        destination.objects.len(),
        "content-addressed destination deduplicates without resident seen map"
    );
    source.objects.clear();
    catalog
        .verify_replayed_state(&mut destination, &state)
        .await
        .unwrap();
    assert_eq!(
        catalog
            .legacy_digest(&mut destination, (&state).into())
            .await
            .unwrap(),
        crate::legacy_state_stream::digest(&state).unwrap()
    );
    for row in &state.events {
        assert_eq!(
            catalog
                .event(&mut destination, &row.event.id)
                .await
                .unwrap(),
            Some(row.clone())
        );
    }
    assert!(destination.maximum <= 128 * 1024);
}

#[tokio::test]
async fn incomplete_corrupt_or_cancelled_reachable_copy_never_succeeds() {
    let state = State::default().prepare(&common::initial()).unwrap().0;
    let mut source = Memory::default();
    let catalog = Catalog::from_validated_state(&mut source, &state)
        .await
        .unwrap();
    let mut forged = serde_json::to_value(&catalog.events).unwrap();
    forged["head"]["count"] = serde_json::json!(catalog.event_count() + 1);
    let forged: Root = serde_json::from_value(forged).unwrap();
    assert!(
        forged
            .copy_pages(&mut source, &mut Memory::default(), &|| Ok(()))
            .await
            .is_err(),
        "a valid page digest cannot satisfy a forged parent count"
    );
    let mut destination = Memory {
        fail_write: true,
        ..Default::default()
    };
    assert!(catalog
        .copy_objects(&mut source, &mut destination, &|| Ok(()))
        .await
        .is_err());
    let calls = Cell::new(0);
    let mut destination = Memory::default();
    assert!(catalog
        .copy_objects(&mut source, &mut destination, &|| {
            calls.set(calls.get() + 1);
            if calls.get() == 3 {
                Err(Error::Unavailable("copy cancelled"))
            } else {
                Ok(())
            }
        })
        .await
        .is_err());
    for bytes in source.objects.values_mut() {
        bytes[0] ^= 1;
    }
    assert!(catalog
        .copy_objects(&mut source, &mut destination, &|| Ok(()))
        .await
        .is_err());
    source.objects.clear();
    assert!(catalog
        .copy_objects(&mut source, &mut destination, &|| Ok(()))
        .await
        .is_err());
}

#[tokio::test]
async fn blob_copy_requires_complete_ordered_chunks_and_final_whole_digest() {
    let mut source = Memory::default();
    let bytes = vec![0x5a; history_blob::CHUNK_BYTES * 3 + 1];
    let blob = history_blob::write_bytes(&mut source, &bytes)
        .await
        .unwrap();
    let mut destination = Memory::default();
    blob.copy_objects(&mut source, &mut destination, &|| Ok(()))
        .await
        .unwrap();
    assert_eq!(
        history_blob::read_bounded(&mut destination, &blob, bytes.len())
            .await
            .unwrap(),
        bytes
    );
    let mut forged = blob.clone();
    forged.sha256 = "f".repeat(64);
    assert!(
        forged
            .copy_objects(&mut source, &mut Memory::default(), &|| Ok(()))
            .await
            .is_err(),
        "valid chunk hashes are insufficient without the complete blob commitment"
    );
}
