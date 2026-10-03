// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};
use vcp_domain::policy::*;
use vcp_tools::process::{Mode, Profile, Request};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires .NET 10 SDK and explicit VCP_TEST_DOTNET"]
async fn real_dotnet_build_and_run_use_isolated_bootstrap_through_process_broker() {
    let executable: std::path::PathBuf = std::env::var_os("VCP_TEST_DOTNET")
        .expect("explicit real .NET 10 SDK executable")
        .into();
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("dotnet workspace");
    fs::create_dir(&workspace).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    fs::write(workspace.join("Probe.csproj"), "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>").unwrap();
    // No package sources or external packages: this exercises NuGet's actual
    // settings resolution without network access or the user's NuGet config.
    fs::write(
        workspace.join("NuGet.Config"),
        "<configuration><packageSources><clear /></packageSources></configuration>",
    )
    .unwrap();
    fs::write(workspace.join("Program.cs"), "if (System.Environment.GetEnvironmentVariable(\"VCP_TEST_DOTNET\") != null) return 1; System.Console.WriteLine(\"VCP_DOTNET_PROBE_OK\"); return 0;").unwrap();
    let mut environment = BTreeMap::from([
        ("SYSTEMROOT".into(), std::env::var("SystemRoot").unwrap()),
        (
            "PROGRAMFILES".into(),
            std::env::var("ProgramFiles").unwrap(),
        ),
        ("CI".into(), "true".into()),
    ]);
    for name in ["TEMP", "TMP", "APPDATA", "LOCALAPPDATA", "DOTNET_CLI_HOME"] {
        let directory = temp.path().join(name);
        fs::create_dir(&directory).unwrap();
        environment.insert(name.into(), directory.display().to_string());
    }
    let config = config(
        &temp.path().join("canonical"),
        &workspace,
        BackendKind::Files,
    );
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
            "synthetic-dotnet-fixture",
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
    let id = host.lifecycle().attach_root(test.codex.clone()).unwrap();
    host.register(id, binding).unwrap();
    host.command(
        Command::SetWorkspaceTrust {
            trust: Trust::Trusted,
        },
        None,
        Revision::ZERO,
    )
    .unwrap();
    let profile = Profile::new(
        "dotnet".into(),
        executable,
        Mode::Direct,
        environment,
        BTreeSet::new(),
        true,
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
                    profile.executable_root_id().unwrap(),
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
    host.configure_process_profile(profile).unwrap();
    for arguments in [
        vec!["build", "Probe.csproj", "--disable-build-servers"],
        vec![
            "run",
            "--project",
            "Probe.csproj",
            "--no-build",
            "--no-restore",
        ],
    ] {
        let is_run = arguments[0] == "run";
        let ticket = host
            .prepare_process(
                id,
                Request {
                    profile: "dotnet".into(),
                    arguments: arguments.into_iter().map(str::to_owned).collect(),
                    directory: String::new(),
                    timeout_ms: 120_000,
                    output_bytes: 1024 * 1024,
                    input: None,
                },
            )
            .unwrap();
        let result = host.dispatch_process(ticket).unwrap().wait().await.unwrap();
        let stdout = host.read_artifact(result.stdout.spec.id.clone()).unwrap();
        let stderr = host.read_artifact(result.stderr.spec.id.clone()).unwrap();
        assert_eq!(
            result.exit_code,
            Some(0),
            "stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
        assert_eq!(result.stdout.state, CaptureState::Complete);
        if is_run {
            assert!(String::from_utf8_lossy(&stdout).contains("VCP_DOTNET_PROBE_OK"));
        }
    }
    assert!(temp.path().join("DOTNET_CLI_HOME/.dotnet").is_dir());
    owner.close().await.unwrap();
    test.codex.shutdown_and_wait().await.unwrap();
}
