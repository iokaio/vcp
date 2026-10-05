// SPDX-License-Identifier: Apache-2.0
//! Private versioned manifest identity for finalized ciphertext and publication.
//! Untagged serialization preserves every legacy manifest byte; the explicit
//! signed format field still separates v1 and v2. Neither variant grants trust.
use super::{stream::StreamManifest, Manifest, FORMAT};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use vcp_domain::WorkspaceId;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ManifestEnvelope {
    Stream(StreamManifest),
    Legacy(Manifest),
}
impl From<Manifest> for ManifestEnvelope {
    fn from(value: Manifest) -> Self {
        Self::Legacy(value)
    }
}
impl From<StreamManifest> for ManifestEnvelope {
    fn from(value: StreamManifest) -> Self {
        Self::Stream(value)
    }
}
impl ManifestEnvelope {
    pub(crate) fn ciphertext_limit(&self) -> Result<u64> {
        match self {
            Self::Legacy(_) => Ok(65 * 1024 * 1024),
            Self::Stream(v) => {
                Ok(super::stream::StreamLimits::for_payload(v.payload.bytes)?.ciphertext_bytes)
            }
        }
    }
    pub fn workspace(&self) -> &WorkspaceId {
        match self {
            Self::Legacy(v) => &v.workspace,
            Self::Stream(v) => &v.workspace,
        }
    }
    pub fn lineage(&self) -> &str {
        match self {
            Self::Legacy(v) => &v.lineage,
            Self::Stream(v) => &v.lineage,
        }
    }
    pub fn sequence(&self) -> u64 {
        match self {
            Self::Legacy(v) => v.sequence,
            Self::Stream(v) => v.sequence,
        }
    }
    pub fn deletion(&self) -> u64 {
        match self {
            Self::Legacy(v) => v.deletion,
            Self::Stream(v) => v.deletion,
        }
    }
    pub fn parent(&self) -> &Option<String> {
        match self {
            Self::Legacy(v) => &v.parent,
            Self::Stream(v) => &v.parent,
        }
    }
    pub fn format(&self) -> &'static str {
        match self {
            Self::Legacy(_) => FORMAT,
            Self::Stream(_) => super::stream::FORMAT,
        }
    }
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Legacy(v) => {
                if v.format != FORMAT || v.objects.len() > 4096 {
                    return Err(Error::Incompatible);
                }
                Ok(())
            }
            Self::Stream(v) => super::stream::validate(
                v,
                super::stream::StreamLimits::for_payload(v.payload.bytes)?,
            ),
        }
    }
}
