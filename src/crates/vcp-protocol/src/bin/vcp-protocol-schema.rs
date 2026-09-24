// SPDX-License-Identifier: Apache-2.0
//! Emit the canonical public wire schema without changing generated files.
use std::io::{self, Write};

// These bytes are captured by rustc, not read from the checkout at execution.
// Hashing normalized newlines makes the generated artifact reproducible across
// Windows/Linux checkouts without blessing a stale exporter after source edits.
const SOURCES: &[(&str, &str)] = &[
    (
        "src/crates/vcp-protocol/src/editor.rs",
        include_str!("../editor.rs"),
    ),
    (
        "src/crates/vcp-protocol/src/memory_retention.rs",
        include_str!("../memory_retention.rs"),
    ),
    (
        "src/crates/vcp-protocol/src/memory_governance.rs",
        include_str!("../memory_governance.rs"),
    ),
    (
        "src/crates/vcp-protocol/src/memory_query.rs",
        include_str!("../memory_query.rs"),
    ),
    (
        "src/crates/vcp-protocol/src/memory.rs",
        include_str!("../memory.rs"),
    ),
    (
        "src/crates/vcp-protocol/src/methods.rs",
        include_str!("../methods.rs"),
    ),
    (
        "src/crates/vcp-protocol/src/errors.rs",
        include_str!("../errors.rs"),
    ),
    (
        "src/crates/vcp-protocol/src/handshake.rs",
        include_str!("../handshake.rs"),
    ),
    (
        "src/crates/vcp-protocol/src/jsonrpc.rs",
        include_str!("../jsonrpc.rs"),
    ),
    (
        "src/crates/vcp-protocol/src/lib.rs",
        include_str!("../lib.rs"),
    ),
    (
        "src/crates/vcp-protocol/src/bin/vcp-protocol-schema.rs",
        include_str!("vcp-protocol-schema.rs"),
    ),
    (
        "src/crates/vcp-protocol/Cargo.toml",
        include_str!("../../Cargo.toml"),
    ),
    (
        "src/third_party/codex/codex-rs/Cargo.lock",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../third_party/codex/codex-rs/Cargo.lock"
        )),
    ),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut schema = schemars::schema_for!(vcp_protocol::methods::PublicApi);
    let sources: std::collections::BTreeMap<_, _> = SOURCES
        .iter()
        .map(|(path, source)| {
            (
                *path,
                vcp_protocol::digest_bytes(source.replace("\r\n", "\n").as_bytes()),
            )
        })
        .collect();
    let provenance = serde_json::json!({
        "format": "vcp-schema-sources/1",
        "sources": sources,
    });
    schema.schema.extensions.insert(
        "$comment".into(),
        serde_json::Value::String(serde_json::to_string(&provenance)?),
    );
    let stdout = io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer_pretty(&mut output, &schema)?;
    output.write_all(b"\n")?;
    Ok(())
}
