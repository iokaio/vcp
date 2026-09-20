// SPDX-License-Identifier: Apache-2.0
//! Scoped observations and native file primitives. Mutations require authority
//! from the trusted broker; repository contents cannot grant it. Git index
//! changes are not performed by these file primitives.
pub mod discovery;
pub mod git;
pub mod instructions;
#[cfg(windows)]
pub mod mutation;
pub mod observation;
pub mod path;
#[cfg(windows)]
pub mod restore;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use vcp_domain::{ByteCount, Revision, RootId, WorkspaceId};

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid or unauthorized path: {0}")]
    Scope(String),
    #[error("link/reparse target is outside the qualified no-follow contract")]
    Link,
    #[error("observation limit: {0}")]
    Limit(&'static str),
    #[error("source changed since observation")]
    Stale,
    #[error("unsupported observation: {0}")]
    Unsupported(&'static str),
    #[error("invalid Git observation: {0}")]
    Git(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootIdentity {
    pub workspace: WorkspaceId,
    pub root: RootId,
    pub repository: String,
    pub worktree: String,
    pub binding: Revision,
}

#[derive(Clone, Debug)]
pub struct Root {
    pub identity: RootIdentity,
    path: PathBuf,
    directory_identity: String,
}
impl Root {
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileVersion {
    pub root: RootId,
    pub binding: Revision,
    pub path: String,
    pub native_identity: String,
    pub sha256: String,
    pub bytes: ByteCount,
}
/// Bytes are deliberately separate from the serializable observation manifest.
/// Consumers durably capture them before making a ContextPart reference.
pub struct Source {
    pub version: FileVersion,
    pub bytes: Vec<u8>,
}
