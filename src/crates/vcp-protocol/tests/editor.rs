// SPDX-License-Identifier: Apache-2.0
use serde_json::json;
use vcp_protocol::editor::{
    DocumentObservation, EditorContext, validate_context, validate_document,
};

fn document() -> serde_json::Value {
    json!({"host":"host","open_id":"open","uri":"file:///C:/root/a.txt","relative_path":"a.txt",
        "version":"7","content_sha256":vcp_protocol::digest_bytes(b"draft"),"dirty":true,"content":"draft",
        "disk_sha256":null,"language":"text","eol":"lf","encoding":"unknown","selections":[],"capture":false})
}

#[test]
fn ephemeral_editor_content_and_diagnostics_are_bounded_and_explicit() {
    let mut input = document();
    let plain: DocumentObservation = serde_json::from_value(input.clone()).unwrap();
    assert!(plain.diagnostics.is_none());
    assert!(validate_document(&plain).is_ok());
    input["diagnostics"] = json!({"collector":"vscode.languages.getDiagnostics","observed_at":"100",
        "observed_document_version":"7","producer_document_version":null,"count":64,"truncated":true,
        "sha256":"a".repeat(64)});
    assert!(validate_document(&serde_json::from_value(input.clone()).unwrap()).is_ok());
    input["diagnostics"]["count"] = json!(65);
    assert!(validate_document(&serde_json::from_value(input.clone()).unwrap()).is_err());
    input["diagnostics"]["count"] = json!(1);
    input["diagnostics"]["sha256"] = json!("not-a-digest");
    assert!(validate_document(&serde_json::from_value(input.clone()).unwrap()).is_err());
    input["diagnostics"] = serde_json::Value::Null;
    input["content"] = json!("other draft");
    assert!(validate_document(&serde_json::from_value(input).unwrap()).is_err());
}

#[test]
fn editor_close_requires_nonempty_exact_unique_handles() {
    let mut input = json!({"scope":{"workspace":"workspace","session":"session"},"task":"task",
        "mutation":{"command_id":"close","expected_revision":"7","steering_revision":"0"},"documents":[]});
    let empty: EditorContext = serde_json::from_value(input.clone()).unwrap();
    assert!(empty.closed.is_empty());
    assert!(validate_context(&empty).is_err());
    input["closed"] = json!(["observation"]);
    assert!(validate_context(&serde_json::from_value(input.clone()).unwrap()).is_ok());
    input["closed"] = json!(["observation", "observation"]);
    assert!(validate_context(&serde_json::from_value(input).unwrap()).is_err());
}
