// SPDX-License-Identifier: Apache-2.0
//! Canonical command orchestration. Execution stays in the retained controller;
//! handlers record decisions and effects through an injected canonical store.
pub mod agents;
pub mod capture;
pub mod command_handler;
pub mod controller;
mod fork;
pub mod policy;
pub mod public;
pub mod public_diff;
pub mod public_evidence;
pub mod public_export;
pub mod public_reads;
pub mod public_presentation;
pub mod public_start;
pub mod query;
pub mod questions;
pub mod rpc;
pub mod snapshot;
mod subscription;
pub use command_handler::{Access, Engine, HostFacts};
pub use subscription::ProjectedEvents;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("current caller access denied")]
    Access,
    #[error("stale controller owner")]
    Owner,
    #[error("command target or immutable decision differs")]
    Target,
    #[error("operation requires current host evidence")]
    Host,
    #[error("canonical target evidence is unavailable in the authorized scope")]
    Unavailable,
    #[error("domain: {0}")]
    Domain(#[from] vcp_domain::Error),
    #[error("canonical store: {0}")]
    Store(#[from] vcp_store::Error),
    #[error("protocol: {0}")]
    Protocol(#[from] vcp_protocol::version::Error),
    #[error("serialization: {0}")]
    Json(#[from] serde_json::Error),
    #[error("authority policy: {0}")]
    Policy(#[from] vcp_policy::Error),
}
pub type Result<T> = std::result::Result<T, Error>;

pub mod editor;
