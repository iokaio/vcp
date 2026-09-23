// SPDX-License-Identifier: Apache-2.0
//! Draft P8 U04/U05/U07/U09 production command integration. Never provider-backed.
use super::*;
use std::process::Stdio;
use std::time::{Duration, Instant};

fn bounded(mut command: Command) -> Output {
    use std::os::windows::process::CommandExt;
    let stdout = tempfile::tempfile().unwrap();
    let stderr = tempfile::tempfile().unwrap();
    command.creation_flags(0x0800_0000);
    command.stdout(Stdio::from(stdout.try_clone().unwrap()));
    command.stderr(Stdio::from(stderr.try_clone().unwrap()));
    let mut child = command.spawn().unwrap();
    let start = Instant::now();
    let status = loop {
        if stdout.metadata().unwrap().len() > 4 * 1024 * 1024
            || stderr.metadata().unwrap().len() > 4 * 1024 * 1024
        {
            let _ = child.kill();
            let _ = child.wait();
            panic!("production fixture output exceeded 4 MiB per stream");
        }
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > Duration::from_secs(180) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("production fixture command exceeded 180 seconds");
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    fn read(mut file: fs::File) -> Vec<u8> {
        use std::io::{Read, Seek, SeekFrom};
        assert!(file.metadata().unwrap().len() <= 4 * 1024 * 1024);
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        bytes
    }
    Output {
        status,
        stdout: read(stdout),
        stderr: read(stderr),
    }
}

fn call(workspace: &Path, data: &Path, args: &[&str]) -> Output {
    let binary = std::env::var_os("VCP_TEST_PRODUCTION_BINARY")
        .expect("exact extracted production executable is required");
    let mut command = Command::new(binary);
    // Restore's destination intentionally does not exist until authenticated apply.
    command.current_dir(workspace.parent().unwrap());
    command
        .args(["--format", "jsonl", "--non-interactive", "--workspace"])
        .arg(workspace)
        .arg("--data-dir")
        .arg(data)
        .args(args);
    for name in [
        "OPENROUTER_API_KEY",
        "OneDrive",
        "OneDriveConsumer",
        "OneDriveCommercial",
    ] {
        command.env_remove(name);
    }
    bounded(command)
}

async fn state(config: &Config) -> State {
    let store = Store::open(&config.canonical_root, config.backend, &[])
        .await
        .unwrap();
    let snapshot = store.state().clone();
    store.close().await.unwrap();
    snapshot
}

async fn seed_plan_policy(config: &Config) {
    use vcp_domain::policy::{Autonomy, Policy};
    use vcp_protocol::command::{Command as CanonicalCommand, CommandEnvelope};
    let store = Store::open(&config.canonical_root, config.backend, &[])
        .await
        .unwrap();
    let mut engine = vcp_engine::Engine::new(store).unwrap();
    let workspace = world(engine.store().state());
    assert!(
        vcp_engine::policy::optional(engine.store().state(), &config.workspace)
            .unwrap()
            .is_none()
    );
    let policy = Policy {
        workspace: config.workspace.clone(),
        revision: PolicyRevision::ZERO,
        mode: Autonomy::Plan,
        denials: vec![],
        workspace_roots: std::collections::BTreeSet::from([RootId::parse(
            config.workspace.as_str(),
        )
        .unwrap()]),
        automatic_effects: std::collections::BTreeSet::new(),
        timeout_ceiling_ms: Units::new(30_000),
        output_ceiling_bytes: ByteCount::new(1024 * 1024),
    };
    let command = CommandEnvelope {
        version: vcp_protocol::version::VERSION,
        id: CommandId::new(),
        workspace: config.workspace.clone(),
        session: config.session.clone(),
        task: None,
        caller: config.actor.clone(),
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        payload: CanonicalCommand::SetPolicy {
            policy: policy.clone(),
        },
    };
    let access = vcp_engine::Access {
        actor: config.actor.clone(),
        workspace: config.workspace.clone(),
        session: config.session.clone(),
        authority: workspace.authority,
        read: true,
        write: true,
        bootstrap: false,
    };
    engine
        .handle(
            command,
            &access,
            &vcp_engine::HostFacts::inspect(Timestamp::new(400)),
        )
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(
            vcp_engine::policy::current(engine.store().state(), &config.workspace).unwrap()
        )
        .unwrap(),
        serde_json::to_value(policy).unwrap()
    );
    engine.into_store().close().await.unwrap();
}

fn selected(data: &Path) -> WorkspaceEntry {
    let entries: Vec<_> = fs::read_dir(data.join("workspaces"))
        .unwrap()
        .map(|entry| entry.unwrap().path().join("workspace.json"))
        .filter(|path| path.is_file())
        .collect();
    assert_eq!(entries.len(), 1);
    serde_json::from_slice(&fs::read(&entries[0]).unwrap()).unwrap()
}

fn world(snapshot: &State) -> vcp_domain::workspace::Workspace {
    snapshot
        .record(
            Collection::Workspace,
            "workspace",
            &WorkspaceId::parse("workspace").unwrap(),
        )
        .unwrap()
        .decode()
        .unwrap()
}

fn no_dispatch(snapshot: &State) {
    assert!(snapshot.records.values().all(|record| !matches!(
        record.collection,
        Collection::Attempt | Collection::Reservation | Collection::Ledger | Collection::Effect
    )));
    let tasks: Vec<_> = snapshot
        .records
        .values()
        .filter(|r| r.collection == Collection::Task)
        .collect();
    assert_eq!(tasks.len(), 2);
    for task in tasks {
        assert_eq!(task.value["state"], "paused");
    }
}

fn positive(response: &Value) -> Vec<Value> {
    let passages = response["passages"].as_array().unwrap();
    assert!(
        !passages.is_empty(),
        "retained sibling was not recalled: {response}"
    );
    let mut evidence = Vec::new();
    for passage in passages {
        assert_eq!(passage["scope"]["workspace"], "workspace");
        assert_eq!(passage["scope"]["task"], "other-task");
        assert!(!passage["evidence"].as_array().unwrap().is_empty());
        evidence.push(json!({"source":passage["source"],"digest":passage["source_digest"],"evidence":passage["evidence"]}));
    }
    evidence.sort_by_key(Value::to_string);
    evidence
}

fn queried(result: &Value) -> &Value {
    assert_eq!(
        result["status"], "queried",
        "local query did not complete: {result}"
    );
    assert!(
        result["response"].is_object(),
        "local query response missing: {result}"
    );
    &result["response"]
}

fn stale_inventory(config: &Config, generation: &str, versions: &[vcp_domain::memory::Version]) {
    let inventory: vcp_memory::search_record::Inventory = serde_json::from_slice(
        &fs::read(
            config
                .canonical_root
                .join("search-generations")
                .join(generation)
                .join("inventory.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(inventory.records.is_empty());
    assert_eq!(inventory.exclusions.len(), versions.len());
    for version in versions {
        let source = vcp_memory::search_record::TextSource::Claim {
            version: version.id.clone(),
            claim: version.proposal.claim.clone(),
        };
        assert!(inventory
            .exclusions
            .iter()
            .any(|exclusion| exclusion.source.as_ref() == Some(&source)
                && exclusion.reason == "stale_binding"));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires hash-pinned production package, MiniLM assets and native Git; zero provider calls"]
async fn production_memory_optimizer_exclusion_survive_encrypted_cross_backend_restore() {
    let assets =
        PathBuf::from(std::env::var_os("VCP_MINILM_ASSETS").expect("pinned assets required"));
    let git =
        PathBuf::from(std::env::var_os("VCP_TEST_NATIVE_GIT").expect("pinned native Git required"));
    assert!(git.is_absolute() && git.is_file() && assets.is_absolute());
    for (from, to, name) in [
        (BackendKind::Files, BackendKind::Sqlite, "sqlite"),
        (BackendKind::Sqlite, BackendKind::Files, "files"),
    ] {
        let setup = |workspace: &Path| {
            let hooks = workspace.parent().unwrap().join("empty-hooks");
            fs::create_dir(&hooks).unwrap();
            fs::write(
                workspace.join("retained.txt"),
                b"independent restored bytes\n",
            )
            .unwrap();
            let git_call = |args: &[&str]| {
                let mut command = Command::new(&git);
                command.env_clear();
                for name in ["PATH", "SystemRoot", "WINDIR", "TEMP", "TMP"] {
                    if let Some(value) = std::env::var_os(name) {
                        command.env(name, value);
                    }
                }
                command
                    .env("GIT_CONFIG_NOSYSTEM", "1")
                    .env("GIT_CONFIG_GLOBAL", "NUL")
                    .env("GIT_CONFIG_SYSTEM", "NUL")
                    .env("GIT_TERMINAL_PROMPT", "0");
                command
                    .current_dir(workspace)
                    .arg("-c")
                    .arg(format!("core.hooksPath={}", hooks.display()))
                    .args([
                        "-c",
                        "core.autocrlf=false",
                        "-c",
                        "commit.gpgsign=false",
                        "-c",
                        "user.name=VCP fixture",
                        "-c",
                        "user.email=fixture@example.invalid",
                    ])
                    .args(args);
                let output = bounded(command);
                assert!(output.status.success(), "Git fixture setup failed");
            };
            git_call(&[
                "init",
                "--quiet",
                &format!("--template={}", hooks.display()),
            ]);
            git_call(&["add", "--", "retained.txt"]);
            git_call(&[
                "commit",
                "--quiet",
                "-m",
                "Synthetic memory restore fixture",
            ]);
        };
        let fixture =
            Fixture::new_scoped_with_setup(from, "workspace", None, None, Some(&setup)).await;
        seed_plan_policy(&fixture.config).await;
        let base = fixture._temp.path();
        let cli = |args: &[&str]| success(call(&fixture.workspace, &fixture.data, args));
        let rebound = cli(&["rebind", "workspace"]);
        let revision = rebound["workspace_revision"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| rebound["workspace_revision"].to_string());
        let trusted = cli(&[
            "workspace",
            "trust",
            "workspace",
            "--expected-revision",
            &revision,
        ]);
        assert_eq!(trusted["trust"], "trusted");
        assert_eq!(trusted["tasks_resumed"], false);
        assert_eq!(trusted["provider_requests"], 0);
        assert_eq!(trusted["execution_profiles_loaded"], false);
        let source_config = selected(&fixture.data).config;
        assert_eq!(
            cli(&["memory", "build", "--assets", assets.to_str().unwrap()])["status"],
            "published"
        );
        let before_query = cli(&[
            "memory",
            "query",
            "retained ocean submarine",
            "--task",
            "other-task",
            "--assets",
            assets.to_str().unwrap(),
        ]);
        positive(queried(&before_query));
        assert!(queried(&before_query)["passages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|passage| passage["rank"]["vector_rank"].is_number()));
        cli(&[
            "optimize",
            "answer",
            "priority",
            "correctness",
            "--expected-revision",
            "0",
        ]);
        let preview = cli(&[
            "memory",
            "prune",
            "--task",
            "task",
            "--preview",
            "--action",
            "exclude",
        ]);
        assert!(preview["selected_count"].as_u64().unwrap() > 0);
        cli(&["prune", "apply", preview["id"].as_str().unwrap()]);
        // Preserve a genuinely unconsumed preview, bound to source authority.
        let stale = cli(&[
            "memory",
            "prune",
            "--task",
            "other-task",
            "--preview",
            "--action",
            "exclude",
        ]);
        let recovery = base.join("independent-recovery");
        let vault = base.join("ciphertext-vault");
        let staging = base.join("backup-staging");
        for path in [&recovery, &vault, &staging] {
            fs::create_dir(path).unwrap();
        }
        let enrolled = cli(&[
            "backup",
            "keys",
            "create",
            "--recovery-dir",
            recovery.to_str().unwrap(),
        ]);
        let key = enrolled["recovery_copy"].as_str().unwrap();
        let checkpoint = base.join("checkpoint.json");
        fs::write(
            &checkpoint,
            serde_json::to_vec(&enrolled["configuration"]["checkpoint"]).unwrap(),
        )
        .unwrap();
        cli(&[
            "backup",
            "configure",
            "--vault",
            vault.to_str().unwrap(),
            "--staging",
            staging.to_str().unwrap(),
            "--manual-only",
        ]);
        let before = state(&source_config).await;
        no_dispatch(&before);
        let versions: Vec<vcp_domain::memory::Version> = before
            .records
            .values()
            .filter(|record| {
                record.collection == Collection::Claim
                    && record.value["document_type"] == "vcp_memory_version_v1"
            })
            .map(|record| record.decode().unwrap())
            .collect();
        assert_eq!(versions.len(), 2);
        let sibling = versions
            .iter()
            .find(|version| version.scope.task.as_str() == "other-task")
            .unwrap();
        let source_history = cli(&["memory", "inspect", sibling.proposal.claim.as_str()]);
        assert_eq!(source_history["kind"], "governed_memory");
        assert!(source_history["next_cursor"].is_null());
        assert_eq!(source_history["versions"].as_array().unwrap().len(), 1);
        assert_eq!(
            source_history["versions"][0]["version"],
            serde_json::to_value(sibling).unwrap()
        );
        assert!(!source_history["versions"][0]["evidence"]
            .as_array()
            .unwrap()
            .is_empty());
        let exclusion_decisions: Vec<_> = before
            .records
            .values()
            .filter(|record| {
                record.collection == Collection::Projection
                    && record.value["document_type"] == "vcp_retention_decision_v1"
            })
            .collect();
        assert!(
            !exclusion_decisions.is_empty(),
            "fixture must contain canonical exclusion decisions"
        );
        for decision in &exclusion_decisions {
            assert_eq!(decision.value["action"], "exclude");
            assert_eq!(decision.value["recall_excluded"], true);
            assert_eq!(decision.value["purged"], false);
        }
        let result = cli(&[
            "backup",
            "create",
            "--key",
            key,
            "--git",
            git.to_str().unwrap(),
            "--operation",
            CommandId::new().as_str(),
        ]);
        assert_eq!(result["phase"], "finished");
        let source_after_backup = state(&source_config).await;
        let objects: Vec<_> = fs::read_dir(&vault)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "age"))
            .collect();
        assert_eq!(objects.len(), 1);
        let object = &objects[0];
        let ciphertext_hash = vcp_protocol::digest_bytes(&fs::read(object).unwrap());
        let destination = base.join("restored workspace");
        let data = base.join("fresh-data");
        let restore_staging = base.join("restore-staging");
        let enrollment = base.join("fresh-enrollment");
        for path in [&data, &restore_staging, &enrollment] {
            fs::create_dir(path).unwrap();
        }
        success(call(
            &enrollment,
            &data,
            &[
                "backup",
                "keys",
                "--workspace-id",
                "workspace",
                "import",
                "--key",
                key,
                "--lineage",
                enrolled["configuration"]["lineage"].as_str().unwrap(),
                "--checkpoint",
                checkpoint.to_str().unwrap(),
            ],
        ));
        let restore = vec![
            "restore",
            "--workspace-id",
            "workspace",
            "--source",
            object.to_str().unwrap(),
            "--key",
            key,
            "--staging",
            restore_staging.to_str().unwrap(),
            "--backend",
            name,
        ];
        let mut args = restore.clone();
        args.push("--preview");
        let preview = success(call(&destination, &data, &args));
        assert_eq!(preview["recovery_verified"], true);
        assert_eq!(preview["ciphertext_sha256"], ciphertext_hash);
        assert!(!destination.exists());
        let bytes = preview["bytes"].as_u64().unwrap().to_string();
        args = restore;
        args.extend([
            "--operation",
            preview["operation"].as_str().unwrap(),
            "--ciphertext-sha256",
            &ciphertext_hash,
            "--bytes",
            &bytes,
        ]);
        let restored = success(call(&destination, &data, &args));
        assert_eq!(restored["activated"], true);
        assert_eq!(restored["tasks_resumed"], false);
        assert_eq!(restored["execution_grants_restored"], false);
        assert_eq!(restored["search"]["lexical_ready"], true);
        assert!(restored["search"]["semantic_pending"].is_boolean());
        assert!(!destination.join(".git").exists());
        assert_eq!(
            fs::read(destination.join("retained.txt")).unwrap(),
            b"independent restored bytes\n"
        );
        let target = selected(&data).config;
        assert_eq!(target.backend, to);
        let after = state(&target).await;
        no_dispatch(&after);
        assert_eq!(world(&after).trust, vcp_domain::workspace::Trust::Untrusted);
        assert!(world(&after).authority > world(&before).authority);
        let original_policy =
            vcp_engine::policy::current(&before, &source_config.workspace).unwrap();
        let restored_policy = vcp_engine::policy::current(&after, &target.workspace).unwrap();
        assert_eq!(restored_policy.mode, vcp_domain::policy::Autonomy::Plan);
        assert!(restored_policy.automatic_effects.is_empty());
        assert!(restored_policy.workspace_roots.is_empty());
        assert_eq!(
            restored_policy.revision,
            original_policy.revision.next().unwrap()
        );
        assert_eq!(restored_policy.workspace, original_policy.workspace);
        assert_eq!(restored_policy.denials, original_policy.denials);
        assert_eq!(
            restored_policy.timeout_ceiling_ms,
            original_policy.timeout_ceiling_ms
        );
        assert_eq!(
            restored_policy.output_ceiling_bytes,
            original_policy.output_ceiling_bytes
        );
        for record in before.records.values().filter(|record| {
            record.collection == Collection::Claim
                || record.value["document_type"].as_str().is_some_and(|kind| {
                    matches!(
                        kind,
                        "vcp_memory_head_v1" | "vcp_memory_sequence_v1" | "vcp_memory_result_v1"
                    )
                })
                || record.id.starts_with("routing-interview-")
        }) {
            assert_eq!(
                after
                    .record(record.collection, &record.id, &target.workspace)
                    .unwrap(),
                record
            );
        }
        for decision in exclusion_decisions {
            assert_eq!(
                after
                    .record(Collection::Projection, &decision.id, &target.workspace)
                    .unwrap(),
                decision
            );
        }
        for (id, receipt) in &before.transactions {
            assert_eq!(after.transactions.get(id), Some(receipt));
        }
        assert_eq!(
            &after.events[..before.events.len()],
            before.events.as_slice()
        );
        for record in before
            .records
            .values()
            .filter(|record| record.collection == Collection::Task)
        {
            let restored_task = after
                .record(Collection::Task, &record.id, &target.workspace)
                .unwrap();
            for field in ["scope", "root", "parent", "objectives", "steering"] {
                assert_eq!(
                    restored_task.value[field], record.value[field],
                    "restored task {field} changed"
                );
            }
        }
        let store = Store::open(&target.canonical_root, to, &[]).await.unwrap();
        let old_access = vcp_memory::access::Access {
            workspace: target.workspace.clone(),
            actor: target.actor.clone(),
            authority: world(&before).authority,
            read: true,
            write: true,
            tasks: None,
        };
        assert!(vcp_memory::search_record::inventory(
            &store,
            &old_access,
            &[],
            &vcp_memory::search_record::ChunkerSpec::default(),
            vcp_memory::search_record::Limits::default()
        )
        .is_err());
        store.close().await.unwrap();
        let target_cli = |args: &[&str]| success(call(&destination, &data, args));
        let protected = state(&target).await;
        let denied = call(
            &destination,
            &data,
            &["prune", "apply", stale["id"].as_str().unwrap()],
        );
        assert_eq!(denied.status.code(), Some(2));
        assert_eq!(state(&target).await, protected);
        assert_eq!(
            target_cli(&["optimize", "status"])["interview"]["answers"],
            json!({"priority":"correctness"})
        );
        let restored_history = target_cli(&["memory", "inspect", sibling.proposal.claim.as_str()]);
        for field in [
            "workspace",
            "claim",
            "kind",
            "at",
            "versions",
            "next_cursor",
        ] {
            assert_eq!(
                restored_history[field], source_history[field],
                "restored history changed {field}"
            );
        }
        assert_ne!(
            world(&after).binding.repository,
            world(&before).binding.repository
        );
        assert_ne!(
            world(&after).binding.worktree,
            world(&before).binding.worktree
        );
        stale_inventory(
            &target,
            restored["search"]["generation"].as_str().unwrap(),
            &versions,
        );
        assert!(target_cli(&[
            "memory",
            "search",
            "retained ocean submarine",
            "--task",
            "other-task"
        ])["passages"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(target_cli(&[
            "memory",
            "search",
            "retained orchard apple harvest",
            "--task",
            "task"
        ])["passages"]
            .as_array()
            .unwrap()
            .is_empty());
        // Retained history is portable; the old physical binding is not current
        // applicability. Rebuilding must not manufacture authorized source rows.
        let built = target_cli(&["memory", "build", "--assets", assets.to_str().unwrap()]);
        assert_eq!(built["status"], "published");
        eprintln!("production-memory-restore readiness from={from:?} to={to:?} lexical_ready={} semantic_pending={} indexed_records={} excluded_records={} fresh_build_status={}",
            restored["search"]["lexical_ready"], restored["search"]["semantic_pending"],
            restored["search"]["indexed_records"], restored["search"]["excluded_records"], built["status"]);
        stale_inventory(
            &target,
            built["generation"]["id"].as_str().unwrap(),
            &versions,
        );
        for _ in 0..2 {
            let recall = target_cli(&[
                "memory",
                "query",
                "retained ocean submarine",
                "--task",
                "other-task",
                "--assets",
                assets.to_str().unwrap(),
            ]);
            assert!(queried(&recall)["passages"].as_array().unwrap().is_empty());
            assert_eq!(recall["embedding_resources"]["observation_retained"], true);
            assert!(recall["embedding_resources"]["samples"].as_u64().unwrap() >= 2);
            eprintln!("production-memory-restore query from={from:?} to={to:?} status={} embedding_observation_retained={} embedding_samples={} passages=0",
                recall["status"], recall["embedding_resources"]["observation_retained"], recall["embedding_resources"]["samples"]);
            let excluded = target_cli(&[
                "memory",
                "query",
                "retained orchard apple harvest",
                "--task",
                "task",
                "--assets",
                assets.to_str().unwrap(),
            ]);
            assert!(queried(&excluded)["passages"]
                .as_array()
                .unwrap()
                .is_empty());
            no_dispatch(&state(&target).await);
        }
        assert_eq!(state(&source_config).await, source_after_backup);
        eprintln!("production-memory-restore from={from:?} to={to:?} passed; provider_calls=0");
    }
}
