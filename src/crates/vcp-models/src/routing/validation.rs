// SPDX-License-Identifier: Apache-2.0
use super::*;
pub(super) fn text(value: &str, maximum: usize) -> Result<()> {
    if value.is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(Error::Protocol("routing bounded text"));
    }
    Ok(())
}
pub(super) fn hash(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(Error::Protocol("routing digest"));
    }
    Ok(())
}
fn strings(values: &[String], maximum: usize) -> Result<()> {
    if values.len() > maximum {
        return Err(Error::Limit("routing evidence references"));
    }
    values.iter().try_for_each(|value| text(value, 2048))
}
impl ModelEndpoint {
    pub fn validate(&self) -> Result<()> {
        text(&self.model, 256)?;
        text(&self.endpoint, 256)
    }
}
impl Provenance {
    fn validate(&self, observed_at: Timestamp) -> Result<()> {
        text(&self.source, 2048)?;
        hash(&self.sha256)?;
        strings(&self.limitations, 32)?;
        if self.observed_at > observed_at {
            return Err(Error::Protocol("future routing provenance"));
        }
        Ok(())
    }
}
fn provenance(values: &[Provenance], observed_at: Timestamp) -> Result<()> {
    if values.is_empty() || values.len() > 32 {
        return Err(Error::Limit("routing provenance count"));
    }
    values
        .iter()
        .try_for_each(|value| value.validate(observed_at))
}
fn usage(value: &Usage) -> Result<()> {
    value
        .disjoint()
        .map_err(|_| Error::Protocol("routing usage categories"))?;
    if value.requests == Units::ZERO && *value != Usage::default() {
        return Err(Error::Protocol("routing usage without request"));
    }
    Ok(())
}
fn snapshot(value: &Snapshot, identity: &ModelEndpoint, observed_at: Timestamp) -> Result<()> {
    hash(&value.id)?;
    hash(&value.raw_sha256)?;
    let compatibility = &value.compatibility;
    text(&compatibility.id, 256)?;
    if compatibility.model != identity.model
        || compatibility.endpoint != identity.endpoint
        || compatibility.qualified_at > value.observed_at
        || value.observed_at > observed_at
        || value.valid_until <= value.observed_at
        || compatibility.valid_until < value.valid_until
        || !compatibility.responses_text_tools
        || !compatibility.byte_ceiling_qualified
        || !compatibility.provider_preferences_qualified
        || (!compatibility.qualified_reasoning_efforts.is_empty()
            && !compatibility.required_parameters.contains("reasoning"))
        || value.context == Units::ZERO
        || value.max_input == Units::ZERO
        || value.max_input > value.context
        || value.max_output == Units::ZERO
        || value.max_output > value.context
        || value.price.id != value.id
        || value.price.model != identity.model
        || value.price.provider != identity.endpoint
        || value.price.valid_until != value.valid_until
        || value.price.currency.code() != "USD"
    {
        return Err(Error::Protocol(
            "routing qualified snapshot identity or limits",
        ));
    }
    crate::catalog::usd_micros(&compatibility.request_price_limit)?;
    let digest = value.identity_digest()?;
    let capability = vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(compatibility)?);
    if value.id != digest || value.price.capability != capability {
        return Err(Error::Protocol("routing snapshot digest"));
    }
    for category in [
        ChargeCategory::Input,
        ChargeCategory::Output,
        ChargeCategory::CacheRead,
        ChargeCategory::CacheWrite,
        ChargeCategory::Request,
        ChargeCategory::ProviderTool,
    ] {
        if value
            .price
            .rates
            .get(&category)
            .is_none_or(|rate| rate.per_units == Units::ZERO)
        {
            return Err(Error::Protocol("routing snapshot incomplete price units"));
        }
    }
    Ok(())
}
impl Candidate {
    fn validate(&self, observed_at: Timestamp) -> Result<()> {
        self.identity.validate()?;
        strings(&self.reasons, 32)?;
        provenance(&self.provenance, observed_at)?;
        if self.availability != State::Supported && self.reasons.is_empty() {
            return Err(Error::Protocol(
                "routing unavailable candidate needs a reason",
            ));
        }
        if self.capabilities.len() > 32
            || self.compatibility.len() > 32
            || self.memberships.len() > 32
        {
            return Err(Error::Limit("routing candidate evidence"));
        }
        self.capabilities
            .keys()
            .try_for_each(|name| text(name, 128))?;
        if let Some(value) = &self.snapshot {
            snapshot(value, &self.identity, observed_at)?;
        }
        let mut ids = BTreeSet::new();
        for observation in &self.compatibility {
            text(&observation.id, 256)?;
            text(&observation.compatibility, 256)?;
            if !ids.insert(&observation.id)
                || observation.observed_at > observed_at
                || observation.valid_until <= observation.observed_at
            {
                return Err(Error::Protocol(
                    "routing compatibility evidence identity or dates",
                ));
            }
            provenance(&observation.provenance, observed_at)?;
        }
        let mut memberships = BTreeSet::new();
        for membership in &self.memberships {
            text(&membership.version, 256)?;
            if !memberships.insert(&membership.version)
                || membership.roles.is_empty()
                || membership.roles.len() > 64
            {
                return Err(Error::Protocol("routing group evidence count or identity"));
            }
            for evidence in &membership.roles {
                text(&evidence.id, 256)?;
                text(&evidence.task_class, 256)?;
                if !ids.insert(&evidence.id)
                    || evidence.observed_at > observed_at
                    || evidence.valid_until <= evidence.observed_at
                    || evidence.quality_bps > 10_000
                    || evidence.latency_p50_ms > evidence.latency_p95_ms
                {
                    return Err(Error::Protocol("routing role evidence identity or metrics"));
                }
                provenance(&evidence.provenance, observed_at)?;
                for value in [&evidence.usage_p50, &evidence.usage_p95]
                    .into_iter()
                    .flatten()
                {
                    usage(value)?;
                }
            }
        }
        Ok(())
    }
}
impl CatalogRevision {
    pub fn create(
        parent: Option<String>,
        observed_at: Timestamp,
        effective_at: Option<Timestamp>,
        mut entries: Vec<Candidate>,
    ) -> Result<Self> {
        entries.sort_by(|a, b| a.identity.cmp(&b.identity));
        let mut value = Self {
            schema_version: SCHEMA_VERSION,
            id: String::new(),
            parent,
            observed_at,
            effective_at,
            entries,
        };
        value.id = value.digest()?;
        value.validate()?;
        Ok(value)
    }
    pub fn digest(&self) -> Result<String> {
        let mut copy = self.clone();
        copy.id.clear();
        Ok(vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(
            &copy,
        )?))
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION || self.entries.len() > 1024 {
            return Err(Error::Protocol("routing catalog version or count"));
        }
        if let Some(parent) = &self.parent {
            hash(parent)?;
        }
        if self.id != self.digest()? {
            return Err(Error::Protocol("routing catalog immutable digest"));
        }
        let mut identities = BTreeSet::new();
        for candidate in &self.entries {
            if !identities.insert(&candidate.identity) {
                return Err(Error::Protocol("duplicate routing endpoint"));
            }
            candidate.validate(self.observed_at)?;
        }
        Ok(())
    }
    pub fn snapshot(&self, identity: &ModelEndpoint) -> Option<&Snapshot> {
        self.entries
            .iter()
            .find(|entry| &entry.identity == identity)
            .and_then(|entry| entry.snapshot.as_ref())
    }
}
impl Policy {
    pub fn seal(mut self) -> Result<Self> {
        self.id = self.digest()?;
        self.validate()?;
        Ok(self)
    }
    pub fn digest(&self) -> Result<String> {
        let mut copy = self.clone();
        copy.id.clear();
        Ok(vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(
            &copy,
        )?))
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION || self.id != self.digest()? {
            return Err(Error::Protocol(
                "routing policy version or immutable digest",
            ));
        }
        if let Some(parent) = &self.parent {
            hash(parent)?;
        }
        if self.quality_floor_bps > 10_000
            || self.minimum_samples == 0
            || self.maximum_evidence_age_ms == 0
            || self.output_tokens == Some(Units::ZERO)
            || self.input_tokens == Some(Units::ZERO)
            || self.allowed_models.len() > 1024
            || self.allowed_endpoints.len() > 1024
        {
            return Err(Error::Protocol("routing policy quality or bounds"));
        }
        for value in self.allowed_models.iter().chain(&self.allowed_endpoints) {
            text(value, 256)?;
        }
        if let Some(limits) = &self.escalation_limits {
            limits.validate()?;
        }
        if let Some(limits) = &self.retrieval_limits {
            limits.validate()?;
        }
        let ordering: BTreeSet<_> = self.ordering.iter().collect();
        if self.ordering.len() != 4 || ordering.len() != 4 {
            return Err(Error::Protocol("routing comparison ordering"));
        }
        if (self.profile == Profile::Low
            && !matches!(
                self.ordering[0],
                Preference::TotalCost | Preference::Latency
            ))
            || (self.profile == Profile::High
                && !matches!(
                    self.ordering[0],
                    Preference::Quality | Preference::Capability
                ))
        {
            return Err(Error::Protocol("routing profile comparison preference"));
        }
        if let Some(pin) = &self.pin {
            pin.candidate.validate()?;
            if pin.fallback_candidates.len() > 1024
                || pin.fallback_candidates.contains(&pin.candidate)
            {
                return Err(Error::Protocol("routing explicit fallback candidates"));
            }
            for candidate in &pin.fallback_candidates {
                candidate.validate()?;
            }
        }
        if let Some(class) = &self.broader_task_class {
            text(class, 256)?;
        }
        Ok(())
    }
}
impl RoutingInput {
    pub fn validate(&self) -> Result<()> {
        hash(&self.input_digest)?;
        hash(&self.catalog)?;
        hash(&self.policy)?;
        text(&self.task_class, 256)?;
        if self.required_capabilities.len() > 32
            || self.excluded.len() > 128
            || self.estimates.len() > 1024
            || self.output_tokens == Units::ZERO
        {
            return Err(Error::Protocol("routing input bounds"));
        }
        for capability in &self.required_capabilities {
            text(capability, 128)?;
        }
        for identity in &self.excluded {
            identity.validate()?;
        }
        if let Some(identity) = &self.retry_pin {
            identity.validate()?;
        }
        let mut candidates = BTreeSet::new();
        for estimate in &self.estimates {
            estimate.candidate.validate()?;
            if !candidates.insert(&estimate.candidate) {
                return Err(Error::Protocol("duplicate routing estimate"));
            }
            strings(&estimate.assumptions, 32)?;
            strings(&estimate.evidence_refs, 32)?;
            if estimate.assumptions.is_empty() {
                return Err(Error::Protocol("routing cost assumptions required"));
            }
            // Invalid component values remain per-candidate exclusions in select.
        }
        Ok(())
    }
}
impl RoutingDecision {
    /// Admission rechecks eligibility at its current time and budget without
    /// silently changing the captured winner or explanation.
    pub fn validate_selected_at(
        &self,
        catalog: &CatalogRevision,
        policy: &Policy,
        now: Timestamp,
        available: Money,
        protected: Micros,
    ) -> Result<()> {
        self.validate()?;
        let mut input = self.input.clone();
        input.now = now;
        input.available = available;
        input.protected_verification = protected;
        let refreshed = super::select(catalog, policy, &input)?;
        if refreshed.candidates.iter().any(|candidate| {
            Some(&candidate.identity) == self.selected.as_ref() && candidate.exclusions.is_empty()
        }) {
            Ok(())
        } else {
            Err(Error::Capability(
                "selected routing candidate is no longer eligible",
            ))
        }
    }
    pub fn digest(&self) -> Result<String> {
        let mut copy = self.clone();
        copy.id.clear();
        Ok(vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(
            &copy,
        )?))
    }
    pub fn validate(&self) -> Result<()> {
        self.input.validate()?;
        if self.schema_version != SCHEMA_VERSION
            || self.id != self.digest()?
            || self.immediate_reservation.is_some()
            || self.candidates.len() > 1025
        {
            return Err(Error::Protocol("routing decision identity or reservation"));
        }
        if self.selected.as_ref()
            != self
                .candidates
                .iter()
                .find(|row| row.exclusions.is_empty())
                .map(|row| &row.identity)
        {
            return Err(Error::Protocol("routing selected eligible identity"));
        }
        Ok(())
    }
    pub fn selected_snapshot<'a>(
        &self,
        catalog: &'a CatalogRevision,
    ) -> Result<Option<&'a Snapshot>> {
        self.validate()?;
        catalog.validate()?;
        if self.input.catalog != catalog.id {
            return Err(Error::Stale);
        }
        self.selected
            .as_ref()
            .map(|identity| {
                catalog
                    .snapshot(identity)
                    .ok_or(Error::Capability("selected routing snapshot"))
            })
            .transpose()
    }
}
pub(super) fn valid_usage(value: &Usage) -> bool {
    usage(value).is_ok()
}
