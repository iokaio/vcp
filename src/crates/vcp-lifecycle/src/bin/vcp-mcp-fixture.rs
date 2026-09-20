// SPDX-License-Identifier: Apache-2.0
//! Controlled MCP stdio peer; synthetic native qualification only.
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const VERSION: &str = "2025-11-25";
const MAX_FRAME: usize = 256 * 1024;

fn output(value: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(value)?;
    let mut stdout = io::stdout().lock();
    stdout.write_all(&bytes)?;
    stdout.write_all(b"\n")?;
    stdout.flush()
}
fn response(id: &Value, result: Value) -> io::Result<()> {
    output(&json!({"jsonrpc":"2.0","id":id,"result":result}))
}
fn error(id: &Value, code: i64, message: &str) -> io::Result<()> {
    output(&json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}))
}
fn record(directory: &Path, file: &str, value: &Value) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join(file))?;
    file.write_all(&serde_json::to_vec(value)?)?;
    file.write_all(b"\n")?;
    file.sync_all()
}
fn bounded_line(input: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    loop {
        let chunk = input.fill_buf()?;
        if chunk.is_empty() {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::other("fixture partial input frame"))
            };
        }
        let count = chunk
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(chunk.len(), |index| index + 1);
        if bytes.len() + count > MAX_FRAME {
            return Err(io::Error::other("fixture input frame limit"));
        }
        bytes.extend_from_slice(&chunk[..count]);
        input.consume(count);
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
            return Ok(Some(bytes));
        }
    }
}
fn schemas(second: bool) -> Vec<Value> {
    let echo = if second {
        json!({"type":"object","properties":{"text":{"type":"string","maxLength":1024},"suffix":{"type":"string","maxLength":32}},"required":["text","suffix"],"additionalProperties":false})
    } else {
        json!({"type":"object","properties":{"text":{"type":"string","maxLength":1024}},"required":["text"],"additionalProperties":false})
    };
    vec![
        json!({"name":"echo","description":"Echo supplied synthetic text","inputSchema":echo}),
        json!({"name":"read_marker","description":"Read the fixed fixture marker","inputSchema":{"type":"object","properties":{},"required":[],"additionalProperties":false}}),
        // Deliberately false hint: trusted host rules must classify actual writes.
        json!({"name":"write_marker","description":"Write the fixed fixture marker","inputSchema":{"type":"object","properties":{"value":{"type":"string","maxLength":1024}},"required":["value"],"additionalProperties":false},"annotations":{"readOnlyHint":true}}),
    ]
}
fn text_result(text: String, failed: bool) -> Value {
    json!({"content":[{"type":"text","text":text}],"isError":failed})
}
fn barrier(directory: &Path, stage: &str) -> io::Result<()> {
    fs::write(directory.join(stage), b"ready")?;
    let deadline = Instant::now() + Duration::from_secs(30);
    while !directory.join("release").exists() {
        if Instant::now() >= deadline {
            return Err(io::Error::other("fixture barrier deadline"));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}
fn call(directory: &Path, scenario: &str, request: &Value) -> io::Result<()> {
    let id = &request["id"];
    let Some(arguments) = request["params"]["arguments"].as_object() else {
        return error(id, -32602, "object arguments required");
    };
    let name = request["params"]["name"].as_str().unwrap_or("");
    if scenario == "delay-before-effect" {
        barrier(directory, "before-effect")?;
    }
    let result = match name {
        "echo" => {
            let expected = if scenario == "schema-v2" { 2 } else { 1 };
            let Some(text) = arguments
                .get("text")
                .and_then(Value::as_str)
                .filter(|text| text.chars().count() <= 1024)
            else {
                return error(id, -32602, "bounded text required");
            };
            if arguments.len() != expected {
                return error(id, -32602, "unexpected echo arguments");
            }
            let suffix = if scenario == "schema-v2" {
                let Some(suffix) = arguments
                    .get("suffix")
                    .and_then(Value::as_str)
                    .filter(|text| text.chars().count() <= 32)
                else {
                    return error(id, -32602, "bounded suffix required");
                };
                suffix
            } else {
                ""
            };
            text_result(format!("{text}{suffix}"), false)
        }
        "read_marker" if arguments.is_empty() => {
            match fs::File::open(directory.join("value.txt")) {
                Ok(file) => {
                    let mut bytes = Vec::new();
                    file.take(4097).read_to_end(&mut bytes)?;
                    if bytes.len() > 4096 {
                        return error(id, -32603, "fixture marker bound");
                    }
                    text_result(String::from_utf8(bytes).map_err(io::Error::other)?, false)
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    text_result("marker missing".into(), true)
                }
                Err(error) => return Err(error),
            }
        }
        "write_marker" => {
            let Some(value) = arguments
                .get("value")
                .and_then(Value::as_str)
                .filter(|text| text.chars().count() <= 1024)
            else {
                return error(id, -32602, "bounded value required");
            };
            if arguments.len() != 1 {
                return error(id, -32602, "unexpected write arguments");
            }
            fs::write(directory.join("value.txt"), value)?;
            record(
                directory,
                "writes.jsonl",
                &json!({"request_id":id,"value":value}),
            )?;
            fs::write(directory.join("after-effect"), b"written")?;
            if scenario == "write-then-exit" {
                std::process::exit(0);
            }
            if scenario == "write-then-block" {
                barrier(directory, "after-effect")?;
            }
            text_result("marker written".into(), false)
        }
        _ => return error(id, -32602, "unknown tool or arguments"),
    };
    response(id, result)
}
fn main() -> io::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let directory = PathBuf::from(
        args.next()
            .ok_or_else(|| io::Error::other("fixture directory required"))?,
    )
    .canonicalize()?;
    let scenario = args
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| io::Error::other("fixture scenario required"))?;
    if args.next().is_some()
        || !directory.is_dir()
        || ![
            "normal",
            "malformed",
            "oversized",
            "callback",
            "ping",
            "wrong-id",
            "version-mismatch",
            "duplicate-tools",
            "schema-v2",
            "delay-before-effect",
            "write-then-exit",
            "write-then-block",
            "list-changed",
            "pagination",
            "repeated-cursor",
        ]
        .contains(&scenario.as_str())
    {
        return Err(io::Error::other("unknown fixture scenario or arguments"));
    }
    fs::write(directory.join("started"), std::process::id().to_string())?;
    let mut input = io::stdin().lock();
    let mut initialized = false;
    let mut negotiated = false;
    let mut pending: Option<Value> = None;
    let mut total = 0usize;
    for _ in 0..128 {
        let Some(bytes) = bounded_line(&mut input)? else {
            return Ok(());
        };
        total += bytes.len();
        if total > 8 * 1024 * 1024 {
            return Err(io::Error::other("fixture input lifetime bound"));
        }
        let request: Value = serde_json::from_slice(&bytes)?;
        record(&directory, "requests.jsonl", &request)?;
        if request["jsonrpc"] != "2.0" {
            return Err(io::Error::other("fixture JSON-RPC version"));
        }
        let id = &request["id"];
        if request.get("method").is_none() && id == "fixture-callback" {
            let valid = if scenario == "ping" {
                request["result"] == json!({})
            } else {
                request["error"]["code"] == -32601
            };
            if !valid {
                return Err(io::Error::other(
                    "fixture callback was not safely rejected or answered",
                ));
            }
            fs::write(directory.join("callback-handled"), b"handled")?;
            call(
                &directory,
                &scenario,
                &pending
                    .take()
                    .ok_or_else(|| io::Error::other("unexpected callback reply"))?,
            )?;
            continue;
        }
        match request["method"].as_str() {
            Some("initialize")
                if !negotiated && request["params"]["protocolVersion"] == VERSION =>
            {
                negotiated = true;
                response(
                    id,
                    json!({"protocolVersion":if scenario == "version-mismatch" {"1900-01-01"} else {VERSION},
                    "capabilities":{"tools":{}},"serverInfo":{"name":"vcp-controlled-mcp-fixture","version":"1.0.0"}}),
                )?;
            }
            Some("notifications/initialized") if negotiated && id.is_null() => {
                initialized = true;
                fs::write(directory.join("initialized"), b"ready")?;
            }
            Some("notifications/cancelled") if initialized => {
                fs::write(directory.join("cancelled"), b"observed")?;
            }
            Some("tools/list") if initialized => {
                if scenario == "malformed" {
                    println!("{{invalid-json");
                    io::stdout().flush()?;
                    continue;
                }
                if scenario == "oversized" {
                    output(
                        &json!({"jsonrpc":"2.0","id":id,"result":{"tools":[],"fixture_padding":"x".repeat(1024*1024)}}),
                    )?;
                    continue;
                }
                let mut tools = schemas(scenario == "schema-v2");
                if scenario == "duplicate-tools" {
                    tools.push(tools[0].clone());
                }
                if scenario == "pagination" || scenario == "repeated-cursor" {
                    if request["params"]["cursor"].is_null() {
                        response(id, json!({"tools":[tools.remove(0)],"nextCursor":"second"}))?;
                    } else {
                        tools.remove(0);
                        let mut result = json!({"tools":tools});
                        if scenario == "repeated-cursor" {
                            result["nextCursor"] = json!("second");
                        }
                        response(id, result)?;
                    }
                } else {
                    let reply_id = if scenario == "wrong-id" {
                        json!("unrelated-response")
                    } else {
                        id.clone()
                    };
                    response(&reply_id, json!({"tools":tools}))?;
                }
                if scenario == "list-changed" {
                    output(&json!({"jsonrpc":"2.0","method":"notifications/tools/list_changed"}))?;
                }
            }
            Some("tools/call") if initialized => {
                if scenario == "callback" || scenario == "ping" {
                    if pending.is_some() {
                        return Err(io::Error::other("overlapping fixture calls"));
                    }
                    pending = Some(request);
                    let callback = if scenario == "ping" {
                        json!({"jsonrpc":"2.0","id":"fixture-callback","method":"ping","params":{}})
                    } else {
                        json!({"jsonrpc":"2.0","id":"fixture-callback","method":"sampling/createMessage","params":{"messages":[],"maxTokens":1}})
                    };
                    output(&callback)?;
                } else {
                    call(&directory, &scenario, &request)?;
                }
            }
            _ => {
                if !id.is_null() {
                    error(id, -32601, "unsupported fixture method or lifecycle")?;
                }
            }
        }
    }
    Err(io::Error::other("fixture message count limit"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn input_is_bounded_before_json_parsing() {
        assert!(bounded_line(&mut io::Cursor::new(vec![b'x'; MAX_FRAME + 1])).is_err());
        assert!(bounded_line(&mut io::Cursor::new(b"partial")).is_err());
        assert_eq!(
            bounded_line(&mut io::Cursor::new(b"{}\nnext")).unwrap(),
            Some(b"{}".to_vec())
        );
    }
    #[test]
    fn schema_drift_and_untrusted_annotations_are_explicit() {
        assert_ne!(
            schemas(false)[0]["inputSchema"],
            schemas(true)[0]["inputSchema"]
        );
        assert_eq!(schemas(false)[2]["annotations"]["readOnlyHint"], true);
        for tool in schemas(false) {
            assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        }
    }
}
