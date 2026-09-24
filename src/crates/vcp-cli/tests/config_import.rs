// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
#![cfg(windows)]
//! Real CLI import transactions are local data operations, never provider work.
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    process::{Command, Output},
    time::Duration,
};
use vcp_domain::Timestamp;
use vcp_models::catalog::{Compatibility, Snapshot};

struct Fixture {
    _temporary: tempfile::TempDir,
    workspace: PathBuf,
    data: PathBuf,
    profile: PathBuf,
    source_root: PathBuf,
}
impl Fixture {
    fn new(provider_endpoint: &str, mcp_endpoint: &str) -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().join("workspace");
        let data = temporary.path().join("private-data");
        let source_root = temporary.path().join("selected-import-root");
        for path in [&workspace, &data, &source_root] {
            fs::create_dir(path).unwrap();
        }
        let catalog = data.join("catalog.json");
        let raw=serde_json::to_vec(&json!({"data":{"id":"fixture/model","endpoints":[{"tag":"fixture/region","status":0,"context_length":32000,"max_prompt_tokens":24000,"max_completion_tokens":8000,"supported_parameters":["tools","max_tokens"],"pricing":{"prompt":"0","completion":"0","request":"0.0001"}}]}})).unwrap();
        let snapshot = Snapshot::from_endpoints(
            &raw,
            Timestamp::ZERO,
            Timestamp::new(u64::MAX),
            Compatibility {
                id: "synthetic-import/1".into(),
                model: "fixture/model".into(),
                endpoint: "fixture/region".into(),
                qualified_at: Timestamp::ZERO,
                valid_until: Timestamp::new(u64::MAX),
                responses_text_tools: true,
                byte_ceiling_qualified: true,
                provider_preferences_qualified: true,
                deny_data_collection: true,
                require_zdr: true,
                request_price_limit: "0.0001".into(),
                required_parameters: BTreeSet::from(["tools".into(), "max_tokens".into()]),
                qualified_reasoning_efforts: BTreeSet::new(),
            },
        )
        .unwrap();
        fs::write(&catalog, raw).unwrap();
        let profile = data.join("profile.json");
        let mut value = json!({"version":1,"workspace":workspace,"trust_workspace":true,"maximum_autonomy":"workspace","automatic_effects":["read"],"budget_usd":"1","provider":snapshot,"catalog":catalog,"affected_paths":[],"max_requests":4,"deadline_seconds":60,"processes":[],"checks":[],"mcp_http":[{"name":"remote","endpoint":mcp_endpoint,"credential":null,"allowed_tools":["read","write"],"limits":{"frame_bytes":4096,"total_discovery_bytes":8192,"tools":8,"pages":2,"timeout_ms":10000,"stderr_bytes":0}}]});
        #[cfg(feature = "qualification")]
        {
            value["qualification_endpoint"] = json!(provider_endpoint);
        }
        #[cfg(not(feature = "qualification"))]
        {
            let _ = provider_endpoint;
        }
        fs::write(&profile, serde_json::to_vec(&value).unwrap()).unwrap();
        Self {
            _temporary: temporary,
            workspace,
            data,
            profile,
            source_root,
        }
    }
    fn command(&self, args: &[String]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_vcp"));
        command
            .args(["--format", "jsonl", "--non-interactive", "--workspace"])
            .arg(&self.workspace)
            .arg("--data-dir")
            .arg(&self.data)
            .arg("--config")
            .arg(&self.profile)
            .args(["config", "import"])
            .args(args)
            .env_remove("OPENROUTER_API_KEY")
            .env_remove("RUST_MIN_STACK");
        command
    }
    async fn run(&self, args: &[String]) -> Output {
        let mut command = self.command(args);
        tokio::time::timeout(
            Duration::from_secs(20),
            tokio::task::spawn_blocking(move || command.output().unwrap()),
        )
        .await
        .expect("local import CLI deadline")
        .unwrap()
    }
    async fn success(&self, args: &[String]) -> Value {
        let output = self.run(args).await;
        assert!(
            output.status.success(),
            "import command failed: {} / {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let row: Value = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .last()
            .expect("structured CLI result");
        row["data"].clone()
    }
    fn source_args(&self, action: &str, file: &str, format: &str) -> Vec<String> {
        vec![
            action.into(),
            "--source".into(),
            self.source_root.join(file).to_string_lossy().into_owned(),
            "--source-root".into(),
            self.source_root.to_string_lossy().into_owned(),
            "--source-format".into(),
            format.into(),
        ]
    }
}
impl Fixture {
    fn codex(&self, timeout_seconds: u64) {
        fs::write(self.source_root.join("config.toml"),format!("[mcp_servers.remote]\nurl = \"https://127.0.0.1:1/mcp\"\nenabled_tools = [\"read\"]\ntool_timeout_sec = {timeout_seconds}\n")).unwrap();
    }
    async fn apply_codex(&self, preview: &Value, fields: &[&str]) -> Value {
        let mut args = self.source_args("apply", "config.toml", "codex-8b78600d-v1");
        args.extend([
            "--preview".into(),
            preview["preview_id"].as_str().unwrap().into(),
        ]);
        for field in fields {
            args.extend(["--select".into(), (*field).into()]);
        }
        self.success(&args).await
    }
    fn state_dir(&self) -> PathBuf {
        self.profile.with_file_name("profile.json.vcp-imports")
    }
    fn loaded_remote(&self) -> (BTreeSet<String>, u64) {
        let profile =
            vcp_cli::settings::load(&self.profile, &self.workspace.canonicalize().unwrap())
                .unwrap();
        let remote = profile
            .mcp_http
            .iter()
            .find(|s| s.name == "remote")
            .unwrap();
        (remote.allowed_tools.clone(), remote.limits.timeout_ms)
    }
}
#[tokio::test]
async fn executable_config_import_selects_fields_preserves_originals_and_rolls_back() {
    let provider = wiremock::MockServer::start().await;
    let f = Fixture::new(&provider.uri(), "https://127.0.0.1:1/mcp");
    f.codex(5);
    let original = fs::read(&f.profile).unwrap();
    let source = fs::read(f.source_root.join("config.toml")).unwrap();
    let status = f.success(&["status".into()]).await;
    assert!(status["current"].is_null());
    let preview = f
        .success(&f.source_args("preview", "config.toml", "codex-8b78600d-v1"))
        .await;
    let ids: BTreeSet<_> = preview["mapping"]["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert!(
        ids.contains("server.remote.allowed_tools") && ids.contains("server.remote.timeout_ms"),
        "{preview}"
    );
    assert!(
        f.success(&["status".into()]).await["current"].is_null(),
        "preview must not publish"
    );
    let applied = f
        .apply_codex(&preview, &["server.remote.allowed_tools"])
        .await;
    assert_eq!(applied["current"]["revision"], 1);
    assert_eq!(applied["original_profile_preserved"], true);
    assert_eq!(f.loaded_remote(), (BTreeSet::from(["read".into()]), 10000));
    let refreshed = f
        .success(&f.source_args("preview", "config.toml", "codex-8b78600d-v1"))
        .await;
    let applied = f
        .apply_codex(&refreshed, &["server.remote.timeout_ms"])
        .await;
    assert_eq!(applied["current"]["revision"], 2);
    assert_eq!(f.loaded_remote(), (BTreeSet::from(["read".into()]), 5000));
    let rollback = f
        .success(&["rollback-preview".into(), "--revision".into(), "0".into()])
        .await;
    let rolled = f
        .success(&[
            "rollback".into(),
            "--revision".into(),
            "0".into(),
            "--preview".into(),
            rollback["preview_id"].as_str().unwrap().into(),
        ])
        .await;
    assert_eq!(rolled["current"]["revision"], 3);
    assert_eq!(
        f.loaded_remote(),
        (BTreeSet::from(["read".into(), "write".into()]), 10000)
    );
    assert_eq!(fs::read(&f.profile).unwrap(), original);
    assert_eq!(fs::read(f.source_root.join("config.toml")).unwrap(), source);
    assert!(provider.received_requests().await.unwrap().is_empty());
}
#[tokio::test]
async fn executable_config_import_rejects_stale_source_base_and_rollback_previews() {
    let f = Fixture::new("http://127.0.0.1:1/v1", "https://127.0.0.1:1/mcp");
    f.codex(5);
    let preview = f
        .success(&f.source_args("preview", "config.toml", "codex-8b78600d-v1"))
        .await;
    let mut apply = f.source_args("apply", "config.toml", "codex-8b78600d-v1");
    apply.extend([
        "--preview".into(),
        preview["preview_id"].as_str().unwrap().into(),
        "--select".into(),
        "server.remote.timeout_ms".into(),
    ]);
    f.codex(4);
    assert!(!f.run(&apply).await.status.success());
    f.codex(5);
    let mut base: Value = serde_json::from_slice(&fs::read(&f.profile).unwrap()).unwrap();
    base["mcp_http"][0]["limits"]["timeout_ms"] = json!(6000);
    fs::write(&f.profile, serde_json::to_vec(&base).unwrap()).unwrap();
    assert!(!f.run(&apply).await.status.success());
    assert!(f.success(&["status".into()]).await["current"].is_null());
    let fresh = f
        .success(&f.source_args("preview", "config.toml", "codex-8b78600d-v1"))
        .await;
    f.apply_codex(&fresh, &["server.remote.timeout_ms"]).await;
    let rollback = f
        .success(&["rollback-preview".into(), "--revision".into(), "0".into()])
        .await;
    base["mcp_http"][0]["allowed_tools"] = json!(["read"]);
    base["mcp_http"][0]["limits"]["timeout_ms"] = json!(3000);
    fs::write(&f.profile, serde_json::to_vec(&base).unwrap()).unwrap();
    assert!(
        vcp_cli::settings::load(&f.profile, &f.workspace.canonicalize().unwrap()).is_err(),
        "changed native authority must require review"
    );
    let args = vec![
        "rollback".into(),
        "--revision".into(),
        "0".into(),
        "--preview".into(),
        rollback["preview_id"].as_str().unwrap().into(),
    ];
    assert!(!f.run(&args).await.status.success());
    let status = f.success(&["status".into()]).await;
    assert_eq!(status["current"]["revision"], 1);
    assert_eq!(status["base_changed"], true);
    let fresh = f
        .success(&["rollback-preview".into(), "--revision".into(), "0".into()])
        .await;
    let applied = f
        .success(&[
            "rollback".into(),
            "--revision".into(),
            "0".into(),
            "--preview".into(),
            fresh["preview_id"].as_str().unwrap().into(),
        ])
        .await;
    assert_eq!(applied["current"]["revision"], 2);
    assert_eq!(
        f.loaded_remote(),
        (BTreeSet::from(["read".into()]), 3000),
        "rollback must not resurrect revoked tools or timeout authority"
    );
}
#[tokio::test]
async fn executable_gemini_import_redacts_secrets_and_never_activates_foreign_commands() {
    let provider = wiremock::MockServer::start().await;
    let f = Fixture::new(&provider.uri(), "https://127.0.0.1:1/mcp");
    let marker = f.workspace.join("IMPORTED_COMMAND_MUST_NOT_RUN");
    let secret = "P1002_SYNTHETIC_SECRET_MUST_NOT_ENTER_HISTORY";
    let source = json!({"apiKey":secret,"hooks":{"BeforeTool":[{"command":format!("cmd.exe /c echo executed > {}",marker.display())}]},"mcpServers":{
        "remote":{"url":"https://127.0.0.1:1/mcp","type":"http","includeTools":["read"],"timeout":4000},
        "rogue":{"command":"cmd.exe","args":["/c",format!("echo executed > {}",marker.display())],"env":{"PRIVATE_TOKEN":secret}}
    }});
    let bytes = serde_json::to_vec(&source).unwrap();
    fs::write(f.source_root.join("settings.json"), &bytes).unwrap();
    let original = fs::read(&f.profile).unwrap();
    let preview = f
        .success(&f.source_args("preview", "settings.json", "gemini-6a466a7e-v1"))
        .await;
    assert!(!preview.to_string().contains(secret));
    assert!(
        preview["mapping"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d.to_string().contains("unsupported")),
        "{preview}"
    );
    let mut args = f.source_args("apply", "settings.json", "gemini-6a466a7e-v1");
    args.extend([
        "--preview".into(),
        preview["preview_id"].as_str().unwrap().into(),
        "--select".into(),
        "server.remote.allowed_tools".into(),
        "--select".into(),
        "server.remote.timeout_ms".into(),
    ]);
    let applied = f.success(&args).await;
    assert!(!applied.to_string().contains(secret));
    assert_eq!(f.loaded_remote(), (BTreeSet::from(["read".into()]), 4000));
    assert!(!marker.exists());
    assert_eq!(fs::read(&f.profile).unwrap(), original);
    assert_eq!(
        fs::read(f.source_root.join("settings.json")).unwrap(),
        bytes
    );
    fn inspect(path: &std::path::Path, secret: &str) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                inspect(&path, secret);
            } else {
                assert!(
                    !String::from_utf8_lossy(&fs::read(path).unwrap()).contains(secret),
                    "secret was persisted in import history"
                );
            }
        }
    }
    inspect(&f.state_dir(), secret);
    assert!(provider.received_requests().await.unwrap().is_empty());
    let mut escape = f.source_args("preview", "settings.json", "gemini-6a466a7e-v1");
    escape[2] = f
        .source_root
        .join("..")
        .join("selected-import-root")
        .join("settings.json")
        .to_string_lossy()
        .into_owned();
    assert!(
        !f.run(&escape).await.status.success(),
        "explicit traversal must fail before parsing"
    );
    let unknown = f.source_args("preview", "settings.json", "gemini-future-v999");
    assert!(!f.run(&unknown).await.status.success());
}
#[cfg(feature = "qualification")]
#[tokio::test]
async fn executable_import_owner_kill_selects_only_complete_old_or_new_revision() {
    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for phase in ["before_publish", "after_publish"] {
        let f = Fixture::new("http://127.0.0.1:1/v1", "https://127.0.0.1:1/mcp");
        f.codex(5);
        let original = fs::read(&f.profile).unwrap();
        let preview = f
            .success(&f.source_args("preview", "config.toml", "codex-8b78600d-v1"))
            .await;
        let mut args = f.source_args("apply", "config.toml", "codex-8b78600d-v1");
        args.extend([
            "--preview".into(),
            preview["preview_id"].as_str().unwrap().into(),
            "--select".into(),
            "server.remote.allowed_tools".into(),
            "--select".into(),
            "server.remote.timeout_ms".into(),
        ]);
        let mut process = ChildGuard(
            f.command(&args)
                .env("VCP_IMPORT_BARRIER", phase)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let barrier = f.state_dir().join(format!("qualification-{phase}.ready"));
        tokio::time::timeout(Duration::from_secs(20), async {
            while !barrier.exists() {
                assert!(
                    process.0.try_wait().unwrap().is_none(),
                    "import owner exited before {phase}"
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("import publication barrier deadline");
        process.0.kill().unwrap();
        assert!(!process.0.wait().unwrap().success());
        let mut first = None;
        for _ in 0..2 {
            let status = f.success(&["status".into()]).await;
            if phase == "before_publish" {
                assert!(status["current"].is_null());
                assert_eq!(
                    f.loaded_remote(),
                    (BTreeSet::from(["read".into(), "write".into()]), 10000)
                );
            } else {
                assert_eq!(status["current"]["revision"], 1);
                assert_eq!(f.loaded_remote(), (BTreeSet::from(["read".into()]), 5000));
            }
            if let Some(previous) = &first {
                assert_eq!(&status, previous, "reopen must not replay publication");
            }
            first = Some(status);
            assert_eq!(fs::read(&f.profile).unwrap(), original);
        }
        if phase == "after_publish" {
            assert!(
                !f.run(&args).await.status.success(),
                "old preview cannot create a second publication after lost acknowledgement"
            );
            assert_eq!(
                f.success(&["status".into()]).await["current"]["revision"],
                1
            );
        }
    }
}
#[tokio::test]
async fn executable_import_rejects_native_aliases_and_synced_profile_roots_before_publication() {
    for scenario in [
        "source-hardlink",
        "source-root-junction",
        "profile-hardlink",
        "synced-profile-root",
        "synced-history-root",
    ] {
        let f = Fixture::new("http://127.0.0.1:1/v1", "https://127.0.0.1:1/mcp");
        f.codex(5);
        let marker = f.workspace.join("alias-import-command-must-not-run");
        let source = f.source_root.join("config.toml");
        let command = serde_json::to_string(&vec![
            "cmd.exe".to_owned(),
            "/c".into(),
            format!("echo forbidden > {}", marker.display()),
        ])
        .unwrap();
        let source_bytes = format!(
            "notify = {command}\n{}",
            fs::read_to_string(&source).unwrap()
        );
        fs::write(&source, &source_bytes).unwrap();
        let mut args = f.source_args("preview", "config.toml", "codex-8b78600d-v1");
        match scenario {
            "source-hardlink" => {
                let alias = f.source_root.join("linked.toml");
                fs::hard_link(&source, &alias).expect("native source hard-link fixture required");
                args[2] = alias.to_string_lossy().into_owned();
            }
            "source-root-junction" => {
                let alias = f._temporary.path().join("junction-import-root");
                let output = Command::new("cmd.exe")
                    .args(["/d", "/c", "mklink", "/J"])
                    .arg(&alias)
                    .arg(&f.source_root)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "native junction fixture required: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                args[2] = alias.join("config.toml").to_string_lossy().into_owned();
                args[4] = alias.to_string_lossy().into_owned();
            }
            "profile-hardlink" => {
                fs::hard_link(&f.profile, f.data.join("profile-hardlink.json"))
                    .expect("native profile hard-link fixture required");
            }
            "synced-profile-root" => {
                let mut profile: Value =
                    serde_json::from_slice(&fs::read(&f.profile).unwrap()).unwrap();
                profile["sync_roots"] = json!([f.data]);
                fs::write(&f.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
            }
            "synced-history-root" => {
                fs::create_dir(f.state_dir()).unwrap();
                let mut profile: Value =
                    serde_json::from_slice(&fs::read(&f.profile).unwrap()).unwrap();
                profile["sync_roots"] = json!([f.state_dir()]);
                fs::write(&f.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
            }
            _ => unreachable!(),
        }
        let profile = fs::read(&f.profile).unwrap();
        let output = f.run(&args).await;
        assert!(
            !output.status.success(),
            "{scenario}: native alias or sync-root import unexpectedly accepted"
        );
        if scenario == "synced-history-root" {
            assert_eq!(
                fs::read_dir(f.state_dir()).unwrap().count(),
                0,
                "a synchronized adjacent history directory must remain empty"
            );
        } else {
            assert!(
                !f.state_dir().exists(),
                "{scenario}: rejected import created a revision journal"
            );
        }
        assert_eq!(fs::read(&f.profile).unwrap(), profile);
        assert_eq!(fs::read(&source).unwrap(), source_bytes.as_bytes());
        assert!(
            !marker.exists(),
            "{scenario}: rejected source executed an imported command"
        );
    }
}
