// SPDX-License-Identifier: Apache-2.0
//! Process-local observations. Never canonical state, admission inputs, or authority.
use crate::BackendKind;
use serde::Serialize;
use std::time::Instant;
#[path = "diagnostics_history.rs"]
mod history;
pub use history::{EventValidationWork, HistoryReadPhases, HistoryReads};

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

/// Full semantic-validation phases. These are nested within `validation` and
/// report only reached phases; a failed earlier check leaves later counts alone.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ValidationPhases {
    pub capacity: StorePhase,
    pub records: StorePhase,
    pub events: StorePhase,
    pub redaction: StorePhase,
    pub accounting: StorePhase,
    pub ingestion: StorePhase,
    pub search: StorePhase,
    pub agents: StorePhase,
}

pub(crate) fn observe<T>(
    phase: Option<&mut StorePhase>,
    operation: impl FnOnce() -> crate::Result<T>,
) -> crate::Result<T> {
    match phase {
        Some(phase) => {
            let started = Instant::now();
            let result = operation();
            phase.record(started, result.is_ok());
            result
        }
        None => operation(),
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
    pub validation_phases: ValidationPhases,
    /// All reads reached during current preparation, including proposal and
    /// publication predicates. Phase counters below are subsets, not additive.
    /// Totals are merged when preparation returns success or error. Dropping
    /// its future before return does not merge that operation's totals; phase
    /// counters may already include phases completed before cancellation.
    pub history_reads: HistoryReads,
    pub validation_history_reads: HistoryReadPhases,
    pub event_validation_work: EventValidationWork,
    pub redaction_validation_work: EventValidationWork,
    pub ingestion_validation_work: EventValidationWork,
    pub append: StorePhase,
    pub checkpoint: StorePhase,
    pub replayed_commits: u64,
    pub checkpoint_replayed_commits: u64,
    pub checkpoint_state_comparisons: u64,
    pub replay_payload_bytes: u64,
    pub validation_input_records: u64,
    /// Logical event-history input size, preserved for compatibility. Actual
    /// Examined rows and prefix reuse are in the per-phase validation_work fields.
    pub validation_input_events: u64,
    pub materialized_records: u64,
    pub materialized_events: u64,
    pub materialized_commands: u64,
    pub verified_artifacts: u64,
    pub duplicate_transactions: u64,
    pub current_watermark: u64,
    pub state_size_full_scans: u64,
    pub state_size_delta_updates: u64,
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
            validation_phases: ValidationPhases::default(),
            history_reads: HistoryReads::default(),
            validation_history_reads: HistoryReadPhases::default(),
            event_validation_work: EventValidationWork::default(),
            redaction_validation_work: EventValidationWork::default(),
            ingestion_validation_work: EventValidationWork::default(),
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
            state_size_full_scans: 0,
            state_size_delta_updates: 0,
        }
    }
}
pub(crate) fn micros(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}
