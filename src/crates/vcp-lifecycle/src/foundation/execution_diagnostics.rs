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
const RETAINED_ENCODINGS: usize = 64;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncodingPurpose {
    CompactionTrial,
    CandidateFit,
    FinalAssembly,
    SealedValidation,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncodingObservation {
    pub scope: Scope,
    pub turn: Option<TurnId>,
    /// These operations precede provider attempt admission.
    pub attempt: Option<AttemptId>,
    pub purpose: EncodingPurpose,
    /// Catalog snapshot identity, binding model and endpoint. Noncanonical
    /// diagnostic-only identities are hashed to keep this field bounded.
    pub catalog: String,
    pub reasoning: Option<vcp_models::reasoning::Effort>,
    pub request_sha256: Option<String>,
    pub started_micros: u64,
    pub elapsed_micros: u64,
    pub work: vcp_models::request::EncodingWork,
}
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
    /// False for old snapshots and unavailable collectors; absence is not zero work.
    #[serde(default)]
    pub encoding_available: bool,
    /// Independent bound: encoding trials never evict failure/verification spans.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub encodings: Vec<EncodingObservation>,
    #[serde(default)]
    pub encodings_dropped: u64,
}
impl Snapshot {
    pub fn for_scope(mut self, scope: &Scope) -> Self {
        self.observations
            .retain(|observation| observation.scope == *scope);
        self.encodings
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
    encodings: VecDeque<EncodingObservation>,
    encodings_dropped: u64,
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
            encodings: VecDeque::new(),
            encodings_dropped: 0,
        })))
    }
}
impl Collector {
    pub(crate) fn encoding(
        &self,
        scope: Scope,
        turn: Option<TurnId>,
        purpose: EncodingPurpose,
        catalog: &str,
        reasoning: Option<vcp_models::reasoning::Effort>,
    ) -> EncodingSession {
        let started = Instant::now();
        let started_micros = self
            .0
            .lock()
            .map(|state| duration_micros(started.duration_since(state.origin)))
            .unwrap_or(0);
        EncodingSession {
            collector: self.clone(),
            started,
            observation: EncodingObservation {
                scope,
                turn,
                attempt: None,
                purpose,
                catalog: if catalog.len() == 64 && catalog.bytes().all(|b| b.is_ascii_hexdigit()) {
                    catalog.to_owned()
                } else {
                    vcp_protocol::digest_bytes(catalog.as_bytes())
                },
                reasoning,
                request_sha256: None,
                started_micros,
                elapsed_micros: 0,
                work: Default::default(),
            },
            work: Default::default(),
        }
    }
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
            encoding_available: false,
            encodings: vec![],
            encodings_dropped: 0,
        };
        if let Ok(state) = self.0.lock() {
            let captured = Instant::now();
            snapshot.owner.clone_from(&state.owner);
            snapshot.snapshot_micros = duration_micros(captured.duration_since(state.origin));
            snapshot.available = true;
            snapshot.encoding_available = true;
            snapshot.dropped = state.dropped;
            snapshot.encodings = state.encodings.iter().cloned().collect();
            snapshot.encodings_dropped = state.encodings_dropped;
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
pub(crate) struct EncodingSession {
    collector: Collector,
    started: Instant,
    observation: EncodingObservation,
    pub(crate) work: std::cell::RefCell<vcp_models::request::EncodingWork>,
}
impl EncodingSession {
    pub(crate) fn request(&mut self, digest: &str) {
        if digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            self.observation.request_sha256 = Some(digest.to_owned());
        }
    }
}
impl Drop for EncodingSession {
    fn drop(&mut self) {
        let work = self.work.get_mut();
        if work.encode_calls == 0 && work.validation_calls == 0 {
            return;
        }
        self.observation.work = work.clone();
        self.observation.elapsed_micros = elapsed(self.started);
        if let Ok(mut state) = self.collector.0.lock() {
            if state.encodings.len() == RETAINED_ENCODINGS {
                state.encodings.pop_front();
                state.encodings_dropped = state.encodings_dropped.saturating_add(1);
            }
            state.encodings.push_back(self.observation.clone());
        }
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
    #[test]
    fn encoding_aggregates_are_separately_bounded_scoped_and_legacy_defaulted() {
        let collector = Collector::default();
        collector
            .begin(Phase::Verification, scope(), None, None)
            .failed();
        let own_scope = scope();
        for i in 0..70 {
            let mut session = collector.encoding(
                own_scope.clone(),
                None,
                EncodingPurpose::CandidateFit,
                &format!("{i:064x}"),
                None,
            );
            // Repeated trials are one aggregate, not 100 retained observations.
            session.work.get_mut().encode_calls = 100;
            session.work.get_mut().encode_failures = 1;
        }
        let snapshot = collector.snapshot();
        assert_eq!(snapshot.observations.len(), 1);
        assert_eq!(snapshot.observations[0].status, Status::Failed);
        assert_eq!(snapshot.dropped, 0);
        // Optional payload-free serialization evidence for the JS analyzer.
        // Unique create-only files preserve previous qualification runs.
        if let Some(directory) = std::env::var_os("VCP_TEST_ENCODING_EVIDENCE") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join(format!("collector-{}.json", EventId::new()));
            let file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .unwrap();
            serde_json::to_writer(file, &serde_json::json!({"kind":"synthetic_collector_serialization", "scope":own_scope,"snapshot":snapshot})).unwrap();
        }
        assert_eq!(
            (snapshot.encodings.len(), snapshot.encodings_dropped),
            (64, 6)
        );
        assert!(snapshot
            .encodings
            .iter()
            .all(|row| row.work.encode_calls == 100
                && row.started_micros + row.elapsed_micros <= snapshot.snapshot_micros));
        assert!(snapshot
            .clone()
            .for_scope(&Scope {
                task: vcp_domain::TaskId::new(),
                ..own_scope
            })
            .encodings
            .is_empty());
        let mut legacy = serde_json::to_value(snapshot).unwrap();
        legacy.as_object_mut().unwrap().remove("encodings");
        legacy.as_object_mut().unwrap().remove("encodings_dropped");
        legacy.as_object_mut().unwrap().remove("encoding_available");
        let decoded: Snapshot = serde_json::from_value(legacy).unwrap();
        assert!(decoded.encodings.is_empty());
        assert_eq!(decoded.encodings_dropped, 0);
        assert!(!decoded.encoding_available);
    }
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
