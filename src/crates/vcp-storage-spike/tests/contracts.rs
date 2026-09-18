// SPDX-License-Identifier: Apache-2.0
use age::{secrecy::ExposeSecret, x25519::Identity};
use ed25519_dalek::SigningKey;
use std::{collections::BTreeSet, fs, io::Write};
use vcp_storage_spike::{
    digest, fixture,
    store::Store,
    vault::{self, Fault, Trust},
    Result,
};

#[tokio::test]
async fn backend_parity_reopen_conversion_and_snapshot_watermark() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (one, objects) = fixture(1, 100);
    for kind in ["sqlite", "files"] {
        let path = root.path().join(kind);
        let mut store = Store::open(&path, kind).await?;
        assert!(Store::open(&path, kind).await.is_err());
        for bytes in objects.values() {
            store.put_artifact(bytes)?;
        }
        store.commit(0, one.clone()).await?;
        store.commit(0, one.clone()).await?;
        let pinned = store.view().unwrap();
        let (two, _) = fixture(2, 101);
        store.commit(1, two.clone()).await?;
        assert_eq!(pinned, one);
        assert!(store.commit(1, fixture(3, 102).0).await.is_err());
        let mut conflicting = fixture(3, 102).0;
        conflicting.command = one.command.clone();
        assert!(store.commit(2, conflicting).await.is_err());
        store.close().await?;
        let other = if kind == "sqlite" { "files" } else { "sqlite" };
        assert!(Store::open(&path, other).await.is_err());
        let source = Store::open(&path, kind).await?;
        assert_eq!(source.view(), Some(two.clone()));
        let other = if kind == "sqlite" { "files" } else { "sqlite" };
        let mut converted =
            Store::open(&root.path().join(format!("{kind}-to-{other}")), other).await?;
        for id in two.artifacts.keys() {
            converted.put_artifact(&source.artifact(id)?)?;
        }
        converted.commit(0, two.clone()).await?;
        converted.close().await?;
        let converted = Store::open(&root.path().join(format!("{kind}-to-{other}")), other).await?;
        assert_eq!(converted.view(), Some(two));
    }
    Ok(())
}

#[tokio::test]
async fn missing_artifacts_invalid_history_and_corrupt_frames_fail_closed() -> Result<()> {
    let root = tempfile::tempdir()?;
    for kind in ["sqlite", "files"] {
        let path = root.path().join(kind);
        let mut store = Store::open(&path, kind).await?;
        let (one, objects) = fixture(1, 10);
        assert!(store.commit(0, one.clone()).await.is_err());
        for bytes in objects.values() {
            store.put_artifact(bytes)?;
        }
        store.commit(0, one.clone()).await?;
        let mut invalid = fixture(2, 11).0;
        invalid.deletion_epoch = 0;
        assert!(store.commit(1, invalid).await.is_err());
        store.close().await?;
        let id = one.artifacts.keys().next().unwrap();
        fs::remove_file(path.join("objects").join(id))?;
        assert!(Store::open(&path, kind).await.is_err());
    }
    let path = root.path().join("corrupt");
    let mut store = Store::open(&path, "files").await?;
    let (one, objects) = fixture(1, 10);
    for bytes in objects.values() {
        store.put_artifact(bytes)?;
    }
    store.commit(0, one).await?;
    store.close().await?;
    let file = path.join("state.frames");
    let original = fs::read(&file)?;
    for offset in [0, 7, 10, original.len() - 1] {
        let mut b = original.clone();
        b[offset] ^= 1;
        fs::write(&file, b)?;
        assert!(Store::open(&path, "files").await.is_err());
    }
    fs::write(&file, &original)?;
    let mut f = fs::OpenOptions::new().append(true).open(&file)?;
    f.write_all(&[1, 2, 3])?;
    drop(f);
    let store = Store::open(&path, "files").await?;
    assert_eq!(store.view().unwrap().sequence, 1);
    store.close().await?;
    assert_eq!(fs::read(file)?, original);
    Ok(())
}

fn trust(writer: &SigningKey) -> Trust {
    Trust {
        writers: BTreeSet::from([writer.verifying_key().to_bytes()]),
        minimum_sequence: 0,
        minimum_deletion: 1,
        parent: None,
    }
}
#[test]
fn encrypted_full_incremental_recovery_rotation_and_writer_authority() -> Result<()> {
    let root = tempfile::tempdir()?;
    let stage = root.path().join("stage");
    let vault = root.path().join("vault");
    fs::create_dir(&stage)?;
    fs::create_dir(&vault)?;
    let identity = Identity::generate();
    // The recovery copy is exported to a separate local directory, then parsed
    // afresh. It is never part of the vault or a test report.
    let recovery = root.path().join("independent-recovery");
    fs::write(&recovery, identity.to_string().expose_secret())?;
    let recovered: Identity = fs::read_to_string(recovery)?
        .parse()
        .map_err(vcp_storage_spike::reject)?;
    let writer = SigningKey::from_bytes(&[42; 32]);
    let (one, objects) = fixture(1, 100);
    let trusted = trust(&writer);
    for incremental in [false, true] {
        let published = vault::publish(
            &one,
            &objects,
            &identity.to_public(),
            &writer,
            &stage,
            &vault,
            None,
            incremental,
            Fault::None,
        )?;
        let restored = vault::restore(&vault, &published.id, &recovered, &trusted)?;
        assert_eq!(restored.view, one);
        assert_eq!(restored.artifacts, objects);
        assert!(vault::restore(&vault, &published.id, &Identity::generate(), &trusted).is_err());
        let stranger = SigningKey::from_bytes(&[43; 32]);
        let forged = vault::publish(
            &one,
            &objects,
            &identity.to_public(),
            &stranger,
            &stage,
            &vault,
            None,
            incremental,
            Fault::None,
        )?;
        assert!(vault::restore(&vault, &forged.id, &recovered, &trusted).is_err());
        let mut replay = trust(&writer);
        replay.minimum_sequence = 1;
        assert!(vault::restore(&vault, &published.id, &recovered, &replay).is_err());
        let (two, _) = fixture(2, 101);
        let update = vault::publish(
            &two,
            &objects,
            &identity.to_public(),
            &writer,
            &stage,
            &vault,
            Some(&published),
            incremental,
            Fault::None,
        )?;
        let mut advanced = trust(&writer);
        advanced.minimum_sequence = 1;
        assert!(vault::restore(&vault, &update.id, &recovered, &advanced).is_err());
        advanced.parent = Some(published.id.clone());
        advanced.minimum_deletion = 2;
        assert!(vault::restore(&vault, &update.id, &recovered, &advanced).is_err());
        advanced.minimum_deletion = 1;
        assert_eq!(
            vault::restore(&vault, &update.id, &recovered, &advanced)?.view,
            two
        );
        if incremental {
            assert!(update.transferred < published.transferred);
        }
        let rotated = Identity::generate();
        let new_writer = SigningKey::from_bytes(&[44; 32]);
        let next = vault::publish(
            &two,
            &objects,
            &rotated.to_public(),
            &new_writer,
            &stage,
            &vault,
            Some(&published),
            incremental,
            Fault::None,
        )?;
        advanced.writers = BTreeSet::from([new_writer.verifying_key().to_bytes()]);
        assert!(vault::restore(&vault, &next.id, &recovered, &advanced).is_err());
        assert_eq!(
            vault::restore(&vault, &next.id, &rotated, &advanced)?.view,
            two
        );
        assert!(vault::restore(&vault, &update.id, &recovered, &advanced).is_err());
    }
    // Every object is age ciphertext, including the manifest and failure residue.
    for entry in fs::read_dir(&vault)? {
        let bytes = fs::read(entry?.path())?;
        assert!(bytes.starts_with(b"age-encryption.org/v1\n"));
        assert!(!bytes.windows(23).any(|w| w == b"VCP_P0_PLAINTEXT_CANARY"));
    }
    Ok(())
}

#[test]
fn tamper_truncation_missing_objects_and_incomplete_publication_reject() -> Result<()> {
    let root = tempfile::tempdir()?;
    let stage = root.path().join("stage");
    let vault = root.path().join("vault");
    fs::create_dir(&stage)?;
    fs::create_dir(&vault)?;
    let identity = Identity::generate();
    let writer = SigningKey::from_bytes(&[42; 32]);
    let trusted = trust(&writer);
    let (one, objects) = fixture(1, 10);
    assert!(vault::publish(
        &one,
        &objects,
        &identity.to_public(),
        &writer,
        &vault,
        &vault,
        None,
        true,
        Fault::None
    )
    .is_err());
    assert!(vault::publish(
        &one,
        &objects,
        &identity.to_public(),
        &writer,
        &stage,
        &vault,
        None,
        true,
        Fault::BeforePublication
    )
    .is_err());
    assert_eq!(fs::read_dir(&vault)?.count(), 0);
    assert!(vault::publish(
        &one,
        &objects,
        &identity.to_public(),
        &writer,
        &stage,
        &vault,
        None,
        true,
        Fault::AfterObjects
    )
    .is_err());
    let published = vault::publish(
        &one,
        &objects,
        &identity.to_public(),
        &writer,
        &stage,
        &vault,
        None,
        true,
        Fault::None,
    )?;
    let original = fs::read(vault.join(format!("{}.age", published.id)))?;
    // Readdress altered ciphertext so rejection must come from age rather than
    // merely the outer content address. Also decrypt raw altered bytes directly.
    for offset in [0, 40, original.len() / 2, original.len() - 1] {
        let mut changed = original.clone();
        changed[offset] ^= 1;
        let id = digest(&changed);
        fs::write(vault.join(format!("{id}.age")), &changed)?;
        assert!(vault::decrypt(&identity, &changed).is_err());
        assert!(vault::restore(&vault, &id, &identity, &trusted).is_err());
    }
    for length in [0, 20, original.len() / 2, original.len() - 1] {
        assert!(vault::decrypt(&identity, &original[..length]).is_err());
    }
    // A recipient can decrypt/re-encrypt, but cannot edit the signed body.
    let plain = vault::decrypt(&identity, &original)?;
    let mut signed: serde_json::Value = serde_json::from_slice(&plain)?;
    signed["signature"] = serde_json::json!(vec![0; 64]);
    let forged = age::encrypt(&identity.to_public(), &serde_json::to_vec(&signed)?)?;
    let id = digest(&forged);
    fs::write(vault.join(format!("{id}.age")), forged)?;
    assert!(vault::restore(&vault, &id, &identity, &trusted).is_err());
    let object = published.objects.values().next().unwrap();
    fs::remove_file(vault.join(format!("{}.age", object.ciphertext)))?;
    assert!(vault::restore(&vault, &published.id, &identity, &trusted).is_err());
    Ok(())
}
