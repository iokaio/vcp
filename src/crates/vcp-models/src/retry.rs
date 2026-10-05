// SPDX-License-Identifier: Apache-2.0
use crate::*;
use serde::{Deserialize, Serialize};
use vcp_domain::{AttemptId, Limit, Timestamp};

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

/// Bounded, allowlisted metadata from an HTTP error body. Provider prose and
/// account identifiers stay in the raw response artifact, never in status text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitSource {
    UpstreamProviderSharedPool,
    OpenrouterInFlightBudget,
    OpenrouterKeyLimit,
    OpenrouterCredits,
}

pub fn error_limit_source(body: &[u8]) -> Option<LimitSource> {
    if body.len() > 64 * 1024 {
        return None;
    }
    let value = crate::decision::unique_json::parse(body).ok()?;
    match value.pointer("/error/metadata/limit_source")?.as_str()? {
        "upstream_provider_shared_pool" => Some(LimitSource::UpstreamProviderSharedPool),
        "openrouter_in_flight_budget" => Some(LimitSource::OpenrouterInFlightBudget),
        "openrouter_key_limit" => Some(LimitSource::OpenrouterKeyLimit),
        "openrouter_credits" => Some(LimitSource::OpenrouterCredits),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderFailure {
    pub failure: Failure,
    pub http_status: Option<u16>,
    pub limit_source: Option<LimitSource>,
    pub retry_after_ms: Option<u64>,
}

impl ProviderFailure {
    pub fn summary(&self) -> String {
        let cause = match (self.failure, self.limit_source) {
            (_, Some(LimitSource::OpenrouterInFlightBudget)) =>
                "provider in-flight spending limit; wait for outstanding requests to settle",
            (_, Some(LimitSource::OpenrouterKeyLimit)) =>
                "provider API key credit limit exhausted; review the key's spending limit",
            (_, Some(LimitSource::OpenrouterCredits)) =>
                "provider credits cannot cover this request; review credits and request limits",
            (Failure::RateLimit, Some(LimitSource::UpstreamProviderSharedPool)) =>
                "upstream provider shared pool is rate limited; retry after cooldown or select another permitted endpoint",
            (Failure::RateLimit, _) =>
                "provider rate limit; retry after cooldown or select another permitted endpoint",
            (Failure::Authentication, _) => "provider authentication failed; review credentials",
            (Failure::Capability, _) => "provider rejected the request; inspect the retained response",
            (Failure::Timeout, _) => "provider response timed out",
            (Failure::Transient, _) => "provider temporarily unavailable",
            _ => "provider response failed; inspect the retained response",
        };
        match self.http_status {
            Some(status) => format!("HTTP {status}: {cause}"),
            None => cause.into(),
        }
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
    pub deadline: Limit<Timestamp>,
}
impl Policy {
    /// Rate limits need a cooldown, not the short reconnect delay used for
    /// transient failures. Explicit finite deadlines and retry counts remain
    /// effective; Unbounded adds no elapsed ceiling or shortened server hint.
    pub fn for_failure(
        max_retries: u32,
        deadline: impl Into<Limit<Timestamp>>,
        failure: Failure,
    ) -> Self {
        Self {
            max_retries,
            base_delay_ms: if failure == Failure::RateLimit {
                5_000
            } else {
                100
            },
            max_delay_ms: if failure == Failure::RateLimit {
                60_000
            } else {
                5_000
            },
            deadline: deadline.into(),
        }
    }
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
            || self
                .deadline
                .finite()
                .is_some_and(|deadline| now >= *deadline)
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
        if self
            .deadline
            .finite()
            .is_some_and(|deadline| not_before >= deadline.get())
        {
            return Ok(None);
        }
        Ok(Some(Retry {
            predecessor: attempt,
            not_before: Timestamp::new(not_before),
            prior_liability_unresolved: submitted,
        }))
    }
}
