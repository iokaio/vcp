// SPDX-License-Identifier: Apache-2.0
//! Deterministic allocation from controller activity and observed usage.
use vcp_domain::{
    request_allocation::{Activity, Allocation, Reason},
    Units,
};

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    pub activity: Activity,
    pub previous_limit: Option<Units>,
    pub actual_output: Option<Units>,
    pub length_limited: bool,
    pub underuse_streak: u8,
}

impl History {
    pub fn observe(
        &mut self,
        activity: Activity,
        limit: Units,
        actual: Option<Units>,
        length_limited: bool,
    ) {
        let underused =
            !length_limited && actual.is_some_and(|value| value.get() <= limit.get() / 4);
        self.underuse_streak = if underused && self.activity == activity {
            self.underuse_streak.saturating_add(1)
        } else if underused {
            1
        } else {
            0
        };
        self.activity = activity;
        self.previous_limit = Some(limit);
        self.actual_output = actual;
        self.length_limited = length_limited;
    }
}

/// Select a response allowance before encoding. The result cannot raise an
/// owner/profile ceiling or provider capacity and never truncates a response.
pub fn choose(
    activity: Activity,
    host_ceiling: Units,
    provider_ceiling: Units,
    history: &History,
) -> crate::Result<Allocation> {
    let ceiling = host_ceiling.min(provider_ceiling);
    if ceiling == Units::ZERO {
        return Err(crate::Error::Capability("zero request output capacity"));
    }
    let baseline = match activity {
        Activity::Discovery | Activity::Verification => 2048,
        Activity::Editing
        | Activity::Repair
        | Activity::Planning
        | Activity::Summary
        | Activity::Resume
        | Activity::Unclassified => 4096,
    }
    .min(ceiling.get());
    let mut output = baseline;
    let mut reason = Reason::ActivityDefault;
    if history.activity == activity {
        if let Some(previous) = history.previous_limit {
            output = previous.get();
            reason = Reason::PreserveRecentUsage;
            if history.length_limited {
                output = output.saturating_mul(2);
                reason = Reason::GrowAfterLengthLimit;
            } else if history.actual_output.is_none() {
                reason = Reason::PreserveWithoutUsage;
            } else if history.underuse_streak >= 3 {
                // Keep headroom and require three completed observations.
                output = output / 2;
                output = output
                    .max(
                        history
                            .actual_output
                            .map_or(0, |actual| actual.get().saturating_mul(2)),
                    )
                    .max(baseline / 2)
                    .max(1);
                reason = Reason::ShrinkAfterRepeatedUnderuse;
            }
        }
    }
    if output > ceiling.get() {
        output = ceiling.get();
        reason = Reason::ProviderCapacity;
    }
    Ok(Allocation {
        version: 1,
        activity,
        input_target: Units::ZERO,
        input_target_exceeded_by_required_context: false,
        output_limit: Units::new(output),
        host_output_ceiling: host_ceiling,
        previous_output: history.actual_output,
        previous_output_limit: history.previous_limit,
        reason,
    })
}

pub fn input_target(activity: Activity, capacity: u64, output: Units) -> Units {
    let target: u64 = match activity {
        Activity::Discovery => 32 * 1024,
        Activity::Verification => 24 * 1024,
        Activity::Editing | Activity::Repair => 96 * 1024,
        Activity::Resume => 128 * 1024,
        Activity::Planning | Activity::Summary | Activity::Unclassified => 64 * 1024,
    };
    Units::new(
        target
            .saturating_sub(output.get().saturating_sub(4096))
            .min(capacity),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_limit_grows_but_never_exceeds_host_or_candidate() {
        let mut history = History::default();
        history.observe(Activity::Editing, Units::new(4096), None, true);
        let grown = choose(
            Activity::Editing,
            Units::new(16384),
            Units::new(12000),
            &history,
        )
        .unwrap();
        assert_eq!(grown.output_limit, Units::new(8192));
        assert_eq!(grown.reason, Reason::GrowAfterLengthLimit);
        let small = choose(
            Activity::Editing,
            Units::new(16384),
            Units::new(3000),
            &history,
        )
        .unwrap();
        assert_eq!(small.output_limit, Units::new(3000));
    }

    #[test]
    fn missing_usage_and_activity_changes_do_not_shrink_from_false_evidence() {
        let mut history = History::default();
        history.observe(Activity::Editing, Units::new(8192), None, false);
        assert_eq!(
            choose(
                Activity::Editing,
                Units::new(16384),
                Units::new(16384),
                &history
            )
            .unwrap()
            .output_limit,
            Units::new(8192)
        );
        assert_eq!(
            choose(
                Activity::Verification,
                Units::new(16384),
                Units::new(16384),
                &history
            )
            .unwrap()
            .output_limit,
            Units::new(2048)
        );
    }

    #[test]
    fn shrink_requires_repeated_completed_underuse() {
        let mut history = History::default();
        for _ in 0..2 {
            history.observe(
                Activity::Editing,
                Units::new(8192),
                Some(Units::new(400)),
                false,
            );
        }
        assert_eq!(
            choose(
                Activity::Editing,
                Units::new(16384),
                Units::new(16384),
                &history
            )
            .unwrap()
            .output_limit,
            Units::new(8192)
        );
        history.observe(
            Activity::Editing,
            Units::new(8192),
            Some(Units::new(400)),
            false,
        );
        assert_eq!(
            choose(
                Activity::Editing,
                Units::new(16384),
                Units::new(16384),
                &history
            )
            .unwrap()
            .output_limit,
            Units::new(4096)
        );
    }

    #[test]
    fn repair_preserves_more_input_and_truncation_trades_optional_input_for_output() {
        let history = History::default();
        let repair = choose(
            Activity::Repair,
            Units::new(16384),
            Units::new(16384),
            &history,
        )
        .unwrap();
        let discovery = choose(
            Activity::Discovery,
            Units::new(16384),
            Units::new(16384),
            &history,
        )
        .unwrap();
        assert!(repair.output_limit > discovery.output_limit);
        assert!(
            input_target(Activity::Repair, 200_000, repair.output_limit)
                > input_target(Activity::Discovery, 200_000, discovery.output_limit)
        );
        assert!(
            input_target(Activity::Repair, 200_000, Units::new(8192))
                < input_target(Activity::Repair, 200_000, Units::new(4096))
        );
        assert_eq!(
            input_target(Activity::Repair, 1000, repair.output_limit),
            Units::new(1000)
        );
        // Unknown/final prose retains the larger conservative answer allowance.
        assert_eq!(
            choose(
                Activity::Summary,
                Units::new(16384),
                Units::new(16384),
                &history
            )
            .unwrap()
            .output_limit,
            Units::new(4096)
        );
    }
}
