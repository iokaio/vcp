// SPDX-License-Identifier: Apache-2.0
#![cfg(feature = "qualification")]
mod common;
use common::*;
use vcp_domain::{
    workspace::{Trust, Workspace},
    ActorId, CommandId, Timestamp,
};
use vcp_store::{
    artifact::ArtifactWriter,
    contract::*,
    keys::{LocalKeys, RecoveryDirectory},
    portable_snapshot::Archive,
    restore_stage::{qualify_storage_full_at, Restore, Stage},
    vault_crypto::{Limits, Manifest, Object, PrivateStaging, FORMAT},
    vault_publish::{Checkpoint, LocalTrust},
    BackendKind, Store,
};

#[tokio::test]
async fn restore_private_short_write_keeps_source_and_resumes_exact_import() {
    const BYTES: &[u8] = b"retained acknowledged restore bytes\0\xff survive staging exhaustion";
    for (from, to) in [
        (BackendKind::Files, BackendKind::Sqlite),
        (BackendKind::Sqlite, BackendKind::Files),
    ] {
        for surface in ["replay_base", "artifact_chunk"] {
            let mut temp = tempfile::tempdir().unwrap();
            temp.disable_cleanup(true);
            println!(
                "retained restore capacity fixture {from:?}/{surface}: {}",
                temp.path().display()
            );
            for name in ["stage", "recovery", "restore", "vault", "targets"] {
                std::fs::create_dir(temp.path().join(name)).unwrap();
            }
            let forbidden = vec![temp.path().join("vault")];
            let recovery =
                RecoveryDirectory::open(&temp.path().join("recovery"), &forbidden).unwrap();
            let keys = LocalKeys::generate().unwrap();
            let copy = keys.export_recovery(&recovery).unwrap();
            let keys = keys.verify_recovery(&copy).unwrap();
            let trust = LocalTrust::enroll(
                &keys,
                workspace().id,
                "a".repeat(64),
                Checkpoint {
                    sequence: 0,
                    deletion: 0,
                    parent: None,
                },
            )
            .unwrap();
            let source_root = temp.path().join("source");
            let mut source = Store::open(&source_root, from, &forbidden).await.unwrap();
            let receipt = source.transact(initial()).await.unwrap();
            let spec = spec();
            let mut writer = source.spool().create(spec.clone()).unwrap();
            writer.write_chunk(BYTES).unwrap();
            let descriptor = writer.finalize().unwrap();
            drop(writer);
            source
                .transact(attach(source.state(), descriptor.clone(), None))
                .await
                .unwrap();
            let original = source.state().clone();
            let archive = Archive::capture(
                &source,
                &source.snapshot().unwrap(),
                &workspace().id,
                &|| false,
            )
            .unwrap();
            let payloads = archive.payloads().unwrap();
            let manifest = Manifest {
                format: FORMAT.into(),
                workspace: workspace().id,
                lineage: "a".repeat(64),
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
            let staging = PrivateStaging::open(&temp.path().join("stage"), &forbidden).unwrap();
            let mut ciphertext = trust
                .encrypt(&keys, &staging, manifest, payloads, 0, Limits::default())
                .unwrap();
            let operation = CommandId::new();
            let target = temp.path().join("targets").join(operation.as_str());
            let mut restore = Restore::begin(
                &temp.path().join("restore"),
                &forbidden,
                operation,
                &trust,
                ciphertext.sha256().into(),
                ciphertext.bytes(),
            )
            .unwrap();
            let acquired = temp.path().join("vault/object.age");
            let mut file = std::fs::File::create(&acquired).unwrap();
            ciphertext.copy_ciphertext(&mut file).unwrap();
            file.sync_all().unwrap();
            drop(file);
            restore.acquire(&acquired, &|| false).unwrap();
            let validated = restore
                .authenticate(&trust, &copy, Limits::default(), &|| false)
                .unwrap();
            let fault_path = if surface == "replay_base" {
                target.join("replay-base.json")
            } else {
                target.join("spool").join(spec.id.as_str()).join(format!(
                    "{:020}-{}.chunk",
                    0,
                    vcp_protocol::digest_bytes(BYTES)
                ))
            };
            let fault = qualify_storage_full_at(fault_path.clone(), 17);
            let failed = restore
                .import(
                    &validated,
                    &trust,
                    to,
                    &target,
                    ActorId::parse("developer").unwrap(),
                    Timestamp::new(200),
                    &forbidden,
                    &|| false,
                )
                .await;
            assert!(
                matches!(failed, Err(vcp_store::Error::Io(ref error)) if error.kind() == std::io::ErrorKind::StorageFull)
            );
            drop(fault);
            assert_eq!(restore.status().stage, Stage::Importing);
            assert!(!restore.status().canonical_imported);
            assert!(!restore.status().search_ready);
            assert!(!fault_path.exists());
            assert!(!target.join("format.json").exists());
            let partials: Vec<_> = std::fs::read_dir(fault_path.parent().unwrap())
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "partial"))
                .collect();
            assert_eq!(partials.len(), 1);
            let partial_bytes = std::fs::read(&partials[0]).unwrap();
            assert_eq!(partial_bytes.len(), 17);
            if surface == "artifact_chunk" {
                assert_eq!(partial_bytes, &BYTES[..17]);
            }
            assert_eq!(source.state(), &original);
            source.close().await.unwrap();
            let mut source = Store::open(&source_root, from, &forbidden).await.unwrap();
            assert_eq!(source.transact(initial()).await.unwrap(), receipt);
            assert_eq!(source.state(), &original);
            let mut source_bytes = Vec::new();
            source.spool().read(&descriptor, &mut source_bytes).unwrap();
            assert_eq!(source_bytes, BYTES);
            source.close().await.unwrap();
            drop(restore);
            let mut restore = Restore::open(&temp.path().join("restore"), &forbidden).unwrap();
            let imported = restore
                .import(
                    &validated,
                    &trust,
                    to,
                    &target,
                    ActorId::parse("developer").unwrap(),
                    Timestamp::new(200),
                    &forbidden,
                    &|| false,
                )
                .await
                .unwrap();
            assert_eq!(restore.status().stage, Stage::Imported);
            let recovered = imported.reopen_verified().await.unwrap();
            assert_eq!(
                recovered.state().watermark.get(),
                original.watermark.get() + 1
            );
            let workspace: Workspace = recovered
                .state()
                .record(
                    Collection::Workspace,
                    workspace().id.as_str(),
                    &workspace().id,
                )
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(workspace.trust, Trust::Untrusted);
            assert_eq!(workspace.authority.get(), 1);
            let mut recovered_bytes = Vec::new();
            recovered
                .spool()
                .read(&descriptor, &mut recovered_bytes)
                .unwrap();
            assert_eq!(recovered_bytes, BYTES);
            let exact = recovered.state().clone();
            recovered.close().await.unwrap();
            drop(imported);
            let retried = restore
                .import(
                    &validated,
                    &trust,
                    to,
                    &target,
                    ActorId::parse("developer").unwrap(),
                    Timestamp::new(200),
                    &forbidden,
                    &|| false,
                )
                .await
                .unwrap();
            let reopened = retried.reopen_verified().await.unwrap();
            assert_eq!(reopened.state(), &exact);
            reopened.close().await.unwrap();
            assert_eq!(std::fs::read(&partials[0]).unwrap(), partial_bytes);
            std::fs::write(
                temp.path().join("outcome.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "source_backend": format!("{from:?}"), "target_backend": format!("{to:?}"),
                    "surface": surface, "injected_error": "StorageFull", "private_prefix_bytes": 17,
                    "source_unchanged": true, "failed_import_acknowledged": false,
                    "retained_partial": partials[0], "exact_retry": true, "authority_resets": 1,
                    "root_activation_performed": false, "physical_volume_exhaustion": false
                }))
                .unwrap(),
            )
            .unwrap();
        }
    }
}
