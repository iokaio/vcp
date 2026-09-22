// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::io::{BufReader, Cursor};
use std::sync::{mpsc as sync, Arc, Mutex};
use std::time::Duration;
use vcp_domain::{
    task::{Objective, TaskState},
    verification::Fingerprint,
    *,
};
use vcp_store::contract::Record;

#[test]
fn commands_require_explicit_question_answers_and_exact_arguments() {
    for blank in ["", " \r\n", "\t"] {
        assert_eq!(parse(blank), Ok(None));
    }
    for invalid in [
        "/answer",
        "/answer question",
        "/answer question yes",
        "/answer question allow extra",
        "/pause extra",
        "/unknown",
        "/read artifact",
        "/read artifact -1",
        "/read artifact words",
        "/read artifact 18446744073709551616",
        "/read artifact 0 extra",
        "/skills activate",
        "/skills disable id extra",
    ] {
        assert!(parse(invalid).is_err(), "{invalid}");
    }
    for (command, expected) in [
        ("/pause", Input::Pause),
        ("/resume", Input::Resume),
        ("/cancel", Input::Cancel),
        ("/exit", Input::Exit),
        ("/status", Input::Status),
        ("/cost", Input::Cost),
        ("/history", Input::History),
        ("/next", Input::Next),
        ("/agents", Input::Agents),
        ("/help", Input::Help),
        (
            "/skills",
            Input::Skills(crate::skills::Command::List { offset: 0 }),
        ),
        (
            "/skills disable workspace::rust::review",
            Input::Skills(crate::skills::Command::Disable {
                id: "workspace::rust::review".into(),
            }),
        ),
        ("/inspect receipt", Input::Inspect("receipt".into())),
        (
            "/read artifact 0",
            Input::Read {
                id: "artifact".into(),
                offset: 0,
            },
        ),
        (
            "/read artifact 16384",
            Input::Read {
                id: "artifact".into(),
                offset: 16384,
            },
        ),
        ("/memory", Input::Unavailable("/memory".into())),
        (
            "/groups",
            Input::Optimize(crate::optimize::Command::Groups {
                model: None,
                offset: 0,
            }),
        ),
        (
            "/optimize",
            Input::Optimize(crate::optimize::Command::Report),
        ),
        (
            "/answer question allow",
            Input::Answer {
                id: "question".into(),
                allow: true,
            },
        ),
        (
            "/answer question deny",
            Input::Answer {
                id: "question".into(),
                allow: false,
            },
        ),
    ] {
        assert_eq!(parse(command), Ok(Some(expected)));
    }
    assert_eq!(
        parse("  preserve 界 e\u{301}  "),
        Ok(Some(Input::Steer("preserve 界 e\u{301}".into())))
    );
    assert!(parse(&"界".repeat(INPUT_LIMIT / 3 + 1)).is_err());
}

#[test]
fn escalation_declaration_binds_current_owner_task_without_resuming() {
    use vcp_lifecycle::foundation::{routing::Request, routing_state::declarations::Kind};
    let task = fixture_task("current-task");
    let command = parse("/escalate complexity --evidence artifact-a")
        .unwrap()
        .unwrap();
    let Input::Escalate(command) = command else {
        panic!("expected owner declaration")
    };
    let mut calls = 0;
    let text = escalation::execute(command, &task, |request| {
        calls += 1;
        let Request::DeclareEscalation { declaration } = request else {
            panic!("declaration must not resume or dispatch")
        };
        assert_eq!(declaration.task, task.scope.task);
        assert_eq!(declaration.expected_revision, task.revision);
        assert_eq!(declaration.steering, task.steering);
        assert_eq!(declaration.declaration, Kind::DeclaredComplexity);
        assert_eq!(
            declaration.evidence,
            vec![ArtifactId::parse("artifact-a").unwrap()]
        );
        Ok(serde_json::json!({"state":"pending"}))
    })
    .unwrap();
    assert_eq!(calls, 1);
    assert!(text.contains("does not resume"));
    assert!(escalation::execute(escalation::Command::Status, &task, |request| {
        assert!(matches!(request, Request::EscalationDeclarations { task: id } if id == task.scope.task));
        Err("owner unavailable".into())
    }).is_err());
}

#[test]
fn input_requires_complete_utf8_lines_across_small_buffer_boundaries() {
    let mut reader = BufReader::with_capacity(1, Cursor::new("界 e\u{301}\r\n/pause\npartial"));
    assert_eq!(
        read_line(&mut reader).unwrap().as_deref(),
        Some("界 e\u{301}\r\n")
    );
    assert_eq!(read_line(&mut reader).unwrap().as_deref(), Some("/pause\n"));
    assert_eq!(
        read_line(&mut reader).unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );
    assert_eq!(
        read_line(&mut Cursor::new(b"\xff\n")).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(read_line(&mut Cursor::new(b"")).unwrap(), None);
    let oversized = vec![b'a'; INPUT_LIMIT + 3];
    assert_eq!(
        read_line(&mut Cursor::new(oversized)).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    let complete = format!("{}\r\n", "a".repeat(INPUT_LIMIT));
    assert_eq!(
        read_line(&mut Cursor::new(&complete)).unwrap(),
        Some(complete)
    );
}

#[tokio::test]
async fn input_channel_delivers_explicit_lines_and_reports_partial_eof_once() {
    let mut lines = input(Cursor::new(
        b"\n/answer question deny\n/answer question allow",
    ))
    .unwrap();
    assert_eq!(lines.recv().await.unwrap().unwrap(), "\n");
    assert_eq!(
        lines.recv().await.unwrap().unwrap(),
        "/answer question deny\n"
    );
    assert_eq!(
        lines.recv().await.unwrap().unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );
    assert!(lines.recv().await.is_none());
}

#[test]
fn terminal_sanitization_preserves_unicode_and_raw_evidence() {
    let raw = "C:\\資料\\e\u{301} 界\x1b]52;c;secret\x07\r\n\u{009b}31m\u{202e}hidden\u{2066}text";
    let saved = raw.to_owned();
    let rendered = sanitize(raw, DISPLAY_LIMIT);
    assert!(rendered.starts_with("C:\\資料\\e\u{301} 界"));
    assert!(!rendered.chars().any(char::is_control));
    for escaped in [
        "\\u{1b}",
        "\\u{7}",
        "\\u{d}",
        "\\u{a}",
        "\\u{9b}",
        "\\u{202e}",
        "\\u{2066}",
    ] {
        assert!(rendered.contains(escaped), "missing {escaped}");
    }
    assert_eq!(raw, saved);
    assert_eq!(sanitize("界界", 3), "界… [display truncated; use /inspect]");
    assert_eq!(sanitize("\x1b", 2), "… [display truncated; use /inspect]");
    assert_eq!(
        sanitize(&"x".repeat(DISPLAY_LIMIT + 1), DISPLAY_LIMIT)
            .matches('x')
            .count(),
        DISPLAY_LIMIT
    );
}

struct GatedWriter {
    entered: Option<sync::Sender<()>>,
    release: sync::Receiver<()>,
    bytes: Arc<Mutex<Vec<u8>>>,
    flushed: sync::Sender<()>,
}
impl Write for GatedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some(entered) = self.entered.take() {
            entered.send(()).unwrap();
            self.release
                .recv_timeout(Duration::from_secs(5))
                .map_err(io::Error::other)?;
        }
        self.bytes.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.flushed.send(()).map_err(io::Error::other)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stalled_output_coalesces_flood_without_blocking_input_or_losing_latest_view() {
    let (entered_tx, entered_rx) = sync::channel();
    let (release_tx, release_rx) = sync::channel();
    let (flushed_tx, flushed_rx) = sync::channel();
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let renderer = Renderer::new(GatedWriter {
        entered: Some(entered_tx),
        release: release_rx,
        bytes: bytes.clone(),
        flushed: flushed_tx,
    })
    .unwrap();
    renderer.show("first");
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    for i in 0..10_000 {
        renderer.show(&format!("progress {i}"));
    }
    renderer.show("final\x1b[2J");
    let mut commands = input(Cursor::new(b"/pause\n")).unwrap();
    let command = tokio::time::timeout(Duration::from_secs(2), commands.recv())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(parse(&command), Ok(Some(Input::Pause)));
    release_tx.send(()).unwrap();
    flushed_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    flushed_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        String::from_utf8(bytes.lock().unwrap().clone()).unwrap(),
        "first\nfinal\\u{1b}[2J\n"
    );
    assert!(!renderer.failed());
}

struct BrokenWriter;
impl Write for BrokenWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[tokio::test]
async fn output_failure_is_reported_to_owner() {
    let renderer = Renderer::new(BrokenWriter).unwrap();
    renderer.show("status");
    tokio::time::timeout(Duration::from_secs(5), async {
        while !renderer.failed() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
}

fn fixture_task(id: &str) -> Task {
    let task = TaskId::parse(id).unwrap();
    Task {
        redaction: None,
        scope: Scope {
            workspace: WorkspaceId::parse("workspace").unwrap(),
            session: SessionId::parse("session").unwrap(),
            task: task.clone(),
        },
        root: task,
        parent: None,
        fork_origin: None,
        revision: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        objectives: vec![Objective {
            text: "preserve 界\x1b[2J".into(),
            constraints: vec![],
            acceptance: vec![],
            source: EventId::parse("event").unwrap(),
            steering: SteeringRevision::ZERO,
        }],
        state: TaskState::Paused,
        fingerprint: Fingerprint {
            repository: "a".repeat(64),
            buffers: "b".repeat(64),
            environment: "c".repeat(64),
        },
        editing: false,
        required_checks: vec![],
        cause: EventId::parse("event").unwrap(),
        reason: "user paused".into(),
    }
}

#[test]
fn agents_pages_preserve_all_children_and_canonical_node_cost_without_resuming() {
    use vcp_domain::accounting::{Currency, Money, RequestRole, Reservation, ReservationState};
    let mut state = State::default();
    let root = fixture_task("root");
    insert_task(&mut state, &root);
    for index in 0..19 {
        let mut child = fixture_task(&format!("child-{index:02}"));
        child.root = root.root.clone();
        child.parent = Some(root.root.clone());
        insert_task(&mut state, &child);
        if index == 0 {
            let reservation = Reservation {
                schema_version: 1,
                id: ReservationId::new(),
                scope: child.scope,
                root: root.root.clone(),
                attempt: AttemptId::new(),
                revision: Revision::ZERO,
                phase: ReservationState::ReconciliationPending,
                amount: Money {
                    currency: Currency::try_from("USD".to_owned()).unwrap(),
                    micros: Micros::new(30),
                },
                charged: Micros::new(10),
                liability: Micros::new(20),
                protected_draw: Micros::ZERO,
                protected_returned: Micros::ZERO,
                day: 0,
                role: RequestRole::Child,
            };
            let row = Record::typed(
                Collection::Reservation,
                reservation.id.as_str(),
                root.scope.workspace.clone(),
                Revision::ZERO,
                &reservation,
            )
            .unwrap();
            state.records.insert(row.key(), row);
        }
    }
    insert_task(&mut state, &fixture_task("unrelated"));
    let before = state.clone();
    let mut ids = std::collections::BTreeSet::new();
    for (offset, count) in [(0, 8), (8, 8), (16, 3)] {
        let page =
            crate::agents_view::page(&state, &root.scope, Timestamp::new(1000), offset).unwrap();
        assert_eq!(page["total"], 19);
        assert_eq!(page["items"].as_array().unwrap().len(), count);
        for item in page["items"].as_array().unwrap() {
            assert!(ids.insert(item["task"].as_str().unwrap().to_owned()));
            assert_eq!(item["state"], "paused");
            assert!(item["readiness"]["materialization"]
                .as_str()
                .unwrap()
                .starts_with("not_ready"));
            assert!(item["readiness"]["process_checks"]
                .as_str()
                .unwrap()
                .contains("not qualified"));
            assert_eq!(item["readiness"]["checks_not_run"], serde_json::json!([]));
            assert!(!item["objective"].as_str().unwrap().contains('\x1b'));
        }
        if offset == 0 {
            assert_eq!(page["items"][0]["cost"]["known"], 10);
            assert_eq!(page["items"][0]["cost"]["reserved"], 0);
            assert_eq!(page["items"][0]["cost"]["uncertain"], 20);
        }
        if offset == 16 {
            assert!(page["next_offset"].is_null());
        }
    }
    assert_eq!(ids.len(), 19);
    assert_eq!(state, before);
    assert!(crate::agents_view::page(&state, &root.scope, Timestamp::new(1000), 20).is_err());
    assert_eq!(parse("/agents 8"), Ok(Some(Input::AgentsPage(8))));
    assert!(parse("/agents -1").is_err());
    let selected = TaskId::parse("child-18").unwrap();
    assert_eq!(
        crate::agents_view::detail(&state, &root.scope, &selected, Timestamp::new(1000)).unwrap()
            ["task"],
        "child-18"
    );
    assert!(
        crate::agents_view::child(&state, &root.scope, &TaskId::parse("unrelated").unwrap())
            .is_err()
    );
    assert!(crate::agents_view::child(&state, &root.scope, &root.root).is_err());
    for (text, action) in [
        ("focus", AgentAction::Focus),
        ("follow", AgentAction::Follow),
        ("pause", AgentAction::Pause),
        ("cancel", AgentAction::Cancel),
        ("resume", AgentAction::Resume),
    ] {
        assert_eq!(
            parse(&format!("/agents {text} child-18")),
            Ok(Some(Input::Agent {
                task: selected.clone(),
                action
            }))
        );
    }
    assert!(parse("/agents pause child-18 extra").is_err());
    assert_eq!(
        parse("/agents delegate \"C:/owner files/child.json\""),
        Ok(Some(Input::Delegate("C:/owner files/child.json".into())))
    );
    assert_eq!(
        parse("/agents recover child-18 \"C:/Program Files/Git/cmd/git.exe\""),
        Ok(Some(Input::RecoverChild {
            task: selected,
            git: "C:/Program Files/Git/cmd/git.exe".into()
        }))
    );
    assert!(parse("/agents delegate \"unclosed").is_err());
    let helper = parse("/agents review src 0.25 120 \"C:/Program Files/Git/cmd/git.exe\" \"C:/child roots\" Review parser errors").unwrap();
    assert!(
        matches!(helper, Some(Input::Helper(Helper { name, scope, objective, .. })) if name == "review" && scope == "src" && objective == "Review parser errors")
    );
    assert!(parse("/agents explore src 0.25 0 git children objective").is_err());
    assert!(parse("/agents review src 0.25 120 \"unclosed").is_err());
    assert!(matches!(
        parse(
            "/agents cleanup preview child-18 \"C:/Program Files/Git/cmd/git.exe\" --reject-edits"
        )
        .unwrap(),
        Some(Input::Cleanup {
            action: CleanupAction::Preview {
                reject_edits: true,
                ..
            },
            ..
        })
    ));
    assert!(matches!(
        parse("/agents cleanup apply child-18").unwrap(),
        Some(Input::Cleanup {
            action: CleanupAction::Apply,
            ..
        })
    ));
    assert!(parse("/agents cleanup apply child-18 extra").is_err());
}
fn insert_task(state: &mut State, task: &Task) {
    let record = Record::typed(
        Collection::Task,
        task.scope.task.as_str(),
        task.scope.workspace.clone(),
        task.revision,
        task,
    )
    .unwrap();
    state.records.insert(record.key(), record);
}

#[test]
fn immutable_view_filters_task_tree_and_scope_and_bounds_large_history() {
    let mut state = State::default();
    let root = fixture_task("root");
    insert_task(&mut state, &root);
    let mut child = fixture_task("child");
    child.root = root.root.clone();
    child.parent = Some(root.root.clone());
    insert_task(&mut state, &child);
    insert_task(&mut state, &fixture_task("unrelated-root"));
    let mut other_session = child.clone();
    other_session.scope.task = TaskId::parse("other-session-child").unwrap();
    other_session.scope.session = SessionId::parse("other-session").unwrap();
    insert_task(&mut state, &other_session);
    for index in 0..40 {
        let record = Record {
            collection: Collection::Approval,
            id: format!("question-{index:02}"),
            workspace: root.scope.workspace.clone(),
            revision: Revision::ZERO,
            value: question_value(&child.scope, &format!("question-{index:02}"), false),
            references: Default::default(),
        };
        state.records.insert(record.key(), record);
    }
    let before = state.clone();
    let rendered = view_at(&state, &root.scope, "fixture/model", Timestamp::new(1000)).unwrap();
    assert_eq!(rendered["state"], "paused");
    assert_eq!(rendered["children"].as_array().unwrap().len(), 1);
    assert_eq!(rendered["children"][0]["task"], "child");
    assert_eq!(rendered["pending_questions"].as_array().unwrap().len(), 8);
    assert_eq!(rendered["question_count"], 40);
    assert_eq!(rendered["pending_questions"][0]["actionable"], false);
    assert_eq!(rendered["model"], "fixture/model");
    assert!(!rendered["objective"].as_str().unwrap().contains('\x1b'));
    assert_eq!(
        view_at(&state, &root.scope, "fixture/model", Timestamp::new(1000)).unwrap(),
        rendered
    );
    assert_eq!(
        state, before,
        "redraw must not answer pending questions or mutate canonical state"
    );
    let mut wrong_scope = root.scope.clone();
    wrong_scope.session = SessionId::parse("wrong-session").unwrap();
    assert!(view_at(&state, &wrong_scope, "fixture/model", Timestamp::new(1000)).is_err());
    wrong_scope = root.scope.clone();
    wrong_scope.workspace = WorkspaceId::parse("wrong-workspace").unwrap();
    assert!(view_at(&state, &wrong_scope, "fixture/model", Timestamp::new(1000)).is_err());
}

fn insert_summary_record(state: &mut State, collection: Collection, id: &str, value: Value) {
    let record = Record {
        collection,
        id: id.into(),
        workspace: WorkspaceId::parse("workspace").unwrap(),
        revision: Revision::ZERO,
        value,
        references: Default::default(),
    };
    state.records.insert(record.key(), record);
}

fn question_value(scope: &Scope, id: &str, answered: bool) -> Value {
    use vcp_protocol::command::{Approval, ApprovalState};
    serde_json::to_value(Approval {
        id: ApprovalId::parse(id).unwrap(),
        scope: scope.clone(),
        effect: ToolRunId::parse("effect-63").unwrap(),
        effect_revision: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        operation_digest: "a".repeat(64),
        actor: ActorId::parse("terminal-test").unwrap(),
        policy: PolicyRevision::ZERO,
        expires_at: Timestamp::ZERO,
        state: if answered {
            ApprovalState::Allowed
        } else {
            ApprovalState::Pending
        },
        revision: Revision::ZERO,
        controller: None,
        owner_epoch: None,
        authority: None,
        binding: None,
    })
    .unwrap()
}

#[test]
fn history_cannot_hide_pending_questions_cost_or_current_result_checks() {
    use vcp_domain::verification::{Check, CheckOutcome, CostCertainty, Verification};
    let mut state = State::default();
    let task = fixture_task("root");
    insert_task(&mut state, &task);
    for index in 0..64 {
        insert_summary_record(
            &mut state,
            Collection::Effect,
            &format!("effect-{index:02}"),
            json!({"scope":task.scope,"state":"completed","observed_changes":[],"reason":"old operation"}),
        );
    }
    insert_summary_record(
        &mut state,
        Collection::Approval,
        "pending-question",
        question_value(&task.scope, "pending-question", false),
    );
    insert_summary_record(
        &mut state,
        Collection::Approval,
        "already-answered",
        question_value(&task.scope, "already-answered", true),
    );
    insert_summary_record(
        &mut state,
        Collection::Ledger,
        "root",
        json!({"scope":task.scope,"currency":"USD","settled":13,"active":7,"unresolved":5,"cap":100}),
    );
    insert_summary_record(
        &mut state,
        Collection::Turn,
        "current-turn",
        json!({"scope":task.scope,"steering":task.steering,"state":"verifying","reason":"check current result"}),
    );
    let report = Verification {
        redaction: None,
        id: VerificationId::parse("current-report").unwrap(),
        scope: task.scope.clone(),
        steering: task.steering,
        fingerprint: task.fingerprint.clone(),
        outputs: vec![ArtifactId::parse("output").unwrap()],
        checks: vec![Check {
            specification: "unit tests".into(),
            outcome: CheckOutcome::Passed,
            output: ArtifactId::parse("output").unwrap(),
            exit_code: Some(0),
        }],
        unresolved_effects: vec![],
        outstanding_issues: vec![],
        cost: CostCertainty::Known,
    };
    insert_summary_record(
        &mut state,
        Collection::Verification,
        "current-report",
        serde_json::to_value(&report).unwrap(),
    );
    let mut stale = report.clone();
    stale.id = VerificationId::parse("stale-report").unwrap();
    stale.fingerprint.repository = "d".repeat(64);
    insert_summary_record(
        &mut state,
        Collection::Verification,
        "stale-report",
        serde_json::to_value(&stale).unwrap(),
    );
    let rendered = view_at(&state, &task.scope, "fixture/model", Timestamp::new(1000)).unwrap();
    assert_eq!(rendered["change_count"], 64);
    assert_eq!(rendered["changes"].as_array().unwrap().len(), 8);
    assert_eq!(rendered["pending_questions"].as_array().unwrap().len(), 1);
    assert_eq!(rendered["pending_questions"][0]["id"], "pending-question");
    assert_eq!(rendered["pending_questions"][0]["actionable"], false);
    assert_eq!(rendered["cost"]["known"], 13);
    assert_eq!(rendered["cost"]["reserved"], 7);
    assert_eq!(rendered["cost"]["uncertain"], 5);
    assert_eq!(rendered["current_step"]["state"], "verifying");
    assert_eq!(rendered["current_checks"].as_array().unwrap().len(), 1);
    assert_eq!(rendered["current_checks"][0]["id"], "current-report");
    assert_eq!(rendered["current_checks"][0]["satisfies"], true);
}

fn add_event(state: &mut State, task: &Task, id: &str, collection: Collection, record: &str) {
    use vcp_protocol::event::{EventEnvelope, EventInput, EventKind};
    let watermark = state.watermark.next().unwrap();
    state.events.push(EventEnvelope {
        redaction: None,
        version: 1,
        sequence: SessionSeq::new(watermark.get()),
        watermark,
        event: EventInput {
            id: EventId::parse(id).unwrap(),
            workspace: task.scope.workspace.clone(),
            session: task.scope.session.clone(),
            task: Some(task.scope.task.clone()),
            actor: ActorId::parse("terminal-test").unwrap(),
            correlation: CommandId::parse(format!("command-{id}")).unwrap(),
            causation: None,
            timestamp: Timestamp::new(1_000 + watermark.get()),
            kind: match collection {
                Collection::Verification => EventKind::VerificationRecorded,
                Collection::Turn => EventKind::TurnTransition,
                _ => EventKind::EffectTransition,
            },
            artifacts: vec![],
            data: json!({"facts":[{"collection":collection,"id":record}]}),
            metadata: None,
        },
    });
    state.watermark = watermark;
}

#[test]
fn latest_failed_check_and_unresolved_effects_remain_visible_after_history_flood() {
    use vcp_domain::verification::{Check, CheckOutcome, CostCertainty, Verification};
    let mut state = State::default();
    let task = fixture_task("root");
    insert_task(&mut state, &task);
    for (id, status) in [("z-running", "running"), ("z-unknown", "outcome_unknown")] {
        let cause = format!("cause-{id}");
        add_event(&mut state, &task, &cause, Collection::Effect, id);
        insert_summary_record(
            &mut state,
            Collection::Effect,
            id,
            json!({"scope":task.scope,"state":status,"cause":cause,"reason":"still stopping","observed_changes":[]}),
        );
    }
    for index in 0..20 {
        let id = format!("a-completed-{index:02}");
        let cause = format!("cause-{index:02}");
        add_event(&mut state, &task, &cause, Collection::Effect, &id);
        insert_summary_record(
            &mut state,
            Collection::Effect,
            &id,
            json!({"scope":task.scope,"state":"succeeded","cause":cause,"reason":"completed","observed_changes":[]}),
        );
    }
    for (id, status) in [
        ("a-old-turn", "requesting_model"),
        ("z-current-turn", "verifying"),
    ] {
        let cause = format!("cause-{id}");
        add_event(&mut state, &task, &cause, Collection::Turn, id);
        insert_summary_record(
            &mut state,
            Collection::Turn,
            id,
            json!({"scope":task.scope,"steering":task.steering,"state":status,"cause":cause,"reason":"current step"}),
        );
    }
    for (id, outcome) in [
        ("a-old-passing", CheckOutcome::Passed),
        (
            "z-current-failed",
            CheckOutcome::Failed {
                reason: "regression".into(),
            },
        ),
    ] {
        let report = Verification {
            redaction: None,
            id: VerificationId::parse(id).unwrap(),
            scope: task.scope.clone(),
            steering: task.steering,
            fingerprint: task.fingerprint.clone(),
            outputs: vec![ArtifactId::parse("output").unwrap()],
            checks: vec![Check {
                specification: "unit tests".into(),
                outcome,
                output: ArtifactId::parse("output").unwrap(),
                exit_code: None,
            }],
            unresolved_effects: vec![],
            outstanding_issues: vec![],
            cost: CostCertainty::Known,
        };
        add_event(
            &mut state,
            &task,
            &format!("cause-{id}"),
            Collection::Verification,
            id,
        );
        insert_summary_record(
            &mut state,
            Collection::Verification,
            id,
            serde_json::to_value(report).unwrap(),
        );
    }
    let rendered = view_at(&state, &task.scope, "fixture/model", Timestamp::new(1000)).unwrap();
    assert_eq!(rendered["change_count"], 22);
    assert_eq!(rendered["changes"].as_array().unwrap().len(), 8);
    assert_eq!(rendered["changes"][0]["id"], "z-unknown");
    assert_eq!(rendered["changes"][1]["id"], "z-running");
    assert_eq!(rendered["changes"][2]["id"], "a-completed-19");
    assert_eq!(rendered["current_step"]["id"], "z-current-turn");
    assert_eq!(rendered["current_checks"].as_array().unwrap().len(), 1);
    assert_eq!(rendered["current_checks"][0]["id"], "z-current-failed");
    assert_eq!(rendered["current_checks"][0]["satisfies"], false);
    assert_eq!(
        rendered["current_checks"][0]["checks"][0]["outcome"]["status"],
        "failed"
    );
}
