// SPDX-License-Identifier: Apache-2.0
//! Versioned, pure hook contracts. Planning confers no execution authority.
pub mod input;
pub mod planner;
pub mod receipt;
pub mod registry;
pub mod result;
pub mod runner;

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid hook contract: {0}")]
    Invalid(&'static str),
    #[error("hook bound exceeded: {0}")]
    Limit(&'static str),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
pub(crate) fn digest<T: serde::Serialize>(value: &T) -> Result<String> {
    Ok(vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(
        value,
    )?))
}
pub(crate) fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
