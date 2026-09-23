// SPDX-License-Identifier: Apache-2.0
//! Internal, versioned atomic metadata-fork facts; not a public wire method.
use serde::{Deserialize, Serialize};
use vcp_domain::{
    ids::*,
    revision::*,
    task::Task,
    workspace::{Scope, Session},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Format {
    #[serde(rename = "vcp-session-fork/1")]
    V1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Acceptance {
    pub document_type: Format,
    pub schema_version: u32,
    pub source: Scope,
    pub through_turn: TurnId,
    pub through_watermark: Watermark,
    pub new_session: SessionId,
    pub new_task: TaskId,
}

/// The target genesis is causally part of acceptance, but is not correlated as
/// another command receipt in the source session.
pub fn target_correlation(
    command: &CommandId,
    acceptance: &Acceptance,
) -> Result<CommandId, serde_json::Error> {
    let digest = crate::digest_bytes(&crate::canonical_bytes(&(
        "vcp-session-fork-target/1",
        command,
        acceptance,
    ))?);
    // SHA-256 and this fixed prefix are always valid identifiers. Deserialize
    // through the typed boundary rather than relying on an unchecked constructor.
    serde_json::from_value(serde_json::Value::String(format!("fork-target-{digest}")))
}

pub fn genesis_facts(session: &Session, task: &Task) -> serde_json::Value {
    serde_json::json!({"schema_version":1,"facts":[
        {"collection":"session","id":session.id,"revision":session.revision,"value":session},
        {"collection":"task","id":task.scope.task,"revision":task.revision,"value":task}
    ]})
}
