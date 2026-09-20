// SPDX-License-Identifier: Apache-2.0
use vcp_domain::{
    task::{Objective, TaskState},
    verification::Fingerprint,
    workspace::*,
    *,
};
use vcp_engine::{Engine, HostFacts};
use vcp_memory::{access::Access, runner};
use vcp_protocol::{
    command::{Command, CommandEnvelope},
    event::EventKind,
};
use vcp_store::{BackendKind, Store};

async fn issue(engine: &mut Engine<Store>, scope: &Scope, payload: Command) {
    let access = vcp_engine::Access {
        actor: ActorId::parse("owner").unwrap(),
        workspace: scope.workspace.clone(),
        session: scope.session.clone(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: true,
    };
    let command = CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: scope.workspace.clone(),
        session: scope.session.clone(),
        task: if matches!(&payload, Command::Initialize { .. }) {
            None
        } else {
            Some(scope.task.clone())
        },
        caller: access.actor.clone(),
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        payload,
    };
    engine
        .handle(
            command,
            &access,
            &HostFacts {
                may_execute: true,
                ..HostFacts::inspect(Timestamp::new(100))
            },
        )
        .await
        .unwrap();
}

async fn fixture(
    path: &std::path::Path,
    backend: BackendKind,
) -> (Engine<Store>, Scope, Access, EventId) {
    let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
    let scope = Scope {
        workspace: WorkspaceId::new(),
        session: SessionId::new(),
        task: TaskId::new(),
    };
    issue(
        &mut engine,
        &scope,
        Command::Initialize {
            binding: Binding {
                host: HostId::new(),
                root: "C:/runner-fixture".into(),
                repository: "repo".into(),
                worktree: "main".into(),
                revision: Revision::ZERO,
            },
        },
    )
    .await;
    issue(
        &mut engine,
        &scope,
        Command::CreateTask {
            root: scope.task.clone(),
            parent: None,
            fork_origin: None,
            objective: Objective {
                text: r#"{"memory_preference":{"key":"output","value":"concise"}}"#.into(),
                constraints: vec![],
                acceptance: vec!["retain".into()],
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
    )
    .await;
    let origin = engine
        .store()
        .state()
        .events
        .iter()
        .find(|e| e.event.kind == EventKind::TaskCreated)
        .unwrap()
        .event
        .id
        .clone();
    issue(
        &mut engine,
        &scope,
        Command::Transition {
            next: TaskState::Running,
            reason: "fixture admission".into(),
            verification: None,
        },
    )
    .await;
    let access = Access {
        workspace: scope.workspace.clone(),
        actor: ActorId::parse("owner").unwrap(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        tasks: None,
    };
    (engine, scope, access, origin)
}

use std::{collections::BTreeSet, sync::atomic::AtomicBool};
use vcp_memory::{
    publication::{self, GarbagePolicy, Publisher},
    search_record,
};
fn inventory(store: &Store, access: &Access) -> search_record::Inventory {
    search_record::inventory(
        store,
        access,
        &[],
        &search_record::ChunkerSpec::default(),
        search_record::Limits::default(),
    )
    .unwrap()
}

#[tokio::test]
async fn empty_and_fully_excluded_inventories_acknowledge_exact_coverage() {
    use vcp_store::contract::{CanonicalStore, Collection, Mutation, Record, Transaction};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for excluded_claim in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let canonical = temp.path().join("canonical");
            let (engine, scope, access, _) = fixture(&canonical, backend).await;
            let mut store = engine.into_store();
            if excluded_claim {
                for tick in 0..8 {
                    if runner::step(&mut store, &access, &scope, Timestamp::new(1000 + tick))
                        .await
                        .unwrap()
                        .caught_up
                    {
                        break;
                    }
                }
                assert!(!inventory(&store, &access).records.is_empty());
                let mut workspace: Workspace = store
                    .state()
                    .record(
                        Collection::Workspace,
                        scope.workspace.as_str(),
                        &scope.workspace,
                    )
                    .unwrap()
                    .decode()
                    .unwrap();
                let prior = workspace.revision;
                workspace.revision = prior.next().unwrap();
                workspace.deletion = workspace.deletion.next().unwrap();
                let mask = vcp_audit::history::RetentionMask {
                    schema_version: 1,
                    workspace: scope.workspace.clone(),
                    session: scope.session.clone(),
                    first: SessionSeq::new(1),
                    last: store.state().sequences[&scope.session],
                    artifacts: vec![],
                    deletion: workspace.deletion,
                    reason: "explicit fixture exclusion".into(),
                };
                store
                    .transact(Transaction {
                        id: TransactionId::new(),
                        expected_watermark: store.state().watermark,
                        mutations: vec![
                            Mutation::Put {
                                expected: Some(prior),
                                record: Record::typed(
                                    Collection::Workspace,
                                    workspace.id.as_str(),
                                    workspace.id.clone(),
                                    workspace.revision,
                                    &workspace,
                                )
                                .unwrap(),
                            },
                            Mutation::Put {
                                expected: None,
                                record: Record::typed(
                                    Collection::Tombstone,
                                    "empty-inventory-mask",
                                    workspace.id.clone(),
                                    Revision::ZERO,
                                    &mask,
                                )
                                .unwrap(),
                            },
                        ],
                        events: vec![],
                        command: None,
                    })
                    .await
                    .unwrap();
            }
            let inventory = inventory(&store, &access);
            assert!(inventory.records.is_empty());
            assert_eq!(!inventory.exclusions.is_empty(), excluded_claim);
            let publisher = Publisher::new(&temp.path().join("components")).unwrap();
            let prepared = publisher
                .prepare(
                    publication::capture(&store, &access, &scope, inventory).unwrap(),
                    None,
                    &AtomicBool::new(false),
                    &|_| {},
                )
                .unwrap();
            assert!(prepared.manifest().empty_complete);
            assert!(prepared.manifest().vector_checksum.is_none());
            assert!(prepared.manifest().vector_deficits.is_empty());
            assert_eq!(
                !prepared.manifest().covered_intents.is_empty(),
                excluded_claim
            );
            assert_eq!(
                prepared.manifest().memory_seq > MemorySeq::ZERO,
                excluded_claim
            );
            publisher
                .publish(
                    &mut store,
                    &access,
                    &prepared,
                    Timestamp::new(2000),
                    &|_| {},
                )
                .await
                .unwrap();
            let recovered = publisher.recover(&store, &access).unwrap();
            assert!(!recovered.rebuild_required);
            assert!(recovered.view.unwrap().inventory.records.is_empty());
            for intent in &prepared.manifest().covered_intents {
                let value: vcp_domain::memory::IndexIntent = store
                    .state()
                    .record(Collection::IndexIntent, intent.as_str(), &scope.workspace)
                    .unwrap()
                    .decode()
                    .unwrap();
                assert_eq!(value.status, vcp_domain::memory::IndexStatus::Ready);
            }
            store.close().await.unwrap();
            let store = Store::open(&canonical, backend, &[]).await.unwrap();
            assert!(!publisher.recover(&store, &access).unwrap().rebuild_required);
            store.close().await.unwrap();
        }
    }
}

#[tokio::test]
async fn recovery_never_returns_generations_from_old_access_or_deletion_epochs() {
    use vcp_store::contract::{CanonicalStore, Collection, Mutation, Record, Transaction};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for authority_change in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let (engine, scope, mut access, _) =
                fixture(&temp.path().join("canonical"), backend).await;
            let mut store = engine.into_store();
            let publisher = Publisher::new(&temp.path().join("components")).unwrap();
            let prepared = publisher
                .prepare(
                    publication::capture(&store, &access, &scope, inventory(&store, &access))
                        .unwrap(),
                    None,
                    &AtomicBool::new(false),
                    &|_| {},
                )
                .unwrap();
            publisher
                .publish(
                    &mut store,
                    &access,
                    &prepared,
                    Timestamp::new(2000),
                    &|_| {},
                )
                .await
                .unwrap();
            assert!(publisher.recover(&store, &access).unwrap().view.is_some());
            let mut workspace: Workspace = store
                .state()
                .record(
                    Collection::Workspace,
                    scope.workspace.as_str(),
                    &scope.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            let prior = workspace.revision;
            workspace.revision = prior.next().unwrap();
            if authority_change {
                workspace.authority = workspace.authority.next().unwrap();
                access.authority = workspace.authority;
            } else {
                workspace.deletion = workspace.deletion.next().unwrap();
            }
            store
                .transact(Transaction {
                    id: TransactionId::new(),
                    expected_watermark: store.state().watermark,
                    mutations: vec![Mutation::Put {
                        expected: Some(prior),
                        record: Record::typed(
                            Collection::Workspace,
                            workspace.id.as_str(),
                            workspace.id.clone(),
                            workspace.revision,
                            &workspace,
                        )
                        .unwrap(),
                    }],
                    events: vec![],
                    command: None,
                })
                .await
                .unwrap();
            let recovered = publisher.recover(&store, &access).unwrap();
            assert!(recovered.view.is_none());
            assert!(recovered.rebuild_required);
            assert_eq!(
                recovered.failed_components,
                vec![prepared.manifest().id.clone()]
            );
            store.close().await.unwrap();
        }
    }
}

#[cfg(windows)]
#[tokio::test]
async fn private_build_pins_directory_namespaces_until_writes_finish() {
    use std::sync::atomic::Ordering;
    let temp = tempfile::tempdir().unwrap();
    let (engine, scope, access, _) =
        fixture(&temp.path().join("canonical"), BackendKind::Files).await;
    let store = engine.into_store();
    let root = temp.path().join("components");
    let publisher = Publisher::new(&root).unwrap();
    assert!(std::fs::rename(&root, temp.path().join("moved-root")).is_err());
    let checked = AtomicBool::new(false);
    let prepared = publisher
        .prepare(
            publication::capture(&store, &access, &scope, inventory(&store, &access)).unwrap(),
            None,
            &AtomicBool::new(false),
            &|barrier| {
                if matches!(barrier, publication::Barrier::BeforeLexical) {
                    let directory = std::fs::read_dir(&root)
                        .unwrap()
                        .next()
                        .unwrap()
                        .unwrap()
                        .path();
                    assert!(
                        std::fs::rename(&directory, temp.path().join("moved-generation")).is_err()
                    );
                    assert!(std::fs::rename(
                        directory.join("lexical"),
                        temp.path().join("moved-lexical")
                    )
                    .is_err());
                    checked.store(true, Ordering::Release);
                }
            },
        )
        .unwrap();
    assert!(checked.load(Ordering::Acquire));
    let directory = root.join(prepared.manifest().id.as_str());
    std::fs::rename(&directory, temp.path().join("finished-generation")).unwrap();
    drop(publisher);
    std::fs::rename(&root, temp.path().join("finished-root")).unwrap();
    store.close().await.unwrap();
}

#[tokio::test]
async fn adopted_vectors_allow_resource_receipts_but_reject_changed_scope_or_source() {
    use vcp_memory::{
        embedding::{self, Encoded, Specification},
        vector::Component,
    };
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (engine, scope, access, _) = fixture(&temp.path().join("canonical"), backend).await;
        let mut store = engine.into_store();
        for tick in 0..8 {
            if runner::step(&mut store, &access, &scope, Timestamp::new(1000 + tick))
                .await
                .unwrap()
                .caught_up
            {
                break;
            }
        }
        let original = inventory(&store, &access);
        let expected = embedding::inventory_chunks(&original).unwrap();
        assert!(!expected.is_empty());
        let encode = |rows: Vec<embedding::Chunk>| {
            rows.into_iter()
                .map(|chunk| {
                    let mut vector = vec![0.; 384];
                    vector[0] = 1.;
                    Encoded {
                        identity: chunk.identity,
                        vector,
                    }
                })
                .collect::<Vec<_>>()
        };
        let component = Component::build(
            scope.workspace.clone(),
            Specification::qualified(),
            encode(expected),
            &|| false,
        )
        .unwrap();
        let path = temp.path().join("admitted-vectors.json");
        let checksum = component.save_private(&path, &|| false).unwrap();
        vcp_budget::record_local_resources(
            &mut store,
            vcp_domain::accounting::LocalResources {
                schema_version: 1,
                id: ObservationId::new(),
                scope: scope.clone(),
                agent: AgentId::new(),
                cpu_millis: Units::new(1),
                peak_ram: ByteCount::new(1),
                disk: ByteCount::new(1),
                source: "fixture-resource-observation".into(),
            },
            &vcp_budget::Actor {
                id: access.actor.clone(),
                now: Timestamp::new(2000),
            },
        )
        .await
        .unwrap();
        let current = inventory(&store, &access);
        assert!(current.watermark > original.watermark);
        let publisher = Publisher::new(&temp.path().join("generations")).unwrap();
        let cancel = AtomicBool::new(false);
        let prepared = publisher
            .prepare_with_vectors(
                publication::capture(&store, &access, &scope, current).unwrap(),
                &path,
                &checksum,
                &cancel,
                &|_| {},
            )
            .unwrap();
        assert!(prepared.manifest().vector_deficits.is_empty());
        assert_eq!(
            prepared.manifest().vector_checksum.as_deref(),
            Some(checksum.as_str())
        );
        for changed_scope in [false, true] {
            let rows = embedding::inventory_chunks(&original).unwrap();
            let mut altered = Vec::new();
            for (index, row) in rows.iter().enumerate() {
                let mut other = row.identity.scope.clone();
                if changed_scope {
                    other.task = TaskId::new();
                }
                altered.extend(
                    embedding::chunks(
                        &if changed_scope {
                            row.identity.source.clone()
                        } else {
                            format!("other-source-{index}")
                        },
                        &other,
                        &row.text,
                        &Specification::qualified(),
                    )
                    .unwrap(),
                );
            }
            assert_eq!(
                rows.len(),
                altered.len(),
                "same count cannot establish source compatibility"
            );
            let wrong = Component::build(
                scope.workspace.clone(),
                Specification::qualified(),
                encode(altered),
                &|| false,
            )
            .unwrap();
            let wrong_path = temp.path().join(format!("wrong-{changed_scope}.json"));
            let wrong_checksum = wrong.save_private(&wrong_path, &|| false).unwrap();
            assert!(publisher
                .prepare_with_vectors(
                    publication::capture(&store, &access, &scope, inventory(&store, &access))
                        .unwrap(),
                    &wrong_path,
                    &wrong_checksum,
                    &cancel,
                    &|_| {}
                )
                .is_err());
        }
        let before = store.state().clone();
        publisher
            .publish(
                &mut store,
                &access,
                &prepared,
                Timestamp::new(3000),
                &|_| {},
            )
            .await
            .unwrap();
        assert!(store.state().watermark > before.watermark);
        assert!(publisher
            .recover(&store, &access)
            .unwrap()
            .view
            .unwrap()
            .vector
            .is_some());
    }
}
#[tokio::test]
async fn publication_cas_retry_corrupt_fallback_and_pins_preserve_canonical_evidence() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (engine, scope, access, _) = fixture(&temp.path().join("canonical"), backend).await;
        let mut store = engine.into_store();
        for tick in 0..8 {
            if runner::step(&mut store, &access, &scope, Timestamp::new(1000 + tick))
                .await
                .unwrap()
                .caught_up
            {
                break;
            }
        }
        let publisher = Publisher::new(&temp.path().join("components")).unwrap();
        let cancel = AtomicBool::new(false);
        let first = publisher
            .prepare(
                publication::capture(&store, &access, &scope, inventory(&store, &access)).unwrap(),
                None,
                &cancel,
                &|_| {},
            )
            .unwrap();
        assert!(!first.manifest().vector_deficits.is_empty());
        assert_eq!(first.manifest().memory_seq, MemorySeq::ZERO);
        assert!(first.manifest().covered_intents.is_empty());
        let receipt = publisher
            .publish(&mut store, &access, &first, Timestamp::new(2000), &|_| {})
            .await
            .unwrap();
        let before = store.state().clone();
        assert_eq!(
            publisher
                .publish(&mut store, &access, &first, Timestamp::new(3000), &|_| {})
                .await
                .unwrap(),
            receipt
        );
        assert_eq!(store.state(), &before);
        let pinned = publisher.recover(&store, &access).unwrap();
        assert!(
            pinned.rebuild_required,
            "lexical-only must expose semantic coverage deficit"
        );
        let next = publisher
            .prepare(
                publication::capture(&store, &access, &scope, inventory(&store, &access)).unwrap(),
                None,
                &cancel,
                &|_| {},
            )
            .unwrap();
        let loser = publisher
            .prepare(
                publication::capture(&store, &access, &scope, inventory(&store, &access)).unwrap(),
                None,
                &cancel,
                &|_| {},
            )
            .unwrap();
        publisher
            .publish(&mut store, &access, &next, Timestamp::new(4000), &|_| {})
            .await
            .unwrap();
        assert!(publisher
            .publish(&mut store, &access, &loser, Timestamp::new(4001), &|_| {})
            .await
            .is_err());
        let policy = GarbagePolicy {
            retained: BTreeSet::new(),
            retention_idle: true,
            historical_deletion_allowed: true,
        };
        assert!(!publisher
            .collect(&store, &access, &first.manifest().id, &policy)
            .unwrap());
        assert!(!publisher
            .collect(&store, &access, &next.manifest().id, &policy)
            .unwrap());
        std::fs::write(
            temp.path()
                .join("components")
                .join(next.manifest().id.as_str())
                .join("lexical/vcp-lexical.json"),
            b"corrupt",
        )
        .unwrap();
        let fallback = publisher.recover(&store, &access).unwrap();
        let committed = store.state().transactions[&next.manifest().transaction].clone();
        assert_eq!(
            publisher
                .publish(&mut store, &access, &next, Timestamp::new(5000), &|_| {})
                .await
                .unwrap(),
            committed,
            "lost reply consults canonical receipt even if derived files were corrupted"
        );
        assert!(fallback.rebuild_required);
        assert_eq!(fallback.failed_components, vec![next.manifest().id.clone()]);
        assert_eq!(
            fallback.view.as_ref().unwrap().manifest.id,
            first.manifest().id
        );
        let canonical = store.state().clone();
        drop(fallback);
        drop(pinned);
        let snapshot = store.snapshot().unwrap();
        assert!(!publisher
            .collect(&store, &access, &first.manifest().id, &policy)
            .unwrap());
        drop(snapshot);
        assert!(publisher
            .collect(&store, &access, &first.manifest().id, &policy)
            .unwrap());
        assert_eq!(
            store.state(),
            &canonical,
            "derived cleanup never removes canonical facts"
        );
        assert!(publisher.recover(&store, &access).unwrap().view.is_none());
    }
}

// Native recovery qualification uses a subprocess stopped at every actual file
// boundary. Register/copy this future test only with P5-05 module integration.
#[tokio::test]
async fn publication_child() {
    let Some(path) = std::env::var_os("VCP_PUBLICATION_CHILD") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    let phase = std::env::var("VCP_PUBLICATION_BARRIER").unwrap();
    let backend = if std::env::var("VCP_PUBLICATION_BACKEND").unwrap() == "files" {
        BackendKind::Files
    } else {
        BackendKind::Sqlite
    };
    let (engine, scope, access, _) = fixture(&path.join("canonical"), backend).await;
    let mut store = engine.into_store();
    for tick in 0..8 {
        if runner::step(&mut store, &access, &scope, Timestamp::new(1000 + tick))
            .await
            .unwrap()
            .caught_up
        {
            break;
        }
    }
    let publisher = Publisher::new(&path.join("components")).unwrap();
    let cancel = AtomicBool::new(false);
    let assets =
        std::env::var_os("VCP_MINILM_ASSETS").expect("native gate requires real pinned assets");
    let mut model =
        vcp_memory::embedding::LocalEmbedding::load(std::path::Path::new(&assets), &|| false)
            .unwrap();
    let baseline = publisher
        .prepare(
            publication::capture(&store, &access, &scope, inventory(&store, &access)).unwrap(),
            Some(&mut model),
            &cancel,
            &|_| {},
        )
        .unwrap();
    publisher
        .publish(
            &mut store,
            &access,
            &baseline,
            Timestamp::new(2000),
            &|_| {},
        )
        .await
        .unwrap();
    std::fs::write(
        path.join("baseline.json"),
        serde_json::to_vec(baseline.manifest()).unwrap(),
    )
    .unwrap();
    let stop = |barrier: publication::Barrier| {
        if format!("{barrier:?}") == phase {
            use std::io::Write;
            let mut ready = std::fs::File::create(path.join("ready.pending")).unwrap();
            ready.write_all(b"committed barrier").unwrap();
            ready.sync_all().unwrap();
            drop(ready);
            std::fs::rename(path.join("ready.pending"), path.join("ready")).unwrap();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    };
    let prepared = publisher
        .prepare(
            publication::capture(&store, &access, &scope, inventory(&store, &access)).unwrap(),
            Some(&mut model),
            &cancel,
            &stop,
        )
        .unwrap();
    publisher
        .publish(&mut store, &access, &prepared, Timestamp::new(3000), &stop)
        .await
        .unwrap();
    panic!("requested barrier not reached");
}

struct ChildGuard(std::process::Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[tokio::test]
#[ignore = "native real model gate: VCP_MINILM_ASSETS required"]
async fn process_kill_at_component_manifest_and_activation_boundaries() {
    assert!(std::env::var_os("VCP_MINILM_ASSETS").is_some());
    for name in ["files", "sqlite"] {
        for phase in [
            "BeforeLexical",
            "AfterLexical",
            "BeforeVector",
            "AfterVector",
            "BeforeManifest",
            "AfterManifest",
            "BeforeActivation",
            "AfterActivation",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let log = std::fs::File::create(temp.path().join("child.log")).unwrap();
            let mut child = ChildGuard(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "publication_child", "--nocapture"])
                    .env("VCP_PUBLICATION_CHILD", temp.path())
                    .env("VCP_PUBLICATION_BACKEND", name)
                    .env("VCP_PUBLICATION_BARRIER", phase)
                    .stdout(log.try_clone().unwrap())
                    .stderr(log)
                    .spawn()
                    .unwrap(),
            );
            let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
            while !temp.path().join("ready").exists() {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "early exit: {}",
                    std::fs::read_to_string(temp.path().join("child.log")).unwrap()
                );
                assert!(
                    std::time::Instant::now() < until,
                    "publication barrier timeout"
                );
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            child.0.kill().unwrap();
            assert!(!child.0.wait().unwrap().success());
            let baseline: vcp_domain::search::Generation =
                serde_json::from_slice(&std::fs::read(temp.path().join("baseline.json")).unwrap())
                    .unwrap();
            let access = Access {
                workspace: baseline.scope.workspace.clone(),
                actor: ActorId::parse("owner").unwrap(),
                authority: baseline.authority,
                read: true,
                write: true,
                tasks: None,
            };
            let backend = if name == "files" {
                BackendKind::Files
            } else {
                BackendKind::Sqlite
            };
            let store = Store::open(&temp.path().join("canonical"), backend, &[])
                .await
                .unwrap();
            let publisher = Publisher::new(&temp.path().join("components")).unwrap();
            let recovered = publisher.recover(&store, &access).unwrap();
            assert!(!recovered.rebuild_required);
            let recovered = recovered.view.unwrap();
            assert_eq!(
                recovered.manifest.id == baseline.id,
                phase != "AfterActivation"
            );
            assert!(recovered.vector.is_some());
            for intent in store
                .state()
                .records
                .values()
                .filter(|row| row.value["document_type"] == "vcp_memory_index_intent_v1")
            {
                assert_eq!(intent.value["status"], "ready");
            }
        }
    }
}
// Append to memory publication integration tests (existing fixture()/inventory()).
#[cfg(windows)]
#[tokio::test]
async fn validated_generation_denies_swaps_and_all_root_managers_share_pins() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (engine, scope, access, _) = fixture(&temp.path().join("canonical"), backend).await;
        let mut store = engine.into_store();
        for tick in 0..8 {
            if runner::step(&mut store, &access, &scope, Timestamp::new(1000 + tick))
                .await
                .unwrap()
                .caught_up
            {
                break;
            }
        }
        let root = temp.path().join("generations");
        let publisher = Publisher::new(&root).unwrap();
        let other_root = Publisher::new(&temp.path().join("other-generations")).unwrap();
        let cancel = AtomicBool::new(false);
        let prepared = publisher
            .prepare(
                publication::capture(&store, &access, &scope, inventory(&store, &access)).unwrap(),
                None,
                &cancel,
                &|_| {},
            )
            .unwrap();
        let id = prepared.manifest().id.clone();
        let directory = root.join(id.as_str());
        let path = directory.join("manifest.json");
        let original = std::fs::read(&path).unwrap();
        let validated = publisher.validate_prepared(prepared, &cancel).unwrap();
        assert!(
            std::fs::write(&path, b"replacement").is_err(),
            "validation must hold write-denying handles"
        );
        assert!(
            std::fs::rename(&directory, root.join("renamed")).is_err(),
            "a generation cannot be swapped before activation"
        );
        assert!(other_root
            .activate(
                &mut store,
                &access,
                &validated,
                Timestamp::new(2000),
                &|_| {}
            )
            .await
            .is_err());
        publisher
            .activate(
                &mut store,
                &access,
                &validated,
                Timestamp::new(2000),
                &|_| {},
            )
            .await
            .unwrap();
        let same_root = Publisher::new(&root).unwrap();
        let next = same_root
            .prepare(
                publication::capture(&store, &access, &scope, inventory(&store, &access)).unwrap(),
                None,
                &cancel,
                &|_| {},
            )
            .unwrap();
        same_root
            .publish(&mut store, &access, &next, Timestamp::new(3000), &|_| {})
            .await
            .unwrap();
        let policy = GarbagePolicy {
            retained: BTreeSet::new(),
            retention_idle: true,
            historical_deletion_allowed: true,
        };
        assert!(
            !same_root.collect(&store, &access, &id, &policy).unwrap(),
            "another manager must see the validated reader pin"
        );
        drop(validated);
        std::fs::write(&path, original).unwrap();
        assert!(same_root.collect(&store, &access, &id, &policy).unwrap());
    }
}

#[tokio::test]
async fn cancelled_private_build_and_bounded_overlay_do_not_claim_fresh_semantic_coverage() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (engine, scope, access, _) = fixture(&temp.path().join("canonical"), backend).await;
        let mut store = engine.into_store();
        for tick in 0..8 {
            if runner::step(&mut store, &access, &scope, Timestamp::new(1000 + tick))
                .await
                .unwrap()
                .caught_up
            {
                break;
            }
        }
        let publisher = Publisher::new(&temp.path().join("components")).unwrap();
        let before = store.state().clone();
        let cancel = AtomicBool::new(false);
        let result = publisher.prepare(
            publication::capture(&store, &access, &scope, inventory(&store, &access)).unwrap(),
            None,
            &cancel,
            &|barrier| {
                if matches!(barrier, publication::Barrier::AfterLexical) {
                    cancel.store(true, std::sync::atomic::Ordering::Release);
                }
            },
        );
        assert!(result.is_err());
        assert_eq!(store.state(), &before);
        assert!(publisher.recover(&store, &access).unwrap().rebuild_required);
        cancel.store(false, std::sync::atomic::Ordering::Release);
        let prepared = publisher
            .prepare(
                publication::capture(&store, &access, &scope, inventory(&store, &access)).unwrap(),
                None,
                &cancel,
                &|_| {},
            )
            .unwrap();
        let mut base = prepared.manifest().clone();
        base.canonical_watermark = Watermark::ZERO;
        let fresh = inventory(&store, &access);
        assert!(!fresh.records.is_empty());
        let overlay =
            publication::overlay(&base, &fresh, 1, 1, std::time::Duration::from_millis(50))
                .unwrap();
        assert!(overlay.records.is_empty());
        assert_eq!(overlay.through, base.canonical_watermark);
        assert_eq!(overlay.unsatisfied, Some("overlay limit exceeded"));
        assert!(overlay.lexical_only);
        let overlay = publication::overlay(
            &base,
            &fresh,
            128,
            256 * 1024,
            std::time::Duration::from_millis(50),
        )
        .unwrap();
        assert_eq!(overlay.records, fresh.records);
        assert_eq!(overlay.through, fresh.watermark);
        assert_eq!(overlay.unsatisfied, Some("overlay has no vectors"));
        assert!(publication::overlay(
            &base,
            &fresh,
            129,
            256 * 1024,
            std::time::Duration::from_millis(50)
        )
        .is_err());
        assert_eq!(store.state(), &before);
    }
}
