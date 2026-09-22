// SPDX-License-Identifier: Apache-2.0
//! Fresh packaged CLI publication, independently decrypted and verified locally.
use super::*;
use std::collections::BTreeMap;
use std::io::Write;
use std::process::Stdio;

async fn data(f: &Fixture, args: &[&str]) -> Value {
    let output = f.run(args).await;
    assert!(
        output.status.success(),
        "packaged CLI step rejected: {args:?}"
    );
    records(&output)
        .into_iter()
        .find(|r| r["type"] == "result")
        .unwrap()["data"]
        .clone()
}

fn decrypt(age: &std::path::Path, key: &std::path::Path, object: &std::path::Path) -> Output {
    Command::new(age)
        .args(["--decrypt", "--identity"])
        .arg(key)
        .arg(object)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

async fn local_data(
    f: &Fixture,
    data_root: &std::path::Path,
    work: &std::path::Path,
    args: &[String],
) -> Value {
    let mut command = Command::new(&f.binary);
    command
        .args(["--format", "jsonl", "--non-interactive", "--data-dir"])
        .arg(data_root)
        .arg("--workspace")
        .arg(work)
        .args(args)
        .stdin(Stdio::null())
        .env_remove("OPENROUTER_API_KEY")
        .env_remove("VCP_TEST_RESTORE_BARRIER")
        .env_remove("VCP_TEST_RESTORE_MARKER");
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(90),
        tokio::task::spawn_blocking(move || command.output().unwrap()),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "local packaged restore/inspection command rejected"
    );
    records(&output)
        .into_iter()
        .find(|r| r["type"] == "result")
        .unwrap()["data"]
        .clone()
}

struct LocalRestore<'a> {
    f: &'a Fixture,
    backend: &'a str,
    before: &'a Value,
    key: &'a std::path::Path,
    object: &'a std::path::Path,
    ciphertext: &'a [u8],
    envelope: &'a Value,
    expected_records: &'a [Value],
    completed_task: &'a str,
    paused_task: &'a str,
}

async fn restore_local(input: LocalRestore<'_>) -> Value {
    let LocalRestore {
        f,
        backend,
        before,
        key,
        object,
        ciphertext,
        envelope,
        expected_records,
        completed_task,
        paused_task,
    } = input;
    use vcp_domain::{
        artifact::{ArtifactDescriptor, CaptureState},
        policy::{AuthorityData, AuthorityDocument, Autonomy},
        task::{Task, TaskState},
        workspace::{Trust, Workspace},
    };
    use vcp_store::{
        contract::{Collection, State},
        BackendKind, Store,
    };
    let data_root = f._temp.path().join("restored-local-data");
    let enrollment = f._temp.path().join("local-enrollment");
    let staging = f._temp.path().join("local-restore-staging");
    let destination = f._temp.path().join("restored-local-workspace");
    for directory in [&data_root, &enrollment, &staging] {
        fs::create_dir(directory).unwrap();
    }
    assert!(!destination.exists());
    let workspace = before["workspace"].as_str().unwrap();
    let checkpoint = f._temp.path().join("independent-before-publication.json");
    fs::write(
        &checkpoint,
        serde_json::to_vec(&before["checkpoint"]).unwrap(),
    )
    .unwrap();
    let imported = local_data(
        f,
        &data_root,
        &enrollment,
        &[
            "backup".into(),
            "keys".into(),
            "--workspace-id".into(),
            workspace.into(),
            "import".into(),
            "--key".into(),
            key.to_string_lossy().into_owned(),
            "--lineage".into(),
            before["lineage"].as_str().unwrap().into(),
            "--checkpoint".into(),
            checkpoint.to_string_lossy().into_owned(),
        ],
    )
    .await;
    assert_eq!(imported["configuration"]["selected"], before["selected"]);
    let target = if backend == "sqlite" {
        "files"
    } else {
        "sqlite"
    };
    let base = vec![
        "restore".into(),
        "--workspace-id".into(),
        workspace.into(),
        "--source".into(),
        object.to_string_lossy().into_owned(),
        "--key".into(),
        key.to_string_lossy().into_owned(),
        "--staging".into(),
        staging.to_string_lossy().into_owned(),
        "--backend".into(),
        target.into(),
    ];
    let mut preview_args = base.clone();
    preview_args.push("--preview".into());
    let preview = local_data(f, &data_root, &destination, &preview_args).await;
    assert!(preview["expected_descriptor"].is_null());
    assert_eq!(
        preview["ciphertext_sha256"],
        vcp_protocol::digest_bytes(ciphertext)
    );
    assert_eq!(preview["bytes"], ciphertext.len());
    let mut apply = base;
    apply.extend([
        "--operation".into(),
        preview["operation"].as_str().unwrap().into(),
        "--ciphertext-sha256".into(),
        preview["ciphertext_sha256"].as_str().unwrap().into(),
        "--bytes".into(),
        preview["bytes"].as_u64().unwrap().to_string(),
    ]);
    let restored = local_data(f, &data_root, &destination, &apply).await;
    assert_eq!(restored["activated"], true);
    assert_eq!(restored["tasks_resumed"], false);
    assert_eq!(restored["execution_grants_restored"], false);
    assert_eq!(restored["rebind"]["trust"], "untrusted");
    for name in ["canary.txt", "value.txt", "package.json", "acceptance.cjs"] {
        assert!(
            fs::read(destination.join(name)).unwrap() == fs::read(f.workspace.join(name)).unwrap(),
            "restored synthetic source differs"
        );
    }
    assert!(
        !destination.join(".git").exists(),
        "restore cannot fabricate repository metadata"
    );
    for (task, state) in [(completed_task, "completed"), (paused_task, "paused")] {
        let status = local_data(
            f,
            &data_root,
            &destination,
            &["tasks".into(), "status".into(), task.into()],
        )
        .await;
        assert_eq!(status["records"][0]["state"], state);
        assert_eq!(status["records"][0]["scope"]["workspace"], workspace);
    }
    for (view, collection) in [("costs", "ledger"), ("verification", "verification")] {
        let page = local_data(
            f,
            &data_root,
            &destination,
            &[
                "inspect".into(),
                completed_task.into(),
                "--view".into(),
                view.into(),
                "--limit".into(),
                "128".into(),
            ],
        )
        .await;
        assert!(page["next_cursor"].is_null());
        for expected in expected_records
            .iter()
            .filter(|r| r["collection"] == collection)
        {
            assert!(
                page["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["collection"] == collection
                        && r["id"] == expected["id"]
                        && r["record"] == expected["value"]),
                "restored CLI lost baseline history"
            );
        }
    }
    // This read-only oracle compares against bytes obtained independently by Go
    // age, not against a history reconstructed by the restore implementation.
    let payloads: BTreeMap<String, Vec<u8>> =
        serde_json::from_value(envelope["payloads"].clone()).unwrap();
    let inventories: Vec<Value> = payloads
        .values()
        .filter_map(|bytes| serde_json::from_slice::<Value>(bytes).ok())
        .filter(|v| v["format"] == "vcp-neutral-history/1")
        .collect();
    assert_eq!(inventories.len(), 1);
    let inventory = &inventories[0];
    let original: State =
        serde_json::from_slice(&payloads[inventory["canonical"].as_str().unwrap()]).unwrap();
    let descriptor = data_root
        .join("workspaces")
        .join(vcp_protocol::digest_bytes(workspace.as_bytes()))
        .join("workspace.json");
    let selected: vcp_cli::settings::WorkspaceEntry =
        serde_json::from_slice(&fs::read(descriptor).unwrap()).unwrap();
    let expected_backend = if target == "files" {
        BackendKind::Files
    } else {
        BackendKind::Sqlite
    };
    assert_eq!(selected.config.backend, expected_backend);
    assert!(!selected.rebind_pending);
    assert!(selected
        .config
        .canonical_root
        .canonicalize()
        .unwrap()
        .starts_with(data_root.canonicalize().unwrap()));
    let store = Store::open(&selected.config.canonical_root, expected_backend, &[])
        .await
        .unwrap();
    let state = store.state();
    let bound: Workspace = state
        .record(Collection::Workspace, workspace, &selected.config.workspace)
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(bound.trust, Trust::Untrusted);
    let policy = vcp_engine::policy::current(state, &selected.config.workspace).unwrap();
    assert_eq!(policy.mode, Autonomy::Plan);
    assert!(policy.automatic_effects.is_empty());
    assert!(policy.workspace_roots.is_empty());
    for row in state
        .records
        .values()
        .filter(|r| r.collection == Collection::Task)
    {
        let task: Task = row.decode().unwrap();
        assert!(task.state.terminal() || task.state == TaskState::Paused);
    }
    for row in state.records.values().filter(|r| {
        r.collection == Collection::Access && r.value["document_type"] == "vcp_authority_v1"
    }) {
        let document: AuthorityDocument = row.decode().unwrap();
        if let AuthorityData::Grant { grant } = document.data {
            assert!(grant.revoked);
        }
    }
    for expected in expected_records {
        let key = format!(
            "{}:{}",
            expected["collection"].as_str().unwrap(),
            expected["id"].as_str().unwrap()
        );
        assert!(
            state.records[&key].value == expected["value"],
            "retained task/check/ledger value changed"
        );
    }
    assert!(
        !original.events.is_empty()
            && !original.commands.is_empty()
            && !original.transactions.is_empty()
    );
    assert!(
        state.events.starts_with(&original.events),
        "original event envelope prefix changed"
    );
    for (id, command) in &original.commands {
        assert!(
            state.commands.get(id) == Some(command),
            "original command receipt changed"
        );
    }
    for (id, transaction) in &original.transactions {
        assert!(
            state.transactions.get(id) == Some(transaction),
            "original transaction receipt changed"
        );
    }
    let mut artifacts = 0;
    let mut bytes = 0usize;
    for (id, row) in original
        .records
        .iter()
        .filter(|(_, row)| row.collection == Collection::Artifact)
    {
        assert!(
            state.records.get(id) == Some(row),
            "artifact descriptor changed after local restore"
        );
        let descriptor: ArtifactDescriptor = row.decode().unwrap();
        if descriptor.state == CaptureState::Purged {
            continue;
        }
        let mut chunks: Vec<_> = inventory["parts"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|part| {
                part["artifact"] == descriptor.spec.id.as_str() && part["role"]["kind"] == "chunk"
            })
            .collect();
        chunks.sort_by_key(|part| part["role"]["index"].as_u64().unwrap());
        let mut expected = Vec::new();
        for (index, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk["role"]["index"], index);
            expected.extend_from_slice(&payloads[chunk["digest"].as_str().unwrap()]);
        }
        let mut actual = Vec::new();
        store.spool().read(&descriptor, &mut actual).unwrap();
        assert!(
            actual == expected,
            "restored artifact payload differs from independently decrypted snapshot"
        );
        artifacts += 1;
        bytes += actual.len();
    }
    assert!(artifacts > 0 && bytes > 0);
    let exact = state.clone();
    store.close().await.unwrap();
    let retry = local_data(f, &data_root, &destination, &apply).await;
    assert_eq!(retry["reconciled"], true);
    assert_eq!(retry["rebind"]["already_completed"], true);
    assert_eq!(retry["tasks_resumed"], false);
    let reopened = Store::open(&selected.config.canonical_root, expected_backend, &[])
        .await
        .unwrap();
    assert!(
        reopened.state() == &exact,
        "exact local restore retry changed canonical state"
    );
    reopened.close().await.unwrap();
    assert_eq!(
        vcp_protocol::digest_bytes(&fs::read(object).unwrap()),
        vcp_protocol::digest_bytes(ciphertext)
    );
    json!({"destination_backend":target,"activated":true,"untrusted":true,"plan_policy_without_grants":true,"unfinished_task_paused":true,"completed_task_preserved":true,"baseline_records_verified":expected_records.len(),"events_verified":original.events.len(),"commands_verified":original.commands.len(),"transactions_verified":original.transactions.len(),"artifact_payloads_verified":artifacts,"artifact_bytes_verified":bytes,"source_files_verified":4,"exact_retry_unchanged":true,"tasks_resumed":false,"independent_machine":false})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the exact extracted package and pinned Go age; run explicit P8 qualification"]
async fn packaged_fresh_snapshots_pass_independent_age_signature_inventory_and_negative_controls() {
    std::env::var_os("VCP_TEST_SKILL_PACKAGE").expect("exact extracted package required");
    let age = PathBuf::from(
        std::env::var_os("VCP_TEST_AGE").expect("pinned independent Go age required"),
    );
    let node = PathBuf::from(std::env::var_os("VCP_TEST_NODE").expect("independent Node required"));
    let git = PathBuf::from(std::env::var_os("VCP_TEST_GIT").expect("native Git required"));
    let report = PathBuf::from(
        std::env::var_os("VCP_TEST_P803_CRYPTO_REPORT")
            .expect("new metadata receipt path required"),
    );
    assert!(!report.exists(), "qualification receipt is create-only");
    let pin: Value = serde_json::from_str(include_str!(
        "../../../../third_party/components/age-qualification.json"
    ))
    .unwrap();
    let age_sha = vcp_protocol::digest_bytes(&fs::read(&age).unwrap());
    assert_eq!(age_sha, pin["binary_sha256"].as_str().unwrap());
    let version = Command::new(&age).arg("--version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8(version.stdout).unwrap().trim(),
        pin["version"].as_str().unwrap()
    );
    let mut cases = Vec::new();
    for backend in ["sqlite", "files"] {
        let server = MockServer::start().await;
        let mut f = Fixture::new(&server.uri(), "complete");
        f.package(true);
        data(&f, &["storage", "configure", "--backend", backend]).await;
        let source = format!("P8-03 fresh packaged snapshot canary: {backend}\n");
        fs::write(f.workspace.join("canary.txt"), &source).unwrap();
        let empty = f._temp.path().join("empty-git-template");
        fs::create_dir(&empty).unwrap();
        let hooks = format!("core.hooksPath={}", empty.display());
        for args in [
            vec!["init", "--quiet", "--template=", "."],
            vec![
                "add",
                "--",
                "value.txt",
                "canary.txt",
                "package.json",
                "acceptance.cjs",
            ],
            vec![
                "-c",
                "user.name=VCP fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "-m",
                "fresh synthetic baseline",
            ],
        ] {
            let mut command = Command::new(&git);
            for (name, _) in std::env::vars_os() {
                if name
                    .to_string_lossy()
                    .to_ascii_uppercase()
                    .starts_with("GIT_")
                {
                    command.env_remove(name);
                }
            }
            let result = command
                .args(["-c", &hooks])
                .args(args)
                .current_dir(&f.workspace)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "NUL")
                .stdin(Stdio::null())
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "synthetic Git fixture preparation rejected"
            );
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .respond_with(move |_: &wiremock::Request| {
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(response(counter.fetch_add(1, Ordering::SeqCst), "complete"))
            })
            .mount(&server)
            .await;
        let run = f
            .run(&[
                "run",
                "Change value to 42 and verify this synthetic task",
                "--autonomy",
                "autonomous",
            ])
            .await;
        assert!(
            run.status.success(),
            "fresh synthetic packaged task did not complete"
        );
        let requests_before = calls.load(Ordering::SeqCst);
        assert!(requests_before > 0);
        assert_eq!(
            fs::read_to_string(f.workspace.join("value.txt")).unwrap(),
            "42\n"
        );
        let task = f.task_id();
        let status = data(&f, &["tasks", "status", &task]).await;
        assert_eq!(status["records"][0]["state"], "completed");
        let mut expected_records =
            vec![json!({"collection":"task","id":task,"value":status["records"][0]})];
        for (view, collection) in [("costs", "ledger"), ("verification", "verification")] {
            let page = data(&f, &["inspect", &task, "--view", view, "--limit", "128"]).await;
            assert!(
                page["next_cursor"].is_null(),
                "fixture history must fit its explicit baseline page"
            );
            let captured: Vec<_> = page["items"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|item| item["collection"] == collection)
                .map(|item| json!({"collection":collection,"id":item["id"],"value":item["record"]}))
                .collect();
            assert!(
                !captured.is_empty(),
                "required history baseline absent: {collection}"
            );
            expected_records.extend(captured);
        }
        let paused_task = f
            .paused_before_send("Retain explicit unfinished work for local restore qualification")
            .await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            requests_before,
            "creating the paused baseline cannot call a provider"
        );
        let paused = data(&f, &["tasks", "status", &paused_task]).await;
        assert_eq!(paused["records"][0]["state"], "paused");
        expected_records
            .push(json!({"collection":"task","id":paused_task,"value":paused["records"][0]}));
        let recovery = f._temp.path().join("independent-recovery");
        let vault = f._temp.path().join("local-vault");
        let stage = f._temp.path().join("private-staging");
        for directory in [&recovery, &vault, &stage] {
            fs::create_dir(directory).unwrap();
        }
        let enrolled = data(
            &f,
            &[
                "backup",
                "keys",
                "create",
                "--recovery-dir",
                recovery.to_str().unwrap(),
            ],
        )
        .await;
        assert_eq!(enrolled["independent_recovery_verified"], true);
        let key = PathBuf::from(enrolled["recovery_copy"].as_str().unwrap());
        let before = &enrolled["configuration"];
        data(
            &f,
            &[
                "backup",
                "configure",
                "--vault",
                vault.to_str().unwrap(),
                "--staging",
                stage.to_str().unwrap(),
                "--manual-only",
            ],
        )
        .await;
        let operation = vcp_domain::CommandId::new().to_string();
        data(
            &f,
            &[
                "backup",
                "create",
                "--key",
                key.to_str().unwrap(),
                "--git",
                git.to_str().unwrap(),
                "--operation",
                &operation,
            ],
        )
        .await;
        let after = data(
            &f,
            &["backup", "keys", "verify", "--key", key.to_str().unwrap()],
        )
        .await;
        let object = vault.join(format!("{operation}.age"));
        let ciphertext = fs::read(&object).unwrap();
        assert!(ciphertext.starts_with(b"age-encryption.org/v1\n"));
        assert!(!ciphertext
            .windows(source.len())
            .any(|window| window == source.as_bytes()));
        let decrypted = decrypt(&age, &key, &object);
        assert!(
            decrypted.status.success(),
            "independent age rejected current CLI-produced snapshot"
        );
        assert!(decrypted.stdout.len() <= 16 * 1024 * 1024);
        let envelope: Value = serde_json::from_slice(&decrypted.stdout).unwrap();
        let expected = json!({"workspace":before["workspace"],"lineage":before["lineage"],"writer":before["selected"]["writer"],
            "sequence":after["configuration"]["checkpoint"]["sequence"],"deletion":after["configuration"]["checkpoint"]["deletion"],
            "parent":before["checkpoint"]["parent"],"manifest_sha256":after["configuration"]["checkpoint"]["parent"],
            "source_sha256":vcp_protocol::digest_bytes(source.as_bytes()),"source_bytes":source.len(),"records":expected_records});
        assert_eq!(
            expected["sequence"].as_u64().unwrap(),
            before["checkpoint"]["sequence"].as_u64().unwrap() + 1
        );
        let mut verifier = Command::new(&node)
            .args(["-e", include_str!("verify_packaged_envelope.cjs")])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        verifier
            .stdin
            .take()
            .unwrap()
            .write_all(
                &serde_json::to_vec(&json!({"envelope":envelope,"expected":expected})).unwrap(),
            )
            .unwrap();
        let checked = verifier.wait_with_output().unwrap();
        assert!(
            checked.status.success(),
            "independent Node signature/inventory verifier rejected snapshot"
        );
        let independent: Value = serde_json::from_slice(&checked.stdout).unwrap();
        let local_restore = restore_local(LocalRestore {
            f: &f,
            backend,
            before,
            key: &key,
            object: &object,
            ciphertext: &ciphertext,
            envelope: &envelope,
            expected_records: &expected_records,
            completed_task: &task,
            paused_task: &paused_task,
        })
        .await;
        // A new recipient supplies an actual wrong key, while retaining the old
        // recovery copy. No key file is read into diagnostic output or argv.
        let revision = &after["configuration"]["revision"];
        let revision = revision
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| revision.to_string());
        let rotated = data(
            &f,
            &[
                "backup",
                "keys",
                "rotate",
                "--recovery-dir",
                recovery.to_str().unwrap(),
                "--expected-revision",
                &revision,
            ],
        )
        .await;
        let wrong = PathBuf::from(rotated["recovery_copy"].as_str().unwrap());
        assert_ne!(
            rotated["configuration"]["selected"]["recipient"],
            before["selected"]["recipient"]
        );
        assert!(
            !decrypt(&age, &wrong, &object).status.success(),
            "wrong recipient accepted snapshot"
        );
        let mut changed = ciphertext.clone();
        *changed.last_mut().unwrap() ^= 1;
        let tampered = f._temp.path().join("tampered-local-copy.age");
        fs::write(&tampered, changed).unwrap();
        assert!(
            !decrypt(&age, &key, &tampered).status.success(),
            "ciphertext tamper accepted"
        );
        let again = decrypt(&age, &key, &object);
        assert!(again.status.success());
        assert_eq!(
            vcp_protocol::digest_bytes(&again.stdout),
            vcp_protocol::digest_bytes(&decrypted.stdout)
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            requests_before,
            "maintenance started provider work"
        );
        assert_eq!(
            fs::read_dir(&vault).unwrap().count(),
            1,
            "only one finalized ciphertext belongs in the vault"
        );
        cases.push(json!({"backend":backend,"local_restore":local_restore,"executable_sha256":vcp_protocol::digest_bytes(&fs::read(&f.binary).unwrap()),
            "ciphertext_sha256":vcp_protocol::digest_bytes(&ciphertext),"ciphertext_bytes":ciphertext.len(),"independent":independent,
            "wrong_key_rejected":true,"tamper_rejected":true,"old_key_preserved_after_rotation":true,
            "synthetic_loopback_requests":requests_before,"paid_provider_requests":0,"maintenance_provider_requests":0}));
    }
    let receipt = json!({"schema":"p803-packaged-independent-crypto/1","age_sha256":age_sha,"age_version":pin["version"],"cases":cases,
        "scope":"fresh synthetic snapshots produced by the exact packaged CLI on this Windows installation","independent_machine":false,
        "cloud_transfer_tested":false,"full_p8_03_qualified":false});
    fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(report)
        .unwrap()
        .write_all(&serde_json::to_vec_pretty(&receipt).unwrap())
        .unwrap();
}
