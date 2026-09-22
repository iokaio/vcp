// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};
use vcp_domain::{policy::*, verification::*};
use vcp_lifecycle::foundation::verification::VerificationConfig;
use vcp_tools::{
    process::{Mode, Profile},
    verification::{Requirement, Runner},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_verification_binds_checks_to_sources_and_honest_completion() {
    let node =
        std::env::var_os("VCP_TEST_NODE").expect("runner supplies explicit native Node dependency");
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for mode in [
            "pass",
            "seeded",
            "missing",
            "zero",
            "wrong",
            "later",
            "analysis",
            "analysis_changed",
            "analysis_late_effect",
            "analysis_reopen",
            "ignored_later",
            "uncovered",
        ] {
            verification_case(backend, mode, &node).await;
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_verification_marks_concurrent_edit_stale() {
    let node =
        std::env::var_os("VCP_TEST_NODE").expect("runner supplies explicit native Node dependency");
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        verification_case(backend, "during", &node).await;
    }
}
#[cfg(feature = "qualification")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_verification_pause_before_publish_retains_checks_without_completion() {
    let node = std::env::var_os("VCP_TEST_NODE").expect("native Node dependency");
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        verification_case(backend, "pause_publish", &node).await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_verification_preserves_uncertain_dispatch_intent() {
    let node =
        std::env::var_os("VCP_TEST_NODE").expect("runner supplies explicit native Node dependency");
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        verification_case(backend, "spawn_failure", &node).await;
    }
}
async fn verification_case(backend: BackendKind, mode: &str, node: &std::ffi::OsStr) {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    let tools = temp.path().join("tools");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&tools).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    let exe = tools.join("node.exe");
    fs::copy(&node, &exe).unwrap();
    if mode == "spawn_failure" {
        fs::write(&exe, b"synthetic invalid executable").unwrap();
    }
    let project = if mode == "uncovered" {
        workspace.join("project")
    } else {
        workspace.clone()
    };
    fs::create_dir_all(&project).unwrap();
    let oracle = temp.path().join("observed.json");
    let release = temp.path().join("release-check");
    fs::write(project.join("value.txt"), b"41\n").unwrap();
    fs::write(
        workspace.join("AGENTS.md"),
        b"Check the actual value before completion.\n",
    )
    .unwrap();
    if mode == "ignored_later" {
        fs::write(workspace.join(".gitignore"), b"AGENTS.md\n").unwrap();
    }
    fs::write(
        project.join("package.json"),
        br#"{"scripts":{"test":"node --test acceptance.cjs"}}"#,
    )
    .unwrap();
    let name = if mode == "wrong" {
        "unrelated"
    } else {
        "seeded_acceptance"
    };
    let script = if mode == "zero" {
        "// no assertions or node:test cases\n".to_owned()
    } else {
        format!(
                "const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs');\ntest({name:?},async()=>{{fs.writeFileSync({oracle},'started');while({blocked}&&!fs.existsSync({release})){{await new Promise(r=>setTimeout(r,10));}}assert.equal(fs.readFileSync('value.txt','utf8').trim(),'42');fs.appendFileSync({oracle},':passed');}});\n",
                oracle=serde_json::to_string(&oracle).unwrap(),release=serde_json::to_string(&release).unwrap(),blocked=mode == "during")
    };
    fs::write(project.join("acceptance.cjs"), script).unwrap();
    let config = config(&temp.path().join("canonical"), &workspace, backend);
    let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
    let binding = task(&host, &config, config.root_task.clone(), None);
    let server = start_mock_server().await;
    let mut registry = ExtensionRegistryBuilder::new();
    registry.turn_start_admission(Arc::new(host.clone()));
    registry.work_admission(Arc::new(host.clone()));
    let starter = host.clone();
    let cwd = workspace.clone();
    let test = test_codex()
        .with_extensions(Arc::new(registry.build()))
        .with_auth(codex_login::CodexAuth::from_api_key(
            "synthetic-verification",
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
    host.register(thread, binding.clone()).unwrap();
    host.command(
        Command::SetWorkspaceTrust {
            trust: Trust::Trusted,
        },
        None,
        Revision::ZERO,
    )
    .unwrap();
    host.command(
        Command::SetPolicy {
            policy: Policy {
                workspace: config.workspace.clone(),
                revision: PolicyRevision::ZERO,
                mode: Autonomy::Autonomous,
                denials: vec![],
                workspace_roots: BTreeSet::from([
                    RootId::parse(config.workspace.as_str()).unwrap(),
                    RootId::parse("exec-node").unwrap(),
                ]),
                automatic_effects: BTreeSet::from([
                    EffectClass::Read,
                    EffectClass::Write,
                    EffectClass::Execute,
                    EffectClass::Network,
                    EffectClass::Install,
                    EffectClass::Publish,
                    EffectClass::Opaque,
                ]),
                timeout_ceiling_ms: Units::new(120_000),
                output_ceiling_bytes: ByteCount::new(8 * 1024 * 1024),
            },
        },
        None,
        Revision::ZERO,
    )
    .unwrap();
    if mode != "missing" {
        host.configure_process_profile(
            Profile::new(
                "node".into(),
                exe,
                Mode::Direct,
                BTreeMap::from([("SystemRoot".into(), std::env::var("SystemRoot").unwrap())]),
                BTreeSet::new(),
                true,
            )
            .unwrap(),
        )
        .unwrap();
    }
    let analysis = mode.starts_with("analysis");
    host.configure_verification(
        thread,
        VerificationConfig {
            requirements: if analysis {
                vec![]
            } else {
                vec![Requirement {
                    timeout_ms: None,
                    manifest: if mode == "uncovered" {
                        "project/package.json"
                    } else {
                        "package.json"
                    }
                    .into(),
                    runner: Runner::Node,
                    profile: "node".into(),
                    expected_tests: vec!["seeded_acceptance".into()],
                    rationale: "Read the changed value and assert the accepted answer".into(),
                }]
            },
            rationale: "Synthetic current-source acceptance".into(),
        },
    )
    .unwrap();
    if !matches!(
        mode,
        "analysis" | "analysis_late_effect" | "analysis_reopen"
    ) {
        fs::write(
            project.join("value.txt"),
            if mode == "seeded" { b"43\n" } else { b"42\n" },
        )
        .unwrap();
    }
    if mode == "uncovered" {
        fs::write(
            workspace.join("uncovered.txt"),
            b"outside the checked project",
        )
        .unwrap();
    }
    let citation = host
        .capture(
            thread,
            Channel::Evidence,
            b"Synthetic cited analysis of the observed input".to_vec(),
        )
        .unwrap()
        .spec
        .id;
    if mode == "analysis" {
        let uncited = host.verify(thread, vec![]).await.unwrap();
        assert!(host.complete_verified(thread, uncited.id).is_err());
    }
    let actor = host.clone();
    let pause_publish = mode == "pause_publish";
    let pause_scope = binding.scope.clone();
    let mut verification = tokio::spawn(async move {
        #[cfg(feature = "qualification")]
        if pause_publish {
            return actor
                .verify_with_publish_observer(thread, vec![], || {
                    let state = actor.snapshot().unwrap();
                    let task: Task = state
                        .record(
                            Collection::Task,
                            pause_scope.task.as_str(),
                            &pause_scope.workspace,
                        )
                        .unwrap()
                        .decode()
                        .unwrap();
                    actor
                        .stop(
                            actor
                                .control_envelope(
                                    CommandId::new(),
                                    pause_scope.task.clone(),
                                    task.revision,
                                    Command::Transition {
                                        next: TaskState::Paused,
                                        reason:
                                            "qualification pause before verification publication"
                                                .into(),
                                        verification: None,
                                    },
                                )
                                .unwrap(),
                        )
                        .unwrap();
                })
                .await;
        }
        #[cfg(not(feature = "qualification"))]
        let _ = (pause_publish, pause_scope);
        actor
            .verify(thread, if analysis { vec![citation] } else { vec![] })
            .await
    });
    if mode == "during" {
        tokio::select! {
            result=&mut verification=>panic!("{backend:?}/{mode}: check ended before the independent barrier: {result:?}"),
            arrived=tokio::time::timeout(Duration::from_secs(60),async {
                while !oracle.exists() { tokio::time::sleep(Duration::from_millis(10)).await; }
            })=>arrived.expect("bounded native preparation did not reach the check barrier"),
        }
        fs::write(
            workspace.join("AGENTS.md"),
            b"New acceptance guidance during the check.\n",
        )
        .unwrap();
        fs::write(&release, b"continue after observed edit").unwrap();
    }
    let verification = verification.await.unwrap().unwrap();
    assert_eq!(verification.scope, binding.scope);
    if mode == "analysis_late_effect" {
        assert!(verification.outstanding_issues.is_empty());
        assert!(verification.unresolved_effects.is_empty());
        let child = super::task(
            &host,
            &config,
            TaskId::new(),
            Some(config.root_task.clone()),
        );
        host.command(
            Command::ProposeEffect {
                id: ToolRunId::new(),
                operation_digest: "f".repeat(64),
            },
            Some(child.scope.task.clone()),
            Revision::new(1),
        )
        .unwrap();
        host.command(
            Command::Transition {
                next: TaskState::Failed,
                reason: "synthetic child stopped with an unresolved proposal".into(),
                verification: None,
            },
            Some(child.scope.task),
            Revision::new(1),
        )
        .unwrap();
        // The old root revision and terminal child state alone would
        // permit completion; the fresh workspace effect fence must not.
    }
    if matches!(mode, "later" | "ignored_later") {
        fs::write(
            workspace.join("AGENTS.md"),
            b"New guidance after a passing check.\n",
        )
        .unwrap();
    }
    let completed = if mode == "analysis_reopen" {
        let task = host.project().unwrap().tasks[&config.root_task].clone();
        host.command(
            Command::Transition {
                next: TaskState::Completed,
                reason: "serialized proof must not bypass native completion".into(),
                verification: Some(verification.id.clone()),
            },
            Some(config.root_task.clone()),
            task.revision,
        )
    } else {
        host.complete_verified(thread, verification.id.clone())
    };
    if matches!(mode, "pass" | "analysis") {
        completed.unwrap_or_else(|e| panic!("{backend:?}/{mode}: {e}; {verification:?}"));
    } else {
        assert!(
            completed.is_err(),
            "{backend:?}/{mode} must not complete: {verification:?}"
        );
    }
    match mode {
        "pass" | "later" | "ignored_later" | "uncovered" => assert_eq!(
            verification.checks[0].outcome,
            CheckOutcome::Passed,
            "{mode}: {verification:?}"
        ),
        "during" | "pause_publish" => {
            assert_eq!(verification.checks[0].outcome, CheckOutcome::Passed);
            assert!(verification
                .outstanding_issues
                .iter()
                .any(|s| s.contains("stale")));
        }
        "missing" => {
            assert!(matches!(
                verification.checks[0].outcome,
                CheckOutcome::NotRun { .. }
            ));
            assert!(!oracle.exists());
        }
        "spawn_failure" => {
            assert!(matches!(
                verification.checks[0].outcome,
                CheckOutcome::Failed { .. }
            ));
            assert!(!verification.unresolved_effects.is_empty());
            assert!(!oracle.exists());
            let intent = host
                .snapshot()
                .unwrap()
                .records
                .values()
                .filter(|r| r.collection == Collection::Artifact)
                .map(|r| r.decode::<ArtifactDescriptor>().unwrap())
                .find(|a| a.spec.schema == "verification-check-intent/1")
                .unwrap();
            let bytes = host.read_artifact(intent.spec.id).unwrap();
            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                value["effect"],
                serde_json::to_value(&verification.unresolved_effects[0]).unwrap()
            );
            let plan: ArtifactId = serde_json::from_value(value["plan"].clone()).unwrap();
            assert!(String::from_utf8(host.read_artifact(plan).unwrap())
                .unwrap()
                .contains("seeded_acceptance"));
        }
        "seeded" | "zero" | "wrong" => assert!(
            matches!(verification.checks[0].outcome, CheckOutcome::Failed { .. }),
            "{mode}: {verification:?}"
        ),
        _ => (),
    }
    if matches!(
        mode,
        "pass" | "during" | "later" | "ignored_later" | "wrong" | "uncovered"
    ) {
        assert_eq!(fs::read_to_string(&oracle).unwrap(), "started:passed");
    }
    if matches!(mode, "later" | "ignored_later") {
        let fresh = host.verify(thread, vec![]).await.unwrap();
        host.complete_verified(thread, fresh.id).unwrap();
    }
    let state = host.snapshot().unwrap();
    assert!(state
        .records
        .values()
        .any(|r| r.collection == Collection::Verification && r.id == verification.id.as_str()));
    if mode == "seeded" {
        fs::write(project.join("value.txt"), b"42\n").unwrap();
        let fresh = host.verify(thread, vec![]).await.unwrap();
        host.complete_verified(thread, fresh.id).unwrap();
        assert!(host
            .snapshot()
            .unwrap()
            .records
            .values()
            .any(|r| r.collection == Collection::Verification
                && r.decode::<Verification>()
                    .unwrap()
                    .checks
                    .iter()
                    .any(|c| matches!(c.outcome, CheckOutcome::Failed { .. }))));
    }
    owner.close().await.unwrap();
    test.codex.shutdown_and_wait().await.unwrap();
    drop(test);
    drop(host);
    if mode == "analysis_reopen" {
        fs::write(project.join("value.txt"), b"changed while closed\n").unwrap();
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let mut registry = ExtensionRegistryBuilder::new();
        registry.turn_start_admission(Arc::new(host.clone()));
        registry.work_admission(Arc::new(host.clone()));
        let starter = host.clone();
        let cwd = workspace.clone();
        let test = test_codex()
            .with_extensions(Arc::new(registry.build()))
            .with_auth(codex_login::CodexAuth::from_api_key(
                "synthetic-verification-reopen",
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
        host.register(thread, binding.clone()).unwrap();
        let task = host.project().unwrap().tasks[&config.root_task].clone();
        host.resume(thread, task.revision, task.fingerprint)
            .unwrap();
        host.configure_verification(
            thread,
            VerificationConfig {
                requirements: vec![],
                rationale: "Fresh owner must retain the original baseline".into(),
            },
        )
        .unwrap();
        assert!(host.complete_verified(thread, verification.id).is_err());
        let citation = host
            .capture(thread, Channel::Evidence, b"Fresh cited analysis".to_vec())
            .unwrap()
            .spec
            .id;
        let fresh = host.verify(thread, vec![citation]).await.unwrap();
        assert!(fresh
            .outstanding_issues
            .iter()
            .any(|s| s.contains("changed work requires")));
        assert!(host.complete_verified(thread, fresh.id).is_err());
        assert_eq!(
            host.snapshot()
                .unwrap()
                .records
                .values()
                .filter(|r| r.collection == Collection::Artifact)
                .filter(|r| r.decode::<ArtifactDescriptor>().unwrap().spec.schema
                    == "verification-baseline/1")
                .count(),
            1
        );
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
    }
}
