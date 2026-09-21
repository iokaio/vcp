// SPDX-License-Identifier: Apache-2.0
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetrievalLimits {
    pub results: u32,
    pub tokens: Units,
    pub bytes: vcp_domain::ByteCount,
}
impl Default for RetrievalLimits {
    fn default() -> Self {
        Self {
            results: 64,
            tokens: Units::new(16384),
            bytes: vcp_domain::ByteCount::new(65536),
        }
    }
}
impl RetrievalLimits {
    pub fn validate(&self) -> Result<()> {
        if !(1..=64).contains(&self.results)
            || !(2..=16384).contains(&self.tokens.get())
            || !(2..=65536).contains(&self.bytes.get())
        {
            return Err(Error::Limit("selected retrieval limits"));
        }
        Ok(())
    }
    pub fn clamp(&self, ceiling: &Self) -> Self {
        Self {
            results: self.results.min(ceiling.results),
            tokens: self.tokens.min(ceiling.tokens),
            bytes: self.bytes.min(ceiling.bytes),
        }
    }
}

/// Optional selections can only narrow a separately configured escalation policy.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EscalationLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_transport_retries: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_quality_switches: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_total_attempts: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_repeated_failures: Option<u32>,
}
impl EscalationLimits {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
    pub fn validate(&self) -> Result<()> {
        if self.is_empty()
            || self.max_transport_retries.is_some_and(|n| n > 4)
            || self.max_quality_switches.is_some_and(|n| n > 8)
            || self.max_total_attempts.is_some_and(|n| n == 0 || n > 64)
            || self
                .minimum_repeated_failures
                .is_some_and(|n| n == 0 || n > 64)
        {
            return Err(Error::Limit("selected escalation limits"));
        }
        Ok(())
    }
    pub fn from_policy(policy: &crate::escalation::Policy, retries: u32) -> Self {
        Self {
            max_transport_retries: Some(policy.max_transport_retries.min(retries)),
            max_quality_switches: Some(policy.max_quality_switches),
            max_total_attempts: Some(policy.max_total_attempts),
            minimum_repeated_failures: Some(policy.minimum_repeated_failures),
        }
    }
    pub fn clamp(&self, ceiling: &Self) -> Self {
        fn minimum(a: Option<u32>, b: Option<u32>) -> Option<u32> {
            match (a, b) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            }
        }
        Self {
            max_transport_retries: minimum(
                self.max_transport_retries,
                ceiling.max_transport_retries,
            ),
            max_quality_switches: minimum(self.max_quality_switches, ceiling.max_quality_switches),
            max_total_attempts: minimum(self.max_total_attempts, ceiling.max_total_attempts),
            minimum_repeated_failures: match (
                self.minimum_repeated_failures,
                ceiling.minimum_repeated_failures,
            ) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            },
        }
    }
    pub fn apply(&self, policy: &mut crate::escalation::Policy) {
        if let Some(n) = self.max_transport_retries {
            policy.max_transport_retries = policy.max_transport_retries.min(n);
        }
        if let Some(n) = self.max_quality_switches {
            policy.max_quality_switches = policy.max_quality_switches.min(n);
        }
        if let Some(n) = self.max_total_attempts {
            policy.max_total_attempts = policy.max_total_attempts.min(n);
        }
        if let Some(n) = self.minimum_repeated_failures {
            policy.minimum_repeated_failures = policy.minimum_repeated_failures.max(n);
        }
    }
}
