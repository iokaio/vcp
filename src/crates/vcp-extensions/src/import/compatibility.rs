// SPDX-License-Identifier: Apache-2.0
use super::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Format {
    #[serde(rename = "codex-8b78600d-v1")]
    Codex8b78600d,
    #[serde(rename = "gemini-6a466a7e-v1")]
    Gemini6a466a7e,
}
impl Format {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "codex-8b78600d-v1" => Ok(Self::Codex8b78600d),
            "gemini-6a466a7e-v1" => Ok(Self::Gemini6a466a7e),
            _ => Err(Error::Version),
        }
    }
    pub fn revision(self) -> &'static str {
        match self {
            Self::Codex8b78600d => "8b78600dc85cc265d7e7e827f6aa903875405287",
            Self::Gemini6a466a7e => "6a466a7e2fe2b1255752c1e74f69b31f0216084d",
        }
    }
}
