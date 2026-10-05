// SPDX-License-Identifier: Apache-2.0
//! Aggregate physical-read and memo observations. No identities or content.
use serde::{Deserialize, Serialize};

macro_rules! counters {
    ($($field:ident),+ $(,)?) => {
        /// `physical_*` measures only calls to the underlying Pages reader,
        /// including index and blob objects, never memo hits. Await duration
        /// includes scheduling/backend work; subtracting it is not pure CPU time.
        /// Bytes count successfully returned bytes, not bytes on failed I/O.
        /// Session-local started reads may exceed completed reads if a read is
        /// cancelled. Store totals merge only when preparation returns; they
        /// do not observe a preparation whose entire future was dropped.
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(default)]
        pub struct HistoryReads { $(pub $field: u64,)+ }
        impl HistoryReads {
            pub(crate) fn add(&mut self, other: Self) {
                $(self.$field = self.$field.saturating_add(other.$field);)+
            }
            pub(crate) fn since(self, earlier: Self) -> Self {
                Self { $($field: self.$field.saturating_sub(earlier.$field),)+ }
            }
        }
    };
}
counters! {
    physical_started, physical_completed, physical_failed, physical_bytes,
    physical_elapsed_micros,
    index_hits, index_misses, index_rejected_hits,
    index_admitted, index_refused_entries, index_refused_bytes,
    payload_hits, payload_misses, payload_admitted,
    payload_refused_entries, payload_refused_bytes,
}

/// Subsets of preparation totals, measured at the existing ordered phase
/// boundaries. Missing legacy fields decode as zero; zero is not proof that an
/// older producer instrumented reads. Consumers must check field presence in
/// legacy StoreDiagnostics before interpreting zero counts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryReadPhases {
    pub capacity: HistoryReads,
    pub records: HistoryReads,
    pub events: HistoryReads,
    pub redaction: HistoryReads,
    pub accounting: HistoryReads,
    pub ingestion: HistoryReads,
    pub search: HistoryReads,
    pub agents: HistoryReads,
}
