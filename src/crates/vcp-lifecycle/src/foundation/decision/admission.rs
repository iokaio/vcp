// SPDX-License-Identifier: Apache-2.0
//! Finite operation qualification. No transport or grants.
use serde::{Deserialize, Serialize};
use vcp_domain::{
    accounting::{ChargeCategory, CostQuote, PriceSnapshot, Usage},
    ArtifactId, OwnerEpoch, Revision, Timestamp, Units, WorkspaceId,
};
use vcp_models::decision::{self, Mode, Operation, Prepared, QualifiedEvaluator, Request};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidencePin {
    pub artifact: ArtifactId,
    pub digest: String,
}

/// Owner-installed evidence document. Deserialization does NOT qualify it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationRecord {
    pub version: u32,
    pub workspace: WorkspaceId,
    pub revision: Revision,
    pub evaluator: QualifiedEvaluator,
    pub question_revision: String,
    pub catalog: EvidencePin,
    pub conformance: EvidencePin,
    pub price: PriceSnapshot,
    /// Qualified inclusive upper bound, including provider framing/scaffolding.
    pub input_ceiling: Units,
    pub output_ceiling: Units,
    pub expires_at: Timestamp,
}

/// Private worker input, constructed from CURRENT registered installation and
/// actual policy-authorized artifact reads, never from the submitted record.
/// Matching hashes alone is not sufficient evidence of installation authority.
pub(crate) struct CurrentInstallation {
    pub workspace: WorkspaceId,
    pub owner: OwnerEpoch,
    pub revision: Revision,
    pub record_digest: String,
    pub catalog: EvidencePin,
    pub conformance: EvidencePin,
    pub configuration_digest: String,
    pub now: Timestamp,
    pub native_bound: Option<decision::native_bound::NativeChargeBound>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rejection {
    Unqualified,
    NativeChargeBoundUnqualified,
    FixtureOnly,
    Stale,
    Bounds,
    Quote,
    Body,
}
impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unqualified => "decision operation is not qualified",
            Self::NativeChargeBoundUnqualified => "native_charge_bound_unqualified",
            Self::FixtureOnly => "fixture decision cannot use production transport",
            Self::Stale => "decision qualification is no longer current",
            Self::Bounds => "decision operation exceeds finite bounds",
            Self::Quote => "decision charge ceiling cannot be quoted",
            Self::Body => "decision body does not match its operation",
        })
    }
}
impl std::error::Error for Rejection {}
type Result<T> = std::result::Result<T, Rejection>;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Origin {
    Production,
    #[cfg(feature = "qualification")]
    Fixture,
}

/// Opaque host-local capability. No Serialize/Deserialize/Clone/Debug.
/// Still requires current provenance/credential/ledger admission before send.
pub(crate) struct Capability {
    record: QualificationRecord,
    digest: String,
    owner: OwnerEpoch,
    origin: Origin,
}
impl Capability {
    pub(crate) fn install(
        record: QualificationRecord,
        current: &CurrentInstallation,
    ) -> Result<Self> {
        // The owner-installed number alone is not a service bound. The worker
        // independently parses the exact captured catalog on every installation.
        if record.evaluator.operation == Operation::JevDecisions {
            let bound = current
                .native_bound
                .as_ref()
                .ok_or(Rejection::NativeChargeBoundUnqualified)?;
            if bound.raw_sha256 != record.catalog.digest
                || bound.model != record.evaluator.model
                || bound.provider != record.evaluator.provider
                || record.input_ceiling < bound.input
                || vcp_models::catalog::usd_micros(&record.evaluator.output_price_per_million)
                    .map_err(|_| Rejection::Quote)?
                    != 0
                || record
                    .price
                    .rates
                    .get(&ChargeCategory::Output)
                    .is_none_or(|r| r.micros.get() != 0)
                || record
                    .price
                    .rates
                    .get(&ChargeCategory::ProviderTool)
                    .is_none_or(|r| r.micros.get() != 0)
            {
                return Err(Rejection::NativeChargeBoundUnqualified);
            }
            for (category, minimum) in [
                (ChargeCategory::Input, &bound.input_rate),
                (ChargeCategory::CacheRead, &bound.cache_read_rate),
                (ChargeCategory::CacheWrite, &bound.cache_write_rate),
                (ChargeCategory::Request, &bound.request_rate),
            ] {
                let actual = record.price.rates.get(&category).ok_or(Rejection::Quote)?;
                if actual.per_units.get() == 0
                    || u128::from(actual.micros.get()) * u128::from(minimum.per_units.get())
                        < u128::from(minimum.micros.get()) * u128::from(actual.per_units.get())
                {
                    return Err(Rejection::Quote);
                }
            }
        }
        Self::checked(record, current, Origin::Production)
    }
    #[cfg(feature = "qualification")]
    pub(crate) fn fixture(
        record: QualificationRecord,
        current: &CurrentInstallation,
    ) -> Result<Self> {
        Self::checked(record, current, Origin::Fixture)
    }
    fn checked(
        record: QualificationRecord,
        current: &CurrentInstallation,
        origin: Origin,
    ) -> Result<Self> {
        let digest = vcp_protocol::digest_bytes(
            &vcp_protocol::canonical_bytes(&record).map_err(|_| Rejection::Unqualified)?,
        );
        let capability = Self {
            record,
            digest,
            owner: current.owner,
            origin,
        };
        capability.current(current)?;
        let r = &capability.record;
        if r.version != 1
            || !matches!(r.evaluator.mode, Mode::Shadow | Mode::Advisory)
            || r.input_ceiling == Units::ZERO
            || r.output_ceiling == Units::ZERO
            || !vcp_domain::accounting::valid_hash(&r.question_revision)
            || r.evaluator.evidence_digest != r.conformance.digest
            || r.price.capability != r.evaluator.configuration_digest
            || r.price.model != r.evaluator.model
            || r.price.provider != r.evaluator.provider
            || r.price.currency.code() != "USD"
            || r.evaluator.operation == Operation::ConventionalChat
                && r.output_ceiling.get() != decision::CONVENTIONAL_OUTPUT_LIMIT
        {
            return Err(Rejection::Unqualified);
        }
        // Six explicit price categories; no missing category becomes free.
        capability.quote(current.now)?;
        Ok(capability)
    }
    pub(crate) fn current(&self, current: &CurrentInstallation) -> Result<()> {
        let r = &self.record;
        if r.workspace != current.workspace
            || self.owner != current.owner
            || r.revision != current.revision
            || self.digest != current.record_digest
            || r.catalog != current.catalog
            || r.conformance != current.conformance
            || r.evaluator.configuration_digest != current.configuration_digest
            || r.expires_at <= current.now
            || r.evaluator.valid_until <= current.now
            || r.price.valid_until <= current.now
        {
            return Err(Rejection::Stale);
        }
        for pin in [&r.catalog, &r.conformance] {
            if !vcp_domain::accounting::valid_hash(&pin.digest) {
                return Err(Rejection::Unqualified);
            }
        }
        Ok(())
    }
    pub(crate) fn require_production(&self) -> Result<()> {
        if self.origin != Origin::Production {
            return Err(Rejection::FixtureOnly);
        }
        Ok(())
    }
    fn quote(&self, now: Timestamp) -> Result<CostQuote> {
        let r = &self.record;
        // Ceiling rates for wire provider.max_price, not a lower observed tariff.
        // Host installation separately proves all controls apply to this operation.
        for (category, text, per) in [
            (
                ChargeCategory::Input,
                r.evaluator.prompt_price_per_million.as_str(),
                1_000_000,
            ),
            (
                ChargeCategory::Output,
                r.evaluator.output_price_per_million.as_str(),
                1_000_000,
            ),
            (
                ChargeCategory::Request,
                r.evaluator.request_price.as_str(),
                1,
            ),
        ] {
            let ceiling = vcp_models::catalog::usd_micros(text).map_err(|_| Rejection::Quote)?;
            let rate = r.price.rates.get(&category).ok_or(Rejection::Quote)?;
            if rate.per_units.get() == 0
                || u128::from(rate.micros.get()) * per
                    < u128::from(ceiling) * u128::from(rate.per_units.get())
            {
                return Err(Rejection::Quote);
            }
        }
        let i = r.input_ceiling.get();
        let bounds = Usage {
            input: Units::new(i.checked_mul(3).ok_or(Rejection::Bounds)?),
            cache_read: Units::new(i),
            cache_write: Units::new(i),
            output: r.output_ceiling,
            requests: Units::new(1),
            ..Usage::default()
        };
        vcp_budget::arithmetic::quote(r.price.clone(), bounds, now).map_err(|_| Rejection::Quote)
    }
    /// The worker supplies Request from its actual routing/escalation seed, never
    /// caller JSON. Enabling a mode remains a separate host configuration gate.
    /// Count comes from durable attempts for that run, including failed/unknown.
    pub(crate) fn prepare(
        &self,
        request: &Request,
        attempts_used: u32,
        current: &CurrentInstallation,
    ) -> Result<FinitePrepared> {
        self.current(current)?;
        let r = &self.record;
        if attempts_used != 0
            || request.question_revision != r.question_revision
            || request.binding.scope.workspace != r.workspace
            || request.deadline > r.expires_at
            || request.deadline > r.evaluator.valid_until
            || request.deadline > r.price.valid_until
        {
            return Err(Rejection::Stale);
        }
        let prepared = decision::prepare(
            request,
            &decision::Policy {
                mode: r.evaluator.mode,
                evaluator: Some(r.evaluator.clone()),
                attempt_limit: 1,
                attempts_used,
            },
            current.now,
        )
        .map_err(|_| Rejection::Unqualified)?
        .ok_or(Rejection::Unqualified)?;
        let body = prepared.body();
        if prepared.endpoint() != r.evaluator.operation.endpoint() {
            return Err(Rejection::Body);
        }
        match r.evaluator.operation {
            Operation::ConventionalChat
                if body["max_tokens"].as_u64() != Some(decision::CONVENTIONAL_OUTPUT_LIMIT)
                    || body["stream"].as_bool() != Some(false)
                    || body.get("tools").is_some() =>
            {
                return Err(Rejection::Body)
            }
            Operation::JevDecisions
                if body.get("max_tokens").is_some()
                    || body.get("max_output_tokens").is_some()
                    || body.get("tools").is_some() =>
            {
                return Err(Rejection::Body)
            }
            _ => (),
        }
        let bytes = vcp_protocol::canonical_bytes(body).map_err(|_| Rejection::Body)?;
        if bytes.len() > decision::MAX_BYTES || bytes.len() as u64 > r.input_ceiling.get() {
            return Err(Rejection::Bounds);
        }
        Ok(FinitePrepared {
            prepared,
            quote: self.quote(current.now)?,
            body_digest: vcp_protocol::digest_bytes(&bytes),
            bytes,
        })
    }
}
/// Not a SendPermit. Worker must atomically capture+reserve, then submit.
pub(crate) struct FinitePrepared {
    pub prepared: Prepared,
    pub quote: CostQuote,
    pub body_digest: String,
    pub bytes: Vec<u8>,
}

#[cfg(test)]
#[path = "admission_tests.rs"]
mod tests;
