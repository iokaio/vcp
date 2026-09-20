// SPDX-License-Identifier: Apache-2.0
//! Durable governed observations. Remembered commands never confer authority.
pub mod access;
pub mod extraction;
pub mod extractors;
mod fix_proof;
pub mod gates;
pub mod history;
pub mod ingest;
pub mod preferences;
pub mod projections;
pub mod proof;
pub mod repository;
pub mod runner;
pub use vcp_domain::memory;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("memory access denied or stale authority")]
    Access,
    #[error("memory conflict: {0}")]
    Conflict(&'static str),
    #[error("invalid memory input: {0}")]
    Invalid(String),
    #[error("domain: {0}")]
    Domain(#[from] vcp_domain::Error),
    #[error("store: {0}")]
    Store(#[from] vcp_store::Error),
    #[error("serialization: {0}")]
    Json(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
