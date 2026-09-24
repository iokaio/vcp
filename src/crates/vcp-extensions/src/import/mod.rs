// SPDX-License-Identifier: Apache-2.0
//! Data-only, explicit foreign configuration subsets. No lookup or execution.
pub mod apply;
pub mod codex;
pub mod compatibility;
pub mod gemini;
pub mod normalize;
pub mod preview;
pub use normalize::{Preferences, Restriction};

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid import: {0}")]
    Invalid(&'static str),
    #[error("import limit: {0}")]
    Limit(&'static str),
    #[error("unsupported source compatibility version")]
    Version,
    #[error("import preview is stale or changed")]
    Stale,
}
pub(crate) fn digest<T: serde::Serialize + ?Sized>(value: &T) -> Result<String> {
    let bytes =
        vcp_protocol::canonical_bytes(&value).map_err(|_| Error::Invalid("serialization"))?;
    Ok(vcp_protocol::digest_bytes(&bytes))
}
pub(crate) fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}
