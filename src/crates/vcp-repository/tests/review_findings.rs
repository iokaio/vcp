// SPDX-License-Identifier: Apache-2.0
use vcp_repository::{merge::ChildPacket, review_findings::*};

fn finding() -> StructuredFinding {
    StructuredFinding {
        schema_version: 1,
        base_fingerprint: "base".into(),
        current_fingerprint: "current".into(),
        examined_paths: ["src/cart.rs".into(), "tests/cart.rs".into()].into(),
        location: FindingLocation {
            path: "src/cart.rs".into(),
            start_line: 12,
            end_line: 14,
        },
        kind: FindingKind::DemonstratedDefect,
        trigger: "An empty cart reaches the checkout division".into(),
        consequence: "Checkout panics instead of returning an empty total".into(),
        evidence: vec![FindingEvidence {
            kind: EvidenceKind::Source,
            reference: "src/cart.rs:12".into(),
            explanation: "The divisor is the unchecked cart length".into(),
        }],
        uncertainty: "Caller validation has not been established".into(),
        introduced_by_change: ChangeCausality::Unknown,
    }
}
#[test]
fn legacy_wire_records_keep_their_shape_without_invented_provenance() {
    let json = r#"{"base_fingerprint":"base","result_fingerprint":"current","changed_paths":[],"findings":["historical unverified note"]}"#;
    let packet: ChildPacket = serde_json::from_str(json).unwrap();
    packet.validate_findings().unwrap();
    assert!(matches!(packet.findings[0], ReviewFinding::Legacy(_)));
    assert_eq!(
        serde_json::to_value(&packet).unwrap(),
        serde_json::from_str::<serde_json::Value>(json).unwrap()
    );
}
#[test]
fn structured_roundtrip_keeps_scope_uncertainty_and_revision() {
    let value = ReviewFinding::Structured(Box::new(finding()));
    value.validate().unwrap();
    assert_eq!(
        serde_json::from_slice::<ReviewFinding>(&serde_json::to_vec(&value).unwrap()).unwrap(),
        value
    );
    let current = finding();
    assert!(current.matches_revision("base", "current"));
    assert!(!current.matches_revision("base", "human-edit-with-same-diff"));
    assert!(!current.matches_revision("different-base", "current"));
}
#[test]
fn changed_line_is_not_evidence_of_introduction_and_preexisting_is_explicit() {
    let mut value = finding();
    value.introduced_by_change = ChangeCausality::Introduced {
        evidence: [0].into(),
    };
    assert!(value.validate().is_err());
    value.evidence[0].kind = EvidenceKind::BaseComparison;
    value.evidence[0].explanation =
        "The base rejects empty checkout before division; current does not".into();
    value.validate().unwrap();
    value.introduced_by_change = ChangeCausality::PreExisting {
        evidence: [0].into(),
    };
    value.evidence[0].explanation = "The same defect is present in both examined revisions".into();
    value.validate().unwrap();
    value.introduced_by_change = ChangeCausality::Introduced {
        evidence: [9].into(),
    };
    assert!(value.validate().is_err());
    value.introduced_by_change = ChangeCausality::Introduced {
        evidence: [].into(),
    };
    assert!(value.validate().is_err());
}
#[test]
fn omitted_causality_remains_unknown_and_unknown_schema_is_rejected() {
    let mut value = serde_json::to_value(finding()).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .remove("introduced_by_change");
    let decoded: StructuredFinding = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(decoded.introduced_by_change, ChangeCausality::Unknown);
    value["schema_version"] = 2.into();
    assert!(serde_json::from_value::<StructuredFinding>(value)
        .unwrap()
        .validate()
        .is_err());
}
#[test]
fn locations_paths_and_all_text_are_bounded() {
    for invalid in [
        "../secret",
        "C:/secret",
        "src/../secret",
        "src\\secret",
        "/absolute",
        "src//cart.rs",
    ] {
        let mut value = finding();
        value.location.path = invalid.into();
        value.examined_paths.insert(invalid.into());
        assert!(value.validate().is_err(), "{invalid}");
    }
    let mut value = finding();
    value.location.start_line = 0;
    assert!(value.validate().is_err());
    value = finding();
    value.location.end_line = 500;
    assert!(value.validate().is_err());
    value = finding();
    value.uncertainty.clear();
    assert!(value.validate().is_err());
    value = finding();
    value.evidence.clear();
    assert!(value.validate().is_err());
    value = finding();
    value.trigger = "x".repeat(2049);
    assert!(value.validate().is_err());
    value = finding();
    value.examined_paths.clear();
    assert!(value.validate().is_err());
}
