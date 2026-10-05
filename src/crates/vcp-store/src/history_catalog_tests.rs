// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::contract::{key, Collection, Record};
use std::collections::BTreeMap;
use vcp_domain::{DeletionEpoch, SessionSeq};
#[path = "../tests/common/mod.rs"]
mod common;

#[derive(Default)]
struct Memory {
    objects: BTreeMap<String, Vec<u8>>,
    failed: bool,
}
impl Pages for Memory {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        if self.failed {
            return Err(Error::Unavailable("catalog injected read"));
        }
        let bytes = self
            .objects
            .get(digest)
            .ok_or(Error::Corruption("catalog object missing"))?;
        if bytes.len() > limit {
            return Err(Error::Limit("catalog object read"));
        }
        Ok(bytes.clone())
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        if self.failed {
            return Err(Error::Unavailable("catalog injected write"));
        }
        if let Some(existing) = self.objects.get(digest) {
            if existing != bytes {
                return Err(Error::Corruption("catalog immutable object"));
            }
        } else {
            self.objects.insert(digest.into(), bytes.into());
        }
        Ok(())
    }
}
fn state(count: usize) -> State {
    let mut transaction = common::initial();
    let original = transaction.events[0].clone();
    transaction.events = (0..count)
        .map(|i| {
            let mut event = original.clone();
            if i != 0 {
                event.id = EventId::parse(format!("event-{i}")).unwrap();
            }
            event.data = serde_json::json!({"payload":"retained exact secret", "ordinal":i});
            event
        })
        .collect();
    State::default().prepare(&transaction).unwrap().0
}
fn reference_scoped(
    state: &State,
    workspace: &WorkspaceId,
    session: &SessionId,
    command: &CommandId,
) -> Result<Option<CommandReceipt>> {
    let Some(receipt) = state.commands.get(&command_key(workspace, command)) else {
        return Ok(None);
    };
    let mut found = false;
    for event in state
        .events
        .iter()
        .filter(|event| event.watermark == receipt.watermark && &event.event.correlation == command)
    {
        if &event.event.workspace != workspace || &event.event.session != session {
            return Err(Error::Access);
        }
        found = true;
    }
    Ok(found.then(|| receipt.clone()))
}

#[tokio::test]
async fn command_pages_keep_canonical_order_and_receipt_only_history() {
    let mut source = state(1);
    for index in 0..80 {
        let mut transaction = common::initial();
        transaction.id = TransactionId::new();
        transaction.expected_watermark = source.watermark;
        transaction.mutations.clear();
        transaction.events[0].id = EventId::new();
        transaction.events[0].correlation = CommandId::parse(format!("command-{index}")).unwrap();
        transaction.command.as_mut().unwrap().command = transaction.events[0].correlation.clone();
        source = source.prepare(&transaction).unwrap().0;
    }
    // Retained receipts remain canonical even when their events were pruned.
    source.events.clear();
    source.sequences.clear();
    let mut pages = Memory::default();
    let catalog = Catalog::from_validated_state(&mut pages, &source)
        .await
        .unwrap();
    let expected = source
        .commands
        .iter()
        .map(|(key, row)| (key.clone(), row.clone()))
        .collect::<Vec<_>>();
    for limit in [1, 7, 64, 4096] {
        let mut actual = Vec::new();
        loop {
            let after = actual
                .last()
                .map(|(key, _): &(String, CommandReceipt)| key.as_str());
            let next = catalog
                .command_page(&mut pages, after, limit)
                .await
                .unwrap();
            if next.is_empty() {
                break;
            }
            actual.extend(next);
        }
        assert_eq!(actual, expected);
    }
    assert!(catalog.command_page(&mut pages, None, 0).await.is_err());
    pages.failed = true;
    assert!(catalog.command_page(&mut pages, None, 1).await.is_err());
}

#[tokio::test]
async fn exact_catalog_rows_and_receipt_contracts_match_complete_reference() {
    let state = state(73);
    let mut pages = Memory::default();
    let catalog = Catalog::from_validated_state(&mut pages, &state)
        .await
        .unwrap();
    assert_eq!(catalog.watermark(), state.watermark);
    assert_eq!(catalog.event_count(), state.events.len() as u64);
    assert_eq!(
        catalog.event_at(&mut pages, 0).await.unwrap().as_ref(),
        state.events.first()
    );
    assert!(catalog
        .event_at(&mut pages, state.events.len() as u64)
        .await
        .unwrap()
        .is_none());
    for width in [1, 7, 64, 4096] {
        let mut found = Vec::new();
        while found.len() < state.events.len() {
            found.extend(
                catalog
                    .event_page(&mut pages, (found.len() as u64).checked_sub(1), width)
                    .await
                    .unwrap(),
            );
        }
        assert_eq!(found, *state.events);
        assert!(catalog
            .event_page(&mut pages, Some(found.len() as u64 - 1), width)
            .await
            .unwrap()
            .is_empty());
    }
    for event in state.events.iter() {
        assert_eq!(
            catalog
                .event(&mut pages, &event.event.id)
                .await
                .unwrap()
                .as_ref(),
            Some(event)
        );
    }
    for receipt in state.commands.values() {
        assert_eq!(
            catalog
                .command(
                    &mut pages,
                    &receipt.workspace,
                    &receipt.command,
                    &receipt.digest
                )
                .await
                .unwrap(),
            state
                .command(&receipt.workspace, &receipt.command, &receipt.digest)
                .unwrap()
        );
        assert_eq!(
            catalog
                .command(
                    &mut pages,
                    &receipt.workspace,
                    &receipt.command,
                    "different"
                )
                .await
                .unwrap_err()
                .to_string(),
            state
                .command(&receipt.workspace, &receipt.command, "different")
                .unwrap_err()
                .to_string()
        );
        for session in [common::session().id, SessionId::parse("other").unwrap()] {
            assert_eq!(
                catalog
                    .scoped_command(&mut pages, &receipt.workspace, &session, &receipt.command)
                    .await
                    .map_err(|error| error.to_string()),
                reference_scoped(&state, &receipt.workspace, &session, &receipt.command)
                    .map_err(|error| error.to_string())
            );
        }
    }
    for receipt in state.transactions.values() {
        assert_eq!(
            catalog
                .transaction(&mut pages, &receipt.transaction)
                .await
                .unwrap()
                .as_ref(),
            Some(receipt)
        );
    }
    assert!(catalog
        .event(&mut pages, &EventId::parse("missing").unwrap())
        .await
        .unwrap()
        .is_none());
    assert!(catalog.event_page(&mut pages, Some(73), 1).await.is_err());
    assert!(catalog.event_page(&mut pages, None, 4097).await.is_err());
    pages.failed = true;
    assert!(catalog.event_page(&mut pages, None, 1).await.is_err());
    assert!(catalog
        .command(
            &mut pages,
            &common::workspace().id,
            &CommandId::parse("create").unwrap(),
            &"a".repeat(64)
        )
        .await
        .is_err());
}

#[tokio::test]
async fn exact_redacted_generation_never_recovers_original_payload_and_retained_receipt_stays_replayable(
) {
    let mut state = state(3);
    let mut workspace = common::workspace();
    workspace.deletion = DeletionEpoch::new(1);
    state
        .records
        .get_mut(&key(Collection::Workspace, workspace.id.as_str()))
        .unwrap()
        .value = serde_json::to_value(&workspace).unwrap();
    for event in state.events.iter_mut() {
        *event = vcp_protocol::redaction::event(event, workspace.deletion).unwrap();
    }
    let mut pages = Memory::default();
    let catalog = Catalog::from_validated_state(&mut pages, &state)
        .await
        .unwrap();
    assert_eq!(
        catalog.event_page(&mut pages, None, 4096).await.unwrap(),
        *state.events
    );
    assert!(!pages.objects.values().any(|bytes| bytes
        .windows(b"retained exact secret".len())
        .any(|window| window == b"retained exact secret")));
    // Event retention preserves exact idempotency while scoped query proof disappears.
    state.events.clear();
    state.sequences.clear();
    let retained = Catalog::from_validated_state(&mut pages, &state)
        .await
        .unwrap();
    let receipt = state.commands.values().next().unwrap();
    assert_eq!(
        retained
            .command(
                &mut pages,
                &receipt.workspace,
                &receipt.command,
                &receipt.digest
            )
            .await
            .unwrap()
            .as_ref(),
        Some(receipt)
    );
    assert!(retained
        .scoped_command(
            &mut pages,
            &receipt.workspace,
            &common::session().id,
            &receipt.command
        )
        .await
        .unwrap()
        .is_none());
    // The old owner root continues to prove its original generation.
    assert_eq!(
        catalog
            .event_page(&mut pages, None, 4096)
            .await
            .unwrap()
            .len(),
        3
    );
}

#[tokio::test]
async fn scope_proof_consumes_complete_group_beyond_page_boundary() {
    let mut state = state(4097);
    let mut other = common::session();
    other.id = SessionId::parse("other").unwrap();
    state.records.insert(
        key(Collection::Session, other.id.as_str()),
        Record::typed(
            Collection::Session,
            other.id.to_string(),
            other.workspace.clone(),
            other.revision,
            &other,
        )
        .unwrap(),
    );
    let last = state.events.last_mut().unwrap();
    last.event.session = other.id.clone();
    last.event.task = None;
    last.sequence = SessionSeq::new(1);
    state
        .sequences
        .insert(common::session().id, SessionSeq::new(4096));
    state.sequences.insert(other.id, SessionSeq::new(1));
    state.validate().unwrap();
    let mut pages = Memory::default();
    let catalog = Catalog::from_validated_state(&mut pages, &state)
        .await
        .unwrap();
    let receipt = state.commands.values().next().unwrap();
    assert!(matches!(
        catalog
            .scoped_command(
                &mut pages,
                &receipt.workspace,
                &common::session().id,
                &receipt.command
            )
            .await,
        Err(Error::Access)
    ));
    assert_eq!(
        catalog
            .event_page(&mut pages, None, 4096)
            .await
            .unwrap()
            .len(),
        4096
    );
    assert_eq!(
        catalog
            .event_page(&mut pages, Some(4095), 4096)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn candidate_append_keeps_old_root_and_rejects_failed_or_changed_content() {
    let state = state(2);
    let mut pages = Memory::default();
    let catalog = Catalog::from_validated_state(&mut pages, &state)
        .await
        .unwrap();
    let mut transaction = common::initial();
    transaction.id = TransactionId::parse("next").unwrap();
    transaction.expected_watermark = state.watermark;
    transaction.mutations.clear();
    transaction.events[0].id = EventId::parse("next-event").unwrap();
    transaction.events[0].correlation = CommandId::parse("next-command").unwrap();
    transaction.command.as_mut().unwrap().command = transaction.events[0].correlation.clone();
    let mut diagnostics = crate::StoreDiagnostics::new(crate::BackendKind::Files);
    let mut size = crate::contract::StateSize::measure(&state).unwrap();
    let prepared =
        PreparedTransition::prepare(&state, &transaction, &mut diagnostics, &mut size).unwrap();
    let next = prepared.state();
    assert_eq!(diagnostics.validation.completed, 1);
    pages.failed = true;
    assert!(catalog
        .append_validated_commit(&mut pages, &prepared)
        .await
        .is_err());
    pages.failed = false;
    let candidate = catalog
        .append_validated_commit(&mut pages, &prepared)
        .await
        .unwrap();
    assert_eq!(
        catalog.event_page(&mut pages, None, 7).await.unwrap(),
        *state.events
    );
    assert_eq!(
        candidate.event_page(&mut pages, None, 7).await.unwrap(),
        *next.events
    );
    assert!(candidate
        .append_validated_commit(&mut pages, &prepared)
        .await
        .is_err());
    let missing = pages.objects.keys().next().unwrap().clone();
    // Corrupt each available immutable object in turn. At least one of the
    // authenticated event-path objects must cause the bounded read to fail.
    let mut detected = false;
    for digest in pages.objects.keys().cloned().collect::<Vec<_>>() {
        let bytes = pages.objects.get_mut(&digest).unwrap();
        let original = bytes[0];
        bytes[0] ^= 1;
        detected |= candidate.event_page(&mut pages, None, 7).await.is_err();
        pages.objects.get_mut(&digest).unwrap()[0] = original;
    }
    assert!(detected);
    pages.objects.remove(&missing);
    // Domain substitution is refused before looking up mutable storage.
    let mut substituted = serde_json::to_value(&candidate).unwrap();
    substituted["events"]["table"] = "command".into();
    let substituted: Catalog = serde_json::from_value(substituted).unwrap();
    assert!(substituted.event_page(&mut pages, None, 1).await.is_err());
}

#[tokio::test]
async fn event_page_bounds_payload_bytes_and_short_page_keeps_exact_continuation() {
    let mut state = state(3);
    for event in state.events.iter_mut() {
        event.event.data = serde_json::json!({"payload":"x".repeat(6 * 1024 * 1024)});
    }
    let mut pages = Memory::default();
    let catalog = Catalog::from_validated_state(&mut pages, &state)
        .await
        .unwrap();
    let first = catalog.event_page(&mut pages, None, 4096).await.unwrap();
    assert_eq!(first, state.events[..2]);
    assert!(
        first
            .iter()
            .map(|event| canonical_bytes(event).unwrap().len())
            .sum::<usize>()
            <= MAX_COMMIT_BYTES
    );
    let last = catalog.event_page(&mut pages, Some(1), 4096).await.unwrap();
    assert_eq!(last, state.events[2..]);
    let receipt = state.commands.values().next().unwrap();
    assert_eq!(
        catalog
            .scoped_command(
                &mut pages,
                &receipt.workspace,
                &common::session().id,
                &receipt.command
            )
            .await
            .unwrap()
            .as_ref(),
        Some(receipt)
    );
}

#[tokio::test]
async fn persisted_catalog_must_match_every_replayed_row_identity_and_complete_group() {
    let state = state(5);
    let mut pages = Memory::default();
    let catalog = Catalog::from_validated_state(&mut pages, &state)
        .await
        .unwrap();
    let decoded: Catalog = serde_json::from_slice(&canonical_bytes(&catalog).unwrap()).unwrap();
    decoded
        .verify_replayed_state(&mut pages, &state)
        .await
        .unwrap();

    let mut changed_state = state.clone();
    changed_state.events[2].event.data = serde_json::json!({"silently":"resealed"});
    let changed = Catalog::from_validated_state(&mut pages, &changed_state)
        .await
        .unwrap();
    assert!(changed
        .verify_replayed_state(&mut pages, &state)
        .await
        .is_err());

    let mut changed = catalog.clone();
    changed.identities = Root::empty(Table::EventIdentity);
    for (ordinal, event) in state.events.iter().rev().enumerate() {
        changed.identities = changed
            .identities
            .insert(
                &mut pages,
                entry(event.event.id.to_string(), &(ordinal as u64)).unwrap(),
            )
            .await
            .unwrap();
    }
    assert!(changed
        .verify_replayed_state(&mut pages, &state)
        .await
        .is_err());

    let mut changed = catalog.clone();
    changed.groups = Root::empty(Table::Commit)
        .insert(
            &mut pages,
            entry(
                ordinal_key(state.watermark.get()),
                &Group { first: 1, count: 4 },
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert!(changed
        .verify_replayed_state(&mut pages, &state)
        .await
        .is_err());

    let mut changed = catalog.clone();
    let mut receipt = state.transactions.values().next().unwrap().clone();
    receipt.digest = "b".repeat(64);
    changed.transactions = insert_object(
        &Root::empty(Table::Transaction),
        &mut pages,
        receipt.transaction.to_string(),
        &receipt,
    )
    .await
    .unwrap();
    assert!(changed
        .verify_replayed_state(&mut pages, &state)
        .await
        .is_err());

    let mut changed = catalog.clone();
    changed.commands = Root::empty(Table::Command);
    assert!(changed
        .verify_replayed_state(&mut pages, &state)
        .await
        .is_err());
    pages.failed = true;
    assert!(catalog
        .verify_replayed_state(&mut pages, &state)
        .await
        .is_err());
}
