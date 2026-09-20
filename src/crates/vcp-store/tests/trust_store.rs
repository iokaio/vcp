// SPDX-License-Identifier: Apache-2.0
use vcp_domain::WorkspaceId;
use vcp_store::{
    keys::{LocalKeys, RecoveryDirectory},
    trust_store::TrustStore,
    vault_publish::{Checkpoint, LocalTrust},
};
#[test]
fn independent_trust_reopens_preserves_monotonic_floor_and_rejects_corruption() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("trust");
    let workspace = temp.path().join("workspace");
    let recovery = temp.path().join("recovery");
    for path in [&root, &workspace, &recovery] {
        std::fs::create_dir(path).unwrap();
    }
    let directory = RecoveryDirectory::open(&recovery, &[root.clone(), workspace.clone()]).unwrap();
    let keys = LocalKeys::generate().unwrap();
    let copy = keys.export_recovery(&directory).unwrap();
    let keys = keys.verify_recovery(&copy).unwrap();
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
    let forbidden = [workspace.clone(), recovery.clone()];
    let mut journal = TrustStore::enroll(&root, &forbidden, trust).unwrap();
    assert!(TrustStore::open(&root, &forbidden).is_err());
    journal
        .update(0, |trust| trust.raise_deletion_floor(8, 0))
        .unwrap();
    assert!(journal
        .update(0, |trust| trust.raise_deletion_floor(9, 0))
        .is_err());
    assert!(journal
        .update(1, |trust| trust.raise_deletion_floor(7, 1))
        .is_err());
    drop(journal);
    let journal = TrustStore::open(&root, &forbidden).unwrap();
    assert_eq!(journal.trust().configuration().checkpoint.deletion, 8);
    assert_eq!(journal.trust().configuration().revision, 1);
    drop(journal);
    // An interrupted private staging write does not become authority.
    std::fs::write(
        root.join("trust-interrupted.partial"),
        b"incomplete public metadata",
    )
    .unwrap();
    drop(TrustStore::open(&root, &forbidden).unwrap());
    let malformed = root.join("trust-xxxxxxxxxxxxxxxxxxxx.json");
    std::fs::write(&malformed, b"{}").unwrap();
    assert!(TrustStore::open(&root, &forbidden).is_err());
    std::fs::remove_file(&malformed).unwrap();
    let latest = root.join("trust-00000000000000000001.json");
    let original = std::fs::read(&latest).unwrap();
    std::fs::write(&latest, vec![b' '; 65537]).unwrap();
    assert!(TrustStore::open(&root, &forbidden).is_err());
    std::fs::write(&latest, original).unwrap();
    let mut bytes = std::fs::read(&latest).unwrap();
    bytes.push(b'!');
    std::fs::write(&latest, bytes).unwrap();
    assert!(TrustStore::open(&root, &forbidden).is_err());
    // No fallback to revision zero and no import from the workspace.
    assert!(TrustStore::open(&workspace, &forbidden).is_err());
}
#[test]
fn failed_flush_or_change_never_exposes_new_in_memory_authority() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("trust");
    let outside = temp.path().join("outside");
    let recovery = temp.path().join("recovery");
    for path in [&root, &outside, &recovery] {
        std::fs::create_dir(path).unwrap();
    }
    let directory = RecoveryDirectory::open(&recovery, &[root.clone(), outside.clone()]).unwrap();
    let keys = LocalKeys::generate().unwrap();
    let copy = keys.export_recovery(&directory).unwrap();
    let keys = keys.verify_recovery(&copy).unwrap();
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
    let mut journal = TrustStore::enroll(&root, &[outside, recovery], trust).unwrap();
    let occupied = root.join("trust-00000000000000000001.json");
    std::fs::create_dir(&occupied).unwrap();
    assert!(journal
        .update(0, |trust| trust.raise_deletion_floor(3, 0))
        .is_err());
    assert_eq!(journal.trust().configuration().revision, 0);
    assert_eq!(journal.trust().configuration().checkpoint.deletion, 0);
    std::fs::remove_dir(&occupied).unwrap();
    journal
        .update(0, |trust| trust.raise_deletion_floor(3, 0))
        .unwrap();
    assert_eq!(journal.trust().configuration().revision, 1);
}
