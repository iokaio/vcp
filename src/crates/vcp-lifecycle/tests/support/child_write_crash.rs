// SPDX-License-Identifier: Apache-2.0
//! Kill the owner inside a registered child's native write, before its receipt.
use super::*;
use std::{io::Write, path::Path, process::Stdio, time::Instant};
use vcp_domain::effect::{Effect, EffectState};
use vcp_lifecycle::foundation::FileDispatchPoint;

#[test]
#[ignore = "supervisor launches this registered child write fault"]
fn child_native_write_fault_process() {
    let root = PathBuf::from(std::env::var_os("VCP_CHILD_WRITE_FAULT_ROOT").unwrap());
    let backend = match std::env::var("VCP_CHILD_WRITE_FAULT_BACKEND")
        .unwrap()
        .as_str()
    {
        "files" => BackendKind::Files,
        "sqlite" => BackendKind::Sqlite,
        _ => panic!("unknown child write backend"),
    };
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(run_child_write_fault(&root, backend));
}

async fn run_child_write_fault(root: &Path, backend: BackendKind) {
    let mut f = Fixture::new_in(backend, 1, tempfile::tempdir_in(root).unwrap()).await;
    let child = f.assign(&["left.txt", "outside.txt"]).await;
    let sibling = f.assign(&["right.txt"]).await;
    let (thread, child_path, child_loop) = f.start(&child).await;
    let (sibling_thread, sibling_path, _) = f.start(&sibling).await;
    let (snapshot, _) = provider_snapshot();
    let sealed = sealed_provider_context(&f.host, thread, &snapshot, None);
    f.host
        .prepare_context(thread, sealed, serde_json::json!([]), vec![])
        .unwrap();
    child_loop
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "Observe the assigned source before the scoped write.".into(),
            text_elements: vec![],
        }]))
        .await
        .unwrap();
    wait_for_event_with_timeout(
        &child_loop,
        |event| {
            if let EventMsg::Error(error) = event {
                panic!("child observation failed: {error:?}");
            }
            matches!(event, EventMsg::TurnComplete(_))
        },
        Duration::from_secs(30),
    )
    .await;
    assert_eq!(f.server.received_requests().await.unwrap().len(), 1);
    let sibling_edit = f
        .host
        .prepare_tool(
            sibling_thread,
            patch("right.txt", "base", "sibling retained"),
        )
        .unwrap();
    f.host.dispatch_tool(sibling_edit).unwrap();
    fs::write(f.workspace.join("outside.txt"), b"parent human edit\n").unwrap();
    let transcript = f
        .host
        .open_output(thread, Channel::ChildTranscript)
        .unwrap();
    transcript
        .write(b"Observed assigned source; isolated write is pending.")
        .unwrap();
    let transcript = transcript.finish().unwrap();
    let proposal = f
        .host
        .prepare_tool(thread, Request::Patch {
            patch: "*** Begin Patch\n*** Update File: left.txt\n@@\n-base\n+child native edit\n*** Update File: outside.txt\n@@\n-base\n+second child edit\n*** End Patch".into(),
        })
        .unwrap();
    let effect = proposal.effect().clone();
    fs::write(
        root.join("config.json"),
        serde_json::to_vec(&f.config).unwrap(),
    )
    .unwrap();
    fs::write(
        root.join("before.json"),
        serde_json::to_vec(&f.host.snapshot().unwrap()).unwrap(),
    )
    .unwrap();
    let marker = serde_json::json!({
        "phase":"child_native_write_before_receipt", "effect":effect, "child":child,
        "sibling":sibling, "child_path":child_path, "sibling_path":sibling_path,
        "transcript":transcript.spec.id, "wire_requests":1
    });
    let root = root.to_path_buf();
    f.host
        .qualification_dispatch_tool_with_file_observer(proposal, move |point| {
            if point == FileDispatchPoint::BeforeReceipt(0) {
                // The canonical worker is stopped here. Do not reenter the host.
                let temporary = root.join("barrier.tmp");
                let mut file = fs::File::create(&temporary).unwrap();
                file.write_all(&serde_json::to_vec(&marker).unwrap())
                    .unwrap();
                file.sync_all().unwrap();
                drop(file);
                fs::rename(temporary, root.join("barrier.json")).unwrap();
                loop {
                    std::thread::park();
                }
            }
            Ok(())
        })
        .unwrap();
    panic!("child native write interruption unexpectedly returned");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn child_native_write_process_kill_retains_unknown_effect_without_replay() {
    struct Supervised(std::process::Child);
    impl Drop for Supervised {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let mut process = Supervised(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "child_graph_dispatch::child_write_crash::child_native_write_fault_process",
                    "--ignored",
                    "--nocapture",
                ])
                .env("VCP_CHILD_WRITE_FAULT_ROOT", root)
                .env(
                    "VCP_CHILD_WRITE_FAULT_BACKEND",
                    if backend == BackendKind::Files {
                        "files"
                    } else {
                        "sqlite"
                    },
                )
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let barrier = root.join("barrier.json");
        let deadline = Instant::now() + Duration::from_secs(60);
        while !barrier.exists() {
            if let Some(status) = process.0.try_wait().unwrap() {
                panic!("child write process exited before receipt barrier: {status}");
            }
            assert!(
                Instant::now() < deadline,
                "child write receipt barrier timed out"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let marker: serde_json::Value =
            serde_json::from_slice(&fs::read(&barrier).unwrap()).unwrap();
        assert_eq!(marker["phase"], "child_native_write_before_receipt");
        assert_eq!(marker["wire_requests"], 1);
        let config: Config =
            serde_json::from_slice(&fs::read(root.join("config.json")).unwrap()).unwrap();
        let child = TaskId::parse(marker["child"].as_str().unwrap()).unwrap();
        let sibling = TaskId::parse(marker["sibling"].as_str().unwrap()).unwrap();
        let effect_id = ToolRunId::parse(marker["effect"].as_str().unwrap()).unwrap();
        let transcript = ArtifactId::parse(marker["transcript"].as_str().unwrap()).unwrap();
        let child_path = PathBuf::from(marker["child_path"].as_str().unwrap());
        let sibling_path = PathBuf::from(marker["sibling_path"].as_str().unwrap());
        let workspace = PathBuf::from(&config.binding.root);
        for path in [
            &config.canonical_root,
            &child_path,
            &sibling_path,
            &workspace,
        ] {
            assert!(path
                .canonicalize()
                .unwrap()
                .starts_with(root.canonicalize().unwrap()));
        }
        assert_eq!(
            fs::read(child_path.join("left.txt")).unwrap(),
            b"child native edit\n"
        );
        assert_eq!(fs::read(workspace.join("left.txt")).unwrap(), b"base\n");
        assert_eq!(
            fs::read(sibling_path.join("right.txt")).unwrap(),
            b"sibling retained\n"
        );
        process.0.kill().unwrap();
        assert!(!process.0.wait().unwrap().success());
        // The first write survives. An independent edit in the not-yet-written
        // second path must remain conflicted rather than be overwritten by replay.
        assert_eq!(fs::read(child_path.join("outside.txt")).unwrap(), b"base\n");
        fs::write(child_path.join("outside.txt"), b"later human child edit\n").unwrap();
        let before: vcp_store::contract::State =
            serde_json::from_slice(&fs::read(root.join("before.json")).unwrap()).unwrap();
        let old_effect: Effect = before
            .record(Collection::Effect, effect_id.as_str(), &config.workspace)
            .unwrap()
            .decode()
            .unwrap();
        let scope = old_effect.scope.clone();
        assert_eq!(scope.task, child);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let reports = host.reconcile_effects().unwrap();
        let state = host.snapshot().unwrap();
        let effect: Effect = state
            .record(Collection::Effect, effect_id.as_str(), &config.workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(effect.state, EffectState::OutcomeUnknown);
        assert_eq!(effect.scope, scope);
        assert_eq!(effect.operation_digest, old_effect.operation_digest);
        assert_eq!(effect.steering, old_effect.steering);
        assert!(effect.execution.is_some());
        for task_id in [&config.root_task, &child, &sibling] {
            let task: Task = state
                .record(Collection::Task, task_id.as_str(), &config.workspace)
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(task.state, TaskState::Paused);
            assert!(host
                .command(
                    Command::Transition {
                        next: TaskState::Completed,
                        reason: "An interrupted child write cannot complete work".into(),
                        verification: None
                    },
                    Some(task_id.clone()),
                    task.revision
                )
                .is_err());
        }
        let attempts = |snapshot: &vcp_store::contract::State| {
            snapshot
                .records
                .values()
                .filter(|row| row.collection == Collection::Attempt)
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(
            attempts(&state),
            attempts(&before),
            "reopening must not issue another provider request"
        );
        assert_eq!(attempts(&state).len(), 1);
        let ledger = vcp_budget::ledger(&state, &scope).unwrap();
        assert_eq!(ledger, vcp_budget::ledger(&before, &scope).unwrap());
        assert_eq!(
            (
                ledger.settled.get(),
                ledger.active.get(),
                ledger.unresolved.get()
            ),
            (100, 0, 0)
        );
        assert_eq!(ledger.allocations[&child], Micros::new(400));
        assert_eq!(ledger.allocations[&sibling], Micros::new(400));
        let graph = vcp_engine::agents::graph(&state, &scope, &config.root_task)
            .unwrap()
            .unwrap();
        let previous = vcp_engine::agents::graph(&before, &scope, &config.root_task)
            .unwrap()
            .unwrap();
        assert_eq!(graph.children, previous.children);
        assert_eq!(graph.ready, previous.ready);
        assert_eq!(graph.results, previous.results);
        for row in before
            .records
            .values()
            .filter(|row| row.collection == Collection::Artifact)
        {
            assert_eq!(
                state
                    .record(Collection::Artifact, &row.id, &config.workspace)
                    .unwrap(),
                row
            );
        }
        assert_eq!(
            host.read_artifact(transcript).unwrap(),
            b"Observed assigned source; isolated write is pending."
        );
        let mut intents = 0;
        let mut receipts = 0;
        for row in state
            .records
            .values()
            .filter(|row| row.collection == Collection::Artifact)
        {
            let artifact: ArtifactDescriptor = row.decode().unwrap();
            if matches!(
                artifact.spec.schema.as_str(),
                "vcp-file-intent-v1" | "vcp-file-outcome-v1"
            ) {
                let value: serde_json::Value =
                    serde_json::from_slice(&host.read_artifact(artifact.spec.id).unwrap()).unwrap();
                if value["effect"] == serde_json::json!(effect_id) {
                    assert_eq!(artifact.spec.scope.task, child);
                    if artifact.spec.schema == "vcp-file-intent-v1" {
                        intents += 1;
                    } else {
                        receipts += 1;
                    }
                }
            }
        }
        assert_eq!(
            (intents, receipts),
            (1, 0),
            "retain intent without inventing the unpublished receipt"
        );
        assert!(reports.iter().any(|report| {
            let value: serde_json::Value =
                serde_json::from_slice(&host.read_artifact(report.spec.id.clone()).unwrap())
                    .unwrap();
            value["effect"] == serde_json::json!(effect_id)
                && value["replayed"] == false
                && value["outcome"] == "outcome_unknown"
                && value["observations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| row["path"] == "left.txt" && row["certainty"] == "applied")
                && value["observations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| row["path"] == "outside.txt" && row["certainty"] == "conflicted")
        }));
        assert_eq!(
            fs::read(child_path.join("left.txt")).unwrap(),
            b"child native edit\n"
        );
        assert_eq!(fs::read(child_path.join("right.txt")).unwrap(), b"base\n");
        assert_eq!(
            fs::read(child_path.join("outside.txt")).unwrap(),
            b"later human child edit\n"
        );
        assert_eq!(fs::read(sibling_path.join("left.txt")).unwrap(), b"base\n");
        assert_eq!(
            fs::read(sibling_path.join("right.txt")).unwrap(),
            b"sibling retained\n"
        );
        assert_eq!(fs::read(workspace.join("left.txt")).unwrap(), b"base\n");
        assert_eq!(fs::read(workspace.join("right.txt")).unwrap(), b"base\n");
        assert_eq!(
            fs::read(workspace.join("outside.txt")).unwrap(),
            b"parent human edit\n"
        );
        let before_reconcile = host.snapshot().unwrap();
        host.reconcile_effects().unwrap();
        let after_reconcile = host.snapshot().unwrap();
        assert_eq!(attempts(&before_reconcile), attempts(&after_reconcile));
        assert_eq!(
            after_reconcile
                .record(Collection::Effect, effect_id.as_str(), &config.workspace)
                .unwrap(),
            before_reconcile
                .record(Collection::Effect, effect_id.as_str(), &config.workspace)
                .unwrap()
        );
        assert_eq!(
            fs::read(child_path.join("left.txt")).unwrap(),
            b"child native edit\n"
        );
        owner.close().await.unwrap();
    }
}
