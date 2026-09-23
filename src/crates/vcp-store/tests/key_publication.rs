// SPDX-License-Identifier: Apache-2.0
//! Disposable synthetic secrets live only in native temporary directories.
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    path::PathBuf,
};
use vcp_domain::{CommandId, WorkspaceId};
use vcp_protocol::{canonical_bytes, digest_bytes};
use vcp_store::{
    keys::{LocalKeys, RecoveryCopy, RecoveryDirectory, VerifiedKeys},
    vault_crypto::{Limits, Manifest, Object, PrivateStaging, FORMAT},
    vault_publish::{Checkpoint, FailureCode, LocalTrust, Phase, Vault},
};

struct Fixture {
    staging: PrivateStaging,
    vault: Vault,
    recovery: RecoveryDirectory,
    stage_path: PathBuf,
    vault_path: PathBuf,
    recovery_path: PathBuf,
    workspace: PathBuf,
    // Directory capability handles must close before TempDir removes ancestors.
    _temp: tempfile::TempDir,
}
fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let stage = temp.path().join("local-stage");
    let vault = temp.path().join("sync-vault");
    let recovery = temp.path().join("independent-recovery");
    let workspace = temp.path().join("workspace");
    for path in [&stage, &vault, &recovery, &workspace] {
        std::fs::create_dir(path).unwrap();
    }
    Fixture {
        staging: PrivateStaging::open(
            &stage,
            &[vault.clone(), workspace.clone(), recovery.clone()],
        )
        .unwrap(),
        vault: Vault::open(
            &vault,
            &[stage.clone(), recovery.clone(), workspace.clone()],
        )
        .unwrap(),
        recovery: RecoveryDirectory::open(
            &recovery,
            &[vault.clone(), stage.clone(), workspace.clone()],
        )
        .unwrap(),
        stage_path: stage,
        vault_path: vault,
        recovery_path: recovery,
        workspace,
        _temp: temp,
    }
}
fn keys(f: &Fixture) -> (VerifiedKeys, RecoveryCopy) {
    let keys = LocalKeys::generate().unwrap();
    let copy = keys.export_recovery(&f.recovery).unwrap();
    (keys.verify_recovery(&copy).unwrap(), copy)
}
fn trust(keys: &VerifiedKeys) -> LocalTrust {
    LocalTrust::enroll(
        keys,
        WorkspaceId::parse("workspace").unwrap(),
        "a".repeat(64),
        Checkpoint {
            sequence: 0,
            deletion: 3,
            parent: None,
        },
    )
    .unwrap()
}
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
#[test]
fn only_an_independently_read_recovery_copy_enables_publication_and_public_config_contains_no_secret(
) {
    let f = fixture();
    let original = LocalKeys::generate().unwrap();
    let public = original.public().unwrap();
    let copy = original.export_recovery(&f.recovery).unwrap();
    let different = LocalKeys::generate().unwrap();
    assert!(different.verify_recovery(&copy).is_err());
    let exported = std::fs::read_to_string(copy.path()).unwrap();
    let imported = LocalKeys::import(&copy).unwrap();
    assert_eq!(imported.public().unwrap(), public);
    // Corrupt the independently saved copy after both process keys are loaded.
    // Verification must reread it, not merely test the still-cached identity.
    std::fs::write(copy.path(), "private-canary-do-not-log").unwrap();
    let error = imported.verify_recovery(&copy).err().unwrap();
    assert!(!error.to_string().contains("private-canary-do-not-log"));
    std::fs::write(copy.path(), &exported).unwrap();
    let verified = original.verify_recovery(&copy).unwrap();
    let enrolled = trust(&verified);
    let config = serde_json::to_string(enrolled.configuration()).unwrap();
    assert!(!config.contains("AGE-SECRET-KEY"));
    assert!(!config.contains(exported.lines().nth(1).unwrap()));
    assert!(!config.contains(
        exported
            .lines()
            .nth(2)
            .unwrap()
            .split_whitespace()
            .last()
            .unwrap()
    ));
    assert!(enrolled.configuration().recovery_verified);
    assert!(!enrolled.configuration().global_newest_known);
    assert!(RecoveryDirectory::open(&f.vault_path, std::slice::from_ref(&f.vault_path)).is_err());
    assert!(RecoveryDirectory::open(&f.workspace, std::slice::from_ref(&f.workspace)).is_err());
    assert!(f.recovery.open_copy("../escape").is_err());
}

#[test]
fn rotation_preserves_old_recovery_but_revocation_replay_forks_and_deletion_floors_reject() {
    let f = fixture();
    let (first, first_copy) = keys(&f);
    let mut anchor = trust(&first);
    let (manifest, bodies) = payload(b"first canonical history".to_vec(), 1, None, 3);
    let parent = digest_bytes(&canonical_bytes(&manifest).unwrap());
    let mut encrypted = anchor
        .encrypt(&first, &f.staging, manifest, bodies, 0, Limits::default())
        .unwrap();
    let admitted = anchor.admit(&encrypted, CommandId::new(), 0).unwrap();
    let published = f
        .vault
        .publish(&mut encrypted, &admitted, &|| false, &|_| {})
        .unwrap();
    let old_path = f.vault_path.join(&published.object);
    let restored = anchor
        .verify_restore(&old_path, &first_copy, Limits::default())
        .unwrap();
    anchor.advance_after_restore(&restored, 0).unwrap();
    assert!(anchor
        .verify_restore(&old_path, &first_copy, Limits::default())
        .is_err());
    let (second, second_copy) = keys(&f);
    anchor.rotate(&second, 1).unwrap();
    assert_eq!(anchor.configuration().revision, 2);
    let (manifest, bodies) = payload(b"next canonical history".to_vec(), 2, Some(parent), 3);
    let mut encrypted = anchor
        .encrypt(
            &second,
            &f.staging,
            manifest.clone(),
            bodies.clone(),
            2,
            Limits::default(),
        )
        .unwrap();
    let admitted = anchor.admit(&encrypted, CommandId::new(), 2).unwrap();
    let published = f
        .vault
        .publish(&mut encrypted, &admitted, &|| false, &|_| {})
        .unwrap();
    let path = f.vault_path.join(&published.object);
    assert!(anchor
        .verify_restore(&path, &first_copy, Limits::default())
        .is_err());
    assert!(anchor
        .verify_restore(&path, &second_copy, Limits::default())
        .is_ok());
    let mut fork = manifest.clone();
    fork.parent = Some("d".repeat(64));
    assert!(anchor
        .encrypt(
            &second,
            &f.staging,
            fork,
            bodies.clone(),
            2,
            Limits::default()
        )
        .is_err());
    anchor.raise_deletion_floor(4, 2).unwrap();
    assert!(anchor.admit(&encrypted, CommandId::new(), 2).is_err());
    assert!(anchor.admit(&encrypted, CommandId::new(), 3).is_err());
    assert!(anchor
        .verify_restore(&path, &second_copy, Limits::default())
        .is_err());
    assert!(anchor.raise_deletion_floor(3, 3).is_err());
    let mut current = manifest;
    current.deletion = 4;
    let encrypted = anchor
        .encrypt(&second, &f.staging, current, bodies, 3, Limits::default())
        .unwrap();
    anchor.revoke_writer(second.public().writer, 3).unwrap();
    assert!(anchor.admit(&encrypted, CommandId::new(), 4).is_err());
    assert_eq!(std::fs::read_dir(&f.vault_path).unwrap().count(), 2);
    assert!(old_path.is_file());
}

#[test]
fn independently_advanced_offline_heads_reject_divergence_without_inventing_global_freshness() {
    let mut f = fixture();
    f._temp.disable_cleanup(true);
    println!(
        "P8-02 offline divergence evidence: {}",
        f._temp.path().display()
    );
    let (keys, copy) = keys(&f);
    let mut left = trust(&keys);
    let mut right = trust(&keys);
    let mut paths = Vec::new();
    for (anchor, marker) in [(&mut left, "offline-left"), (&mut right, "offline-right")] {
        let (manifest, bodies) = payload(marker.as_bytes().to_vec(), 1, None, 3);
        let mut encrypted = anchor
            .encrypt(&keys, &f.staging, manifest, bodies, 0, Limits::default())
            .unwrap();
        let admitted = anchor.admit(&encrypted, CommandId::new(), 0).unwrap();
        let published = f
            .vault
            .publish(&mut encrypted, &admitted, &|| false, &|_| {})
            .unwrap();
        let path = f.vault_path.join(&published.object);
        let verified = anchor
            .verify_restore(&path, &copy, Limits::default())
            .unwrap();
        anchor.advance_after_restore(&verified, 0).unwrap();
        paths.push(path);
    }
    assert_ne!(
        left.configuration().checkpoint.parent,
        right.configuration().checkpoint.parent
    );
    assert!(left
        .verify_restore(&paths[1], &copy, Limits::default())
        .is_err());
    assert!(right
        .verify_restore(&paths[0], &copy, Limits::default())
        .is_err());
    // A higher sequence alone cannot overcome a known divergent parent.
    let (manifest, bodies) = payload(
        b"right-descendant".to_vec(),
        2,
        right.configuration().checkpoint.parent.clone(),
        3,
    );
    let mut encrypted = right
        .encrypt(
            &keys,
            &f.staging,
            manifest,
            bodies,
            right.configuration().revision,
            Limits::default(),
        )
        .unwrap();
    let admitted = right
        .admit(&encrypted, CommandId::new(), right.configuration().revision)
        .unwrap();
    let published = f
        .vault
        .publish(&mut encrypted, &admitted, &|| false, &|_| {})
        .unwrap();
    let newer = f.vault_path.join(&published.object);
    let before = canonical_bytes(left.configuration()).unwrap();
    assert!(left
        .verify_restore(&newer, &copy, Limits::default())
        .is_err());
    assert_eq!(canonical_bytes(left.configuration()).unwrap(), before);
    assert!(right
        .verify_restore(&newer, &copy, Limits::default())
        .is_ok());
    let fresh = trust(&keys);
    assert!(!fresh.configuration().global_newest_known);
    assert!(fresh
        .verify_restore(&paths[0], &copy, Limits::default())
        .is_ok());
    assert!(fresh
        .verify_restore(&paths[1], &copy, Limits::default())
        .is_ok());
    std::fs::write(
        f._temp.path().join("offline-result.json"),
        canonical_bytes(&serde_json::json!({
            "pass": true, "same_sequence_divergence_refused": true,
            "higher_sequence_wrong_parent_refused": true, "local_trust_unchanged_on_failure": true,
            "fresh_offline_global_newest_known": false,
            "scope": "independent local trust histories; no cloud transport or machine handoff"
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn finalized_ciphertext_only_publisher_is_immutable_and_retry_checks_the_exact_owned_object() {
    let f = fixture();
    let (keys, copy) = keys(&f);
    let anchor = trust(&keys);
    let (manifest, bodies) = payload(b"private-payload-marker".to_vec(), 1, None, 3);
    let mut encrypted = anchor
        .encrypt(&keys, &f.staging, manifest, bodies, 0, Limits::default())
        .unwrap();
    let admitted = anchor.admit(&encrypted, CommandId::new(), 0).unwrap();
    let phases = RefCell::new(Vec::new());
    let first = f
        .vault
        .publish(&mut encrypted, &admitted, &|| false, &|phase| {
            phases.borrow_mut().push(phase)
        })
        .unwrap();
    assert!(first.locally_published);
    assert!(!first.transfer_observed);
    assert!(!first.restore_verified);
    assert_eq!(first.object.len(), 40);
    let bytes = std::fs::read(f.vault_path.join(&first.object)).unwrap();
    assert!(bytes.starts_with(b"age-encryption.org/v1\n"));
    for marker in [
        b"private-payload-marker".as_slice(),
        b"AGE-SECRET-KEY",
        keys.public().key_ref.as_bytes(),
    ] {
        assert!(!bytes.windows(marker.len()).any(|window| window == marker));
    }
    assert_eq!(std::fs::read_dir(&f.vault_path).unwrap().count(), 1);
    assert!(anchor
        .verify_restore(&f.vault_path.join(&first.object), &copy, Limits::default())
        .is_ok());
    let again = f
        .vault
        .publish(&mut encrypted, &admitted, &|| false, &|_| {})
        .unwrap();
    assert_eq!(again.ciphertext_sha256, first.ciphertext_sha256);
    assert_eq!(again.object, first.object);
    std::fs::write(
        f.vault_path.join(&first.object),
        b"foreign content must survive conflict",
    )
    .unwrap();
    let rejected = f
        .vault
        .publish(&mut encrypted, &admitted, &|| false, &|_| {})
        .err()
        .unwrap();
    assert_eq!(rejected.code, FailureCode::IdentityConflict);
    assert_eq!(
        std::fs::read(f.vault_path.join(&first.object)).unwrap(),
        b"foreign content must survive conflict"
    );
    assert_eq!(phases.borrow().last(), Some(&Phase::LocallyPublished));
    drop(encrypted);
    assert_eq!(std::fs::read_dir(&f.stage_path).unwrap().count(), 0);
}

#[test]
fn cancelled_copies_and_failed_encryption_remove_only_owned_ciphertext_partials() {
    let f = fixture();
    let (keys, _copy) = keys(&f);
    let anchor = trust(&keys);
    let marker = b"SECRET-SOURCE-CANARY";
    let (manifest, bodies) = payload(marker.repeat(16_000), 1, None, 3);
    assert!(anchor
        .encrypt(
            &keys,
            &f.staging,
            manifest.clone(),
            bodies.clone(),
            0,
            Limits {
                ciphertext_bytes: 1,
                ..Limits::default()
            }
        )
        .is_err());
    assert_eq!(std::fs::read_dir(&f.stage_path).unwrap().count(), 0);
    let mut encrypted = anchor
        .encrypt(&keys, &f.staging, manifest, bodies, 0, Limits::default())
        .unwrap();
    for during in [false, true] {
        let admitted = anchor.admit(&encrypted, CommandId::new(), 0).unwrap();
        let cancelled = Cell::new(!during);
        let seen = Cell::new(false);
        let failure = f
            .vault
            .publish(&mut encrypted, &admitted, &|| cancelled.get(), &|phase| {
                if let Phase::CiphertextBytes(count) = phase {
                    assert!(count > 0);
                    for file in std::fs::read_dir(&f.vault_path).unwrap() {
                        let bytes = std::fs::read(file.unwrap().path()).unwrap();
                        assert!(bytes.starts_with(b"age-encryption.org/v1\n"));
                        assert!(!bytes.windows(marker.len()).any(|window| window == marker));
                    }
                    seen.set(true);
                    cancelled.set(true);
                }
            })
            .err()
            .unwrap();
        assert_eq!(failure.code, FailureCode::Cancelled);
        assert_eq!(seen.get(), during);
        assert!(failure.cleanup_pending.is_none());
        assert_eq!(std::fs::read_dir(&f.vault_path).unwrap().count(), 0);
    }
    drop(encrypted);
    assert_eq!(std::fs::read_dir(&f.stage_path).unwrap().count(), 0);
}

#[cfg(windows)]
#[test]
fn native_cleanup_failure_retains_an_owned_handle_and_directory_replacement_is_fenced() {
    let f = fixture();
    assert!(std::fs::rename(&f.recovery_path, f.recovery_path.with_extension("moved")).is_err());
    let (keys, _copy) = keys(&f);
    let anchor = trust(&keys);
    let (manifest, bodies) = payload(b"synthetic protected source".repeat(8000), 1, None, 3);
    let mut encrypted = anchor
        .encrypt(&keys, &f.staging, manifest, bodies, 0, Limits::default())
        .unwrap();
    let admitted = anchor.admit(&encrypted, CommandId::new(), 0).unwrap();
    let cancelled = Cell::new(false);
    let original_permissions = RefCell::new(None);
    let failed = f
        .vault
        .publish(&mut encrypted, &admitted, &|| cancelled.get(), &|phase| {
            if matches!(phase, Phase::CiphertextBytes(_)) {
                let path = std::fs::read_dir(&f.vault_path)
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap()
                    .path();
                let mut permissions = std::fs::metadata(&path).unwrap().permissions();
                original_permissions.replace(Some(permissions.clone()));
                permissions.set_readonly(true);
                std::fs::set_permissions(path, permissions).unwrap();
                cancelled.set(true);
            }
        })
        .err()
        .unwrap();
    assert_eq!(failed.code, FailureCode::CleanupPending);
    let partial = failed.cleanup_pending.unwrap();
    let path = f.vault_path.join(partial.object());
    assert!(std::fs::rename(&path, path.with_extension("substituted")).is_err());
    std::fs::set_permissions(&path, original_permissions.into_inner().unwrap()).unwrap();
    assert!(partial.cleanup().is_ok());
    assert!(!path.exists());
}

#[test]
#[ignore = "requires independently provisioned Go age and Node executables"]
fn independent_age_and_node_verify_the_finalized_production_envelope() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let f = fixture();
    let (keys, copy) = keys(&f);
    let trust = trust(&keys);
    let (manifest, payloads) = payload(b"independent-interop-canary".to_vec(), 1, None, 3);
    let mut ciphertext = trust
        .encrypt(&keys, &f.staging, manifest, payloads, 0, Limits::default())
        .unwrap();
    let permit = trust.admit(&ciphertext, CommandId::new(), 0).unwrap();
    let published = f
        .vault
        .publish(&mut ciphertext, &permit, &|| false, &|_| {})
        .unwrap();
    // argv contains paths only, never the secret identity or recovered plaintext.
    let age = std::env::var_os("VCP_TEST_AGE").expect("independent Go age path");
    let decrypted = Command::new(age)
        .args(["--decrypt", "--identity"])
        .arg(copy.path())
        .arg(f.vault_path.join(published.object))
        .output()
        .unwrap();
    assert!(
        decrypted.status.success(),
        "independent age rejected envelope"
    );
    let envelope: serde_json::Value = serde_json::from_slice(&decrypted.stdout).unwrap();
    assert!(envelope["payloads"]
        .as_object()
        .unwrap()
        .values()
        .any(|value| {
            serde_json::from_value::<Vec<u8>>(value.clone()).unwrap()
                == b"independent-interop-canary"
        }));
    let node = std::env::var_os("VCP_TEST_NODE").expect("independent Node path");
    let script = r#"const crypto=require('node:crypto');let data=[];process.stdin.on('data',x=>data.push(x));process.stdin.on('end',()=>{const e=JSON.parse(Buffer.concat(data));const key=crypto.createPublicKey({key:Buffer.concat([Buffer.from('302a300506032b6570032100','hex'),Buffer.from(e.writer)]),format:'der',type:'spki'});const valid=crypto.verify(null,Buffer.concat([Buffer.from('vcp-portable-manifest-signature-v1\0'),Buffer.from(e.body)]),key,Buffer.from(e.signature));process.exit(valid?0:1);});"#;
    let mut verifier = Command::new(node)
        .args(["-e", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    verifier
        .stdin
        .take()
        .unwrap()
        .write_all(&decrypted.stdout)
        .unwrap();
    assert!(
        verifier.wait().unwrap().success(),
        "independent Node rejected Ed25519 signature"
    );
}

#[test]
fn publication_checkpoint_uses_opaque_proof_not_mutable_diagnostics() {
    let f = fixture();
    let (keys, _) = keys(&f);
    let mut trust = trust(&keys);
    let (manifest, payloads) = payload(b"checkpoint".to_vec(), 1, None, 3);
    let expected_parent = digest_bytes(&canonical_bytes(&manifest).unwrap());
    let mut ciphertext = trust
        .encrypt(&keys, &f.staging, manifest, payloads, 0, Limits::default())
        .unwrap();
    let permit = trust.admit(&ciphertext, CommandId::new(), 0).unwrap();
    let mut receipt = f
        .vault
        .publish(&mut ciphertext, &permit, &|| false, &|_| {})
        .unwrap();
    receipt.sequence = u64::MAX;
    receipt.deletion = u64::MAX;
    receipt.key_ref = "tampered-public-diagnostic".into();
    trust.advance_after_publication(&receipt, 0).unwrap();
    assert_eq!(trust.configuration().checkpoint.sequence, 1);
    assert_eq!(trust.configuration().checkpoint.deletion, 3);
    assert_eq!(
        trust.configuration().checkpoint.parent,
        Some(expected_parent)
    );
    assert!(trust.advance_after_publication(&receipt, 1).is_err());
}
