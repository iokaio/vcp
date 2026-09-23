// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::json;

struct Host {
    calls: usize,
}
impl RpcHost for Host {
    fn supported_methods(&self) -> &[&str] {
        &["memory/query"]
    }
    fn authorize(&self, access: &Access) -> Result<(), RpcError> {
        if access.read {
            Ok(())
        } else {
            Err(application(
                Code::PolicyDenied,
                Retry::Never,
                None,
                "read denied",
            ))
        }
    }
    async fn call(&mut self, call: Call, _: &Access) -> Result<ResultValue, RpcError> {
        assert!(matches!(call, Call::MemoryQuery(_)));
        self.calls += 1;
        Ok(ResultValue::MemoryQuery(serde_json::from_value(json!({
            "scope":{"workspace":"workspace","session":"session"},"task":"task",
            "generation":null,"generation_watermark":null,
            "canonical_watermark":"18446744073709551615","sequence":"9007199254740993",
            "findings":[{"record_id":"artifact:chunk","source":{"kind":"artifact","artifact":"capture"},
                "root":"root","source_sha256":"a".repeat(64),"start":"2","end":"8",
                "outcome":"accepted","evidence_status":"observed","evidence":["capture"],
                "content":"source","trimmed":true,"rank":1}],
            "rebuild_required":true,"degraded":["lexical_only"],"truncated":true,"complete":false
        })).unwrap()))
    }
}

#[tokio::test]
async fn query_sources_profile_is_required_before_host_dispatch() {
    for detailed in [false, true] {
        let mut server = super::tests::server();
        server.methods = vec!["memory/query".into()];
        server.capabilities = capabilities_for_methods(&server.methods);
        let mut rpc = RpcSession::new(server).unwrap();
        let mut host = Host { calls: 0 };
        let mut capabilities = vec!["memory/query"];
        if detailed {
            capabilities.push(MEMORY_QUERY_SOURCES_CAPABILITY);
        }
        let init = super::tests::request(
            1,
            "initialize",
            json!({
                "protocol_version":"1.0","client":{"name":"query-client","version":"1"},
                "capabilities":capabilities,"required_capabilities":capabilities
            }),
        );
        let initialized = rpc
            .dispatch_host(
                &mut host,
                &super::tests::access(),
                jsonrpc::parse_frame(&init.to_string()),
            )
            .await
            .unwrap();
        assert!(serde_json::to_value(initialized)
            .unwrap()
            .get("error")
            .is_none());
        let request = || {
            jsonrpc::parse_frame(&super::tests::request(
                2,
                "memory/query",
                json!({
                    "scope":{"workspace":"workspace","session":"session"},"task":"task","query":"source","limit":1
                }),
            ).to_string())
        };
        let result = serde_json::to_value(
            rpc.dispatch_host(&mut host, &super::tests::access(), request())
                .await
                .unwrap(),
        )
        .unwrap();
        if detailed {
            assert_eq!(result["result"]["kind"], "memory_query");
            let page = &result["result"]["value"];
            assert_eq!(page["canonical_watermark"], "18446744073709551615");
            assert_eq!(page["sequence"], "9007199254740993");
            assert_eq!(
                page["findings"][0]["source"],
                json!({"kind":"artifact","artifact":"capture"})
            );
            assert_eq!(page["complete"], false);
            assert_eq!(page["truncated"], true);
            assert_eq!(host.calls, 1);
        } else {
            assert_eq!(
                result["error"]["data"]["details"]["code"],
                "CAPABILITY_UNAVAILABLE"
            );
            assert_eq!(host.calls, 0);
        }
        let before = host.calls;
        let mut denied = super::tests::access();
        denied.read = false;
        let result = serde_json::to_value(
            rpc.dispatch_host(&mut host, &denied, request())
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(result.get("error").is_some());
        assert_eq!(host.calls, before);
    }
}

#[test]
fn source_profile_cannot_be_advertised_without_query_implementation() {
    let mut server = super::tests::server();
    server
        .capabilities
        .insert(MEMORY_QUERY_SOURCES_CAPABILITY.into());
    assert!(RpcSession::new(server).is_err());
}
