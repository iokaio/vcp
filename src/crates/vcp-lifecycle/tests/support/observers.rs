// SPDX-License-Identifier: Apache-2.0
use super::hooks::Fixture;
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};
use vcp_lifecycle::foundation::{
    observers::{Configuration, Limits},
    verification::VerificationConfig,
};
use vcp_tools::{
    process::{Mode, Profile},
    verification::{Requirement, Runner},
};

fn enabled(max_attempts: u32) -> Configuration {
    Configuration {
        enabled: true,
        limits: Limits {
            max_attempts,
            debounce_ms: 0,
            ..Limits::default()
        },
    }
}
async fn fixture(backend: BackendKind) -> Fixture {
    let mut f = Fixture::new(backend).await;
    fs::write(f.workspace.join("value.txt"), "41\n").unwrap();
    fs::write(
        f.workspace.join("package.json"),
        r#"{"scripts":{"test":"node --test acceptance.cjs"}}"#,
    )
    .unwrap();
    fs::write(f.workspace.join("acceptance.cjs"),"const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs');test('observer_acceptance',()=>assert.equal(fs.readFileSync('value.txt','utf8').trim(),'42'));\n").unwrap();
    let executable = f._temp.path().join("node.exe");
    fs::copy(
        std::env::var_os("VCP_TEST_NODE").expect("explicit native Node fixture required"),
        &executable,
    )
    .unwrap();
    f.policy.revision = PolicyRevision::new(1);
    f.policy
        .workspace_roots
        .insert(RootId::parse("exec-node").unwrap());
    f.host
        .command(
            Command::SetPolicy {
                policy: f.policy.clone(),
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
    f.host
        .configure_process_profile(
            Profile::new(
                "node".into(),
                executable,
                Mode::Direct,
                BTreeMap::from([("SystemRoot".into(), std::env::var("SystemRoot").unwrap())]),
                BTreeSet::new(),
                true,
            )
            .unwrap(),
        )
        .unwrap();
    f.host
        .configure_verification(
            f.thread,
            VerificationConfig {
                requirements: vec![Requirement {
                    timeout_ms: None,
                    manifest: "package.json".into(),
                    runner: Runner::Node,
                    profile: "node".into(),
                    expected_tests: vec!["observer_acceptance".into()],
                    rationale: "Observe repeated actual failed checks".into(),
                }],
                rationale: "Native observer qualification baseline".into(),
            },
        )
        .unwrap();
    f
}
async fn verify(f: &Fixture, count: usize) -> Vec<VerificationId> {
    let mut ids = Vec::new();
    for _ in 0..count {
        let report = f.host.verify(f.thread, vec![]).await.unwrap();
        assert!(report.checks.iter().any(|c| matches!(
            c.outcome,
            vcp_domain::verification::CheckOutcome::Failed { .. }
        )));
        ids.push(report.id);
    }
    ids
}
async fn drain(f: &Fixture) -> serde_json::Value {
    let mut status = f.host.observer_status(f.thread).unwrap();
    for _ in 0..16 {
        let next = f.host.poll_observers(f.thread).await.unwrap();
        if next == status {
            return next;
        }
        status = next;
    }
    status
}
fn main_attempts(f: &Fixture) -> usize {
    f.host
        .snapshot()
        .unwrap()
        .records
        .values()
        .filter(|r| r.collection == Collection::Attempt)
        .count()
}
fn task(f: &Fixture) -> Task {
    f.host
        .snapshot()
        .unwrap()
        .record(
            Collection::Task,
            f.config.root_task.as_str(),
            &f.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_observer_matched_trace_groups_three_failed_checks_without_extra_inference() {
    let mut campaign = Vec::new();
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for enabled_observer in [false, true] {
            let f = fixture(backend).await;
            let before = f.host.snapshot().unwrap();
            assert_eq!(
                f.host.poll_observers(f.thread).await.unwrap()["enabled"],
                false
            );
            assert_eq!(
                f.host.snapshot().unwrap().watermark,
                before.watermark,
                "defaultoff poll must not write"
            );
            let ids = verify(&f, 3).await;
            if enabled_observer {
                f.host.configure_observers(enabled(4)).unwrap();
            }
            let started = std::time::Instant::now();
            let status = drain(&f).await;
            let elapsed = started.elapsed().as_millis();
            assert_eq!(main_attempts(&f), 0);
            if enabled_observer {
                let proposals = status["state"]["proposals"].as_array().unwrap();
                assert_eq!(proposals.len(), 1, "{status}");
                let grouped: BTreeSet<VerificationId> =
                    serde_json::from_value(proposals[0]["verifications"].clone()).unwrap();
                assert_eq!(grouped, ids.into_iter().collect());
                assert_eq!(proposals[0]["disposition"], "current");
                assert_eq!(status["provider_requests"], 0);
                assert_eq!(status["charged_micros"], 0);
                let budget = status["state"]["budget"].clone();
                for _ in 0..8 {
                    assert_eq!(
                        f.host.poll_observers(f.thread).await.unwrap()["state"]["budget"],
                        budget
                    );
                }
                assert_eq!(
                    f.host.observer_status(f.thread).unwrap()["state"]["proposals"]
                        .as_array()
                        .unwrap()
                        .len(),
                    1
                );
            } else {
                assert!(status["state"].is_null());
            }
            campaign.push(serde_json::json!({"backend":format!("{backend:?}"),"enabled":enabled_observer,"fixture":"three_identical_actual_failed_checks","main_requests":0,"helper_requests":0,"charged_micros":0,"uncertain_liability_micros":0,"interventions":0,"failed_verifications":3,"grouped_repetition_facts":if enabled_observer{1}else{0},"local_poll_elapsed_millis":elapsed,"local_measured":status.get("measured"),"budget":status.pointer("/state/budget"),"task_success_claim":false}));
            f.close().await;
        }
    }
    println!(
        "P10_OBSERVER_NATIVE_MATCHED_CAMPAIGN={}",
        serde_json::to_string(&campaign).unwrap()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_observer_nonrepeat_and_changed_input_do_not_publish_repetition() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for correction in [false, true] {
            let f = fixture(backend).await;
            verify(&f, 2).await;
            if correction {
                fs::write(f.workspace.join("value.txt"), "43\n").unwrap();
                verify(&f, 1).await;
            }
            f.host.configure_observers(enabled(4)).unwrap();
            let status = drain(&f).await;
            assert!(
                status["state"]["proposals"].as_array().unwrap().is_empty(),
                "{status}"
            );
            assert_eq!(main_attempts(&f), 0);
            f.close().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_observer_pause_correction_and_exhausted_local_budget_preserve_history() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = fixture(backend).await;
        verify(&f, 3).await;
        f.host.configure_observers(enabled(1)).unwrap();
        let observed = drain(&f).await;
        assert_eq!(
            observed["state"]["proposals"].as_array().unwrap().len(),
            1,
            "{observed}"
        );
        let current = task(&f);
        let mut objective = current.objectives.last().unwrap().clone();
        objective.text = "Owner corrected the task; retain prior repetition only as history".into();
        f.host
            .change_authority(
                Command::Steer { objective },
                Some(f.config.root_task.clone()),
                current.revision,
            )
            .unwrap()
            .wait()
            .await
            .unwrap();
        let paused = f.host.observer_status(f.thread).unwrap();
        assert_eq!(paused["state"]["proposals"][0]["disposition"], "historical");
        let before = f.host.snapshot().unwrap().watermark;
        for _ in 0..3 {
            let _ = f.host.poll_observers(f.thread).await.unwrap();
        }
        assert_eq!(
            f.host.snapshot().unwrap().watermark,
            before,
            "pause must stop fresh local scheduling"
        );
        let held = f.host.lifecycle().inspect(f.thread).unwrap();
        f.host.lifecycle().resume(f.thread, &held.revision).unwrap();
        let paused_task = task(&f);
        f.host
            .resume(f.thread, paused_task.revision, paused_task.fingerprint)
            .unwrap();
        verify(&f, 3).await;
        let exhausted = drain(&f).await;
        assert_eq!(exhausted["state"]["budget"]["attempts"], 1);
        assert_eq!(exhausted["state"]["attempts"].as_array().unwrap().len(), 1);
        assert_eq!(exhausted["state"]["proposals"].as_array().unwrap().len(), 1);
        assert_eq!(main_attempts(&f), 0);
        f.close().await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_observer_shared_root_cap_stops_local_work_without_provider_liability() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let mut configuration = config(&temp.path().join("canonical"), &workspace, backend);
        configuration.cap.micros = Micros::ZERO;
        let (host, owner) = CanonicalHost::open(configuration.clone()).unwrap();
        let binding = super::task(&host, &configuration, configuration.root_task.clone(), None);
        let server = start_mock_server().await;
        let mut registry = ExtensionRegistryBuilder::new();
        registry.turn_start_admission(Arc::new(host.clone()));
        registry.work_admission(Arc::new(host.clone()));
        let starter = host.clone();
        let cwd = workspace.clone();
        let test = test_codex()
            .with_extensions(Arc::new(registry.build()))
            .with_auth(codex_login::CodexAuth::from_api_key(
                "synthetic-observer-cap",
            ))
            .with_allowed_tools(AllowedTools(vec![]))
            .with_config(move |c| {
                c.cwd = cwd.try_into().unwrap();
                configure_fixture_provider(c);
                starter
                    .lifecycle()
                    .authorize_startup(c.cwd.as_path(), None)
                    .unwrap();
            })
            .build_with_auto_env(&server)
            .await
            .unwrap();
        let thread = host.lifecycle().attach_root(test.codex.clone()).unwrap();
        host.register(thread, binding).unwrap();
        host.configure_observers(enabled(4)).unwrap();
        let before = host.snapshot().unwrap();
        let status = host.poll_observers(thread).await.unwrap();
        assert!(
            status["notice"]
                .as_str()
                .unwrap()
                .contains("shared root cost cap exhausted"),
            "{status}"
        );
        assert_eq!(status["state"]["budget"]["attempts"], 0);
        assert_eq!(status["provider_requests"], 0);
        let after = host.snapshot().unwrap();
        for (key, row) in before
            .records
            .iter()
            .filter(|(_, r)| matches!(r.collection, Collection::Ledger | Collection::Attempt))
        {
            assert_eq!(after.records.get(key), Some(row));
        }
        assert!(!after
            .records
            .values()
            .any(|r| r.collection == Collection::Attempt));
        assert!(server.received_requests().await.unwrap().is_empty());
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_observer_enabled_before_checks_runs_only_at_real_verification_safe_points() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = fixture(backend).await;
        f.host.configure_observers(enabled(8)).unwrap();
        let ids = verify(&f, 3).await;
        let status = f.host.observer_status(f.thread).unwrap();
        assert_eq!(
            status["state"]["proposals"].as_array().unwrap().len(),
            1,
            "automatic verification safe point omitted: {status}"
        );
        let grouped: BTreeSet<VerificationId> =
            serde_json::from_value(status["state"]["proposals"][0]["verifications"].clone())
                .unwrap();
        assert_eq!(grouped, ids.into_iter().collect());
        assert_eq!(main_attempts(&f), 0);
        assert_eq!(status["provider_requests"], 0);
        f.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_observer_own_event_backlog_does_not_starve_verification_facts() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = fixture(backend).await;
        // Each explicit configuration emits an observer-owned event. More than
        // one event page must not prevent the later real verification facts
        // from reaching the subscription.
        for _ in 0..130 {
            f.host.configure_observers(enabled(8)).unwrap();
        }
        let ids = verify(&f, 3).await;
        let status = drain(&f).await;
        assert_eq!(
            status["state"]["proposals"].as_array().unwrap().len(),
            1,
            "{status}"
        );
        let grouped: BTreeSet<VerificationId> =
            serde_json::from_value(status["state"]["proposals"][0]["verifications"].clone())
                .unwrap();
        assert_eq!(grouped, ids.into_iter().collect());
        let watermark = f.host.snapshot().unwrap().watermark;
        let budget = status["state"]["budget"].clone();
        let idle = f.host.poll_observers(f.thread).await.unwrap();
        assert_eq!(f.host.snapshot().unwrap().watermark, watermark);
        assert_eq!(idle["state"]["budget"], budget);
        assert_eq!(main_attempts(&f), 0);
        f.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_observer_verification_purge_redacts_proposals_without_resetting_budget() {
    use vcp_domain::retention_selector::{Criterion, Selector, Tree};
    use vcp_lifecycle::foundation::history_retention::Request;
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = fixture(backend).await;
        verify(&f, 3).await;
        f.host.configure_observers(enabled(4)).unwrap();
        let status = drain(&f).await;
        assert_eq!(
            status["state"]["proposals"].as_array().unwrap().len(),
            1,
            "{status}"
        );
        assert!(status["state"]["budget"]["attempts"].as_u64().unwrap() > 0);
        let observer_id = f
            .host
            .snapshot()
            .unwrap()
            .records
            .values()
            .find(|row| {
                row.collection == Collection::Projection
                    && row.value["document_type"] == "vcp_observer_state_v1"
            })
            .unwrap()
            .id
            .clone();
        f.host
            .command(
                Command::Transition {
                    next: TaskState::Failed,
                    reason: "terminal verification retention fixture".into(),
                    verification: None,
                },
                Some(f.config.root_task.clone()),
                task(&f).revision,
            )
            .unwrap();
        let preview = f
            .host
            .history_retention(Request::Preview {
                selector: Selector {
                    schema_version: 1,
                    tree: Tree::Match(Criterion::Event("verification_recorded".into())),
                },
                action: vcp_memory::retention::Action::Purge,
            })
            .unwrap();
        assert_eq!(preview["protected_count"], 0, "{preview}");
        let applied = f
            .host
            .history_retention(Request::Apply {
                preview: preview["id"].as_str().unwrap().into(),
            })
            .unwrap();
        assert_eq!(applied["logical_unavailable"], true);
        let hidden = f.host.observer_status(f.thread).unwrap();
        assert_eq!(hidden["enabled"], false, "{hidden}");
        assert!(hidden["state"].is_null(), "{hidden}");
        assert!(f
            .host
            .configure_observers(enabled(4))
            .unwrap_err()
            .contains("budget cannot be reset"));
        let cleaned = f
            .host
            .history_retention(Request::Cleanup {
                receipt: applied["id"].as_str().unwrap().into(),
            })
            .unwrap();
        assert_eq!(cleaned["rewrite_complete"], true, "{cleaned}");
        let snapshot = f.host.snapshot().unwrap();
        let tombstone = snapshot
            .records
            .values()
            .find(|row| row.collection == Collection::Projection && row.id == observer_id)
            .unwrap();
        assert_ne!(tombstone.value["document_type"], "vcp_observer_state_v1");
        assert!(
            tombstone.value.get("state").is_none(),
            "{}",
            tombstone.value
        );
        assert!(f.host.observer_status(f.thread).unwrap()["state"].is_null());
        assert!(f
            .host
            .configure_observers(enabled(4))
            .unwrap_err()
            .contains("budget cannot be reset"));
        assert_eq!(f.host.snapshot().unwrap().watermark, snapshot.watermark);
        assert_eq!(main_attempts(&f), 0);
        f.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_observer_reopen_keeps_historical_receipt_and_requires_explicit_enable() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = fixture(backend).await;
        verify(&f, 3).await;
        f.host.configure_observers(enabled(4)).unwrap();
        let status = drain(&f).await;
        assert_eq!(
            status["state"]["proposals"].as_array().unwrap().len(),
            1,
            "{status}"
        );
        let configuration = f.config.clone();
        let thread = f.thread;
        let binding = ThreadBinding {
            scope: Scope {
                workspace: configuration.workspace.clone(),
                session: configuration.session.clone(),
                task: configuration.root_task.clone(),
            },
            agent: AgentId::new(),
            role: RequestRole::Main,
        };
        f.owner.close().await.unwrap();
        // The retained thread and host retain worker handles after the owner
        // is fenced. Release them before acquiring a new canonical owner.
        f.test.codex.shutdown_and_wait().await.unwrap();
        drop(f.test);
        drop(f.host);
        let (host, owner) = CanonicalHost::open(configuration.clone()).unwrap();
        host.register(thread, binding).unwrap();
        let reopened = host.observer_status(thread).unwrap();
        assert_eq!(reopened["enabled"], false);
        assert_eq!(
            reopened["state"]["proposals"][0]["disposition"],
            "historical"
        );
        let before = host.snapshot().unwrap().watermark;
        let _ = host.poll_observers(thread).await.unwrap();
        assert_eq!(host.snapshot().unwrap().watermark, before);
        assert_eq!(reopened["state"]["budget"], status["state"]["budget"]);
        owner.close().await.unwrap();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_observer_late_local_result_obeys_pause_correction_and_deadline() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for boundary in ["pause", "correction", "deadline"] {
            let f = fixture(backend).await;
            verify(&f, 3).await;
            f.host.configure_observers(enabled(4)).unwrap();
            let arrived = Arc::new(tokio::sync::Notify::new());
            let release = Arc::new(tokio::sync::Notify::new());
            let host = f.host.clone();
            let thread = f.thread;
            let ready = arrived.clone();
            let permit = release.clone();
            let mut pending = tokio::spawn(async move {
                host.qualification_poll_observers_after_compute(thread, ready, permit)
                    .await
            });
            tokio::select! {
                result = &mut pending => panic!("observer returned before compute barrier: {result:?}"),
                result = tokio::time::timeout(Duration::from_secs(10), arrived.notified()) => result.unwrap(),
            }
            let state = f.host.observer_status(f.thread).unwrap();
            assert_eq!(state["state"]["attempts"][0]["status"], "running");
            if boundary == "deadline" {
                tokio::time::sleep(Duration::from_millis(2100)).await;
            } else {
                let current = task(&f);
                if boundary == "pause" {
                    f.host
                        .stop(
                            f.host
                                .control_envelope(
                                    CommandId::new(),
                                    f.config.root_task.clone(),
                                    current.revision,
                                    Command::Transition {
                                        next: TaskState::Paused,
                                        reason: "pause while local observation result waits".into(),
                                        verification: None,
                                    },
                                )
                                .unwrap(),
                        )
                        .unwrap();
                } else {
                    let mut objective = current.objectives.last().unwrap().clone();
                    objective.text = "corrected while local observation waits".into();
                    f.host
                        .change_authority(
                            Command::Steer { objective },
                            Some(f.config.root_task.clone()),
                            current.revision,
                        )
                        .unwrap()
                        .wait()
                        .await
                        .unwrap();
                }
            }
            release.notify_one();
            let result = tokio::time::timeout(Duration::from_secs(10), pending)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let proposals = result["state"]["proposals"].as_array().unwrap();
            if boundary == "deadline" {
                assert!(
                    proposals.is_empty(),
                    "expired local result must not publish"
                );
                assert_eq!(result["state"]["attempts"][0]["status"], "expired");
            } else {
                assert!(
                    proposals.iter().all(|p| p["disposition"] == "historical"),
                    "late advice cannot become current: {result}"
                );
            }
            assert_eq!(main_attempts(&f), 0);
            f.close().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "launched by independent native owner-loss supervisor"]
async fn native_observer_kill_owner() {
    let root = std::path::PathBuf::from(std::env::var_os("VCP_OBSERVER_KILL_ROOT").unwrap());
    let backend = if std::env::var("VCP_OBSERVER_KILL_BACKEND").unwrap() == "files" {
        BackendKind::Files
    } else {
        BackendKind::Sqlite
    };
    let mut f = fixture(backend).await;
    f._temp.disable_cleanup(true);
    verify(&f, 3).await;
    f.host.configure_observers(enabled(4)).unwrap();
    let arrived = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let host = f.host.clone();
    let thread = f.thread;
    let ready = arrived.clone();
    let mut pending = tokio::spawn(async move {
        host.qualification_poll_observers_after_compute(thread, ready, release)
            .await
    });
    tokio::select! {
        result = &mut pending => panic!("owner observer returned before compute barrier: {result:?}"),
        result = tokio::time::timeout(Duration::from_secs(10), arrived.notified()) => result.unwrap(),
    }
    let record = serde_json::json!({"config":f.config,"thread":thread,"fixture_root":f._temp.path(),"status":f.host.observer_status(thread).unwrap()});
    fs::write(
        root.join("barrier.tmp"),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();
    fs::rename(root.join("barrier.tmp"), root.join("barrier.json")).unwrap();
    tokio::time::sleep(Duration::from_secs(60)).await;
    panic!("supervisor failed to kill local observer owner");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_observer_owner_process_loss_never_replays_pending_local_work() {
    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for backend in ["files", "sqlite"] {
        let root = tempfile::tempdir().unwrap();
        let mut child = ChildGuard(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "observers::native_observer_kill_owner",
                    "--ignored",
                    "--nocapture",
                ])
                .env("VCP_OBSERVER_KILL_ROOT", root.path())
                .env("VCP_OBSERVER_KILL_BACKEND", backend)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        tokio::time::timeout(Duration::from_secs(45), async {
            while !root.path().join("barrier.json").exists() {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "observer owner exited before durable running barrier"
                );
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap();
        let barrier: serde_json::Value =
            serde_json::from_slice(&fs::read(root.path().join("barrier.json")).unwrap()).unwrap();
        assert_eq!(
            barrier["status"]["state"]["attempts"][0]["status"],
            "running"
        );
        child.0.kill().unwrap();
        assert!(!child.0.wait().unwrap().success());
        let configuration: Config = serde_json::from_value(barrier["config"].clone()).unwrap();
        let thread: codex_protocol::ThreadId =
            serde_json::from_value(barrier["thread"].clone()).unwrap();
        for _ in 0..2 {
            let (host, owner) = CanonicalHost::open(configuration.clone()).unwrap();
            host.register(
                thread,
                ThreadBinding {
                    scope: Scope {
                        workspace: configuration.workspace.clone(),
                        session: configuration.session.clone(),
                        task: configuration.root_task.clone(),
                    },
                    agent: AgentId::new(),
                    role: RequestRole::Main,
                },
            )
            .unwrap();
            let status = host.observer_status(thread).unwrap();
            assert_eq!(status["enabled"], false);
            assert_eq!(status["state"]["attempts"][0]["status"], "interrupted");
            assert_eq!(status["state"]["attempts"].as_array().unwrap().len(), 1);
            assert!(status["state"]["proposals"].as_array().unwrap().is_empty());
            assert_eq!(
                status["state"]["budget"],
                barrier["status"]["state"]["budget"]
            );
            let watermark = host.snapshot().unwrap().watermark;
            let _ = host.poll_observers(thread).await.unwrap();
            assert_eq!(host.snapshot().unwrap().watermark, watermark);
            assert!(!host
                .snapshot()
                .unwrap()
                .records
                .values()
                .any(|r| r.collection == Collection::Attempt));
            owner.close().await.unwrap();
        }
        eprintln!(
            "observer owner-kill evidence {backend}: {}",
            barrier["fixture_root"]
        );
    }
}
