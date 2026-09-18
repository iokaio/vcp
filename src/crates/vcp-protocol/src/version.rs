// SPDX-License-Identifier: Apache-2.0
pub const VERSION: u32 = 1;
pub const MAX_COMMAND_BYTES: usize = 256 * 1024;
pub const MAX_PAGE_EVENTS: usize = 128;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unsupported internal protocol version {0}; this build accepts version 1")]
    Version(u32),
    #[error("command exceeds the 256 KiB internal envelope limit")]
    Limit,
    #[error("invalid internal envelope: {0}")]
    Json(#[from] serde_json::Error),
}
