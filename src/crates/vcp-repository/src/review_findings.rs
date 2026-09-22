// SPDX-License-Identifier: Apache-2.0
//! Bounded, untrusted review evidence. Validation proves shape and attribution,
//! never that a reported defect is real or that parent verification passed.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ReviewFinding {
    /// Historical packets remain readable, without invented provenance.
    Legacy(String),
    Structured(Box<StructuredFinding>),
}
impl From<String> for ReviewFinding {
    fn from(value: String) -> Self {
        Self::Legacy(value)
    }
}
impl From<&str> for ReviewFinding {
    fn from(value: &str) -> Self {
        Self::Legacy(value.into())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingLocation {
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingKind {
    DemonstratedDefect,
    Suggestion,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Source,
    Reproduction,
    BaseComparison,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingEvidence {
    pub kind: EvidenceKind,
    /// Artifact/source reference only; resolving it still requires current access.
    pub reference: String,
    pub explanation: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChangeCausality {
    #[default]
    Unknown,
    /// Indexes into supporting evidence. A changed location alone is insufficient.
    Introduced {
        evidence: BTreeSet<usize>,
    },
    PreExisting {
        evidence: BTreeSet<usize>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructuredFinding {
    pub schema_version: u32,
    pub base_fingerprint: String,
    pub current_fingerprint: String,
    pub examined_paths: BTreeSet<String>,
    pub location: FindingLocation,
    pub kind: FindingKind,
    pub trigger: String,
    pub consequence: String,
    pub evidence: Vec<FindingEvidence>,
    pub uncertainty: String,
    #[serde(default)]
    pub introduced_by_change: ChangeCausality,
}
fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.contains('\0')
}
fn path(value: &str) -> bool {
    value.len() <= 4096
        && !value.contains('\\')
        && crate::path::relative(Path::new(value)).is_ok_and(|normalized| normalized == value)
}
impl ReviewFinding {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Legacy(value) if value.len() <= 8192 => Ok(()),
            Self::Legacy(_) => Err(Error::Limit("legacy review finding")),
            Self::Structured(value) => value.validate(),
        }
    }
}
impl StructuredFinding {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || !text(&self.base_fingerprint, 256)
            || !text(&self.current_fingerprint, 256)
            || self.examined_paths.is_empty()
            || self.examined_paths.len() > 128
            || self.examined_paths.iter().any(|p| !path(p))
            || !self.examined_paths.contains(&self.location.path)
            || self.location.start_line == 0
            || self.location.end_line < self.location.start_line
            || self.location.end_line - self.location.start_line > 200
            || !text(&self.trigger, 2048)
            || !text(&self.consequence, 2048)
            || !text(&self.uncertainty, 2048)
            || self.evidence.is_empty()
            || self.evidence.len() > 16
            || self
                .evidence
                .iter()
                .any(|e| !text(&e.reference, 1024) || !text(&e.explanation, 2048))
        {
            return Err(Error::Scope("invalid structured review finding".into()));
        }
        if let ChangeCausality::Introduced { evidence }
        | ChangeCausality::PreExisting { evidence } = &self.introduced_by_change
        {
            if evidence.is_empty()
                || evidence.iter().any(|index| {
                    self.evidence
                        .get(*index)
                        .is_none_or(|item| item.kind == EvidenceKind::Source)
                })
            {
                return Err(Error::Scope(
                    "change causality requires comparison or reproduction evidence".into(),
                ));
            }
        }
        // Count the entire serialized object, including potentially numerous paths.
        if serde_json::to_vec(self)?.len() > 32_768 {
            return Err(Error::Limit("structured review finding"));
        }
        Ok(())
    }
    /// Historical evidence remains inspectable; consumers label it stale rather
    /// than treating unchanged diff text as a fresh review.
    pub fn matches_revision(&self, base: &str, current: &str) -> bool {
        self.base_fingerprint == base && self.current_fingerprint == current
    }
}
