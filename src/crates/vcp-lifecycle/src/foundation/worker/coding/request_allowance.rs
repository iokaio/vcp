// SPDX-License-Identifier: Apache-2.0
//! A captured observation of the existing shared-root gate, never a reservation.
use super::*;

pub(super) const GUIDANCE: &str = "The canonical_root_request_allowance observation is a snapshot before admission. Its remaining count includes the request receiving this context; children, helpers and retries share the root allowance, and concurrent work can consume it. observed_at and deadline are host Unix timestamps in milliseconds; remaining_ms is the time left at observation, not a live clock or a guarantee. Context assembly, model generation, tools, verification and the final answer consume that same time. The snapshot ages and grants no permission, deadline extension, retry or extra request. Batch independent vcp_read/vcp_list/vcp_search calls in one response when their inputs are already known; use bounded vcp_search for cross-file discovery instead of serial directory exploration. Preserve dependent ordering; vcp_verify and vcp_mcp still require isolated responses. Plan to leave time and a request for the final answer after required checks. If evidence or allowance is insufficient, report the limitation rather than inventing results or skipping required checks.";

pub(super) fn guidance(tools: &crate::foundation::coding::CanonicalTools) -> &'static str {
    if tools.is_all() {
        GUIDANCE
    } else {
        "The canonical_root_request_allowance observation is a snapshot before admission. Its remaining count includes the request receiving this context; children, helpers and retries share the root allowance. observed_at and deadline are host Unix timestamps in milliseconds; remaining_ms is the time left at observation, not a live clock or a guarantee. Context assembly, model generation, tools, verification and the final answer consume that same time. The snapshot ages and grants no permission, deadline extension, retry or extra request. Only the owner's model tool ceiling is advertised. Batch independent available reads when their inputs are known; preserve dependent ordering and any isolated-response requirements. Leave time and a request for the final answer after applicable checks. Report unavailable checks and insufficient evidence honestly."
    }
}

#[derive(Debug, serde::Serialize)]
pub(super) struct Allowance {
    kind: &'static str,
    schema_version: u32,
    root: TaskId,
    max_requests: u32,
    requests_used: usize,
    requests_remaining_including_this_request: usize,
    observed_at: Timestamp,
    deadline: Timestamp,
    remaining_ms: u64,
}
impl Allowance {
    pub(super) fn exhausted(&self) -> bool {
        self.requests_used >= self.max_requests as usize
    }
}

pub(super) fn shared_limits(
    limits: impl Iterator<Item = (u32, Timestamp)>,
) -> Option<(u32, Timestamp)> {
    limits.reduce(|(requests, deadline), (next_requests, next_deadline)| {
        (requests.min(next_requests), deadline.min(next_deadline))
    })
}

fn observe<'a>(
    root: &TaskId,
    limits: impl Iterator<Item = (u32, Timestamp)>,
    attempt_roots: impl Iterator<Item = &'a TaskId>,
    observed_at: Timestamp,
) -> Option<Allowance> {
    let (max_requests, deadline) = shared_limits(limits)?;
    let requests_used = attempt_roots.filter(|candidate| *candidate == root).count();
    Some(Allowance {
        kind: "canonical_root_request_allowance",
        schema_version: 1,
        root: root.clone(),
        max_requests,
        requests_used,
        requests_remaining_including_this_request: (max_requests as usize)
            .saturating_sub(requests_used),
        observed_at,
        deadline,
        remaining_ms: deadline.get().saturating_sub(observed_at.get()),
    })
}

impl Context {
    pub(super) fn coding_request_allowance(&self) -> Result<Option<Allowance>> {
        if self.coding.is_empty() {
            return Ok(None);
        }
        // Deliberately identical to admission: every canonical attempt counts,
        // regardless of role, phase, retry predecessor or unresolved liability.
        let attempts = self
            .engine
            .store()
            .state()
            .records
            .values()
            .filter(|record| record.collection == Collection::Attempt)
            .map(Record::decode::<Attempt>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(observe(
            &self.config.root_task,
            self.coding
                .values()
                .map(|state| (state.config.max_requests, state.config.deadline)),
            attempts.iter().map(|attempt| &attempt.root),
            now(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowance_uses_minimum_shared_limit_and_counts_root_attempts_without_refunds() {
        let root = TaskId::parse("shared-root").unwrap();
        let other = TaskId::parse("unrelated-root").unwrap();
        let limits = [
            (6, Timestamp::new(90)),
            (128, Timestamp::new(70)),
            (4, Timestamp::new(80)),
        ];
        let initial = observe(
            &root,
            limits.into_iter(),
            [&other].into_iter(),
            Timestamp::new(10),
        )
        .unwrap();
        assert_eq!(
            (
                initial.max_requests,
                initial.requests_used,
                initial.requests_remaining_including_this_request
            ),
            (4, 0, 4)
        );
        assert!(!initial.exhausted());
        assert_eq!(initial.observed_at, Timestamp::new(10));
        assert_eq!(initial.deadline, Timestamp::new(70));
        assert_eq!(initial.remaining_ms, 60);
        assert_eq!(
            shared_limits(limits.into_iter()),
            Some((4, Timestamp::new(70)))
        );
        // Main, child, helper/retry attempts all carry the same root identity.
        // No completed/failed/cancelled attempt is refunded by this projection.
        for used in 1..=5 {
            let roots: Vec<_> = std::iter::once(&other)
                .chain(std::iter::repeat_n(&root, used))
                .collect();
            let current = observe(
                &root,
                limits.into_iter(),
                roots.into_iter(),
                Timestamp::new(20),
            )
            .unwrap();
            assert_eq!(current.requests_used, used);
            assert_eq!(
                current.requests_remaining_including_this_request,
                4usize.saturating_sub(used)
            );
            assert_eq!(current.exhausted(), used >= 4);
            assert_eq!(current.remaining_ms, 50);
        }
        assert!(observe(
            &root,
            [].into_iter(),
            [&root].into_iter(),
            Timestamp::new(10)
        )
        .is_none());
    }

    #[test]
    fn time_observation_saturates_without_extending_deadlines_or_refunding_requests() {
        let root = TaskId::parse("shared-root").unwrap();
        for observed_at in [70, 71, u64::MAX] {
            let value = observe(
                &root,
                [(1, Timestamp::new(70))].into_iter(),
                [&root].into_iter(),
                Timestamp::new(observed_at),
            )
            .unwrap();
            assert_eq!(value.deadline, Timestamp::new(70));
            assert_eq!(value.remaining_ms, 0);
            assert_eq!(value.requests_remaining_including_this_request, 0);
            assert!(value.exhausted());
        }
        let value = observe(
            &root,
            [(1, Timestamp::new(u64::MAX))].into_iter(),
            [].into_iter(),
            Timestamp::new(0),
        )
        .unwrap();
        assert_eq!(value.remaining_ms, u64::MAX);
        assert!(!value.exhausted());
    }
}
