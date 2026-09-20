// SPDX-License-Identifier: Apache-2.0
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};
use vcp_domain::{policy::GrantScope, Revision, WorkspaceId};
use vcp_extensions::mcp::{
    client::{Client, Incoming},
    http::{Decoder, Error, Frame, Limits, ResponseTo},
    registration::{Capabilities, Registration, Transport},
};

// Convenience only for tests whose expected result spans multiple complete
// events. Production integration must process each Step before its suffix.
fn push(d: &mut Decoder, mut bytes: &[u8], now: Instant) -> Result<Vec<Frame>, Error> {
    let mut frames = Vec::new();
    loop {
        let step = d.push(bytes, now)?;
        if let Some(frame) = step.frame {
            frames.push(frame);
        }
        if step.consumed == bytes.len() {
            return Ok(frames);
        }
        assert!(step.consumed > 0);
        bytes = &bytes[step.consumed..];
    }
}

fn decoder(content_type: &str, limits: Limits, now: Instant) -> Decoder {
    Decoder::new(
        ResponseTo::Request,
        200,
        Some(content_type),
        limits,
        now,
        now + Duration::from_secs(10),
    )
    .unwrap()
}
fn jsons(frames: &[Frame]) -> Vec<Vec<u8>> {
    frames
        .iter()
        .filter_map(|f| f.json_bytes().map(<[u8]>::to_vec))
        .collect()
}
fn client() -> Client {
    Client::new(Registration {
        id: "fixture".into(),
        revision: Revision::ZERO,
        scope: GrantScope::Workspace {
            workspace: WorkspaceId::parse("workspace").unwrap(),
        },
        transport: Transport::Stdio {
            process_profile: "trusted".into(),
            resolved_digest: "0".repeat(64),
        },
        allowed_resources: BTreeSet::new(),
        allowed_prompts: BTreeSet::new(),
        auth_refs: BTreeSet::new(),
        allowed_tools: BTreeSet::from(["echo".into()]),
        trusted_effects: BTreeMap::new(),
        capabilities: Capabilities::default(),
        limits: vcp_extensions::mcp::registration::Limits {
            frame_bytes: 4096,
            total_discovery_bytes: 16384,
            tools: 16,
            pages: 4,
            timeout_ms: 1000,
            stderr_bytes: 4096,
        },
    })
    .unwrap()
}

#[test]
fn media_type_and_status_are_exact_not_prefix_matches() {
    let now = Instant::now();
    for value in [
        "application/json",
        "Application/JSON",
        "text/event-stream; charset=utf-8",
        "text/event-stream; charset=\"UTF-8\"",
    ] {
        decoder(value, Limits::default(), now);
    }
    for value in [
        "application/json-seq",
        "text/event-stream-fake",
        "application/json,text/event-stream",
        "application/json; charset=utf-16",
        "application/json; charset=utf-8; charset=utf-8",
        "text/event-stream; boundary=a",
        "application/json\r\nx: y",
        "text/event-stream;",
    ] {
        assert!(
            matches!(
                Decoder::new(
                    ResponseTo::Request,
                    200,
                    Some(value),
                    Limits::default(),
                    now,
                    now + Duration::from_secs(1)
                ),
                Err(Error::MediaType)
            ),
            "{value}"
        );
    }
    for status in [202, 204, 301, 400, 401, 404, 405, 429, 500] {
        assert!(matches!(
            Decoder::new(
                ResponseTo::Request,
                status,
                Some("application/json"),
                Limits::default(),
                now,
                now + Duration::from_secs(1)
            ),
            Err(Error::Status)
        ));
    }
    assert!(matches!(
        Decoder::new(
            ResponseTo::Request,
            200,
            None,
            Limits::default(),
            now,
            now + Duration::from_secs(1)
        ),
        Err(Error::MediaType)
    ));
}

#[test]
fn acknowledgements_require_empty_body_and_202() {
    let now = Instant::now();
    let make = || {
        Decoder::new(
            ResponseTo::NotificationOrResponse,
            202,
            None,
            Limits::default(),
            now,
            now + Duration::from_secs(1),
        )
        .unwrap()
    };
    assert!(make().finish(now).unwrap().frame.is_none());
    for bytes in [b" ".as_slice(), b"{}", b"\n"] {
        let mut d = make();
        assert!(matches!(
            push(&mut d, bytes, now),
            Err(Error::AcknowledgementBody)
        ));
        assert!(matches!(push(&mut d, b"", now), Err(Error::State)));
    }
    assert!(matches!(
        Decoder::new(
            ResponseTo::NotificationOrResponse,
            200,
            Some("application/json"),
            Limits::default(),
            now,
            now + Duration::from_secs(1)
        ),
        Err(Error::Status)
    ));
}

#[test]
fn json_emits_only_at_eof_and_codec_retains_protocol_validation() {
    let now = Instant::now();
    for body in [
        br#"{"jsonrpc":"2.0","id":"wrong","result":{}}"#.as_slice(),
        br#"{"jsonrpc":"2.0","id":"vcp-1","id":"vcp-1","result":{}}"#,
        br#"{}{}"#,
        br#"[]"#,
    ] {
        let mut c = client();
        let out = c.initialize().unwrap();
        c.confirm_sent(&out).unwrap();
        let mut d = decoder("application/json", Limits::default(), now);
        for byte in body {
            assert!(push(&mut d, &[*byte], now).unwrap().is_empty());
        }
        let end = d.finish(now).unwrap();
        assert!(c.receive(end.frame.unwrap().json_bytes().unwrap()).is_err());
    }
    for body in [b"".as_slice(), b" \r\n\t"] {
        let mut d = decoder("application/json", Limits::default(), now);
        push(&mut d, body, now).unwrap();
        assert!(matches!(d.finish(now), Err(Error::Empty)));
    }
}

#[test]
fn sse_is_invariant_under_every_chunk_boundary_including_bom_crlf_and_utf8() {
    let now = Instant::now();
    let body = "\u{feff}: comment\r\nid: opaque-id\r\nretry: 000200\r\ndata:\r\n\r\nevent: message\ndata: {\ndata: \"value\":\"雪\"}\n\n".as_bytes();
    for split in 0..=body.len() {
        let mut d = decoder("text/event-stream", Limits::default(), now);
        let mut frames = push(&mut d, &body[..split], now).unwrap();
        frames.extend(push(&mut d, &body[split..], now).unwrap());
        assert_eq!(
            jsons(&frames),
            vec!["{\n\"value\":\"雪\"}".as_bytes().to_vec()],
            "split {split}"
        );
        assert_eq!(frames.len(), 2);
        let Frame::Metadata(meta) = &frames[0] else {
            panic!()
        };
        assert_eq!(meta.last_event_id.as_deref(), Some("opaque-id"));
        assert_eq!(meta.retry_milliseconds.as_deref(), Some("000200"));
        let end = d.finish(now).unwrap();
        assert!(!end.discarded_partial_event);
        assert_eq!(end.summary.messages, 1);
    }
    let mut d = decoder("text/event-stream", Limits::default(), now);
    let mut frames = vec![];
    for byte in body {
        frames.extend(push(&mut d, &[*byte], now).unwrap());
    }
    assert_eq!(
        jsons(&frames),
        vec!["{\n\"value\":\"雪\"}".as_bytes().to_vec()]
    );
}

#[test]
fn cr_lf_and_crlf_have_equivalent_event_framing() {
    let now = Instant::now();
    for newline in ["\n", "\r", "\r\n"] {
        let body = ["data: {}", "", "data: []", "", ""].join(newline);
        let mut d = decoder("text/event-stream", Limits::default(), now);
        assert_eq!(
            jsons(&push(&mut d, body.as_bytes(), now).unwrap()),
            vec![b"{}".to_vec(), b"[]".to_vec()]
        );
        assert!(!d.finish(now).unwrap().discarded_partial_event);
    }
}

#[test]
fn metadata_is_inert_preserved_and_obeys_field_rules() {
    let now = Instant::now();
    let mut d = decoder("text/event-stream", Limits::default(), now);
    let frames = push(&mut d, b"id: first\nretry: 99999999999999999999999999999999999999\n\nid: ignored\0id\nretry: +1\nunknown: inert\ndata: {}\n\nid\nretry: 1.5\ndata:  {}\n\n", now).unwrap();
    assert_eq!(jsons(&frames), vec![b"{}".to_vec(), b" {}".to_vec()]);
    let Frame::Json {
        metadata: Some(meta),
        ..
    } = &frames[1]
    else {
        panic!()
    };
    assert_eq!(meta.last_event_id.as_deref(), Some("first"));
    assert_eq!(
        meta.retry_milliseconds.as_deref(),
        Some("99999999999999999999999999999999999999")
    );
    let Frame::Json {
        metadata: Some(meta),
        ..
    } = &frames[2]
    else {
        panic!()
    };
    assert_eq!(meta.last_event_id.as_deref(), Some(""));
    assert!(!format!("{frames:?}").contains("999999"));
    assert!(!format!("{frames:?}").contains("first"));
    assert_eq!(d.finish(now).unwrap().summary.messages, 2);
}

#[test]
fn no_data_comments_unknown_fields_and_empty_priming_are_not_json() {
    let now = Instant::now();
    let mut d = decoder("text/event-stream", Limits::default(), now);
    let frames = push(
        &mut d,
        b": keepalive\n\nunknown: ignored\n\ndata:\n\nData: case-sensitive-ignored\n\n",
        now,
    )
    .unwrap();
    assert_eq!(frames.len(), 1);
    assert!(matches!(frames[0], Frame::Metadata(_)));
    let end = d.finish(now).unwrap();
    assert_eq!(end.summary.events, 4);
    assert_eq!(end.summary.messages, 0);
}

#[test]
fn incomplete_sse_is_discarded_and_never_completes_pending_call() {
    let now = Instant::now();
    for body in [
        b"data: {}".as_slice(),
        b"data: {}\n",
        b"id: x\n",
        b": unfinished",
    ] {
        let mut d = decoder("text/event-stream", Limits::default(), now);
        assert!(push(&mut d, body, now).unwrap().is_empty());
        let end = d.finish(now).unwrap();
        assert!(end.discarded_partial_event);
        assert_eq!(end.summary.messages, 0);
        assert!(end.frame.is_none());
    }
}

#[test]
fn absolute_deadline_never_resets_on_keepalive_retry_or_empty_chunks() {
    let now = Instant::now();
    let mut d = decoder("text/event-stream", Limits::default(), now);
    push(
        &mut d,
        b": keepalive\nretry: 0\n\n",
        now + Duration::from_secs(9),
    )
    .unwrap();
    assert!(matches!(
        push(&mut d, b"", now + Duration::from_secs(10)),
        Err(Error::Deadline)
    ));
    assert!(matches!(push(&mut d, b"", now), Err(Error::State)));
    let mut d = decoder("text/event-stream", Limits::default(), now);
    push(&mut d, b"", now + Duration::from_secs(1)).unwrap();
    assert!(matches!(d.check_deadline(now), Err(Error::State)));
    assert!(matches!(
        decoder("application/json", Limits::default(), now).finish(now + Duration::from_secs(10)),
        Err(Error::Deadline)
    ));
}

#[test]
fn budgets_reject_before_unbounded_lines_data_or_metadata_are_retained() {
    let now = Instant::now();
    let cases = [
        (
            Limits {
                frame_bytes: 2,
                ..Limits::default()
            },
            b"data: xxx\n\n".as_slice(),
        ),
        (
            Limits {
                line_bytes: 4,
                ..Limits::default()
            },
            b":xxxx",
        ),
        (
            Limits {
                metadata_bytes: 2,
                ..Limits::default()
            },
            b"id: xxx\n\n",
        ),
        (
            Limits {
                metadata_bytes: 2,
                ..Limits::default()
            },
            b"retry: 000\n\n",
        ),
        (
            Limits {
                lines: 2,
                ..Limits::default()
            },
            b":a\n:b\n:c\n",
        ),
        (
            Limits {
                events: 2,
                ..Limits::default()
            },
            b"\n\n\n",
        ),
        (
            Limits {
                body_bytes: 2,
                frame_bytes: 2,
                ..Limits::default()
            },
            b"   ",
        ),
    ];
    for (limits, body) in cases {
        let mut d = decoder("text/event-stream", limits, now);
        assert!(matches!(push(&mut d, body, now), Err(Error::Bounds)));
        assert!(matches!(push(&mut d, b"", now), Err(Error::State)));
    }
    let mut d = decoder(
        "text/event-stream",
        Limits {
            frame_bytes: 2,
            ..Limits::default()
        },
        now,
    );
    assert_eq!(
        jsons(&push(&mut d, b"data: {}\n\n", now).unwrap()),
        vec![b"{}".to_vec()]
    );
    let mut d = decoder(
        "application/json",
        Limits {
            frame_bytes: 2,
            ..Limits::default()
        },
        now,
    );
    assert!(matches!(push(&mut d, b"{} ", now), Err(Error::Bounds)));
}

#[test]
fn strict_utf8_and_unsupported_event_types_fail_without_payload_errors() {
    let now = Instant::now();
    for bytes in [b"data: \xff\n\n".as_slice(), b":\xff\n", b"id:\xff\n"] {
        let mut d = decoder("text/event-stream", Limits::default(), now);
        assert!(matches!(push(&mut d, bytes, now), Err(Error::Utf8)));
    }
    let mut d = decoder("text/event-stream", Limits::default(), now);
    push(&mut d, b"data: \xe2", now).unwrap();
    assert!(matches!(d.finish(now), Err(Error::Utf8)));
    let mut d = decoder("text/event-stream", Limits::default(), now);
    let err = push(
        &mut d,
        b"event: endpoint\ndata: https://secret.invalid/token\n\n",
        now,
    )
    .unwrap_err();
    assert_eq!(err, Error::EventType);
    assert!(!format!("{err:?} {err}").contains("secret"));
}

#[test]
fn production_codec_accepts_initialized_frame_after_priming_and_no_reconnect_action() {
    let now = Instant::now();
    let mut c = client();
    let out = c.initialize().unwrap();
    c.confirm_sent(&out).unwrap();
    let payload = serde_json::to_vec(&serde_json::json!({"jsonrpc":"2.0","id":out.request_id().unwrap(),"result":{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"fixture","version":"1"}}})).unwrap();
    let mut d = decoder("text/event-stream", Limits::default(), now);
    assert!(push(&mut d, b"id: priming\ndata:\nretry: 1000\n\n", now)
        .unwrap()
        .iter()
        .all(|f| f.json_bytes().is_none()));
    assert!(c.pending_kind().is_some(), "priming is not a receipt");
    let mut wire = b"data: ".to_vec();
    wire.extend(payload);
    wire.extend(b"\n\n");
    let frames = push(&mut d, &wire, now).unwrap();
    let Incoming::Initialized { notification, .. } =
        c.receive(frames[0].json_bytes().unwrap()).unwrap()
    else {
        panic!()
    };
    assert!(
        c.list_tools().is_err(),
        "framing does not send initialized implicitly"
    );
    c.validate_send(&notification).unwrap();
    c.confirm_sent(&notification).unwrap();
    let ack = Decoder::new(
        ResponseTo::NotificationOrResponse,
        202,
        None,
        Limits::default(),
        now,
        now + Duration::from_secs(1),
    )
    .unwrap();
    ack.finish(now).unwrap();
    assert!(c.list_tools().is_ok());
    assert_eq!(d.finish(now).unwrap().summary.messages, 1);
}

#[test]
fn a_later_bad_event_cannot_discard_an_already_yielded_reply() {
    let now = Instant::now();
    let mut c = client();
    let out = c.initialize().unwrap();
    c.confirm_sent(&out).unwrap();
    let payload = serde_json::to_vec(&serde_json::json!({"jsonrpc":"2.0","id":out.request_id().unwrap(),"result":{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"fixture","version":"1"}}})).unwrap();
    let mut wire = b"data: ".to_vec();
    wire.extend(payload);
    wire.extend(b"\r\n\r\n");
    let reply_length = wire.len();
    wire.extend(b"data: \xff\n\n");
    let mut d = decoder("text/event-stream", Limits::default(), now);
    let step = d.push(&wire, now).unwrap();
    // A CR completes the blank line; the following LF is consumed on the next step.
    assert_eq!(step.consumed, reply_length - 1);
    assert!(matches!(
        c.receive(step.frame.unwrap().json_bytes().unwrap())
            .unwrap(),
        Incoming::Initialized { .. }
    ));
    assert!(matches!(
        d.push(&wire[step.consumed..], now),
        Err(Error::Utf8)
    ));
    // The valid protocol result was delivered before the later framing failure;
    // the host owns persistence and connection closure, not this decoder.
    assert!(c.connection().is_some());
}

#[test]
fn step_yields_before_pending_control_reply_and_http_remains_disabled() {
    let now = Instant::now();
    let mut c = client();
    let out = c.initialize().unwrap();
    c.confirm_sent(&out).unwrap();
    let initialized = serde_json::to_vec(&serde_json::json!({"jsonrpc":"2.0","id":out.request_id().unwrap(),"result":{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"fixture","version":"1"}}})).unwrap();
    let Incoming::Initialized { notification, .. } = c.receive(&initialized).unwrap() else {
        panic!()
    };
    c.confirm_sent(&notification).unwrap();
    let list = c.list_tools().unwrap();
    c.confirm_sent(&list).unwrap();
    let mut wire = b"data: {\"jsonrpc\":\"2.0\",\"id\":\"ping\",\"method\":\"ping\"}\n\n".to_vec();
    let first_len = wire.len();
    wire.extend(
        format!(
            "data: {{\"jsonrpc\":\"2.0\",\"id\":\"{}\",\"result\":{{\"tools\":[]}}}}\n\n",
            list.request_id().unwrap()
        )
        .as_bytes(),
    );
    let mut d = decoder("text/event-stream", Limits::default(), now);
    let step = d.push(&wire, now).unwrap();
    assert_eq!(step.consumed, first_len);
    let Incoming::ControlReply(reply) = c
        .receive(step.frame.unwrap().json_bytes().unwrap())
        .unwrap()
    else {
        panic!()
    };
    c.validate_send(&reply).unwrap();
    c.confirm_sent(&reply).unwrap();
    // Host must perform the separately admitted POST and receive its empty 202.
    let ack = Decoder::new(
        ResponseTo::NotificationOrResponse,
        202,
        None,
        Limits::default(),
        now,
        now + Duration::from_secs(1),
    )
    .unwrap();
    ack.finish(now).unwrap();
    let next = d.push(&wire[step.consumed..], now).unwrap();
    assert!(matches!(
        c.receive(next.frame.unwrap().json_bytes().unwrap())
            .unwrap(),
        Incoming::DiscoveryComplete { .. }
    ));

    let mut registration = Registration {
        id: "http-fixture".into(),
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
        allowed_tools: BTreeSet::new(),
        trusted_effects: BTreeMap::new(),
        capabilities: Capabilities::default(),
        limits: vcp_extensions::mcp::registration::Limits {
            frame_bytes: 4096,
            total_discovery_bytes: 16384,
            tools: 16,
            pages: 4,
            timeout_ms: 1000,
            stderr_bytes: 0,
        },
    };
    registration.validate().unwrap();
    assert!(
        Client::new(registration.clone()).is_err(),
        "framing alone does not enable HTTP"
    );
    registration.transport = Transport::Stdio {
        process_profile: "trusted".into(),
        resolved_digest: "0".repeat(64),
    };
    assert!(Client::new(registration).is_ok());
}

#[test]
fn constructor_rejects_unbounded_limits_and_expired_or_excessive_deadlines() {
    let now = Instant::now();
    for limits in [
        Limits {
            events: 0,
            ..Limits::default()
        },
        Limits {
            body_bytes: usize::MAX,
            ..Limits::default()
        },
        Limits {
            metadata_bytes: 4097,
            ..Limits::default()
        },
    ] {
        assert!(matches!(
            Decoder::new(
                ResponseTo::Request,
                200,
                Some("application/json"),
                limits,
                now,
                now + Duration::from_secs(1)
            ),
            Err(Error::Limits)
        ));
    }
    assert!(matches!(
        Decoder::new(
            ResponseTo::Request,
            200,
            Some("application/json"),
            Limits::default(),
            now,
            now
        ),
        Err(Error::Deadline)
    ));
    assert!(matches!(
        Decoder::new(
            ResponseTo::Request,
            200,
            Some("application/json"),
            Limits::default(),
            now,
            now + Duration::from_secs(301)
        ),
        Err(Error::Limits)
    ));
}
