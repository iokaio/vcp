// SPDX-License-Identifier: Apache-2.0
//! Synthetic escalation evidence only; no fixture qualifies a real endpoint.
use std::collections::{BTreeMap, BTreeSet};
use vcp_context::manifest::Revisions;
use vcp_domain::{accounting::Ledger, workspace::Scope};
use vcp_domain::{
    accounting::{Money, RequestRole, Usage},
    *,
};
use vcp_models::escalation;
use vcp_models::{
    catalog::{Compatibility, Snapshot},
    routing::*,
};

fn money(amount: u64) -> Money {
    Money {
        currency: "USD".to_string().try_into().unwrap(),
        micros: Micros::new(amount),
    }
}
fn provenance() -> Vec<Provenance> {
    vec![Provenance {
        source: "fixture://synthetic-routing-observation".into(),
        sha256: "a".repeat(64),
        observed_at: Timestamp::new(10),
        effective_at: Some(Timestamp::new(9)),
        limitations: vec!["Synthetic test labels; no actual model qualification".into()],
    }]
}
fn candidate(
    model: &str,
    group: Group,
    quality: u16,
    latency: u64,
    input_price: &str,
) -> Candidate {
    let endpoint = "fixture/region";
    let compatibility = Compatibility {
        id: format!("synthetic-{model}"),
        model: model.into(),
        endpoint: endpoint.into(),
        qualified_at: Timestamp::new(1),
        valid_until: Timestamp::new(1000),
        responses_text_tools: true,
        byte_ceiling_qualified: true,
        provider_preferences_qualified: true,
        deny_data_collection: true,
        require_zdr: true,
        request_price_limit: "0.000010".into(),
        required_parameters: BTreeSet::from(["tools".into()]),
    };
    let raw=serde_json::to_vec(&serde_json::json!({"data":{"id":model,"endpoints":[{"tag":endpoint,"status":0,"context_length":4000,"max_prompt_tokens":3000,"max_completion_tokens":1000,"supported_parameters":["tools"],"pricing":{"prompt":input_price,"completion":"0.000002","request":"0.000010"}}]}})).unwrap();
    let snapshot = Snapshot::from_endpoints(
        &raw,
        Timestamp::new(10),
        Timestamp::new(1000),
        compatibility,
    )
    .unwrap();
    let observation = CompatibilityObservation {
        id: format!("probe-{model}"),
        compatibility: snapshot.compatibility.id.clone(),
        kind: EvidenceKind::Live,
        state: State::Supported,
        observed_at: Timestamp::new(10),
        valid_until: Timestamp::new(1000),
        provenance: provenance(),
    };
    Candidate {
        identity: ModelEndpoint {
            model: model.into(),
            endpoint: endpoint.into(),
        },
        availability: State::Supported,
        reasons: vec![],
        provenance: provenance(),
        capabilities: BTreeMap::from([("tools".into(), State::Supported)]),
        snapshot: Some(snapshot),
        compatibility: vec![observation],
        memberships: vec![GroupMembership {
            version: format!("membership-{model}"),
            group,
            roles: vec![RoleEvidence {
                id: format!("quality-{model}"),
                role: RequestRole::Main,
                task_class: "coding".into(),
                kind: EvidenceKind::Live,
                observed_at: Timestamp::new(10),
                valid_until: Timestamp::new(1000),
                samples: 100,
                quality_bps: quality,
                latency_p50_ms: latency / 2,
                latency_p95_ms: latency,
                usage_p50: None,
                usage_p95: None,
                provenance: provenance(),
            }],
        }],
    }
}
fn policy(entries: &[Candidate]) -> Policy {
    Policy {
        schema_version: 1,
        id: String::new(),
        parent: None,
        profile: Profile::Low,
        allowed_models: entries
            .iter()
            .map(|entry| entry.identity.model.clone())
            .collect(),
        allowed_endpoints: entries
            .iter()
            .map(|entry| entry.identity.endpoint.clone())
            .collect(),
        allowed_groups: BTreeSet::from([Group::Frontier, Group::High, Group::Medium, Group::Low]),
        quality_floor_bps: 8000,
        minimum_samples: 20,
        maximum_evidence_age_ms: 1000,
        deny_data_collection: true,
        require_zdr: true,
        ordering: vec![
            Preference::TotalCost,
            Preference::Latency,
            Preference::Quality,
            Preference::Capability,
        ],
        pin: None,
        broader_task_class: None,
    }
    .seal()
    .unwrap()
}
fn estimate(candidate: &Candidate) -> CostEstimate {
    CostEstimate {candidate:candidate.identity.clone(),first_attempt:Usage {input:Units::new(100),output:Units::new(50),requests:Units::new(1),..Usage::default()},retries:Usage::default(),handoff:Usage::default(),support:Some(money(30)),children:Some(money(0)),verification:Some(money(100)),assumptions:vec!["One first attempt; bounded fixture has no retry, child or handoff; support and protected verification explicit".into()],evidence_refs:vec!["fixture://cost-assumptions-v1".into()]}
}
fn input(catalog: &CatalogRevision, policy: &Policy) -> RoutingInput {
    RoutingInput {
        retry_pin: None,
        excluded: Default::default(),
        workspace: WorkspaceId::parse("workspace").unwrap(),
        root: TaskId::parse("root").unwrap(),
        task: TaskId::parse("root").unwrap(),
        input_revision: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        input_digest: "b".repeat(64),
        catalog: catalog.id.clone(),
        policy: policy.id.clone(),
        role: RequestRole::Main,
        task_class: "coding".into(),
        now: Timestamp::new(30),
        required_capabilities: BTreeSet::from(["tools".into()]),
        input_tokens: Units::new(100),
        output_tokens: Units::new(50),
        available: money(10_000),
        protected_verification: Micros::new(100),
        estimates: catalog.entries.iter().map(estimate).collect(),
    }
}
fn setup(entries: Vec<Candidate>) -> (CatalogRevision, Policy, RoutingInput) {
    let policy = policy(&entries);
    let catalog = CatalogRevision::create(None, Timestamp::new(20), None, entries).unwrap();
    let input = input(&catalog, &policy);
    (catalog, policy, input)
}

struct Fixture {
    catalog: CatalogRevision,
    routing: Policy,
    decision: RoutingDecision,
    policy: escalation::Policy,
    counters: escalation::Counters,
    trigger: escalation::Trigger,
    barrier: escalation::Barrier,
    ledger: Ledger,
    previous: ModelEndpoint,
}
impl Fixture {
    fn new() -> Self {
        let (catalog, routing, input) = setup(vec![candidate(
            "fixture/good",
            Group::High,
            9500,
            20,
            "0.000001",
        )]);
        let revisions = Revisions {
            scope: Scope {
                workspace: input.workspace.clone(),
                session: SessionId::parse("session").unwrap(),
                task: input.task.clone(),
            },
            steering: input.steering,
            policy: PolicyRevision::ZERO,
            authority: AuthorityRevision::ZERO,
            deletion: DeletionEpoch::ZERO,
            binding: Revision::ZERO,
            instructions: Revision::ZERO,
            tools: Revision::ZERO,
            skills: Revision::ZERO,
            memory: Revision::ZERO,
            task_state: input.input_revision,
        };
        let ledger = Ledger {
            schema_version: 1,
            scope: revisions.scope.clone(),
            revision: Revision::ZERO,
            policy: revisions.policy,
            currency: money(0).currency,
            cap: Micros::new(10_000),
            protected: Micros::new(100),
            settled: Micros::new(500),
            active: Micros::new(200),
            unresolved: Micros::new(300),
            allocations: BTreeMap::new(),
            daily: None,
            overrun: false,
        };
        Self {
            decision: select(&catalog, &routing, &input).unwrap(),
            catalog,
            routing,
            policy: escalation::Policy {
                max_transport_retries: 2,
                max_quality_switches: 2,
                max_decompositions: 1,
                max_total_attempts: 5,
                minimum_repeated_failures: 2,
                deadline: Timestamp::new(1000),
            },
            counters: escalation::Counters {
                total_attempts: 1,
                ..Default::default()
            },
            trigger: escalation::Trigger {
                kind: escalation::TriggerKind::FailedVerification,
                evidence: vec![ArtifactId::parse("failed-check").unwrap()],
                observations: 2,
            },
            barrier: escalation::Barrier {
                expected: revisions.clone(),
                current: revisions,
                state: escalation::SchedulingState::Running,
                now: Timestamp::new(30),
                not_before: Timestamp::new(20),
            },
            ledger,
            previous: ModelEndpoint {
                model: "fixture/prior".into(),
                endpoint: "fixture/region".into(),
            },
        }
    }
    fn evaluate(&self) -> vcp_models::Result<escalation::Outcome> {
        escalation::evaluate(
            &self.policy,
            &self.counters,
            &self.trigger,
            &self.barrier,
            AttemptId::parse("previous").unwrap(),
            &self.previous,
            &self.decision,
            &self.catalog,
            &self.routing,
            &self.ledger,
            Micros::new(40),
        )
    }
    fn blocked(&self, reason: escalation::Blocked) {
        assert_eq!(
            self.evaluate().unwrap(),
            escalation::Outcome::Blocked { reason }
        );
    }
}

#[test]
fn counters_survive_serialization_and_stop_without_resetting_prior_liability() {
    let mut fixture = Fixture::new();
    let escalation::Outcome::Ready { plan } = fixture.evaluate().unwrap() else {
        panic!("qualified switch")
    };
    assert_eq!(plan.before.total_attempts, 1);
    assert_eq!(plan.after.total_attempts, 2);
    assert_eq!(plan.after.quality_switches, 1);
    assert_eq!(plan.after.transport_retries, 0);
    assert_eq!(plan.unresolved, Micros::new(300));
    assert_eq!(plan.remaining, Micros::new(8900));
    assert_eq!(plan.estimated_handoff, Micros::new(40));
    let persisted = serde_json::to_vec(&plan).unwrap();
    let recovered: escalation::Plan = serde_json::from_slice(&persisted).unwrap();
    assert_eq!(plan, recovered);
    fixture.counters = recovered.after;
    let escalation::Outcome::Ready { plan } = fixture.evaluate().unwrap() else {
        panic!("second switch")
    };
    fixture.counters = plan.after;
    fixture.blocked(escalation::Blocked::Limit);
}

#[test]
fn retries_do_not_spend_quality_allowance_or_switch_models() {
    let mut fixture = Fixture::new();
    fixture.trigger.kind = escalation::TriggerKind::TransportRetry {
        failure: vcp_models::retry::Failure::Transient,
    };
    fixture.blocked(escalation::Blocked::RetryModelChanged);
    fixture.previous = fixture.decision.selected.clone().unwrap();
    let escalation::Outcome::Ready { plan } = fixture.evaluate().unwrap() else {
        panic!("same-model retry")
    };
    assert_eq!(plan.after.transport_retries, 1);
    assert_eq!(plan.after.quality_switches, 0);
    fixture.trigger.kind = escalation::TriggerKind::TransportRetry {
        failure: vcp_models::retry::Failure::Authentication,
    };
    fixture.blocked(escalation::Blocked::NoTrigger);
    fixture.trigger.kind = escalation::TriggerKind::UnsupportedCapability;
    fixture.blocked(escalation::Blocked::SameModel);
}

#[test]
fn repeated_progress_is_not_a_stall_and_unsupported_capability_needs_evidence() {
    let mut fixture = Fixture::new();
    fixture.trigger.observations = 1;
    fixture.blocked(escalation::Blocked::NoTrigger);
    fixture.trigger.observations = 3;
    fixture.trigger.kind = escalation::TriggerKind::RepeatedStrategy {
        before: "a".repeat(64),
        after: "b".repeat(64),
    };
    fixture.blocked(escalation::Blocked::NoTrigger);
    fixture.trigger.kind = escalation::TriggerKind::RepeatedStrategy {
        before: "a".repeat(64),
        after: "a".repeat(64),
    };
    assert!(matches!(
        fixture.evaluate().unwrap(),
        escalation::Outcome::Ready { .. }
    ));
    fixture.trigger.kind = escalation::TriggerKind::UnsupportedCapability;
    fixture.trigger.evidence.clear();
    assert!(fixture.evaluate().is_err());
}

#[test]
fn pause_steering_authority_backoff_and_deadline_block_before_admission() {
    for state in [
        escalation::SchedulingState::Paused,
        escalation::SchedulingState::WaitingForInput,
        escalation::SchedulingState::Cancelled,
        escalation::SchedulingState::Terminal,
    ] {
        let mut fixture = Fixture::new();
        fixture.barrier.state = state;
        fixture.blocked(escalation::Blocked::Scheduling);
    }
    let mut fixture = Fixture::new();
    fixture.barrier.current.steering = SteeringRevision::new(1);
    fixture.blocked(escalation::Blocked::Stale);
    let mut fixture = Fixture::new();
    fixture.barrier.current.authority = AuthorityRevision::new(1);
    fixture.blocked(escalation::Blocked::Stale);
    let mut fixture = Fixture::new();
    fixture.barrier.not_before = Timestamp::new(31);
    fixture.blocked(escalation::Blocked::Backoff);
    let mut fixture = Fixture::new();
    fixture.policy.deadline = Timestamp::new(30);
    fixture.blocked(escalation::Blocked::Deadline);
}

#[test]
fn combined_root_limit_and_unresolved_spend_cannot_be_bypassed() {
    let mut fixture = Fixture::new();
    fixture.policy.max_total_attempts = 1;
    fixture.blocked(escalation::Blocked::Limit);
    let mut fixture = Fixture::new();
    fixture.ledger.unresolved = Micros::new(8900);
    fixture.blocked(escalation::Blocked::Budget);
    let mut fixture = Fixture::new();
    fixture.counters.total_attempts = 0;
    assert!(fixture.evaluate().is_err());
    let mut fixture = Fixture::new();
    fixture.policy.max_quality_switches = 9;
    assert!(fixture.evaluate().is_err());
}

#[test]
fn strict_pin_and_tampered_selection_never_gain_fallback_permission() {
    let mut fixture = Fixture::new();
    fixture.routing.pin = Some(Pin {
        candidate: fixture.previous.clone(),
        fallback_candidates: BTreeSet::new(),
    });
    fixture.routing = fixture.routing.seal().unwrap();
    let mut input = fixture.decision.input.clone();
    input.policy = fixture.routing.id.clone();
    fixture.decision = select(&fixture.catalog, &fixture.routing, &input).unwrap();
    fixture.blocked(escalation::Blocked::NoCandidate);
    let mut fixture = Fixture::new();
    fixture.decision.selected = Some(fixture.previous.clone());
    assert!(fixture.evaluate().is_err());
}

#[test]
fn handoff_reassembles_smaller_context_and_keeps_constraints_and_originals() {
    use vcp_context::{
        handoff::Packet,
        manifest::{Content, Envelope, Kind, Part, Trust},
        selection::{assemble, Utf8ByteCeiling},
    };
    use vcp_domain::artifact::*;
    let fixture = Fixture::new();
    let escalation::Outcome::Ready { plan } = fixture.evaluate().unwrap() else {
        panic!("switch")
    };
    let mut references = Vec::new();
    let parts: Vec<_> = [
        (
            "operating",
            Kind::Operating,
            Trust::Operating,
            "Respect authority.",
        ),
        (
            "objective",
            Kind::Objective,
            Trust::User,
            "Preserve current edits.",
        ),
        (
            "state",
            Kind::TaskState,
            Trust::Observed,
            "Prior effect unresolved; do not repeat.",
        ),
        (
            "constraint",
            Kind::Constraint,
            Trust::User,
            "Never publish.",
        ),
    ]
    .into_iter()
    .map(|(id, kind, trust, text)| {
        let source = ArtifactDescriptor {
            spec: ArtifactSpec {
                id: ArtifactId::new(),
                scope: plan.revisions.scope.clone(),
                media_type: "text/plain".into(),
                schema: "fixture/1".into(),
                source: "synthetic".into(),
                channel: Channel::Evidence,
                retention: "history".into(),
                omissions: vec![],
            },
            state: CaptureState::Complete,
            length: ByteCount::new(text.len() as u64),
            sha256: vcp_protocol::digest_bytes(text.as_bytes()),
            retained: vec![Range {
                start: ByteCount::ZERO,
                end: ByteCount::new(text.len() as u64),
            }],
        };
        let part = Part::captured_text(
            id.into(),
            kind,
            trust,
            &source,
            text.as_bytes(),
            true,
            0,
            "canonical fixture".into(),
        )
        .unwrap();
        references.push(source);
        part
    })
    .collect();
    let envelope = Envelope {
        model: plan.selected.model.clone(),
        catalog: plan.selected_snapshot.clone(),
        compatibility: plan.selected_compatibility.clone(),
        context: Units::new(20_000),
        output: Units::new(100),
        overhead: Units::new(20),
        margin: Units::new(20),
        supports_tools: true,
        preserves_trust: true,
    };
    let encode = |parts: &[Part],
                  envelope: &Envelope,
                  tools: &serde_json::Value|
     -> vcp_context::manifest::Result<Vec<u8>> {
        Ok(vcp_protocol::canonical_bytes(
            &serde_json::json!({"parts":parts.iter().map(|p|&p.content).collect::<Vec<&Content>>(),"model":envelope.model,"tools":tools}),
        )?)
    };
    let original = assemble(
        parts,
        plan.revisions.clone(),
        envelope.clone(),
        serde_json::json!([]),
        vec![],
        &Utf8ByteCeiling,
        encode,
    )
    .unwrap();
    let packet = Packet::new(
        original.manifest,
        fixture.ledger.clone(),
        serde_json::json!({"unresolved_effect":"preserved"}),
        references,
        vec![],
    )
    .unwrap();
    let mut smaller = envelope.clone();
    smaller.context = Units::new(2000);
    let read = |id: &ArtifactId| {
        packet
            .manifest
            .included
            .iter()
            .find(|p| p.artifact == *id)
            .unwrap()
            .content
            .bytes()
    };
    let next = packet
        .reassemble(
            &plan.revisions,
            &fixture.ledger,
            smaller,
            serde_json::json!([{"name":"revised_schema"}]),
            &Utf8ByteCeiling,
            read,
            encode,
        )
        .unwrap();
    let bound = escalation::bind_handoff(&plan, &packet, &next).unwrap();
    assert_eq!(bound.original_artifacts.len(), 4);
    assert_eq!(bound.previous_attempt, plan.previous_attempt);
    assert_ne!(packet.manifest.schemas_sha256, next.manifest.schemas_sha256);
    let mut too_small = envelope;
    too_small.context = Units::new(200);
    assert!(packet
        .reassemble(
            &plan.revisions,
            &fixture.ledger,
            too_small,
            serde_json::json!([]),
            &Utf8ByteCeiling,
            read,
            encode
        )
        .is_err());
    let trimmed = assemble(
        packet
            .manifest
            .included
            .iter()
            .filter(|p| p.kind != Kind::Constraint)
            .cloned()
            .collect(),
        plan.revisions.clone(),
        next.manifest.envelope.clone(),
        serde_json::json!([]),
        vec![],
        &Utf8ByteCeiling,
        encode,
    )
    .unwrap();
    assert!(escalation::bind_handoff(&plan, &packet, &trimmed).is_err());
}
