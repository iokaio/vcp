// SPDX-License-Identifier: Apache-2.0
#[cfg(test)]
extern crate self as vcp_store;
mod accounting_contract;
mod admitted_history;
pub mod artifact;
mod backend;
mod canonical_lock;
pub mod contract;
mod current_size;
mod current_state;
mod diagnostics;
mod durable_owner;
pub mod export_contract;
mod forecast_contract;
mod fork_contract;
mod historical_facts;
mod history;
mod history_blob;
mod history_catalog;
mod history_index;
mod history_publication;
mod journal_frame;
pub mod keys;
mod legacy_state_stream;
#[cfg(test)]
mod legacy_store_fixture;
mod memory_review_contract;
pub mod migration;
mod observer_contract;
mod original_commits;
pub mod portable_snapshot;
mod private_paths;
mod redaction_contract;
mod replay_base;
mod resolved_history;
mod restore_authority;
pub mod restore_import;
pub mod restore_stage;
pub mod rewrite;
pub mod snapshot_inputs;
pub mod snapshot_jobs;
mod store;
mod store_format;
mod store_history_reader;
pub mod trust_store;
pub mod vault_crypto;
pub mod vault_publish;
pub use backend::{BackendKind, Barrier};
pub use current_state::{CurrentState, CurrentStateView};
pub use diagnostics::{StoreDiagnostics, StorePhase, ValidationPhases};
pub use history::{CommandHistoryPage, EventHistoryPage};
pub use store::{snapshot_pin_active, Snapshot, Store};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("store conflict: {0}")]
    Conflict(&'static str),
    #[error("store integrity failure: {0}")]
    Corruption(&'static str),
    #[error("unsupported store format or backend")]
    Incompatible,
    #[error("store limit exceeded: {0}")]
    Limit(&'static str),
    #[error("workspace access denied")]
    Access,
    #[error("store unavailable: {0}")]
    Unavailable(&'static str),
    #[error("storage I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid serialized record: {0}")]
    Json(#[from] serde_json::Error),
    #[error("domain contract: {0}")]
    Domain(#[from] vcp_domain::Error),
    #[error("database failure: {0}")]
    Database(#[from] sqlx::Error),
}
pub type Result<T> = std::result::Result<T, Error>;

mod editor_contract;
