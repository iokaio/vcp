// SPDX-License-Identifier: Apache-2.0
#![cfg(feature = "qualification")]
use std::collections::BTreeMap;
use vcp_domain::{CommandId, WorkspaceId};
use vcp_protocol::digest_bytes;
use vcp_store::{
    keys::{LocalKeys, RecoveryDirectory},
    vault_crypto::{Limits, Manifest, Object, PrivateStaging, FORMAT},
    vault_publish::{Checkpoint, LocalTrust, Phase, Vault},
};

fn payload(
    bytes: Vec<u8>,
    sequence: u64,
    parent: Option<String>,
    deletion: u64,
) -> (Manifest, BTreeMap<String, Vec<u8>>) {
    let hash = digest_bytes(&bytes);
    (
        Manifest {
            format: FORMAT.into(),
            workspace: WorkspaceId::parse("workspace").unwrap(),
            lineage: "a".repeat(64),
            sequence,
            deletion,
            parent,
            objects: BTreeMap::from([(
                hash.clone(),
                Object {
                    bytes: bytes.len() as u64,
                    sha256: hash.clone(),
                },
            )]),
        },
        BTreeMap::from([(hash, bytes)]),
    )
}

#[cfg(feature = "qualification")]
#[test]
fn staging_storage_full_at_header_body_and_finalization_never_publishes_plaintext() {
    let mut temp = tempfile::tempdir().unwrap();
    temp.disable_cleanup(true);
    println!(
        "retained staging capacity fixture: {}",
        temp.path().display()
    );
    let stage_path = temp.path().join("stage");
    let vault_path = temp.path().join("vault");
    let recovery_path = temp.path().join("recovery");
    for path in [&stage_path, &vault_path, &recovery_path] {
        std::fs::create_dir(path).unwrap();
    }
    let mut staging = PrivateStaging::open(&stage_path, std::slice::from_ref(&vault_path)).unwrap();
    let vault = Vault::open(&vault_path, &[stage_path.clone(), recovery_path.clone()]).unwrap();
    let directory =
        RecoveryDirectory::open(&recovery_path, &[stage_path.clone(), vault_path.clone()]).unwrap();
    let keys = LocalKeys::generate().unwrap();
    let recovery = keys.export_recovery(&directory).unwrap();
    let keys = keys.verify_recovery(&recovery).unwrap();
    let anchor = LocalTrust::enroll(
        &keys,
        WorkspaceId::parse("workspace").unwrap(),
        "a".repeat(64),
        Checkpoint {
            sequence: 0,
            deletion: 3,
            parent: None,
        },
    )
    .unwrap();
    let marker = b"SYNTHETIC-STAGING-CAPACITY-MARKER";
    // Header initialization, streaming body, and buffered finalization are
    // distinct write failures. The machine volume is never filled.
    for (index, (limit, repeats, expected)) in [
        (0, 1, "ciphertext initialization failed"),
        (65_536, 16_000, "ciphertext write failed"),
        (512, 32, "ciphertext finalization failed"),
    ]
    .into_iter()
    .enumerate()
    {
        let (manifest, bodies) = payload(marker.repeat(repeats), 1, None, 3);
        staging.qualify_storage_full_after(Some(limit));
        let failure = anchor.encrypt(
            &keys,
            &staging,
            manifest.clone(),
            bodies.clone(),
            0,
            Limits::default(),
        );
        assert!(
            matches!(failure, Err(vcp_store::Error::Unavailable(reason)) if reason == expected)
        );
        assert_eq!(std::fs::read_dir(&stage_path).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(&vault_path).unwrap().count(), index);
        staging.qualify_storage_full_after(None);
        let mut encrypted = anchor
            .encrypt(
                &keys,
                &staging,
                manifest,
                bodies.clone(),
                0,
                Limits::default(),
            )
            .unwrap();
        let permit = anchor.admit(&encrypted, CommandId::new(), 0).unwrap();
        let receipt = vault
            .publish(&mut encrypted, &permit, &|| false, &|phase| {
                if matches!(phase, Phase::CiphertextBytes(_)) {
                    for file in std::fs::read_dir(&vault_path).unwrap() {
                        let bytes = std::fs::read(file.unwrap().path()).unwrap();
                        assert!(bytes.starts_with(b"age-encryption.org/v1\n"));
                        assert!(!bytes.windows(marker.len()).any(|window| window == marker));
                    }
                }
            })
            .unwrap();
        let recovered = anchor
            .verify_restore(
                &vault_path.join(&receipt.object),
                &recovery,
                Limits::default(),
            )
            .unwrap();
        assert_eq!(recovered.restored().payloads, bodies);
        drop(encrypted);
        assert_eq!(std::fs::read_dir(&stage_path).unwrap().count(), 0);
        let outcome = serde_json::json!({
            "injected_storage_full_after_ciphertext_bytes": limit,
            "observed_error": expected, "failed_partial_cleanup": true,
            "capacity_return_publication_verified": true,
            "authenticated_payload_match": true,
            "published_object": receipt.object, "retained_root": temp.path(),
            "physical_volume_exhaustion": false
        });
        std::fs::write(
            temp.path().join(format!("outcome-{limit}.json")),
            serde_json::to_vec_pretty(&outcome).unwrap(),
        )
        .unwrap();
        println!("staging capacity receipt: injected StorageFull after {limit} ciphertext bytes; {expected}; cleanup, capacity-return publication and decrypt passed");
    }
}
