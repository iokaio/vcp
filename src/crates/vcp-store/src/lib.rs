// SPDX-License-Identifier: Apache-2.0
mod accounting_contract;
pub mod artifact;
mod backend;
pub mod contract;
pub mod keys;
pub mod migration;
pub mod portable_snapshot;
mod private_paths;
mod redaction_contract;
mod replay_base;
mod restore_authority;
pub mod restore_import;
pub mod restore_stage;
pub mod rewrite;
pub mod snapshot_inputs;
pub mod snapshot_jobs;
mod store;
pub mod trust_store;
pub mod vault_crypto;
pub mod vault_publish;
pub use backend::{BackendKind, Barrier};
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
