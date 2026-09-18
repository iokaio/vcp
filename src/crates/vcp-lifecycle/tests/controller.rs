// SPDX-License-Identifier: Apache-2.0
use codex_core::{
    CodexThread, NotSubmittedReason, StartThreadOptions, TurnInputRequest, TurnInputSubmission,
    TurnStartOptions,
};
use codex_extension_api::{ExtensionRegistry, ExtensionRegistryBuilder, TurnStartAdmission};
use codex_protocol::{
    ThreadId,
    protocol::{EventMsg, SessionSource, SubAgentSource},
    user_input::UserInput,
};
use core_test_support::{
    responses,
    streaming_sse::{StreamingSseChunk, start_streaming_sse_server},
    test_codex::{TestCodex, test_codex},
    wait_for_event,
};
use std::{sync::Arc, time::Duration};
use tokio::{sync::oneshot, time::timeout};
use vcp_lifecycle::{Error, Lifecycle};

fn extensions(host: &Lifecycle) -> Arc<ExtensionRegistry<codex_core::config::Config>> {
    let mut builder = ExtensionRegistryBuilder::new();
    builder.turn_start_admission(Arc::new(host.clone()));
    Arc::new(builder.build())
}

fn dispatch_extensions(host: &Lifecycle) -> Arc<ExtensionRegistry<codex_core::config::Config>> {
    let mut builder = ExtensionRegistryBuilder::new();
    builder.turn_start_admission(Arc::new(host.clone()));
    builder.work_admission(Arc::new(host.clone()));
    Arc::new(builder.build())
}

fn authorized_builder(
    host: &Lifecycle,
    resumed: Option<ThreadId>,
) -> core_test_support::test_codex::TestCodexBuilder {
    let owner = host.clone();
    test_codex()
        .with_extensions(dispatch_extensions(host))
        .with_config(move |config| {
            config.analytics_enabled = Some(false);
            owner
                .authorize_startup(config.cwd.as_path(), resumed)
                .unwrap();
        })
}

fn input(text: &str) -> TurnInputRequest {
    TurnInputRequest::user_input(vec![UserInput::Text {
        text: text.into(),
        text_elements: vec![],
    }])
}

fn delegated(text: &str) -> TurnInputRequest {
    input(text).on_start(TurnStartOptions {
        parent_turn_id: Some("host-parent".into()),
        ..Default::default()
    })
}

async fn child(test: &TestCodex) -> Arc<CodexThread> {
    test.thread_manager
        .start_thread(StartThreadOptions {
            session_source: Some(SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                parent_thread_id: test.session_configured.thread_id,
                depth: 1,
                agent_path: None,
                agent_nickname: None,
                agent_role: None,
            })),
            environments: Some(test.codex.environment_selections().await),
            ..StartThreadOptions::new(test.config.clone())
        })
        .await
        .unwrap()
        .thread
}

async fn complete(thread: &CodexThread, request: TurnInputRequest) {
    assert!(matches!(
        thread.start_or_steer_turn(request).await.unwrap(),
        TurnInputSubmission::Started { .. }
    ));
    timeout(
        Duration::from_secs(10),
        wait_for_event(thread, |e| matches!(e, EventMsg::TurnComplete(_))),
    )
    .await
    .unwrap();
}

async fn denied(thread: &CodexThread, request: TurnInputRequest) {
    assert_eq!(
        thread.start_or_steer_turn(request).await.unwrap(),
        TurnInputSubmission::NotSubmitted {
            reason: NotSubmittedReason::ServerDraining
        }
    );
}

async fn hold(host: &Lifecycle, id: ThreadId) {
    timeout(
        Duration::from_secs(10),
        host.hold(id, &host.inspect(id).unwrap().revision)
            .unwrap()
            .wait(),
    )
    .await
    .unwrap()
    .unwrap();
}

#[tokio::test]
async fn checkpoint_reopen_requires_binding_interruption_and_deliberate_resume() {
    let server = responses::start_mock_server().await;
    let observed = responses::mount_sse_sequence(
        &server,
        vec![
            responses::sse_completed("before-close"),
            responses::sse_completed("after-resume"),
        ],
    )
    .await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("lifecycle.journal");
    let (host, owner) =
        Lifecycle::open(&path, "workspace-fixture", Duration::from_secs(5)).unwrap();
    let test = authorized_builder(&host, None)
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    complete(&test.codex, input("first turn")).await;
    let old_revision = host.inspect(root).unwrap().revision;
    hold(&host, root).await;
    let home = test.home.clone();
    let rollout = test.session_configured.rollout_path.clone().unwrap();
    owner.close().await.unwrap();
    test.codex.shutdown_and_wait().await.unwrap();
    drop(test);
    // Owner loss owns an asynchronous interruption even if its caller disappears.
    // Wait for that operation to finish before attempting exclusive reacquisition.
    timeout(Duration::from_secs(5), async {
        while !host.inspect(root).unwrap().interruption_error.is_some()
            && !host.inspect(root).unwrap().interrupt_complete
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    drop(host);
    let (host, _owner) =
        Lifecycle::open(&path, "workspace-fixture", Duration::from_secs(5)).unwrap();
    assert_eq!(host.threads().unwrap(), vec![root]);
    assert!(host.inspect(root).unwrap().local_hold);
    assert_eq!(host.resume(root, &old_revision), Err(Error::StaleRevision));
    assert_eq!(
        host.resume(root, &host.inspect(root).unwrap().revision),
        Err(Error::IncompleteInterruption)
    );
    assert_eq!(observed.requests().len(), 1);
    let test = authorized_builder(&host, Some(root))
        .resume(&server, home, rollout)
        .await
        .unwrap();
    host.bind_recovered(test.codex.clone()).unwrap();
    denied(&test.codex, input("reopening is not consent to continue")).await;
    hold(&host, root).await;
    host.resume(root, &host.inspect(root).unwrap().revision)
        .unwrap();
    complete(&test.codex, input("explicit continuation")).await;
    assert_eq!(observed.requests().len(), 2);
    assert_eq!(host.work().unwrap().len(), 2);
    assert!(
        host.work()
            .unwrap()
            .iter()
            .all(|work| work.receipt.is_some())
    );
}

#[tokio::test]
async fn aborted_model_request_requires_explicit_receipt_reconciliation() {
    let (release, gate) = oneshot::channel();
    let (server, _) = start_streaming_sse_server(vec![vec![
        StreamingSseChunk {
            gate: None,
            body: responses::sse(vec![
                serde_json::json!({"type":"response.created", "response":{"id":"held"}}),
            ]),
        },
        StreamingSseChunk {
            gate: Some(gate),
            body: responses::sse_completed("held"),
        },
    ]])
    .await;
    let (host, _owner) = Lifecycle::new(Duration::from_secs(5));
    let test = authorized_builder(&host, None)
        .with_model("gpt-5.4")
        .build_with_streaming_server(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    test.codex
        .start_or_steer_turn(input("pause in flight"))
        .await
        .unwrap();
    timeout(Duration::from_secs(5), server.wait_for_request_count(1))
        .await
        .unwrap();
    hold(&host, root).await;
    let work = host.work().unwrap();
    assert_eq!(work.len(), 1);
    assert!(work[0].receipt.is_none());
    assert_eq!(
        host.resume(root, &host.inspect(root).unwrap().revision),
        Err(Error::UnresolvedWork)
    );
    let stale = host.inspect(root).unwrap().revision;
    host.reconcile(
        work[0].id,
        &stale,
        "synthetic provider observer: one aborted request; zero billable fixture usage",
    )
    .unwrap();
    assert_eq!(host.resume(root, &stale), Err(Error::StaleRevision));
    host.resume(root, &host.inspect(root).unwrap().revision)
        .unwrap();
    assert_eq!(server.requests().await.len(), 1);
    let _ = release.send(());
    test.codex.shutdown_and_wait().await.unwrap();
    server.shutdown().await;
}

#[tokio::test]
async fn retained_patch_dispatch_records_intent_and_completion() {
    let server = responses::start_mock_server().await;
    let patch = "*** Begin Patch\n*** Add File: observed.txt\n+durable effect\n*** End Patch";
    let observed = responses::mount_sse_sequence(
        &server,
        vec![
            responses::sse(vec![
                responses::ev_response_created("patch"),
                responses::ev_apply_patch_custom_tool_call("patch-id", patch),
                responses::ev_completed("patch"),
            ]),
            responses::sse_completed("summary"),
        ],
    )
    .await;
    let directory = tempfile::tempdir().unwrap();
    let (host, _owner) = Lifecycle::open(
        &directory.path().join("journal"),
        "fixture",
        Duration::from_secs(5),
    )
    .unwrap();
    let test = authorized_builder(&host, None)
        .build_with_auto_env(&server)
        .await
        .unwrap();
    host.attach_root(test.codex.clone()).unwrap();
    timeout(
        Duration::from_secs(15),
        test.submit_turn_with_policy(
            "apply fixture patch",
            codex_protocol::protocol::SandboxPolicy::DangerFullAccess,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(test.workspace_path("observed.txt")).unwrap(),
        "durable effect\n"
    );
    assert_eq!(observed.requests().len(), 2);
    let work = host.work().unwrap();
    assert_eq!(
        work.iter()
            .map(|work| work.kind.as_str())
            .collect::<Vec<_>>(),
        ["Model", "Tool", "Model"]
    );
    assert_eq!(work[1].label, "patch-id");
    assert!(work.iter().all(|work| work.receipt.is_some()));
}

#[derive(Debug)]
struct PauseBeforeTool(Lifecycle);
impl codex_extension_api::HostWorkAdmission for PauseBeforeTool {
    fn admit_startup(
        &self,
        workspace: &std::path::Path,
        resumed: Option<ThreadId>,
    ) -> Result<Box<dyn Send>, String> {
        codex_extension_api::HostWorkAdmission::admit_startup(&self.0, workspace, resumed)
    }
    fn admit(
        &self,
        thread: ThreadId,
        kind: codex_extension_api::HostWorkKind,
        label: &str,
    ) -> Result<Box<dyn codex_extension_api::HostWorkPermit>, String> {
        if kind == codex_extension_api::HostWorkKind::Tool {
            drop(
                self.0
                    .hold(thread, &self.0.inspect(thread).unwrap().revision)
                    .unwrap(),
            );
        }
        codex_extension_api::HostWorkAdmission::admit(&self.0, thread, kind, label)
    }
}

#[tokio::test]
async fn pause_at_actual_tool_dispatch_prevents_file_effect() {
    let server = responses::start_mock_server().await;
    let observed = responses::mount_sse_sequence(
        &server,
        vec![responses::sse(vec![
            responses::ev_response_created("patch"),
            responses::ev_apply_patch_custom_tool_call(
                "rejected",
                "*** Begin Patch\n*** Add File: forbidden.txt\n+must not exist\n*** End Patch",
            ),
            responses::ev_completed("patch"),
        ])],
    )
    .await;
    let (host, _owner) = Lifecycle::new(Duration::from_secs(5));
    let mut builder = ExtensionRegistryBuilder::new();
    builder.turn_start_admission(Arc::new(host.clone()));
    builder.work_admission(Arc::new(PauseBeforeTool(host.clone())));
    let owner = host.clone();
    let test = test_codex()
        .with_extensions(Arc::new(builder.build()))
        .with_config(move |config| {
            config.analytics_enabled = Some(false);
            owner.authorize_startup(config.cwd.as_path(), None).unwrap();
        })
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    test.codex
        .start_or_steer_turn(input("attempt patch"))
        .await
        .unwrap();
    timeout(Duration::from_secs(10), async {
        while !host.inspect(root).unwrap().interrupt_complete {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(!test.workspace_path("forbidden.txt").exists());
    assert!(host.inspect(root).unwrap().local_hold);
    assert_eq!(observed.requests().len(), 1);
    assert!(host.work().unwrap().iter().all(|work| work.kind == "Model"));
}

#[tokio::test]
async fn private_controls_are_revision_checked_idempotent_and_revalidated() {
    use vcp_lifecycle::control::Action;
    let server = responses::start_mock_server().await;
    let (host, _owner) = Lifecycle::new(Duration::from_secs(5));
    let test = authorized_builder(&host, None)
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    let before = host.inspect(root).unwrap().revision;
    let paused = host
        .control("pause-1", root, &before, Action::Pause, || unreachable!())
        .await
        .unwrap();
    assert!(paused.local_hold);
    host.control("pause-1", root, &before, Action::Pause, || unreachable!())
        .await
        .unwrap();
    assert!(matches!(
        host.control("pause-1", root, &before, Action::Resume, || Ok(()))
            .await,
        Err(Error::CommandConflict)
    ));
    assert!(matches!(
        host.control("resume-stale", root, &before, Action::Resume, || Ok(()))
            .await,
        Err(Error::StaleRevision)
    ));
    assert!(matches!(
        host.control(
            "resume-denied",
            root,
            &paused.revision,
            Action::Resume,
            || Err(Error::RevalidationFailed)
        )
        .await,
        Err(Error::RevalidationFailed)
    ));
    assert!(host.inspect(root).unwrap().local_hold);
    let revision = host.inspect(root).unwrap().revision;
    host.control("resume-1", root, &revision, Action::Resume, || Ok(()))
        .await
        .unwrap();
    assert!(!host.inspect(root).unwrap().local_hold);
    // A replay of the old pause acknowledgement must not pause the now active root.
    assert!(
        !host
            .control("pause-1", root, &before, Action::Pause, || unreachable!())
            .await
            .unwrap()
            .local_hold
    );
}

#[tokio::test]
async fn startup_requires_explicit_single_use_workspace_authority() {
    let server = responses::start_mock_server().await;
    let (host, _owner) = Lifecycle::new(Duration::from_secs(5));
    let rejected = test_codex()
        .with_extensions(dispatch_extensions(&host))
        .build_with_auto_env(&server)
        .await;
    assert!(rejected.is_err());
    let test = authorized_builder(&host, None)
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    assert!(
        codex_extension_api::HostWorkAdmission::admit_startup(
            &host,
            test.config.cwd.as_path(),
            None
        )
        .is_err()
    );
    host.authorize_startup(test.config.cwd.as_path(), None)
        .unwrap();
    hold(&host, root).await;
    assert!(
        codex_extension_api::HostWorkAdmission::admit_startup(
            &host,
            test.config.cwd.as_path(),
            None
        )
        .is_err()
    );
    let foreign = tempfile::tempdir().unwrap();
    host.authorize_startup(test.config.cwd.as_path(), Some(root))
        .unwrap();
    assert!(
        codex_extension_api::HostWorkAdmission::admit_startup(&host, foreign.path(), Some(root))
            .is_err()
    );
    assert!(
        codex_extension_api::HostWorkAdmission::admit_startup(
            &host,
            test.config.cwd.as_path(),
            Some(root)
        )
        .is_ok()
    );
}

#[tokio::test]
async fn graceful_close_fences_late_receipts_before_releasing_writer_lock() {
    use codex_extension_api::{HostWorkAdmission, HostWorkKind};
    let server = responses::start_mock_server().await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("journal");
    let (host, owner) = Lifecycle::open(&path, "fixture", Duration::from_secs(5)).unwrap();
    let test = authorized_builder(&host, None)
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    let mut late =
        HostWorkAdmission::admit(&host, root, HostWorkKind::Tool, "unknown fixture effect")
            .unwrap();
    owner.close().await.unwrap();
    let (reopened, _owner) = Lifecycle::open(&path, "fixture", Duration::from_secs(5)).unwrap();
    // The new owner holds an OS-enforced exclusive byte-range lock. Metadata
    // remains observable; an append-only receipt would grow the journal.
    let length_before = std::fs::metadata(&path).unwrap().len();
    assert!(late.complete().is_err());
    assert_eq!(std::fs::metadata(&path).unwrap().len(), length_before);
    assert_eq!(reopened.inspect(root).unwrap().unresolved_work, 1);
    assert!(host.admit_turn_start_for_thread(root).is_none());
    test.codex.shutdown_and_wait().await.unwrap();
}

#[cfg(windows)]
#[tokio::test]
async fn native_pause_stops_grandchild_and_releases_exclusive_lock() {
    let server = responses::start_mock_server().await;
    let (host, _owner) = Lifecycle::new(Duration::from_secs(5));
    let test = test_codex()
        .with_extensions(extensions(&host))
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let process = host
        .spawn_process(
            root,
            std::path::Path::new(env!("CARGO_BIN_EXE_vcp-process-fixture")),
            &["tree".into(), directory.path().as_os_str().into()],
            directory.path(),
            &Default::default(),
            1024,
        )
        .unwrap();
    timeout(Duration::from_secs(5), async {
        while !directory.path().join("child-ready").exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(process.active_process_count().unwrap(), 2);
    assert!(
        std::fs::OpenOptions::new()
            .write(true)
            .open(directory.path().join("locked"))
            .is_err()
    );
    let stopping = std::time::Instant::now();
    hold(&host, root).await;
    assert_eq!(process.active_process_count().unwrap(), 0);
    let job_empty_ms = stopping.elapsed().as_millis();
    // Kernel membership can reach zero before the last file-object cleanup is
    // observable to a new opener. Observe both facts within a bounded deadline.
    timeout(Duration::from_secs(5), async {
        while std::fs::OpenOptions::new()
            .write(true)
            .open(directory.path().join("locked"))
            .is_err()
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    eprintln!(
        "VCP_PROCESS_STOP {}",
        serde_json::json!({
            "members_before": 2, "members_after": 0, "job_empty_ms": job_empty_ms,
            "exclusive_lock_released_ms": stopping.elapsed().as_millis()
        })
    );
    assert!(
        host.spawn_process(
            root,
            std::path::Path::new(env!("CARGO_BIN_EXE_vcp-process-fixture")),
            &["write".into(), directory.path().join("forbidden").into()],
            directory.path(),
            &Default::default(),
            0
        )
        .is_err()
    );
    assert!(!directory.path().join("forbidden").exists());
    let outcome = timeout(Duration::from_secs(5), process.wait())
        .await
        .unwrap()
        .unwrap();
    assert_ne!(outcome.exit_code, Some(0));
    assert!(
        host.work()
            .unwrap()
            .iter()
            .all(|work| work.receipt.is_some())
    );
}

#[cfg(windows)]
#[tokio::test]
async fn native_argv_environment_crlf_and_bounded_output_are_observed() {
    use std::{collections::BTreeMap, ffi::OsString, path::Path};
    let server = responses::start_mock_server().await;
    let (host, _owner) = Lifecycle::new(Duration::from_secs(5));
    let test = test_codex()
        .with_extensions(extensions(&host))
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let cwd = directory.path().join("space λ 文");
    std::fs::create_dir(&cwd).unwrap();
    let executable = Path::new(env!("CARGO_BIN_EXE_vcp-process-fixture"));
    let environment = BTreeMap::from([(
        OsString::from("VCP_FIXTURE_VALUE"),
        OsString::from("explicit λ"),
    )]);
    let arguments = [
        "argv".into(),
        cwd.as_os_str().into(),
        "a b".into(),
        "quote\"slash\\".into(),
        "$(no-shell) & 文".into(),
    ];
    let outcome = host
        .spawn_process(root, executable, &arguments, &cwd, &environment, 1024)
        .unwrap()
        .wait()
        .await
        .unwrap();
    assert_eq!(outcome.exit_code, Some(0));
    assert_eq!(outcome.stdout.bytes, b"one\r\ntwo\r\n");
    let actual: serde_json::Value =
        serde_json::from_slice(&std::fs::read(cwd.join("argv.json")).unwrap()).unwrap();
    assert_eq!(
        actual["args"],
        serde_json::json!(["a b", "quote\"slash\\", "$(no-shell) & 文"])
    );
    assert_eq!(actual["fixture_env"], "explicit λ");
    assert_eq!(actual["inherited"], serde_json::Value::Null);
    let process = host
        .spawn_process(
            root,
            executable,
            &["flood".into(), cwd.as_os_str().into()],
            &cwd,
            &environment,
            1024,
        )
        .unwrap();
    let outcome = timeout(Duration::from_secs(10), process.wait())
        .await
        .unwrap()
        .unwrap();
    for capture in [outcome.stdout, outcome.stderr] {
        assert_eq!(capture.total, 2 * 1024 * 1024);
        assert_eq!(capture.bytes, vec![b'x'; 1024]);
    }
    let mut long = cwd.clone();
    for _ in 0..8 {
        long.push("qualified-long-path-component-0123456789");
    }
    std::fs::create_dir_all(&long).unwrap();
    let marker = long.join("marker.txt");
    assert!(marker.as_os_str().len() > 260);
    let outcome = host
        .spawn_process(
            root,
            executable,
            &["write".into(), marker.as_os_str().into()],
            &cwd,
            &environment,
            128,
        )
        .unwrap()
        .wait()
        .await
        .unwrap();
    assert_eq!(outcome.exit_code, Some(0));
    assert_eq!(std::fs::read(marker).unwrap(), b"fixture effect\r\n");
}

#[tokio::test]
async fn independent_child_hold_survives_parent_readmission() {
    let server = responses::start_mock_server().await;
    let observed = responses::mount_sse_sequence(
        &server,
        [
            "parent",
            "sibling",
            "parent-again",
            "sibling-again",
            "child",
        ]
        .into_iter()
        .map(responses::sse_completed)
        .collect(),
    )
    .await;
    let (host, _owner) = Lifecycle::new(Duration::from_secs(5));
    let test = test_codex()
        .with_extensions(extensions(&host))
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    let child = child(&test).await;
    let child_id = host
        .attach_child(root, &host.inspect(root).unwrap().revision, child.clone())
        .unwrap();
    let sibling = self::child(&test).await;
    let sibling_id = host
        .attach_child(root, &host.inspect(root).unwrap().revision, sibling.clone())
        .unwrap();
    hold(&host, child_id).await;
    denied(&child, delegated("independently held")).await;
    complete(&test.codex, input("parent remains active")).await;
    complete(&sibling, delegated("sibling remains active")).await;
    hold(&host, root).await;
    assert!(host.inspect(sibling_id).unwrap().inherited_hold);
    assert_eq!(
        host.resume(child_id, &host.inspect(child_id).unwrap().revision),
        Err(Error::Held)
    );
    denied(&sibling, delegated("inherited hold")).await;
    host.resume(root, &host.inspect(root).unwrap().revision)
        .unwrap();
    assert!(host.inspect(child_id).unwrap().local_hold);
    assert!(!host.inspect(child_id).unwrap().inherited_hold);
    denied(&child, delegated("still independently held")).await;
    complete(&test.codex, input("root readmitted")).await;
    complete(&sibling, delegated("sibling readmitted")).await;
    host.resume(child_id, &host.inspect(child_id).unwrap().revision)
        .unwrap();
    complete(&child, delegated("child explicitly readmitted")).await;
    assert_eq!(observed.requests().len(), 5);
    assert!(
        !observed
            .requests()
            .last()
            .unwrap()
            .body_contains_text("independently held")
    );
}

#[tokio::test]
async fn sealing_interrupts_both_streams_and_keeps_controllers_inspectable() {
    let (root_release, root_gate) = oneshot::channel();
    let (child_release, child_gate) = oneshot::channel();
    let pending = |id: &str, gate| {
        vec![
            StreamingSseChunk {
                gate: None,
                body: responses::sse(vec![responses::ev_response_created(id)]),
            },
            StreamingSseChunk {
                gate: Some(gate),
                body: responses::sse(vec![responses::ev_completed(id)]),
            },
        ]
    };
    let (server, _) = start_streaming_sse_server(vec![
        pending("root", root_gate),
        pending("child", child_gate),
        vec![StreamingSseChunk {
            gate: None,
            body: responses::sse_completed("root-resumed"),
        }],
        vec![StreamingSseChunk {
            gate: None,
            body: responses::sse_completed("child-resumed"),
        }],
    ])
    .await;
    let (host, _owner) = Lifecycle::new(Duration::from_secs(5));
    let test = test_codex()
        .with_extensions(extensions(&host))
        .with_model("gpt-5.4")
        .build_with_streaming_server(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    let child = child(&test).await;
    host.attach_child(root, &host.inspect(root).unwrap().revision, child.clone())
        .unwrap();
    test.codex
        .start_or_steer_turn(input("root active"))
        .await
        .unwrap();
    timeout(Duration::from_secs(10), server.wait_for_request_count(1))
        .await
        .unwrap();
    child
        .start_or_steer_turn(delegated("child active"))
        .await
        .unwrap();
    timeout(Duration::from_secs(10), server.wait_for_request_count(2))
        .await
        .unwrap();
    hold(&host, root).await;
    for thread in [&test.codex, &child] {
        timeout(
            Duration::from_secs(10),
            wait_for_event(thread, |e| matches!(e, EventMsg::TurnAborted(_))),
        )
        .await
        .unwrap();
        assert!(!thread.config_snapshot().await.model.is_empty());
    }
    denied(&test.codex, input("root rejected")).await;
    denied(&child, delegated("child rejected")).await;
    assert_eq!(server.requests().await.len(), 2);
    let _ = root_release.send(());
    let _ = child_release.send(());
    host.resume(root, &host.inspect(root).unwrap().revision)
        .unwrap();
    complete(&test.codex, input("root explicitly resumed")).await;
    complete(&child, delegated("child explicitly resumed")).await;
    assert_eq!(server.requests().await.len(), 4);
    child.shutdown_and_wait().await.unwrap();
    test.codex.shutdown_and_wait().await.unwrap();
    server.shutdown().await;
}

#[tokio::test]
async fn cancelled_waiter_preserves_seal_and_drains_issued_permits() {
    let server = responses::start_mock_server().await;
    let (host, _owner) = Lifecycle::new(Duration::from_secs(5));
    let test = test_codex()
        .with_extensions(extensions(&host))
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    let permit = host.admit_turn_start_for_thread(root).unwrap();
    let waiter = host
        .hold(root, &host.inspect(root).unwrap().revision)
        .unwrap();
    assert!(host.admit_continuation_start_for_thread(root).is_none());
    assert_eq!(
        host.resume(root, &host.inspect(root).unwrap().revision),
        Err(Error::Busy)
    );
    assert!(!host.inspect(root).unwrap().interrupt_complete);
    drop(waiter);
    drop(permit);
    timeout(Duration::from_secs(10), async {
        while !host.inspect(root).unwrap().interrupt_complete {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(host.inspect(root).unwrap().local_hold);
    host.resume(root, &host.inspect(root).unwrap().revision)
        .unwrap();
    assert!(host.admit_turn_start_for_thread(root).is_some());
}

#[tokio::test]
async fn timed_out_drain_requires_successful_interruption_before_resume() {
    let server = responses::start_mock_server().await;
    let (host, _owner) = Lifecycle::new(Duration::from_millis(50));
    let test = test_codex()
        .with_extensions(extensions(&host))
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    let permit = host.admit_continuation_start_for_thread(root).unwrap();
    assert_eq!(
        host.hold(root, &host.inspect(root).unwrap().revision)
            .unwrap()
            .wait()
            .await,
        Err(Error::DrainTimeout)
    );
    assert_eq!(
        host.resume(root, &host.inspect(root).unwrap().revision),
        Err(Error::IncompleteInterruption)
    );
    assert_eq!(
        host.inspect(root).unwrap().interruption_error,
        Some(Error::DrainTimeout)
    );
    assert!(host.admit_turn_start_for_thread(root).is_none());
    drop(permit);
    hold(&host, root).await;
    host.resume(root, &host.inspect(root).unwrap().revision)
        .unwrap();
}

#[tokio::test]
async fn stale_foreign_and_unregistered_scope_cannot_gain_authority() {
    let server = responses::start_mock_server().await;
    let (host, _owner) = Lifecycle::new(Duration::from_secs(5));
    let test = test_codex()
        .with_extensions(extensions(&host))
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    let original = host.inspect(root).unwrap().revision;
    let unregistered = child(&test).await;
    denied(&unregistered, delegated("unknown scope")).await;
    hold(&host, root).await;
    assert_eq!(host.resume(root, &original), Err(Error::StaleRevision));
    assert_eq!(
        host.attach_child(
            root,
            &host.inspect(root).unwrap().revision,
            unregistered.clone()
        ),
        Err(Error::Held)
    );
    let (foreign, _foreign_owner) = Lifecycle::new(Duration::from_secs(5));
    let other = test_codex()
        .with_extensions(extensions(&foreign))
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let other_id = foreign.attach_root(other.codex.clone()).unwrap();
    assert_eq!(
        host.resume(root, &foreign.inspect(other_id).unwrap().revision),
        Err(Error::StaleRevision)
    );
    host.resume(root, &host.inspect(root).unwrap().revision)
        .unwrap();
    let before_registration = host.inspect(root).unwrap().revision;
    host.attach_child(root, &before_registration, unregistered.clone())
        .unwrap();
    assert!(matches!(
        host.hold(root, &before_registration),
        Err(Error::StaleRevision)
    ));
    assert_eq!(
        host.attach_child(root, &host.inspect(root).unwrap().revision, unregistered),
        Err(Error::DuplicateThread)
    );
    assert!(host.admit_turn_start().is_none());
    assert!(host.admit_continuation_start().is_none());
}

#[tokio::test]
async fn owner_loss_interrupts_active_work_and_cannot_be_resumed() {
    let (release, gate) = oneshot::channel();
    let (server, _) = start_streaming_sse_server(vec![vec![
        StreamingSseChunk {
            gate: None,
            body: responses::sse(vec![responses::ev_response_created("active")]),
        },
        StreamingSseChunk {
            gate: Some(gate),
            body: responses::sse(vec![responses::ev_completed("active")]),
        },
    ]])
    .await;
    let (host, owner) = Lifecycle::new(Duration::from_secs(5));
    let test = test_codex()
        .with_extensions(extensions(&host))
        .with_model("gpt-5.4")
        .build_with_streaming_server(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    test.codex
        .start_or_steer_turn(input("active before owner loss"))
        .await
        .unwrap();
    timeout(Duration::from_secs(10), server.wait_for_request_count(1))
        .await
        .unwrap();
    drop(owner);
    assert!(!host.inspect(root).unwrap().owner_attached);
    assert!(host.admit_turn_start_for_thread(root).is_none());
    assert!(host.admit_continuation_start_for_thread(root).is_none());
    timeout(
        Duration::from_secs(10),
        wait_for_event(&test.codex, |e| matches!(e, EventMsg::TurnAborted(_))),
    )
    .await
    .unwrap();
    assert_eq!(
        host.resume(root, &host.inspect(root).unwrap().revision),
        Err(Error::OwnerLost)
    );
    denied(&test.codex, input("no implicit reconnect")).await;
    assert_eq!(server.requests().await.len(), 1);
    let _ = release.send(());
    test.codex.shutdown_and_wait().await.unwrap();
    server.shutdown().await;
}

#[tokio::test]
async fn released_controller_is_not_retained_or_reported_as_interrupted() {
    let server = responses::start_mock_server().await;
    let (host, _owner) = Lifecycle::new(Duration::from_secs(5));
    let test = test_codex()
        .with_extensions(extensions(&host))
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    let weak = Arc::downgrade(&test.codex);
    test.codex.shutdown_and_wait().await.unwrap();
    drop(test.thread_manager.remove_thread(&root).await);
    drop(test);
    timeout(Duration::from_secs(10), async {
        while weak.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        host.hold(root, &host.inspect(root).unwrap().revision)
            .unwrap()
            .wait()
            .await,
        Err(Error::InterruptFailed)
    );
    let view = host.inspect(root).unwrap();
    assert!(view.local_hold);
    assert!(!view.interrupt_complete);
    assert_eq!(view.interruption_error, Some(Error::InterruptFailed));
    assert_eq!(
        host.resume(root, &view.revision),
        Err(Error::IncompleteInterruption)
    );
}
