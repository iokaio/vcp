// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::collections::BTreeMap;
use vcp_domain::{accounting::Rate, Micros};
fn fixture() -> (QualificationRecord, CurrentInstallation) {
    let evaluator = QualifiedEvaluator {
        model: "fixture/comparator".into(),
        provider: "fixture".into(),
        served_model: "fixture/comparator-v1".into(),
        served_provider: "Fixture".into(),
        operation: Operation::ConventionalChat,
        purpose: decision::Purpose::Routing,
        mode: Mode::Shadow,
        evidence_digest: "c".repeat(64),
        configuration_digest: "d".repeat(64),
        valid_until: Timestamp::new(3000),
        require_distributions: false,
        require_confidence: false,
        deny_data_collection: true,
        require_zdr: true,
        prompt_price_per_million: "1".into(),
        output_price_per_million: "1".into(),
        request_price: "0.000001".into(),
    };
    let token = Rate {
        micros: Micros::new(1_000_000),
        per_units: Units::new(1_000_000),
    };
    let record = QualificationRecord {
        version: 1,
        workspace: WorkspaceId::parse("workspace").unwrap(),
        revision: Revision::new(1),
        evaluator,
        question_revision: "e".repeat(64),
        catalog: EvidencePin {
            artifact: ArtifactId::parse("catalog").unwrap(),
            digest: "b".repeat(64),
        },
        conformance: EvidencePin {
            artifact: ArtifactId::parse("conformance").unwrap(),
            digest: "c".repeat(64),
        },
        price: PriceSnapshot {
            id: "f".repeat(64),
            capability: "d".repeat(64),
            model: "fixture/comparator".into(),
            provider: "fixture".into(),
            currency: "USD".to_owned().try_into().unwrap(),
            valid_until: Timestamp::new(3000),
            rates: BTreeMap::from([
                (ChargeCategory::Input, token.clone()),
                (ChargeCategory::Output, token.clone()),
                (ChargeCategory::CacheRead, token.clone()),
                (ChargeCategory::CacheWrite, token),
                (
                    ChargeCategory::Request,
                    Rate {
                        micros: Micros::new(1),
                        per_units: Units::new(1),
                    },
                ),
                (
                    ChargeCategory::ProviderTool,
                    Rate {
                        micros: Micros::ZERO,
                        per_units: Units::new(1),
                    },
                ),
            ]),
        },
        input_ceiling: Units::new(10000),
        output_ceiling: Units::new(decision::CONVENTIONAL_OUTPUT_LIMIT),
        expires_at: Timestamp::new(3000),
    };
    let current = current(&record);
    (record, current)
}
fn current(r: &QualificationRecord) -> CurrentInstallation {
    CurrentInstallation {
        workspace: r.workspace.clone(),
        owner: OwnerEpoch::new(1),
        revision: r.revision,
        record_digest: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(r).unwrap()),
        catalog: r.catalog.clone(),
        conformance: r.conformance.clone(),
        configuration_digest: r.evaluator.configuration_digest.clone(),
        now: Timestamp::new(1000),
    }
}
fn request(r: &QualificationRecord) -> Request {
    let state = serde_json::json!({"selected":"baseline","eligible":["baseline","other"]});
    Request {
        version: decision::VERSION,
        binding: decision::Binding {
            scope: vcp_domain::workspace::Scope {
                workspace: r.workspace.clone(),
                session: vcp_domain::SessionId::parse("session").unwrap(),
                task: vcp_domain::TaskId::parse("task").unwrap(),
            },
            root: vcp_domain::TaskId::parse("task").unwrap(),
            step: Revision::new(1),
            steering: vcp_domain::SteeringRevision::new(1),
            authority: vcp_domain::AuthorityRevision::new(1),
            deletion: vcp_domain::DeletionEpoch::ZERO,
            policy: "a".repeat(64),
            catalog: r.catalog.digest.clone(),
            input: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&state).unwrap()),
            evidence: BTreeMap::new(),
        },
        purpose: decision::Purpose::Routing,
        question_revision: r.question_revision.clone(),
        state,
        questions: BTreeMap::from([(
            "choice".into(),
            decision::Question::Choice {
                instructions: "Choose only a listed candidate; abstain if uncertain".into(),
                options: BTreeMap::from([
                    ("baseline".into(), "Selected baseline".into()),
                    ("other".into(), "Eligible other".into()),
                ]),
            },
        )]),
        deadline: Timestamp::new(2000),
    }
}
#[test]
fn finite_comparator_uses_real_quote_and_single_body_cap() {
    let (record, current) = fixture();
    let request = request(&record);
    let cap = Capability::install(record, &current).unwrap();
    cap.require_production().unwrap();
    let p = cap.prepare(&request, 0, &current).unwrap();
    assert_eq!(p.quote.amount.micros, Micros::new(31025));
    vcp_budget::arithmetic::validate_quote(&p.quote, current.now).unwrap();
    assert_eq!(
        p.prepared.body()["max_tokens"],
        decision::CONVENTIONAL_OUTPUT_LIMIT
    );
    assert_eq!(p.body_digest, vcp_protocol::digest_bytes(&p.bytes));
    assert!(matches!(
        cap.prepare(&request, 1, &current),
        Err(Rejection::Stale)
    ));
}
#[test]
fn native_claimed_finite_numbers_do_not_establish_production_qualification() {
    let (mut record, _) = fixture();
    record.evaluator.operation = Operation::JevDecisions;
    let pin = current(&record);
    assert!(matches!(
        Capability::install(record, &pin),
        Err(Rejection::NativeChargeBoundUnqualified)
    ));
}
#[test]
fn current_installation_is_required_and_immutable() {
    let (record, mut current) = fixture();
    let cap = Capability::install(record, &current).unwrap();
    current.owner = OwnerEpoch::new(2);
    assert_eq!(cap.current(&current), Err(Rejection::Stale));
    current.owner = OwnerEpoch::new(1);
    current.conformance.digest = "a".repeat(64);
    assert_eq!(cap.current(&current), Err(Rejection::Stale));
}
#[test]
fn unknown_categories_underpriced_controls_overflow_and_fake_cap_fail() {
    for case in 0..4 {
        let (mut record, _) = fixture();
        match case {
            0 => {
                record.price.rates.remove(&ChargeCategory::CacheWrite);
            }
            1 => {
                record
                    .price
                    .rates
                    .get_mut(&ChargeCategory::Output)
                    .unwrap()
                    .micros = Micros::ZERO;
            }
            2 => record.input_ceiling = Units::new(u64::MAX),
            _ => record.output_ceiling = Units::new(1023),
        }
        let pin = current(&record);
        assert!(Capability::install(record, &pin).is_err());
    }
}
#[cfg(feature = "qualification")]
#[test]
fn synthetic_native_capability_never_becomes_production_or_invents_wire_cap() {
    let (mut record, _) = fixture();
    record.evaluator.operation = Operation::JevDecisions;
    let pin = current(&record);
    let req = request(&record);
    let cap = Capability::fixture(record, &pin).unwrap();
    assert_eq!(cap.require_production(), Err(Rejection::FixtureOnly));
    let prepared = cap.prepare(&req, 0, &pin).unwrap();
    assert!(prepared.prepared.body().get("max_tokens").is_none());
    assert!(prepared.prepared.body().get("max_output_tokens").is_none());
}
