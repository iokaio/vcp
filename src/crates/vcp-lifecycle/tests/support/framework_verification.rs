// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};
use vcp_domain::{policy::*, verification::CheckOutcome};
use vcp_lifecycle::foundation::verification::VerificationConfig;
use vcp_tools::{
    process::{Mode, Profile},
    verification::{Requirement, Runner},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn actual_cargo_and_documentation_checks_preserve_failed_evidence_before_completion() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for runner in [Runner::Cargo, Runner::Node] {
            run(backend, runner).await;
        }
    }
}

#[cfg(feature = "dotnet-qualification")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn actual_dotnet_checks_preserve_failed_evidence_before_completion() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        run(backend, Runner::Dotnet).await;
    }
}

async fn run(backend: BackendKind, runner: Runner) {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    let oracle = temp.path().join("assertion-observed");
    fs::write(
        workspace.join("AGENTS.md"),
        "Run the named acceptance check before completion.\n",
    )
    .unwrap();
    let mut environment = BTreeMap::from([
        ("SystemRoot".into(), std::env::var("SystemRoot").unwrap()),
        ("TEMP".into(), temp.path().display().to_string()),
        ("TMP".into(), temp.path().display().to_string()),
    ]);
    let (manifest, expected, changed, initial, defective, fixed, executable) = match runner {
        Runner::Cargo => {
            fs::create_dir(workspace.join("src")).unwrap();
            fs::write(workspace.join("Cargo.toml"), "[package]\nname = \"vcp-acceptance-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n").unwrap();
            fs::write(workspace.join("Cargo.lock"), "version = 4\n\n[[package]]\nname = \"vcp-acceptance-fixture\"\nversion = \"0.1.0\"\n").unwrap();
            fs::write(workspace.join("src/lib.rs"), "// SPDX-License-Identifier: Apache-2.0\nmod value;\n#[test]\nfn observed_answer() { std::fs::write(\"../assertion-observed\", b\"ran\").unwrap(); assert_eq!(value::answer(), 42); }\n").unwrap();
            // Explicit real toolchain binaries, not a rustup shim or ambient
            // credential environment. The qualification script supplies these
            // non-secret native compiler settings and records their identities.
            for name in ["PATH", "LIB", "INCLUDE", "LIBPATH"] {
                if let Some(value) = std::env::var_os(format!("VCP_TEST_COMPILER_{name}")) {
                    environment.insert(name.into(), value.into_string().unwrap());
                }
            }
            environment.insert(
                "CARGO_TARGET_DIR".into(),
                temp.path().join("cargo-target").display().to_string(),
            );
            environment.insert(
                "CARGO_HOME".into(),
                temp.path().join("cargo-home").display().to_string(),
            );
            let executable = std::env::var_os("VCP_TEST_CARGO")
                .expect("explicit real native Cargo")
                .into();
            (
                "Cargo.toml",
                "observed_answer",
                "src/value.rs",
                "// SPDX-License-Identifier: Apache-2.0\npub fn answer() -> u32 { 40 }\n",
                "// SPDX-License-Identifier: Apache-2.0\npub fn answer() -> u32 { 41 }\n",
                "// SPDX-License-Identifier: Apache-2.0\npub fn answer() -> u32 { 42 }\n",
                executable,
            )
        }
        Runner::Node => {
            fs::write(
                workspace.join("package.json"),
                r#"{"scripts":{"test":"node --test docs.test.cjs"}}"#,
            )
            .unwrap();
            fs::write(workspace.join("guide.md"), "# Existing guide\n").unwrap();
            fs::write(workspace.join("docs.test.cjs"), "const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs');test('readme_relative_link',()=>{fs.writeFileSync('../assertion-observed','ran');const text=fs.readFileSync('README.md','utf8');const links=[...text.matchAll(/\\[[^\\]]+\\]\\(([^)]+)\\)/g)];assert.equal(links.length,1);for(const link of links)assert.ok(fs.statSync(link[1]).isFile());});\n").unwrap();
            let executable = temp.path().join("node.exe");
            fs::copy(
                std::env::var_os("VCP_TEST_NODE").expect("explicit native Node"),
                &executable,
            )
            .unwrap();
            (
                "package.json",
                "readme_relative_link",
                "README.md",
                "# Initial documentation\n",
                "[Guide](missing.md)\n",
                "[Guide](guide.md)\n",
                executable,
            )
        }
        Runner::Dotnet => {
            // Generated assemblies and intermediate assets are explicitly
            // excluded; source and project files remain in the completion fence.
            fs::write(
                workspace.join(".gitignore"),
                "/app/bin/\n/app/obj/\n/tests/bin/\n/tests/obj/\n",
            )
            .unwrap();
            let executable: std::path::PathBuf = std::env::var_os("VCP_TEST_DOTNET")
                .expect("explicit real native dotnet executable")
                .into();
            let packages = std::env::var_os("VCP_TEST_NUGET_PACKAGES")
                .expect("explicit offline NuGet package cache");
            let dotnet_home = temp.path().join("dotnet-home");
            fs::create_dir(&dotnet_home).unwrap();
            environment.insert(
                "ProgramFiles(x86)".into(),
                std::env::var("ProgramFiles(x86)").unwrap(),
            );
            for name in ["APPDATA", "LOCALAPPDATA"] {
                let directory = dotnet_home.join(name);
                fs::create_dir(&directory).unwrap();
                environment.insert(name.into(), directory.display().to_string());
            }
            for directory in ["app", "tests"] {
                fs::create_dir(workspace.join(directory)).unwrap();
            }
            fs::write(workspace.join("app/App.csproj"), "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>\n").unwrap();
            fs::write(workspace.join("tests/Tests.csproj"), "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>net10.0</TargetFramework><IsTestProject>true</IsTestProject></PropertyGroup><ItemGroup><PackageReference Include=\"Microsoft.NET.Test.Sdk\" Version=\"17.11.1\"/><PackageReference Include=\"xunit\" Version=\"2.9.2\"/><PackageReference Include=\"xunit.runner.visualstudio\" Version=\"2.8.2\"/><ProjectReference Include=\"../app/App.csproj\"/></ItemGroup></Project>\n").unwrap();
            fs::write(workspace.join("tests/Checks.cs"), "using Xunit;\nnamespace AcceptanceFixture;\npublic class Checks { [Fact] public void ObservedAnswer() { System.IO.File.WriteAllText(System.IO.Path.Combine(System.AppContext.BaseDirectory, \"../../../../../assertion-observed\"), \"ran\"); Assert.Equal(42, App.Value.Answer()); } }\n").unwrap();
            fs::write(workspace.join("fixture.slnx"), "<Solution><Project Path=\"app/App.csproj\"/><Project Path=\"tests/Tests.csproj\"/></Solution>\n").unwrap();
            // No ambient NuGet configuration, credential provider or network source.
            fs::write(
                workspace.join("NuGet.config"),
                "<configuration><packageSources><clear /></packageSources></configuration>\n",
            )
            .unwrap();
            fs::write(
                workspace.join("app/Value.cs"),
                "namespace App; public static class Value { public static int Answer() => 40; }\n",
            )
            .unwrap();
            let restored = std::process::Command::new(&executable)
                .current_dir(&workspace)
                .env_clear()
                .envs(&environment)
                .args([
                    "restore",
                    "fixture.slnx",
                    "--configfile",
                    "NuGet.config",
                    "--nologo",
                    "-p:NuGetAudit=false",
                    "--disable-build-servers",
                ])
                .arg("--packages")
                .arg(packages)
                .output()
                .unwrap();
            assert!(
                restored.status.success(),
                "offline restore failed: {} {}",
                String::from_utf8_lossy(&restored.stdout),
                String::from_utf8_lossy(&restored.stderr)
            );
            (
                "fixture.slnx",
                "AcceptanceFixture.Checks.ObservedAnswer",
                "app/Value.cs",
                "namespace App; public static class Value { public static int Answer() => 40; }\n",
                "namespace App; public static class Value { public static int Answer() => 41; }\n",
                "namespace App; public static class Value { public static int Answer() => 42; }\n",
                executable,
            )
        }
    };
    fs::write(workspace.join(changed), initial).unwrap();
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
                timeout_ceiling_ms: Units::new(120_000),
                output_ceiling_bytes: ByteCount::new(1024 * 1024),
            },
        },
        None,
        Revision::ZERO,
    )
    .unwrap();
    host.configure_process_profile(
        Profile::new(
            "check".into(),
            executable,
            Mode::Direct,
            environment,
            BTreeSet::new(),
            true,
        )
        .unwrap(),
    )
    .unwrap();
    host.configure_verification(thread, VerificationConfig {
        requirements: vec![Requirement { timeout_ms: None,manifest: manifest.into(),runner,profile:"check".into(),expected_tests:vec![expected.into()],rationale:"Exercise the actual changed source or documentation target".into()}],
        rationale:"Native framework qualification with a seeded defect and independently observed test execution".into(),
    }).unwrap();

    fs::write(workspace.join(changed), defective).unwrap();
    let failed = host.verify(thread, vec![]).await.unwrap();
    assert!(
        matches!(failed.checks[0].outcome, CheckOutcome::Failed { .. }),
        "{backend:?}/{runner:?}: {failed:?}"
    );
    assert_eq!(fs::read(&oracle).unwrap(), b"ran");
    assert!(host.complete_verified(thread, failed.id.clone()).is_err());
    let failed_state = host.snapshot().unwrap();
    let failed_record = failed_state
        .record(
            Collection::Verification,
            failed.id.as_str(),
            &config.workspace,
        )
        .unwrap()
        .clone();
    let failed_output = host.read_artifact(failed.checks[0].output.clone()).unwrap();

    fs::write(workspace.join(changed), fixed).unwrap();
    fs::write(&oracle, b"awaiting fresh check").unwrap();
    let passed = host.verify(thread, vec![]).await.unwrap();
    assert_eq!(
        passed.checks[0].outcome,
        CheckOutcome::Passed,
        "{backend:?}/{runner:?}: {passed:?}"
    );
    assert_eq!(fs::read(&oracle).unwrap(), b"ran");
    assert_ne!(passed.id, failed.id);
    assert!(
        passed.outstanding_issues.is_empty(),
        "{backend:?}/{runner:?}: {:?}",
        passed.outstanding_issues
    );
    host.complete_verified(thread, passed.id).unwrap();
    let state = host.snapshot().unwrap();
    assert_eq!(
        state
            .record(
                Collection::Verification,
                failed.id.as_str(),
                &config.workspace
            )
            .unwrap(),
        &failed_record
    );
    assert_eq!(
        host.read_artifact(failed.checks[0].output.clone()).unwrap(),
        failed_output
    );
    assert_eq!(
        host.project().unwrap().tasks[&config.root_task].state,
        TaskState::Completed
    );
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "verification adds no model request"
    );
    owner.close().await.unwrap();
    test.codex.shutdown_and_wait().await.unwrap();
}
