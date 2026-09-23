// SPDX-License-Identifier: Apache-2.0
//! Exercise the initialized wire boundary, including strict legacy v1 decoding.
use super::*;
use serde::Deserialize;
use serde_json::json;

struct Host;
impl RpcHost for Host {
    fn supported_methods(&self) -> &[&str] {
        &["workspace/open"]
    }
    fn authorize(&self, _: &Access) -> Result<(), RpcError> {
        Ok(())
    }
    async fn call(&mut self, call: Call, _: &Access) -> Result<ResultValue, RpcError> {
        assert!(matches!(call, Call::WorkspaceOpen(_)));
        Ok(ResultValue::Workspace(
            serde_json::from_value(json!({
                "workspace":"workspace", "host":"host", "root":"C:/workspace",
                "root_id":"opaque-root", "binding_revision":"9007199254740993",
                "revision":"19", "authority_revision":"7", "trust":"untrusted"
            }))
            .unwrap(),
        ))
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
struct LegacyWorkspace {
    workspace: methods::Id,
    host: methods::Id,
    root: String,
    trust: methods::Trust,
    revision: methods::Counter,
    authority_revision: methods::Counter,
}

#[tokio::test]
async fn workspace_binding_projection_requires_explicit_negotiation() {
    for requested in [false, true] {
        let mut server = super::tests::server();
        server.methods = vec!["workspace/open".into()];
        server.capabilities = capabilities_for_methods(&server.methods);
        let mut rpc = RpcSession::new(server).unwrap();
        let mut caps = vec!["workspace/open"];
        if requested {
            caps.push(WORKSPACE_BINDING_CAPABILITY);
        }
        let initialized = send(
            &mut rpc,
            "initialize",
            json!({
                "protocol_version":"1.0", "client":{"name":"binding-test","version":"1"},
                "capabilities":caps, "required_capabilities":caps
            }),
        )
        .await;
        assert!(initialized.get("error").is_none(), "{initialized}");
        let result = send(
            &mut rpc,
            "workspace/open",
            json!({
                "command_id":"read", "host":"host", "root":"C:/workspace"
            }),
        )
        .await;
        assert!(result.get("error").is_none(), "{result}");
        let value = result["result"]["value"].clone();
        let decoded: methods::WorkspaceView = serde_json::from_value(value.clone()).unwrap();
        if requested {
            assert_eq!(value["root_id"], "opaque-root");
            assert_eq!(value["binding_revision"], "9007199254740993");
            assert!(decoded.root_id.is_some());
            assert!(serde_json::from_value::<LegacyWorkspace>(value).is_err());
        } else {
            assert!(value.get("root_id").is_none());
            assert!(value.get("binding_revision").is_none());
            assert!(decoded.root_id.is_none());
            assert!(decoded.binding_revision.is_none());
            serde_json::from_value::<LegacyWorkspace>(value).unwrap();
        }
    }
}

#[tokio::test]
async fn workspace_binding_capability_rejects_missing_method_and_older_server() {
    let mut server = super::tests::server();
    server
        .capabilities
        .insert(WORKSPACE_BINDING_CAPABILITY.into());
    assert!(RpcSession::new(server).is_err());
    let mut server = super::tests::server();
    server.methods = vec!["workspace/open".into()];
    server.capabilities = capabilities_for_methods(&server.methods);
    server.capabilities.remove(WORKSPACE_BINDING_CAPABILITY);
    let mut rpc = RpcSession::new(server).unwrap();
    let result = send(&mut rpc, "initialize", json!({
        "protocol_version":"1.0", "client":{"name":"binding-test","version":"1"},
        "capabilities":["workspace/open"], "required_capabilities":[WORKSPACE_BINDING_CAPABILITY]
    })).await;
    assert_eq!(result["error"]["data"]["kind"], "unsupported_capability");
}
