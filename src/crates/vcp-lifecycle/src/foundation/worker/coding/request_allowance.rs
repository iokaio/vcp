// SPDX-License-Identifier: Apache-2.0
//! A captured observation of the existing shared-root gate, never a reservation.
use super::*;

pub(super) const GUIDANCE: &str = "The canonical_root_request_allowance observation is a snapshot before admission. Its remaining count includes the request receiving this context; children, helpers and retries share the root allowance, and concurrent work can consume it. It grants no permission and cannot increase any limit. Batch independent vcp_read/vcp_list/vcp_search calls in one response when their inputs are already known; use bounded vcp_search for cross-file discovery instead of serial directory exploration. Preserve dependent ordering; vcp_verify and vcp_mcp still require isolated responses. Plan to leave a request for the final answer after required checks. If evidence or allowance is insufficient, report the limitation rather than inventing results or skipping required checks.";

#[derive(Debug, serde::Serialize)]
pub(super) struct Allowance {
    kind: &'static str,
    schema_version: u32,
    root: TaskId,
    max_requests: u32,
    requests_used: usize,
    requests_remaining_including_this_request: usize,
}
impl Allowance {
    pub(super) fn exhausted(&self) -> bool {
        self.requests_used >= self.max_requests as usize
    }
}

fn observe<'a>(
    root: &TaskId,
    limits: impl Iterator<Item = u32>,
    attempt_roots: impl Iterator<Item = &'a TaskId>,
) -> Option<Allowance> {
    let max_requests = limits.min()?;
    let requests_used = attempt_roots.filter(|candidate| *candidate == root).count();
    Some(Allowance {
        kind: "canonical_root_request_allowance",
        schema_version: 1,
        root: root.clone(),
        max_requests,
        requests_used,
        requests_remaining_including_this_request: (max_requests as usize)
            .saturating_sub(requests_used),
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
            self.coding.values().map(|state| state.config.max_requests),
            attempts.iter().map(|attempt| &attempt.root),
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
        let initial = observe(&root, [6, 128, 4].into_iter(), [&other].into_iter()).unwrap();
        assert_eq!(
            (
                initial.max_requests,
                initial.requests_used,
                initial.requests_remaining_including_this_request
            ),
            (4, 0, 4)
        );
        assert!(!initial.exhausted());
        // Main, child, helper/retry attempts all carry the same root identity.
        // No completed/failed/cancelled attempt is refunded by this projection.
        for used in 1..=5 {
            let roots: Vec<_> = std::iter::once(&other)
                .chain(std::iter::repeat_n(&root, used))
                .collect();
            let current = observe(&root, [128, 4, 6].into_iter(), roots.into_iter()).unwrap();
            assert_eq!(current.requests_used, used);
            assert_eq!(
                current.requests_remaining_including_this_request,
                4usize.saturating_sub(used)
            );
            assert_eq!(current.exhausted(), used >= 4);
        }
        assert!(observe(&root, [].into_iter(), [&root].into_iter()).is_none());
    }
}
