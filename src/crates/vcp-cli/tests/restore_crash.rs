// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! Actual CLI process termination. Requires independently generated native
//! encrypted fixtures; this does not claim an actual cloud or power-loss test.
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};
use vcp_cli::settings::WorkspaceEntry;
use vcp_domain::{
    artifact::CaptureState,
    task::{Task, TaskState},
    workspace::{Trust, Workspace},
    *,
};
use vcp_store::{contract::*, BackendKind, Store};

struct Case {
    _temp: tempfile::TempDir,
    data: PathBuf,
    enrollment: PathBuf,
    destination: PathBuf,
    staging: PathBuf,
    source: PathBuf,
    key: PathBuf,
    fixture: Value,
    backend: BackendKind,
    backend_name: &'static str,
}
impl Case {
    fn new(seed: &Path, backend: BackendKind, backend_name: &'static str) -> Self {
        let fixture: Value =
            serde_json::from_slice(&fs::read(seed.join("fixture.json")).unwrap()).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let enrollment = temp.path().join("enrollment");
        let staging = temp.path().join("staging");
        for path in [&data, &enrollment, &staging] {
            fs::create_dir(path).unwrap();
        }
        let source = temp.path().join("source.age");
        fs::copy(seed.join("snapshot.age"), &source).unwrap();
        assert_eq!(
            vcp_protocol::digest_bytes(&fs::read(&source).unwrap()),
            fixture["ciphertext_sha256"].as_str().unwrap()
        );
        let keys: Vec<_> = fs::read_dir(seed.join("recovery"))
            .unwrap()
            .map(|row| row.unwrap().path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("recovery"))
            .collect();
        assert_eq!(keys.len(), 1);
        assert!(!fs::symlink_metadata(&keys[0])
            .unwrap()
            .file_type()
            .is_symlink());
        let original = temp.path().join("unrelated-original");
        fs::create_dir(&original).unwrap();
        fs::write(
            original.join("preserve.txt"),
            b"original local data survives",
        )
        .unwrap();
        let destination = temp.path().join("restored");
        let value = Self {
            _temp: temp,
            data,
            enrollment,
            destination,
            staging,
            source,
            key: keys[0].clone(),
            fixture,
            backend,
            backend_name,
        };
        let checkpoint = value._temp.path().join("checkpoint.json");
        fs::write(
            &checkpoint,
            serde_json::to_vec(&value.fixture["checkpoint"]).unwrap(),
        )
        .unwrap();
        value.ok(
            &value.enrollment,
            &[
                "backup".into(),
                "keys".into(),
                "--workspace-id".into(),
                value.workspace().to_string(),
                "import".into(),
                "--key".into(),
                value.key.to_string_lossy().into_owned(),
                "--lineage".into(),
                value.fixture["lineage"].as_str().unwrap().into(),
                "--checkpoint".into(),
                checkpoint.to_string_lossy().into_owned(),
            ],
        );
        value
    }
    fn workspace(&self) -> WorkspaceId {
        WorkspaceId::parse(self.fixture["workspace"].as_str().unwrap()).unwrap()
    }
    fn command(&self, work: &Path, args: &[String]) -> Command {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new(env!("CARGO_BIN_EXE_vcp"));
        command
            .creation_flags(0x0800_0000)
            .current_dir(self._temp.path())
            .env_remove("OPENROUTER_API_KEY")
            .env_remove("VCP_TEST_RESTORE_BARRIER")
            .env_remove("VCP_TEST_RESTORE_MARKER")
            .arg("--format")
            .arg("jsonl")
            .arg("--data-dir")
            .arg(&self.data)
            .arg("--workspace")
            .arg(work)
            .args(args)
            .stdin(Stdio::null());
        command
    }
    fn output(&self, work: &Path, args: &[String]) -> Output {
        let id = CommandId::new();
        let stdout = self._temp.path().join(format!("{id}.stdout"));
        let stderr = self._temp.path().join(format!("{id}.stderr"));
        let mut child = self
            .command(work, args)
            .stdout(fs::File::create(&stdout).unwrap())
            .stderr(fs::File::create(&stderr).unwrap())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(180);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("bounded CLI command timed out");
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        assert!(fs::metadata(&stdout).unwrap().len() <= 4 * 1024 * 1024);
        assert!(fs::metadata(&stderr).unwrap().len() <= 64 * 1024);
        Output {
            status,
            stdout: fs::read(stdout).unwrap(),
            stderr: fs::read(stderr).unwrap(),
        }
    }
    fn ok(&self, work: &Path, args: &[String]) -> Value {
        let output = self.output(work, args);
        assert!(
            output.status.success(),
            "CLI failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .find(|row| row["type"] == "result" && row.get("data").is_some())
            .unwrap()["data"]
            .clone()
    }
    fn restore_args(&self) -> Vec<String> {
        vec![
            "restore".into(),
            "--workspace-id".into(),
            self.workspace().to_string(),
            "--source".into(),
            self.source.to_string_lossy().into_owned(),
            "--key".into(),
            self.key.to_string_lossy().into_owned(),
            "--staging".into(),
            self.staging.to_string_lossy().into_owned(),
            "--backend".into(),
            self.backend_name.into(),
        ]
    }
    fn preview(&self) -> (String, Vec<String>) {
        let mut args = self.restore_args();
        args.push("--preview".into());
        let preview = self.ok(&self.destination, &args);
        assert!(preview["expected_descriptor"].is_null());
        let operation = preview["operation"].as_str().unwrap().to_owned();
        let mut args = self.restore_args();
        args.extend([
            "--operation".into(),
            operation.clone(),
            "--ciphertext-sha256".into(),
            preview["ciphertext_sha256"].as_str().unwrap().into(),
            "--bytes".into(),
            preview["bytes"].as_u64().unwrap().to_string(),
        ]);
        (operation, args)
    }
    fn kill_at(&self, args: &[String], phase: &str) {
        let marker = self._temp.path().join("kill-ready");
        let stdout = self._temp.path().join("killed-stdout.jsonl");
        let stderr = self._temp.path().join("killed-stderr.log");
        let mut child = self
            .command(&self.destination, args)
            .env("VCP_TEST_RESTORE_BARRIER", phase)
            .env("VCP_TEST_RESTORE_MARKER", &marker)
            .stdout(fs::File::create(stdout).unwrap())
            .stderr(fs::File::create(&stderr).unwrap())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            if fs::read_to_string(&marker).ok().as_deref() == Some(phase) {
                break;
            }
            if let Some(status) = child.try_wait().unwrap() {
                panic!(
                    "CLI exited before {phase}: {status}: {}",
                    fs::read_to_string(&stderr).unwrap()
                );
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("CLI barrier {phase} timed out");
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert_eq!(fs::read_to_string(marker).unwrap(), phase);
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success());
    }
    fn registry(&self) -> PathBuf {
        self.data
            .join("workspaces")
            .join(vcp_protocol::digest_bytes(
                self.workspace().as_str().as_bytes(),
            ))
    }
    fn descriptor(&self) -> Option<WorkspaceEntry> {
        let path = self.registry().join("workspace.json");
        path.exists()
            .then(|| serde_json::from_slice(&fs::read(path).unwrap()).unwrap())
    }
    fn original_unchanged(&self) {
        assert_eq!(
            fs::read(self._temp.path().join("unrelated-original/preserve.txt")).unwrap(),
            b"original local data survives"
        );
        assert_eq!(
            vcp_protocol::digest_bytes(&fs::read(&self.source).unwrap()),
            self.fixture["ciphertext_sha256"].as_str().unwrap()
        );
    }
    async fn verified_state(&self) -> State {
        let descriptor = self.descriptor().unwrap();
        assert!(!descriptor.rebind_pending);
        assert_eq!(descriptor.config.backend, self.backend);
        let store = Store::open(&descriptor.config.canonical_root, self.backend, &[])
            .await
            .unwrap();
        let workspace: Workspace = store
            .state()
            .record(
                Collection::Workspace,
                self.workspace().as_str(),
                &self.workspace(),
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(workspace.trust, Trust::Untrusted);
        assert_eq!(
            vcp_engine::policy::current(store.state(), &self.workspace())
                .unwrap()
                .mode,
            vcp_domain::policy::Autonomy::Plan
        );
        let tasks: Vec<Task> = store
            .state()
            .records
            .values()
            .filter(|r| r.collection == Collection::Task)
            .map(|r| r.decode().unwrap())
            .collect();
        assert!(!tasks.is_empty());
        assert!(tasks.iter().all(|t| t.state != TaskState::Running));
        for row in store
            .state()
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
        {
            let value: vcp_domain::artifact::ArtifactDescriptor = row.decode().unwrap();
            if value.state != CaptureState::Purged {
                store.spool().verify(&value).unwrap();
            }
        }
        for (path, bytes) in self.fixture["expected_files"].as_object().unwrap() {
            assert_eq!(
                fs::read(self.destination.join(path)).unwrap(),
                bytes.as_str().unwrap().as_bytes()
            );
        }
        assert!(!self.destination.join(".git").exists());
        let state = store.state().clone();
        store.close().await.unwrap();
        self.original_unchanged();
        state
    }
}
fn fixture_root() -> PathBuf {
    PathBuf::from(
        std::env::var_os("VCP_TEST_PORTABILITY_FIXTURES")
            .expect("explicit native encrypted fixture root required"),
    )
}

#[tokio::test]
#[ignore = "requires native encrypted fixtures; actual qualification CLI process kills"]
async fn cli_restore_survives_six_activation_boundaries_on_both_backends() {
    let fixtures = fixture_root();
    for (seed, backend, name) in [
        ("files", BackendKind::Sqlite, "sqlite"),
        ("sqlite", BackendKind::Files, "files"),
    ] {
        for phase in [
            "intent",
            "import",
            "activation_receipt",
            "descriptor",
            "trust",
            "rebind",
        ] {
            let case = Case::new(&fixtures.join(seed), backend, name);
            let (_, args) = case.preview();
            case.kill_at(&args, phase);
            assert_eq!(
                case.descriptor().is_some(),
                matches!(phase, "descriptor" | "trust" | "rebind"),
                "{phase}"
            );
            let resumed = case.ok(&case.destination, &args);
            assert_eq!(resumed["activated"], true);
            assert_eq!(resumed["tasks_resumed"], false);
            let state = case.verified_state().await;
            let repeated = case.ok(&case.destination, &args);
            assert_eq!(repeated["activated"], true);
            assert_eq!(
                vcp_protocol::canonical_bytes(&case.verified_state().await).unwrap(),
                vcp_protocol::canonical_bytes(&state).unwrap(),
                "exact retry mutated state after {phase}"
            );
        }
    }
}

#[tokio::test]
#[ignore = "requires native encrypted fixtures; actual qualification CLI process kills"]
async fn receipt_before_selection_revalidates_source_and_current_deletion_floor() {
    let fixtures = fixture_root();
    for (seed, backend, name) in [
        ("files", BackendKind::Files, "files"),
        ("sqlite", BackendKind::Sqlite, "sqlite"),
    ] {
        for change in ["source", "canonical", "deletion"] {
            let case = Case::new(&fixtures.join(seed), backend, name);
            let (operation, args) = case.preview();
            case.kill_at(&args, "activation_receipt");
            assert!(case.descriptor().is_none());
            match change {
                "source" => {
                    fs::write(
                        case.destination.join("tracked.txt"),
                        b"new destination work must survive",
                    )
                    .unwrap();
                }
                "canonical" => {
                    let target = case.registry().join("canonical-roots").join(&operation);
                    let mut store = Store::open(&target, backend, &[]).await.unwrap();
                    let original = store.state().events.last().unwrap().event.clone();
                    store
                        .transact(Transaction {
                            id: TransactionId::new(),
                            expected_watermark: store.state().watermark,
                            mutations: vec![],
                            events: vec![vcp_protocol::event::EventInput {
                                id: EventId::new(),
                                workspace: original.workspace,
                                session: original.session,
                                task: original.task,
                                actor: original.actor,
                                correlation: CommandId::new(),
                                causation: None,
                                timestamp: original.timestamp,
                                kind: vcp_protocol::event::EventKind::Diagnostic,
                                artifacts: vec![],
                                data: json!({"qualification":"post-import canonical change"}),
                                metadata: None,
                            }],
                            command: None,
                        })
                        .await
                        .unwrap();
                    store.close().await.unwrap();
                }
                "deletion" => {
                    let path = vcp_cli::backup::trust_path(&case.data, &case.workspace());
                    let mut trust = vcp_store::trust_store::TrustStore::open(
                        &path,
                        std::slice::from_ref(&case.enrollment),
                    )
                    .unwrap();
                    let recovery = vcp_store::keys::RecoveryDirectory::open(
                        case.key.parent().unwrap(),
                        std::slice::from_ref(&case.enrollment),
                    )
                    .unwrap()
                    .open_copy(case.key.file_stem().unwrap().to_str().unwrap())
                    .unwrap();
                    let proof = trust
                        .trust()
                        .verify_restore(
                            &case.source,
                            &recovery,
                            vcp_store::vault_crypto::Limits::default(),
                        )
                        .unwrap();
                    let revision = trust.trust().configuration().revision;
                    trust
                        .update(revision, |local| {
                            local.advance_after_restore(&proof, revision)
                        })
                        .unwrap();
                    let revision = trust.trust().configuration().revision;
                    let floor = trust.trust().configuration().checkpoint.deletion + 1;
                    trust
                        .update(revision, |local| {
                            local.raise_deletion_floor(floor, revision)
                        })
                        .unwrap();
                }
                _ => unreachable!(),
            }
            let refused = case.output(&case.destination, &args);
            assert!(!refused.status.success(), "modified {change} was accepted");
            assert!(
                case.descriptor().is_none(),
                "modified {change} was selected before rejection"
            );
            if change == "source" {
                assert_eq!(
                    fs::read(case.destination.join("tracked.txt")).unwrap(),
                    b"new destination work must survive"
                );
            }
            case.original_unchanged();
        }
    }
}
