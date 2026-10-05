// SPDX-License-Identifier: Apache-2.0
//! Bounded current-owner timing evidence. Never execution or retry authority.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
    time::Instant,
};
use vcp_domain::{workspace::Scope, AttemptId, EventId, TurnId};

const RETAINED_SPANS: usize = 256;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    ContextAssembly,
    PolicyAdmission,
    ProviderExchange,
    /// Wrapper dispatch lifetime; canonical effects record physical outcomes.
    ToolDispatch,
    Verification,
    Repair,
    Resume,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Active,
    Succeeded,
    Failed,
    Interrupted,
    Skipped,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub sequence: u64,
    pub phase: Phase,
    pub scope: Scope,
    pub turn: Option<TurnId>,
    pub attempt: Option<AttemptId>,
    /// Validated call identity only; arguments and output never enter timing data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    /// Monotonic offset from this collector owner's origin, not wall time.
    pub started_micros: u64,
    pub elapsed_micros: u64,
    pub status: Status,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema_version: u32,
    pub owner: String,
    /// Snapshot offset from the same origin as Observation::started_micros.
    pub snapshot_micros: u64,
    /// Prior owner/crash timing is unavailable; canonical history is separate.
    pub window: String,
    pub complete_history: bool,
    pub available: bool,
    pub capacity: usize,
    pub dropped: u64,
    pub observations: Vec<Observation>,
}
impl Snapshot {
    pub fn for_scope(mut self, scope: &Scope) -> Self {
        self.observations
            .retain(|observation| observation.scope == *scope);
        self
    }
}
struct Active {
    started: Instant,
    observation: Observation,
}
struct State {
    owner: String,
    origin: Instant,
    next: u64,
    dropped: u64,
    active: BTreeMap<u64, Active>,
    finished: VecDeque<Observation>,
}
#[derive(Clone)]
pub(crate) struct Collector(Arc<Mutex<State>>);
impl Default for Collector {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(State {
            owner: EventId::new().to_string(),
            origin: Instant::now(),
            next: 0,
            dropped: 0,
            active: BTreeMap::new(),
            finished: VecDeque::new(),
        })))
    }
}
impl Collector {
    pub(crate) fn begin(
        &self,
        phase: Phase,
        scope: Scope,
        turn: Option<TurnId>,
        attempt: Option<AttemptId>,
    ) -> Span {
        let mut id = None;
        if let Ok(mut state) = self.0.lock() {
            if state.active.len() < RETAINED_SPANS && state.next < u64::MAX {
                if state.active.len() + state.finished.len() == RETAINED_SPANS {
                    state.finished.pop_front();
                    state.dropped = state.dropped.saturating_add(1);
                }
                let sequence = state.next;
                state.next += 1;
                let started = Instant::now();
                let started_micros = duration_micros(started.duration_since(state.origin));
                state.active.insert(
                    sequence,
                    Active {
                        started,
                        observation: Observation {
                            sequence,
                            phase,
                            scope,
                            turn,
                            attempt,
                            call_id: None,
                            started_micros,
                            elapsed_micros: 0,
                            status: Status::Active,
                        },
                    },
                );
                id = Some(sequence);
            } else {
                state.dropped = state.dropped.saturating_add(1);
            }
        }
        Span {
            collector: self.clone(),
            id,
        }
    }
    pub(crate) fn snapshot(&self) -> Snapshot {
        let mut snapshot = Snapshot {
            schema_version: 1,
            owner: String::new(),
            snapshot_micros: 0,
            window: "current_owner_only".into(),
            complete_history: false,
            available: false,
            capacity: RETAINED_SPANS,
            dropped: 0,
            observations: vec![],
        };
        if let Ok(state) = self.0.lock() {
            let captured = Instant::now();
            snapshot.owner.clone_from(&state.owner);
            snapshot.snapshot_micros = duration_micros(captured.duration_since(state.origin));
            snapshot.available = true;
            snapshot.dropped = state.dropped;
            snapshot.observations.extend(state.finished.iter().cloned());
            snapshot
                .observations
                .extend(state.active.values().map(|active| {
                    let mut observation = active.observation.clone();
                    observation.elapsed_micros =
                        duration_micros(captured.duration_since(active.started));
                    observation
                }));
            snapshot
                .observations
                .sort_by_key(|observation| observation.sequence);
        }
        snapshot
    }
}
pub(crate) struct Span {
    collector: Collector,
    id: Option<u64>,
}
impl Span {
    pub(crate) fn with_call_id(self, call_id: &str) -> Self {
        if !call_id.is_empty() && call_id.len() <= 256 && !call_id.chars().any(char::is_control) {
            if let (Some(id), Ok(mut state)) = (self.id, self.collector.0.lock()) {
                if let Some(active) = state.active.get_mut(&id) {
                    if active.observation.phase == Phase::ToolDispatch {
                        active.observation.call_id = Some(call_id.to_owned());
                    }
                }
            }
        }
        self
    }
    pub(crate) fn finish<T, E>(mut self, result: &Result<T, E>) {
        self.close(if result.is_ok() {
            Status::Succeeded
        } else {
            Status::Failed
        });
    }
    pub(crate) fn succeeded(mut self) {
        self.close(Status::Succeeded);
    }
    pub(crate) fn failed(mut self) {
        self.close(Status::Failed);
    }
    pub(crate) fn skipped(mut self) {
        self.close(Status::Skipped);
    }
    fn close(&mut self, status: Status) {
        let Some(id) = self.id.take() else {
            return;
        };
        if let Ok(mut state) = self.collector.0.lock() {
            if let Some(mut active) = state.active.remove(&id) {
                active.observation.elapsed_micros = elapsed(active.started);
                active.observation.status = status;
                if state.finished.len() == RETAINED_SPANS {
                    state.finished.pop_front();
                    state.dropped = state.dropped.saturating_add(1);
                }
                state.finished.push_back(active.observation);
            }
        }
    }
}
impl Drop for Span {
    fn drop(&mut self) {
        self.close(Status::Interrupted);
    }
}
fn elapsed(started: Instant) -> u64 {
    duration_micros(started.elapsed())
}
fn duration_micros(duration: std::time::Duration) -> u64 {
    duration.as_micros().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scope() -> Scope {
        Scope {
            workspace: vcp_domain::WorkspaceId::parse("diagnostic-workspace").unwrap(),
            session: vcp_domain::SessionId::parse("diagnostic-session").unwrap(),
            task: vcp_domain::TaskId::parse("diagnostic-task").unwrap(),
        }
    }
    #[test]
    fn statuses_and_scope_are_observed_without_inventing_completion() {
        let collector = Collector::default();
        let span = collector.begin(Phase::Verification, scope(), None, None);
        let snapshot = collector.snapshot();
        assert!(!snapshot.complete_history);
        assert_eq!(snapshot.observations[0].status, Status::Active);
        assert!(
            snapshot.observations[0].started_micros + snapshot.observations[0].elapsed_micros
                <= snapshot.snapshot_micros
        );
        drop(span);
        assert_eq!(
            collector.snapshot().observations[0].status,
            Status::Interrupted
        );
        collector
            .begin(Phase::Repair, scope(), None, None)
            .finish(&Err::<(), _>("fixture rejection"));
        collector
            .begin(Phase::Resume, scope(), None, None)
            .succeeded();
        let snapshot = collector.snapshot();
        assert_eq!(
            snapshot
                .observations
                .iter()
                .map(|o| o.status)
                .collect::<Vec<_>>(),
            vec![Status::Interrupted, Status::Failed, Status::Succeeded]
        );
        assert!(snapshot.observations.iter().all(|o| o.scope == scope()));
    }
    #[test]
    fn bounded_retention_reports_omissions_and_new_owner_is_separate() {
        let collector = Collector::default();
        for _ in 0..300 {
            collector
                .begin(Phase::ContextAssembly, scope(), None, None)
                .succeeded();
        }
        let snapshot = collector.snapshot();
        assert_eq!(snapshot.observations.len(), RETAINED_SPANS);
        assert_eq!(snapshot.dropped, 44);
        assert_ne!(snapshot.owner, Collector::default().snapshot().owner);
    }

    #[test]
    fn call_metadata_is_bounded_and_absent_from_legacy_non_tool_observations() {
        let collector = Collector::default();
        collector
            .begin(Phase::ProviderExchange, scope(), None, None)
            .with_call_id("not-a-tool")
            .succeeded();
        collector
            .begin(Phase::ToolDispatch, scope(), None, None)
            .with_call_id(&"x".repeat(257))
            .failed();
        collector
            .begin(Phase::ToolDispatch, scope(), None, None)
            .with_call_id("call\nspoof")
            .skipped();
        let snapshot = serde_json::to_value(collector.snapshot()).unwrap();
        assert!(snapshot["observations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|span| span.get("call_id").is_none()));
    }
}
