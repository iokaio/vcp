// SPDX-License-Identifier: Apache-2.0
//! Explicit OpenRouter Responses effort; no inferred provider default or extra
//! token budget. Only the four levels documented for Responses are admitted.
use crate::{catalog::Snapshot, Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Minimal,
    Low,
    Medium,
    High,
}

pub fn validate(snapshot: &Snapshot, effort: Option<Effort>) -> Result<()> {
    if let Some(effort) = effort {
        if !snapshot
            .compatibility
            .required_parameters
            .contains("reasoning")
            || !snapshot
                .compatibility
                .qualified_reasoning_efforts
                .contains(&effort)
        {
            return Err(Error::Capability(
                "requested Responses reasoning effort is not qualified for this exact endpoint",
            ));
        }
    }
    Ok(())
}
