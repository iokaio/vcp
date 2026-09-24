// SPDX-License-Identifier: Apache-2.0
//! Synthetic routing evidence for a private loopback editor qualification only.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
use vcp_domain::{
    accounting::{Money, RequestRole, Usage},
    Micros, Timestamp, Units,
};
use vcp_lifecycle::foundation::routing::Configuration;
use vcp_models::{catalog::Snapshot, routing::*};

pub(super) fn configure(path: &Path) {
    let mut profile: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let snapshot: Snapshot = serde_json::from_value(profile["provider"].clone()).unwrap();
    let raw = fs::read_to_string(profile["catalog"].as_str().unwrap()).unwrap();
    let now = Timestamp::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
    );
    let expiry = Timestamp::new(now.get() + 600_000);
    let mut compatibility = snapshot.compatibility.clone();
    compatibility.qualified_at = now;
    compatibility.valid_until = expiry;
    let snapshot = Snapshot::from_endpoints(raw.as_bytes(), now, expiry, compatibility).unwrap();
    profile["provider"] = serde_json::to_value(&snapshot).unwrap();
    let identity = ModelEndpoint {
        model: snapshot.compatibility.model.clone(),
        endpoint: snapshot.compatibility.endpoint.clone(),
    };
    let provenance = vec![Provenance {
        source: "fixture://editor-loopback-routing".into(),
        sha256: "a".repeat(64),
        observed_at: now,
        effective_at: None,
        limitations: vec![
            "Synthetic admission evidence only; no production endpoint qualification".into(),
        ],
    }];
    let candidate = Candidate {
        identity: identity.clone(),
        availability: State::Supported,
        reasons: vec![],
        provenance: provenance.clone(),
        capabilities: BTreeMap::from([("responses_text_tools".into(), State::Supported)]),
        snapshot: Some(snapshot.clone()),
        compatibility: vec![CompatibilityObservation {
            id: "editor-synthetic-probe".into(),
            compatibility: snapshot.compatibility.id.clone(),
            kind: EvidenceKind::Live,
            state: State::Supported,
            observed_at: now,
            valid_until: expiry,
            provenance: provenance.clone(),
        }],
        memberships: vec![GroupMembership {
            version: "editor-synthetic-membership/1".into(),
            group: Group::Low,
            roles: vec![RoleEvidence {
                id: "editor-synthetic-main".into(),
                role: RequestRole::Main,
                task_class: "coding".into(),
                kind: EvidenceKind::Live,
                observed_at: now,
                valid_until: expiry,
                samples: 100,
                quality_bps: 9500,
                latency_p50_ms: 5,
                latency_p95_ms: 10,
                usage_p50: None,
                usage_p95: None,
                provenance,
            }],
        }],
    };
    let policy = Policy {
        schema_version: 1,
        id: String::new(),
        parent: None,
        profile: Profile::Low,
        allowed_models: BTreeSet::from([identity.model.clone()]),
        allowed_endpoints: BTreeSet::from([identity.endpoint.clone()]),
        allowed_groups: BTreeSet::from([Group::Low]),
        quality_floor_bps: 8000,
        minimum_samples: 20,
        maximum_evidence_age_ms: 600_000,
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
    .unwrap();
    let zero = Money {
        currency: "USD".to_owned().try_into().unwrap(),
        micros: Micros::ZERO,
    };
    let configuration = Configuration {
        escalation: None,
        catalog: CatalogRevision::create(None, now, None, vec![candidate]).unwrap(),
        policy,
        task_class: "coding".into(),
        estimates: vec![CostEstimate {
            candidate: identity,
            first_attempt: Usage {
                input: Units::new(200_000),
                output: Units::new(4096),
                requests: Units::new(1),
                ..Usage::default()
            },
            retries: Usage::default(),
            handoff: Usage::default(),
            support: Some(zero.clone()),
            children: Some(zero.clone()),
            verification: Some(zero),
            assumptions: vec!["One final loopback answer; no tool work".into()],
            evidence_refs: vec!["fixture://editor-loopback-routing".into()],
        }],
        raw_catalogs: BTreeMap::from([(snapshot.id.clone(), raw)]),
    };
    configuration.validate().unwrap();
    profile["routing"] = serde_json::to_value(configuration).unwrap();
    fs::write(path, serde_json::to_vec(&profile).unwrap()).unwrap();
}
