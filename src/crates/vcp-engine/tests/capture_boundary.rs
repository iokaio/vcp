// SPDX-License-Identifier: Apache-2.0
use vcp_domain::{artifact::*, ids::*, workspace::Scope};
use vcp_engine::capture::*;
use vcp_store::{BackendKind, Store};
fn spec() -> ArtifactSpec {
    ArtifactSpec {
        id: ArtifactId::new(),
        scope: Scope {
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
            task: TaskId::new(),
        },
        media_type: "application/json".into(),
        schema: "fixture/1".into(),
        source: "offline-fixture".into(),
        channel: Channel::Response,
        retention: "history".into(),
        omissions: vec![],
    }
}
#[tokio::test]
async fn transport_credentials_are_separate_from_request_capture_and_failed_capture_fences_dispatch(
) {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::open(temporary.path(), BackendKind::Files, &[])
        .await
        .unwrap();
    let credential = ProviderCredential::from_config("synthetic-provider-password".into());
    let recovery_identity = "synthetic-recovery-identity";
    let body =
        RequestBody::new(serde_json::json!({"model":"fixture","input":"public synthetic request"}));
    let mut capture = CaptureSession::new(&store);
    let request = capture.request(spec(), &body).unwrap();
    assert_eq!(
        credential.header_for_transport(),
        "synthetic-provider-password"
    );
    let mut bytes = Vec::new();
    store.spool().read(&request.descriptor, &mut bytes).unwrap();
    assert_eq!(bytes, request.body_for_transport());
    let text = String::from_utf8(bytes).unwrap();
    assert!(!text.contains(credential.header_for_transport()));
    assert!(!text.contains(recovery_identity));
    assert!(request
        .descriptor
        .spec
        .omissions
        .contains(&Omission::AuthenticationHeaders));
    let response = spec();
    let mut writer = capture.stream(response.clone()).unwrap();
    writer.write(b"observed response prefix").unwrap();
    drop(writer);
    assert!(!capture.dispatch_allowed());
    assert!(capture.request(spec(), &body).is_err());
    drop(capture);
    drop(store);
    let reopened = Store::open(temporary.path(), BackendKind::Files, &[])
        .await
        .unwrap();
    let unfinished = reopened.spool().unfinished().unwrap();
    assert_eq!(unfinished.len(), 1);
    assert_eq!(unfinished[0].spec.id, response.id);
    assert_eq!(unfinished[0].state, CaptureState::Pending);
    assert!(!CaptureSession::new(&reopened).dispatch_allowed());
}
