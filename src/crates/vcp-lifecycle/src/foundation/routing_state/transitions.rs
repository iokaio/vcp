// SPDX-License-Identifier: Apache-2.0
//! Rebuildable observations, not a fitted model or a completion forecast.
use super::{authorize, err, HistoryWindow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{
    task::{Task, TaskState},
    workspace::Workspace,
    *,
};
use vcp_memory::access::Access;
use vcp_memory::retention::{purged, Target};
use vcp_protocol::{canonical_bytes, digest_bytes, event::EventKind};
use vcp_store::{
    contract::{key, Collection},
    Store,
};

const MAX_SCAN: usize = 100_000;
const MAX_OBSERVATIONS: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub revision: Revision,
    pub state: TaskState,
    pub event: EventId,
    pub watermark: Watermark,
    pub timestamp: Timestamp,
    pub connected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapReason {
    RedactedEvent,
    MissingFacts,
    InvalidFact,
    RevisionGap,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gap {
    pub event: EventId,
    pub reason: GapReason,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trace {
    pub task: TaskId,
    pub observations: Vec<Observation>,
    pub gaps: Vec<Gap>,
    pub left_censored: bool,
    pub right_censored: bool,
}
impl Trace {
    fn new(task: TaskId) -> Self {
        Self {
            task,
            observations: vec![],
            gaps: vec![],
            left_censored: true,
            right_censored: true,
        }
    }
    fn gap(&mut self, event: &EventId, reason: GapReason) {
        self.gaps.push(Gap {
            event: event.clone(),
            reason,
        });
        self.right_censored = true;
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionCount {
    pub from: TaskState,
    pub to: TaskState,
    pub count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub schema_version: u32,
    pub alphabet: String,
    pub id: String,
    pub workspace: WorkspaceId,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
    pub cutoff: Watermark,
    pub window: HistoryWindow,
    pub source_tasks: Option<BTreeSet<TaskId>>,
    pub traces: Vec<Trace>,
    pub transitions: Vec<TransitionCount>,
    pub excluded_pruned_tasks: u64,
    pub limitations: Vec<String>,
}

/// One coherent current retained view. Never substitutes current task rows for
/// historical states or saves an aggregate that could outlive source deletion.
pub fn observe(store: &Store, access: &Access, window: HistoryWindow) -> Result<Evidence> {
    authorize(store, access, false)?;
    let state = store.state();
    if state.events.len() > MAX_SCAN || state.records.len() > MAX_SCAN {
        return Err("transition evidence exceeds 100000 canonical rows/events".into());
    }
    if window.from.is_some_and(|from| from >= window.until) {
        return Err("invalid transition evidence window".into());
    }
    let workspace: Workspace = state
        .record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        )
        .map_err(err)?
        .decode()
        .map_err(err)?;
    let mut traces = BTreeMap::<TaskId, Trace>::new();
    let mut previous = BTreeMap::<TaskId, (Revision, TaskState)>::new();
    let mut excluded = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut observations = 0usize;
    let mut scanned_facts = 0usize;
    // Store events are in canonical append order. Timestamps select the window;
    // they never order state transitions, including after clock rollback.
    for envelope in &state.events {
        let event = &envelope.event;
        if event.workspace != access.workspace
            || event.timestamp >= window.until
            || window.from.is_some_and(|from| event.timestamp < from)
            || (access.tasks.is_some()
                && !event.task.as_ref().is_some_and(|id| access.allows_task(id)))
        {
            continue;
        }
        if !seen.insert(event.id.clone()) {
            return Err("duplicate canonical event identity".into());
        }
        // Unknown event formats are never interpreted as version-one facts.
        let facts = (envelope.version == 1 && event.data["schema_version"] == 1)
            .then(|| event.data["facts"].as_array())
            .flatten();
        let unavailable = envelope.redaction.is_some()
            || purged(state, &access.workspace, &Target::Event(event.id.clone())).map_err(err)?;
        if unavailable || facts.is_none() {
            if matches!(
                event.kind,
                EventKind::TaskCreated
                    | EventKind::TaskTransition
                    | EventKind::ObjectiveChanged
                    | EventKind::FingerprintObserved
            ) {
                if let Some(id) = &event.task {
                    if eligible(store, access, id, &mut excluded)? {
                        let trace = traces
                            .entry(id.clone())
                            .or_insert_with(|| Trace::new(id.clone()));
                        trace.gap(
                            &event.id,
                            if unavailable {
                                GapReason::RedactedEvent
                            } else {
                                GapReason::MissingFacts
                            },
                        );
                        previous.remove(id);
                        observations += 1;
                    }
                }
            }
        } else if let Some(facts) = facts {
            scanned_facts = scanned_facts
                .checked_add(facts.len())
                .ok_or("transition fact count overflow")?;
            if scanned_facts > MAX_SCAN {
                return Err("transition evidence exceeds 100000 facts".into());
            }
            for fact in facts.iter().filter(|fact| fact["collection"] == "task") {
                let Some(id) = fact["id"].as_str().and_then(|id| TaskId::parse(id).ok()) else {
                    return Err("task fact lacks a valid identity".into());
                };
                if !eligible(store, access, &id, &mut excluded)? {
                    continue;
                }
                observations += 1;
                if observations > MAX_OBSERVATIONS {
                    return Err(
                        "transition evidence exceeds 4096 observations; narrow the window".into(),
                    );
                }
                let trace = traces
                    .entry(id.clone())
                    .or_insert_with(|| Trace::new(id.clone()));
                let task = serde_json::from_value::<Task>(fact["value"].clone())
                    .ok()
                    .filter(|task| {
                        task.scope.workspace == access.workspace
                            && task.scope.task == id
                            && task.scope.session == event.session
                            && task.cause == event.id
                            && serde_json::to_value(task.revision).ok().as_ref()
                                == Some(&fact["revision"])
                            && task.redaction.is_none()
                            && task.validate().is_ok()
                    });
                let Some(task) = task else {
                    trace.gap(&event.id, GapReason::InvalidFact);
                    previous.remove(&id);
                    continue;
                };
                let prior = previous.get(&id).copied();
                let connected = prior.is_some_and(|(revision, from)| {
                    revision.get().checked_add(1) == Some(task.revision.get()) && !from.terminal()
                });
                if prior.is_some() && !connected {
                    trace.gap(&event.id, GapReason::RevisionGap);
                    observations += 1;
                }
                if trace.observations.is_empty() && trace.gaps.is_empty() {
                    trace.left_censored =
                        task.revision != Revision::ZERO || task.state != TaskState::Pending;
                }
                trace.observations.push(Observation {
                    revision: task.revision,
                    state: task.state,
                    event: event.id.clone(),
                    watermark: envelope.watermark,
                    timestamp: event.timestamp,
                    connected,
                });
                trace.right_censored = !task.state.terminal();
                previous.insert(id, (task.revision, task.state));
            }
        }
        if observations > MAX_OBSERVATIONS {
            return Err("transition evidence exceeds 4096 observations; narrow the window".into());
        }
    }
    // Fixed alphabet order avoids relying on enum serialization/hash ordering.
    let alphabet = [
        TaskState::Pending,
        TaskState::Running,
        TaskState::WaitingForInput,
        TaskState::Blocked,
        TaskState::Paused,
        TaskState::Completed,
        TaskState::Failed,
        TaskState::Cancelled,
    ];
    let mut transitions = Vec::new();
    for from in alphabet {
        for to in alphabet {
            if from == to {
                continue;
            } // Steering/fingerprint revisions are not state visits.
            let count = traces
                .values()
                .flat_map(|trace| trace.observations.windows(2))
                .filter(|pair| pair[1].connected && pair[0].state == from && pair[1].state == to)
                .count() as u64;
            if count > 0 {
                transitions.push(TransitionCount { from, to, count });
            }
        }
    }
    let mut evidence = Evidence {
        schema_version: 1, alphabet: "canonical-task-state/1".into(), id: String::new(),
        workspace: access.workspace.clone(), authority: access.authority, deletion: workspace.deletion,
        cutoff: state.watermark, window, source_tasks: access.tasks.clone(),
        traces: traces.into_values().collect(), transitions, excluded_pruned_tasks: excluded.len() as u64,
        limitations: vec![
            "Observed task-state transitions only; not a fitted model, calibrated probability or routing input.".into(),
            "Missing/pruned/out-of-window revisions break chains. Blocked and paused tasks are resumable, not terminal.".into(),
            "Turn/action symbols, attempt counters, endpoint cohorts and reward attribution remain unavailable in this alphabet.".into(),
        ],
    };
    let encoded = canonical_bytes(&evidence).map_err(err)?;
    // Leave room for the CLI envelope within its 1 MiB private response limit.
    if encoded.len() > 512 * 1024 {
        return Err("transition evidence exceeds 512 KiB; narrow the window or task scope".into());
    }
    evidence.id = format!("transition-evidence-{}", digest_bytes(&encoded));
    Ok(evidence)
}

fn eligible(
    store: &Store,
    access: &Access,
    id: &TaskId,
    excluded: &mut BTreeSet<TaskId>,
) -> Result<bool> {
    if !access.allows_task(id) {
        return Ok(false);
    }
    let Some(record) = store
        .state()
        .records
        .get(&key(Collection::Task, id.as_str()))
        .filter(|record| record.workspace == access.workspace)
    else {
        return Ok(false);
    };
    if purged(
        store.state(),
        &access.workspace,
        &Target::Record(record.key()),
    )
    .map_err(err)?
    {
        excluded.insert(id.clone());
        return Ok(false);
    }
    let current: Task = record.decode().map_err(err)?;
    if current.redaction.is_some() {
        excluded.insert(id.clone());
        return Ok(false);
    }
    Ok(true)
}
