// SPDX-License-Identifier: Apache-2.0
mod accounting_contract;
pub mod artifact;
mod backend;
pub mod contract;
pub mod migration;
mod redaction_contract;
mod replay_base;
pub mod rewrite;
mod store;
pub use backend::{BackendKind, Barrier};
pub use store::{Snapshot, Store};

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
