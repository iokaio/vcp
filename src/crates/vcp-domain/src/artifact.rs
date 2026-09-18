// SPDX-License-Identifier: Apache-2.0
use crate::{workspace::Scope, *};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureState {
    Pending,
    Complete,
    Aborted,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Range {
    pub start: ByteCount,
    pub end: ByteCount,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Omission {
    AuthenticationHeaders,
    RecoveryMaterial,
    UnobservedTail,
    CaptureFailure,
    ExplicitAbort,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactSpec {
    pub id: ArtifactId,
    pub scope: Scope,
    pub media_type: String,
    pub schema: String,
    pub source: String,
    pub channel: Channel,
    pub retention: String,
    pub omissions: Vec<Omission>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    RequestBody,
    Response,
    Stdout,
    Stderr,
    ChildTranscript,
    Evidence,
}
impl ArtifactSpec {
    pub fn validate(&self) -> Result<()> {
        for value in [
            &self.media_type,
            &self.schema,
            &self.source,
            &self.retention,
        ] {
            if value.trim().is_empty() || value.len() > 256 || value.contains('\0') {
                return Err(Error::Invalid("artifact metadata"));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactDescriptor {
    pub spec: ArtifactSpec,
    pub state: CaptureState,
    pub length: ByteCount,
    pub sha256: String,
    pub retained: Vec<Range>,
}
impl ArtifactDescriptor {
    pub fn validate(&self) -> Result<()> {
        self.spec.validate()?;
        if self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err(Error::Invalid("artifact hash"));
        }
        if self.retained
            != vec![Range {
                start: ByteCount::ZERO,
                end: self.length,
            }]
        {
            return Err(Error::Invalid("contiguous retained extent"));
        }
        Ok(())
    }
}
