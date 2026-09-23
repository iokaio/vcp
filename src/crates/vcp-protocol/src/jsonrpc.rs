// SPDX-License-Identifier: Apache-2.0
//! Bounded JSON-line transport and JSON-RPC 2.0 envelopes; no dispatch or authority.
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
/// Absolute lexical parser ceiling; negotiated transports may choose less.
pub const MAX_FRAME_BYTES: u32 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    #[error("frame exceeds the byte limit")]
    TooLarge,
    #[error("frame is not UTF-8")]
    InvalidUtf8,
    #[error("stream ended within a frame")]
    Truncated,
    #[error("decoder is closed after a framing error")]
    Closed,
}

/// Limits include an optional CR but exclude LF. Fatal framing errors poison the
/// connection; callers must close it, never resume at a later newline.
pub struct LineDecoder {
    maximum: usize,
    buffer: Vec<u8>,
    closed: bool,
}

impl LineDecoder {
    pub fn new(maximum: usize) -> Self {
        Self {
            maximum,
            buffer: Vec::new(),
            closed: false,
        }
    }

    /// Completed frames preceding an error remain in order. Invalid JSON is an
    /// envelope error, not a framing error. Split UTF-8 codepoints are retained.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Result<String, FrameError>> {
        if self.closed {
            return vec![Err(FrameError::Closed)];
        }
        let mut frames = Vec::new();
        for &byte in bytes {
            if byte == b'\n' {
                if self.buffer.last() == Some(&b'\r') {
                    self.buffer.pop();
                }
                let raw = std::mem::take(&mut self.buffer);
                match String::from_utf8(raw) {
                    Ok(frame) => frames.push(Ok(frame)),
                    Err(_) => {
                        self.closed = true;
                        frames.push(Err(FrameError::InvalidUtf8));
                        break;
                    }
                }
            } else if self.buffer.len() == self.maximum {
                self.closed = true;
                self.buffer.clear();
                frames.push(Err(FrameError::TooLarge));
                break;
            } else {
                self.buffer.push(byte);
            }
        }
        frames
    }

    pub fn finish(&mut self) -> Result<(), FrameError> {
        if self.closed {
            return Err(FrameError::Closed);
        }
        self.closed = true;
        if self.buffer.is_empty() {
            Ok(())
        } else {
            self.buffer.clear();
            Err(FrameError::Truncated)
        }
    }
}

/// Numeric request IDs use the JavaScript exact integer range. Opaque IDs should
/// use strings. Explicit null is a request ID; an absent ID is a notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum RequestId {
    String(String),
    Integer(i64),
    Null,
}

#[cfg(feature = "schema")]
impl schemars::JsonSchema for RequestId {
    fn schema_name() -> String {
        "RequestId".into()
    }
    fn json_schema(_: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::{
            InstanceType, NumberValidation, Schema, SchemaObject, SubschemaValidation,
        };
        let string = SchemaObject {
            instance_type: Some(InstanceType::String.into()),
            ..Default::default()
        };
        let integer = SchemaObject {
            instance_type: Some(InstanceType::Integer.into()),
            number: Some(Box::new(NumberValidation {
                minimum: Some(-MAX_SAFE_INTEGER as f64),
                maximum: Some(MAX_SAFE_INTEGER as f64),
                ..Default::default()
            })),
            ..Default::default()
        };
        let null = SchemaObject {
            instance_type: Some(InstanceType::Null.into()),
            ..Default::default()
        };
        Schema::Object(SchemaObject {
            subschemas: Some(Box::new(SubschemaValidation {
                any_of: Some(vec![string.into(), integer.into(), null.into()]),
                ..Default::default()
            })),
            ..Default::default()
        })
    }
}

impl<'de> Deserialize<'de> for RequestId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_value(Value::deserialize(deserializer)?)
            .ok_or_else(|| serde::de::Error::custom("ID must be a string, null, or safe integer"))
    }
}

impl RequestId {
    fn from_value(value: Value) -> Option<Self> {
        match value {
            Value::String(value) => Some(Self::String(value)),
            Value::Null => Some(Self::Null),
            Value::Number(value) => value
                .as_i64()
                .filter(|v| (-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(v))
                .map(Self::Integer),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ErrorData {
    pub kind: String,
    pub details: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<ErrorData>,
}

impl RpcError {
    pub fn parse_error() -> Self {
        Self::standard(-32700, "Parse error")
    }
    pub fn invalid_request() -> Self {
        Self::standard(-32600, "Invalid Request")
    }
    pub fn method_not_found() -> Self {
        Self::standard(-32601, "Method not found")
    }
    pub fn invalid_params() -> Self {
        Self::standard(-32602, "Invalid params")
    }
    pub fn internal_error() -> Self {
        Self::standard(-32603, "Internal error")
    }
    fn standard(code: i32, message: &str) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
    pub fn application(kind: &str, details: Value) -> Self {
        Self {
            code: -32000,
            message: "VCP request rejected".into(),
            data: Some(ErrorData {
                kind: kind.into(),
                details,
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub id: Option<RequestId>,
    pub method: String,
    pub params: Option<Value>,
}

impl Request {
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }
    /// Enforces the standard's no-response rule, including failed notifications.
    pub fn respond(&self, outcome: Result<Value, RpcError>) -> Option<Response> {
        self.id.clone().map(|id| Response { id, outcome })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    pub id: RequestId,
    pub outcome: Result<Value, RpcError>,
}

impl Response {
    pub fn invalid(error: RpcError) -> Self {
        Self {
            id: RequestId::Null,
            outcome: Err(error),
        }
    }
}

impl Serialize for Request {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut value = serializer.serialize_map(Some(
            2 + usize::from(self.id.is_some()) + usize::from(self.params.is_some()),
        ))?;
        value.serialize_entry("jsonrpc", "2.0")?;
        value.serialize_entry("method", &self.method)?;
        if let Some(id) = &self.id {
            value.serialize_entry("id", id)?;
        }
        if let Some(params) = &self.params {
            value.serialize_entry("params", params)?;
        }
        value.end()
    }
}

impl Serialize for Response {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut value = serializer.serialize_map(Some(3))?;
        value.serialize_entry("jsonrpc", "2.0")?;
        value.serialize_entry("id", &self.id)?;
        match &self.outcome {
            Ok(result) => value.serialize_entry("result", result)?,
            Err(error) => value.serialize_entry("error", error)?,
        }
        value.end()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Message {
    Request(Request),
    Response(Response),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Envelope {
    Single(Result<Message, RpcError>),
    Batch(Vec<Result<Message, RpcError>>),
}

/// Empty batches produce a single Invalid Request. Nonempty batches preserve
/// each entry, including invalid entries. Dispatchers process entries separately,
/// suppress notification replies, and send no frame for an all-notification batch.
pub fn parse_frame(frame: &str) -> Envelope {
    // Reuse the bounded lexical decoder: duplicate keys and serde-private map
    // spellings must never change interpretation between admission and dispatch.
    // Its 16 MiB absolute ceiling also applies beyond the transport frame limit.
    match crate::persisted_json::parse(frame.as_bytes()) {
        Err(_) => Envelope::Single(Err(RpcError::parse_error())),
        Ok(Value::Array(values)) if !values.is_empty() => {
            Envelope::Batch(values.into_iter().map(parse_message).collect())
        }
        Ok(value) => Envelope::Single(parse_message(value)),
    }
}

fn parse_message(value: Value) -> Result<Message, RpcError> {
    let invalid = RpcError::invalid_request;
    let object = value.as_object().ok_or_else(invalid)?;
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "jsonrpc" | "id" | "method" | "params" | "result" | "error"
        )
    }) {
        return Err(invalid());
    }
    if object.get("jsonrpc") != Some(&json!("2.0")) {
        return Err(invalid());
    }
    let id = object
        .get("id")
        .map(|id| RequestId::from_value(id.clone()).ok_or_else(invalid))
        .transpose()?;
    if let Some(method) = object.get("method") {
        if object.contains_key("result") || object.contains_key("error") {
            return Err(invalid());
        }
        let method = method.as_str().ok_or_else(invalid)?.to_owned();
        let params = object.get("params").cloned();
        if params
            .as_ref()
            .is_some_and(|p| !p.is_object() && !p.is_array())
        {
            return Err(invalid());
        }
        return Ok(Message::Request(Request { id, method, params }));
    }
    let id = id.ok_or_else(invalid)?;
    if object.contains_key("params") {
        return Err(invalid());
    }
    let outcome = match (object.get("result"), object.get("error")) {
        (Some(result), None) => Ok(result.clone()),
        (None, Some(error)) => Err(serde_json::from_value(error.clone()).map_err(|_| invalid())?),
        _ => return Err(invalid()),
    };
    Ok(Message::Response(Response { id, outcome }))
}

/// Bounded output uses the same byte ceiling as input, before appending LF.
pub fn encode_frame(value: &impl Serialize, maximum: usize) -> Result<Vec<u8>, RpcError> {
    struct Bounded {
        bytes: Vec<u8>,
        maximum: usize,
        exceeded: bool,
    }
    impl std::io::Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.maximum.saturating_sub(self.bytes.len()) {
                self.exceeded = true;
                return Err(std::io::Error::other("frame byte limit"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut output = Bounded {
        bytes: Vec::new(),
        maximum,
        exceeded: false,
    };
    if serde_json::to_writer(&mut output, value).is_err() {
        return Err(if output.exceeded {
            RpcError::application("frame_too_large", json!({"maximum":maximum}))
        } else {
            RpcError::internal_error()
        });
    }
    output.bytes.push(b'\n');
    Ok(output.bytes)
}

/// A batch containing only notifications has no response frame (not `[]`).
pub fn encode_batch_responses(
    responses: &[Response],
    maximum: usize,
) -> Result<Option<Vec<u8>>, RpcError> {
    if responses.is_empty() {
        Ok(None)
    } else {
        encode_frame(&responses, maximum).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_utf8_crlf_and_escaped_newlines() {
        let bytes = "{\"text\":\"é\\n中\"}\r\n{}\n".as_bytes();
        let mut decoder = LineDecoder::new(100);
        let frames: Vec<_> = bytes
            .iter()
            .flat_map(|byte| decoder.push(&[*byte]))
            .collect();
        assert_eq!(
            frames,
            vec![Ok("{\"text\":\"é\\n中\"}".into()), Ok("{}".into())]
        );
        assert_eq!(decoder.finish(), Ok(()));
    }

    #[test]
    fn unknown_envelope_fields_and_response_params_reject() {
        for value in [
            json!({"jsonrpc":"2.0","method":"task/read","id":1,"grant_controller":true}),
            json!({"jsonrpc":"2.0","id":1,"result":{},"params":{}}),
            json!({"jsonrpc":"2.0","id":1,"result":{},"future_authority":true}),
            json!({"jsonrpc":"2.0","id":1,"error":{"code":-32600,"message":"Invalid Request"},"params":[]}),
        ] {
            assert_eq!(
                parse_frame(&value.to_string()),
                Envelope::Single(Err(RpcError::invalid_request()))
            );
        }
    }

    #[test]
    fn output_limit_stops_streaming_serialization_before_materializing_payload() {
        use serde::ser::SerializeSeq;
        use std::cell::Cell;
        struct Huge<'a>(&'a Cell<usize>);
        impl Serialize for Huge<'_> {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut sequence = serializer.serialize_seq(Some(1_000_000_000))?;
                for _ in 0..1_000_000_000 {
                    self.0.set(self.0.get() + 1);
                    sequence.serialize_element("chunk")?;
                }
                sequence.end()
            }
        }
        let visits = Cell::new(0);
        let error = encode_frame(&Huge(&visits), 64).unwrap_err();
        assert_eq!(error.data.unwrap().kind, "frame_too_large");
        assert!(
            visits.get() <= 10,
            "serializer must stop at the byte boundary"
        );
        assert_eq!(encode_frame(&json!("abcd"), 6).unwrap(), b"\"abcd\"\n");
        assert!(encode_frame(&json!("abcde"), 6).is_err());
        assert!(encode_frame(&json!(null), 0).is_err());
    }

    #[test]
    fn limit_is_bytes_and_connection_cannot_recover_from_framing_errors() {
        let mut decoder = LineDecoder::new(2);
        assert_eq!(
            decoder.push(b"{}\nabc\n{}\n"),
            vec![Ok("{}".into()), Err(FrameError::TooLarge)]
        );
        assert_eq!(decoder.push(b"{}\n"), vec![Err(FrameError::Closed)]);
        let mut decoder = LineDecoder::new(3);
        assert_eq!(
            decoder.push(&[0xff, b'\n']),
            vec![Err(FrameError::InvalidUtf8)]
        );
        let mut decoder = LineDecoder::new(3);
        assert!(decoder.push(b"{}").is_empty());
        assert_eq!(decoder.finish(), Err(FrameError::Truncated));
        let mut decoder = LineDecoder::new(1);
        assert_eq!(
            decoder.push("é\n".as_bytes()),
            vec![Err(FrameError::TooLarge)]
        );
    }

    #[test]
    fn malformed_envelopes_and_safe_ids() {
        for frame in [
            "[]",
            "null",
            "{}",
            r#"{"jsonrpc":"1.0","method":"x"}"#,
            r#"{"jsonrpc":"2.0","method":"x","params":null}"#,
            r#"{"jsonrpc":"2.0","method":"x","id":9007199254740992}"#,
            r#"{"jsonrpc":"2.0","method":"x","id":1.5}"#,
            r#"{"jsonrpc":"2.0","id":1,"result":null,"error":{}}"#,
        ] {
            assert_eq!(
                parse_frame(frame),
                Envelope::Single(Err(RpcError::invalid_request())),
                "{frame}"
            );
        }
        assert_eq!(
            parse_frame("{"),
            Envelope::Single(Err(RpcError::parse_error()))
        );
        for id in [
            json!(MAX_SAFE_INTEGER),
            json!(-MAX_SAFE_INTEGER),
            json!("18446744073709551615"),
            Value::Null,
        ] {
            let frame = json!({"jsonrpc":"2.0","method":"x","id":id}).to_string();
            let Envelope::Single(Ok(Message::Request(request))) = parse_frame(&frame) else {
                panic!("valid request rejected");
            };
            assert!(!request.is_notification());
            let response = request.respond(Ok(Value::Null)).unwrap();
            assert_eq!(serde_json::to_value(response).unwrap()["id"], id);
        }
    }

    #[test]
    fn batch_preserves_invalid_entries_and_never_answers_notifications() {
        let Envelope::Batch(entries) = parse_frame(
            r#"[{"jsonrpc":"2.0","method":"unknown"},1,{"jsonrpc":"2.0","method":"x","id":null}]"#,
        ) else {
            panic!("expected batch");
        };
        let Ok(Message::Request(notification)) = &entries[0] else {
            panic!("notification");
        };
        assert!(notification
            .respond(Err(RpcError::method_not_found()))
            .is_none());
        assert_eq!(entries[1], Err(RpcError::invalid_request()));
        let Ok(Message::Request(request)) = &entries[2] else {
            panic!("request");
        };
        assert!(request.respond(Ok(json!(1))).is_some());
    }

    #[test]
    fn response_roundtrip_keeps_null_result_and_output_limit() {
        let response = Response {
            id: RequestId::String("second".into()),
            outcome: Ok(Value::Null),
        };
        let bytes = encode_frame(&response, 100).unwrap();
        assert_eq!(
            parse_frame(std::str::from_utf8(&bytes).unwrap()),
            Envelope::Single(Ok(Message::Response(response)))
        );
        assert!(encode_frame(&json!({"x":"large"}), 2).is_err());
        assert_eq!(encode_batch_responses(&[], 100).unwrap(), None);
    }

    #[test]
    fn duplicate_keys_and_private_serde_number_objects_cannot_override_ids() {
        assert_eq!(
            parse_frame(r#"{"jsonrpc":"2.0","method":"read","method":"write","id":1}"#),
            Envelope::Single(Err(RpcError::parse_error()))
        );
        assert_eq!(
            parse_frame(
                r#"{"jsonrpc":"2.0","method":"x","id":{"$serde_json::private::Number":"1"}}"#
            ),
            Envelope::Single(Err(RpcError::invalid_request()))
        );
    }
}
