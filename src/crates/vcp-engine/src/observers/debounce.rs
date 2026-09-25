// SPDX-License-Identifier: Apache-2.0
use vcp_domain::Timestamp;
/// Fixed earliest-next-admission debounce. Event storms replace one pending
/// snapshot; they never push this boundary forward and starve an admitted slot.
pub fn ready(now: Timestamp, last: Option<Timestamp>, delay_ms: u64) -> bool {
    last.is_none_or(|last| now.get() >= last.get().saturating_add(delay_ms))
}
