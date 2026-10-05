// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    keys::RecoveryDirectory,
    vault_crypto::{Finalization, Object},
    vault_publish::{Phase, Vault},
};
use std::{
    cell::{Cell, RefCell},
    io::Read,
};
struct Fixture {
    _temp: tempfile::TempDir,
    stage: PrivateStaging,
    stage_path: PathBuf,
    vault: Vault,
    recovery: RecoveryCopy,
    keys: VerifiedKeys,
    trust: LocalTrust,
    forbidden: Vec<PathBuf>,
}
fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let stage_path = temp.path().join("stage");
    let vault_path = temp.path().join("vault");
    let recovery_path = temp.path().join("recovery");
    for path in [&stage_path, &vault_path, &recovery_path] {
        std::fs::create_dir(path).unwrap();
    }
    let forbidden = vec![vault_path.clone()];
    let stage = PrivateStaging::open(&stage_path, &forbidden).unwrap();
    let vault = Vault::open(&vault_path, &[stage_path.clone(), recovery_path.clone()]).unwrap();
    let directory = RecoveryDirectory::open(&recovery_path, &forbidden).unwrap();
    let keys = LocalKeys::generate().unwrap();
    let recovery = keys.export_recovery(&directory).unwrap();
    let keys = keys.verify_recovery(&recovery).unwrap();
    let trust = LocalTrust::enroll(
        &keys,
        WorkspaceId::parse("workspace").unwrap(),
        "a".repeat(64),
        Checkpoint {
            sequence: 0,
            deletion: 0,
            parent: None,
        },
    )
    .unwrap();
    Fixture {
        _temp: temp,
        stage,
        stage_path,
        vault,
        recovery,
        keys,
        trust,
        forbidden,
    }
}
fn manifest(bytes: &[u8]) -> StreamManifest {
    StreamManifest {
        format: stream::FORMAT.into(),
        workspace: WorkspaceId::parse("workspace").unwrap(),
        lineage: "a".repeat(64),
        sequence: 1,
        deletion: 0,
        parent: None,
        archive_root: stream::RootDescriptor {
            format: "vcp-neutral-history/2".into(),
            sha256: digest_bytes(b"root"),
            descriptor_bytes: 4,
        },
        payload: Object {
            bytes: bytes.len() as u64,
            sha256: digest_bytes(bytes),
        },
    }
}
fn encrypt(f: &Fixture, bytes: &[u8]) -> FinalizedCiphertext {
    f.trust
        .encrypt_stream(
            &f.keys,
            &f.stage,
            manifest(bytes),
            bytes,
            0,
            Limits::default(),
            &|| Ok(()),
        )
        .unwrap()
}
fn persist(f: &Fixture, object: &mut FinalizedCiphertext, name: &str) -> (PathBuf, Finalization) {
    let path = f.stage_path.join(name);
    let mut file = File::create(&path).unwrap();
    object.copy_ciphertext(&mut file).unwrap();
    file.sync_all().unwrap();
    (path, object.finalization())
}
#[test]
fn legacy_finalization_bytes_and_versioned_ciphertext_reopen_remain_exact() {
    let f = fixture();
    let bytes = b"legacy synthetic data".to_vec();
    let hash = digest_bytes(&bytes);
    let legacy = Manifest {
        format: vault_crypto::FORMAT.into(),
        workspace: f.trust.configuration().workspace.clone(),
        lineage: "a".repeat(64),
        sequence: 1,
        deletion: 0,
        parent: None,
        objects: BTreeMap::from([(
            hash.clone(),
            Object {
                bytes: bytes.len() as u64,
                sha256: hash.clone(),
            },
        )]),
    };
    let mut original = f
        .trust
        .encrypt(
            &f.keys,
            &f.stage,
            legacy.clone(),
            BTreeMap::from([(hash, bytes)]),
            0,
            Limits::default(),
        )
        .unwrap();
    let (path, receipt) = persist(&f, &mut original, "legacy.age");
    let old_shape = serde_json::json!({"manifest":legacy,"writer":receipt.writer,"recipient":receipt.recipient,"sha256":receipt.sha256,"bytes":receipt.bytes});
    assert_eq!(
        canonical_bytes(&receipt).unwrap(),
        canonical_bytes(&old_shape).unwrap()
    );
    let decoded: Finalization =
        serde_json::from_slice(&canonical_bytes(&old_shape).unwrap()).unwrap();
    let directory = Arc::new(Directory::open(&f.stage_path, &f.forbidden).unwrap());
    let mut reopened = FinalizedCiphertext::reopen(&path, &decoded, directory.clone()).unwrap();
    assert_eq!(reopened.format(), vault_crypto::FORMAT);
    let mut first = Vec::new();
    let mut second = Vec::new();
    original.copy_ciphertext(&mut first).unwrap();
    reopened.copy_ciphertext(&mut second).unwrap();
    assert_eq!(first, second);
    let mut stream = encrypt(&f, b"stream synthetic data");
    let (path, receipt) = persist(&f, &mut stream, "stream.age");
    let decoded: Finalization =
        serde_json::from_slice(&canonical_bytes(&receipt).unwrap()).unwrap();
    let mut reopened = FinalizedCiphertext::reopen(&path, &decoded, directory.clone()).unwrap();
    assert_eq!(reopened.format(), stream::FORMAT);
    assert_eq!(reopened.recipient, f.keys.public().recipient);
    let mut first = Vec::new();
    let mut second = Vec::new();
    stream.copy_ciphertext(&mut first).unwrap();
    reopened.copy_ciphertext(&mut second).unwrap();
    assert_eq!(first, second);
    for fault in ["hash", "length"] {
        let mut bad = receipt.clone();
        if fault == "hash" {
            bad.sha256 = "0".repeat(64);
        } else {
            bad.bytes += 1;
        }
        assert!(FinalizedCiphertext::reopen(&path, &bad, directory.clone()).is_err());
    }
}
#[tokio::test]
async fn streamed_ciphertext_uses_existing_recorded_publication_resume_and_restore_trust() {
    let mut f = fixture();
    let bytes = vec![0x6d; 300000];
    let mut object = encrypt(&f, &bytes);
    let permit = f.trust.admit(&object, CommandId::new(), 0).unwrap();
    let identity = RefCell::new(None);
    let recorded = Cell::new(false);
    let mut pending = Box::pin(f.vault.publish_recorded(
        &mut object,
        &permit,
        None,
        |value| async {
            *identity.borrow_mut() = Some(value);
            std::future::pending::<Result<()>>().await
        },
        &|| false,
        &|_| {},
    ));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(10), &mut pending)
            .await
            .is_err()
    );
    drop(pending);
    let prior = identity
        .into_inner()
        .expect("native ownership callback ran");
    let path = f
        .vault
        .directory()
        .join(format!("{}.age", permit.operation));
    assert_eq!(
        std::fs::metadata(&path).unwrap().len(),
        0,
        "ownership must be recorded before ciphertext bytes"
    );
    let published = f
        .vault
        .publish_recorded(
            &mut object,
            &permit,
            Some(&prior),
            |_| async {
                recorded.set(true);
                Ok(())
            },
            &|| false,
            &|phase| {
                if matches!(phase, Phase::CiphertextBytes(_)) {
                    assert!(recorded.get());
                }
            },
        )
        .await
        .ok()
        .unwrap();
    assert!(published.locally_published);
    assert!(!published.transfer_observed && !published.restore_verified);
    let reconciled = f.vault.reconcile(&object, &permit).unwrap();
    assert_eq!(reconciled.ciphertext_sha256, published.ciphertext_sha256);
    let mut restored = f
        .trust
        .verify_stream_restore(&f.stage, &path, &f.recovery, Limits::default(), &|| Ok(()))
        .unwrap();
    assert_eq!(restored.manifest(), &manifest(&bytes));
    let mut restored_bytes = Vec::new();
    restored
        .reader()
        .unwrap()
        .read_to_end(&mut restored_bytes)
        .unwrap();
    assert_eq!(restored_bytes, bytes);
    f.trust.advance_after_stream_restore(&restored, 0).unwrap();
    assert_eq!(f.trust.configuration().checkpoint.sequence, 1);
    assert!(f.trust.advance_after_stream_restore(&restored, 1).is_err());
    assert!(f
        .trust
        .verify_stream_restore(&f.stage, &path, &f.recovery, Limits::default(), &|| Ok(()))
        .is_err());
}
#[test]
fn streamed_admission_keeps_writer_recipient_freshness_scope_and_cancellation_fences() {
    let f = fixture();
    let bytes = b"retained fixture";
    for fault in ["workspace", "lineage", "sequence", "deletion", "parent"] {
        let mut m = manifest(bytes);
        let mut public = f.trust.configuration().clone();
        match fault {
            "workspace" => m.workspace = WorkspaceId::new(),
            "lineage" => m.lineage = "b".repeat(64),
            "sequence" => m.sequence = 0,
            "deletion" => public.checkpoint.deletion = 1,
            "parent" => m.parent = Some("b".repeat(64)),
            _ => unreachable!(),
        }
        let trust = LocalTrust::from_local_configuration(public).unwrap();
        assert!(trust
            .encrypt_stream(
                &f.keys,
                &f.stage,
                m,
                bytes.as_slice(),
                0,
                Limits::default(),
                &|| Ok(())
            )
            .is_err());
    }
    let mut object = encrypt(&f, bytes);
    let writer = object.writer;
    object.writer = [0; 32];
    assert!(f.trust.admit(&object, CommandId::new(), 0).is_err());
    object.writer = writer;
    let recipient = object.recipient.clone();
    object.recipient = age::x25519::Identity::generate().to_public().to_string();
    assert!(f.trust.admit(&object, CommandId::new(), 0).is_err());
    object.recipient = recipient;
    assert!(f.trust.admit(&object, CommandId::new(), 1).is_err());
    let permit = f.trust.admit(&object, CommandId::new(), 0).unwrap();
    assert!(f
        .vault
        .publish(&mut object, &permit, &|| true, &|_| {})
        .is_err());
    assert!(!f
        .vault
        .directory()
        .join(format!("{}.age", permit.operation))
        .exists());
    assert!(f
        .trust
        .encrypt_stream(
            &f.keys,
            &f.stage,
            manifest(bytes),
            bytes.as_slice(),
            0,
            Limits::default(),
            &|| Err(Error::Unavailable("cancel"))
        )
        .is_err());
}
