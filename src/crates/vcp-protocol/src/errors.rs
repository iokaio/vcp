// SPDX-License-Identifier: Apache-2.0
//! Stable application failures, separate from standard JSON-RPC syntax errors.
use crate::{jsonrpc::RpcError, methods::Id};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Code {
    PolicyDenied,
    ApprovalRequired,
    ApprovalStale,
    BudgetExhausted,
    CapabilityUnavailable,
    VersionConflict,
    ProviderRetryable,
    ProviderRejected,
    StoreUnavailable,
    IndexNotReady,
    OutcomeUnknown,
    Cancelled,
    CommandConflict,
    InputRequired,
    AuthorityStale,
    UnsupportedVersion,
    CursorGap,
    ResourceLimit,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Retry {
    Never,
    AfterInput,
    AfterRevalidation,
    /// Resolve the original durable command; do not invent a replacement ID.
    ReconcileOriginal,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ApplicationError {
    pub code: Code,
    pub retry: Retry,
    pub operation: Option<Id>,
    /// Safe static explanation, never raw provider, filesystem or token errors.
    pub explanation: String,
    pub reconciliation: Option<String>,
}
impl ApplicationError {
    pub fn into_rpc(self) -> RpcError {
        match serde_json::to_value(self) {
            Ok(details) => RpcError::application("application", details),
            Err(_) => RpcError::internal_error(),
        }
    }
}
