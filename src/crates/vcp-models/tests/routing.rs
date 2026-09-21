// SPDX-License-Identifier: Apache-2.0
//! Synthetic observations exercise routing gates; they qualify no live endpoint.
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{
    accounting::{Money, RequestRole, Usage},
    *,
};
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
    candidate_with_byte_bound(model, group, quality, latency, input_price, true)
}
fn candidate_with_byte_bound(
    model: &str,
    group: Group,
    quality: u16,
    latency: u64,
    input_price: &str,
    byte_ceiling_qualified: bool,
) -> Candidate {
    let endpoint = "fixture/region";
    let compatibility = Compatibility {
        id: format!("synthetic-{model}"),
        model: model.into(),
        endpoint: endpoint.into(),
        qualified_at: Timestamp::new(1),
        valid_until: Timestamp::new(1000),
        responses_text_tools: true,
        byte_ceiling_qualified,
        provider_preferences_qualified: true,
        deny_data_collection: true,
        require_zdr: true,
        request_price_limit: "0.000010".into(),
        required_parameters: BTreeSet::from(["tools".into()]),
        qualified_reasoning_efforts: BTreeSet::new(),
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
        output_tokens: None,
        input_tokens: None,
        escalation_limits: None,
        reasoning_effort: None,
        retrieval_limits: None,
    }
    .seal()
    .unwrap()
}
fn estimate(candidate: &Candidate) -> CostEstimate {
    CostEstimate {candidate:candidate.identity.clone(),first_attempt:Usage {input:Units::new(100),output:Units::new(50),requests:Units::new(1),..Usage::default()},retries:Usage::default(),handoff:Usage::default(),support:Some(money(30)),children:Some(money(0)),verification:Some(money(100)),assumptions:vec!["One first attempt; bounded fixture has no retry, child or handoff; support and protected verification explicit".into()],evidence_refs:vec!["fixture://cost-assumptions-v1".into()]}
}

#[test]
fn optional_output_limit_preserves_legacy_bytes_and_rejects_zero() {
    let original = policy(&[]);
    let legacy = serde_json::to_value(&original).unwrap();
    for field in [
        "output_tokens",
        "input_tokens",
        "escalation_limits",
        "reasoning_effort",
        "retrieval_limits",
    ] {
        assert!(legacy.get(field).is_none(), "{field}");
    }
    let legacy_bytes = vcp_protocol::canonical_bytes(&legacy).unwrap();
    let reopened: Policy = serde_json::from_slice(&legacy_bytes).unwrap();
    reopened.validate().unwrap();
    assert_eq!(reopened.output_tokens, None);
    assert_eq!(reopened.id, original.id);
    assert_eq!(
        vcp_protocol::canonical_bytes(&reopened).unwrap(),
        legacy_bytes
    );
    let mut selected = reopened.clone();
    selected.output_tokens = Some(Units::new(256));
    let selected = selected.seal().unwrap();
    assert_ne!(selected.id, reopened.id);
    assert_eq!(selected.output_tokens, Some(Units::new(256)));
    let mut invalid = reopened;
    invalid.output_tokens = Some(Units::ZERO);
    assert!(invalid.seal().is_err());
}

#[test]
fn selected_resource_limits_validate_before_policy_publication() {
    use vcp_models::routing::{EscalationLimits, RetrievalLimits};
    let mut invalid = policy(&[]);
    invalid.input_tokens = Some(Units::ZERO);
    assert!(invalid.seal().is_err());
    assert!(EscalationLimits::default().validate().is_err());
    for limits in [
        EscalationLimits {
            max_transport_retries: Some(5),
            ..Default::default()
        },
        EscalationLimits {
            max_quality_switches: Some(9),
            ..Default::default()
        },
        EscalationLimits {
            max_total_attempts: Some(0),
            ..Default::default()
        },
        EscalationLimits {
            minimum_repeated_failures: Some(65),
            ..Default::default()
        },
    ] {
        assert!(limits.validate().is_err());
    }
    assert!(RetrievalLimits::default().validate().is_ok());
    for limits in [
        RetrievalLimits {
            results: 0,
            ..Default::default()
        },
        RetrievalLimits {
            tokens: Units::new(1),
            ..Default::default()
        },
        RetrievalLimits {
            bytes: vcp_domain::ByteCount::new(65537),
            ..Default::default()
        },
    ] {
        assert!(limits.validate().is_err());
    }
}

#[test]
fn selected_input_ceiling_blocks_selection_above_the_bound() {
    let (catalog, mut policy, mut input) = setup(vec![candidate(
        "fixture/input",
        Group::Low,
        9000,
        5,
        "0.000001",
    )]);
    policy.input_tokens = Some(Units::new(100));
    policy = policy.seal().unwrap();
    input.policy = policy.id.clone();
    input.input_tokens = Units::new(100);
    assert!(select(&catalog, &policy, &input)
        .unwrap()
        .selected
        .is_some());
    input.input_tokens = Units::new(101);
    assert!(select(&catalog, &policy, &input).is_err());
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

#[test]
fn unqualified_bytes_require_full_input_admission_without_inflating_expected_task_cost() {
    let entry = candidate_with_byte_bound("unqualified", Group::Low, 9000, 5, "0.000001", false);
    let (catalog, policy, mut input) = setup(vec![entry]);
    let snapshot = catalog.entries[0].snapshot.as_ref().unwrap();
    assert!(!snapshot.compatibility.byte_ceiling_qualified);
    assert_eq!(
        snapshot.reservation_input(Units::new(100)),
        Units::new(3000)
    );
    let decision = select(&catalog, &policy, &input).unwrap();
    assert!(decision.selected.is_some());
    // Expected cost is still 100 input + 100 output + 10 request + 130 support/
    // verification; it is not relabelled as the full-capacity reservation.
    assert_eq!(
        decision.candidates[0]
            .total_estimate
            .as_ref()
            .unwrap()
            .total
            .micros,
        Micros::new(340)
    );
    assert!(decision.candidates[0]
        .assumptions
        .iter()
        .any(|s| s.contains("unqualified")));
    // Each input/cache bound costs 3000, output 100, request 10, protection 100.
    input.available = money(9209);
    let denied = select(&catalog, &policy, &input).unwrap();
    assert!(denied.selected.is_none());
    excluded(&denied, "unqualified", Exclusion::Budget);
    input.available = money(9210);
    assert!(select(&catalog, &policy, &input)
        .unwrap()
        .selected
        .is_some());
    input.input_tokens = Units::new(3001);
    input.estimates[0].first_attempt.input = Units::new(3001);
    excluded(
        &select(&catalog, &policy, &input).unwrap(),
        "unqualified",
        Exclusion::ContextCapacity,
    );
}
fn excluded(decision: &RoutingDecision, model: &str, reason: Exclusion) {
    assert!(
        decision
            .candidates
            .iter()
            .find(|row| row.identity.model == model)
            .unwrap()
            .exclusions
            .contains(&reason),
        "{model} should report {reason:?}: {:?}",
        decision.candidates
    );
}

#[test]
fn explicit_reasoning_effort_filters_unqualified_endpoints_without_relaxing_a_pin() {
    use vcp_models::reasoning::Effort;
    let cheap = candidate("cheap", Group::Low, 9000, 5, "0.000001");
    let mut qualified = candidate("qualified", Group::Low, 9000, 5, "0.000002");
    let previous = qualified.snapshot.take().unwrap();
    let mut compatibility = previous.compatibility;
    compatibility.required_parameters.insert("reasoning".into());
    compatibility
        .qualified_reasoning_efforts
        .insert(Effort::Low);
    let raw = serde_json::to_vec(&serde_json::json!({"data":{"id":"qualified","endpoints":[{"tag":"fixture/region","status":0,"context_length":4000,"max_prompt_tokens":3000,"max_completion_tokens":1000,"supported_parameters":["tools","reasoning"],"pricing":{"prompt":"0.000002","completion":"0.000002","request":"0.000010"}}]}})).unwrap();
    qualified.snapshot = Some(
        Snapshot::from_endpoints(
            &raw,
            Timestamp::new(10),
            Timestamp::new(1000),
            compatibility,
        )
        .unwrap(),
    );
    let (catalog, mut policy, mut input) = setup(vec![cheap.clone(), qualified.clone()]);
    policy.reasoning_effort = Some(Effort::Low);
    policy = policy.seal().unwrap();
    input.policy = policy.id.clone();
    let decision = select(&catalog, &policy, &input).unwrap();
    assert_eq!(decision.selected, Some(qualified.identity));
    excluded(&decision, "cheap", Exclusion::UnsupportedReasoningEffort);
    policy.pin = Some(Pin {
        candidate: cheap.identity,
        fallback_candidates: BTreeSet::new(),
    });
    policy = policy.seal().unwrap();
    input.policy = policy.id.clone();
    assert_eq!(select(&catalog, &policy, &input).unwrap().selected, None);
}
#[test]
fn selected_output_bound_is_enforced_in_selection_and_current_revalidation() {
    let (catalog, mut policy, mut input) = setup(vec![candidate(
        "fixture/output",
        Group::Low,
        9000,
        5,
        "0.000001",
    )]);
    policy.output_tokens = Some(Units::new(50));
    policy = policy.seal().unwrap();
    input.policy = policy.id.clone();
    input.output_tokens = Units::new(50);
    let decision = select(&catalog, &policy, &input).unwrap();
    input.output_tokens = Units::new(51);
    assert!(select(&catalog, &policy, &input).is_err());
    input.output_tokens = Units::new(49);
    assert!(select(&catalog, &policy, &input).is_ok());
    policy.output_tokens = Some(Units::new(49));
    policy = policy.seal().unwrap();
    assert!(
        decision
            .validate_selected_at(
                &catalog,
                &policy,
                input.now,
                input.available.clone(),
                input.protected_verification,
            )
            .is_err(),
        "policy change cannot reuse a prepared decision"
    );
}

#[test]
fn cheap_low_quality_loses_and_all_failed_candidates_remain_inspectable() {
    let entries = vec![
        candidate("fixture/cheap", Group::Low, 7000, 5, "0.0000001"),
        candidate("fixture/good", Group::High, 9500, 20, "0.000001"),
    ];
    let (catalog, policy, input) = setup(entries);
    let decision = select(&catalog, &policy, &input).unwrap();
    assert_eq!(decision.selected.as_ref().unwrap().model, "fixture/good");
    excluded(&decision, "fixture/cheap", Exclusion::QualityFloor);
    assert_eq!(decision.candidates.len(), 2);
    assert_eq!(decision.immediate_reservation, None);
    let selected = decision
        .candidates
        .iter()
        .find(|row| row.identity.model == "fixture/good")
        .unwrap();
    let costs = selected.total_estimate.as_ref().unwrap();
    assert_eq!(costs.first_attempt, Micros::new(210));
    assert_eq!(costs.total.micros, Micros::new(340));
    assert_eq!(
        decision
            .selected_snapshot(&catalog)
            .unwrap()
            .unwrap()
            .compatibility
            .model,
        "fixture/good"
    );
    assert_eq!(decision, select(&catalog, &policy, &input).unwrap());
}
#[test]
fn expensive_retry_history_and_protected_completion_change_the_winner() {
    let (catalog, policy, mut input) = setup(vec![
        candidate("fixture/fast", Group::Low, 9000, 1, "0.0000001"),
        candidate("fixture/steady", Group::Medium, 9000, 50, "0.000001"),
    ]);
    input
        .estimates
        .iter_mut()
        .find(|e| e.candidate.model == "fixture/fast")
        .unwrap()
        .retries = Usage {
        input: Units::new(1000),
        output: Units::new(500),
        requests: Units::new(10),
        ..Usage::default()
    };
    let decision = select(&catalog, &policy, &input).unwrap();
    assert_eq!(decision.selected.as_ref().unwrap().model, "fixture/steady");
    input.protected_verification = Micros::new(99);
    let decision = select(&catalog, &policy, &input).unwrap();
    assert!(decision.selected.is_none());
    excluded(
        &decision,
        "fixture/steady",
        Exclusion::InsufficientVerificationReserve,
    );
    input.protected_verification = Micros::new(500);
    input.available = money(600);
    let decision = select(&catalog, &policy, &input).unwrap();
    excluded(&decision, "fixture/steady", Exclusion::Budget);
    assert!(decision.selected.is_none());
}
#[test]
fn availability_live_qualification_capabilities_and_provider_restrictions_fail_closed() {
    let base = candidate("fixture/base", Group::Medium, 9000, 10, "0.000001");
    type Mutation = Box<dyn Fn(&mut Candidate)>;
    let cases: Vec<(&str, Exclusion, Mutation)> = vec![
        (
            "removed",
            Exclusion::Unavailable,
            Box::new(|c| {
                c.availability = State::Unsupported;
                c.reasons.push("Removed in dated metadata".into());
            }),
        ),
        (
            "unknown",
            Exclusion::UnknownAvailability,
            Box::new(|c| {
                c.availability = State::Unknown;
                c.reasons.push("Unrecognized metadata".into());
            }),
        ),
        (
            "capability",
            Exclusion::UnknownCapability,
            Box::new(|c| {
                c.capabilities.clear();
            }),
        ),
        (
            "unsupported",
            Exclusion::UnsupportedCapability,
            Box::new(|c| {
                c.capabilities.insert("tools".into(), State::Unsupported);
            }),
        ),
        (
            "snapshot",
            Exclusion::MissingSnapshot,
            Box::new(|c| {
                c.snapshot = None;
            }),
        ),
        (
            "scripted",
            Exclusion::MissingLiveQualification,
            Box::new(|c| {
                c.compatibility[0].kind = EvidenceKind::Scripted;
            }),
        ),
        (
            "renamed-probe",
            Exclusion::MissingLiveQualification,
            Box::new(|c| {
                c.compatibility[0].compatibility = "qualification-for-old-alias".into();
            }),
        ),
    ];
    for (label, reason, change) in cases {
        let mut entry = base.clone();
        change(&mut entry);
        let (catalog, policy, input) = setup(vec![entry]);
        let decision = select(&catalog, &policy, &input).unwrap();
        assert!(decision.selected.is_none(), "{label}");
        excluded(&decision, "fixture/base", reason);
    }
    let (catalog, mut policy, mut input) = setup(vec![base]);
    policy.allowed_endpoints.clear();
    policy = policy.seal().unwrap();
    input.policy = policy.id.clone();
    let decision = select(&catalog, &policy, &input).unwrap();
    excluded(&decision, "fixture/base", Exclusion::ProviderDenied);
    assert!(decision.selected.is_none());
}
#[test]
fn strict_pin_cannot_silently_expand_and_explicit_fallback_is_recorded() {
    let (catalog, mut policy, mut input) = setup(vec![
        candidate("fixture/cheap", Group::Low, 9000, 5, "0.0000001"),
        candidate("fixture/pinned", Group::High, 7000, 10, "0.000001"),
    ]);
    let pin = catalog
        .entries
        .iter()
        .find(|c| c.identity.model == "fixture/pinned")
        .unwrap()
        .identity
        .clone();
    policy.pin = Some(Pin {
        candidate: pin.clone(),
        fallback_candidates: BTreeSet::new(),
    });
    policy = policy.seal().unwrap();
    input.policy = policy.id.clone();
    let decision = select(&catalog, &policy, &input).unwrap();
    assert!(decision.selected.is_none());
    excluded(&decision, "fixture/cheap", Exclusion::PinRestricted);
    policy
        .pin
        .as_mut()
        .unwrap()
        .fallback_candidates
        .insert(catalog.entries[0].identity.clone());
    policy = policy.seal().unwrap();
    input.policy = policy.id.clone();
    let decision = select(&catalog, &policy, &input).unwrap();
    assert_eq!(decision.selected.as_ref().unwrap().model, "fixture/cheap");
    assert_eq!(decision.fallback_from, Some(pin));
    policy.pin.as_mut().unwrap().candidate = ModelEndpoint {
        model: "fixture/removed-alias".into(),
        endpoint: "fixture/region".into(),
    };
    policy.pin.as_mut().unwrap().fallback_candidates.clear();
    policy = policy.seal().unwrap();
    input.policy = policy.id.clone();
    let decision = select(&catalog, &policy, &input).unwrap();
    assert!(decision.selected.is_none());
    excluded(&decision, "fixture/removed-alias", Exclusion::Unavailable);
}
#[test]
fn valid_pin_wins_even_when_explicit_fallback_is_cheaper() {
    let (catalog, mut policy, mut input) = setup(vec![
        candidate("fixture/cheap", Group::Low, 9000, 5, "0.0000001"),
        candidate("fixture/pinned", Group::High, 9000, 10, "0.000001"),
    ]);
    policy.pin = Some(Pin {
        candidate: catalog.entries[1].identity.clone(),
        fallback_candidates: BTreeSet::from([catalog.entries[0].identity.clone()]),
    });
    policy = policy.seal().unwrap();
    input.policy = policy.id.clone();
    let decision = select(&catalog, &policy, &input).unwrap();
    assert_eq!(decision.selected.as_ref().unwrap().model, "fixture/pinned");
    excluded(&decision, "fixture/cheap", Exclusion::PinPreferred);
    assert_eq!(decision.fallback_from, None);
}
#[test]
fn missing_cost_is_not_free_and_integer_rounding_never_underestimates() {
    let (catalog, policy, mut input) = setup(vec![candidate(
        "fixture/tiny",
        Group::Low,
        9000,
        5,
        "0.0000000000001",
    )]);
    let bounded = Usage {
        input: Units::new(100),
        output: Units::new(1),
        requests: Units::new(1),
        ..Usage::default()
    };
    input.output_tokens = Units::new(1);
    input.estimates[0].first_attempt = bounded;
    let decision = select(&catalog, &policy, &input).unwrap();
    assert_eq!(
        decision.candidates[0]
            .total_estimate
            .as_ref()
            .unwrap()
            .first_attempt,
        Micros::new(13)
    );
    input.estimates[0].support = None;
    let decision = select(&catalog, &policy, &input).unwrap();
    assert!(decision.selected.is_none());
    excluded(&decision, "fixture/tiny", Exclusion::UnknownCost);
    input.estimates.clear();
    excluded(
        &select(&catalog, &policy, &input).unwrap(),
        "fixture/tiny",
        Exclusion::MissingCostEstimate,
    );
}
#[test]
fn broader_cohort_requires_permission_and_never_hides_quality_failure() {
    let mut entry = candidate("fixture/model", Group::Medium, 9000, 10, "0.000001");
    entry.memberships[0].roles[0].task_class = "general".into();
    let (catalog, mut policy, mut input) = setup(vec![entry]);
    excluded(
        &select(&catalog, &policy, &input).unwrap(),
        "fixture/model",
        Exclusion::MissingRoleEvidence,
    );
    policy.broader_task_class = Some("general".into());
    policy = policy.seal().unwrap();
    input.policy = policy.id.clone();
    let decision = select(&catalog, &policy, &input).unwrap();
    assert_eq!(
        decision.candidates[0].broader_cohort_used.as_deref(),
        Some("general")
    );
    assert!(decision.selected.is_some());
    policy.quality_floor_bps = 9500;
    policy = policy.seal().unwrap();
    input.policy = policy.id.clone();
    excluded(
        &select(&catalog, &policy, &input).unwrap(),
        "fixture/model",
        Exclusion::QualityFloor,
    );
}
#[test]
fn profiles_and_support_roles_use_explicit_distinct_ordering() {
    let mut low = candidate("fixture/economical", Group::Low, 8500, 5, "0.0000001");
    let mut high = candidate("fixture/strong", Group::Frontier, 9500, 20, "0.000001");
    for entry in [&mut low, &mut high] {
        let mut helper = entry.memberships[0].roles[0].clone();
        helper.id.push_str("-helper");
        helper.role = RequestRole::Helper;
        entry.memberships[0].roles.push(helper);
    }
    let (catalog, mut policy, mut input) = setup(vec![low, high]);
    assert_eq!(
        select(&catalog, &policy, &input)
            .unwrap()
            .selected
            .as_ref()
            .unwrap()
            .model,
        "fixture/economical"
    );
    policy.profile = Profile::High;
    policy.ordering = vec![
        Preference::Capability,
        Preference::Quality,
        Preference::TotalCost,
        Preference::Latency,
    ];
    policy = policy.seal().unwrap();
    input.policy = policy.id.clone();
    assert_eq!(
        select(&catalog, &policy, &input)
            .unwrap()
            .selected
            .as_ref()
            .unwrap()
            .model,
        "fixture/strong"
    );
    input.role = RequestRole::Helper;
    let decision = select(&catalog, &policy, &input).unwrap();
    assert_eq!(
        decision.selected.as_ref().unwrap().model,
        "fixture/economical"
    );
    assert_eq!(decision.ordering[0], Preference::TotalCost);
}
#[test]
fn immutable_catalog_reopens_without_rebinding_an_earlier_decision() {
    let (catalog, policy, input) = setup(vec![candidate(
        "fixture/model",
        Group::Medium,
        9000,
        10,
        "0.000001",
    )]);
    let decision = select(&catalog, &policy, &input).unwrap();
    let bytes = serde_json::to_vec(&catalog).unwrap();
    let restored: CatalogRevision = serde_json::from_slice(&bytes).unwrap();
    restored.validate().unwrap();
    assert_eq!(catalog, restored);
    let mut removed = catalog.entries.clone();
    removed[0].availability = State::Unsupported;
    removed[0].reasons.push("Endpoint removed".into());
    let refreshed =
        CatalogRevision::create(Some(catalog.id.clone()), Timestamp::new(25), None, removed)
            .unwrap();
    assert!(select(&refreshed, &policy, &input).is_err());
    assert!(decision.selected_snapshot(&refreshed).is_err());
    assert!(decision.selected_snapshot(&catalog).unwrap().is_some());
    let mut changed = restored;
    changed.entries[0].memberships[0].roles[0].quality_bps = 10_000;
    assert!(changed.validate().is_err());
    let mut alias = catalog.entries[0].clone();
    alias.identity.model = "fixture/renamed".into();
    assert!(CatalogRevision::create(None, Timestamp::new(20), None, vec![alias]).is_err());
}
#[test]
fn stale_or_contradictory_evidence_cannot_select_and_ties_are_identity_stable() {
    let first = candidate("fixture/a", Group::Medium, 9000, 10, "0.000001");
    let second = candidate("fixture/b", Group::Medium, 9000, 10, "0.000001");
    let (catalog, policy, mut input) = setup(vec![second, first]);
    assert_eq!(
        select(&catalog, &policy, &input)
            .unwrap()
            .selected
            .as_ref()
            .unwrap()
            .model,
        "fixture/a"
    );
    input.now = Timestamp::new(1000);
    excluded(
        &select(&catalog, &policy, &input).unwrap(),
        "fixture/a",
        Exclusion::StaleSnapshot,
    );
    let mut entry = catalog.entries[0].clone();
    let mut contradictory = entry.memberships[0].clone();
    contradictory.version.push_str("-research");
    contradictory.group = Group::Low;
    contradictory.roles[0].id.push_str("-research");
    contradictory.roles[0].kind = EvidenceKind::Research;
    entry.memberships.push(contradictory);
    let (catalog, policy, input) = setup(vec![entry]);
    excluded(
        &select(&catalog, &policy, &input).unwrap(),
        "fixture/a",
        Exclusion::ContradictoryMembership,
    );
    assert!(select(&catalog, &policy, &input)
        .unwrap()
        .selected
        .is_none());
}
#[test]
fn replay_with_reduced_balance_stops_without_minting_a_reservation() {
    let (catalog, policy, mut input) = setup(vec![candidate(
        "fixture/model",
        Group::Medium,
        9000,
        10,
        "0.000001",
    )]);
    let decision = select(&catalog, &policy, &input).unwrap();
    assert!(decision.selected.is_some());
    input.available = money(339);
    let retry = select(&catalog, &policy, &input).unwrap();
    assert!(retry.selected.is_none());
    assert_eq!(retry.immediate_reservation, None);
    excluded(&retry, "fixture/model", Exclusion::Budget);
    assert_ne!(decision.id, retry.id);
}

#[test]
fn underestimated_output_cannot_win_by_omitting_requested_completion_cost() {
    let (catalog, policy, mut input) = setup(vec![
        candidate("fixture/cheap", Group::Low, 9000, 5, "0.0000001"),
        candidate("fixture/honest", Group::Medium, 9000, 10, "0.000001"),
    ]);
    input
        .estimates
        .iter_mut()
        .find(|e| e.candidate.model == "fixture/cheap")
        .unwrap()
        .first_attempt
        .output = Units::ZERO;
    let decision = select(&catalog, &policy, &input).unwrap();
    excluded(&decision, "fixture/cheap", Exclusion::InvalidCost);
    assert_eq!(decision.selected.as_ref().unwrap().model, "fixture/honest");
    input
        .estimates
        .iter_mut()
        .for_each(|e| e.first_attempt.output = Units::new(49));
    let rejected = select(&catalog, &policy, &input).unwrap();
    assert!(rejected.selected.is_none());
    assert!(rejected
        .candidates
        .iter()
        .all(|c| c.exclusions.contains(&Exclusion::InvalidCost)));
}

#[test]
fn excluded_strict_pin_cannot_fall_back_to_an_otherwise_eligible_endpoint() {
    let (catalog, mut policy, mut input) = setup(vec![
        candidate("fixture/pinned", Group::Low, 9000, 5, "0.0000001"),
        candidate("fixture/alternative", Group::High, 9500, 10, "0.000001"),
    ]);
    let pinned = catalog
        .entries
        .iter()
        .find(|c| c.identity.model == "fixture/pinned")
        .unwrap()
        .identity
        .clone();
    policy.pin = Some(Pin {
        candidate: pinned.clone(),
        fallback_candidates: BTreeSet::new(),
    });
    policy = policy.seal().unwrap();
    input.policy = policy.id.clone();
    assert_eq!(
        select(&catalog, &policy, &input).unwrap().selected,
        Some(pinned.clone())
    );
    input.excluded.insert(pinned);
    let decision = select(&catalog, &policy, &input).unwrap();
    assert_eq!(decision.selected, None);
    assert_eq!(decision.fallback_from, None);
    excluded(&decision, "fixture/pinned", Exclusion::PreviouslyFailed);
    excluded(&decision, "fixture/alternative", Exclusion::PinRestricted);
}

#[test]
fn retry_pin_never_changes_endpoint_even_when_alternative_is_better_or_affordable() {
    let (catalog, policy, mut input) = setup(vec![
        candidate("fixture/prior", Group::High, 9500, 20, "0.000003"),
        candidate("fixture/cheap", Group::Low, 9000, 5, "0.0000001"),
    ]);
    let prior = catalog
        .entries
        .iter()
        .find(|c| c.identity.model == "fixture/prior")
        .unwrap()
        .identity
        .clone();
    input.retry_pin = Some(prior.clone());
    let decision = select(&catalog, &policy, &input).unwrap();
    assert_eq!(decision.selected, Some(prior));
    excluded(&decision, "fixture/cheap", Exclusion::RetryPinned);
    input.available = money(500);
    let decision = select(&catalog, &policy, &input).unwrap();
    assert!(decision.selected.is_none());
    excluded(&decision, "fixture/cheap", Exclusion::RetryPinned);
}

#[test]
fn admitted_selection_rechecks_role_expiry_without_rewriting_captured_decision() {
    let mut entry = candidate("fixture/good", Group::High, 9500, 20, "0.000001");
    entry.memberships[0].roles[0].valid_until = Timestamp::new(31);
    let (catalog, policy, input) = setup(vec![entry]);
    let decision = select(&catalog, &policy, &input).unwrap();
    let original = decision.clone();
    assert!(decision
        .validate_selected_at(
            &catalog,
            &policy,
            Timestamp::new(30),
            input.available.clone(),
            input.protected_verification
        )
        .is_ok());
    assert!(decision
        .validate_selected_at(
            &catalog,
            &policy,
            Timestamp::new(31),
            input.available.clone(),
            input.protected_verification
        )
        .is_err());
    assert_eq!(decision, original);
    assert!(
        decision
            .selected_snapshot(&catalog)
            .unwrap()
            .unwrap()
            .current(Timestamp::new(31))
            .is_ok(),
        "role evidence can expire before provider snapshot"
    );
}
