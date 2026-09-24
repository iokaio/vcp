// SPDX-License-Identifier: Apache-2.0
//! Inspection metadata requires its negotiated profile and current read access.
use super::*;
use serde_json::json;

struct Host {
    methods: Vec<&'static str>,
    calls: usize,
    readable: bool,
}
impl RpcHost for Host {
    fn supported_methods(&self) -> &[&str] {
        &self.methods
    }
    fn authorize(&self, _: &Access) -> Result<(), RpcError> {
        if self.readable {
            Ok(())
        } else {
            Err(application(
                Code::PolicyDenied,
                Retry::AfterRevalidation,
                None,
                "read access revoked",
            ))
        }
    }
    async fn call(&mut self, _: Call, _: &Access) -> Result<ResultValue, RpcError> {
        self.calls += 1;
        // The probe has no canonical store: reaching it proves admission only.
        Err(RpcError::internal_error())
    }
}
async fn send(rpc: &mut RpcSession, host: &mut Host, method: &str, params: Value) -> Value {
    let frame = json!({"jsonrpc":"2.0","id":"inspect","method":method,"params":params});
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

#[tokio::test]
async fn inspector_profiles_gate_dispatch_and_recheck_current_access() {
    for (method, profile, params) in [
        (
            "history/query",
            vcp_protocol::history::CAPABILITY,
            json!({"scope":{"workspace":"workspace","session":"session"},"task":null,"selector":null,"text":null,"artifact":null,"expand_compacted":false,"limit":1,"cursor":null}),
        ),
        (
            "memory/history",
            vcp_protocol::memory_history::CAPABILITY,
            json!({"scope":{"workspace":"workspace","session":"session"},"task":"task","claim":"claim","limit":1,"cursor":null}),
        ),
    ] {
        for negotiated in [false, true] {
            let mut host = Host {
                methods: vec![method],
                calls: 0,
                readable: true,
            };
            let mut server = super::tests::server();
            server.methods = vec![method.into()];
            server.capabilities = capabilities_for_methods(&server.methods);
            assert!(server.capabilities.contains(profile));
            let mut rpc = RpcSession::new(server).unwrap();
            let caps = if negotiated {
                vec![method, profile]
            } else {
                vec![method]
            };
            let init = send(&mut rpc, &mut host, "initialize", json!({"protocol_version":"1.0","client":{"name":"inspector","version":"1"},"capabilities":caps,"required_capabilities":caps})).await;
            assert!(init.get("error").is_none(), "{init}");
            let result = send(&mut rpc, &mut host, method, params.clone()).await;
            if negotiated {
                assert_eq!(host.calls, 1, "{result}");
                host.readable = false;
                let denied = send(&mut rpc, &mut host, method, params.clone()).await;
                assert_eq!(denied["error"]["data"]["details"]["code"], "POLICY_DENIED");
                assert_eq!(host.calls, 1);
            } else {
                assert_eq!(
                    result["error"]["data"]["details"]["code"],
                    "CAPABILITY_UNAVAILABLE"
                );
                assert_eq!(host.calls, 0);
            }
        }
        assert!(!capabilities_for_methods(&[]).contains(profile));
    }
}
