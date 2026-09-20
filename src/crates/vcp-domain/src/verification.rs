// SPDX-License-Identifier: Apache-2.0
use crate::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fingerprint {
    pub repository: String,
    pub buffers: String,
    pub environment: String,
}
impl Fingerprint {
    pub fn validate(&self) -> Result<()> {
        for value in [&self.repository, &self.buffers, &self.environment] {
            if value.len() != 64
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(Error::Invalid("SHA-256 fingerprint"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum CheckOutcome {
    Passed,
    Failed { reason: String },
    NotRun { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub specification: String,
    pub outcome: CheckOutcome,
    pub output: ArtifactId,
    pub exit_code: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "certainty", rename_all = "snake_case", deny_unknown_fields)]
pub enum CostCertainty {
    Known,
    Uncertain {
        attempts: Vec<AttemptId>,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verification {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redaction: Option<crate::redaction::ContentRedaction>,
    pub id: VerificationId,
    pub scope: workspace::Scope,
    pub steering: SteeringRevision,
    pub fingerprint: Fingerprint,
    pub outputs: Vec<ArtifactId>,
    pub checks: Vec<Check>,
    pub unresolved_effects: Vec<ToolRunId>,
    pub outstanding_issues: Vec<String>,
    pub cost: CostCertainty,
}
impl Verification {
    pub fn applies(
        &self,
        scope: &workspace::Scope,
        steering: SteeringRevision,
        fingerprint: &Fingerprint,
    ) -> bool {
        self.redaction.is_none()
            && self.scope == *scope
            && self.steering == steering
            && self.fingerprint == *fingerprint
            && self.fingerprint.validate().is_ok()
    }
    pub fn satisfies(&self, required_checks: &[String], editing: bool) -> bool {
        if self.redaction.is_some()
            || self.outputs.is_empty()
            || !self.unresolved_effects.is_empty()
            || !self.outstanding_issues.is_empty()
        {
            return false;
        }
        if editing && (required_checks.is_empty() || self.checks.is_empty()) {
            return false;
        }
        let mut seen = std::collections::BTreeSet::new();
        if self.checks.iter().any(|c| {
            !seen.insert(&c.specification)
                || c.outcome != CheckOutcome::Passed
                || c.exit_code.is_some_and(|n| n != 0)
        }) {
            return false;
        }
        required_checks
            .iter()
            .all(|required| self.checks.iter().any(|c| &c.specification == required))
    }
}
