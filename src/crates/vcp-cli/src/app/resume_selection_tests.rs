// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::cell::Cell;
use vcp_domain::{
    revision::*,
    task::{Objective, TaskState},
    verification::Fingerprint,
    workspace::Scope,
};
use vcp_protocol::event::{EventEnvelope, EventInput, EventKind};
use vcp_store::contract::{reference::ReferenceStore, Record};

#[derive(Clone, Copy)]
enum Fault {
    None,
    Corrupt,
    Short,
}

struct ReadCut<'a> {
    state: &'a State,
    pages: Cell<usize>,
    fault: Fault,
}
impl ReferenceStore for ReadCut<'_> {
    fn state(&self) -> &State {
        panic!("resume selection must use current records and bounded history")
    }
    fn current(&self) -> vcp_store::CurrentStateView<'_> {
        self.state.into()
    }
    async fn history_event_count(&self) -> vcp_store::Result<u64> {
        Ok(self.state.events.len() as u64)
    }
    async fn history_events(
        &self,
        after: Option<u64>,
        limit: usize,
    ) -> vcp_store::Result<Vec<EventEnvelope>> {
        self.pages.set(self.pages.get() + 1);
        assert!((1..=256).contains(&limit));
        match self.fault {
            Fault::Corrupt => {
                return Err(vcp_store::Error::Corruption(
                    "injected resume history failure",
                ))
            }
            Fault::Short => return Ok(vec![]),
            Fault::None => {}
        }
        let first = after.map_or(0, |value| value + 1) as usize;
        Ok(self
            .state
            .events
            .iter()
            .skip(first)
            .take(1)
            .cloned()
            .collect())
    }
    async fn transact(
        &mut self,
        _: vcp_store::contract::Transaction,
    ) -> vcp_store::Result<vcp_store::contract::Receipt> {
        panic!("resume selection is read-only")
    }
}

fn fixture() -> (State, WorkspaceId) {
    let workspace = WorkspaceId::new();
    let mut state = State::default();
    state.watermark = Watermark::new(3);
    for (id, session, status, activity) in [
        ("older", "session-a", TaskState::Paused, 5),
        ("newer", "session-b", TaskState::WaitingForInput, 9),
        ("completed", "session-a", TaskState::Completed, 20),
    ] {
        let task = Task {
            scope: Scope {
                workspace: workspace.clone(),
                session: SessionId::parse(session).unwrap(),
                task: TaskId::parse(id).unwrap(),
            },
            root: TaskId::parse(id).unwrap(),
            parent: None,
            fork_origin: None,
            revision: Revision::new(3),
            steering: SteeringRevision::ZERO,
            objectives: vec![Objective {
                text: id.repeat(300),
                constraints: vec![],
                acceptance: vec![],
                source: EventId::new(),
                steering: SteeringRevision::ZERO,
            }],
            state: status,
            fingerprint: Fingerprint {
                repository: "a".repeat(64),
                buffers: "b".repeat(64),
                environment: "c".repeat(64),
            },
            editing: false,
            required_checks: vec![],
            cause: EventId::new(),
            reason: format!("reason for {id}"),
            redaction: None,
        };
        let record = Record::typed(
            Collection::Task,
            id,
            workspace.clone(),
            task.revision,
            &task,
        )
        .unwrap();
        state.records.insert(record.key(), record);
        let watermark = Watermark::new(state.events.len() as u64 + 1);
        state.events.push(EventEnvelope {
            version: 1,
            sequence: SessionSeq::new(if id == "completed" { 2 } else { 1 }),
            watermark,
            redaction: None,
            event: EventInput {
                id: EventId::new(),
                workspace: workspace.clone(),
                session: task.scope.session.clone(),
                task: Some(task.scope.task),
                actor: ActorId::new(),
                correlation: CommandId::new(),
                causation: None,
                timestamp: Timestamp::new(activity),
                kind: EventKind::Commentary,
                artifacts: vec![],
                data: json!({}),
                metadata: None,
            },
        });
    }
    (state, workspace)
}

#[tokio::test]
async fn resume_selection_reuses_one_history_scan_with_identical_task_and_summary() {
    let (state, workspace) = fixture();
    for (task, session, expected) in [
        (None, None, "newer"),
        (None, Some("session-a"), "older"),
        (Some("older"), None, "older"),
        (Some("completed"), None, "completed"),
    ] {
        let reader = ReadCut {
            state: &state,
            pages: Cell::new(0),
            fault: Fault::None,
        };
        let requested = task.map(|id| TaskId::parse(id).unwrap());
        let session = session.map(|id| SessionId::parse(id).unwrap());
        let (selected, summary) =
            resume_selection(&reader, &workspace, requested.as_ref(), session.as_ref())
                .await
                .unwrap();
        let expected_task =
            task_from(&state, &workspace, &TaskId::parse(expected).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(&selected).unwrap(),
            serde_json::to_value(expected_task).unwrap()
        );
        // Compare the exact display value to the old separately recomputed
        // summary, including truncation and the completed-task None case.
        let expected_summary = crate::continuation::candidates(&state, &workspace)
            .unwrap()
            .into_iter()
            .find(|row| row.task == selected.scope.task);
        let display = summary
            .map(crate::continuation::summarize)
            .transpose()
            .unwrap();
        let expected_display = expected_summary
            .map(crate::continuation::summarize)
            .transpose()
            .unwrap();
        assert_eq!(
            serde_json::to_value(&display).unwrap(),
            serde_json::to_value(expected_display).unwrap()
        );
        if expected == "completed" {
            assert!(display.is_none());
        } else {
            assert!(display.unwrap().details_truncated);
        }
        assert_eq!(
            reader.pages.get(),
            state.events.len(),
            "selection plus summary traverses history exactly once"
        );
    }
}

#[tokio::test]
async fn resume_selection_missing_task_and_history_errors_preserve_precedence() {
    let (state, workspace) = fixture();
    let missing = TaskId::new();
    let existing = TaskId::parse("older").unwrap();
    let missing_session = SessionId::new();
    for fault in [Fault::None, Fault::Corrupt, Fault::Short] {
        let reader = ReadCut {
            state: &state,
            pages: Cell::new(0),
            fault,
        };
        let expected = task_from(&state, &workspace, &missing).unwrap_err();
        assert_eq!(
            resume_selection(&reader, &workspace, Some(&missing), None)
                .await
                .unwrap_err(),
            expected
        );
        assert_eq!(
            reader.pages.get(),
            0,
            "explicit missing IDs fail before history access"
        );
        for (task, session) in [(None, Some(&missing_session)), (Some(&existing), None)] {
            reader.pages.set(0);
            let result = resume_selection(&reader, &workspace, task, session).await;
            match fault {
                Fault::Corrupt => assert!(result
                    .unwrap_err()
                    .contains("injected resume history failure")),
                Fault::Short => assert!(result.unwrap_err().contains("cut changed or incomplete")),
                Fault::None if task.is_none() => assert_eq!(
                    result.unwrap_err(),
                    "no unfinished task is available to resume"
                ),
                Fault::None => assert!(result.is_ok()),
            }
            assert!(
                reader.pages.get() > 0,
                "history errors cannot be hidden by candidate absence"
            );
        }
    }
    let mut empty = state.clone();
    empty.records.clear();
    let reader = ReadCut {
        state: &empty,
        pages: Cell::new(0),
        fault: Fault::None,
    };
    assert_eq!(
        resume_selection(&reader, &workspace, None, None)
            .await
            .unwrap_err(),
        "no unfinished task is available to resume"
    );
    assert_eq!(reader.pages.get(), state.events.len());
}
