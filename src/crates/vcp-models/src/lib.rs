// SPDX-License-Identifier: Apache-2.0
//! OpenRouter conversion and bounded normalization. Transport remains in the
//! retained client and may only be reached through canonical host admission.
pub mod catalog;
pub mod decision;
pub mod escalation;
pub mod request;
pub mod retry;
pub mod routing;
pub mod stream;

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("provider capability unavailable: {0}")]
    Capability(&'static str),
    #[error("provider input exceeds {0}")]
    Limit(&'static str),
    #[error("invalid provider protocol: {0}")]
    Protocol(&'static str),
    #[error("provider metadata is stale or differs from qualification")]
    Stale,
    #[error("provider decimal value is invalid or overflows")]
    Decimal,
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
