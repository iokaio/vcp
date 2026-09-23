// SPDX-License-Identifier: Apache-2.0
//! A canonical turn ID is a caller-selected identity, never a replay signal.
//! Collision checks cover all scopes before capture creates durable artifacts.
use vcp_domain::TurnId;
use vcp_store::contract::{key, Collection, State};

pub(super) fn require_unused(state: &State, id: &TurnId) -> Result<(), &'static str> {
    if state
        .records
        .contains_key(&key(Collection::Turn, id.as_str()))
    {
        return Err("canonical turn identity is already in use");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vcp_domain::{
        ids::*,
        revision::*,
        task::{Turn, TurnState},
        workspace::Scope,
    };
    use vcp_store::contract::Record;

    #[test]
    fn existing_identity_is_rejected_for_same_or_different_scope_without_mutation() {
        let id = TurnId::parse("caller-selected-turn").unwrap();
        for (workspace, session, task) in [
            ("workspace", "session", "task"),
            ("other-workspace", "session", "task"),
            ("workspace", "other-session", "task"),
            ("workspace", "session", "other-task"),
        ] {
            let mut state = State::default();
            assert!(require_unused(&state, &id).is_ok());
            let turn = Turn {
                redaction: None,
                id: id.clone(),
                scope: Scope {
                    workspace: WorkspaceId::parse(workspace).unwrap(),
                    session: SessionId::parse(session).unwrap(),
                    task: TaskId::parse(task).unwrap(),
                },
                revision: Revision::ZERO,
                steering: SteeringRevision::ZERO,
                state: TurnState::Completed,
                trigger: ArtifactId::parse("input").unwrap(),
                cause: EventId::new(),
                reason: "already used".into(),
            };
            let record = Record::typed(
                Collection::Turn,
                id.as_str(),
                turn.scope.workspace.clone(),
                Revision::ZERO,
                &turn,
            )
            .unwrap();
            state.records.insert(record.key(), record);
            let before = state.clone();
            assert!(require_unused(&state, &id).is_err());
            assert_eq!(state, before);
            assert!(require_unused(&state, &TurnId::parse("another-turn").unwrap()).is_ok());
        }
    }
}
