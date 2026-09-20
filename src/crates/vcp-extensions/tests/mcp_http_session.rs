// SPDX-License-Identifier: Apache-2.0
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};
use vcp_domain::{policy::GrantScope, Revision, WorkspaceId};
use vcp_extensions::mcp::{
    client::{Client, Incoming, Outbound},
    http::{Decoder, ExchangeId, Limits as HttpLimits, Session, SessionError},
    registration::{Capabilities, Limits, Registration, Transport},
};

fn registration() -> Registration {
    Registration {
        id: "remote".into(),
        revision: Revision::ZERO,
        scope: GrantScope::Workspace {
            workspace: WorkspaceId::parse("workspace").unwrap(),
        },
        transport: Transport::StreamableHttp {
            remote_profile: "trusted".into(),
            resolved_digest: "0".repeat(64),
        },
        allowed_resources: BTreeSet::new(),
        allowed_prompts: BTreeSet::new(),
        auth_refs: BTreeSet::new(),
        allowed_tools: BTreeSet::from(["echo".into()]),
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
fn reply(out: &Outbound, result: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"jsonrpc":"2.0","id":out.request_id().unwrap(),"result":result}))
        .unwrap()
}
fn init() -> Value {
    json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"fixture","version":"1"}})
}
fn headers(session: &Session, id: &ExchangeId) -> BTreeMap<String, String> {
    let mut headers = BTreeMap::new();
    session.headers(id).unwrap().visit(|name, value| {
        headers.insert(name.into(), value.into());
    });
    headers
}
fn initialized() -> (Session, Outbound) {
    let mut session = Session::new(registration()).unwrap();
    let out = session.initialize().unwrap();
    let id = session.begin_exchange(&out).unwrap();
    session.written(&id, &out).unwrap();
    session
        .head(&id, 200, Some("private-session-canary"))
        .unwrap();
    assert!(session.session_digest().is_none());
    let bytes = reply(&out, init());
    let Incoming::Initialized { notification, .. } = session.receive(&id, &bytes).unwrap() else {
        panic!()
    };
    session.finish(&id, &out, bytes.len()).unwrap();
    (session, notification)
}
fn acknowledge(session: &mut Session, out: &Outbound) {
    let id = session.begin_exchange(out).unwrap();
    session.written(&id, out).unwrap();
    session.head(&id, 202, None).unwrap();
    session.finish(&id, out, 0).unwrap();
}
fn ready() -> Session {
    let (mut session, notice) = initialized();
    acknowledge(&mut session, &notice);
    session
}

#[test]
fn http_requires_session_adapter_and_outbound_headers_change_only_after_negotiation() {
    assert!(Client::new(registration()).is_err());
    let mut session = Session::new(registration()).unwrap();
    let out = session.initialize().unwrap();
    let id = session.begin_exchange(&out).unwrap();
    let h = headers(&session, &id);
    assert_eq!(h["accept"], "application/json, text/event-stream");
    assert!(!h.contains_key("mcp-protocol-version"));
    assert!(!h.contains_key("mcp-session-id"));
    let (mut session, notice) = initialized();
    let id = session.begin_exchange(&notice).unwrap();
    let h = headers(&session, &id);
    assert_eq!(h["mcp-protocol-version"], "2025-11-25");
    assert_eq!(h["mcp-session-id"], "private-session-canary");
    assert!(!format!("{session:?} {:?}", session.headers(&id).unwrap())
        .contains("private-session-canary"));
}

#[test]
fn response_head_is_never_full_write_proof_and_session_header_is_staged() {
    let mut session = Session::new(registration()).unwrap();
    let out = session.initialize().unwrap();
    let id = session.begin_exchange(&out).unwrap();
    session.head(&id, 200, Some("uncommitted")).unwrap();
    assert!(session.session_digest().is_none());
    assert_eq!(
        session.receive(&id, &reply(&out, init())).unwrap_err(),
        SessionError::State
    );
    assert!(session.written(&id, &out).is_err());
    assert!(session.initialize().is_err());
}

#[test]
fn early_head_followed_by_actual_write_is_valid_but_observation_cannot_be_reused() {
    let mut session = Session::new(registration()).unwrap();
    let out = session.initialize().unwrap();
    let id = session.begin_exchange(&out).unwrap();
    session.head(&id, 200, None).unwrap();
    session.written(&id, &out).unwrap();
    assert!(session.written(&id, &out).is_err());
    assert!(session.validate_send(&id, &out).is_err());
    let bytes = reply(&out, init());
    assert!(matches!(
        session.receive(&id, &bytes).unwrap(),
        Incoming::Initialized { .. }
    ));
    session.finish(&id, &out, bytes.len()).unwrap();
}

#[test]
fn initialized_and_control_need_empty_ack_after_write() {
    let (mut session, notice) = initialized();
    let id = session.begin_exchange(&notice).unwrap();
    session.written(&id, &notice).unwrap();
    assert!(session.list_tools().is_err());
    session.head(&id, 202, None).unwrap();
    assert!(session.list_tools().is_err());
    session.finish(&id, &notice, 0).unwrap();
    assert!(session.list_tools().is_ok());
    let (mut session, notice) = initialized();
    let id = session.begin_exchange(&notice).unwrap();
    session.written(&id, &notice).unwrap();
    session.head(&id, 202, None).unwrap();
    assert_eq!(
        session.finish(&id, &notice, 1),
        Err(SessionError::AcknowledgementBody)
    );
    assert!(session.list_tools().is_err());
}

#[test]
fn ping_post_can_be_acknowledged_while_original_sse_exchange_is_parked() {
    let mut session = ready();
    let out = session.list_tools().unwrap();
    let id = session.begin_exchange(&out).unwrap();
    session.written(&id, &out).unwrap();
    session.head(&id, 200, None).unwrap();
    let Incoming::ControlReply(control) = session
        .receive(&id, br#"{"jsonrpc":"2.0","id":"ping1","method":"ping"}"#)
        .unwrap()
    else {
        panic!()
    };
    let control_id = session.begin_exchange(&control).unwrap();
    assert!(session.begin_exchange(&control).is_err());
    session.written(&control_id, &control).unwrap();
    session.head(&control_id, 202, None).unwrap();
    session.finish(&control_id, &control, 0).unwrap();
    let bytes = reply(&out, json!({"tools":[]}));
    assert!(matches!(
        session.receive(&id, &bytes).unwrap(),
        Incoming::DiscoveryComplete { .. }
    ));
    session.finish(&id, &out, bytes.len()).unwrap();
    assert!(session.list_tools().is_ok());
}

#[test]
fn cross_session_exchange_and_outbound_tokens_cannot_acknowledge_other_bytes() {
    let mut one = Session::new(registration()).unwrap();
    let mut two = Session::new(registration()).unwrap();
    let a = one.initialize().unwrap();
    let b = two.initialize().unwrap();
    let a_id = one.begin_exchange(&a).unwrap();
    let b_id = two.begin_exchange(&b).unwrap();
    assert!(one.headers(&b_id).is_err());
    assert!(one.written(&a_id, &b).is_err());
    one.written(&a_id, &a).unwrap();
    assert!(two.written(&a_id, &b).is_err());
}

#[test]
fn expired_changed_and_invalid_session_ids_fail_closed_without_reinitialization() {
    for (status, header, expected) in [
        (404, None, SessionError::Expired),
        (200, Some("changed"), SessionError::Header),
        (200, Some("bad space"), SessionError::Header),
        (200, Some("bad\r\nheader"), SessionError::Header),
        (200, Some(""), SessionError::Header),
        (302, None, SessionError::Status),
        (202, None, SessionError::Status),
    ] {
        let mut session = ready();
        let out = session.list_tools().unwrap();
        let id = session.begin_exchange(&out).unwrap();
        session.written(&id, &out).unwrap();
        assert_eq!(session.head(&id, status, header), Err(expected));
        assert!(session.session_digest().is_none());
        assert!(session.connection().is_none());
        assert!(session.initialize().is_err());
        assert!(session.list_tools().is_err());
    }
}

#[test]
fn no_session_id_is_valid_but_cannot_be_added_later() {
    let mut session = Session::new(registration()).unwrap();
    let out = session.initialize().unwrap();
    let id = session.begin_exchange(&out).unwrap();
    session.written(&id, &out).unwrap();
    session.head(&id, 200, None).unwrap();
    let bytes = reply(&out, init());
    let Incoming::Initialized { notification, .. } = session.receive(&id, &bytes).unwrap() else {
        panic!()
    };
    session.finish(&id, &out, bytes.len()).unwrap();
    let id = session.begin_exchange(&notification).unwrap();
    assert!(!headers(&session, &id).contains_key("mcp-session-id"));
    assert_eq!(
        session.head(&id, 202, Some("late")),
        Err(SessionError::Header)
    );
}

#[test]
fn missing_reply_at_eof_is_not_success_and_valid_reply_survives_later_bad_frame() {
    let mut session = ready();
    let out = session.list_tools().unwrap();
    let id = session.begin_exchange(&out).unwrap();
    session.written(&id, &out).unwrap();
    session.head(&id, 200, None).unwrap();
    assert_eq!(
        session.finish(&id, &out, 0),
        Err(SessionError::MissingReply)
    );
    let mut session = ready();
    let out = session.list_tools().unwrap();
    let id = session.begin_exchange(&out).unwrap();
    session.written(&id, &out).unwrap();
    let class = session.head(&id, 200, None).unwrap();
    let now = Instant::now();
    let mut decoder = Decoder::new(
        class,
        200,
        Some("text/event-stream"),
        HttpLimits::default(),
        now,
        now + Duration::from_secs(1),
    )
    .unwrap();
    let mut stream = b"data: ".to_vec();
    stream.extend(reply(&out, json!({"tools":[]})));
    stream.extend(b"\n\nevent: forbidden\ndata: {}\n\n");
    let step = decoder.push(&stream, now).unwrap();
    let receipt = session
        .receive(&id, step.frame.unwrap().json_bytes().unwrap())
        .unwrap();
    assert!(decoder.push(&stream[step.consumed..], now).is_err());
    session.abort();
    assert!(matches!(receipt, Incoming::DiscoveryComplete { .. }));
}

#[test]
fn discovered_schema_identity_drives_call_and_list_change_invalidates_it() {
    let mut session = ready();
    let out = session.list_tools().unwrap();
    let id = session.begin_exchange(&out).unwrap();
    session.written(&id, &out).unwrap();
    session
        .head(&id, 200, Some("private-session-canary"))
        .unwrap();
    let bytes = reply(
        &out,
        json!({"tools":[{"name":"echo","inputSchema":{"type":"object","properties":{"text":{"type":"string","maxLength":32}},"required":["text"],"additionalProperties":false}}]}),
    );
    let Incoming::DiscoveryComplete { mut tools, .. } = session.receive(&id, &bytes).unwrap()
    else {
        panic!()
    };
    session.finish(&id, &out, bytes.len()).unwrap();
    let tool = tools.remove(0);
    let args = tool.check_arguments(br#"{"text":"fixture"}"#).unwrap();
    let out = session.call(&tool.identity, &args).unwrap();
    let id = session.begin_exchange(&out).unwrap();
    session.written(&id, &out).unwrap();
    session.head(&id, 200, None).unwrap();
    let bytes = reply(
        &out,
        json!({"content":[{"type":"text","text":"fixture"}],"structuredContent":{"result":"fixture"},"isError":false}),
    );
    assert!(matches!(
        session.receive(&id, &bytes).unwrap(),
        Incoming::CallReply(_)
    ));
    session.finish(&id, &out, bytes.len()).unwrap();
    let out = session.call(&tool.identity, &args).unwrap();
    let id = session.begin_exchange(&out).unwrap();
    session.written(&id, &out).unwrap();
    session.head(&id, 200, None).unwrap();
    assert!(matches!(
        session
            .receive(
                &id,
                br#"{"jsonrpc":"2.0","method":"notifications/tools/list_changed"}"#
            )
            .unwrap(),
        Incoming::Notification(_)
    ));
    assert!(session.tool(&tool.identity).is_err());
}

#[test]
fn invalid_initialize_does_not_commit_session_or_allow_followup() {
    for bad in ["wrong-version", "oversized-session"] {
        let mut session = Session::new(registration()).unwrap();
        let out = session.initialize().unwrap();
        let id = session.begin_exchange(&out).unwrap();
        session.written(&id, &out).unwrap();
        if bad == "oversized-session" {
            assert_eq!(
                session.head(&id, 200, Some(&"a".repeat(1025))),
                Err(SessionError::Header)
            );
        } else {
            session.head(&id, 200, Some("uncommitted-session")).unwrap();
            let mut result = init();
            result["protocolVersion"] = json!("2024-11-05");
            assert!(session.receive(&id, &reply(&out, result)).is_err());
        }
        assert!(session.session_digest().is_none());
        assert!(session.connection().is_none());
        assert!(session.list_tools().is_err());
    }
}

#[test]
fn acknowledgement_before_written_and_finish_from_another_owner_are_rejected() {
    let (mut session, notice) = initialized();
    let id = session.begin_exchange(&notice).unwrap();
    session.head(&id, 202, None).unwrap();
    assert_eq!(session.finish(&id, &notice, 0), Err(SessionError::State));
    let mut one = Session::new(registration()).unwrap();
    let mut two = Session::new(registration()).unwrap();
    let a = one.initialize().unwrap();
    let b = two.initialize().unwrap();
    let id = one.begin_exchange(&a).unwrap();
    one.written(&id, &a).unwrap();
    one.head(&id, 200, None).unwrap();
    let bytes = reply(&a, init());
    one.receive(&id, &bytes).unwrap();
    assert_eq!(one.finish(&id, &b, bytes.len()), Err(SessionError::State));
}

#[test]
fn capture_visitor_covers_staged_and_committed_session_values_without_debug_exposure() {
    let mut session = Session::new(registration()).unwrap();
    let out = session.initialize().unwrap();
    let id = session.begin_exchange(&out).unwrap();
    session.written(&id, &out).unwrap();
    session
        .head(&id, 200, Some("capture-private-canary"))
        .unwrap();
    let mut staged = Vec::new();
    session.visit_sensitive_values(|value| staged.push(value.to_owned()));
    assert_eq!(staged, ["capture-private-canary"]);
    assert!(session.session_digest().is_none());
    session.receive(&id, &reply(&out, init())).unwrap();
    let mut committed = Vec::new();
    session.visit_sensitive_values(|value| committed.push(value.to_owned()));
    assert_eq!(committed, staged);
    assert!(!format!("{session:?}").contains("capture-private-canary"));
    session.abort();
    let mut after_abort = Vec::new();
    session.visit_sensitive_values(|value| after_abort.push(value.to_owned()));
    assert!(after_abort.is_empty());
    // Captures after failure need the independently retained private snapshot.
    assert_eq!(staged, ["capture-private-canary"]);
}
