// SPDX-License-Identifier: Apache-2.0
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{Map, Value};
use vcp_domain::*;
use vcp_protocol::{canonical_bytes, event::*, persisted_json, subscription::*};

fn literal_value() -> Value {
    let mut literal = Map::new();
    literal.insert("$serde_json::private::Number".into(), "1.5".into());
    literal.insert("$serde_json::private::RawValue".into(), "[1.5]".into());
    literal.insert("sibling".into(), true.into());
    let mut data = Map::new();
    data.insert("nested".into(), Value::Object(literal));
    data.insert(
        "decimal".into(),
        serde_json::from_str("0.12345678901234567890123456789").unwrap(),
    );
    data.insert(
        "large".into(),
        serde_json::from_str("18446744073709551616").unwrap(),
    );
    data.insert("exponent".into(), serde_json::from_str("3e-128").unwrap());
    Value::Object(data)
}
fn event() -> EventEnvelope {
    EventEnvelope {
        version: 1,
        sequence: SessionSeq::new(1),
        watermark: Watermark::new(1),
        redaction: None,
        event: EventInput {
            id: EventId::parse("event").unwrap(),
            workspace: WorkspaceId::parse("workspace").unwrap(),
            session: SessionId::parse("session").unwrap(),
            task: None,
            actor: ActorId::parse("owner").unwrap(),
            correlation: CommandId::parse("command").unwrap(),
            causation: None,
            timestamp: Timestamp::new(1),
            kind: EventKind::Diagnostic,
            artifacts: vec![],
            data: literal_value(),
            metadata: None,
        },
    }
}
fn conversions<T: Serialize + DeserializeOwned>(original: &T) {
    let bytes = canonical_bytes(original).unwrap();
    let parsed: T = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(canonical_bytes(&parsed).unwrap(), bytes);
    let value = serde_json::to_value(original).unwrap();
    let owned: T = serde_json::from_value(value.clone()).unwrap();
    let borrowed = T::deserialize(&value).unwrap();
    assert_eq!(canonical_bytes(&owned).unwrap(), bytes);
    assert_eq!(canonical_bytes(&borrowed).unwrap(), bytes);
}
#[test]
fn literal_private_keys_are_objects_including_escaped_names_and_siblings() {
    for raw in [
        r#"{"$serde_json::private::Number":"1.5"}"#,
        r#"{"$serde_json::private::RawValue":"[1.5]"}"#,
        r#"{"a":0,"\u0024serde_json::private::Number":"1.5","$serde_json::private::RawValue":"[1.5]"}"#,
    ] {
        let parsed = persisted_json::parse(raw.as_bytes()).unwrap();
        assert!(parsed.is_object());
        assert_eq!(
            persisted_json::parse(&canonical_bytes(&parsed).unwrap()).unwrap(),
            parsed
        );
    }
    let original = literal_value();
    assert_eq!(
        persisted_json::parse(&canonical_bytes(&original).unwrap()).unwrap(),
        original
    );
}
#[test]
fn scalar_array_and_numeric_behavior_matches_the_selected_feature_graph() {
    for raw in [
        "null",
        "true",
        "false",
        "\"text\"",
        "[]",
        "{}",
        "[0,1,true]",
        "-0",
        "0.12345678901234567890123456789",
        "18446744073709551616",
        "3e-128",
    ] {
        let expected: Value = serde_json::from_str(raw).unwrap();
        assert_eq!(persisted_json::parse(raw.as_bytes()).unwrap(), expected);
        assert_eq!(
            canonical_bytes(&persisted_json::parse(raw.as_bytes()).unwrap()).unwrap(),
            canonical_bytes(&expected).unwrap()
        );
    }
}
#[test]
fn previously_readable_nesting_remains_readable_and_excess_is_bounded() {
    let mut readable = 0;
    for depth in 0..=132 {
        let raw = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
        if let Ok(previous) = serde_json::from_str::<Value>(&raw) {
            readable += 1;
            assert_eq!(
                persisted_json::parse(raw.as_bytes()).unwrap(),
                previous,
                "depth {depth}"
            );
        }
    }
    assert!(readable >= 120);
    let excessive = format!("{}0{}", "[".repeat(130), "]".repeat(130));
    assert!(persisted_json::parse(excessive.as_bytes()).is_err());
    assert!(persisted_json::parse(&vec![b' '; 16 * 1024 * 1024 + 1]).is_err());
    for raw in [r#"{"a":1,"\u0061":2}"#, r#"[{"a":0,"a":1}]"#, "0 1", "[0,]"] {
        assert!(persisted_json::parse(raw.as_bytes()).is_err());
    }
}
#[test]
fn typed_events_and_pages_preserve_literal_data_in_all_value_conversions() {
    let event = event();
    conversions(&event.event);
    conversions(&event);
    let page = EventPage::Events {
        events: vec![event],
        next_cursor: Cursor {
            version: 1,
            snapshot: SnapshotId::parse("snapshot").unwrap(),
            workspace: WorkspaceId::parse("workspace").unwrap(),
            session: SessionId::parse("session").unwrap(),
            after: SessionSeq::ZERO,
            end: SessionSeq::new(1),
            watermark: Watermark::new(1),
            authority: AuthorityRevision::ZERO,
            deletion: DeletionEpoch::ZERO,
            expires_at: Timestamp::new(1000),
            limit: 10,
        },
        snapshot_watermark: Watermark::new(1),
        at_end: true,
    };
    conversions(&page);
    conversions(&EventPage::Gap {
        reason: GapReason::SnapshotExpired,
        restart_from_snapshot: true,
    });
    let mut additive = serde_json::to_value(&page).unwrap();
    additive["extra"] = true.into();
    let _: EventPage = serde_json::from_value(additive).unwrap();
    assert!(serde_json::from_str::<EventPage>(
        r#"{"status":"gap","reason":"snapshot_expired","restart_from_snapshot":true,"extra":0}"#
    )
    .is_ok());
    for raw in [
        r#"{"status":"gap","status":"events","reason":"snapshot_expired","restart_from_snapshot":true}"#,
        r#"{"status":"other"}"#,
        r#"{"status":"events","events":[]}"#,
    ] {
        assert!(serde_json::from_str::<EventPage>(raw).is_err());
    }
}
