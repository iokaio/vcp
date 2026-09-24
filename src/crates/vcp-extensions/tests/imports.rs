// SPDX-License-Identifier: Apache-2.0
use std::collections::BTreeSet;
use vcp_extensions::import::{
    apply::{apply, rollback},
    compatibility::Format,
    normalize::{effective, Context, Server, Transport},
    preview::{preview, Status},
    Preferences, Restriction,
};
fn context() -> Context {
    Context {
        servers: vec![Server {
            name: "docs".into(),
            transport: Transport::Http {
                endpoint: "https://example.invalid/mcp".into(),
                credential_environment: None,
            },
            allowed_tools: ["read".into(), "write".into()].into(),
            timeout_ms: 10000,
        }],
    }
}
#[test]
fn pinned_goldens_preview_select_and_rollback() {
    for (format, source) in [
        (
            Format::Codex8b78600d,
            include_bytes!("fixtures/imports/codex.toml").as_slice(),
        ),
        (
            Format::Gemini6a466a7e,
            include_bytes!("fixtures/imports/gemini.json").as_slice(),
        ),
    ] {
        let mut context = context();
        let prior = Preferences::default();
        let plan = preview(source, &format, &context, &prior).unwrap();
        assert_eq!(plan.changes.len(), 2);
        plan.validate().unwrap();
        let selected = ["server.docs.allowed_tools".into()].into();
        let applied = apply(&plan, &selected, &context, &prior).unwrap();
        assert_eq!(
            applied.servers["docs"].allowed_tools,
            Some(["read".into()].into())
        );
        assert_eq!(applied.servers["docs"].timeout_ms, None);
        let effective = effective(&context, &applied).unwrap();
        assert_eq!(effective["docs"].timeout_ms, Some(10000));
        context.servers[0].allowed_tools.clear();
        context.servers[0].timeout_ms = 100;
        let restored = rollback(&prior, &context).unwrap();
        assert_eq!(
            restored.servers["docs"],
            Restriction {
                allowed_tools: Some(BTreeSet::new()),
                timeout_ms: Some(100)
            }
        );
        assert!(apply(&plan, &selected, &context, &prior).is_err());
    }
}
#[test]
fn unknown_fields_keys_values_and_credentials_are_not_echoed() {
    let input=br#"{"secret-server-name":{"secret-key":"secret-value"},"mcpServers":{"docs":{"url":"https://example.invalid/mcp","type":"http","headers":{"Authorization":"Bearer secret-token"},"timeout":1}}}"#;
    let plan = preview(
        input,
        &Format::Gemini6a466a7e,
        &context(),
        &Preferences::default(),
    )
    .unwrap();
    assert!(plan.changes.is_empty());
    assert!(plan
        .diagnostics
        .iter()
        .any(|d| d.status == Status::Conflicting));
    let serialized = serde_json::to_string(&plan).unwrap();
    for secret in [
        "secret-server-name",
        "secret-key",
        "secret-value",
        "secret-token",
        "Authorization",
    ] {
        assert!(!serialized.contains(secret));
    }
    // Every container and scalar has its own redacted location.
    assert!(plan.diagnostics.len() >= 9);
}
#[test]
fn versions_duplicates_depth_and_malformed_fields_fail_closed() {
    assert!(Format::parse("gemini-future-v2").is_err());
    for bytes in [
        br#"{"schema_version":2}"#.as_slice(),
        br#"{"mcpServers":{},"mcpServers":{}}"#.as_slice(),
    ] {
        assert!(preview(
            bytes,
            &Format::Gemini6a466a7e,
            &context(),
            &Preferences::default()
        )
        .is_err());
    }
    let deep = format!("{}0{}", "[".repeat(100), "]".repeat(100));
    assert!(preview(
        deep.as_bytes(),
        &Format::Gemini6a466a7e,
        &context(),
        &Preferences::default()
    )
    .is_err());
    let malformed=br#"{"mcpServers":{"docs":{"url":"https://example.invalid/mcp","type":"http","timeout":0,"includeTools":["read(arguments)"]}}}"#;
    let plan = preview(
        malformed,
        &Format::Gemini6a466a7e,
        &context(),
        &Preferences::default(),
    )
    .unwrap();
    assert!(plan.changes.is_empty());
    assert!(plan
        .diagnostics
        .iter()
        .any(|d| d.status == Status::Conflicting));
}
#[test]
fn changed_preview_unknown_selection_and_changed_preferences_rejected() {
    let base = context();
    let prior = Preferences::default();
    let plan = preview(
        include_bytes!("fixtures/imports/gemini.json"),
        &Format::Gemini6a466a7e,
        &base,
        &prior,
    )
    .unwrap();
    assert!(apply(&plan, &["unknown".into()].into(), &base, &prior).is_err());
    let mut forged = plan.clone();
    forged.changes[0].new.allowed_tools = Some(["admin".into()].into());
    assert!(apply(
        &forged,
        &[forged.changes[0].id.clone()].into(),
        &base,
        &prior
    )
    .is_err());
    let mut changed = prior.clone();
    changed.servers.insert(
        "docs".into(),
        Restriction {
            allowed_tools: None,
            timeout_ms: Some(100),
        },
    );
    assert!(apply(&plan, &BTreeSet::new(), &base, &changed).is_err());
}
#[test]
fn identity_mismatch_and_sse_never_map() {
    for source in [
        r#"{"mcpServers":{"docs":{"url":"https://other.invalid/mcp","type":"http","timeout":1}}}"#,
        r#"{"mcpServers":{"docs":{"url":"https://example.invalid/mcp","timeout":1}}}"#,
    ] {
        assert!(preview(
            source.as_bytes(),
            &Format::Gemini6a466a7e,
            &context(),
            &Preferences::default()
        )
        .unwrap()
        .changes
        .is_empty());
    }
}
#[test]
fn stdio_requires_exact_command_arguments_directory_and_no_imported_environment() {
    let base = Context {
        servers: vec![Server {
            name: "docs".into(),
            transport: Transport::Stdio {
                command: "C:/qualified/server.exe".into(),
                args: vec!["--stdio".into()],
                cwd: "C:/workspace".into(),
            },
            allowed_tools: ["read".into()].into(),
            timeout_ms: 10000,
        }],
    };
    let source = r#"{"mcpServers":{"docs":{"command":"C:/qualified/server.exe","args":["--stdio"],"cwd":"C:/workspace","timeout":10}}}"#;
    assert_eq!(
        preview(
            source.as_bytes(),
            &Format::Gemini6a466a7e,
            &base,
            &Preferences::default()
        )
        .unwrap()
        .changes
        .len(),
        1
    );
    for modified in [
        source.replace("--stdio", "--execute"),
        source.replace(
            "\"timeout\":10",
            "\"env\":{\"X\":\"secret\"},\"timeout\":10",
        ),
        source.replace("C:/workspace", "../escape"),
        source.replace("\"timeout\":10", "\"type\":\"http\",\"timeout\":10"),
        source.replace("\"timeout\":10", "\"type\":\"stdio\",\"timeout\":10"),
    ] {
        assert!(preview(
            modified.as_bytes(),
            &Format::Gemini6a466a7e,
            &base,
            &Preferences::default()
        )
        .unwrap()
        .changes
        .is_empty());
    }
}

#[test]
fn conflicting_modes_and_credentials_never_match() {
    let base = context();
    for fields in [
        r#""httpUrl":"https://example.invalid/mcp","type":"sse""#,
        r#""httpUrl":"https://example.invalid/mcp","type":null"#,
        r#""url":"https://example.invalid/mcp","type":"http","args":[]"#,
        r#""url":"https://example.invalid/mcp","type":"http","cwd":"C:/workspace""#,
        r#""url":"https://example.invalid/mcp","type":"http","bearer_token":"secret-literal""#,
        r#""url":"https://example.invalid/mcp","type":"http","bearer_token_env_var":false"#,
        r#""url":"https://example.invalid/mcp","type":"http","command":"malicious.exe""#,
    ] {
        let source = format!(r#"{{"mcpServers":{{"docs":{{{fields},"timeout":1}}}}}}"#);
        let plan = preview(
            source.as_bytes(),
            &Format::Gemini6a466a7e,
            &base,
            &Preferences::default(),
        )
        .unwrap();
        assert!(plan.changes.is_empty(), "{fields}");
        assert!(!serde_json::to_string(&plan)
            .unwrap()
            .contains("secret-literal"));
    }
    let source = b"[mcp_servers.docs]\nurl='https://example.invalid/mcp'\nbearer_token_env_var='SECRET_REF'\ntool_timeout_sec=1";
    let plan = preview(
        source,
        &Format::Codex8b78600d,
        &base,
        &Preferences::default(),
    )
    .unwrap();
    assert!(plan.changes.is_empty());
    assert!(plan.diagnostics.iter().any(|d| d
        .reason
        .contains("configure the native credential reference")));
    assert!(!serde_json::to_string(&plan).unwrap().contains("SECRET_REF"));
}
