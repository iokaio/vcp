// SPDX-License-Identifier: Apache-2.0
//! Read-only workspace discovery. A candidate is context, never resume authority.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{
    accounting::{Ledger, Reservation, ReservationState},
    artifact::ArtifactDescriptor,
    effect::{Effect, EffectState},
    ids::*,
    revision::{Revision, Timestamp, Watermark},
    task::{Task, TaskState},
};
use vcp_protocol::command::{Approval, ApprovalState};
use vcp_store::{
    contract::{CanonicalStore, Collection, State},
    CurrentStateView,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Child {
    pub task: TaskId,
    pub parent: Option<TaskId>,
    pub state: TaskState,
    pub reason: String,
    pub expected_revision: Revision,
    /// The child's own paused/cancelled state remains independent of this fence.
    pub paused_ancestors: Vec<TaskId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub details_truncated: bool,
    pub task: TaskId,
    pub session: SessionId,
    pub expected_revision: Revision,
    pub state: TaskState,
    pub objective: String,
    pub reason: String,
    /// Most recent canonical event in this task tree; absent means unknown.
    pub last_activity: Option<Timestamp>,
    pub children: Vec<Child>,
    pub artifacts: Vec<ArtifactDescriptor>,
    pub observed_changes: Vec<ArtifactId>,
    pub unsettled_reservations: Vec<Reservation>,
    pub ledger: Option<Ledger>,
    pub unresolved_effects: Vec<Effect>,
    /// Historical pending questions; freshness and owner authorization are
    /// deliberately left to the existing question/resume handlers.
    pub pending_approvals: Vec<Approval>,
    pub waiting_for_input: Vec<TaskId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Discovery {
    pub workspace: WorkspaceId,
    pub watermark: Watermark,
    pub candidates: Vec<Candidate>,
    /// Full details remain available through task, history and cost inspectors.
    pub truncated: bool,
}

pub fn discover(state: &State, workspace: &WorkspaceId) -> Result<Discovery, String> {
    let rows = candidates(state, workspace)?;
    discovery(state.watermark, workspace, rows)
}

pub async fn discover_store(
    store: &impl CanonicalStore,
    workspace: &WorkspaceId,
) -> Result<Discovery, String> {
    let rows = candidates_store(store, workspace).await?;
    discovery(store.current().watermark, workspace, rows)
}

#[cfg(windows)]
pub fn discover_live(
    host: &vcp_lifecycle::foundation::CanonicalHost,
    workspace: &WorkspaceId,
) -> Result<Discovery, String> {
    let reader = host.history_reader()?;
    let mut members = BTreeMap::<TaskId, Vec<usize>>::new();
    let mut index = 0usize;
    let mut rows = candidates_from(reader.current(), workspace, |tasks| {
        for task in tasks {
            members.entry(task.clone()).or_default().push(index);
        }
        index += 1;
        None
    })?;
    let mut at = 0u64;
    loop {
        let page = reader.page(at.checked_sub(1), 256)?;
        if page.events.is_empty() && at < page.count {
            return Err("continuation history ended before its cut".into());
        }
        for event in page.events {
            at += 1;
            if &event.event.workspace != workspace {
                continue;
            }
            if let Some(indices) = event.event.task.as_ref().and_then(|task| members.get(task)) {
                for index in indices {
                    rows[*index].last_activity =
                        rows[*index].last_activity.max(Some(event.event.timestamp));
                }
            }
        }
        if at == page.count {
            break;
        }
        if at > page.count {
            return Err("continuation history exceeded its cut".into());
        }
    }
    sort(&mut rows);
    discovery(reader.current().watermark, workspace, rows)
}

fn discovery(
    watermark: Watermark,
    workspace: &WorkspaceId,
    rows: Vec<Candidate>,
) -> Result<Discovery, String> {
    let mut discovery = Discovery {
        workspace: workspace.clone(),
        watermark,
        truncated: rows.len() > 128,
        candidates: Vec::new(),
    };
    for row in rows.into_iter().take(128) {
        discovery.candidates.push(summarize(row)?);
    }
    Ok(discovery)
}

/// Bounds a single explicit-resume preview exactly like a chooser row.
pub fn summarize(mut row: Candidate) -> Result<Candidate, String> {
    row.details_truncated |= truncate_text(&mut row.objective, 1024);
    row.details_truncated |= truncate_text(&mut row.reason, 1024);
    for child in &mut row.children {
        row.details_truncated |= truncate_text(&mut child.reason, 256);
    }
    macro_rules! cap {
        ($($field:ident),+ $(,)?) => {$(
            if row.$field.len() > 8 {
                row.$field.truncate(8);
                row.details_truncated = true;
            }
        )+};
    }
    cap!(
        children,
        artifacts,
        observed_changes,
        unsettled_reservations,
        unresolved_effects,
        pending_approvals,
        waiting_for_input
    );
    if serde_json::to_vec(&row).map_err(|e| e.to_string())?.len() > 6 * 1024 {
        // Keep every selectable task identity/revision even when its
        // recovery details require the dedicated inspectors.
        row.children.clear();
        row.artifacts.clear();
        row.observed_changes.clear();
        row.unsettled_reservations.clear();
        row.ledger = None;
        row.unresolved_effects.clear();
        row.pending_approvals.clear();
        row.waiting_for_input.clear();
        // JSON escaping can expand control characters sixfold; these smaller
        // fallback limits bound even adversarial text after detail removal.
        truncate_text(&mut row.objective, 256);
        truncate_text(&mut row.reason, 256);
        row.details_truncated = true;
    }
    Ok(row)
}

fn truncate_text(text: &mut String, limit: usize) -> bool {
    if text.len() <= limit {
        return false;
    }
    let mut boundary = limit;
    while !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    text.truncate(boundary);
    true
}

/// Only reads the supplied snapshot, never opens an owner or dispatches work.
/// Callers resolve/reconcile the workspace binding before offering continuation.
/// Ordering is descending last activity, then ascending opaque task ID.
pub fn candidates(state: &State, workspace: &WorkspaceId) -> Result<Vec<Candidate>, String> {
    let mut result = candidates_from(state.into(), workspace, |members| {
        state
            .events
            .iter()
            .filter(|e| {
                &e.event.workspace == workspace
                    && e.event.task.as_ref().is_some_and(|t| members.contains(t))
            })
            .map(|e| e.event.timestamp)
            .max()
    })?;
    sort(&mut result);
    Ok(result)
}

/// Retains only one activity timestamp per current candidate while scanning
/// authenticated global rows once. Missing or failed history is never rendered
/// as an apparently inactive task.
pub async fn candidates_store(
    store: &impl CanonicalStore,
    workspace: &WorkspaceId,
) -> Result<Vec<Candidate>, String> {
    let watermark = store.current().watermark;
    let mut members = BTreeMap::<TaskId, Vec<usize>>::new();
    let mut row = 0usize;
    let mut result = candidates_from(store.current(), workspace, |tasks| {
        for task in tasks {
            members.entry(task.clone()).or_default().push(row);
        }
        row += 1;
        None
    })?;
    let count = store
        .history_event_count()
        .await
        .map_err(|e| e.to_string())?;
    let mut at = 0u64;
    while at < count {
        let limit = (count - at).min(256) as usize;
        let events = store
            .history_events(at.checked_sub(1), limit)
            .await
            .map_err(|e| e.to_string())?;
        if events.is_empty() || events.len() > limit || store.current().watermark != watermark {
            return Err("workspace continuation history cut changed or incomplete".into());
        }
        for event in events {
            if event.watermark > watermark {
                return Err("workspace continuation event exceeds source cut".into());
            }
            at += 1;
            if &event.event.workspace != workspace {
                continue;
            }
            if let Some(rows) = event.event.task.as_ref().and_then(|task| members.get(task)) {
                for row in rows {
                    result[*row].last_activity =
                        result[*row].last_activity.max(Some(event.event.timestamp));
                }
            }
        }
    }
    if store.current().watermark != watermark {
        return Err("workspace continuation source changed".into());
    }
    sort(&mut result);
    Ok(result)
}

fn candidates_from(
    state: CurrentStateView<'_>,
    workspace: &WorkspaceId,
    mut activity: impl FnMut(&BTreeSet<TaskId>) -> Option<Timestamp>,
) -> Result<Vec<Candidate>, String> {
    let mut tasks = BTreeMap::<TaskId, Task>::new();
    let mut effects = Vec::<Effect>::new();
    let mut artifacts = Vec::<ArtifactDescriptor>::new();
    let mut reservations = Vec::<Reservation>::new();
    let mut ledgers = Vec::<Ledger>::new();
    let mut approvals = Vec::<Approval>::new();
    for record in state.records.values().filter(|r| &r.workspace == workspace) {
        // Fail closed on malformed relevant records rather than silently hiding
        // recovery obligations from the chooser.
        match record.collection {
            Collection::Task => {
                let task: Task = record.decode().map_err(|e| e.to_string())?;
                tasks.insert(task.scope.task.clone(), task);
            }
            Collection::Effect => effects.push(record.decode().map_err(|e| e.to_string())?),
            Collection::Artifact => artifacts.push(record.decode().map_err(|e| e.to_string())?),
            Collection::Reservation => {
                reservations.push(record.decode().map_err(|e| e.to_string())?)
            }
            Collection::Ledger => ledgers.push(record.decode().map_err(|e| e.to_string())?),
            Collection::Approval => approvals.push(record.decode().map_err(|e| e.to_string())?),
            _ => {}
        }
    }
    let mut result = Vec::new();
    for root in tasks
        .values()
        .filter(|t| t.parent.is_none() && !t.state.terminal())
    {
        let members: BTreeSet<_> = tasks
            .values()
            .filter(|t| t.root == root.scope.task)
            .map(|t| t.scope.task.clone())
            .collect();
        let mut children = Vec::new();
        for child in tasks
            .values()
            .filter(|t| members.contains(&t.scope.task) && t.parent.is_some())
        {
            let mut paused_ancestors = Vec::new();
            let mut visited = BTreeSet::from([child.scope.task.clone()]);
            let mut parent = child.parent.as_ref();
            while let Some(id) = parent {
                if !visited.insert(id.clone()) {
                    return Err("cyclic task ancestry in workspace continuation".into());
                }
                let ancestor = tasks
                    .get(id)
                    .ok_or("missing task ancestor in workspace continuation")?;
                if ancestor.root != root.scope.task {
                    return Err("task ancestor crosses continuation root".into());
                }
                if ancestor.state == TaskState::Paused {
                    paused_ancestors.push(id.clone());
                }
                parent = ancestor.parent.as_ref();
            }
            children.push(Child {
                task: child.scope.task.clone(),
                parent: child.parent.clone(),
                state: child.state,
                reason: child.reason.clone(),
                expected_revision: child.revision,
                paused_ancestors,
            });
        }
        let last_activity = activity(&members);
        let observed_changes: BTreeSet<_> = effects
            .iter()
            .filter(|e| members.contains(&e.scope.task))
            .flat_map(|e| e.observed_changes.iter().cloned())
            .collect();
        result.push(Candidate {
            details_truncated: false,
            task: root.scope.task.clone(),
            session: root.scope.session.clone(),
            expected_revision: root.revision,
            state: root.state,
            objective: root
                .objectives
                .last()
                .map(|o| o.text.clone())
                .unwrap_or_default(),
            reason: root.reason.clone(),
            last_activity,
            children,
            artifacts: artifacts
                .iter()
                .filter(|a| members.contains(&a.spec.scope.task))
                .cloned()
                .collect(),
            observed_changes: observed_changes.into_iter().collect(),
            unsettled_reservations: reservations
                .iter()
                .filter(|r| {
                    r.root == root.scope.task
                        && matches!(
                            r.phase,
                            ReservationState::Created
                                | ReservationState::Submitted
                                | ReservationState::ReconciliationPending
                        )
                })
                .cloned()
                .collect(),
            ledger: ledgers
                .iter()
                .find(|l| l.scope.task == root.scope.task)
                .cloned(),
            unresolved_effects: effects
                .iter()
                .filter(|e| {
                    members.contains(&e.scope.task)
                        && matches!(
                            e.state,
                            EffectState::DispatchRecorded
                                | EffectState::Running
                                | EffectState::OutcomeUnknown
                        )
                })
                .cloned()
                .collect(),
            pending_approvals: approvals
                .iter()
                .filter(|a| members.contains(&a.scope.task) && a.state == ApprovalState::Pending)
                .cloned()
                .collect(),
            waiting_for_input: tasks
                .values()
                .filter(|t| {
                    members.contains(&t.scope.task) && t.state == TaskState::WaitingForInput
                })
                .map(|t| t.scope.task.clone())
                .collect(),
        });
    }
    Ok(result)
}

fn sort(result: &mut [Candidate]) {
    result.sort_by(|a, b| {
        b.last_activity
            .cmp(&a.last_activity)
            .then_with(|| a.task.cmp(&b.task))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vcp_domain::{revision::*, task::Objective, verification::Fingerprint, workspace::Scope};
    use vcp_protocol::event::{EventEnvelope, EventInput, EventKind};
    use vcp_store::contract::Record;

    struct Bounded<'a> {
        current: CurrentStateView<'a>,
        events: Vec<EventEnvelope>,
        mode: u8,
    }
    impl vcp_store::contract::reference::ReferenceStore for Bounded<'_> {
        fn state(&self) -> &State {
            panic!("live continuation must not request archival State")
        }
        fn current(&self) -> CurrentStateView<'_> {
            self.current
        }
        async fn history_event_count(&self) -> vcp_store::Result<u64> {
            Ok(self.events.len() as u64)
        }
        async fn history_events(
            &self,
            after: Option<u64>,
            limit: usize,
        ) -> vcp_store::Result<Vec<EventEnvelope>> {
            assert!((1..=256).contains(&limit));
            let first = after.map_or(0, |n| n + 1) as usize;
            if self.mode == 1 && first > 0 {
                return Err(vcp_store::Error::Corruption("injected history failure"));
            }
            if self.mode == 2 {
                return Ok(vec![]);
            }
            Ok(self.events.iter().skip(first).take(1).cloned().collect())
        }
        async fn transact(
            &mut self,
            _: vcp_store::contract::Transaction,
        ) -> vcp_store::Result<vcp_store::contract::Receipt> {
            panic!("read only")
        }
    }

    #[tokio::test]
    async fn bounded_continuation_and_agent_activity_match_archive_and_fail_closed() {
        let mut state = State::default();
        let workspace = WorkspaceId::new();
        let a = add_task(&mut state, &workspace, "a", None, TaskState::Paused);
        add_task(&mut state, &workspace, "b", None, TaskState::Paused);
        let child = add_task(
            &mut state,
            &workspace,
            "child",
            Some("b"),
            TaskState::Paused,
        );
        state.watermark = Watermark::new(1);
        for (index, (task, timestamp)) in [
            (child.scope.task.clone(), 9),
            (a.scope.task.clone(), 5),
            (child.scope.task.clone(), 3),
        ]
        .into_iter()
        .enumerate()
        {
            state.events.push(EventEnvelope {
                version: 1,
                sequence: SessionSeq::new(index as u64 + 1),
                watermark: state.watermark,
                redaction: None,
                event: EventInput {
                    id: EventId::new(),
                    workspace: workspace.clone(),
                    session: child.scope.session.clone(),
                    task: Some(task),
                    actor: ActorId::new(),
                    correlation: CommandId::new(),
                    causation: None,
                    timestamp: Timestamp::new(timestamp),
                    kind: EventKind::Commentary,
                    artifacts: vec![],
                    data: json!({}),
                    metadata: None,
                },
            });
        }
        let mut bounded = Bounded {
            current: (&state).into(),
            events: state.events.to_vec(),
            mode: 0,
        };
        let expected = serde_json::to_value(candidates(&state, &workspace).unwrap()).unwrap();
        let live =
            serde_json::to_value(candidates_store(&bounded, &workspace).await.unwrap()).unwrap();
        assert_eq!(live, expected);
        assert_eq!(live[0]["task"], "b");
        assert_eq!(live[0]["last_activity"], "9");
        let root = Scope {
            workspace: workspace.clone(),
            session: child.scope.session.clone(),
            task: child.root.clone(),
        };
        let expected = crate::agents_view::page(&state, &root, Timestamp::new(10), 0).unwrap();
        let live = crate::agents_view::page_store(&bounded, &root, Timestamp::new(10), 0)
            .await
            .unwrap();
        assert_eq!(live, expected);
        // Agent activity uses last canonical ordinal, not maximum timestamp.
        assert_eq!(live["items"][0]["last_activity"]["timestamp"], "3");
        for mode in [1, 2] {
            bounded.mode = mode;
            assert!(candidates_store(&bounded, &workspace).await.is_err());
            assert!(
                crate::agents_view::page_store(&bounded, &root, Timestamp::new(10), 0)
                    .await
                    .is_err()
            );
        }
    }

    fn add_task(
        state: &mut State,
        workspace: &WorkspaceId,
        id: &str,
        parent: Option<&str>,
        status: TaskState,
    ) -> Task {
        let task = Task {
            scope: Scope {
                workspace: workspace.clone(),
                session: SessionId::parse("session").unwrap(),
                task: TaskId::parse(id).unwrap(),
            },
            root: TaskId::parse(parent.unwrap_or(id)).unwrap(),
            parent: parent.map(|p| TaskId::parse(p).unwrap()),
            fork_origin: None,
            revision: Revision::new(3),
            steering: SteeringRevision::ZERO,
            objectives: vec![Objective {
                text: id.into(),
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
            reason: format!("own reason for {id}"),
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
        task
    }

    #[test]
    fn discovery_is_scoped_stable_and_does_not_mutate() {
        let mut state = State::default();
        let workspace = WorkspaceId::new();
        add_task(&mut state, &workspace, "b", None, TaskState::Paused);
        add_task(&mut state, &workspace, "a", None, TaskState::Paused);
        add_task(&mut state, &workspace, "done", None, TaskState::Completed);
        add_task(
            &mut state,
            &WorkspaceId::new(),
            "foreign",
            None,
            TaskState::Paused,
        );
        let original = state.clone();
        let rows = candidates(&state, &workspace).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.task.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(rows[0].expected_revision, Revision::new(3));
        assert_eq!(state, original);
    }

    #[test]
    fn child_activity_orders_root_and_independent_pause_remains_visible() {
        let mut state = State::default();
        let workspace = WorkspaceId::new();
        add_task(&mut state, &workspace, "a", None, TaskState::Paused);
        add_task(&mut state, &workspace, "b", None, TaskState::Paused);
        let child = add_task(
            &mut state,
            &workspace,
            "child",
            Some("b"),
            TaskState::Paused,
        );
        add_task(
            &mut state,
            &workspace,
            "cancelled",
            Some("b"),
            TaskState::Cancelled,
        );
        state.events.push(EventEnvelope {
            redaction: None,
            version: 1,
            sequence: SessionSeq::new(1),
            watermark: Watermark::new(1),
            event: EventInput {
                id: EventId::new(),
                workspace: workspace.clone(),
                session: child.scope.session.clone(),
                task: Some(child.scope.task.clone()),
                actor: ActorId::new(),
                correlation: CommandId::new(),
                causation: None,
                timestamp: Timestamp::new(8),
                kind: EventKind::TaskTransition,
                artifacts: vec![],
                data: json!({}),
                metadata: None,
            },
        });
        let rows = candidates(&state, &workspace).unwrap();
        assert_eq!(rows[0].task.as_str(), "b");
        let displayed = rows[0]
            .children
            .iter()
            .find(|c| c.task == child.scope.task)
            .unwrap();
        assert_eq!(displayed.reason, child.reason);
        assert_eq!(displayed.state, TaskState::Paused);
        assert_eq!(displayed.paused_ancestors, vec![child.root]);
        assert_eq!(rows[0].children[0].state, TaskState::Cancelled);
    }

    #[test]
    fn partial_changes_and_unsettled_child_money_are_visible_without_replay() {
        let mut state = State::default();
        let workspace = WorkspaceId::new();
        add_task(&mut state, &workspace, "root", None, TaskState::Paused);
        let child = add_task(
            &mut state,
            &workspace,
            "child",
            Some("root"),
            TaskState::WaitingForInput,
        );
        let artifact = ArtifactId::new();
        let effect = Effect {
            redaction: None,
            id: ToolRunId::new(),
            scope: child.scope.clone(),
            revision: Revision::ZERO,
            steering: SteeringRevision::ZERO,
            state: EffectState::OutcomeUnknown,
            operation_digest: "a".repeat(64),
            execution: Some(ExecutionId::new()),
            exit_code: None,
            observed_changes: vec![artifact.clone()],
            cause: EventId::new(),
            reason: "owner closed after dispatch".into(),
        };
        let effect_record = Record::typed(
            Collection::Effect,
            effect.id.as_str(),
            workspace.clone(),
            effect.revision,
            &effect,
        )
        .unwrap();
        state.records.insert(effect_record.key(), effect_record);
        let reservation: Reservation = serde_json::from_value(json!({
            "schema_version": 1, "id": ReservationId::new(), "scope": child.scope,
            "root": child.root, "attempt": AttemptId::new(), "revision": "0",
            "phase": "reconciliation_pending", "amount": {"currency":"USD", "micros":"100"},
            "charged":"0", "liability":"100", "protected_draw":"0", "protected_returned":"0",
            "day": 0, "role":"child"
        }))
        .unwrap();
        let reservation_record = Record::typed(
            Collection::Reservation,
            reservation.id.as_str(),
            workspace.clone(),
            reservation.revision,
            &reservation,
        )
        .unwrap();
        state
            .records
            .insert(reservation_record.key(), reservation_record);
        let original = state.clone();
        let rows = candidates(&state, &workspace).unwrap();
        assert_eq!(rows[0].observed_changes, [artifact]);
        assert_eq!(rows[0].unresolved_effects, [effect]);
        assert_eq!(rows[0].unsettled_reservations, [reservation]);
        assert_eq!(rows[0].waiting_for_input, [child.scope.task]);
        assert_eq!(state, original);
    }

    #[test]
    fn discovery_reports_row_limit_without_silently_selecting_omitted_tasks() {
        let mut state = State::default();
        let workspace = WorkspaceId::new();
        for i in 0..129 {
            add_task(
                &mut state,
                &workspace,
                &format!("task-{i:03}"),
                None,
                TaskState::Paused,
            );
        }
        let page = discover(&state, &workspace).unwrap();
        assert!(page.truncated);
        assert_eq!(page.candidates.len(), 128);
        assert_eq!(page.candidates[0].task.as_str(), "task-000");
        assert_eq!(page.watermark, state.watermark);
    }

    #[test]
    fn oversized_objective_keeps_selectable_task_and_reports_truncated_details() {
        let mut state = State::default();
        let workspace = WorkspaceId::new();
        let mut root = add_task(&mut state, &workspace, "root", None, TaskState::Paused);
        root.objectives[0].text = "界".repeat(20_000);
        let record = Record::typed(
            Collection::Task,
            "root",
            workspace.clone(),
            root.revision,
            &root,
        )
        .unwrap();
        state.records.insert(record.key(), record);
        let page = discover(&state, &workspace).unwrap();
        assert_eq!(page.candidates.len(), 1);
        assert_eq!(page.candidates[0].task, root.scope.task);
        assert_eq!(page.candidates[0].expected_revision, root.revision);
        assert!(page.candidates[0].details_truncated);
        assert!(page.candidates[0].objective.len() <= 1024);
        assert!(!page.truncated);
    }
}
