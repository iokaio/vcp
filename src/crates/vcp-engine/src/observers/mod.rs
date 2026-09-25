// SPDX-License-Identifier: Apache-2.0
//! Opt-in bounded local observation contracts. No provider or execution authority.
pub mod budget;
pub mod debounce;
pub mod dedup;
pub mod proposal;
pub mod subscription;
pub const VERSION: &str = "exact-verification-repetition/1";
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid observer state or input: {0}")]
    Invalid(&'static str),
    #[error("observer capacity exhausted")]
    Capacity,
    #[error("observer work is stale or already settled")]
    Stale,
}
