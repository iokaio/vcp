// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::sync::atomic::AtomicBool;
use vcp_lifecycle::foundation::{
    memory_publication::{self, Status},
    memory_vectors,
};
use vcp_memory::publication::Publisher;

fn publication_request(
    private: &std::path::Path,
    assets: std::path::PathBuf,
    degrade: bool,
) -> memory_publication::Request {
    memory_publication::Request {
        vectors: memory_vectors::Request {
            assets,
            private_root: private.into(),
            sources: vec![],
            chunker: vcp_memory::search_record::ChunkerSpec::default(),
            cancelled: Arc::new(AtomicBool::new(false)),
        },
        allow_lexical_only: degrade,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn missing_assets_require_explicit_lexical_only_publication_and_preserve_pending_intents() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let (temp, config, host, owner, _test, thread) = vector_fixture(backend).await;
        let private = temp.path().join("private");
        std::fs::create_dir(&private).unwrap();
        let generations = config.canonical_root.join("search-generations");
        let publisher = Arc::new(Publisher::new(&generations).unwrap());
        let manager = host.publication_manager(thread, publisher).unwrap();
        let deferred = host
            .publish_memory(
                thread,
                manager.clone(),
                publication_request(&private, temp.path().join("missing-assets"), false),
            )
            .await
            .unwrap();
        assert!(matches!(deferred.status, Status::Deferred(_)));
        assert!(deferred.manifest.is_none());
        assert!(!host
            .snapshot()
            .unwrap()
            .records
            .values()
            .any(|row| row.collection == Collection::Generation));
        let degraded = host
            .publish_memory(
                thread,
                manager,
                publication_request(&private, temp.path().join("missing-assets"), true),
            )
            .await
            .unwrap();
        assert!(matches!(degraded.status, Status::LexicalOnly(_)));
        let manifest = degraded.manifest.unwrap();
        assert_eq!(manifest.memory_seq, MemorySeq::ZERO);
        assert!(manifest.vector_checksum.is_none());
        assert!(!manifest.vector_deficits.is_empty());
        assert!(manifest.covered_intents.is_empty());
        assert!(degraded.resources.unwrap().observation_retained);
        let resources = degraded.publication_resources.unwrap();
        assert!(resources.observation_retained);
        assert!(resources.samples >= 2);
        assert!(resources.sampled_temporary_disk_peak_bytes > 0);
        eprintln!(
            "lexical_publication_resources={}",
            serde_json::to_string(&resources).unwrap()
        );
        let snapshot = host.snapshot().unwrap();
        let active: vcp_domain::search::Active = snapshot
            .record(
                Collection::Generation,
                config.workspace.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(active.generation, manifest.id);
        assert!(snapshot
            .records
            .values()
            .filter(|row| row.collection == Collection::IndexIntent)
            .all(|row| row.value["status"] == "pending"));
        let inventory: vcp_memory::search_record::Inventory = serde_json::from_slice(
            &std::fs::read(
                generations
                    .join(manifest.id.as_str())
                    .join("inventory.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let reader = vcp_memory::lexical::open(
            &generations.join(manifest.id.as_str()).join("lexical"),
            &inventory,
            vcp_memory::lexical::Limits::default(),
        )
        .unwrap();
        assert_eq!(reader.len(), inventory.records.len());
        drop(reader);
        owner.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "explicit native publication gate: VCP_MINILM_ASSETS required"]
async fn admitted_host_publication_adopts_real_vectors_and_atomically_covers_intents() {
    let assets = std::env::var_os("VCP_MINILM_ASSETS").expect("pinned asset directory required");
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let (temp, config, host, owner, _test, thread) = vector_fixture(backend).await;
        let private = temp.path().join("private");
        std::fs::create_dir(&private).unwrap();
        let generations = config.canonical_root.join("search-generations");
        let manager = host
            .publication_manager(thread, Arc::new(Publisher::new(&generations).unwrap()))
            .unwrap();
        let source = retained_source(&host, &config, thread);
        let mut request = publication_request(&private, assets.clone().into(), false);
        request.vectors.sources.push(source);
        let outcome = host.publish_memory(thread, manager, request).await.unwrap();
        assert_eq!(outcome.status, Status::Published);
        let manifest = outcome.manifest.unwrap();
        let receipt = outcome.receipt.unwrap();
        assert!(manifest.vector_deficits.is_empty());
        assert!(!manifest.covered_intents.is_empty());
        assert!(manifest.memory_seq > MemorySeq::ZERO);
        let publication_resources = outcome.publication_resources.unwrap();
        assert!(publication_resources.observation_retained);
        assert!(publication_resources.samples >= 2);
        assert!(publication_resources.sampled_temporary_disk_peak_bytes > 0);
        eprintln!(
            "publication_preparation_resources={}",
            serde_json::to_string(&publication_resources).unwrap()
        );
        let resources = outcome.resources.unwrap();
        assert!(resources.observation_retained);
        eprintln!(
            "published_local_vector_resources={}",
            serde_json::to_string(&resources).unwrap()
        );
        let snapshot = host.snapshot().unwrap();
        let active: vcp_domain::search::Active = snapshot
            .record(
                Collection::Generation,
                config.workspace.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(active.generation, manifest.id);
        assert_eq!(active.transaction, receipt.transaction);
        for id in &manifest.covered_intents {
            let intent: vcp_domain::memory::IndexIntent = snapshot
                .record(Collection::IndexIntent, id.as_str(), &config.workspace)
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(intent.status, vcp_domain::memory::IndexStatus::Ready);
        }
        assert_eq!(snapshot.transactions[&manifest.transaction], receipt);
        let component = vcp_memory::vector::Component::open(
            &generations.join(manifest.id.as_str()).join("vectors.json"),
            manifest.vector_checksum.as_ref().unwrap(),
            &config.workspace,
            &vcp_memory::embedding::Specification::qualified(),
            &|| false,
        )
        .unwrap();
        let inventory: vcp_memory::search_record::Inventory = serde_json::from_slice(
            &std::fs::read(
                generations
                    .join(manifest.id.as_str())
                    .join("inventory.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(inventory.records.iter().any(|record| record.kind
            == vcp_memory::search_record::SearchKind::Source
            && record.text == "pub fn retained_answer() -> u32 { 42 }\n"));
        let chunks = vcp_memory::embedding::inventory_chunks(&inventory).unwrap();
        assert_eq!(
            component
                .rows()
                .iter()
                .map(|row| &row.identity)
                .collect::<Vec<_>>(),
            chunks.iter().map(|row| &row.identity).collect::<Vec<_>>()
        );
        owner.close().await.unwrap();
    }
}
async fn vector_fixture(
    backend: BackendKind,
) -> (
    tempfile::TempDir,
    Config,
    CanonicalHost,
    vcp_lifecycle::foundation::CanonicalOwner,
    TestCodex,
    codex_protocol::ThreadId,
) {
    vector_fixture_objective(
        backend,
        r#"{"memory_preference":{"key":"output","value":"concise"}}"#,
    )
    .await
}
async fn vector_fixture_objective(
    backend: BackendKind,
    objective: &str,
) -> (
    tempfile::TempDir,
    Config,
    CanonicalHost,
    vcp_lifecycle::foundation::CanonicalOwner,
    TestCodex,
    codex_protocol::ThreadId,
) {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    let config = config(&temp.path().join("canonical"), &workspace, backend);
    let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
    host.command(
        Command::CreateTask {
            root: config.root_task.clone(),
            parent: None,
            fork_origin: None,
            objective: Objective {
                text: objective.into(),
                constraints: vec![],
                acceptance: vec!["retain preference".into()],
                source: EventId::new(),
                steering: SteeringRevision::ZERO,
            },
            fingerprint: Fingerprint {
                repository: "a".repeat(64),
                buffers: "b".repeat(64),
                environment: "c".repeat(64),
            },
            editing: false,
            required_checks: vec![],
        },
        Some(config.root_task.clone()),
        Revision::ZERO,
    )
    .unwrap();
    host.command(
        Command::Transition {
            next: TaskState::Running,
            reason: "explicit fixture start".into(),
            verification: None,
        },
        Some(config.root_task.clone()),
        Revision::ZERO,
    )
    .unwrap();
    for _ in 0..8 {
        if host.maintain_memory().unwrap().caught_up {
            break;
        }
    }
    let server = start_mock_server().await;
    let mut registry = ExtensionRegistryBuilder::new();
    registry.turn_start_admission(Arc::new(host.clone()));
    registry.work_admission(Arc::new(host.clone()));
    let starter = host.clone();
    let cwd = workspace;
    let test = test_codex()
        .with_extensions(Arc::new(registry.build()))
        .with_auth(codex_login::CodexAuth::from_api_key(
            "synthetic-memory-fixture",
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
    host.register(
        thread,
        ThreadBinding {
            scope: Scope {
                workspace: config.workspace.clone(),
                session: config.session.clone(),
                task: config.root_task.clone(),
            },
            agent: AgentId::new(),
            role: RequestRole::Main,
        },
    )
    .unwrap();
    (temp, config, host, owner, test, thread)
}

fn retained_source(
    host: &CanonicalHost,
    config: &Config,
    thread: codex_protocol::ThreadId,
) -> vcp_memory::search_record::SourceBinding {
    use std::collections::BTreeSet;
    use vcp_domain::policy::{Autonomy, Policy};
    std::fs::write(
        std::path::Path::new(&config.binding.root).join("source.rs"),
        b"pub fn retained_answer() -> u32 { 42 }\n",
    )
    .unwrap();
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
                mode: Autonomy::Workspace,
                denials: vec![],
                workspace_roots: BTreeSet::from(
                    [RootId::parse(config.workspace.as_str()).unwrap()],
                ),
                automatic_effects: BTreeSet::new(),
                timeout_ceiling_ms: Units::new(30_000),
                output_ceiling_bytes: ByteCount::new(1024 * 1024),
            },
        },
        None,
        Revision::ZERO,
    )
    .unwrap();
    host.configure_verification(
        thread,
        vcp_lifecycle::foundation::verification::VerificationConfig {
            requirements: vec![],
            rationale: "Retain actual source for publication acceptance".into(),
        },
    )
    .unwrap();
    let state = host.snapshot().unwrap();
    let artifacts: Vec<ArtifactDescriptor> = state
        .records
        .values()
        .filter(|r| r.collection == Collection::Artifact)
        .map(|r| r.decode().unwrap())
        .collect();
    let baseline = artifacts
        .iter()
        .find(|a| a.spec.schema == "verification-baseline/1")
        .unwrap();
    let bytes = host.read_artifact(baseline.spec.id.clone()).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let task: Task = state
        .record(
            Collection::Task,
            config.root_task.as_str(),
            &config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    let mut fingerprint = task.fingerprint;
    fingerprint.repository =
        vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&value["manifest"]).unwrap());
    host.command(
        Command::ObserveFingerprint {
            fingerprint: fingerprint.clone(),
        },
        Some(config.root_task.clone()),
        task.revision,
    )
    .unwrap();
    let source = artifacts
        .iter()
        .find(|a| {
            a.spec.schema == "verification-source/1"
                && a.sha256
                    == vcp_protocol::digest_bytes(b"pub fn retained_answer() -> u32 { 42 }\n")
        })
        .unwrap();
    for _ in 0..16 {
        if host.maintain_memory().unwrap().caught_up {
            break;
        }
    }
    vcp_memory::search_record::SourceBinding {
        manifest: baseline.spec.id.clone(),
        artifact: source.spec.id.clone(),
        root: RootId::parse(config.workspace.as_str()).unwrap(),
        path: "source.rs".into(),
        symbols: vec![],
        fingerprint,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn empty_authorized_inventory_publishes_without_opening_model_assets() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let (temp, config, host, owner, _test, thread) =
            vector_fixture_objective(backend, "Observe an empty workspace").await;
        let private = temp.path().join("private");
        std::fs::create_dir(&private).unwrap();
        let generations = config.canonical_root.join("search-generations");
        let manager = host
            .publication_manager(thread, Arc::new(Publisher::new(&generations).unwrap()))
            .unwrap();
        let outcome = host
            .publish_memory(
                thread,
                manager,
                publication_request(&private, temp.path().join("missing-assets"), false),
            )
            .await
            .unwrap();
        assert_eq!(outcome.status, Status::Published);
        assert!(outcome.resources.is_none());
        assert!(outcome.publication_resources.unwrap().observation_retained);
        assert_eq!(std::fs::read_dir(&private).unwrap().count(), 0);
        let manifest = outcome.manifest.unwrap();
        assert!(manifest.vector_deficits.is_empty());
        let inventory: vcp_memory::search_record::Inventory = serde_json::from_slice(
            &std::fs::read(
                generations
                    .join(manifest.id.as_str())
                    .join("inventory.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(inventory.records.is_empty());
        let state = host.snapshot().unwrap();
        let active: vcp_domain::search::Active = state
            .record(
                Collection::Generation,
                config.workspace.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(active.generation, manifest.id);
        owner.close().await.unwrap();
    }
}
