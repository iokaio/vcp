// SPDX-License-Identifier: Apache-2.0
//! Data only. Host must resolve profile/auth references and obtain broker authority.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{
    policy::{EffectClass, GrantScope},
    Revision,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub id: String,
    pub revision: Revision,
    pub scope: GrantScope,
    pub transport: Transport,
    pub auth_refs: BTreeSet<String>,
    pub allowed_tools: BTreeSet<String>,
    #[serde(default)]
    pub allowed_resources: BTreeSet<String>,
    #[serde(default)]
    pub allowed_prompts: BTreeSet<String>,
    pub trusted_effects: BTreeMap<String, BTreeSet<EffectClass>>,
    pub limits: Limits,
    pub capabilities: Capabilities,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Transport {
    Stdio {
        process_profile: String,
        resolved_digest: String,
    },
    // Endpoint details must live in the resolved trusted remote profile, keeping
    // credential-bearing URL queries/userinfo out of this pure serialized record.
    StreamableHttp {
        remote_profile: String,
        resolved_digest: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub frame_bytes: u64,
    pub total_discovery_bytes: u64,
    pub tools: u64,
    pub pages: u64,
    pub timeout_ms: u64,
    pub stderr_bytes: u64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Capabilities {
    pub resources: bool,
    pub prompts: bool,
    pub sampling: bool,
    pub roots: bool,
    pub elicitation: bool,
    pub tasks: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("invalid MCP registration")]
    Invalid,
    #[error("MCP capability is not implemented by this profile")]
    UnsupportedCapability,
}
impl Registration {
    pub fn validate(&self) -> Result<(), Error> {
        let (profile, digest) = match &self.transport {
            Transport::Stdio {
                process_profile,
                resolved_digest,
            } => (process_profile, resolved_digest),
            Transport::StreamableHttp {
                remote_profile,
                resolved_digest,
            } => (remote_profile, resolved_digest),
        };
        if !identifier(&self.id)
            || !identifier(profile)
            || !sha256(digest)
            || self.auth_refs.len() > 16
            || self.auth_refs.iter().any(|id| !identifier(id))
            || self.allowed_tools.len() > 256
            || self.allowed_tools.iter().any(|name| !remote_name(name))
            || self.allowed_resources.len() > 256
            || self
                .allowed_resources
                .iter()
                .any(|uri| !super::content::valid_uri(uri))
            || self.allowed_prompts.len() > 256
            || self.allowed_prompts.iter().any(|name| !remote_name(name))
            || self
                .trusted_effects
                .keys()
                .any(|name| !self.allowed_tools.contains(name))
            || self.trusted_effects.values().any(BTreeSet::is_empty)
            || self.limits.frame_bytes == 0
            || self.limits.frame_bytes > 1024 * 1024
            || self.limits.total_discovery_bytes < self.limits.frame_bytes
            || self.limits.total_discovery_bytes > 16 * 1024 * 1024
            || self.limits.tools == 0
            || self.limits.tools > 256
            || self.limits.pages == 0
            || self.limits.pages > 64
            || self.limits.timeout_ms == 0
            || self.limits.timeout_ms > 300_000
            || self.limits.stderr_bytes > 1024 * 1024
        {
            return Err(Error::Invalid);
        }
        if self.capabilities.sampling
            || self.capabilities.roots
            || self.capabilities.elicitation
            || self.capabilities.tasks
        {
            return Err(Error::UnsupportedCapability);
        }
        Ok(())
    }
    pub fn resources_enabled(&self) -> bool {
        self.capabilities.resources || !self.allowed_resources.is_empty()
    }
    pub fn prompts_enabled(&self) -> bool {
        self.capabilities.prompts || !self.allowed_prompts.is_empty()
    }
    pub fn digest(&self) -> Result<String, Error> {
        self.validate()?;
        vcp_protocol::canonical_bytes(self)
            .map(|bytes| vcp_protocol::digest_bytes(&bytes))
            .map_err(|_| Error::Invalid)
    }
    pub fn effects(&self, remote: &str) -> Result<BTreeSet<EffectClass>, Error> {
        self.validate()?;
        if !self.allowed_tools.contains(remote) {
            return Err(Error::Invalid);
        }
        Ok(self
            .trusted_effects
            .get(remote)
            .cloned()
            .unwrap_or_else(|| BTreeSet::from([EffectClass::Opaque])))
    }
}
pub(crate) fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}
pub(crate) fn remote_name(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
pub(crate) fn sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
