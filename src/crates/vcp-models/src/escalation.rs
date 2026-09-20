// SPDX-License-Identifier: Apache-2.0
//! P6-03 deterministic scheduling predicate, never a dispatch authorization.
//! The host persists returned counters with admission, rechecks current barriers,
//! and rebuilds context with `vcp_context::handoff::Packet::reassemble`.
use crate::{routing, Error, Result};
use serde::{Deserialize, Serialize};
use vcp_context::{
    handoff::Packet,
    manifest::{Revisions, Sealed},
};
use vcp_domain::{accounting::Ledger, *};
use vcp_protocol::{canonical_bytes, digest_bytes};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    TransportRetry,
    QualitySwitch,
    Decomposition,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TriggerKind {
    TransportRetry {
        failure: crate::retry::Failure,
    },
    InvalidToolOutput,
    FailedVerification,
    UnsupportedCapability,
    DeclaredComplexity,
    /// Equality is only a bounded stall signal; evidence must identify observed
    /// actions and progress, never hidden reasoning or a confidence assertion.
    RepeatedStrategy {
        before: String,
        after: String,
    },
    Decomposition,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trigger {
    pub kind: TriggerKind,
    pub evidence: Vec<ArtifactId>,
    pub observations: u32,
}
impl Trigger {
    pub fn class(&self) -> Class {
        match self.kind {
            TriggerKind::TransportRetry { .. } => Class::TransportRetry,
            TriggerKind::Decomposition => Class::Decomposition,
            _ => Class::QualitySwitch,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Counters {
    pub transport_retries: u32,
    pub quality_switches: u32,
    pub decompositions: u32,
    /// Includes the original request and all admitted actions at the root.
    pub total_attempts: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub max_transport_retries: u32,
    pub max_quality_switches: u32,
    pub max_decompositions: u32,
    pub max_total_attempts: u32,
    pub minimum_repeated_failures: u32,
    pub deadline: Timestamp,
}
impl Policy {
    pub fn validate(&self) -> Result<()> {
        if self.max_transport_retries > 4
            || self.max_quality_switches > 8
            || self.max_decompositions > 8
            || self.max_total_attempts == 0
            || self.max_total_attempts > 64
            || self.minimum_repeated_failures == 0
            || self.minimum_repeated_failures > 64
        {
            return Err(Error::Limit("escalation policy"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulingState {
    Running,
    Paused,
    WaitingForInput,
    Cancelled,
    Terminal,
}
/// Read from canonical scheduling state, not model output. Revision equality
/// includes steering, authority, deletion, tools, instructions and task state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Barrier {
    pub expected: Revisions,
    pub current: Revisions,
    pub state: SchedulingState,
    pub now: Timestamp,
    pub not_before: Timestamp,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Blocked {
    Scheduling,
    Stale,
    Deadline,
    Backoff,
    Limit,
    NoTrigger,
    NoCandidate,
    StrictPin,
    SameModel,
    RetryModelChanged,
    Budget,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub previous_attempt: AttemptId,
    pub previous: routing::ModelEndpoint,
    pub selected: routing::ModelEndpoint,
    pub selected_snapshot: String,
    pub selected_compatibility: String,
    pub routing_decision: String,
    pub routing_policy: String,
    pub trigger: Trigger,
    pub before: Counters,
    pub after: Counters,
    pub revisions: Revisions,
    pub policy: Policy,
    pub ledger_revision: Revision,
    pub remaining: Micros,
    pub estimated_request: Micros,
    pub estimated_handoff: Micros,
    /// Prior uncertainty is preserved, never reclassified as available funds.
    pub unresolved: Micros,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
// One scheduling result is immediately moved into its durable admission record;
// this is not a collection of mostly empty outcomes requiring boxed storage.
#[allow(clippy::large_enum_variant)]
pub enum Outcome {
    Ready { plan: Plan },
    Blocked { reason: Blocked },
}
fn blocked(reason: Blocked) -> Result<Outcome> {
    Ok(Outcome::Blocked { reason })
}
fn hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Selection has already enforced quality, capability and provider restrictions.
/// Recompute it here against the supplied policy/catalog to prevent a forged or
/// stale selection from being promoted by the escalation boundary.
#[allow(clippy::too_many_arguments)]
pub fn evaluate(
    policy: &Policy,
    counters: &Counters,
    trigger: &Trigger,
    barrier: &Barrier,
    previous_attempt: AttemptId,
    previous: &routing::ModelEndpoint,
    decision: &routing::RoutingDecision,
    catalog: &routing::CatalogRevision,
    routing_policy: &routing::Policy,
    ledger: &Ledger,
    estimated_handoff: Micros,
) -> Result<Outcome> {
    policy.validate()?;
    let actions = counters
        .transport_retries
        .checked_add(counters.quality_switches)
        .and_then(|v| v.checked_add(counters.decompositions))
        .ok_or(Error::Limit("escalation counters"))?;
    if counters.total_attempts == 0 || actions >= counters.total_attempts {
        return Err(Error::Protocol("escalation counters omit original attempt"));
    }
    if trigger.evidence.is_empty()
        || trigger.evidence.len() > 64
        || trigger.observations == 0
        || trigger.observations > 64
        || trigger
            .evidence
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != trigger.evidence.len()
    {
        return Err(Error::Protocol("escalation trigger evidence"));
    }
    if barrier.state != SchedulingState::Running {
        return blocked(Blocked::Scheduling);
    }
    if barrier.expected != barrier.current {
        return blocked(Blocked::Stale);
    }
    if barrier.now >= policy.deadline {
        return blocked(Blocked::Deadline);
    }
    if barrier.now < barrier.not_before {
        return blocked(Blocked::Backoff);
    }
    if counters.total_attempts >= policy.max_total_attempts {
        return blocked(Blocked::Limit);
    }
    match &trigger.kind {
        TriggerKind::TransportRetry {
            failure:
                crate::retry::Failure::Cancelled
                | crate::retry::Failure::Protocol
                | crate::retry::Failure::Capability
                | crate::retry::Failure::Authentication,
        } => return blocked(Blocked::NoTrigger),
        TriggerKind::FailedVerification | TriggerKind::InvalidToolOutput
            if trigger.observations < policy.minimum_repeated_failures =>
        {
            return blocked(Blocked::NoTrigger)
        }
        TriggerKind::RepeatedStrategy { before, after } => {
            if !hash(before) || !hash(after) {
                return Err(Error::Protocol("strategy progress fingerprints"));
            }
            if before != after || trigger.observations < policy.minimum_repeated_failures {
                return blocked(Blocked::NoTrigger);
            }
        }
        _ => {}
    }
    let (used, maximum) = match trigger.class() {
        Class::TransportRetry => (counters.transport_retries, policy.max_transport_retries),
        Class::QualitySwitch => (counters.quality_switches, policy.max_quality_switches),
        Class::Decomposition => (counters.decompositions, policy.max_decompositions),
    };
    if used >= maximum {
        return blocked(Blocked::Limit);
    }
    decision.validate()?;
    if routing::select(catalog, routing_policy, &decision.input)? != *decision {
        return Err(Error::Protocol(
            "escalation decision differs from qualified selection",
        ));
    }
    let current = &barrier.current;
    if decision.input.workspace != current.scope.workspace
        || decision.input.task != current.scope.task
        || decision.input.steering != current.steering
        || decision.input.input_revision != current.task_state
        || decision.input.now != barrier.now
        || ledger.scope.workspace != current.scope.workspace
        || ledger.scope.session != current.scope.session
        || ledger.scope.task != decision.input.root
    {
        return blocked(Blocked::Stale);
    }
    let Some(selected) = &decision.selected else {
        return blocked(Blocked::NoCandidate);
    };
    let snapshot = decision
        .selected_snapshot(catalog)?
        .ok_or(Error::Protocol("escalation snapshot"))?;
    if let Some(pin) = &routing_policy.pin {
        if *selected != pin.candidate && !pin.fallback_candidates.contains(selected) {
            return blocked(Blocked::StrictPin);
        }
    }
    if trigger.class() == Class::QualitySwitch && *selected == *previous {
        return blocked(Blocked::SameModel);
    }
    if trigger.class() == Class::TransportRetry && *selected != *previous {
        return blocked(Blocked::RetryModelChanged);
    }
    let estimate = decision
        .candidates
        .iter()
        .find(|c| c.identity == *selected)
        .and_then(|c| c.total_estimate.as_ref())
        .ok_or(Error::Protocol("escalation cost estimate"))?;
    if estimate.total.currency != ledger.currency {
        return Err(Error::Protocol("escalation currency"));
    }
    let held = [
        ledger.settled,
        ledger.active,
        ledger.unresolved,
        ledger.protected,
    ]
    .iter()
    .try_fold(0u64, |total, amount| total.checked_add(amount.get()))
    .ok_or(Error::Limit("escalation liabilities"))?;
    let remaining = ledger.cap.get().saturating_sub(held);
    let needed = estimate
        .total
        .micros
        .get()
        .checked_add(
            estimated_handoff
                .get()
                .saturating_sub(estimate.handoff.get()),
        )
        .ok_or(Error::Limit("escalation cost"))?;
    if ledger.overrun || held > ledger.cap.get() || needed > remaining {
        return blocked(Blocked::Budget);
    }
    let mut after = counters.clone();
    after.total_attempts += 1;
    match trigger.class() {
        Class::TransportRetry => after.transport_retries += 1,
        Class::QualitySwitch => after.quality_switches += 1,
        Class::Decomposition => after.decompositions += 1,
    }
    Ok(Outcome::Ready {
        plan: Plan {
            previous_attempt,
            previous: previous.clone(),
            selected: selected.clone(),
            selected_snapshot: snapshot.id.clone(),
            selected_compatibility: snapshot.compatibility.id.clone(),
            routing_decision: decision.id.clone(),
            routing_policy: routing_policy.id.clone(),
            trigger: trigger.clone(),
            before: counters.clone(),
            after,
            revisions: current.clone(),
            policy: policy.clone(),
            ledger_revision: ledger.revision,
            remaining: Micros::new(remaining),
            estimated_request: estimate.first_attempt,
            estimated_handoff: estimated_handoff.max(estimate.handoff),
            unresolved: ledger.unresolved,
        },
    })
}

/// Attribution to the existing portable packet and newly assembled request.
/// This record grants neither source access nor permission to replay effects.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handoff {
    pub packet_sha256: String,
    pub destination_manifest_sha256: String,
    pub destination_request_sha256: String,
    pub previous_attempt: AttemptId,
    pub routing_decision: String,
    pub original_artifacts: Vec<ArtifactId>,
}
pub fn bind_handoff(plan: &Plan, packet: &Packet, destination: &Sealed) -> Result<Handoff> {
    Packet::new(
        packet.manifest.clone(),
        packet.ledger.clone(),
        packet.current_state.clone(),
        packet.references.clone(),
        packet.discarded.clone(),
    )
    .map_err(|_| Error::Protocol("invalid portable handoff"))?;
    if packet.manifest.revisions != plan.revisions
        || destination.manifest.revisions != plan.revisions
        || packet.ledger.revision != plan.ledger_revision
        || packet.ledger.unresolved != plan.unresolved
        || packet.remaining != plan.remaining
        || destination.manifest.envelope.model != plan.selected.model
        || destination.manifest.envelope.catalog != plan.selected_snapshot
        || destination.manifest.envelope.compatibility != plan.selected_compatibility
        || digest_bytes(&canonical_bytes(&destination.manifest)?) != destination.manifest_digest()
        || digest_bytes(destination.body()) != destination.manifest.request_sha256
    {
        return Err(Error::Stale);
    }
    // New assembly may trim optional material, but required semantics cannot
    // disappear or silently change identity. Original references remain retained.
    for required in packet
        .manifest
        .included
        .iter()
        .filter(|part| part.mandatory)
    {
        if !destination
            .manifest
            .included
            .iter()
            .any(|part| part == required)
        {
            return Err(Error::Capability("handoff dropped required context"));
        }
    }
    if packet.references.len() > 4096 {
        return Err(Error::Limit("handoff references"));
    }
    Ok(Handoff {
        packet_sha256: digest_bytes(&canonical_bytes(packet)?),
        destination_manifest_sha256: destination.manifest_digest().to_string(),
        destination_request_sha256: destination.manifest.request_sha256.clone(),
        previous_attempt: plan.previous_attempt.clone(),
        routing_decision: plan.routing_decision.clone(),
        original_artifacts: packet
            .references
            .iter()
            .map(|r| r.spec.id.clone())
            .collect(),
    })
}
