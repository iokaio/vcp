// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::collections::BTreeMap;
use vcp_domain::{
    artifact::{ArtifactDescriptor, CaptureState},
    ByteCount,
};
#[path = "../tests/common/mod.rs"]
mod common;

#[derive(Default)]
struct Memory {
    objects: BTreeMap<String, Vec<u8>>,
    reads: usize,
    fail: bool,
}
impl Pages for Memory {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        self.reads += 1;
        if self.fail {
            return Err(Error::Unavailable("artifact index read"));
        }
        let bytes = self
            .objects
            .get(digest)
            .ok_or(Error::Corruption("artifact index missing"))?;
        if bytes.len() > limit {
            return Err(Error::Limit("artifact index read"));
        }
        Ok(bytes.clone())
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        self.objects.insert(digest.into(), bytes.into());
        Ok(())
    }
}
fn fixture(count: usize) -> (State, ArtifactId) {
    let state = State::default().prepare(&common::initial()).unwrap().0;
    let spec = common::spec();
    let artifact = spec.id.clone();
    let descriptor = ArtifactDescriptor {
        spec,
        state: CaptureState::Complete,
        length: ByteCount::ZERO,
        sha256: vcp_protocol::digest_bytes(b""),
        retained: vec![vcp_domain::artifact::Range {
            start: ByteCount::ZERO,
            end: ByteCount::ZERO,
        }],
    };
    let mut tx = common::attach(&state, descriptor, None);
    tx.events = (0..count).map(|i| {
        let mut event = state.events[0].event.clone();
        event.id = EventId::parse(format!("artifact-event-{i}")).unwrap();
        event.correlation = CommandId::parse("artifact-index").unwrap();
        // Wrong-kind and malformed provenance must remain visible to the caller.
        event.data = serde_json::json!({"session_export": if i % 2 == 0 {serde_json::Value::Null} else {serde_json::json!({"bad":true})}});
        if i % 3 == 0 { event.artifacts = vec![artifact.clone(), artifact.clone()]; }
        event
    }).collect();
    (state.prepare(&tx).unwrap().0, artifact)
}

#[tokio::test]
async fn reference_index_matches_exact_predicate_and_proves_sparse_absence_without_payload_scans() {
    let (state, artifact) = fixture(130);
    let workspace = common::workspace().id;
    let mut pages = Memory::default();
    let catalog = Catalog::from_validated_state(&mut pages, &state)
        .await
        .unwrap();
    catalog
        .verify_replayed_state(&mut pages, &state)
        .await
        .unwrap();
    let expected = state
        .events
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            row.event.workspace == workspace && row.event.artifacts.contains(&artifact)
        })
        .map(|(ordinal, row)| (ordinal as u64, row.clone()))
        .collect::<Vec<_>>();
    for width in [1, 7, 64, 4096] {
        let mut actual = Vec::new();
        loop {
            let rows = catalog
                .artifact_events(
                    &mut pages,
                    &workspace,
                    &artifact,
                    actual.last().map(|row: &(u64, EventEnvelope)| row.0),
                    width,
                )
                .await
                .unwrap();
            if rows.is_empty() {
                break;
            }
            actual.extend(rows);
        }
        assert_eq!(actual, expected);
    }
    pages.reads = 0;
    assert!(catalog
        .artifact_events(&mut pages, &workspace, &ArtifactId::new(), None, 1)
        .await
        .unwrap()
        .is_empty());
    assert!(
        pages.reads <= 3,
        "absence must read only index height, not retained payloads"
    );
    assert!(catalog
        .artifact_events(
            &mut pages,
            &WorkspaceId::parse("sibling").unwrap(),
            &artifact,
            None,
            1
        )
        .await
        .unwrap()
        .is_empty());
    assert!(catalog
        .artifact_events(&mut pages, &workspace, &artifact, None, 0)
        .await
        .is_err());
    assert!(catalog
        .artifact_events(
            &mut pages,
            &workspace,
            &artifact,
            Some(catalog.event_count()),
            1
        )
        .await
        .is_err());
    pages.fail = true;
    assert!(catalog
        .artifact_events(&mut pages, &workspace, &artifact, None, 1)
        .await
        .is_err());
}

#[tokio::test]
async fn reference_index_replay_rejects_missing_extra_or_misbound_membership() {
    let (state, artifact) = fixture(12);
    let workspace = common::workspace().id;
    let mut pages = Memory::default();
    let catalog = Catalog::from_validated_state(&mut pages, &state)
        .await
        .unwrap();
    let mut forged = catalog.clone();
    forged.artifacts = Root::empty(Table::ArtifactReference);
    assert!(forged
        .verify_replayed_state(&mut pages, &state)
        .await
        .is_err());
    let mut forged = catalog.clone();
    forged.artifacts = forged
        .artifacts
        .insert(
            &mut pages,
            entry(key(&workspace, &artifact, 2).unwrap(), &2u64).unwrap(),
        )
        .await
        .unwrap();
    assert!(forged
        .verify_replayed_state(&mut pages, &state)
        .await
        .is_err());
    assert!(forged
        .artifact_events(&mut pages, &workspace, &artifact, None, 64)
        .await
        .is_err());
    let mut forged = catalog.clone();
    forged.artifacts = Root::empty(Table::ArtifactReference);
    for mut row in catalog.artifacts.page(&mut pages, None, 64).await.unwrap() {
        if row.key == key(&workspace, &artifact, 1).unwrap() {
            row.value = serde_json::json!(2);
        }
        forged.artifacts = forged.artifacts.insert(&mut pages, row).await.unwrap();
    }
    assert!(forged
        .verify_replayed_state(&mut pages, &state)
        .await
        .is_err());
    assert!(forged
        .artifact_events(&mut pages, &workspace, &artifact, None, 64)
        .await
        .is_err());
    let mut copied = Memory::default();
    catalog
        .copy_objects(&mut pages, &mut copied, &|| Ok(()))
        .await
        .unwrap();
    pages.objects.clear();
    catalog
        .verify_replayed_state(&mut copied, &state)
        .await
        .unwrap();
    let locator = catalog
        .events
        .get(&mut copied, &ordinal_key(1))
        .await
        .unwrap()
        .unwrap();
    let event: EventRow = serde_json::from_value(locator.value).unwrap();
    // Remove the complete payload object's chunk index, keeping membership and
    // envelope locator intact: unavailable evidence can never become absence.
    let mut isolated = Memory::default();
    catalog
        .artifacts
        .copy_pages(&mut copied, &mut isolated, &|| Ok(()))
        .await
        .unwrap();
    catalog
        .events
        .copy_pages(&mut copied, &mut isolated, &|| Ok(()))
        .await
        .unwrap();
    assert!(event.blob.bytes > 0);
    assert!(catalog
        .artifact_events(&mut isolated, &workspace, &artifact, None, 1)
        .await
        .is_err());
}

#[tokio::test]
async fn reference_index_byte_cutoff_continues_and_retains_redacted_references() {
    let (state, artifact) = fixture(1);
    let workspace = common::workspace().id;
    let mut pages = Memory::default();
    let mut catalog = Catalog::from_validated_state(&mut pages, &state)
        .await
        .unwrap();
    catalog.watermark = state.watermark.next().unwrap();
    let mut envelopes = Vec::new();
    for i in 0..3 {
        let mut row = state.events[1].clone();
        row.watermark = catalog.watermark;
        row.event.id = EventId::parse(format!("large-reference-{i}")).unwrap();
        row.event.data = serde_json::json!({"payload": "x".repeat(6 * 1024 * 1024)});
        envelopes.push(row);
    }
    let mut redacted = state.events[1].clone();
    redacted.watermark = catalog.watermark;
    redacted.event.id = EventId::parse("redacted-reference").unwrap();
    envelopes.push(
        vcp_protocol::redaction::event(&redacted, vcp_domain::DeletionEpoch::new(1)).unwrap(),
    );
    // Exercise the bounded row decoder directly; these synthetic envelopes do
    // not manufacture a semantic admission certificate or published owner.
    catalog.append_events(&mut pages, &envelopes).await.unwrap();
    let first = catalog
        .artifact_events(&mut pages, &workspace, &artifact, Some(1), 64)
        .await
        .unwrap();
    assert_eq!(first.len(), 2);
    assert_eq!(first[0].0, 2);
    assert!(
        first
            .iter()
            .map(|(_, row)| canonical_bytes(row).unwrap().len())
            .sum::<usize>()
            <= MAX_COMMIT_BYTES
    );
    let next = catalog
        .artifact_events(&mut pages, &workspace, &artifact, Some(first[1].0), 64)
        .await
        .unwrap();
    assert_eq!(next.len(), 2);
    assert_eq!(next[0].1, envelopes[2]);
    assert_eq!(next[1].1, envelopes[3]);
    assert!(next[1].1.redaction.is_some());
    assert!(catalog
        .artifact_events(&mut pages, &workspace, &artifact, Some(next[1].0), 64)
        .await
        .unwrap()
        .is_empty());
}
