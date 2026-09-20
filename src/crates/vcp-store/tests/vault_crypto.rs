// SPDX-License-Identifier: Apache-2.0
//! Production crypto seam tests; all identities are disposable test material.
use age::{secrecy::ExposeSecret, x25519::Identity};
use ed25519_dalek::SigningKey;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
use vcp_domain::WorkspaceId;
use vcp_store::vault_crypto::{self, Limits, Manifest, Object, PrivateStaging, Trust, FORMAT};

fn fixture() -> (Manifest, BTreeMap<String, Vec<u8>>) {
    let bytes = b"public synthetic history and unsettled-liability fixture".to_vec();
    let hash = vcp_protocol::digest_bytes(&bytes);
    (
        Manifest {
            format: FORMAT.into(),
            workspace: WorkspaceId::parse("workspace").unwrap(),
            lineage: "a".repeat(64),
            sequence: 2,
            deletion: 3,
            parent: Some("b".repeat(64)),
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
fn trust(writer: &SigningKey) -> Trust {
    Trust {
        workspace: WorkspaceId::parse("workspace").unwrap(),
        lineage: "a".repeat(64),
        writers: BTreeSet::from([writer.verifying_key().to_bytes()]),
        minimum_sequence: 1,
        minimum_deletion: 3,
        parent: Some("b".repeat(64)),
    }
}
fn staging(root: &Path) -> PrivateStaging {
    let stage = root.join("stage");
    let vault = root.join("vault");
    std::fs::create_dir_all(&stage).unwrap();
    std::fs::create_dir_all(&vault).unwrap();
    PrivateStaging::open(&stage, &[vault]).unwrap()
}
fn encrypted(root: &Path, identity: &Identity, writer: &SigningKey) -> PathBuf {
    let (manifest, payloads) = fixture();
    let mut finalized = vault_crypto::encrypt(
        &staging(root),
        &identity.to_public(),
        writer,
        manifest,
        payloads,
        Limits::default(),
    )
    .unwrap();
    assert_eq!(finalized.format(), FORMAT);
    let mut bytes = Vec::new();
    finalized.copy_ciphertext(&mut bytes).unwrap();
    assert_eq!(finalized.bytes(), bytes.len() as u64);
    assert_eq!(finalized.sha256(), vcp_protocol::digest_bytes(&bytes));
    assert!(bytes.starts_with(b"age-encryption.org/v1\n"));
    assert!(!bytes
        .windows(b"synthetic history".len())
        .any(|value| value == b"synthetic history"));
    let path = root.join("hydrated.age");
    std::fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn independent_recovery_roundtrip_returns_only_authenticated_expected_lineage() {
    let root = tempfile::tempdir().unwrap();
    let identity = Identity::generate();
    let writer = SigningKey::from_bytes(&[42; 32]);
    let recovery = root.path().join("separate-recovery.txt");
    std::fs::write(&recovery, identity.to_string().expose_secret()).unwrap();
    let path = encrypted(root.path(), &identity, &writer);
    let recovered: Identity = std::fs::read_to_string(recovery).unwrap().parse().unwrap();
    let restored =
        vault_crypto::decrypt(&path, &recovered, &trust(&writer), Limits::default()).unwrap();
    let (manifest, payloads) = fixture();
    assert_eq!(restored.manifest, manifest);
    assert_eq!(restored.payloads, payloads);
    assert_eq!(restored.writer, writer.verifying_key().to_bytes());
    for fault in [
        "workspace",
        "lineage",
        "sequence",
        "deletion",
        "parent",
        "writer",
    ] {
        let mut expected = trust(&writer);
        match fault {
            "workspace" => expected.workspace = WorkspaceId::parse("other-workspace").unwrap(),
            "lineage" => expected.lineage = "c".repeat(64),
            "sequence" => expected.minimum_sequence = 2,
            "deletion" => expected.minimum_deletion = 4,
            "parent" => expected.parent = None,
            "writer" => {
                expected.writers =
                    BTreeSet::from([SigningKey::from_bytes(&[44; 32]).verifying_key().to_bytes()])
            }
            _ => unreachable!(),
        }
        assert!(
            vault_crypto::decrypt(&path, &recovered, &expected, Limits::default()).is_err(),
            "accepted {fault}"
        );
    }
}

#[test]
fn wrong_key_header_payload_tamper_and_truncated_final_chunk_reject() {
    let root = tempfile::tempdir().unwrap();
    let identity = Identity::generate();
    let writer = SigningKey::from_bytes(&[42; 32]);
    let path = encrypted(root.path(), &identity, &writer);
    let original = std::fs::read(&path).unwrap();
    assert!(vault_crypto::decrypt(
        &path,
        &Identity::generate(),
        &trust(&writer),
        Limits::default()
    )
    .is_err());
    for fault in ["header", "payload", "final_chunk"] {
        let mut bytes = original.clone();
        match fault {
            "header" => bytes[0] ^= 1,
            "payload" => {
                let n = bytes.len() - 20;
                bytes[n] ^= 1;
            }
            "final_chunk" => {
                bytes.pop();
            }
            _ => unreachable!(),
        }
        std::fs::write(&path, bytes).unwrap();
        assert!(
            vault_crypto::decrypt(&path, &identity, &trust(&writer), Limits::default()).is_err(),
            "accepted {fault}"
        );
    }
}

#[test]
fn public_recipient_cannot_enroll_writer_or_bypass_signature_or_payload_inventory() {
    let root = tempfile::tempdir().unwrap();
    let identity = Identity::generate();
    let owner = SigningKey::from_bytes(&[42; 32]);
    let stranger = SigningKey::from_bytes(&[43; 32]);
    let path = encrypted(root.path(), &identity, &stranger);
    assert!(vault_crypto::decrypt(&path, &identity, &trust(&owner), Limits::default()).is_err());
    let good = encrypted(root.path(), &identity, &owner);
    let plaintext = age::decrypt(&identity, &std::fs::read(&good).unwrap()).unwrap();
    let original: serde_json::Value = serde_json::from_slice(&plaintext).unwrap();
    for fault in ["signature", "payload", "duplicate_payload"] {
        let mut value = original.clone();
        match fault {
            "signature" => value["signature"] = serde_json::json!(vec![0u8; 64]),
            "payload" => {
                let key = value["payloads"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .next()
                    .unwrap()
                    .clone();
                value["payloads"][key] = serde_json::json!([1, 2, 3]);
            }
            "duplicate_payload" => {
                value["payloads"]["0".repeat(64)] = serde_json::json!([1]);
            }
            _ => unreachable!(),
        }
        let forged = age::encrypt(
            &identity.to_public(),
            &vcp_protocol::canonical_bytes(&value).unwrap(),
        )
        .unwrap();
        std::fs::write(&good, forged).unwrap();
        assert!(
            vault_crypto::decrypt(&good, &identity, &trust(&owner), Limits::default()).is_err(),
            "accepted {fault}"
        );
    }
}

#[test]
fn explicit_limits_and_opaque_names_fail_closed_without_plaintext_staging() {
    let root = tempfile::tempdir().unwrap();
    let identity = Identity::generate();
    let writer = SigningKey::from_bytes(&[42; 32]);
    let stage = staging(root.path());
    let (manifest, payloads) = fixture();
    for limits in [
        Limits {
            payload_bytes: 1,
            ..Limits::default()
        },
        Limits {
            plaintext_bytes: 1,
            payload_bytes: 1,
            ..Limits::default()
        },
        Limits {
            ciphertext_bytes: 1,
            ..Limits::default()
        },
        Limits {
            objects: 0,
            ..Limits::default()
        },
    ] {
        assert!(vault_crypto::encrypt(
            &stage,
            &identity.to_public(),
            &writer,
            manifest.clone(),
            payloads.clone(),
            limits
        )
        .is_err());
    }
    for name in ["../escape", "C:relative", "CON", "name:stream", "/absolute"] {
        let mut bad = manifest.clone();
        let object = bad.objects.values().next().unwrap().clone();
        bad.objects = BTreeMap::from([(name.into(), object)]);
        let value = payloads.values().next().unwrap().clone();
        assert!(vault_crypto::encrypt(
            &stage,
            &identity.to_public(),
            &writer,
            bad,
            BTreeMap::from([(name.into(), value)]),
            Limits::default()
        )
        .is_err());
    }
    let path = encrypted(root.path(), &identity, &writer);
    assert!(vault_crypto::decrypt(
        &path,
        &identity,
        &trust(&writer),
        Limits {
            ciphertext_bytes: 1,
            ..Limits::default()
        }
    )
    .is_err());
    assert!(vault_crypto::decrypt(
        &path,
        &identity,
        &trust(&writer),
        Limits {
            plaintext_bytes: 16,
            payload_bytes: 1,
            ..Limits::default()
        }
    )
    .is_err());
    assert!(
        PrivateStaging::open(&root.path().join("vault"), &[root.path().join("vault")]).is_err()
    );
    assert!(PrivateStaging::open(&root.path().join("stage"), &[]).is_err());
    for entry in std::fs::read_dir(root.path().join("stage")).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        assert!(bytes.is_empty() || bytes.starts_with(b"age-encryption.org/v1\n"));
        assert!(!bytes
            .windows(b"synthetic history".len())
            .any(|value| value == b"synthetic history"));
    }
}
