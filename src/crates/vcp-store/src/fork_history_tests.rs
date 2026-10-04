// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{BackendKind, CurrentState, Store};

#[path = "../tests/atomic_fork.rs"]
mod atomic;
#[path = "fork_history_reference.rs"]
mod reference;

struct Reader {
    events: Vec<EventEnvelope>,
    fail_session: bool,
    fail_event: bool,
    session_reads: usize,
    event_reads: usize,
}

impl Reader {
    fn new(state: &State) -> Self {
        Self {
            events: state.events.to_vec(),
            fail_session: false,
            fail_event: false,
            session_reads: 0,
            event_reads: 0,
        }
    }
}

impl ForkHistory for Reader {
    fn has_session(&mut self, session: &SessionId) -> Result<bool> {
        self.session_reads += 1;
        if self.fail_session {
            return Err(Error::Unavailable("fork session history unavailable"));
        }
        Ok(self.events.iter().any(|row| &row.event.session == session))
    }

    fn any_event(
        &mut self,
        id: &EventId,
        predicate: &dyn Fn(&EventEnvelope) -> bool,
    ) -> Result<bool> {
        self.event_reads += 1;
        if self.fail_event {
            return Err(Error::Unavailable("fork cause history unavailable"));
        }
        Ok(self
            .events
            .iter()
            .any(|row| &row.event.id == id && predicate(row)))
    }
}

fn compare(state: &State, tx: &Transaction, expected: bool) {
    let current = CurrentState::from_state(state);
    let mut reader = Reader::new(state);
    let actual = validate_with_history((&current).into(), tx, &mut reader);
    let original = reference::validate(state, tx);
    assert_eq!(actual.is_ok(), expected);
    assert_eq!(actual.is_ok(), original.is_ok());
    if let (Err(actual), Err(original)) = (actual, original) {
        assert_eq!(actual.to_string(), original.to_string());
    }
}

#[tokio::test]
async fn current_fork_proof_matches_frozen_full_history_predicates() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        let tx = atomic::fixture(&mut store).await;
        let original = store.state().clone();
        compare(&original, &tx, true);
        let acceptance: Acceptance = serde_json::from_value(tx.events[0].data.clone()).unwrap();
        let turn: Turn = original
            .record(
                Collection::Turn,
                acceptance.through_turn.as_str(),
                &acceptance.source.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let index = original
            .events
            .iter()
            .position(|row| row.event.id == turn.cause)
            .unwrap();
        for variant in 0..12 {
            let mut changed = original.clone();
            match variant {
                0 => {
                    changed.events.remove(index);
                }
                1 => changed.events[index].watermark = Watermark::ZERO,
                2 => changed.events[index].event.workspace = vcp_domain::ids::WorkspaceId::new(),
                3 => changed.events[index].event.session = SessionId::new(),
                4 => changed.events[index].event.task = None,
                5 => changed.events[index].event.kind = EventKind::Commentary,
                6 => changed.events[index].event.data["schema_version"] = serde_json::json!(2),
                7 => changed.events[index].event.data["facts"] = serde_json::json!([]),
                8 => {
                    let fact = changed.events[index].event.data["facts"][0].clone();
                    changed.events[index].event.data["facts"]
                        .as_array_mut()
                        .unwrap()
                        .push(fact);
                }
                9 => {
                    changed.events[index].event.data["facts"][0]["value"]["reason"] =
                        serde_json::json!("changed")
                }
                10 => {
                    changed.events[index].redaction =
                        Some(vcp_domain::redaction::ContentRedaction {
                            deletion: DeletionEpoch::new(1),
                            original_digest: "a".repeat(64),
                        })
                }
                _ => {
                    // Historical session existence is authoritative independently of current sequences.
                    let mut row = changed.events[index].clone();
                    row.event.id = EventId::new();
                    row.event.session = acceptance.new_session.clone();
                    changed.events.push(row);
                }
            }
            compare(&changed, &tx, false);
        }
        // Preserve the original any-match behavior even for duplicate identities in a reference state.
        let mut duplicate = original.clone();
        let mut wrong = duplicate.events[index].clone();
        wrong.event.task = None;
        duplicate.events.insert(0, wrong);
        compare(&duplicate, &tx, true);
        let current = CurrentState::from_state(&original);
        for fail_session in [true, false] {
            let mut reader = Reader::new(&original);
            reader.fail_session = fail_session;
            reader.fail_event = !fail_session;
            assert!(matches!(
                validate_with_history((&current).into(), &tx, &mut reader),
                Err(Error::Unavailable(_))
            ));
            assert_eq!(reader.session_reads, 1);
            assert_eq!(reader.event_reads, usize::from(!fail_session));
        }
        store.close().await.unwrap();
    }
}
