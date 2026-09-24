// SPDX-License-Identifier: Apache-2.0
//! Explicit data-only skill discovery. Skill content grants no execution authority.
pub mod activation;
pub mod catalog;
pub mod discovery;
pub mod hooks;
pub mod mcp;
pub mod skill_manifest;

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid skill metadata: {0}")]
    Metadata(String),
    #[error("skill limit: {0}")]
    Limit(&'static str),
    #[error("skill source or dependency changed; rediscover before activation")]
    Stale,
    #[error("skill is unavailable: {0}")]
    Unavailable(String),
    #[error(transparent)]
    Repository(#[from] vcp_repository::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub(crate) fn digest<T: serde::Serialize>(value: &T) -> Result<String> {
    // These schemas use only string keys, integers and ordered sets.
    Ok(vcp_protocol::digest_bytes(&serde_json::to_vec(value)?))
}
