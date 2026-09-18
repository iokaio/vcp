// SPDX-License-Identifier: Apache-2.0
pub mod arithmetic;
pub mod service;
pub use service::*;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unknown, expired or mismatched price/capability information")]
    Quote,
    #[error("currency units differ")]
    Currency,
    #[error("checked accounting arithmetic overflow")]
    Overflow,
    #[error("budget denied: {0}")]
    Denied(&'static str),
    #[error("accounting conflict: {0}")]
    Conflict(&'static str),
    #[error("domain: {0}")]
    Domain(#[from] vcp_domain::Error),
    #[error("store: {0}")]
    Store(#[from] vcp_store::Error),
    #[error("serialization: {0}")]
    Json(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
