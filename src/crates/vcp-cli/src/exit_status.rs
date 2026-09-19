// SPDX-License-Identifier: Apache-2.0
use serde::{Deserialize, Serialize};

/// Preserve every observed condition; a process code summarizes their precedence.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conditions {
    pub unresolved_effect: bool,
    pub cancelled: bool,
    pub budget_exhausted: bool,
    pub required_input: bool,
    pub incomplete: bool,
    pub invalid_configuration: bool,
    pub internal_failure: bool,
    pub durably_paused: bool,
    pub completed: bool,
}
impl Conditions {
    pub fn code(&self) -> u8 {
        for (present, code) in [
            (self.unresolved_effect, 7),
            (self.cancelled, 6),
            (self.budget_exhausted, 5),
            (self.required_input, 4),
            (self.incomplete, 3),
            (self.invalid_configuration, 2),
            (self.internal_failure, 1),
            (self.durably_paused, 8),
            (self.completed, 0),
        ] {
            if present {
                return code;
            }
        }
        // Absence of an outcome is never evidence of completion.
        1
    }
}
