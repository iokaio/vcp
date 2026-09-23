// SPDX-License-Identifier: Apache-2.0
//! Golden compatibility/decoding traces. These never execute engine commands.
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use vcp_protocol::{
    handshake::{negotiate, ConnectionLimits, ExecutionHost, InitializeParams, ServerInfo},
    jsonrpc::{
        encode_frame, parse_frame, Envelope, LineDecoder, Message, Request, RequestId, RpcError,
    },
    methods::{Call, RequestEnvelope},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixtures {
    profile: String,
    purpose: String,
    server: ServerFixture,
    handshakes: Vec<Case>,
    calls: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ServerFixture {
    engine_build: String,
    capabilities: BTreeSet<String>,
    methods: Vec<String>,
    limits: ConnectionLimits,
    execution_host: ExecutionHost,
    sandbox_capabilities: Vec<String>,
}

impl From<ServerFixture> for ServerInfo {
    fn from(value: ServerFixture) -> Self {
        Self {
            engine_build: value.engine_build,
            capabilities: value.capabilities,
            methods: value.methods,
            limits: value.limits,
            execution_host: value.execution_host,
            sandbox_capabilities: value.sandbox_capabilities,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    input: Value,
    expected_status: String,
    expected_response: Option<Value>,
}

fn fixtures() -> Fixtures {
    let fixtures: Fixtures =
        serde_json::from_str(include_str!("../../../tests/fixtures/protocol/v1.json")).unwrap();
    assert_eq!(fixtures.profile, "vcp-public/1.0");
    assert!(fixtures.purpose.contains("no engine execution"));
    fixtures
}

/// Exercise byte framing and envelope parsing before typed method decoding.
fn request(case: &Case) -> Request {
    let encoded = encode_frame(&case.input, 1024 * 1024).unwrap();
    let mut decoder = LineDecoder::new(1024 * 1024);
    let frames: Vec<_> = encoded
        .chunks(7)
        .flat_map(|chunk| decoder.push(chunk))
        .collect();
    decoder.finish().unwrap();
    assert_eq!(frames.len(), 1, "{}", case.name);
    let frame = frames[0].as_ref().unwrap();
    let Envelope::Single(Ok(Message::Request(request))) = parse_frame(frame) else {
        panic!("{} must decode as a request", case.name);
    };
    assert_eq!(
        serde_json::to_value(&request).unwrap(),
        case.input,
        "{} envelope roundtrip",
        case.name
    );
    request
}

fn assert_response(case: &Case, request: &Request, result: Result<Value, RpcError>) {
    let response = request.respond(result).expect("fixture has a request ID");
    let encoded = encode_frame(&response, 1024 * 1024).unwrap();
    let actual: Value = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(
        Some(&actual),
        case.expected_response.as_ref(),
        "{} response",
        case.name
    );
    assert_eq!(
        parse_frame(std::str::from_utf8(&encoded).unwrap()),
        Envelope::Single(Ok(Message::Response(response))),
        "{} response roundtrip",
        case.name
    );
}

#[test]
fn golden_old_new_and_incompatible_peers_have_exact_responses() {
    let fixtures = fixtures();
    let server = ServerInfo::from(fixtures.server);
    let mut seen = BTreeSet::new();
    for case in fixtures.handshakes {
        assert!(seen.insert(case.name.clone()), "duplicate case");
        let request = request(&case);
        assert_eq!(request.method, "initialize");
        let params: InitializeParams =
            serde_json::from_value(request.params.clone().unwrap()).unwrap();
        let result = negotiate(&params, &server);
        let actual_status = match &result {
            Ok(_) => "negotiated",
            Err(error) if error.code == -32602 => "invalid_params",
            Err(error) => {
                &error
                    .data
                    .as_ref()
                    .expect("structured negotiation error")
                    .kind
            }
        };
        assert_eq!(actual_status, case.expected_status, "{}", case.name);
        assert_response(
            &case,
            &request,
            result.map(|result| serde_json::to_value(result).unwrap()),
        );
    }
    assert_eq!(seen.len(), 5);
}

#[test]
fn golden_calls_preserve_exact_counters_and_fail_closed_before_execution() {
    let fixtures = fixtures();
    let mut seen = BTreeSet::new();
    for case in fixtures.calls {
        assert!(seen.insert(case.name.clone()), "duplicate case");
        let request = request(&case);
        let result = Call::decode(&request.method, request.params.clone().unwrap());
        match case.expected_status.as_str() {
            "decoded" => {
                let call = result.unwrap_or_else(|error| panic!("{}: {error}", case.name));
                // This is decoded data, deliberately not a fabricated success
                // response or evidence that a command was durably accepted.
                assert_eq!(
                    serde_json::to_value(call).unwrap(),
                    json!({"method":request.method,"params":request.params}),
                    "{} typed roundtrip",
                    case.name
                );
                assert!(case.expected_response.is_none());
            }
            "invalid_params" => {
                assert!(result.is_err(), "{} unexpectedly admitted", case.name);
                assert_response(&case, &request, Err(RpcError::invalid_params()));
            }
            other => panic!("unrecognized fixture outcome {other}"),
        }
        if case.name == "null_request_id" {
            assert_eq!(request.id, Some(RequestId::Null));
            assert!(!request.is_notification());
            let typed: RequestEnvelope = serde_json::from_value(case.input.clone()).unwrap();
            assert_eq!(typed.id, Some(RequestId::Null));
            assert_eq!(serde_json::to_value(typed).unwrap(), case.input);
        }
    }
    assert_eq!(seen.len(), 13);
}
