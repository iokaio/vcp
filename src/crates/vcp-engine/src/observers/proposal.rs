// SPDX-License-Identifier: Apache-2.0
use super::{subscription::Input, Error, Result};
use serde::{Deserialize, Serialize};
use vcp_domain::VerificationId;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Current,
    Historical,
}
/// An observed repetition fact only. It does not diagnose a stall or authorize
/// stopping, changing strategy, relaxing required checks, or changing authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub attempt: u32,
    pub key: String,
    pub input: Input,
    pub source_evidence: String,
    pub verifications: Vec<VerificationId>,
    pub disposition: Disposition,
}
impl Proposal {
    pub fn validate(&self) -> Result<()> {
        self.input.validate()?;
        if self.attempt == 0
            || self.key != super::dedup::key(&self.input)?
            || self.source_evidence.is_empty()
            || self.source_evidence.len() > 256
            || self.source_evidence.chars().any(char::is_control)
            || self.verifications.len() < 3
            || self.verifications.len() > 4096
        {
            return Err(Error::Invalid("proposal"));
        }
        let ids: std::collections::BTreeSet<_> = self.verifications.iter().collect();
        if ids.len() != self.verifications.len() {
            return Err(Error::Invalid("duplicate verification"));
        }
        Ok(())
    }
}
