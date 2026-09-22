// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! Actual CLI process termination. Requires independently generated native
//! encrypted fixtures; this does not claim an actual cloud or power-loss test.
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Output, Stdio},
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

/// A failed supervisor assertion must not leave a product process parked at
/// its qualification barrier. Drop is bounded and never panics during unwind.
struct GuardedChild(Child);
impl std::ops::Deref for GuardedChild {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.0
    }
}
impl std::ops::DerefMut for GuardedChild {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}
impl Drop for GuardedChild {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(Some(_))) {
            return;
        }
        let _ = self.0.kill();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            match self.0.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                Err(_) => break,
            }
        }
        eprintln!(
            "supervisor could not confirm termination of PID {}",
            self.0.id()
        );
    }
}

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
        Self::with_temp(seed, backend, backend_name, tempfile::tempdir().unwrap())
    }
    fn with_temp(
        seed: &Path,
        backend: BackendKind,
        backend_name: &'static str,
        temp: tempfile::TempDir,
    ) -> Self {
        let fixture: Value =
            serde_json::from_slice(&fs::read(seed.join("fixture.json")).unwrap()).unwrap();
        let base = temp.path().canonicalize().unwrap();
        let data = base.join("data");
        let enrollment = base.join("enrollment");
        let staging = base.join("staging");
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
        let mut child = GuardedChild(
            self.command(work, args)
                .stdout(fs::File::create(&stdout).unwrap())
                .stderr(fs::File::create(&stderr).unwrap())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(180);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                wait_for_exit(&mut child);
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
        if let Some(expected) = preview["expected_descriptor"].as_str() {
            args.extend(["--expected-descriptor".into(), expected.into()]);
        }
        (operation, args)
    }
    fn kill_at(&self, args: &[String], phase: &str) {
        let marker = self._temp.path().join("kill-ready");
        let stdout = self._temp.path().join("killed-stdout.jsonl");
        let stderr = self._temp.path().join("killed-stderr.log");
        let mut child = GuardedChild(
            self.command(&self.destination, args)
                .env("VCP_TEST_RESTORE_BARRIER", phase)
                .env("VCP_TEST_RESTORE_MARKER", &marker)
                .stdout(fs::File::create(&stdout).unwrap())
                .stderr(fs::File::create(&stderr).unwrap())
                .spawn()
                .unwrap(),
        );
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
                wait_for_exit(&mut child);
                panic!("CLI barrier {phase} timed out");
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert_eq!(fs::read_to_string(&marker).unwrap(), phase);
        assert!(
            !fs::read_to_string(&stdout).unwrap().lines().any(|line| {
                serde_json::from_str::<Value>(line)
                    .is_ok_and(|row| row["type"] == "result" && row["data"]["activated"] == true)
            }),
            "restore success was acknowledged before the selected crash boundary"
        );
        // The test supervisor persists its observation outside canonical state
        // before terminating the product process. A timeout never reaches here.
        self.receipt(
            "supervisor-observed.json",
            &json!({
                "barrier": phase, "observed": true, "pid": child.id(),
                "acknowledged": false, "marker": marker,
                "barrier_deadline_seconds": 180,
            }),
        );
        child.kill().unwrap();
        let status = wait_for_exit(&mut child);
        assert!(!status.success());
        self.receipt(
            "supervisor-killed.json",
            &json!({
                "barrier": phase, "termination_observed": true,
                "exit_code": status.code(), "acknowledged": false,
            }),
        );
    }
    fn receipt(&self, name: &str, value: &Value) {
        let mut file = fs::File::create(self._temp.path().join(name)).unwrap();
        file.write_all(&serde_json::to_vec_pretty(value).unwrap())
            .unwrap();
        file.sync_all().unwrap();
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
fn wait_for_exit(child: &mut Child) -> ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        assert!(Instant::now() < deadline, "killed CLI did not exit");
        std::thread::sleep(Duration::from_millis(25));
    }
}
fn fixture_root() -> PathBuf {
    PathBuf::from(
        std::env::var_os("VCP_TEST_PORTABILITY_FIXTURES")
            .expect("explicit native encrypted fixture root required"),
    )
}

/// Retain every case, including failed roots, under an explicit local evidence
/// directory. This is a local activation test, not a new capture/cloud campaign.
#[tokio::test]
#[ignore = "requires native encrypted fixtures and VCP_TEST_RECOVERY_EVIDENCE"]
async fn interrupted_restore_with_competing_roots_uses_validated_selection() {
    use vcp_store::{
        keys::{LocalKeys, RecoveryDirectory},
        trust_store::TrustStore,
        vault_crypto::{Limits, PrivateStaging},
    };
    let fixtures = fixture_root();
    let evidence = PathBuf::from(
        std::env::var_os("VCP_TEST_RECOVERY_EVIDENCE")
            .expect("explicit retained supervisor evidence directory required"),
    );
    fs::create_dir_all(&evidence).unwrap();
    assert!(
        !evidence
            .canonicalize()
            .unwrap()
            .ancestors()
            .any(|path| path.join(".git").exists()),
        "retained CLI plaintext evidence must be outside repositories"
    );
    for (seed, backend, name) in [
        ("files", BackendKind::Sqlite, "sqlite"),
        ("sqlite", BackendKind::Files, "files"),
    ] {
        for (phase, corrupt) in [
            ("activation_receipt", false),
            ("descriptor", false),
            ("activation_receipt", true),
        ] {
            let mut temp = tempfile::Builder::new()
                .prefix(&format!("restore-{name}-{phase}-{corrupt}-"))
                .tempdir_in(&evidence)
                .unwrap();
            temp.disable_cleanup(true);
            let mut case = Case::with_temp(&fixtures.join(seed), backend, name, temp);
            println!("retained restore case: {}", case._temp.path().display());

            // Decode before advancing independent trust; after the predecessor
            // is selected, encrypt the same captured payloads as its descendant.
            let recovery = RecoveryDirectory::open(
                case.key.parent().unwrap(),
                std::slice::from_ref(&case.enrollment),
            )
            .unwrap()
            .open_copy(case.key.file_stem().unwrap().to_str().unwrap())
            .unwrap();
            let (mut manifest, payloads) = {
                let trust = TrustStore::open(
                    &vcp_cli::backup::trust_path(&case.data, &case.workspace()),
                    std::slice::from_ref(&case.enrollment),
                )
                .unwrap();
                let proof = trust
                    .trust()
                    .verify_restore(&case.source, &recovery, Limits::default())
                    .unwrap();
                (
                    proof.restored().manifest.clone(),
                    proof.restored().payloads.clone(),
                )
            };
            let (_, initial) = case.preview();
            assert_eq!(case.ok(&case.destination, &initial)["activated"], true);
            let previous = case.descriptor().unwrap();
            let old_root = previous.config.canonical_root.clone();
            let old_workspace = case.destination.clone();
            fs::write(
                old_workspace.join("preserve-local.txt"),
                b"acknowledged local edits",
            )
            .unwrap();
            let prior = case.verified_state().await;
            let prior_digest =
                vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&prior).unwrap());
            let previous_descriptor = fs::read(case.registry().join("workspace.json")).unwrap();
            let stage_path = case._temp.path().join("descendant-encryption");
            fs::create_dir(&stage_path).unwrap();
            let stage =
                PrivateStaging::open(&stage_path, std::slice::from_ref(&case.data)).unwrap();
            {
                let trust = TrustStore::open(
                    &vcp_cli::backup::trust_path(&case.data, &case.workspace()),
                    std::slice::from_ref(&case.enrollment),
                )
                .unwrap();
                manifest.parent = trust.trust().configuration().checkpoint.parent.clone();
                manifest.sequence += 1;
                let keys = LocalKeys::import(&recovery)
                    .unwrap()
                    .verify_recovery(&recovery)
                    .unwrap();
                let mut encrypted = trust
                    .trust()
                    .encrypt(
                        &keys,
                        &stage,
                        manifest,
                        payloads,
                        trust.trust().configuration().revision,
                        Limits::default(),
                    )
                    .unwrap();
                case.source = case._temp.path().join("descendant.age");
                let mut file = fs::File::create(&case.source).unwrap();
                encrypted.copy_ciphertext(&mut file).unwrap();
                file.sync_all().unwrap();
                case.fixture["ciphertext_sha256"] = json!(encrypted.sha256());
            }
            case.destination = case._temp.path().join("descendant-workspace");
            let (operation, args) = case.preview();
            let candidate = case.registry().join("canonical-roots").join(&operation);
            case.receipt(
                "acknowledged-predecessor.json",
                &json!({
                    "old_root": old_root, "state_sha256": prior_digest,
                    "events": prior.events.len(), "records": prior.records.len(),
                    "commands": prior.commands.len(), "watermark": prior.watermark,
                    "descendant_operation": operation, "backend": name,
                    "barrier": phase, "corrupt_candidate": corrupt,
                }),
            );
            case.kill_at(&args, phase);
            assert!(old_root.is_dir() && candidate.is_dir());
            let selected = case.descriptor().unwrap().config.canonical_root;
            assert_eq!(
                selected,
                if phase == "descriptor" {
                    candidate.clone()
                } else {
                    old_root.clone()
                }
            );
            let imported = Store::open(&candidate, backend, &[]).await.unwrap();
            let imported_state = imported.state().clone();
            imported.close().await.unwrap();
            case.receipt(
                "candidate-state-before-recovery.json",
                &serde_json::to_value(&imported_state).unwrap(),
            );

            if corrupt {
                // A validly framed append still invalidates the imported exact
                // state receipt. It must not win selection by being newer.
                let mut store = Store::open(&candidate, backend, &[]).await.unwrap();
                let mut event = store.state().events.last().unwrap().event.clone();
                event.id = EventId::new();
                event.correlation = CommandId::new();
                event.kind = vcp_protocol::event::EventKind::Diagnostic;
                event.data = json!({"qualification":"candidate changed after validation"});
                store
                    .transact(Transaction {
                        id: TransactionId::new(),
                        expected_watermark: store.state().watermark,
                        mutations: vec![],
                        events: vec![event],
                        command: None,
                    })
                    .await
                    .unwrap();
                store.close().await.unwrap();
            }
            let (newer, older) = if corrupt {
                (&candidate, &old_root)
            } else {
                (&old_root, &candidate)
            };
            set_root_modified(
                older,
                std::time::UNIX_EPOCH + Duration::from_secs(1_600_000_000),
            );
            set_root_modified(
                newer,
                std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
            );
            assert!(
                fs::metadata(newer).unwrap().modified().unwrap()
                    > fs::metadata(older).unwrap().modified().unwrap()
            );
            case.receipt(
                "recovery-inputs.json",
                &json!({
                    "selected_root_before_retry": selected,
                    "newer_root": newer, "newer_mtime_seconds": 1_700_000_000u64,
                    "older_root": older, "older_mtime_seconds": 1_600_000_000u64,
                    "candidate_changed_after_validation": corrupt,
                    "old_and_candidate_roots_present": true,
                }),
            );
            let result = case.output(&case.destination, &args);
            if corrupt {
                assert!(!result.status.success(), "invalid newer candidate selected");
                assert_eq!(
                    fs::read(case.registry().join("workspace.json")).unwrap(),
                    previous_descriptor
                );
            } else {
                assert!(
                    result.status.success(),
                    "restore recovery failed: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
                assert_eq!(case.descriptor().unwrap().config.canonical_root, candidate);
                let recovered = case.verified_state().await;
                assert_retained_import(&imported_state, &recovered);
                let repeated = case.ok(&case.destination, &args);
                assert_eq!(repeated["activated"], true);
                assert_eq!(repeated["tasks_resumed"], false);
                assert_eq!(
                    vcp_protocol::canonical_bytes(&case.verified_state().await).unwrap(),
                    vcp_protocol::canonical_bytes(&recovered).unwrap()
                );
            }
            let old = Store::open(&old_root, backend, &[]).await.unwrap();
            assert_eq!(
                vcp_protocol::canonical_bytes(old.state()).unwrap(),
                vcp_protocol::canonical_bytes(&prior).unwrap()
            );
            for record in old
                .state()
                .records
                .values()
                .filter(|row| row.collection == Collection::Artifact)
            {
                let artifact: vcp_domain::artifact::ArtifactDescriptor = record.decode().unwrap();
                if artifact.state != CaptureState::Purged {
                    old.spool().verify(&artifact).unwrap();
                }
            }
            old.close().await.unwrap();
            assert_eq!(
                fs::read(old_workspace.join("preserve-local.txt")).unwrap(),
                b"acknowledged local edits"
            );
            case.receipt(
                "recovery-result.json",
                &json!({
                    "pass": true, "barrier": phase, "backend": name,
                    "candidate_corrupted": corrupt, "candidate_selected": !corrupt,
                    "newer_root": newer, "old_root": old_root, "candidate": candidate,
                    "predecessor_state_sha256": prior_digest,
                    "predecessor_records_and_artifacts_unchanged": true,
                    "imported_history_accounting_and_child_state_unchanged": !corrupt,
                    "exact_retry_state_unchanged": !corrupt,
                    "selected_root": case.descriptor().unwrap().config.canonical_root,
                }),
            );
        }
    }
}

fn assert_retained_import(imported: &State, recovered: &State) {
    // Rebind changes workspace authority/binding; the model-free search rebuild
    // changes only its generation, index/projection and local-resource records.
    // Captured history, liabilities, child graph and effect state must remain
    // byte-identical, with no new attempts or effects introduced by recovery.
    let stable = |record: &&Record| {
        !matches!(
            record.collection,
            Collection::Workspace
                | Collection::Generation
                | Collection::IndexIntent
                | Collection::Projection
                | Collection::LocalResources
        )
    };
    let before: std::collections::BTreeMap<_, _> = imported
        .records
        .iter()
        .filter(|(_, row)| stable(row))
        .collect();
    let after: std::collections::BTreeMap<_, _> = recovered
        .records
        .iter()
        .filter(|(_, row)| stable(row))
        .collect();
    assert_eq!(before, after, "restore changed retained canonical facts");
    for collection in [
        Collection::Ledger,
        Collection::Reservation,
        Collection::Attempt,
        Collection::Settlement,
    ] {
        assert!(
            before.values().any(|row| row.collection == collection),
            "rich restore fixture is missing {} evidence",
            collection.name()
        );
    }
    assert!(
        before
            .values()
            .filter(|row| row.collection == Collection::Task)
            .count()
            >= 2,
        "rich restore fixture must retain a root and child task"
    );
    assert!(
        !imported.events.is_empty()
            && !imported.commands.is_empty()
            && !imported.transactions.is_empty()
    );
    assert!(
        recovered.events.starts_with(&imported.events),
        "imported event prefix changed"
    );
    for (key, receipt) in &imported.commands {
        assert_eq!(
            recovered.commands.get(key),
            Some(receipt),
            "imported command receipt changed"
        );
    }
    for (key, receipt) in &imported.transactions {
        assert_eq!(
            recovered.transactions.get(key),
            Some(receipt),
            "imported transaction receipt changed"
        );
    }
}

fn set_root_modified(path: &Path, modified: std::time::SystemTime) {
    use std::os::windows::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .access_mode(0x0100) // FILE_WRITE_ATTRIBUTES, without directory data access.
        .custom_flags(0x0200_0000)
        .open(path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
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
