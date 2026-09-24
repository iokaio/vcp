// SPDX-License-Identifier: Apache-2.0
use super::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    SessionStart,
    TaskStart,
    BeforeContextAssembly,
    BeforeToolAuthorization,
    AfterToolCompletion,
    BeforeCompaction,
    AfterVerification,
    TaskCompletion,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailurePolicy {
    Block,
    Warn,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HookCommand {
    pub profile: String,
    pub arguments: Vec<String>,
    pub working_directory: String,
}
/// The named broker profile owns executable identity, environment, authority and
/// platform enforcement. This does not assert filesystem/network sandboxing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEffectScope {
    BrokerProfile,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HookDefinition {
    pub id: String,
    pub version: u32,
    pub source_hash: String,
    pub event: HookEvent,
    pub priority: i32,
    pub before: Vec<String>,
    pub after: Vec<String>,
    pub command: HookCommand,
    pub effect_scope: HookEffectScope,
    pub timeout_ms: u64,
    pub max_output_bytes: usize,
    pub failure_policy: FailurePolicy,
}
impl HookDefinition {
    pub fn validate(&self) -> Result<()> {
        if serde_json::to_vec(self)?.len() > 65_536 {
            return Err(Error::Limit("hook definition bytes"));
        }
        if self.failure_policy == FailurePolicy::Warn
            && matches!(
                self.event,
                HookEvent::BeforeContextAssembly
                    | HookEvent::BeforeToolAuthorization
                    | HookEvent::BeforeCompaction
            )
        {
            return Err(Error::Invalid("gating events must fail closed"));
        }
        if self.id.is_empty()
            || self.id.len() > 128
            || !self
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-.:".contains(&b))
            || self.version == 0
            || !super::hash(&self.source_hash)
        {
            return Err(Error::Invalid("hook identity"));
        }
        if self.timeout_ms == 0
            || self.timeout_ms > 300_000
            || self.max_output_bytes == 0
            || self.max_output_bytes > 1_048_576
        {
            return Err(Error::Limit("execution bounds"));
        }
        if self.command.profile.is_empty()
            || self.command.profile.contains('\0')
            || self.command.working_directory.contains('\0')
            || self.command.arguments.iter().any(|s| s.contains('\0'))
        {
            return Err(Error::Invalid("command"));
        }
        if self
            .command
            .arguments
            .iter()
            .any(|s| s.starts_with("--vcp-hook-input-sha256"))
        {
            return Err(Error::Invalid("reserved input digest argument"));
        }
        let mut names = BTreeSet::new();
        if self
            .before
            .iter()
            .chain(&self.after)
            .any(|n| n == &self.id || !names.insert(n))
        {
            return Err(Error::Invalid("ambiguous ordering"));
        }
        Ok(())
    }
}
