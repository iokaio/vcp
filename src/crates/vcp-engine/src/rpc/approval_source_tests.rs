// SPDX-License-Identifier: Apache-2.0
//! Compatibility is tested at the actual initialized RPC serialization boundary.
use super::*;
use serde::Deserialize;
use serde_json::json;

const HOST_METHODS: &[&str] = &["task/read", "approval/respond", "session/snapshot"];

fn task() -> methods::TaskView {
    serde_json::from_value(json!({
        "scope":{"workspace":"workspace","session":"session"},
        "task":"task","root":"task","parent":null,"turn":null,
        "revision":"9","steering_revision":"4","state":"paused",
        "reason":"pending approval preserved across attachment", "effects":"pending",
        "pending_inputs":[{"id":"approval","kind":"approval","revision":"7",
            "operation_digest":"a".repeat(64),"effect_revision":"18446744073709551615",
            "policy_revision":"9007199254740993"}]
    }))
    .unwrap()
}

struct Host;
impl RpcHost for Host {
    fn supported_methods(&self) -> &[&str] {
        HOST_METHODS
    }
    fn authorize(&self, _: &Access) -> Result<(), RpcError> {
        Ok(())
    }
    async fn call(&mut self, call: Call, _: &Access) -> Result<ResultValue, RpcError> {
        match call {
            Call::TaskRead(_) => Ok(ResultValue::Task(task())),
            Call::SessionSnapshot(_) => Ok(ResultValue::Snapshot(methods::SessionSnapshot {
                session: methods::SessionView {
                    scope: task().scope,
                    revision: 0.into(),
                    configuration_revision: 0.into(),
                    fork_origin: None,
                    fork_through: None,
                },
                sequence: 9.into(),
                watermark: 9.into(),
                subscription: id("subscription")?,
                event_cursor: "opaque-cursor".into(),
                tasks: vec![task()],
                next_cursor: None,
                complete: true,
            })),
            _ => Err(RpcError::method_not_found()),
        }
    }
}

async fn send(rpc: &mut RpcSession, method: &str, params: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":method,"params":params});
    serde_json::to_value(
        rpc.dispatch_host(
            &mut Host,
            &super::tests::access(),
            jsonrpc::parse_frame(&request.to_string()),
        )
        .await
        .unwrap(),
    )
    .unwrap()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct LegacyPendingInput {
    id: methods::Id,
    kind: methods::InputKind,
    revision: methods::Counter,
    operation_digest: Option<String>,
}

#[tokio::test]
async fn source_revisions_require_explicit_negotiation_for_task_and_snapshot() {
    for requested in [false, true] {
        let mut server = super::tests::server();
        server.methods = HOST_METHODS
            .iter()
            .map(|method| (*method).to_owned())
            .collect();
        server.capabilities = capabilities_for_methods(&server.methods);
        let mut rpc = RpcSession::new(server).unwrap();
        let mut caps: Vec<_> = HOST_METHODS
            .iter()
            .map(|method| (*method).to_owned())
            .collect();
        if requested {
            caps.push(APPROVAL_SOURCE_REVISIONS_CAPABILITY.into());
        }
        let initialized = send(
            &mut rpc,
            "initialize",
            json!({"protocol_version":"1.0",
            "client":{"name":"strict-compatibility","version":"1"},
            "capabilities":caps,"required_capabilities":caps}),
        )
        .await;
        assert!(initialized.get("error").is_none(), "{initialized}");
        assert_eq!(
            initialized["result"]["capabilities"]
                .as_array()
                .unwrap()
                .contains(&json!(APPROVAL_SOURCE_REVISIONS_CAPABILITY)),
            requested
        );
        for method in ["task/read", "session/snapshot"] {
            let params = if method == "task/read" {
                json!({"scope":task().scope,"task":"task"})
            } else {
                json!({"scope":task().scope,"limit":128,"cursor":null})
            };
            let result = send(&mut rpc, method, params).await;
            assert!(result.get("error").is_none(), "{result}");
            let input = if method == "task/read" {
                &result["result"]["value"]["pending_inputs"][0]
            } else {
                &result["result"]["value"]["tasks"][0]["pending_inputs"][0]
            };
            let decoded: methods::PendingInput = serde_json::from_value(input.clone()).unwrap();
            if requested {
                assert_eq!(input["effect_revision"], "18446744073709551615");
                assert_eq!(input["policy_revision"], "9007199254740993");
                assert!(serde_json::from_value::<LegacyPendingInput>(input.clone()).is_err());
                assert!(decoded.effect_revision.is_some());
            } else {
                assert!(input.get("effect_revision").is_none());
                assert!(input.get("policy_revision").is_none());
                serde_json::from_value::<LegacyPendingInput>(input.clone()).unwrap();
                assert!(decoded.effect_revision.is_none());
            }
            assert_eq!(input["revision"], "7");
            assert_eq!(input["operation_digest"], "a".repeat(64));
        }
    }
}

#[tokio::test]
async fn optional_capability_needs_both_implemented_methods_and_required_support() {
    for methods in [vec!["task/read"], vec!["approval/respond"], vec![]] {
        let mut server = super::tests::server();
        server.methods = methods.into_iter().map(str::to_owned).collect();
        server.capabilities = capabilities_for_methods(&server.methods);
        assert!(!server
            .capabilities
            .contains(APPROVAL_SOURCE_REVISIONS_CAPABILITY));
        server
            .capabilities
            .insert(APPROVAL_SOURCE_REVISIONS_CAPABILITY.into());
        assert!(RpcSession::new(server).is_err());
    }
    // An older server with both methods may omit the optional extension.
    let mut rpc = RpcSession::new(super::tests::server()).unwrap();
    let request = super::tests::request(
        1,
        "initialize",
        json!({"protocol_version":"1.0",
        "client":{"name":"new-client","version":"1"},"capabilities":[],
        "required_capabilities":[APPROVAL_SOURCE_REVISIONS_CAPABILITY]}),
    );
    let temp = tempfile::tempdir().unwrap();
    let mut engine = super::tests::setup(temp.path(), vcp_store::BackendKind::Files).await;
    let response = super::tests::send(&mut rpc, &mut engine, &super::tests::access(), request)
        .await
        .unwrap();
    assert_eq!(response["error"]["data"]["kind"], "unsupported_capability");
    engine.into_store().close().await.unwrap();
}
