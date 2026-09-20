// SPDX-License-Identifier: Apache-2.0
pub mod history;
pub mod history_query;
pub mod inspection;
pub mod projection;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("history access denied")]
    Access,
    #[error("history data removed by current retention scope")]
    Removed,
    #[error("history cursor requires a fresh snapshot: {0}")]
    Restart(&'static str),
    #[error("unsupported projection or history format")]
    Version,
    #[error("projection input differs: {0}")]
    Integrity(&'static str),
    #[error("bounded history capacity exceeded")]
    Limit,
    #[error("store: {0}")]
    Store(#[from] vcp_store::Error),
    #[error("domain: {0}")]
    Domain(#[from] vcp_domain::Error),
    #[error("serialization: {0}")]
    Json(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
