// SPDX-License-Identifier: Apache-2.0
use clap::Parser;
use std::io::{self, Write};
use vcp_cli::{
    args::{parse_usd, Cli, Command},
    exit_status::Conditions,
    jsonl::{Jsonl, Payload},
};
use vcp_domain::ids::CommandId;

#[test]
fn run_skill_selection_is_bounded_validated_and_preserves_order() {
    let parse = |skills: &[&str]| {
        let mut args = vec!["vcp", "run", "review sources", "--budget-usd", "1"];
        for skill in skills {
            args.extend(["--skill", skill]);
        }
        Cli::try_parse_from(args)
    };
    for invalid in ["", "bad\nidentity", "bad\0identity", &"a".repeat(2049)] {
        assert!(parse(&[invalid]).is_err());
    }
    let validate = |skills: &[&str]| {
        let Some(Command::Run(run)) = parse(skills).unwrap().command else {
            panic!("expected run")
        };
        run.validate(None)
    };
    let selected = ["vcp-builtin::architecture::architecture", "review"];
    assert_eq!(validate(&selected).unwrap().skills, selected);
    assert!(validate(&["review", "review"]).is_err());
    let ids: Vec<String> = (0..33).map(|i| format!("skill-{i}")).collect();
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    assert!(validate(&ids[..32]).is_ok());
    assert!(validate(&ids).is_err());
    assert!(validate(&[]).unwrap().skills.is_empty());
}

#[test]
fn malformed_commands_never_produce_typed_inputs() {
    for args in [
        vec!["vcp", "run"],
        vec!["vcp", "run", "text", "--file", "task.md"],
        vec!["vcp", "run", "text", "--unknown"],
        vec!["vcp", "resume"],
        vec!["vcp", "resume", "task", "--last"],
        vec!["vcp", "sessions", "fork", "session"],
        vec!["vcp", "tasks", "pause", "../task"],
        vec!["vcp", "inspect", "task", "--view", "invented"],
        vec!["vcp", "inspect", "../task", "--view", "costs"],
        vec!["vcp", "inspect", "task", "--view", "chain", "--limit", "0"],
        vec![
            "vcp", "inspect", "task", "--view", "chain", "--limit", "129",
        ],
        vec![
            "vcp", "inspect", "task", "--view", "prompts", "--offset", "0",
        ],
        vec![
            "vcp", "inspect", "task", "--view", "prompts", "--offset", "0", "--length", "65537",
        ],
        vec![
            "vcp", "inspect", "task", "--view", "prompts", "--offset", "0", "--length", "1",
            "--cursor", "{}",
        ],
        vec!["vcp", "run", "text", "--format", "json"],
    ] {
        assert!(Cli::try_parse_from(args.clone()).is_err(), "{args:?}");
    }
    for args in [
        vec!["vcp", "--format", "jsonl", "tasks", "pause", "task"],
        vec!["vcp", "tasks", "pause", "task", "--format", "jsonl"],
        vec![
            "vcp",
            "sessions",
            "fork",
            "session",
            "--through-turn",
            "turn",
        ],
        vec!["vcp", "resume", "--last"],
    ] {
        assert!(Cli::try_parse_from(args).is_ok());
    }
}

#[test]
fn preflight_resolves_workspace_and_task_input_without_creating_state() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace with spaces");
    std::fs::create_dir(&workspace).unwrap();
    let parse = |path: &std::path::Path| {
        Cli::try_parse_from([
            "vcp",
            "--workspace",
            path.to_str().unwrap(),
            "run",
            "inspect sources",
            "--budget-usd",
            "0.01",
        ])
        .unwrap()
    };
    let validated = parse(&workspace).validate(None).unwrap();
    assert_eq!(validated.workspace, workspace.canonicalize().unwrap());
    assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 0);
    let missing = temp.path().join("absent");
    assert!(parse(&missing).validate(None).is_err());
    assert!(!missing.exists());
    let file = temp.path().join("file");
    std::fs::write(&file, "not a workspace").unwrap();
    assert!(parse(&file).validate(None).is_err());
}

#[test]
fn money_has_exact_micros_and_rejects_ambiguous_or_overflowing_values() {
    for (text, micros) in [
        ("2.00", 2_000_000),
        ("0.000001", 1),
        ("13.012345", 13_012_345),
    ] {
        assert_eq!(parse_usd(text).unwrap().get(), micros);
    }
    for text in [
        "0",
        "0.000000",
        "-1",
        "+1",
        "1e2",
        "NaN",
        "inf",
        ".1",
        "1.",
        "1.0000001",
        "1,000",
        "1.2.3",
        "18446744073710",
        "18446744073709551616",
    ] {
        assert!(parse_usd(text).is_err(), "{text}");
    }
}

#[test]
fn task_file_is_bounded_utf8_data_and_budget_is_required() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("task with spaces.md");
    let parse = || {
        let cli = Cli::try_parse_from(["vcp", "run", "--file", path.to_str().unwrap()]).unwrap();
        let Some(Command::Run(run)) = cli.command else {
            panic!("expected run")
        };
        run
    };
    let cap = parse_usd("1").unwrap();
    let literal = "$(Write-Output private)\nUnicode: 日本語 🦀\r\n";
    std::fs::write(&path, format!("\u{feff}{literal}")).unwrap();
    assert!(parse().validate(None).is_err());
    assert_eq!(parse().validate(Some(cap)).unwrap().objective, literal);
    for bytes in [
        vec![0xff, 0xfe],
        vec![b'x'; 65_537],
        vec![],
        vec![b' ', b'\n'],
        vec![b'x', 0],
    ] {
        std::fs::write(&path, bytes).unwrap();
        assert!(parse().validate(Some(cap)).is_err());
    }
    std::fs::remove_file(&path).unwrap();
    assert!(parse().validate(Some(cap)).is_err());
}

#[test]
fn exit_precedence_preserves_simultaneous_conditions() {
    let mut conditions = Conditions {
        unresolved_effect: true,
        cancelled: true,
        budget_exhausted: true,
        required_input: true,
        incomplete: true,
        invalid_configuration: true,
        internal_failure: true,
        durably_paused: true,
        completed: true,
    };
    assert_eq!(conditions.code(), 7);
    conditions.unresolved_effect = false;
    assert_eq!(conditions.code(), 6);
    conditions.cancelled = false;
    assert_eq!(conditions.code(), 5);
    conditions.budget_exhausted = false;
    assert_eq!(conditions.code(), 4);
    conditions.required_input = false;
    assert_eq!(conditions.code(), 3);
    conditions.incomplete = false;
    assert_eq!(conditions.code(), 2);
    conditions.invalid_configuration = false;
    assert_eq!(conditions.code(), 1);
    conditions.internal_failure = false;
    assert_eq!(conditions.code(), 8);
    conditions.durably_paused = false;
    assert_eq!(conditions.code(), 0);
    assert_eq!(Conditions::default().code(), 1);
}

#[test]
fn jsonl_frames_unicode_newlines_and_one_final_record() {
    let mut bytes = vec![];
    let correlation = CommandId::new();
    let mut stream = Jsonl::new(&mut bytes);
    for reason in ["first\nsecond", "日本語 🦀", "\u{1b}[2J"] {
        stream
            .emit(&correlation, None, Payload::CursorGap { reason })
            .unwrap();
    }
    let conditions = Conditions {
        invalid_configuration: true,
        ..Default::default()
    };
    stream
        .emit(
            &correlation,
            None,
            Payload::Result {
                conditions: &conditions,
                exit_code: 2,
                receipt: None,
            },
        )
        .unwrap();
    assert!(stream
        .emit(&correlation, None, Payload::CursorGap { reason: "late" })
        .is_err());
    let lines: Vec<_> = std::str::from_utf8(&bytes).unwrap().lines().collect();
    assert_eq!(lines.len(), 4);
    for line in &lines {
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["correlation"], correlation.as_str());
    }
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(lines[0]).unwrap()["reason"],
        "first\nsecond"
    );
}

struct Broken;
impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn broken_pipe_and_undurable_success_cannot_emit_a_final_success() {
    let correlation = CommandId::new();
    let mut stream = Jsonl::new(Broken);
    assert_eq!(
        stream
            .emit(&correlation, None, Payload::CursorGap { reason: "lost" })
            .unwrap_err()
            .kind(),
        io::ErrorKind::BrokenPipe
    );
    assert!(stream
        .emit(&correlation, None, Payload::CursorGap { reason: "retry" })
        .is_err());
    let mut bytes = vec![];
    let mut stream = Jsonl::new(&mut bytes);
    for conditions in [
        Conditions {
            completed: true,
            ..Default::default()
        },
        Conditions {
            durably_paused: true,
            ..Default::default()
        },
    ] {
        assert!(stream
            .emit(
                &correlation,
                None,
                Payload::Result {
                    exit_code: conditions.code(),
                    conditions: &conditions,
                    receipt: None
                }
            )
            .is_err());
    }
    assert!(bytes.is_empty());
}
