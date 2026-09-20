// SPDX-License-Identifier: Apache-2.0
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{policy::GrantScope, Revision, WorkspaceId};
use vcp_extensions::mcp::{
    client::{CallReply, Client, Error, Incoming, Notification, Outbound, PendingKind},
    registration::{Capabilities, Limits, Registration, Transport},
    schema::{self, Schema},
};

fn registration() -> Registration {
    Registration {
        id: "fixture".into(),
        revision: Revision::ZERO,
        scope: GrantScope::Workspace {
            workspace: WorkspaceId::parse("workspace").unwrap(),
        },
        transport: Transport::Stdio {
            process_profile: "trusted".into(),
            resolved_digest: "0".repeat(64),
        },
        auth_refs: BTreeSet::new(),
        allowed_tools: BTreeSet::from(["echo".into(), "second".into(), "unsupported".into()]),
        trusted_effects: BTreeMap::new(),
        limits: Limits {
            frame_bytes: 4096,
            total_discovery_bytes: 16384,
            tools: 16,
            pages: 4,
            timeout_ms: 1000,
            stderr_bytes: 4096,
        },
        capabilities: Capabilities::default(),
    }
}
fn wire(value: Value) -> Vec<u8> {
    serde_json::to_vec(&value).unwrap()
}
fn reply(out: &Outbound, result: Value) -> Vec<u8> {
    wire(json!({"jsonrpc":"2.0","id":out.request_id().unwrap(),"result":result}))
}
fn init_result() -> Value {
    json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"fixture","version":"1"}})
}
fn ready() -> Client {
    let mut client = Client::new(registration()).unwrap();
    let out = client.initialize().unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(out.bytes()).unwrap()["params"]["capabilities"],
        json!({})
    );
    client.confirm_sent(&out).unwrap();
    let Incoming::Initialized { notification, .. } =
        client.receive(&reply(&out, init_result())).unwrap()
    else {
        panic!()
    };
    assert!(
        client.list_tools().is_err(),
        "initialized notification must be dispatched first"
    );
    client.confirm_sent(&notification).unwrap();
    client
}
fn tool(name: &str) -> Value {
    json!({"name":name,"description":"untrusted tool text","inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"text":{"type":"string","maxLength":32}},"required":["text"],"additionalProperties":false},"annotations":{"readOnlyHint":true}})
}
fn discover(client: &mut Client) -> vcp_extensions::mcp::client::DiscoveredTool {
    let out = client.list_tools().unwrap();
    client.confirm_sent(&out).unwrap();
    let Incoming::DiscoveryComplete { tools, rejected } = client
        .receive(&reply(&out, json!({"tools":[tool("echo")]})))
        .unwrap()
    else {
        panic!()
    };
    assert!(rejected.is_empty());
    tools.into_iter().next().unwrap()
}

#[test]
fn handshake_rejects_revision_drift_unsent_and_mismatched_response_without_followup() {
    for failure in ["version", "unsent", "wrong-id", "duplicate", "trailing"] {
        let mut client = Client::new(registration()).unwrap();
        let out = client.initialize().unwrap();
        if failure != "unsent" {
            client.confirm_sent(&out).unwrap();
        }
        let mut result = init_result();
        if failure == "version" {
            result["protocolVersion"] = json!("2026-07-28");
        }
        let mut response = reply(&out, result);
        if failure == "wrong-id" {
            response = wire(json!({"jsonrpc":"2.0","id":"wrong","result":init_result()}));
        }
        if failure == "duplicate" {
            response = br#"{"jsonrpc":"2.0","id":"vcp-1","id":"vcp-1","result":{}}"#.to_vec();
        }
        if failure == "trailing" {
            response.extend_from_slice(b"{}");
        }
        assert!(client.receive(&response).is_err(), "{failure}");
        assert!(client.initialize().is_err());
        assert!(client.list_tools().is_err());
    }
}

#[test]
fn pages_publish_atomically_and_unsupported_tools_remain_rejected() {
    let mut client = ready();
    let out = client.list_tools().unwrap();
    client.confirm_sent(&out).unwrap();
    assert!(matches!(
        client
            .receive(&reply(
                &out,
                json!({"tools":[tool("echo")],"nextCursor":"page2"})
            ))
            .unwrap(),
        Incoming::DiscoveryPage { next: true }
    ));
    assert_eq!(client.tools().count(), 0);
    let next = client.list_tools().unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(next.bytes()).unwrap()["params"]["cursor"],
        "page2"
    );
    client.confirm_sent(&next).unwrap();
    let mut unsupported = tool("unsupported");
    unsupported["inputSchema"]["allOf"] = json!([]);
    let Incoming::DiscoveryComplete { tools, rejected } = client
        .receive(&reply(
            &next,
            json!({"tools":[tool("second"),tool("not-allowed"),unsupported]}),
        ))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(tools.len(), 2);
    assert_eq!(rejected.len(), 2);
    assert_eq!(tools[0].annotations["readOnlyHint"], true);
    assert_eq!(
        registration().effects("echo").unwrap(),
        BTreeSet::from([vcp_domain::policy::EffectClass::Opaque])
    );
}

#[test]
fn duplicate_names_and_cursor_cycles_poison_discovery() {
    for repeated_name in [true, false] {
        let mut client = ready();
        let first = client.list_tools().unwrap();
        client.confirm_sent(&first).unwrap();
        client
            .receive(&reply(
                &first,
                json!({"tools":[tool("echo")],"nextCursor":"same"}),
            ))
            .unwrap();
        let next = client.list_tools().unwrap();
        client.confirm_sent(&next).unwrap();
        let result = if repeated_name {
            json!({"tools":[tool("echo")]})
        } else {
            json!({"tools":[tool("second")],"nextCursor":"same"})
        };
        assert!(client.receive(&reply(&next, result)).is_err());
        assert_eq!(client.tools().count(), 0);
        assert!(client.list_tools().is_err());
    }
}

#[test]
fn calls_bind_arguments_and_catalog_and_do_not_treat_annotations_as_authority() {
    let mut client = ready();
    let tool = discover(&mut client);
    let args = tool
        .check_arguments(br#"{"text":"private-payload-sentinel"}"#)
        .unwrap();
    let wrong = Schema::compile(br#"{"type":"object"}"#, schema::Limits::default())
        .unwrap()
        .arguments(br#"{}"#)
        .unwrap();
    assert_eq!(
        client.call(&tool.identity, &wrong).unwrap_err(),
        Error::Arguments
    );
    let out = client.call(&tool.identity, &args).unwrap();
    assert!(!format!("{out:?} {args:?} {tool:?}").contains("private-payload-sentinel"));
    assert!(
        client.call(&tool.identity, &args).is_err(),
        "only one outstanding call"
    );
    client.confirm_sent(&out).unwrap();
    assert!(
        client.confirm_sent(&out).is_err(),
        "a write cannot be confirmed twice"
    );
    let Incoming::CallReply(CallReply::ToolResult(result)) = client.receive(&reply(&out, json!({"content":[{"type":"text","text":"external-result"},{"type":"resource_link","uri":"https://example.invalid/private"}],"structuredContent":{"count":1},"isError":false}))).unwrap() else { panic!() };
    assert_eq!(result.text, vec!["external-result"]);
    assert_eq!(result.omitted_content, vec!["resource_link"]);
    assert_eq!(result.structured_content, Some(json!({"count":1})));
    assert!(!result.is_error);
    assert!(!format!("{result:?}").contains("external-result"));
    assert!(matches!(
        client
            .receive(&wire(
                json!({"jsonrpc":"2.0","method":"notifications/tools/list_changed"})
            ))
            .unwrap(),
        Incoming::Notification(Notification::ToolListChanged)
    ));
    assert_eq!(
        client.call(&tool.identity, &args).unwrap_err(),
        Error::Stale
    );
    let refreshed = discover(&mut client);
    assert_ne!(
        tool.identity, refreshed.identity,
        "same schema after list change still invalidates prepared identity"
    );
    assert_eq!(
        client.call(&tool.identity, &args).unwrap_err(),
        Error::Stale
    );
}

#[test]
fn callbacks_are_rejected_and_ping_replied_without_replaying_pending_call() {
    let mut client = ready();
    let tool = discover(&mut client);
    let args = tool.check_arguments(br#"{"text":"x"}"#).unwrap();
    let out = client.call(&tool.identity, &args).unwrap();
    client.confirm_sent(&out).unwrap();
    for method in [
        "sampling/createMessage",
        "roots/list",
        "elicitation/create",
        "ping",
    ] {
        let Incoming::ControlReply(control) = client
            .receive(&wire(
                json!({"jsonrpc":"2.0","id":7,"method":method,"params":{}}),
            ))
            .unwrap()
        else {
            panic!()
        };
        let value: Value = serde_json::from_slice(control.bytes()).unwrap();
        if method == "ping" {
            assert_eq!(value["result"], json!({}));
        } else {
            assert_eq!(value["error"]["code"], -32601);
        }
        assert_eq!(client.pending_kind(), Some(PendingKind::CallTool));
        client.confirm_sent(&control).unwrap();
    }
    assert!(matches!(
        client.receive(&reply(&out, json!({"content":[]}))).unwrap(),
        Incoming::CallReply(CallReply::ToolResult(_))
    ));
    assert!(client.pending_kind().is_none());
}

#[test]
fn rpc_error_tool_error_and_unknown_outcome_are_distinct_and_never_retried() {
    for rpc in [false, true] {
        let mut client = ready();
        let tool = discover(&mut client);
        let args = tool.check_arguments(br#"{"text":"x"}"#).unwrap();
        let out = client.call(&tool.identity, &args).unwrap();
        client.confirm_sent(&out).unwrap();
        let response = if rpc {
            wire(
                json!({"jsonrpc":"2.0","id":out.request_id().unwrap(),"error":{"code":-32000,"message":"external failure sentinel"}}),
            )
        } else {
            reply(
                &out,
                json!({"content":[{"type":"text","text":"external failure sentinel"}],"isError":true}),
            )
        };
        let event = client.receive(&response).unwrap();
        assert!(!format!("{event:?}").contains("external failure sentinel"));
        match event {
            Incoming::CallReply(CallReply::RpcError(error)) if rpc => {
                assert_eq!(error.code, -32000)
            }
            Incoming::CallReply(CallReply::ToolResult(result)) if !rpc => assert!(result.is_error),
            _ => panic!(),
        }
        let again = client.call(&tool.identity, &args).unwrap();
        assert_ne!(
            out.request_id(),
            again.request_id(),
            "only an explicit caller request creates another call"
        );
        assert_eq!(client.abandon(), Some(PendingKind::CallTool));
        assert!(client.call(&tool.identity, &args).is_err());
    }
}

#[test]
fn bounded_wire_and_output_schema_fail_closed() {
    let mut client = ready();
    let tool = discover(&mut client);
    let args = tool.check_arguments(br#"{"text":"x"}"#).unwrap();
    let out = client.call(&tool.identity, &args).unwrap();
    client.confirm_sent(&out).unwrap();
    assert_eq!(
        client.receive(&vec![b' '; 4097]).unwrap_err(),
        Error::Bounds
    );
    let mut client = ready();
    let mut declared = tool_value_with_output();
    let out = client.list_tools().unwrap();
    client.confirm_sent(&out).unwrap();
    client
        .receive(&reply(&out, json!({"tools":[declared.take()]})))
        .unwrap();
    let tool = client.tools().next().unwrap().clone();
    let args = tool.check_arguments(br#"{"text":"x"}"#).unwrap();
    let out = client.call(&tool.identity, &args).unwrap();
    client.confirm_sent(&out).unwrap();
    assert_eq!(
        client
            .receive(&reply(
                &out,
                json!({"content":[],"structuredContent":{"count":"wrong"}})
            ))
            .unwrap_err(),
        Error::Protocol
    );
}
fn tool_value_with_output() -> Value {
    let mut value = tool("echo");
    value["outputSchema"] = json!({"type":"object","properties":{"count":{"type":"integer"}},"required":["count"],"additionalProperties":false});
    value
}

#[test]
fn outbound_admission_rejects_reuse_and_cross_connection_before_writes() {
    let mut first = Client::new(registration()).unwrap();
    let mut second = Client::new(registration()).unwrap();
    let a = first.initialize().unwrap();
    let b = second.initialize().unwrap();
    assert_eq!(
        a.bytes(),
        b.bytes(),
        "wire IDs can repeat only across separate connections"
    );
    assert!(
        first.validate_send(&b).is_err(),
        "private ownership token binds otherwise identical bytes"
    );
    first.validate_send(&a).unwrap();
    first.confirm_sent(&a).unwrap();
    assert!(
        first.validate_send(&a).is_err(),
        "cannot replay a confirmed write"
    );
    first.abandon();
    assert!(first.validate_send(&a).is_err());
}

#[test]
fn explicit_schema_dialect_is_checked_and_numeric_rounding_never_changes_arguments() {
    let exact = Schema::compile(
        br#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object"}"#,
        schema::Limits::default(),
    )
    .unwrap();
    for bytes in [
        br#"{"n":1.0}"#.as_slice(),
        br#"{"n":1e0}"#,
        br#"{"n":1e-400}"#,
        br#"{"n":18446744073709551616}"#,
    ] {
        assert!(exact.arguments(bytes).is_err());
    }
    let args = exact.arguments(br#"{"n":18446744073709551615}"#).unwrap();
    assert_eq!(args.canonical_bytes(), br#"{"n":18446744073709551615}"#);
    assert!(Schema::compile(
        br#"{"$schema":"https://example.invalid/unknown","type":"object"}"#,
        schema::Limits::default()
    )
    .is_err());
}
