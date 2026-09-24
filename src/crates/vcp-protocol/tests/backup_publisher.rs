// SPDX-License-Identifier: Apache-2.0
use serde_json::{json, Value};
use vcp_protocol::backup_publisher as wire;

const OPERATION: &str = "12345678-1234-1234-1234-123456789abc";
const COMMAND: &str = "abcdef01-2345-6789-abcd-0123456789ef";
fn create() -> Value {
    json!({"scope":{"workspace":"workspace","session":"session"},"mutation":{"command_id":OPERATION,"expected_revision":"9007199254740993","steering_revision":"0"},"expected_binding_revision":"4","capability":"opaque-loaded-capability","expected_capability_generation":"5"})
}

#[test]
fn publisher_accepts_only_canonical_operation_ids_and_explicit_workspace_revisions() {
    let value = create();
    let decoded: wire::Create = serde_json::from_value(value.clone()).unwrap();
    assert!(decoded.validate().is_ok());
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    for invalid in [
        "not-an-operation",
        "../private/key",
        "12345678123412341234123456789abc",
        "12345678-1234-1234-1234-123456789ABC",
        "12345678_1234-1234-1234-123456789abc",
        "12345678-1234-1234-1234-123456789abg",
    ] {
        let mut value = create();
        value["mutation"]["command_id"] = json!(invalid);
        assert!(!serde_json::from_value::<wire::Create>(value).is_ok_and(|v| v.validate().is_ok()));
        let read = serde_json::from_value::<wire::Read>(
            json!({"scope":{"workspace":"w","session":"s"},"operation":invalid}),
        );
        assert!(!read.is_ok_and(|v| v.validate().is_ok()));
    }
    let mut value = create();
    value["mutation"]["steering_revision"] = json!("1");
    assert!(serde_json::from_value::<wire::Create>(value)
        .unwrap()
        .validate()
        .is_err());
    let mut value = create();
    value["expected_binding_revision"] = json!(4);
    assert!(serde_json::from_value::<wire::Create>(value).is_err());
}

#[test]
fn no_native_material_is_accepted_and_cancel_can_target_an_intent_without_a_job() {
    for (key, value) in [
        ("key", json!("C:/private/key")),
        ("vault", json!("C:/vault")),
        ("git", json!("tool")),
        ("actor", json!("other")),
        ("controller", json!("token")),
        ("automatic", json!(true)),
    ] {
        let mut input = create();
        input[key] = value;
        assert!(serde_json::from_value::<wire::Create>(input).is_err());
    }
    let cancel = json!({"scope":{"workspace":"w","session":"s"},"mutation":{"command_id":COMMAND,"expected_revision":"1","steering_revision":"0"},"expected_binding_revision":"2","operation":OPERATION,"expected_operation_revision":"3","expected_job_revision":null});
    let value: wire::Cancel = serde_json::from_value(cancel.clone()).unwrap();
    assert!(value.validate().is_ok());
    assert!(value.expected_job_revision.is_none());
    let mut retry = cancel;
    retry["capability"] = json!("opaque");
    retry["expected_capability_generation"] = json!("4");
    assert!(serde_json::from_value::<wire::Retry>(retry.clone()).is_err());
    retry["expected_job_revision"] = json!("9007199254740994");
    let value: wire::Retry = serde_json::from_value(retry.clone()).unwrap();
    assert!(value.validate().is_ok());
    assert_eq!(serde_json::to_value(value).unwrap(), retry);
}

#[test]
fn publication_and_cleanup_facts_do_not_claim_cloud_or_restore_success() {
    let status = json!({"scope":{"workspace":"w","session":"s"},"watermark":"1","capability":{"state":"loaded","reference":"opaque","generation":"2","configuration_revision":"3"},"busy":true,"active_operation":null,"destination":"local_encrypted_vault","cloud_transfer":"unknown","restore_verification":"not_observed"});
    let view: wire::StatusView = serde_json::from_value(status.clone()).unwrap();
    assert_eq!(serde_json::to_value(view).unwrap(), status);
    let mut bad = status.clone();
    bad["capability"]["key_reference"] = json!("not-exposed");
    assert!(serde_json::from_value::<wire::StatusView>(bad).is_err());
    let mut bad = status;
    bad["cloud_transfer"] = json!("complete");
    assert!(serde_json::from_value::<wire::StatusView>(bad).is_err());
    let published = json!({"scope":{"workspace":"w","session":"s"},"operation":OPERATION,"revision":"5","job_revision":"6","watermark":"7","phase":"published","cancel_requested":true,"snapshot_watermark":"3","deletion_revision":"0","authority_revision":"1","source_pins":"held","local_publication":"published","checkpoint":"reconciliation_required","cleanup":"pending","failure":"checkpoint_pending","cloud_transfer":"unknown","restore_verification":"not_observed"});
    let view: wire::JobView = serde_json::from_value(published.clone()).unwrap();
    assert_eq!(view.phase, wire::Phase::Published);
    assert!(view.cancel_requested);
    assert_eq!(view.source_pins, wire::SourcePins::Held);
    assert_eq!(serde_json::to_value(view).unwrap(), published);
    let mut bad = published.clone();
    bad["failure"] = json!("raw native path or failure text");
    assert!(serde_json::from_value::<wire::JobView>(bad).is_err());
    let mut bad = published;
    bad["publication"] = json!({"private":"native identity"});
    assert!(serde_json::from_value::<wire::JobView>(bad).is_err());
}
