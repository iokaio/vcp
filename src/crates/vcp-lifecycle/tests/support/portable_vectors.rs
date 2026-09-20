// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{collections::BTreeMap, sync::atomic::AtomicBool};
use vcp_lifecycle::foundation::{memory_publication, memory_vectors, restore_search};
use vcp_memory::publication::Publisher;
use vcp_store::{contract::*, Store};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "explicit real MiniLM retained-vector encrypted restore qualification"]
async fn encrypted_restore_rebuilds_compatible_retained_vectors_without_loading_another_model() {
    use vcp_store::{
        keys::{LocalKeys, RecoveryDirectory},
        portable_snapshot::Archive,
        restore_stage::Restore,
        snapshot_inputs::Inputs,
        vault_crypto::{Limits, Manifest, Object, PrivateStaging, FORMAT},
        vault_publish::{Checkpoint, LocalTrust},
    };
    let assets = std::path::PathBuf::from(
        std::env::var_os("VCP_MINILM_ASSETS")
            .expect("pinned assets required for source qualification"),
    );
    for (from, to) in [
        (BackendKind::Files, BackendKind::Sqlite),
        (BackendKind::Sqlite, BackendKind::Files),
    ] {
        let (temp, config, host, owner, test, thread) =
            super::memory_publication::vector_fixture(from).await;
        let source = super::memory_publication::retained_source(&host, &config, thread);
        let private = temp.path().join("private");
        std::fs::create_dir(&private).unwrap();
        let publisher =
            Arc::new(Publisher::new(&config.canonical_root.join("search-generations")).unwrap());
        let manager = host.publication_manager(thread, publisher.clone()).unwrap();
        let published = host
            .publish_memory(
                thread,
                manager,
                memory_publication::Request {
                    vectors: memory_vectors::Request {
                        assets: assets.clone(),
                        private_root: private,
                        sources: vec![source],
                        chunker: Default::default(),
                        cancelled: Arc::new(AtomicBool::new(false)),
                    },
                    allow_lexical_only: false,
                },
            )
            .await
            .unwrap();
        assert_eq!(published.status, memory_publication::Status::Published);
        let (opened, _) = host
            .open_memory_view(thread, publisher.clone(), Arc::new(AtomicBool::new(false)))
            .await
            .unwrap();
        let view = opened.view.unwrap();
        assert!(view
            .inventory
            .records
            .iter()
            .any(|row| row.kind == vcp_memory::search_record::SearchKind::Source));
        let original_vectors: BTreeMap<_, _> = view
            .vector
            .as_ref()
            .unwrap()
            .rows()
            .iter()
            .map(|row| (row.identity.id.clone(), row.clone()))
            .collect();
        let generation = host
            .capture_backup_generation(publisher.clone(), view, Arc::new(AtomicBool::new(false)))
            .await
            .unwrap();
        let vector_artifact = generation.vectors.clone().unwrap();
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        drop(test);
        drop(host);
        drop(publisher);
        let source = Store::open(&config.canonical_root, from, &[])
            .await
            .unwrap();
        let [recovery_path, stage, vault, restore_path, roots] =
            ["recovery", "stage", "vault", "restore", "roots"].map(|name| temp.path().join(name));
        for path in [&recovery_path, &stage, &vault, &restore_path, &roots] {
            std::fs::create_dir(path).unwrap();
        }
        let forbidden = vec![vault.clone(), config.canonical_root.clone()];
        let keys = LocalKeys::generate().unwrap();
        let copy = keys
            .export_recovery(&RecoveryDirectory::open(&recovery_path, &forbidden).unwrap())
            .unwrap();
        let keys = keys.verify_recovery(&copy).unwrap();
        let trust = LocalTrust::enroll(
            &keys,
            config.workspace.clone(),
            "d".repeat(64),
            Checkpoint {
                sequence: 0,
                deletion: 0,
                parent: None,
            },
        )
        .unwrap();
        let archive = Archive::capture_with_inputs(
            &source,
            &source.snapshot().unwrap(),
            &config.workspace,
            &Inputs {
                checkpoint: None,
                generations: vec![generation],
            },
            &|| false,
        )
        .unwrap();
        let payloads = archive.payloads().unwrap();
        let manifest = Manifest {
            format: FORMAT.into(),
            workspace: config.workspace.clone(),
            lineage: "d".repeat(64),
            sequence: 1,
            deletion: 0,
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
        let object = vault.join("opaque.age");
        let mut output = std::fs::File::create(&object).unwrap();
        encrypted.copy_ciphertext(&mut output).unwrap();
        output.sync_all().unwrap();
        drop(output);
        let operation = CommandId::new();
        let target = roots.join(operation.as_str());
        let mut restore = Restore::begin(
            &restore_path,
            &forbidden,
            operation,
            &trust,
            encrypted.sha256().into(),
            encrypted.bytes(),
        )
        .unwrap();
        restore.acquire(&object, &|| false).unwrap();
        let validated = restore
            .authenticate(&trust, &copy, Limits::default(), &|| false)
            .unwrap();
        let imported = restore
            .import(
                &validated,
                &trust,
                to,
                &target,
                config.actor.clone(),
                Timestamp::new(3000),
                &forbidden,
                &|| false,
            )
            .await
            .unwrap();
        let restored = imported.reopen_verified().await.unwrap();
        let workspace: Workspace = restored
            .state()
            .record(
                Collection::Workspace,
                config.workspace.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(workspace.trust, Trust::Untrusted);
        let mut target_config = config.clone();
        target_config.backend = to;
        target_config.canonical_root = target;
        target_config.binding = workspace.binding.clone();
        restored.close().await.unwrap();
        // This hook has no assets/model argument. Only preserved source vectors
        // can make the new generation semantic-ready on the destination.
        let ready =
            restore_search::rebuild_after_restore(&target_config, Arc::new(AtomicBool::new(false)))
                .await
                .unwrap();
        assert!(
            ready.rebuilt && ready.lexical_ready && !ready.semantic_pending,
            "{ready:?}"
        );
        assert!(ready.source_reauthorization_required);
        let restored = Store::open(&target_config.canonical_root, to, &[])
            .await
            .unwrap();
        let access = vcp_memory::access::Access {
            workspace: config.workspace.clone(),
            actor: config.actor.clone(),
            authority: workspace.authority,
            read: true,
            write: false,
            tasks: None,
        };
        let publisher =
            Publisher::open_existing(&restored.canonical_anchor().join("search-generations"))
                .unwrap();
        let view = publisher.recover(&restored, &access).unwrap().view.unwrap();
        let children = std::fs::read_dir(publisher.storage_root())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            children,
            vec![view.manifest.id.to_string()],
            "owned vector staging must be removed after adoption"
        );
        let vectors = view.vector.as_ref().unwrap();
        assert!(!vectors.rows().is_empty());
        assert!(
            vectors.rows().len() < original_vectors.len(),
            "stale native sources must be removed"
        );
        for row in vectors.rows() {
            assert_eq!(original_vectors.get(&row.identity.id), Some(row));
        }
        assert!(view
            .inventory
            .records
            .iter()
            .all(|row| row.kind != vcp_memory::search_record::SearchKind::Source));
        assert!(restored.state().records.values().any(|row| row.collection
            == Collection::LocalResources
            && row.value["source"]
                .as_str()
                .is_some_and(|s| s.contains(vector_artifact.as_str()))));
        assert!(!restored
            .state()
            .records
            .values()
            .any(|row| row.collection == Collection::Attempt));
        let watermark = restored.state().watermark;
        drop(view);
        drop(publisher);
        restored.close().await.unwrap();
        let repeated =
            restore_search::rebuild_after_restore(&target_config, Arc::new(AtomicBool::new(false)))
                .await
                .unwrap();
        assert!(!repeated.rebuilt && !repeated.semantic_pending);
        assert_eq!(repeated.generation, ready.generation);
        let restored = Store::open(&target_config.canonical_root, to, &[])
            .await
            .unwrap();
        assert_eq!(restored.state().watermark, watermark);
        restored.close().await.unwrap();
        source.close().await.unwrap();
    }
}
