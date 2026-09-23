// SPDX-License-Identifier: Apache-2.0
//! Pure scoped identities and state transitions. No storage, transport or UI.
pub mod accounting;
pub mod agents;
pub mod artifact;
pub mod controller;
pub mod effect;
pub mod forecast;
pub mod ids;
pub mod ingestion;
pub mod memory;
pub mod policy;
pub mod redaction;
pub mod retention;
pub mod retention_selector;
pub mod revision;
pub mod search;
pub mod task;
pub mod verification;
pub mod workspace;

pub use ids::*;
pub use revision::*;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("invalid {0}")]
    Invalid(&'static str),
    #[error("workspace or entity scope mismatch")]
    Scope,
    #[error("stale entity revision")]
    Stale,
    #[error("steering changed; replan before dispatch")]
    Steering,
    #[error("illegal lifecycle transition")]
    Transition,
    #[error("current completion evidence is insufficient")]
    Evidence,
    #[error("resume requires current workspace, policy, budget and effect reconciliation")]
    Revalidation,
    #[error("counter overflow")]
    Overflow,
}

pub type Result<T> = std::result::Result<T, Error>;
