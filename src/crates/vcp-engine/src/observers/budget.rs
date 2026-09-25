// SPDX-License-Identifier: Apache-2.0
use super::{Error, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub max_attempts: u32,
    pub steps_per_attempt: u32,
    pub max_total_steps: u64,
    pub deadline_ms: u64,
    pub debounce_ms: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_attempts: 32,
            steps_per_attempt: 4096,
            max_total_steps: 65536,
            deadline_ms: 2000,
            debounce_ms: 100,
        }
    }
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        if self.max_attempts == 0
            || self.max_attempts > 64
            || self.steps_per_attempt == 0
            || self.steps_per_attempt > 4096
            || self.max_total_steps == 0
            || self.max_total_steps > 262144
            || self.deadline_ms == 0
            || self.deadline_ms > 2000
            || self.debounce_ms > 1000
        {
            return Err(Error::Invalid("limits"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub attempts: u32,
    pub reserved_steps: u64,
}
impl Budget {
    pub fn reserve(&mut self, limits: &Limits) -> Result<()> {
        limits.validate()?;
        let next = self
            .reserved_steps
            .checked_add(u64::from(limits.steps_per_attempt))
            .ok_or(Error::Capacity)?;
        if self.attempts >= limits.max_attempts || next > limits.max_total_steps {
            return Err(Error::Capacity);
        }
        self.attempts += 1;
        self.reserved_steps = next;
        Ok(())
    }
}
