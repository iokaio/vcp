// SPDX-License-Identifier: Apache-2.0
//! Provider-neutral integer accounting, separate from local resource metrics.
use crate::{ids::*, revision::*, workspace::Scope};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Currency(String);
impl Currency {
    pub fn code(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for Currency {
    type Error = crate::Error;
    fn try_from(value: String) -> crate::Result<Self> {
        if value.len() != 3 || !value.bytes().all(|c| c.is_ascii_uppercase()) {
            return Err(crate::Error::Invalid("three-letter currency unit"));
        }
        Ok(Self(value))
    }
}
impl From<Currency> for String {
    fn from(value: Currency) -> Self {
        value.0
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Money {
    pub currency: Currency,
    pub micros: Micros,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChargeCategory {
    Input,
    Output,
    CacheRead,
    CacheWrite,
    Request,
    ProviderTool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rate {
    pub micros: Micros,
    pub per_units: Units,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceSnapshot {
    pub id: String,
    pub provider: String,
    pub model: String,
    pub currency: Currency,
    pub capability: String,
    pub valid_until: Timestamp,
    pub rates: BTreeMap<ChargeCategory, Rate>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Usage {
    /// Inclusive totals: cache and reasoning are subsets, not extra charges.
    pub input: Units,
    pub output: Units,
    pub cache_read: Units,
    pub cache_write: Units,
    pub reasoning: Units,
    pub requests: Units,
    pub provider_tools: Units,
}
impl Usage {
    pub fn disjoint(&self) -> crate::Result<BTreeMap<ChargeCategory, Units>> {
        let cached = self
            .cache_read
            .get()
            .checked_add(self.cache_write.get())
            .ok_or(crate::Error::Overflow)?;
        let input = self
            .input
            .get()
            .checked_sub(cached)
            .ok_or(crate::Error::Invalid("overlapping input subtotals"))?;
        if self.reasoning > self.output {
            return Err(crate::Error::Invalid("reasoning exceeds inclusive output"));
        }
        Ok(BTreeMap::from([
            (ChargeCategory::Input, Units::new(input)),
            (ChargeCategory::Output, self.output),
            (ChargeCategory::CacheRead, self.cache_read),
            (ChargeCategory::CacheWrite, self.cache_write),
            (ChargeCategory::Request, self.requests),
            (ChargeCategory::ProviderTool, self.provider_tools),
        ]))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CostQuote {
    pub normalization_version: u32,
    pub price: PriceSnapshot,
    pub bounds: Usage,
    pub amount: Money,
    pub method: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestRole {
    Main,
    Helper,
    Compaction,
    Reviewer,
    Child,
    Optimizer,
    Memory,
    Verification,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReservationState {
    Created,
    Submitted,
    Settled,
    Released,
    ReconciliationPending,
    ExplicitlyResolved,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DailyPolicy {
    pub cap: Micros,
    pub utc_offset_minutes: i16,
    pub scope: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ledger {
    pub schema_version: u32,
    pub scope: Scope,
    pub revision: Revision,
    pub policy: PolicyRevision,
    pub currency: Currency,
    pub cap: Micros,
    pub protected: Micros,
    pub settled: Micros,
    pub active: Micros,
    pub unresolved: Micros,
    pub allocations: BTreeMap<TaskId, Micros>,
    pub daily: Option<DailyPolicy>,
    pub overrun: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reservation {
    pub schema_version: u32,
    pub id: ReservationId,
    pub scope: Scope,
    pub root: TaskId,
    pub attempt: AttemptId,
    pub revision: Revision,
    pub phase: ReservationState,
    pub amount: Money,
    pub charged: Micros,
    pub liability: Micros,
    pub protected_draw: Micros,
    pub protected_returned: Micros,
    pub day: i64,
    pub role: RequestRole,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    pub schema_version: u32,
    pub id: AttemptId,
    pub scope: Scope,
    pub root: TaskId,
    pub reservation: ReservationId,
    pub revision: Revision,
    pub phase: ReservationState,
    pub role: RequestRole,
    pub agent: AgentId,
    pub previous: Option<AttemptId>,
    pub request: ArtifactId,
    pub request_digest: String,
    pub admission_digest: String,
    pub steering: SteeringRevision,
    pub quote: CostQuote,
    pub admitted_policy: PolicyRevision,
    pub send_intent: Option<EventId>,
    pub observation_mode: Option<String>,
    pub usage_watermark: Units,
    pub charged: Micros,
    pub uncertain: Option<String>,
    pub provider_request: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum UsageMode {
    Cumulative { version: Units },
    Incremental { start: Units, end: Units },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resolution {
    pub actor: ActorId,
    pub policy: PolicyRevision,
    pub reason: String,
    pub remaining_uncertainty: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageObservation {
    pub id: ObservationId,
    pub scope: Scope,
    pub attempt: AttemptId,
    pub provider_request: String,
    pub mode: UsageMode,
    pub amount: Money,
    pub final_usage: bool,
    pub raw: ArtifactId,
    pub correction: Option<Resolution>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdjustmentDirection {
    Debit,
    Credit,
    None,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settlement {
    pub schema_version: u32,
    pub id: ObservationId,
    pub scope: Scope,
    pub attempt: AttemptId,
    pub observation: UsageObservation,
    pub applied: bool,
    pub direction: AdjustmentDirection,
    pub adjustment: Micros,
    pub total: Micros,
    pub normalization_version: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalResources {
    pub schema_version: u32,
    pub id: ObservationId,
    pub scope: Scope,
    pub agent: AgentId,
    pub cpu_millis: Units,
    pub peak_ram: ByteCount,
    pub disk: ByteCount,
    pub source: String,
}
pub fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
impl Ledger {
    pub fn validate(&self) -> crate::Result<()> {
        if self.schema_version != 1 {
            return Err(crate::Error::Invalid("ledger version"));
        }
        if let Some(daily) = &self.daily {
            if daily.scope != "local_root" || !(-1439..=1439).contains(&daily.utc_offset_minutes) {
                return Err(crate::Error::Invalid(
                    "daily scope or fixed timezone offset",
                ));
            }
        }
        Ok(())
    }
}
impl Reservation {
    pub fn validate(&self) -> crate::Result<()> {
        if self.schema_version != 1
            || self.protected_returned > self.protected_draw
            || self.protected_draw > self.amount.micros
        {
            return Err(crate::Error::Invalid("reservation"));
        }
        let expected = match self.phase {
            ReservationState::Settled
            | ReservationState::Released
            | ReservationState::ExplicitlyResolved => 0,
            _ => self.amount.micros.get().saturating_sub(self.charged.get()),
        };
        if self.liability.get() != expected {
            return Err(crate::Error::Invalid("reservation liability"));
        }
        Ok(())
    }
}
impl Attempt {
    pub fn validate(&self) -> crate::Result<()> {
        if self.schema_version != 1
            || self.quote.normalization_version != 1
            || !valid_hash(&self.request_digest)
            || !valid_hash(&self.admission_digest)
            || self.previous.as_ref() == Some(&self.id)
            || (matches!(
                self.phase,
                ReservationState::Submitted
                    | ReservationState::Settled
                    | ReservationState::ReconciliationPending
                    | ReservationState::ExplicitlyResolved
            ) && self.send_intent.is_none())
            || (self.phase == ReservationState::Created && self.send_intent.is_some())
        {
            return Err(crate::Error::Invalid("attempt"));
        }
        self.quote.bounds.disjoint()?;
        Ok(())
    }
}
