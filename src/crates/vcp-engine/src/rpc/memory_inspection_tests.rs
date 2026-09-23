// SPDX-License-Identifier: Apache-2.0
//! Required inspection state is negotiated before calling the governed host.
use super::*;
use serde_json::json;

struct Host {
    calls: usize,
    readable: bool,
    implemented: bool,
}
impl RpcHost for Host {
    fn supported_methods(&self) -> &[&str] {
        if self.implemented {
            &["memory/inspect"]
        } else {
            &[]
        }
    }
    fn authorize(&self, _: &Access) -> Result<(), RpcError> {
        if self.readable {
            Ok(())
        } else {
            Err(application(
                Code::PolicyDenied,
                Retry::AfterRevalidation,
                None,
                "current inspection access revoked",
            ))
        }
    }
    async fn call(&mut self, _: Call, _: &Access) -> Result<ResultValue, RpcError> {
        self.calls += 1;
        serde_json::from_value(json!({"kind":"memory","value":{
            "scope":{"workspace":"workspace","session":"session"},"task":"task",
            "generation":null,"sequence":"9007199254740993","complete":true,
            "findings":[{"claim":"claim","version":"version","content":"","evidence":[],
                "state":{"memory_sequence":"9007199254740993","canonical_watermark":"18446744073709551615",
                    "visibility":"purged","applicable":false,"current":true,"resolution":null,"evidence":[]}}]
        }})).map_err(|_| RpcError::internal_error())
    }
}
fn server(extension: bool) -> ServerInfo {
    let mut server = super::tests::server();
    server.methods = vec!["memory/inspect".into()];
    server.capabilities = capabilities_for_methods(&server.methods);
    if !extension {
        server
            .capabilities
            .remove(MEMORY_INSPECTION_STATE_CAPABILITY);
    }
    server
}
async fn send(rpc: &mut RpcSession, host: &mut Host, method: &str, params: Value) -> Value {
    let frame = json!({"jsonrpc":"2.0","id":"request","method":method,"params":params});
    serde_json::to_value(
        rpc.dispatch_host(
            host,
            &super::tests::access(),
            jsonrpc::parse_frame(&frame.to_string()),
        )
        .await
        .unwrap(),
    )
    .unwrap()
}
fn initialize(extension: bool) -> Value {
    let mut caps = vec!["memory/inspect"];
    if extension {
        caps.push(MEMORY_INSPECTION_STATE_CAPABILITY);
    }
    json!({"protocol_version":"1.0","client":{"name":"memory-inspector","version":"1"},
        "capabilities":caps,"required_capabilities":caps})
}
fn inspect() -> Value {
    json!({"scope":{"workspace":"workspace","session":"session"},"task":"task","claim":"claim","version":null})
}

#[tokio::test]
async fn legacy_method_negotiation_cannot_dispatch_ambiguous_memory_projection() {
    let mut host = Host {
        calls: 0,
        readable: true,
        implemented: true,
    };
    let mut rpc = RpcSession::new(server(true)).unwrap();
    let result = send(&mut rpc, &mut host, "initialize", initialize(false)).await;
    assert!(result.get("error").is_none(), "{result}");
    let response = send(&mut rpc, &mut host, "memory/inspect", inspect()).await;
    assert_eq!(
        response["error"]["data"]["details"]["code"],
        "CAPABILITY_UNAVAILABLE"
    );
    assert_eq!(host.calls, 0);

    let mut rpc = RpcSession::new(server(true)).unwrap();
    let result = send(&mut rpc, &mut host, "initialize", initialize(true)).await;
    assert!(result.get("error").is_none(), "{result}");
    let result = send(&mut rpc, &mut host, "memory/inspect", inspect()).await;
    assert_eq!(host.calls, 1);
    assert!(result.get("error").is_none(), "{result}");
    let finding = &result["result"]["value"]["findings"][0];
    assert_eq!(finding["state"]["visibility"], "purged");
    assert_eq!(finding["state"]["applicable"], false);
    assert_eq!(finding["state"]["memory_sequence"], "9007199254740993");
    assert_eq!(
        finding["state"]["canonical_watermark"],
        "18446744073709551615"
    );
    assert_eq!(finding["content"], "");
    host.readable = false;
    let denied = send(&mut rpc, &mut host, "memory/inspect", inspect()).await;
    assert_eq!(denied["error"]["data"]["details"]["code"], "POLICY_DENIED");
    assert_eq!(
        host.calls, 1,
        "revoked authority must deny before projection"
    );
}

#[tokio::test]
async fn memory_extension_requires_advertised_and_actually_implemented_method() {
    let mut invalid = server(true);
    invalid.methods.clear();
    invalid.capabilities.remove("memory/inspect");
    assert!(RpcSession::new(invalid).is_err());

    let mut host = Host {
        calls: 0,
        readable: true,
        implemented: false,
    };
    let mut rpc = RpcSession::new(server(true)).unwrap();
    let denied = send(&mut rpc, &mut host, "initialize", initialize(true)).await;
    assert!(denied.get("error").is_some());
    assert_eq!(host.calls, 0);

    host.implemented = true;
    let mut rpc = RpcSession::new(server(false)).unwrap();
    let unsupported = send(&mut rpc, &mut host, "initialize", initialize(true)).await;
    assert_eq!(
        unsupported["error"]["data"]["kind"],
        "unsupported_capability"
    );
    assert_eq!(host.calls, 0);
}
