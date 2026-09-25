// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};
use vcp_domain::{policy::*, verification::CheckOutcome};
use vcp_lifecycle::foundation::{coding::CodingConfig, verification::VerificationConfig};
use vcp_tools::{
    process::{Mode, Profile, Request},
    verification::{Requirement, Runner},
};

// Deliberately opt-in: this is the real >120-second native acceptance, not a
// fast timing substitute. Both stores run independently and reopen their proof.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "native >120-second qualification; requires VCP_TEST_NODE"]
async fn declared_long_parent_checks_complete_and_reopen_on_both_stores() {
    tokio::join!(
        run(BackendKind::Sqlite, false),
        run(BackendKind::Files, false),
        run(BackendKind::Sqlite, true),
        run(BackendKind::Files, true)
    );
}
async fn run(backend: BackendKind, exec: bool) {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    fs::write(
        workspace.join("AGENTS.md"),
        "Run the explicitly configured long acceptance check.\n",
    )
    .unwrap();
    fs::write(
        workspace.join("package.json"),
        r#"{"scripts":{"test":"node --test long.test.cjs"}}"#,
    )
    .unwrap();
    fs::write(workspace.join("long.test.cjs"), "const test=require('node:test'),assert=require('node:assert/strict');test('long_check',async()=>{const start=Date.now();await new Promise(resolve=>setTimeout(resolve,121000));assert.ok(Date.now()-start>=120000);});\n").unwrap();
    let executable = temp.path().join("node.exe");
    fs::copy(
        std::env::var_os("VCP_TEST_NODE").expect("explicit native Node"),
        &executable,
    )
    .unwrap();
    let environment = BTreeMap::from([
        ("SystemRoot".into(), std::env::var("SystemRoot").unwrap()),
        ("TEMP".into(), temp.path().display().to_string()),
        ("TMP".into(), temp.path().display().to_string()),
    ]);
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
            "synthetic-framework-verification",
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
                    RootId::parse("exec-check").unwrap(),
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
                timeout_ceiling_ms: Units::new(180_000),
                output_ceiling_bytes: ByteCount::new(1024 * 1024),
            },
        },
        None,
        Revision::ZERO,
    )
    .unwrap();

    let profile = Profile::new(
        "check".into(),
        executable,
        Mode::Direct,
        environment,
        BTreeSet::new(),
        true,
    )
    .unwrap();
    host.configure_process_profile(profile.clone()).unwrap();
    let check_request = Request {
        profile: "check".into(),
        arguments: vec![
            "--test".into(),
            "--test-reporter=tap".into(),
            "long.test.cjs".into(),
        ],
        directory: String::new(),
        timeout_ms: 150_000,
        output_bytes: 1024 * 1024,
        input: None,
    };
    // The legacy profile ceiling rejects before dispatch; no two-minute wait
    // is required to prove the trusted boundary itself.
    assert!(host.prepare_process(thread, check_request.clone()).is_err());
    host.configure_process_profile(profile.with_max_timeout_ms(180_000).unwrap())
        .unwrap();
    let (snapshot, raw) = provider_snapshot();
    host.configure_provider(snapshot, raw).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    host.configure_coding(
        thread,
        CodingConfig {
            canonical_tools: Default::default(),
            operating: "Run the configured long verification".into(),
            affected_paths: vec!["long.test.cjs".into()],
            max_requests: 1,
            deadline: Timestamp::new(now + 600_000),
        },
    )
    .unwrap();
    if exec {
        let started = std::time::Instant::now();
        let proposal = host.prepare_process(thread, check_request).unwrap();
        let process = host.dispatch_process(proposal).unwrap();
        let outcome = process.wait().await.unwrap();
        assert!(started.elapsed() >= Duration::from_secs(120));
        assert_eq!(outcome.exit_code, Some(0));
        assert!(outcome.reason.is_none());
        let evidence = host
            .read_artifact(outcome.evidence.spec.id.clone())
            .unwrap();
        let stdout = host.read_artifact(outcome.stdout.spec.id.clone()).unwrap();
        assert!(String::from_utf8_lossy(&stdout).contains("ok 1 - long_check"));
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        drop(test);
        drop(host);
        let (reopened, owner) = CanonicalHost::open(config).unwrap();
        assert_eq!(
            reopened.read_artifact(outcome.evidence.spec.id).unwrap(),
            evidence
        );
        assert_eq!(
            reopened.read_artifact(outcome.stdout.spec.id).unwrap(),
            stdout
        );
        owner.close().await.unwrap();
        return;
    }
    host.configure_verification(
        thread,
        VerificationConfig {
            requirements: vec![Requirement {
                manifest: "package.json".into(),
                runner: Runner::Node,
                profile: "check".into(),
                expected_tests: vec!["long_check".into()],
                rationale: "Observe a real check longer than the legacy 120-second ceiling".into(),
                timeout_ms: Some(150_000),
            }],
            rationale: "Native long foreground parent verification".into(),
        },
    )
    .unwrap();
    let started = std::time::Instant::now();
    let passed = host.verify(thread, vec![]).await.unwrap();
    assert!(started.elapsed() >= Duration::from_secs(120));
    assert_eq!(
        passed.checks[0].outcome,
        CheckOutcome::Passed,
        "{backend:?}: {passed:?}"
    );
    let record = host
        .snapshot()
        .unwrap()
        .record(
            Collection::Verification,
            passed.id.as_str(),
            &config.workspace,
        )
        .unwrap()
        .clone();
    let output = host.read_artifact(passed.checks[0].output.clone()).unwrap();
    assert!(!output.is_empty());
    assert!(server.received_requests().await.unwrap().is_empty());
    owner.close().await.unwrap();
    test.codex.shutdown_and_wait().await.unwrap();
    drop(test);
    drop(host);
    let (reopened, owner) = CanonicalHost::open(config.clone()).unwrap();
    assert_eq!(
        reopened
            .snapshot()
            .unwrap()
            .record(
                Collection::Verification,
                passed.id.as_str(),
                &config.workspace
            )
            .unwrap(),
        &record
    );
    assert_eq!(
        reopened
            .read_artifact(passed.checks[0].output.clone())
            .unwrap(),
        output
    );
    owner.close().await.unwrap();
}
