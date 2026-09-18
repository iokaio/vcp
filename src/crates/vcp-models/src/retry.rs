// SPDX-License-Identifier: Apache-2.0
use crate::*;
use serde::{Deserialize, Serialize};
use vcp_domain::{AttemptId, Timestamp};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Failure {
    BeforeSubmission,
    RateLimit,
    Transient,
    Timeout,
    Cancelled,
    Protocol,
    Capability,
    Authentication,
}
pub fn http_failure(status: u16) -> Failure {
    match status {
        401 => Failure::Authentication,
        400 | 402 | 403 | 404 | 422 => Failure::Capability,
        429 => Failure::RateLimit,
        408 | 504 => Failure::Timeout,
        500..=599 => Failure::Transient,
        _ => Failure::Protocol,
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Retry {
    pub predecessor: AttemptId,
    pub not_before: Timestamp,
    /// Ambiguous submitted attempts retain their independent liability.
    pub prior_liability_unresolved: bool,
}
#[derive(Clone, Debug)]
pub struct Policy {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub deadline: Timestamp,
}
impl Policy {
    /// Scheduling this result requires a fresh attempt/reservation and current
    /// owner generation. This module performs neither waiting nor transport.
    pub fn next(
        &self,
        attempt: AttemptId,
        completed_retries: u32,
        now: Timestamp,
        failure: Failure,
        submitted: bool,
        retry_after_ms: Option<u64>,
        owner_current: bool,
    ) -> Result<Option<Retry>> {
        if self.max_retries > 4
            || self.base_delay_ms == 0
            || self.max_delay_ms > 60_000
            || self.base_delay_ms > self.max_delay_ms
        {
            return Err(Error::Limit("retry policy"));
        }
        if !owner_current
            || completed_retries >= self.max_retries
            || now >= self.deadline
            || matches!(
                failure,
                Failure::Cancelled
                    | Failure::Protocol
                    | Failure::Capability
                    | Failure::Authentication
            )
        {
            return Ok(None);
        }
        if failure == Failure::BeforeSubmission && submitted {
            return Err(Error::Protocol("contradictory submission certainty"));
        }
        let delay = self
            .base_delay_ms
            .checked_mul(1u64 << completed_retries)
            .ok_or(Error::Limit("retry delay"))?
            .min(self.max_delay_ms)
            .max(retry_after_ms.unwrap_or(0));
        if delay > self.max_delay_ms {
            return Ok(None);
        }
        let not_before = now
            .get()
            .checked_add(delay)
            .ok_or(Error::Limit("retry deadline"))?;
        if not_before >= self.deadline.get() {
            return Ok(None);
        }
        Ok(Some(Retry {
            predecessor: attempt,
            not_before: Timestamp::new(not_before),
            prior_liability_unresolved: submitted,
        }))
    }
}
