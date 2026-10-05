// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_protocol::event::EventEnvelope;
#[path = "../tests/common/mod.rs"]
mod common;

// Frozen original loop/epoch lookup, independent of the new row adapter.
fn reference(state: &State) -> Result<()> {
    for event in &state.events {
        vcp_protocol::redaction::validate_event(event)
            .map_err(|_| Error::Corruption("explicit event redaction"))?;
        if let Some(redaction) = &event.redaction {
            let workspace: Workspace = state
                .record(
                    Collection::Workspace,
                    event.event.workspace.as_str(),
                    &event.event.workspace,
                )?
                .decode()?;
            if workspace.deletion == DeletionEpoch::ZERO {
                return Err(Error::Conflict(
                    "retention rewrite needs committed deletion epoch",
                ));
            }
            if redaction.deletion > workspace.deletion {
                return Err(Error::Corruption("event redaction exceeds deletion epoch"));
            }
        }
    }
    Ok(())
}
fn equivalent(state: &State) {
    let expected = reference(state).map_err(|e| e.to_string());
    for width in [1, 2, 7, 256] {
        for owned in [false, true] {
            let result: Result<()> = (|| {
                for page in state.events.chunks(width) {
                    if owned {
                        validate_event_rows(&state.records, page.iter().cloned().map(Ok))?;
                    } else {
                        validate_event_rows(&state.records, page.iter().map(Ok))?;
                    }
                }
                Ok(())
            })();
            assert_eq!(result.map_err(|e| e.to_string()), expected);
        }
    }
}
fn redacted() -> State {
    let (mut state, _) = State::default().prepare(&common::initial()).unwrap();
    let mut workspace = common::workspace();
    workspace.deletion = DeletionEpoch::new(1);
    state
        .records
        .get_mut(&key(Collection::Workspace, workspace.id.as_str()))
        .unwrap()
        .value = serde_json::to_value(&workspace).unwrap();
    state.events[0] =
        vcp_protocol::redaction::event(&state.events[0], DeletionEpoch::new(1)).unwrap();
    state
}

#[test]
fn redacted_history_pages_preserve_content_and_current_epoch_checks() {
    equivalent(&State::default());
    let state = redacted();
    reference(&state).unwrap();
    let mutations: &[fn(&mut State)] = &[
        |s| s.events[0].event.data = serde_json::json!({"resurrected":"content"}),
        |s| {
            s.events[0]
                .redaction
                .as_mut()
                .unwrap()
                .original_digest
                .clear()
        },
        |s| s.events[0].redaction.as_mut().unwrap().deletion = DeletionEpoch::new(2),
        |s| s.events[0].redaction.as_mut().unwrap().deletion = DeletionEpoch::ZERO,
        |s| {
            s.records
                .remove(&key(Collection::Workspace, common::workspace().id.as_str()));
        },
        |s| {
            s.records
                .get_mut(&key(Collection::Workspace, common::workspace().id.as_str()))
                .unwrap()
                .workspace = WorkspaceId::new()
        },
        |s| {
            s.records
                .get_mut(&key(Collection::Workspace, common::workspace().id.as_str()))
                .unwrap()
                .value["deletion"] = serde_json::json!("0")
        },
    ];
    for count in [1, 3, 17] {
        let mut history = state.clone();
        let event = history.events[0].clone();
        history.events.extend(std::iter::repeat_n(event, count - 1));
        equivalent(&history);
        for mutation in mutations {
            let mut invalid = history.clone();
            mutation(&mut invalid);
            assert!(reference(&invalid).is_err());
            equivalent(&invalid);
        }
    }
}

#[test]
fn redaction_reader_failure_is_not_a_successful_end_of_stream() {
    let state = redacted();
    let result = validate_event_rows(
        &state.records,
        state
            .events
            .iter()
            .cloned()
            .map(Ok)
            .chain([Err(Error::Unavailable("synthetic redaction reader"))]),
    );
    assert!(matches!(
        result,
        Err(Error::Unavailable("synthetic redaction reader"))
    ));
    assert!(matches!(
        validate_event_rows::<EventEnvelope>(&state.records, [Err(Error::Access)]),
        Err(Error::Access)
    ));
}
