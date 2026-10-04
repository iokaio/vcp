// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::cell::Cell;
use vcp_protocol::event::EventEnvelope;
use vcp_store::contract::{CanonicalStore, Receipt, State, Transaction};

struct Reader<'a> {
    source: &'a State,
    calls: Cell<usize>,
    fail_after: Option<usize>,
    empty_after: Option<usize>,
    receipt_error: bool,
    artifact_index_error: bool,
}
impl CanonicalStore for Reader<'_> {
    fn state(&self) -> &State {
        panic!("export reader requested resident history")
    }
    fn current(&self) -> vcp_store::CurrentStateView<'_> {
        self.source.into()
    }
    async fn history_event_count(&self) -> vcp_store::Result<u64> {
        Ok(self.source.events.len() as u64)
    }
    async fn history_events(
        &self,
        after: Option<u64>,
        limit: usize,
    ) -> vcp_store::Result<Vec<EventEnvelope>> {
        let start = after.map_or(0, |ordinal| ordinal as usize + 1);
        self.calls.set(self.calls.get() + 1);
        if self.fail_after.is_some_and(|cut| start >= cut) {
            return Err(vcp_store::Error::Corruption("injected export page"));
        }
        if self.empty_after.is_some_and(|cut| start >= cut) {
            return Ok(Vec::new());
        }
        Ok(self
            .source
            .events
            .iter()
            .skip(start)
            .take(limit.min(2))
            .cloned()
            .collect())
    }
    async fn command_receipt_by_id(
        &self,
        workspace: &WorkspaceId,
        command: &CommandId,
    ) -> vcp_store::Result<Option<CommandReceipt>> {
        if self.receipt_error {
            return Err(vcp_store::Error::Corruption("injected export receipt"));
        }
        Ok(self
            .source
            .commands
            .get(&vcp_store::contract::command_key(workspace, command))
            .cloned())
    }
    async fn history_artifact_events(
        &self,
        workspace: &WorkspaceId,
        artifact: &ArtifactId,
        after: Option<u64>,
        limit: usize,
    ) -> vcp_store::Result<Vec<(u64, EventEnvelope)>> {
        if self.artifact_index_error {
            return Err(vcp_store::Error::Corruption("injected artifact index"));
        }
        let start = after.map_or(0, |ordinal| ordinal as usize + 1);
        Ok(self
            .source
            .events
            .iter()
            .enumerate()
            .skip(start)
            .filter(|(_, row)| {
                &row.event.workspace == workspace && row.event.artifacts.contains(artifact)
            })
            .take(limit.min(1))
            .map(|(ordinal, row)| (ordinal as u64, row.clone()))
            .collect())
    }
    async fn transact(&mut self, _: Transaction) -> vcp_store::Result<Receipt> {
        panic!("export read mutated records")
    }
}
fn reader(source: &State) -> Reader<'_> {
    Reader {
        source,
        calls: Cell::new(0),
        fail_after: None,
        empty_after: None,
        receipt_error: false,
        artifact_index_error: false,
    }
}
pub(super) async fn compare(source: &State, artifact: &ArtifactDescriptor) {
    let live = reader(source);
    let expected = export_contract::validate_read(source, access().authority, None, artifact);
    let actual =
        export_contract::validate_read_store(&live, access().authority, None, artifact).await;
    assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
    let empty = std::collections::BTreeSet::new();
    for (authority, allowed) in [
        (access().authority.next().unwrap(), None),
        (access().authority, Some(&empty)),
    ] {
        let expected = export_contract::validate_read(source, authority, allowed, artifact);
        let actual =
            export_contract::validate_read_store(&live, authority, allowed, artifact).await;
        assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
    }
    for selected in [None, Some(artifact.spec.scope.task.clone())] {
        let expected = export_contract::Sources::capture(
            source,
            artifact.spec.scope.clone(),
            selected.clone(),
            access().authority,
        );
        let actual = export_contract::Sources::capture_store(
            &live,
            artifact.spec.scope.clone(),
            selected,
            access().authority,
        )
        .await;
        assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
        if let Ok(sources) = actual {
            let archived = sources.events(source).unwrap();
            let streamed = sources.events_store(&live).await.unwrap();
            assert_eq!(
                vcp_protocol::canonical_bytes(&archived).unwrap(),
                vcp_protocol::canonical_bytes(&streamed).unwrap()
            );
        }
    }
    assert!(
        live.calls.get() > 1,
        "source capture must cross bounded pages"
    );
}
pub(super) async fn faults(source: &State, artifact: &ArtifactDescriptor) {
    let mut live = reader(source);
    live.fail_after = Some(2);
    assert!(
        export_contract::validate_read_store(&live, access().authority, None, artifact)
            .await
            .is_err()
    );
    live.fail_after = None;
    live.empty_after = Some(2);
    assert!(
        export_contract::validate_read_store(&live, access().authority, None, artifact)
            .await
            .is_err()
    );
    live.empty_after = None;
    live.receipt_error = true;
    assert!(
        export_contract::validate_read_store(&live, access().authority, None, artifact)
            .await
            .is_err()
    );
    let mut duplicate = source.clone();
    let event = duplicate
        .events
        .iter()
        .find(|row| {
            row.event.artifacts.contains(&artifact.spec.id)
                && row.event.data.get("session_export").is_some()
        })
        .unwrap()
        .clone();
    duplicate.events.push(event);
    compare(&duplicate, artifact).await;
    let mut missing = source.clone();
    missing.commands.clear();
    compare(&missing, artifact).await;
    let ordinary: ArtifactDescriptor = source
        .records
        .values()
        .filter(|row| row.collection == Collection::Artifact)
        .map(|row| row.decode::<ArtifactDescriptor>().unwrap())
        .find(|artifact| artifact.spec.schema == "synthetic-evidence/1")
        .unwrap();
    let mut indexed = reader(source);
    indexed.fail_after = Some(0);
    export_contract::validate_read_store(&indexed, access().authority, None, &ordinary)
        .await
        .unwrap();
    assert_eq!(
        indexed.calls.get(),
        0,
        "ordinary artifact absence must use the authenticated reference index"
    );
    indexed.artifact_index_error = true;
    assert!(
        export_contract::validate_read_store(&indexed, access().authority, None, &ordinary)
            .await
            .is_err()
    );
}
