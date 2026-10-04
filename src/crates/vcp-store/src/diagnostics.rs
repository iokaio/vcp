// SPDX-License-Identifier: Apache-2.0
//! Process-local observations. Never canonical state, admission inputs, or authority.
use crate::BackendKind;
use serde::Serialize;
use std::time::Instant;

/// A completed phase count and its cumulative monotonic duration. Timings may
/// overlap: validation is included in preparation/replay, which are included in open.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct StorePhase {
    pub completed: u64,
    pub failed: u64,
    pub elapsed_micros: u64,
}
impl StorePhase {
    pub(crate) fn record(&mut self, started: Instant, success: bool) {
        self.completed = self.completed.saturating_add(1);
        self.failed = self.failed.saturating_add(u64::from(!success));
        self.elapsed_micros = self.elapsed_micros.saturating_add(micros(started));
    }
}

/// Bounded, payload-free observations for this store owner. Reopening starts a
/// new snapshot; counters must not be mistaken for lifetime durable totals.
#[derive(Clone, Debug, Serialize)]
pub struct StoreDiagnostics {
    pub schema_version: u32,
    pub backend: BackendKind,
    pub open: StorePhase,
    pub replay_base: StorePhase,
    pub backend_open: StorePhase,
    pub replay: StorePhase,
    pub materialized_verification: StorePhase,
    pub checkpoint_loading: StorePhase,
    pub checkpoint_verification: StorePhase,
    pub artifact_verification: StorePhase,
    pub preparation: StorePhase,
    pub validation: StorePhase,
    pub append: StorePhase,
    pub checkpoint: StorePhase,
    pub replayed_commits: u64,
    pub checkpoint_replayed_commits: u64,
    pub checkpoint_state_comparisons: u64,
    pub replay_payload_bytes: u64,
    pub validation_input_records: u64,
    pub validation_input_events: u64,
    pub materialized_records: u64,
    pub materialized_events: u64,
    pub materialized_commands: u64,
    pub verified_artifacts: u64,
    pub duplicate_transactions: u64,
    pub current_watermark: u64,
}
impl StoreDiagnostics {
    pub(crate) fn new(backend: BackendKind) -> Self {
        Self {
            schema_version: 1,
            backend,
            open: StorePhase::default(),
            replay_base: StorePhase::default(),
            backend_open: StorePhase::default(),
            replay: StorePhase::default(),
            materialized_verification: StorePhase::default(),
            checkpoint_loading: StorePhase::default(),
            checkpoint_verification: StorePhase::default(),
            artifact_verification: StorePhase::default(),
            preparation: StorePhase::default(),
            validation: StorePhase::default(),
            append: StorePhase::default(),
            checkpoint: StorePhase::default(),
            replayed_commits: 0,
            checkpoint_replayed_commits: 0,
            checkpoint_state_comparisons: 0,
            replay_payload_bytes: 0,
            validation_input_records: 0,
            validation_input_events: 0,
            materialized_records: 0,
            materialized_events: 0,
            materialized_commands: 0,
            verified_artifacts: 0,
            duplicate_transactions: 0,
            current_watermark: 0,
        }
    }
}
pub(crate) fn micros(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}
