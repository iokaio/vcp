// SPDX-License-Identifier: Apache-2.0
//! Recorded request allocation; observations never grant execution authority.
use crate::Units;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    #[default]
    Unclassified,
    Discovery,
    Editing,
    Repair,
    Verification,
    Planning,
    Summary,
    Resume,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Allocation {
    pub version: u32,
    pub activity: Activity,
    /// Input target uses the manifest's qualified estimate method, not an
    /// assumed byte-to-token conversion. It may yield to required evidence.
    pub input_target: Units,
    pub input_target_exceeded_by_required_context: bool,
    pub output_limit: Units,
    pub host_output_ceiling: Units,
    pub previous_output: Option<Units>,
    pub previous_output_limit: Option<Units>,
    pub reason: Reason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    ActivityDefault,
    PreserveWithoutUsage,
    PreserveRecentUsage,
    GrowAfterLengthLimit,
    ShrinkAfterRepeatedUnderuse,
    ProviderCapacity,
}

impl Allocation {
    pub fn validate(&self, input_capacity: u64, output: Units) -> crate::Result<()> {
        if self.version != 1
            || self.output_limit == Units::ZERO
            || self.output_limit != output
            || self.output_limit > self.host_output_ceiling
            || self.input_target.get() > input_capacity
            || self.previous_output_limit == Some(Units::ZERO)
        {
            return Err(crate::Error::Invalid("request allocation"));
        }
        Ok(())
    }
}
