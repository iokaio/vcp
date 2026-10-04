// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::cmp::Ordering;

fn current(observed: Timestamp, until: Timestamp, input: &RoutingInput, policy: &Policy) -> bool {
    observed <= input.now
        && input.now < until
        && input.now.get().saturating_sub(observed.get()) <= policy.maximum_evidence_age_ms
}
fn empty(identity: ModelEndpoint) -> CandidateDecision {
    CandidateDecision {
        identity,
        exclusions: vec![],
        source_reasons: vec![],
        group: None,
        quality_bps: None,
        samples: None,
        latency_p95_ms: None,
        total_estimate: None,
        evidence_refs: vec![],
        broader_cohort_used: None,
        assumptions: vec![],
    }
}
fn cost(snapshot: &Snapshot, usage: &Usage) -> std::result::Result<Micros, Exclusion> {
    if !validation::valid_usage(usage) {
        return Err(Exclusion::InvalidCost);
    }
    let categories = usage.disjoint().map_err(|_| Exclusion::InvalidCost)?;
    let mut total = 0u64;
    for (category, units) in categories {
        let rate = snapshot
            .price
            .rates
            .get(&category)
            .ok_or(Exclusion::UnknownCost)?;
        if rate.per_units == Units::ZERO {
            return Err(Exclusion::InvalidCost);
        }
        // Same checked, upward-rounded money convention as ledger arithmetic.
        let numerator = u128::from(rate.micros.get()) * u128::from(units.get());
        let denominator = u128::from(rate.per_units.get());
        let amount = numerator / denominator + u128::from(numerator % denominator != 0);
        let amount = u64::try_from(amount).map_err(|_| Exclusion::InvalidCost)?;
        total = total.checked_add(amount).ok_or(Exclusion::InvalidCost)?;
    }
    Ok(Micros::new(total))
}
fn estimate_cost(
    snapshot: &Snapshot,
    estimate: &CostEstimate,
    input_tokens: Units,
    output_tokens: Units,
) -> std::result::Result<CostBreakdown, Exclusion> {
    if estimate.first_attempt.requests == Units::ZERO
        || estimate.first_attempt.input < input_tokens
        || estimate.first_attempt.output < output_tokens
    {
        return Err(Exclusion::InvalidCost);
    }
    let fixed = |value: &Option<Money>| {
        let value = value.as_ref().ok_or(Exclusion::UnknownCost)?;
        if value.currency != snapshot.price.currency {
            return Err(Exclusion::CurrencyMismatch);
        }
        Ok(value.micros)
    };
    let first_attempt = cost(snapshot, &estimate.first_attempt)?;
    let retries = cost(snapshot, &estimate.retries)?;
    let handoff = cost(snapshot, &estimate.handoff)?;
    let support = fixed(&estimate.support)?;
    let children = fixed(&estimate.children)?;
    let verification = fixed(&estimate.verification)?;
    let total = [
        first_attempt,
        retries,
        handoff,
        support,
        children,
        verification,
    ]
    .into_iter()
    .try_fold(0u64, |sum, part| {
        sum.checked_add(part.get()).ok_or(Exclusion::InvalidCost)
    })?;
    Ok(CostBreakdown {
        first_attempt,
        retries,
        handoff,
        support,
        children,
        verification,
        total: Money {
            currency: snapshot.price.currency.clone(),
            micros: Micros::new(total),
        },
    })
}
fn role_evidence<'a>(
    candidate: &'a Candidate,
    policy: &Policy,
    input: &RoutingInput,
    row: &mut CandidateDecision,
) -> Option<(&'a GroupMembership, &'a RoleEvidence)> {
    let matching: Vec<_> = candidate
        .memberships
        .iter()
        .flat_map(|group| group.roles.iter().map(move |role| (group, role)))
        .filter(|(_, evidence)| evidence.role == input.role)
        .collect();
    let find = |class: &str| {
        let mut rows: Vec<_> = matching
            .iter()
            .copied()
            .filter(|(_, evidence)| {
                evidence.task_class == class
                    && evidence.kind == EvidenceKind::Live
                    && current(evidence.observed_at, evidence.valid_until, input, policy)
            })
            .collect();
        rows.sort_by(|a, b| {
            b.1.observed_at
                .cmp(&a.1.observed_at)
                .then(a.1.id.cmp(&b.1.id))
        });
        rows
    };
    let mut pool = find(&input.task_class);
    let mut selected_class = input.task_class.as_str();
    if pool.is_empty() {
        if let Some(broader) = &policy.broader_task_class {
            pool = find(broader);
            if !pool.is_empty() {
                selected_class = broader;
                row.broader_cohort_used = Some(broader.clone());
            }
        }
    }
    if pool.is_empty() {
        row.exclusions.push(
            if matching
                .iter()
                .any(|(_, e)| e.task_class == input.task_class && e.kind == EvidenceKind::Live)
            {
                Exclusion::StaleRoleEvidence
            } else {
                Exclusion::MissingRoleEvidence
            },
        );
        return None;
    }
    let groups: BTreeSet<_> = matching
        .iter()
        .filter(|(_, e)| {
            e.task_class == selected_class && current(e.observed_at, e.valid_until, input, policy)
        })
        .map(|(group, _)| group.group)
        .collect();
    if groups.len() != 1 {
        row.exclusions.push(Exclusion::ContradictoryMembership);
    }
    pool.first().copied()
}
fn evaluate(
    candidate: &Candidate,
    catalog: &CatalogRevision,
    policy: &Policy,
    input: &RoutingInput,
    owner_selected: bool,
) -> CandidateDecision {
    let mut row = empty(candidate.identity.clone());
    let Some((input_tokens, output_tokens)) = input.request_for(&candidate.identity) else {
        row.exclusions.push(Exclusion::ContextCapacity);
        row.assumptions
            .push("No valid request encoding/allocation for this candidate.".into());
        return row;
    };
    if policy
        .input_tokens
        .is_some_and(|limit| input_tokens > limit)
        || policy
            .output_tokens
            .is_some_and(|limit| output_tokens > limit)
    {
        row.exclusions.push(Exclusion::ContextCapacity);
    }
    if input
        .retry_pin
        .as_ref()
        .is_some_and(|pin| pin != &candidate.identity)
    {
        row.exclusions.push(Exclusion::RetryPinned);
    }
    if input.excluded.contains(&candidate.identity) {
        row.exclusions.push(Exclusion::PreviouslyFailed);
    }
    row.source_reasons = candidate.reasons.clone();
    row.evidence_refs
        .extend(candidate.provenance.iter().map(|p| p.source.clone()));
    if catalog.observed_at > input.now || catalog.effective_at.is_some_and(|date| date > input.now)
    {
        row.exclusions.push(Exclusion::CatalogNotEffective);
    }
    match candidate.availability {
        State::Unsupported => row.exclusions.push(Exclusion::Unavailable),
        State::Unknown => row.exclusions.push(Exclusion::UnknownAvailability),
        State::Supported => {}
    }
    if !policy.allowed_models.contains(&candidate.identity.model) {
        row.exclusions.push(Exclusion::ModelDenied);
    }
    if !policy
        .allowed_endpoints
        .contains(&candidate.identity.endpoint)
    {
        row.exclusions.push(Exclusion::ProviderDenied);
    }
    for capability in &input.required_capabilities {
        match candidate.capabilities.get(capability) {
            Some(State::Supported) => {}
            Some(State::Unsupported) => row.exclusions.push(Exclusion::UnsupportedCapability),
            _ => row.exclusions.push(Exclusion::UnknownCapability),
        }
    }
    let Some(snapshot) = &candidate.snapshot else {
        row.exclusions.push(Exclusion::MissingSnapshot);
        return row;
    };
    row.evidence_refs.push(snapshot.id.clone());
    if crate::reasoning::validate(snapshot, policy.reasoning_effort).is_err() {
        row.exclusions.push(Exclusion::UnsupportedReasoningEffort);
    }
    if snapshot.current(input.now).is_err() {
        row.exclusions.push(Exclusion::StaleSnapshot);
    }
    if !snapshot.compatibility.byte_ceiling_qualified {
        // Request bytes remain a context estimate, never a tokenizer guarantee.
        // Admission uses the full endpoint input capacity independently for
        // ordinary input and both cache categories. Check that immediate bound
        // separately from the configured expected task cost used for ranking.
        row.assumptions.push("Input byte estimate is unqualified; immediate admission reserves the full endpoint input capacity for ordinary input, cache read and cache write.".into());
        let bound = snapshot.reservation_input(input_tokens);
        let maximum = bound
            .get()
            .checked_mul(3)
            .ok_or(Exclusion::InvalidCost)
            .and_then(|inclusive| {
                cost(
                    snapshot,
                    &Usage {
                        input: Units::new(inclusive),
                        cache_read: bound,
                        cache_write: bound,
                        output: output_tokens,
                        requests: Units::new(1),
                        ..Usage::default()
                    },
                )
            });
        match maximum {
            Err(Exclusion::UnknownCost) if input.available.micros.is_unbounded() => row
                .assumptions
                .push("Immediate monetary bound unavailable; cost remains unknown.".into()),
            Err(reason) => row.exclusions.push(reason),
            Ok(maximum) => {
                let protected = if input.role == RequestRole::Verification {
                    0
                } else {
                    input.protected_verification.get()
                };
                if maximum
                    .get()
                    .checked_add(protected)
                    .is_none_or(|amount| input.available.micros.exceeds(&Micros::new(amount)))
                {
                    row.exclusions.push(Exclusion::Budget);
                }
            }
        }
    }
    let observations: Vec<_> = candidate
        .compatibility
        .iter()
        .filter(|observation| {
            observation.kind == EvidenceKind::Live
                && observation.compatibility == snapshot.compatibility.id
                && current(
                    observation.observed_at,
                    observation.valid_until,
                    input,
                    policy,
                )
        })
        .collect();
    if !owner_selected
        && !observations
            .iter()
            .any(|observation| observation.state == State::Supported)
    {
        row.exclusions.push(Exclusion::MissingLiveQualification);
    }
    if observations
        .iter()
        .any(|observation| observation.state != State::Supported)
    {
        row.exclusions.push(Exclusion::ContradictoryCompatibility);
    }
    row.evidence_refs.extend(
        observations
            .iter()
            .map(|observation| observation.id.clone()),
    );
    if (policy.deny_data_collection && !snapshot.compatibility.deny_data_collection)
        || (policy.require_zdr && !snapshot.compatibility.require_zdr)
    {
        row.exclusions.push(Exclusion::DataPolicy);
    }
    if input_tokens > snapshot.max_input
        || output_tokens > snapshot.max_output
        || input_tokens
            .get()
            .checked_add(output_tokens.get())
            .is_none_or(|total| total > snapshot.context.get())
    {
        row.exclusions.push(Exclusion::ContextCapacity);
    }
    if owner_selected {
        row.assumptions.push("Explicit owner role assignment; no measured quality or live model qualification is claimed. Eligibility uses current provider compatibility and metadata.".into());
    } else if let Some((membership, evidence)) = role_evidence(candidate, policy, input, &mut row) {
        row.group = Some(membership.group);
        row.quality_bps = Some(evidence.quality_bps);
        row.samples = Some(evidence.samples);
        row.latency_p95_ms = Some(evidence.latency_p95_ms);
        row.evidence_refs
            .extend([membership.version.clone(), evidence.id.clone()]);
        row.evidence_refs
            .extend(evidence.provenance.iter().map(|p| p.source.clone()));
        if !policy.allowed_groups.contains(&membership.group) {
            row.exclusions.push(Exclusion::GroupDenied);
        }
        if evidence.samples < policy.minimum_samples {
            row.exclusions.push(Exclusion::InsufficientSamples);
        }
        if evidence.quality_bps < policy.quality_floor_bps {
            row.exclusions.push(Exclusion::QualityFloor);
        }
    }
    match input
        .estimates
        .iter()
        .find(|estimate| estimate.candidate == candidate.identity)
    {
        None if input.available.micros.is_unbounded() => row
            .assumptions
            .push("Cost estimate unavailable; cost remains unknown.".into()),
        None => row.exclusions.push(Exclusion::MissingCostEstimate),
        Some(estimate) => {
            row.assumptions.extend(estimate.assumptions.clone());
            row.evidence_refs.extend(estimate.evidence_refs.clone());
            match estimate_cost(snapshot, estimate, input_tokens, output_tokens) {
                Err(Exclusion::UnknownCost) if input.available.micros.is_unbounded() => row
                    .assumptions
                    .push("Cost estimate incomplete; cost remains unknown.".into()),
                Err(reason) => row.exclusions.push(reason),
                Ok(cost) => {
                    if cost.total.currency != input.available.currency {
                        row.exclusions.push(Exclusion::CurrencyMismatch);
                    }
                    if !input.available.micros.is_unbounded()
                        && cost.verification > input.protected_verification
                    {
                        row.exclusions
                            .push(Exclusion::InsufficientVerificationReserve);
                    }
                    let protected = if input.role == RequestRole::Verification {
                        0
                    } else {
                        input.protected_verification.get()
                    };
                    let without_verification = cost.total.micros.get() - cost.verification.get();
                    let required =
                        without_verification.checked_add(protected.max(cost.verification.get()));
                    if required
                        .is_none_or(|amount| input.available.micros.exceeds(&Micros::new(amount)))
                    {
                        row.exclusions.push(Exclusion::Budget);
                    }
                    row.total_estimate = Some(cost);
                }
            }
        }
    }
    // Distinct required capabilities may produce the same stable reason code.
    let mut distinct = Vec::new();
    for reason in row.exclusions {
        if !distinct.contains(&reason) {
            distinct.push(reason);
        }
    }
    row.exclusions = distinct;
    row.evidence_refs.sort();
    row.evidence_refs.dedup();
    row
}
fn compare(a: &CandidateDecision, b: &CandidateDecision, ordering: &[Preference]) -> Ordering {
    match (a.exclusions.is_empty(), b.exclusions.is_empty()) {
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        (false, false) => return a.identity.cmp(&b.identity),
        (true, true) => {}
    }
    for preference in ordering {
        let order = match preference {
            Preference::Quality => b.quality_bps.cmp(&a.quality_bps),
            Preference::Latency => a.latency_p95_ms.cmp(&b.latency_p95_ms),
            Preference::Capability => a.group.cmp(&b.group),
            Preference::TotalCost => a
                .total_estimate
                .as_ref()
                .map(|cost| cost.total.micros)
                .cmp(&b.total_estimate.as_ref().map(|cost| cost.total.micros)),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    a.identity.cmp(&b.identity)
}
pub fn select(
    catalog: &CatalogRevision,
    policy: &Policy,
    input: &RoutingInput,
) -> Result<RoutingDecision> {
    select_inner(catalog, policy, input, None)
}

/// Explicit owner order is a preference, never empirical quality evidence.
/// Every other capability, exact identity, privacy, context and budget gate
/// still applies. An empty set denies dispatch; it never expands selection.
pub fn select_owner_set(
    catalog: &CatalogRevision,
    policy: &Policy,
    input: &RoutingInput,
    ordered: &[ModelEndpoint],
) -> Result<RoutingDecision> {
    if ordered.len() > 32 || ordered.iter().collect::<BTreeSet<_>>().len() != ordered.len() {
        return Err(Error::Protocol(
            "owner model set bounds or duplicate identity",
        ));
    }
    ordered.iter().try_for_each(ModelEndpoint::validate)?;
    select_inner(catalog, policy, input, Some(ordered))
}

fn select_inner(
    catalog: &CatalogRevision,
    policy: &Policy,
    input: &RoutingInput,
    owner_order: Option<&[ModelEndpoint]>,
) -> Result<RoutingDecision> {
    catalog.validate()?;
    policy.validate()?;
    input.validate()?;
    if input.catalog != catalog.id || input.policy != policy.id {
        return Err(Error::Stale);
    }
    if input.candidate_requests.is_empty()
        && policy
            .input_tokens
            .is_some_and(|limit| input.input_tokens > limit)
    {
        return Err(Error::Capability(
            "routing input exceeds selected policy limit",
        ));
    }
    if input.candidate_requests.is_empty()
        && policy
            .output_tokens
            .is_some_and(|limit| input.output_tokens > limit)
    {
        return Err(Error::Capability(
            "routing output exceeds selected policy limit",
        ));
    }
    let mut candidates: Vec<_> = catalog
        .entries
        .iter()
        .map(|candidate| {
            let mut row = evaluate(candidate, catalog, policy, input, owner_order.is_some());
            if owner_order.is_some_and(|order| !order.contains(&candidate.identity)) {
                row.exclusions.push(Exclusion::PinRestricted);
            }
            row
        })
        .collect();
    if let Some(pin) = &policy.pin {
        if !candidates.iter().any(|row| row.identity == pin.candidate) {
            let mut row = empty(pin.candidate.clone());
            row.exclusions.push(Exclusion::Unavailable);
            row.source_reasons
                .push("Pinned exact identity is absent from this catalog revision".into());
            candidates.push(row);
        }
        let available = candidates
            .iter()
            .any(|row| row.identity == pin.candidate && row.exclusions.is_empty());
        for row in &mut candidates {
            if row.identity != pin.candidate {
                if !pin.fallback_candidates.contains(&row.identity) {
                    row.exclusions.push(Exclusion::PinRestricted);
                } else if available {
                    row.exclusions.push(Exclusion::PinPreferred);
                }
            }
        }
    }
    let ordering = if matches!(input.role, RequestRole::Main | RequestRole::Child) {
        policy.ordering.clone()
    } else {
        vec![
            Preference::TotalCost,
            Preference::Latency,
            Preference::Quality,
            Preference::Capability,
        ]
    };
    candidates.sort_by(|a, b| {
        if let Some(order) = owner_order {
            b.exclusions
                .is_empty()
                .cmp(&a.exclusions.is_empty())
                .then_with(|| {
                    order
                        .iter()
                        .position(|id| id == &a.identity)
                        .unwrap_or(usize::MAX)
                        .cmp(
                            &order
                                .iter()
                                .position(|id| id == &b.identity)
                                .unwrap_or(usize::MAX),
                        )
                })
                .then(a.identity.cmp(&b.identity))
        } else {
            compare(a, b, &ordering)
        }
    });
    let selected = candidates
        .first()
        .filter(|row| row.exclusions.is_empty())
        .map(|row| row.identity.clone());
    let fallback_from = owner_order
        .and_then(|order| order.first())
        .filter(|primary| selected.as_ref().is_some_and(|id| id != *primary))
        .cloned()
        .or_else(|| {
            policy
                .pin
                .as_ref()
                .filter(|pin| selected.as_ref().is_some_and(|id| id != &pin.candidate))
                .map(|pin| pin.candidate.clone())
        });
    let mut decision = RoutingDecision {
        schema_version: SCHEMA_VERSION,
        id: String::new(),
        input: input.clone(),
        profile: policy.profile,
        ordering,
        candidates,
        selected,
        fallback_from,
        immediate_reservation: None,
    };
    decision.id = decision.digest()?;
    decision.validate()?;
    Ok(decision)
}
