// SPDX-License-Identifier: Apache-2.0
//! P5-08: current recall and exclusion survive authenticated cross-backend restore.
use std::{collections::BTreeSet, sync::atomic::AtomicBool};
use vcp_domain::{memory::Outcome, task::Objective, verification::Fingerprint, workspace::*, *};
use vcp_engine::{Engine, HostFacts};
use vcp_memory::{
    access::Access,
    publication::{self, Publisher},
    retrieval::*,
    search_record::{self, ChunkerSpec},
};
use vcp_protocol::command::{Command, CommandEnvelope};
use vcp_store::{contract::*, BackendKind, Store};

fn owner(scope: &Scope) -> vcp_engine::Access {
    vcp_engine::Access {
        workspace: scope.workspace.clone(),
        session: scope.session.clone(),
        actor: ActorId::parse("owner").unwrap(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: true,
    }
}
async fn issue(engine: &mut Engine<Store>, scope: &Scope, payload: Command) {
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
        caller: owner(scope).actor,
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        payload,
    };
    engine
        .handle(
            command,
            &owner(scope),
            &HostFacts::inspect(Timestamp::new(100)),
        )
        .await
        .unwrap();
}
async fn fixture(path: &std::path::Path, backend: BackendKind) -> (Store, Scope, Access) {
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
                root: "C:/retrieval-fixture".into(),
                repository: "repo".into(),
                worktree: "main".into(),
                revision: Revision::ZERO,
            },
        },
    )
    .await;
    issue(&mut engine,&scope,Command::CreateTask{root:scope.task.clone(),parent:None,fork_origin:None,
        objective:Objective{text:serde_json::json!({"memory_preference":{"key":"style","value":"retained-preference-only ".repeat(40)}}).to_string(),constraints:vec![],acceptance:vec!["retain".into()],source:EventId::new(),steering:SteeringRevision::ZERO},
        fingerprint:Fingerprint{repository:"a".repeat(64),buffers:"b".repeat(64),environment:"c".repeat(64)},editing:false,required_checks:vec![]}).await;
    let access = Access {
        workspace: scope.workspace.clone(),
        actor: owner(&scope).actor,
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        tasks: None,
    };
    let event = engine.store().state().events.last().unwrap().clone();
    let proposal = vcp_memory::preferences::materialize(engine.store_mut(), &access, &event)
        .await
        .unwrap()
        .unwrap();
    let commit =
        vcp_memory::repository::propose(engine.store_mut(), &access, proposal, Timestamp::new(200))
            .await
            .unwrap();
    assert_eq!(commit.result.resolution.outcome, Outcome::Accepted);
    (engine.into_store(), scope, access)
}
fn request(scope: &Scope) -> Request {
    Request {
        workspace: scope.workspace.clone(),
        tasks: None,
        roots: None,
        paths: None,
        symbols: None,
        text: "style".into(),
        historical: None,
        minimum_sequence: Some(MemorySeq::new(1)),
        timeout_ms: 30_000,
        results: 8,
        tokens: 4096,
        bytes: 8192,
    }
}

async fn publish(
    store: &mut Store,
    scope: &Scope,
    access: &Access,
    root: &std::path::Path,
) -> Publisher {
    let publisher = Publisher::new(root).unwrap();
    let inventory = search_record::inventory(
        store,
        access,
        &[],
        &ChunkerSpec::default(),
        search_record::Limits::default(),
    )
    .unwrap();
    let prepared = publisher
        .prepare(
            publication::capture(store, access, scope, inventory).unwrap(),
            None,
            &AtomicBool::new(false),
            &|_| {},
        )
        .unwrap();
    publisher
        .publish(store, access, &prepared, Timestamp::new(300), &|_| {})
        .await
        .unwrap();
    publisher
}

#[tokio::test]
async fn sourced_recall_exclusion_and_authority_survive_encrypted_cross_backend_restore() {
    use vcp_domain::retention_selector::{Criterion, Selector, Tree};
    use vcp_memory::retention::{self, Action};
    use vcp_store::{
        keys::{LocalKeys, RecoveryDirectory},
        portable_snapshot::Archive,
        restore_stage::Restore,
        vault_crypto::{Limits, Manifest, Object, PrivateStaging, FORMAT},
        vault_publish::{Checkpoint, LocalTrust},
    };
    for (from, to) in [
        (BackendKind::Files, BackendKind::Sqlite),
        (BackendKind::Sqlite, BackendKind::Files),
    ] {
        for excluded in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("source");
            let (mut store, scope, access) = fixture(&root, from).await;
            let publisher = publish(&mut store, &scope, &access, &temp.path().join("search")).await;
            let view = publisher.recover(&store, &access).unwrap().view.unwrap();
            let req = request(&scope);
            let chunker = ChunkerSpec::default();
            let before = query(
                &store,
                &access,
                Some(&view),
                &req,
                &[],
                &chunker,
                None,
                &|| false,
            )
            .unwrap();
            assert_eq!(before.passages.len(), 1);
            let evidence = before.passages[0].evidence.clone();
            assert!(!evidence.is_empty());
            if excluded {
                let preview = retention::preview(
                    &store,
                    &access,
                    Selector {
                        schema_version: 1,
                        tree: Tree::Match(Criterion::Task(scope.task.clone())),
                    },
                    Action::Exclude,
                    Timestamp::new(400),
                )
                .unwrap();
                assert!(!preview.selected.is_empty());
                retention::apply(&mut store, &access, &preview, Timestamp::new(401))
                    .await
                    .unwrap();
                assert!(revalidate_fence(&store, &access, before.fence.as_ref().unwrap()).is_err());
                let denied = query(
                    &store,
                    &access,
                    Some(&view),
                    &req,
                    &[],
                    &chunker,
                    None,
                    &|| false,
                )
                .unwrap();
                assert!(
                    denied.passages.is_empty(),
                    "a pinned old generation must honor exclusion"
                );
            }
            drop(view);
            drop(publisher);
            store.close().await.unwrap();
            let source = Store::open(&root, from, &[]).await.unwrap();
            let original = source.state().clone();
            let archive = Archive::capture(
                &source,
                &source.snapshot().unwrap(),
                &scope.workspace,
                &|| false,
            )
            .unwrap();
            let retained: Vec<_> = evidence
                .iter()
                .map(|id| (id.clone(), archive.retained_artifact(id).unwrap().to_vec()))
                .collect();
            let [stage, recovery, vault, restore_root, roots] =
                ["stage", "recovery", "vault", "restore", "roots"]
                    .map(|name| temp.path().join(name));
            for path in [&stage, &recovery, &vault, &restore_root, &roots] {
                std::fs::create_dir(path).unwrap();
            }
            let forbidden = vec![vault.clone(), root.clone()];
            let keys = LocalKeys::generate().unwrap();
            let copy = keys
                .export_recovery(&RecoveryDirectory::open(&recovery, &forbidden).unwrap())
                .unwrap();
            let keys = keys.verify_recovery(&copy).unwrap();
            let ws: Workspace = source
                .state()
                .record(
                    Collection::Workspace,
                    scope.workspace.as_str(),
                    &scope.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            let trust = LocalTrust::enroll(
                &keys,
                scope.workspace.clone(),
                "d".repeat(64),
                Checkpoint {
                    sequence: 0,
                    deletion: 0,
                    parent: None,
                },
            )
            .unwrap();
            let payloads = archive.payloads().unwrap();
            let manifest = Manifest {
                format: FORMAT.into(),
                workspace: scope.workspace.clone(),
                lineage: "d".repeat(64),
                sequence: 1,
                deletion: ws.deletion.get(),
                parent: None,
                objects: payloads
                    .iter()
                    .map(|(hash, bytes)| {
                        (
                            hash.clone(),
                            Object {
                                bytes: bytes.len() as u64,
                                sha256: hash.clone(),
                            },
                        )
                    })
                    .collect(),
            };
            let mut encrypted = trust
                .encrypt(
                    &keys,
                    &PrivateStaging::open(&stage, &forbidden).unwrap(),
                    manifest,
                    payloads,
                    trust.configuration().revision,
                    Limits::default(),
                )
                .unwrap();
            let operation = CommandId::new();
            let mut restore = Restore::begin(
                &restore_root,
                &forbidden,
                operation.clone(),
                &trust,
                encrypted.sha256().into(),
                encrypted.bytes(),
            )
            .unwrap();
            let object = vault.join("snapshot.age");
            let mut output = std::fs::File::create(&object).unwrap();
            encrypted.copy_ciphertext(&mut output).unwrap();
            output.sync_all().unwrap();
            drop(output);
            let ciphertext = std::fs::read(&object).unwrap();
            assert!(!ciphertext
                .windows(b"retained-preference-only".len())
                .any(|w| w == b"retained-preference-only"));
            restore.acquire(&object, &|| false).unwrap();
            // Resume the staged operation from durable state, then authenticate.
            drop(restore);
            let mut restore = Restore::open(&restore_root, &forbidden).unwrap();
            let proof = restore
                .authenticate(&trust, &copy, Limits::default(), &|| false)
                .unwrap();
            let imported = restore
                .import(
                    &proof,
                    &trust,
                    to,
                    &roots.join(operation.as_str()),
                    access.actor.clone(),
                    Timestamp::new(500),
                    &forbidden,
                    &|| false,
                )
                .await
                .unwrap();
            assert!(!restore.status().search_ready);
            let mut restored = imported.reopen_verified().await.unwrap();
            let target_ws: Workspace = restored
                .state()
                .record(
                    Collection::Workspace,
                    scope.workspace.as_str(),
                    &scope.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(target_ws.trust, Trust::Untrusted);
            assert!(target_ws.authority > ws.authority);
            assert!(search_record::inventory(
                &restored,
                &access,
                &[],
                &chunker,
                search_record::Limits::default()
            )
            .is_err());
            let target_access = Access {
                workspace: scope.workspace.clone(),
                actor: access.actor.clone(),
                authority: target_ws.authority,
                read: true,
                write: true,
                tasks: None,
            };
            for (id, bytes) in &retained {
                let target_archive = Archive::capture(
                    &restored,
                    &restored.snapshot().unwrap(),
                    &scope.workspace,
                    &|| false,
                )
                .unwrap();
                assert_eq!(&target_archive.retained_artifact(id).unwrap(), bytes);
            }
            for (id, receipt) in &original.transactions {
                assert_eq!(restored.state().transactions.get(id), Some(receipt));
            }
            assert_eq!(
                &restored.state().events[..original.events.len()],
                original.events.as_slice()
            );
            let unavailable = query(
                &restored,
                &target_access,
                None,
                &req,
                &[],
                &chunker,
                None,
                &|| false,
            )
            .unwrap();
            assert!(unavailable.rebuild_required && unavailable.passages.is_empty());
            let rebuilt = publish(
                &mut restored,
                &scope,
                &target_access,
                &temp.path().join("target-search"),
            )
            .await;
            let view = rebuilt
                .recover(&restored, &target_access)
                .unwrap()
                .view
                .unwrap();
            let recalled = query(
                &restored,
                &target_access,
                Some(&view),
                &req,
                &[],
                &chunker,
                None,
                &|| false,
            )
            .unwrap();
            if excluded {
                assert!(
                    recalled.passages.is_empty(),
                    "rebuild/restore must not resurrect excluded recall"
                );
            } else {
                assert_eq!(recalled.passages.len(), 1);
                assert_eq!(recalled.passages[0].text, before.passages[0].text);
                assert_eq!(recalled.passages[0].evidence, evidence);
                revalidate_fence(&restored, &target_access, recalled.fence.as_ref().unwrap())
                    .unwrap();
            }
            let narrow = Access {
                tasks: Some(BTreeSet::new()),
                ..target_access
            };
            assert!(query(
                &restored,
                &narrow,
                Some(&view),
                &req,
                &[],
                &chunker,
                None,
                &|| false
            )
            .unwrap()
            .passages
            .is_empty());
            assert_eq!(
                source.state(),
                &original,
                "restore must preserve the original root"
            );
            drop(view);
            restored.close().await.unwrap();
            source.close().await.unwrap();
        }
    }
}
