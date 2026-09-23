// SPDX-License-Identifier: Apache-2.0
//! Production command boundaries: no provider, qualification feature or profile.
#![cfg(windows)]

#[path = "../../vcp-store/tests/common/mod.rs"]
mod common;

#[path = "support/memory_optimizer.rs"]
mod memory_optimizer;

use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use vcp_cli::settings::WorkspaceEntry;
use vcp_domain::{accounting::*, memory::Outcome, task::TaskState, *};
use vcp_lifecycle::foundation::Config;
use vcp_store::{contract::*, BackendKind, Store};

struct Fixture {
    _temp: tempfile::TempDir,
    workspace: PathBuf,
    data: PathBuf,
    config: Config,
    tasks: Vec<Value>,
}

impl Fixture {
    async fn new(backend: BackendKind) -> Self {
        Self::new_scoped(backend, "workspace", None, None).await
    }

    async fn new_scoped(
        backend: BackendKind,
        workspace_id: &str,
        shared_data: Option<&Path>,
        marker: Option<&str>,
    ) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let workspace = base.join("workspace with spaces");
        let data = shared_data
            .map(Path::to_path_buf)
            .unwrap_or_else(|| base.join("private-data"));
        fs::create_dir_all(&workspace).unwrap();
        let mut initial = common::initial();
        let mut w = common::workspace();
        w.id = WorkspaceId::parse(workspace_id).unwrap();
        w.binding.root = workspace.to_string_lossy().into_owned();
        let mut task = common::task();
        task.scope.workspace = w.id.clone();
        let mut session = common::session();
        session.workspace = w.id.clone();
        initial.mutations[1] = Mutation::Put {
            expected: None,
            record: Record::typed(
                Collection::Session,
                session.id.to_string(),
                w.id.clone(),
                session.revision,
                &session,
            )
            .unwrap(),
        };
        initial.events[0].workspace = w.id.clone();
        initial.command.as_mut().unwrap().workspace = w.id.clone();
        task.state = TaskState::Paused;
        task.objectives[0].text = json!({"memory_preference": {
            "key": "orchard", "value": marker.map_or_else(|| "retained orchard apple harvest preference".to_owned(), |marker| format!("retained orchard apple harvest preference {marker}"))
        }})
        .to_string();
        initial.mutations[0] = Mutation::Put {
            expected: None,
            record: Record::typed(
                Collection::Workspace,
                w.id.to_string(),
                w.id.clone(),
                w.revision,
                &w,
            )
            .unwrap(),
        };
        initial.mutations[2] = Mutation::Put {
            expected: None,
            record: Record::typed(
                Collection::Task,
                task.scope.task.to_string(),
                w.id.clone(),
                task.revision,
                &task,
            )
            .unwrap(),
        };
        initial.events[0].data =
            json!({"facts":[{"collection":"task","id":task.scope.task,"value":task}]});
        let directory = data.join("workspaces").join(w.id.as_str());
        let canonical_root = directory.join("canonical");
        fs::create_dir_all(&canonical_root).unwrap();
        let mut store = Store::open(&canonical_root, backend, &[]).await.unwrap();
        store.transact(initial).await.unwrap();
        let origin = store.state().events.last().unwrap().clone();
        let access = vcp_memory::access::Access {
            workspace: w.id.clone(),
            actor: ActorId::parse("human").unwrap(),
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            tasks: None,
        };
        let proposal = vcp_memory::preferences::materialize(&mut store, &access, &origin)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            vcp_memory::repository::propose(&mut store, &access, proposal, Timestamp::new(200))
                .await
                .unwrap()
                .result
                .resolution
                .outcome,
            Outcome::Accepted
        );

        // A real governed sibling source must be filtered, not merely absent.
        let mut sibling = task.clone();
        sibling.scope.task = TaskId::parse("other-task").unwrap();
        sibling.root = sibling.scope.task.clone();
        sibling.objectives[0].source = EventId::parse("other-created").unwrap();
        sibling.cause = sibling.objectives[0].source.clone();
        sibling.objectives[0].text = json!({"memory_preference": {
            "key":"ocean", "value":"retained ocean submarine foreign-task-sentinel"
        }})
        .to_string();
        let mut event = origin.event;
        event.id = sibling.cause.clone();
        event.task = Some(sibling.scope.task.clone());
        event.data =
            json!({"facts":[{"collection":"task","id":sibling.scope.task,"value":sibling}]});
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![Mutation::Put {
                    expected: None,
                    record: Record::typed(
                        Collection::Task,
                        sibling.scope.task.to_string(),
                        w.id.clone(),
                        sibling.revision,
                        &sibling,
                    )
                    .unwrap(),
                }],
                events: vec![event],
                command: None,
            })
            .await
            .unwrap();
        let origin = store.state().events.last().unwrap().clone();
        let proposal = vcp_memory::preferences::materialize(&mut store, &access, &origin)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            vcp_memory::repository::propose(&mut store, &access, proposal, Timestamp::new(300))
                .await
                .unwrap()
                .result
                .resolution
                .outcome,
            Outcome::Accepted
        );
        store.close().await.unwrap();

        let currency: Currency = "USD".to_owned().try_into().unwrap();
        let config = Config {
            canonical_root,
            backend,
            workspace: w.id.clone(),
            session: task.scope.session.clone(),
            binding: w.binding.clone(),
            actor: access.actor,
            root_task: task.scope.task.clone(),
            cap: Money {
                currency: currency.clone(),
                micros: Micros::new(1_000_000),
            },
            protected: Micros::ZERO,
            price: PriceSnapshot {
                id: "a".repeat(64),
                provider: "offline-fixture".into(),
                model: "never-dispatched".into(),
                currency,
                capability: "b".repeat(64),
                valid_until: Timestamp::new(u64::MAX),
                rates: [
                    ChargeCategory::Input,
                    ChargeCategory::Output,
                    ChargeCategory::CacheRead,
                    ChargeCategory::CacheWrite,
                    ChargeCategory::Request,
                    ChargeCategory::ProviderTool,
                ]
                .into_iter()
                .map(|kind| {
                    (
                        kind,
                        Rate {
                            micros: Micros::ZERO,
                            per_units: Units::new(1),
                        },
                    )
                })
                .collect(),
            },
            input_ceiling: Units::new(1024),
            output_ceiling: Units::new(1024),
            max_transport_retries: 0,
            artifact_limit: ByteCount::new(vcp_store::artifact::DEFAULT_ARTIFACT_LIMIT),
            host_tool_denials: vec![],
        };
        let root = vcp_repository::Root::open(
            vcp_repository::RootIdentity {
                workspace: w.id.clone(),
                root: RootId::parse(w.id.as_str()).unwrap(),
                repository: w.binding.repository,
                worktree: w.binding.worktree,
                binding: w.binding.revision,
            },
            &workspace,
        )
        .unwrap();
        let entry = WorkspaceEntry {
            version: 1,
            rebind_pending: false,
            config: config.clone(),
            identity: Some(vcp_cli::binding::capture(&root).unwrap()),
        };
        fs::write(
            directory.join("workspace.json"),
            serde_json::to_vec(&entry).unwrap(),
        )
        .unwrap();
        Self {
            _temp: temp,
            workspace,
            data,
            config,
            tasks: vec![
                serde_json::to_value(task).unwrap(),
                serde_json::to_value(sibling).unwrap(),
            ],
        }
    }

    fn call(&self, args: &[&str]) -> Output {
        let binary = std::env::var_os("VCP_TEST_PRODUCTION_BINARY")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_vcp").into());
        let mut command = Command::new(binary);
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
        for key in [
            "OPENROUTER_API_KEY",
            "OneDrive",
            "OneDriveConsumer",
            "OneDriveCommercial",
        ] {
            command.env_remove(key);
        }
        command
            .current_dir(&self.workspace)
            .args(["--format", "jsonl", "--non-interactive", "--workspace"])
            .arg(&self.workspace)
            .arg("--data-dir")
            .arg(&self.data)
            .args(args)
            .output()
            .unwrap()
    }

    async fn assert_no_dispatch_or_resume(&self) {
        assert_paused_without_dispatch(
            &self.config.canonical_root,
            self.config.backend,
            &self.config.workspace,
            &self.tasks,
        )
        .await;
    }

    async fn assert_search_is_read_only(&self) -> Value {
        let store = Store::open(&self.config.canonical_root, self.config.backend, &[])
            .await
            .unwrap();
        let before = store.state().clone();
        store.close().await.unwrap();
        let response = success(self.call(&["memory", "search", "retained", "--task", "task"]));
        let store = Store::open(&self.config.canonical_root, self.config.backend, &[])
            .await
            .unwrap();
        assert_eq!(
            store.state(),
            &before,
            "ordinary search mutated canonical history"
        );
        store.close().await.unwrap();
        response
    }
}

async fn assert_paused_without_dispatch(
    root: &Path,
    backend: BackendKind,
    workspace: &WorkspaceId,
    tasks: &[Value],
) {
    let store = Store::open(root, backend, &[]).await.unwrap();
    assert_eq!(tasks.len(), 2);
    for task in tasks {
        assert_eq!(task["state"], "paused");
        assert_eq!(task["scope"]["workspace"], workspace.as_str());
        let record = store
            .state()
            .record(
                Collection::Task,
                task["scope"]["task"].as_str().unwrap(),
                workspace,
            )
            .unwrap();
        assert_eq!(&record.value, task, "local memory changed the paused task");
    }
    assert!(store.state().records.values().all(|record| !matches!(
        record.collection,
        Collection::Attempt | Collection::Reservation | Collection::Ledger
    )));
    assert!(!store.state().events.iter().any(|event| matches!(
        event.event.kind,
        vcp_protocol::event::EventKind::AttemptSubmitted
            | vcp_protocol::event::EventKind::ReservationCreated
    )));
    store.close().await.unwrap();
}

fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|frame| frame["type"] == "result")
        .expect("command result")["data"]
        .clone()
}

fn assert_scoped(response: &Value) {
    let passages = response["passages"].as_array().unwrap();
    assert!(!passages.is_empty(), "source was not retrieved: {response}");
    for passage in passages {
        assert_eq!(passage["scope"]["workspace"], "workspace");
        assert_eq!(passage["scope"]["task"], "task");
        assert!(!passage["source"].is_null());
        assert_eq!(passage["source_digest"].as_str().unwrap().len(), 64);
        assert!(!passage["evidence"].as_array().unwrap().is_empty());
        assert!(!passage.to_string().contains("foreign-task-sentinel"));
    }
}

// Finite two-source observation, not a corpus-quality or broad ANN recall claim.
// Read the production-built rows directly and compute cosine distances without
// Component::query, DiskANN, fusion, or the production search ranking code.
fn assert_production_ann_matches_exact(fixture: &Fixture, assets: &Path, generation: &str) {
    use vcp_memory::embedding::{Encoded, LocalEmbedding};
    let directory = fixture
        .config
        .canonical_root
        .join("search-generations")
        .join(generation);
    let vector_bytes = fs::read(directory.join("vectors.json")).unwrap();
    let inventory_bytes = fs::read(directory.join("inventory.json")).unwrap();
    let manifest: Value =
        serde_json::from_slice(&fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["vector_checksum"],
        vcp_protocol::digest_bytes(&vector_bytes)
    );
    assert_eq!(
        manifest["inventory_checksum"],
        vcp_protocol::digest_bytes(&inventory_bytes)
    );
    let document: Value = serde_json::from_slice(&vector_bytes).unwrap();
    let inventory: Value = serde_json::from_slice(&inventory_bytes).unwrap();
    let records = inventory["records"].as_array().unwrap();
    let rows: Vec<Encoded> = serde_json::from_value(document["rows"].clone()).unwrap();
    assert_eq!(records.len(), 2, "finite corpus changed");
    assert_eq!(
        rows.len(),
        2,
        "one embedding chunk per governed source required"
    );
    assert_eq!(
        rows.iter()
            .map(|row| row.identity.scope.task.as_str())
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from(["task", "other-task"])
    );
    for row in &rows {
        let record = records
            .iter()
            .find(|record| record["id"] == row.identity.source)
            .expect("chunk source missing from production inventory");
        assert_eq!(
            record["scope"],
            serde_json::to_value(&row.identity.scope).unwrap()
        );
        assert_eq!(row.identity.scope.workspace, fixture.config.workspace);
        assert_eq!(row.vector.len(), 384);
    }
    let text = "retained orchard apple harvest";
    let query = {
        let model = LocalEmbedding::load(assets, &|| false).unwrap();
        model.query(text, &|| false).unwrap()
    };
    let query_norm = query
        .iter()
        .map(|value| f64::from(*value).powi(2))
        .sum::<f64>()
        .sqrt();
    assert!(query_norm.is_finite() && query_norm > 0.0);
    let mut exact: Vec<_> = rows
        .iter()
        .map(|row| {
            assert_eq!(query.len(), row.vector.len());
            let dot = query
                .iter()
                .zip(&row.vector)
                .map(|(left, right)| f64::from(*left) * f64::from(*right))
                .sum::<f64>();
            let norm = row
                .vector
                .iter()
                .map(|value| f64::from(*value).powi(2))
                .sum::<f64>()
                .sqrt();
            assert!(norm.is_finite() && norm > 0.0);
            (
                1.0 - dot / (query_norm * norm),
                &row.identity.id,
                &row.identity.source,
            )
        })
        .collect();
    exact.sort_by(|left, right| left.0.total_cmp(&right.0).then_with(|| left.1.cmp(right.1)));
    assert!(
        (exact[0].0 - exact[1].0).abs() > 1e-5,
        "ambiguous floating-point tie in finite fixture"
    );
    let observed = success(fixture.call(&[
        "memory",
        "query",
        text,
        "--assets",
        assets.to_str().unwrap(),
        "--limit",
        "2",
    ]));
    assert_eq!(observed["response"]["generation"], generation);
    let degraded = observed["response"]["degraded"].as_array().unwrap();
    assert!(
        !degraded
            .iter()
            .any(|reason| reason == "bounded_exact_subset" || reason == "vector_reduced_recall"),
        "ANN did not supply the full finite corpus: {observed}"
    );
    let passages = observed["response"]["passages"].as_array().unwrap();
    assert_eq!(passages.len(), 2, "finite production recall: {observed}");
    for (index, (distance, _, source)) in exact.iter().enumerate() {
        let passage = passages
            .iter()
            .find(|passage| passage["record_id"].as_str() == Some(source.as_str()))
            .expect("exact source missing from production top two");
        assert_eq!(
            passage["rank"]["vector_rank"],
            index + 1,
            "production ANN rank differs from independent cosine: {observed}"
        );
        assert!(
            (passage["rank"]["vector_distance"].as_f64().unwrap() - distance).abs() < 1e-5,
            "production distance differs from independent cosine: {observed}"
        );
    }
    eprintln!("finite production ANN/exact observation: backend={:?}, records=2, chunks=2, queries=1, matching_ranks=2; no broad quality claim", fixture.config.backend);
}

#[tokio::test]
async fn production_memory_missing_assets_lexical_reopen_and_owner_fence() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let fixture = Fixture::new(backend).await;
        let missing = fixture.data.join("missing-assets");
        let assets = missing.to_str().unwrap();
        assert!(!fixture
            .call(&["memory", "build", "--assets", assets])
            .status
            .success());
        assert!(!fixture
            .call(&["memory", "query", "retained", "--assets", assets])
            .status
            .success());
        let built = success(fixture.call(&[
            "memory",
            "build",
            "--assets",
            assets,
            "--allow-lexical-only",
        ]));
        assert_eq!(built["status"], "lexical_only");
        assert!(!built["generation"].is_null());
        assert_scoped(&fixture.assert_search_is_read_only().await);
        fixture.assert_no_dispatch_or_resume().await;

        // Holding the real store owner must fence a competing production CLI.
        let owner = Store::open(&fixture.config.canonical_root, backend, &[])
            .await
            .unwrap();
        let before = owner.state().clone();
        assert!(!fixture
            .call(&[
                "memory",
                "build",
                "--assets",
                assets,
                "--allow-lexical-only"
            ])
            .status
            .success());
        assert!(!fixture
            .call(&["memory", "query", "retained", "--assets", assets])
            .status
            .success());
        assert_eq!(owner.state(), &before);
        owner.close().await.unwrap();
        fixture.assert_no_dispatch_or_resume().await;
    }
}

#[tokio::test]
#[ignore = "requires provisioned VCP_MINILM_ASSETS; performs local CPU inference only"]
async fn production_memory_real_assets_build_query_reopen_and_scope() {
    let assets =
        std::env::var("VCP_MINILM_ASSETS").expect("provisioned VCP_MINILM_ASSETS required");
    assert!(Path::new(&assets).is_dir());
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let fixture = Fixture::new(backend).await;
        let mut generation = None;
        for _ in 0..2 {
            let built = success(fixture.call(&["memory", "build", "--assets", &assets]));
            assert_eq!(built["status"], "published");
            generation = Some(built["generation"]["id"].as_str().unwrap().to_owned());
            let query = success(fixture.call(&[
                "memory",
                "query",
                "retained orchard apple harvest",
                "--assets",
                &assets,
                "--task",
                "task",
            ]));
            assert_eq!(query["status"], "queried");
            assert_scoped(&query["response"]);
            assert!(
                query["response"]["passages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|passage| passage["rank"]["vector_rank"].is_number()),
                "query did not use the reopened vector index: {query}"
            );
            let excluded = success(fixture.call(&[
                "memory",
                "query",
                "retained",
                "--assets",
                &assets,
                "--task",
                "absent-task",
            ]));
            assert!(excluded["response"]["passages"]
                .as_array()
                .unwrap()
                .is_empty());
            assert_scoped(&fixture.assert_search_is_read_only().await);
            fixture.assert_no_dispatch_or_resume().await;
        }
        assert_production_ann_matches_exact(
            &fixture,
            Path::new(&assets),
            generation.as_deref().unwrap(),
        );
        let before_sibling = success(fixture.call(&[
            "memory",
            "query",
            "retained ocean submarine",
            "--assets",
            &assets,
            "--task",
            "other-task",
        ]));
        let sibling_passages = before_sibling["response"]["passages"].as_array().unwrap();
        assert!(
            !sibling_passages.is_empty(),
            "sibling was absent before exclusion: {before_sibling}"
        );
        assert!(
            sibling_passages
                .iter()
                .all(|passage| passage["scope"]["task"] == "other-task"),
            "unexpected sibling scope: {before_sibling}"
        );
        // Exclude the indexed claim using the supported preview/apply path.
        // Every retention action advances the deletion epoch. Publisher recovery
        // deliberately rejects all prior-epoch generations until explicit rebuild.
        // Retained stale vector bytes must therefore never resurrect the claim.
        let index = fixture
            .config
            .canonical_root
            .join("search-generations")
            .join(generation.unwrap());
        let components: Vec<_> = ["manifest.json", "inventory.json", "vectors.json"]
            .into_iter()
            .map(|name| (index.join(name), fs::read(index.join(name)).unwrap()))
            .collect();
        let preview = success(fixture.call(&[
            "memory",
            "prune",
            "--task",
            "task",
            "--preview",
            "--action",
            "exclude",
        ]));
        assert!(
            preview["selected_count"].as_u64().unwrap() > 0,
            "empty memory exclusion: {preview}"
        );
        let applied = success(fixture.call(&["prune", "apply", preview["id"].as_str().unwrap()]));
        assert_eq!(applied["preview"]["id"], preview["id"]);
        assert_eq!(applied["preview"]["action"], "exclude");
        let after = success(fixture.call(&[
            "memory",
            "query",
            "retained orchard apple harvest",
            "--assets",
            &assets,
            "--task",
            "task",
        ]));
        assert_eq!(after["status"], "queried");
        assert!(
            after["response"]["passages"].as_array().unwrap().is_empty(),
            "excluded claim recalled from stale index; preview={preview}; query={after}"
        );
        assert!(!after["response"]
            .to_string()
            .contains("orchard apple harvest"));
        assert!(fixture.assert_search_is_read_only().await["passages"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(
            after["response"]["rebuild_required"], true,
            "missing deletion-epoch rebuild diagnosis; preview={preview}; query={after}"
        );
        assert!(
            after["response"]["degraded"]
                .as_array()
                .unwrap()
                .iter()
                .any(|reason| reason == "generation_unavailable"),
            "stale generation did not report unavailability; preview={preview}; query={after}"
        );
        for (file, before) in &components {
            assert_eq!(
                fs::read(file).unwrap(),
                *before,
                "query/exclusion rewrote the retained index"
            );
        }
        // Rebuild only after proving stale-index exclusion. The unchanged sibling
        // must survive in the new generation while the excluded target stays out.
        let rebuilt = success(fixture.call(&["memory", "build", "--assets", &assets]));
        assert_eq!(
            rebuilt["status"], "published",
            "rebuild failed after exclusion; preview={preview}; result={rebuilt}"
        );
        let excluded_again = success(fixture.call(&[
            "memory",
            "query",
            "retained orchard apple harvest",
            "--assets",
            &assets,
            "--task",
            "task",
        ]));
        assert!(
            excluded_again["response"]["passages"]
                .as_array()
                .unwrap()
                .is_empty(),
            "rebuild resurrected exclusion; preview={preview}; query={excluded_again}"
        );
        let sibling = success(fixture.call(&[
            "memory",
            "query",
            "retained ocean submarine",
            "--assets",
            &assets,
            "--task",
            "other-task",
        ]));
        let passages = sibling["response"]["passages"].as_array().unwrap();
        assert!(
            !passages.is_empty(),
            "unrelated sibling lost after rebuild; preview={preview}; query={sibling}"
        );
        assert!(
            passages
                .iter()
                .all(|passage| passage["scope"]["task"] == "other-task"),
            "unexpected scope after exclusion; preview={preview}; query={sibling}"
        );
        for (file, before) in components {
            assert_eq!(
                fs::read(file).unwrap(),
                before,
                "query/exclusion rewrote the retained index"
            );
        }
        fixture.assert_no_dispatch_or_resume().await;
    }
}

/// Prepare disposable canonical roots for the native offline supervisor. This
/// opt-in test seeds data only; the supervisor launches the production binary
/// under its network-denied token in separate processes.
#[tokio::test]
#[ignore = "requires a dedicated supervisor directory and offline seed sentinel"]
async fn production_memory_prepare_offline_fixture() {
    let directory = std::env::current_dir().unwrap();
    let workspace = WorkspaceId::new();
    let guard = vcp_repository::Root::open(
        vcp_repository::RootIdentity {
            workspace: workspace.clone(),
            root: RootId::parse(workspace.as_str()).unwrap(),
            repository: "offline-memory-seeder".into(),
            worktree: "offline-memory-seeder".into(),
            binding: Revision::ZERO,
        },
        &directory,
    )
    .unwrap();
    let _held = guard.hold(None, true).unwrap();
    let request = guard
        .read(Path::new("memory-offline-seed-request.json"), 4096)
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&request.bytes).unwrap(),
        json!({
            "schema": "vcp-memory-offline-seed-request/1"
        })
    );
    let directory = directory.canonicalize().unwrap();
    let temporary = std::env::temp_dir().canonicalize().unwrap();
    assert!(
        temporary.starts_with(&directory) && temporary != directory,
        "supervisor TEMP must be a dedicated descendant of its current directory"
    );
    let temporary_guard = vcp_repository::Root::open(
        vcp_repository::RootIdentity {
            workspace: workspace.clone(),
            root: RootId::parse(workspace.as_str()).unwrap(),
            repository: "offline-memory-seeder-temp".into(),
            worktree: "offline-memory-seeder-temp".into(),
            binding: Revision::ZERO,
        },
        &temporary,
    )
    .unwrap();
    let _temporary_held = temporary_guard.hold(None, true).unwrap();
    // Reserve the output before creating fixtures; never overwrite prior evidence.
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("memory-offline-fixtures.json"))
        .unwrap();
    let mut fixtures = Vec::new();
    let mut rows = Vec::new();
    for (backend, name) in [
        (BackendKind::Files, "files"),
        (BackendKind::Sqlite, "sqlite"),
    ] {
        let fixture = Fixture::new(backend).await;
        assert!(fixture.config.canonical_root.starts_with(&temporary));
        rows.push(json!({
            "backend": name,
            "workspace": fixture.workspace,
            "data": fixture.data,
            "canonical_root": fixture.config.canonical_root,
            "workspace_id": fixture.config.workspace,
            "root_task": fixture.config.root_task,
            "tasks": fixture.tasks,
        }));
        fixtures.push(fixture);
    }
    for fixture in fixtures {
        let _retained = fixture._temp.keep();
    }
    serde_json::to_writer_pretty(
        &mut output,
        &json!({
            "schema": "vcp-memory-offline-fixtures/1",
            "model_calls": 0,
            "subprocesses": 0,
            "fixtures": rows,
        }),
    )
    .unwrap();
    output.sync_all().unwrap();
}

/// Verify the supervisor's exact seeded tasks after restricted CLI operations.
#[tokio::test]
#[ignore = "requires the offline supervisor's retained fixture receipt"]
async fn production_memory_verify_offline_fixture() {
    let directory = std::env::current_dir().unwrap();
    let workspace = WorkspaceId::new();
    let guard = vcp_repository::Root::open(
        vcp_repository::RootIdentity {
            workspace: workspace.clone(),
            root: RootId::parse(workspace.as_str()).unwrap(),
            repository: "offline-memory-verifier".into(),
            worktree: "offline-memory-verifier".into(),
            binding: Revision::ZERO,
        },
        &directory,
    )
    .unwrap();
    let _held = guard.hold(None, true).unwrap();
    let input = guard
        .read(Path::new("memory-offline-fixtures.json"), 65536)
        .unwrap();
    let receipt: Value = serde_json::from_slice(&input.bytes).unwrap();
    assert_eq!(receipt["schema"], "vcp-memory-offline-fixtures/1");
    let directory = directory.canonicalize().unwrap();
    let rows = receipt["fixtures"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    for (row, expected) in rows.iter().zip(["files", "sqlite"]) {
        assert_eq!(row["backend"], expected);
        assert_eq!(row["workspace_id"], "workspace");
        assert_eq!(row["root_task"], "task");
        let root = Path::new(row["canonical_root"].as_str().unwrap());
        let relative = root
            .strip_prefix(&directory)
            .expect("canonical fixture outside supervisor directory");
        let _root_held = guard.hold(Some(relative), true).unwrap();
        assert_paused_without_dispatch(
            root,
            serde_json::from_value(row["backend"].clone()).unwrap(),
            &WorkspaceId::parse(row["workspace_id"].as_str().unwrap()).unwrap(),
            row["tasks"].as_array().unwrap(),
        )
        .await;
    }
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("memory-offline-verification.json"))
        .unwrap();
    serde_json::to_writer_pretty(
        &mut output,
        &json!({
            "schema": "vcp-memory-offline-verification/1", "passed": true,
            "fixture_receipt_sha256": vcp_protocol::digest_bytes(&input.bytes),
            "backends": ["files", "sqlite"], "unchanged_paused_tasks": 4,
            "model_calls": 0, "subprocesses": 0,
        }),
    )
    .unwrap();
    output.sync_all().unwrap();
}

/// Both populated roots are discoverable in the same production registry. Task
/// IDs deliberately collide across roots; only their workspace identities differ.
#[tokio::test]
#[ignore = "requires provisioned VCP_MINILM_ASSETS; two populated local workspace roots"]
async fn production_memory_shared_registry_cross_workspace_isolation() {
    use std::collections::BTreeSet;
    let assets =
        std::env::var("VCP_MINILM_ASSETS").expect("provisioned VCP_MINILM_ASSETS required");
    assert!(Path::new(&assets).is_dir());
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let registry = tempfile::tempdir().unwrap();
        let data = registry
            .path()
            .canonicalize()
            .unwrap()
            .join("shared-private-data");
        let left_marker = "workspacealpharetainedorchidmarker";
        let right_marker = "workspacebetaretainedsubmarinemarker";
        let left =
            Fixture::new_scoped(backend, "workspace-alpha", Some(&data), Some(left_marker)).await;
        let right =
            Fixture::new_scoped(backend, "workspace-beta", Some(&data), Some(right_marker)).await;
        assert_eq!(left.data, right.data);
        assert_ne!(left.config.canonical_root, right.config.canonical_root);
        assert_eq!(left.config.root_task, right.config.root_task);
        assert_eq!(fs::read_dir(data.join("workspaces")).unwrap().count(), 2);
        let left_build = success(left.call(&["memory", "build", "--assets", &assets]));
        let right_build = success(right.call(&["memory", "build", "--assets", &assets]));
        assert_eq!(left_build["status"], "published");
        assert_eq!(right_build["status"], "published");
        assert_ne!(
            left_build["generation"]["id"],
            right_build["generation"]["id"]
        );
        let observe = |fixture: &Fixture, query: &str| {
            success(fixture.call(&[
                "memory", "query", query, "--assets", &assets, "--task", "task",
            ]))
        };
        let check = |fixture: &Fixture, observed: &Value, own: &str, foreign: &str| {
            assert_eq!(observed["status"], "queried");
            let passages = observed["response"]["passages"].as_array().unwrap();
            assert!(
                !passages.is_empty(),
                "populated workspace source missing: {observed}"
            );
            assert!(
                !observed["response"].to_string().contains(foreign),
                "foreign marker crossed workspace boundary: {observed}"
            );
            for passage in passages {
                assert_eq!(
                    passage["scope"]["workspace"],
                    fixture.config.workspace.as_str(),
                    "foreign scope: {observed}"
                );
                assert_eq!(passage["scope"]["task"], "task");
                assert!(
                    passage["text"].as_str().unwrap().contains(own),
                    "wrong retained source: {observed}"
                );
                assert!(!passage["source"].is_null());
                assert_eq!(passage["source_digest"].as_str().unwrap().len(), 64);
                assert!(!passage["evidence"].as_array().unwrap().is_empty());
                assert!(passage["rank"]["vector_rank"].is_number());
            }
            passages
                .iter()
                .map(|passage| passage["record_id"].as_str().unwrap().to_owned())
                .collect::<BTreeSet<_>>()
        };
        let left_ids = check(
            &left,
            &observe(&left, left_marker),
            left_marker,
            right_marker,
        );
        let right_ids = check(
            &right,
            &observe(&right, right_marker),
            right_marker,
            left_marker,
        );
        assert!(left_ids.is_disjoint(&right_ids));
        // Foreign query text may match authorized local semantic results. Assert
        // source identity and scope, rather than incorrectly requiring no hits.
        for (fixture, own, foreign, expected, forbidden) in [
            (&left, left_marker, right_marker, &left_ids, &right_ids),
            (&right, right_marker, left_marker, &right_ids, &left_ids),
            (&left, left_marker, right_marker, &left_ids, &right_ids),
        ] {
            let response = observe(fixture, foreign);
            let actual = check(fixture, &response, own, foreign);
            assert_eq!(&actual, expected);
            assert!(actual.is_disjoint(forbidden));
        }
        left.assert_no_dispatch_or_resume().await;
        right.assert_no_dispatch_or_resume().await;
        eprintln!("production shared-registry isolation: backend={backend:?}, populated_workspaces=2, colliding_root_task_ids=1, positive_queries=2, alternating_foreign_queries=3; zero provider attempts");
    }
}
