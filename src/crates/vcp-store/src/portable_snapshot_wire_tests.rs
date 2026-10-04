// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    history_index::io::Files,
    private_paths::Directory,
    vault_crypto::{stream, Limits, PrivateStaging, Trust},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{Seek, SeekFrom},
};

#[derive(Default)]
struct Memory {
    data: BTreeMap<String, Vec<u8>>,
    fail: bool,
    writes: usize,
    fail_at: Option<usize>,
}
impl Pages for Memory {
    async fn read(&mut self, key: &str, limit: usize) -> Result<Vec<u8>> {
        let bytes = self
            .data
            .get(key)
            .ok_or(Error::Corruption("wire fixture missing"))?;
        if bytes.len() > limit {
            return Err(Error::Limit("wire fixture page"));
        }
        Ok(bytes.clone())
    }
    async fn write(&mut self, key: &str, bytes: &[u8]) -> Result<()> {
        self.writes += 1;
        if self.fail || self.fail_at == Some(self.writes) {
            return Err(Error::Unavailable("wire fixture destination"));
        }
        if let Some(prior) = self.data.insert(key.into(), bytes.into()) {
            assert_eq!(prior, bytes);
        }
        Ok(())
    }
}

#[tokio::test]
async fn private_disk_pack_encrypt_restore_streams_beyond_v1_archive_bound() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    std::fs::create_dir(&vault).unwrap();
    let source = temp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    let target = temp.path().join("target");
    std::fs::create_dir(&target).unwrap();
    let stage = temp.path().join("stage");
    std::fs::create_dir(&stage).unwrap();
    let source_directory = Directory::open(&source, std::slice::from_ref(&vault)).unwrap();
    let target_directory = Directory::open(&target, std::slice::from_ref(&vault)).unwrap();
    let mut pages = Files::new(&source_directory);
    let mut indexed = IndexedPages::new(&mut pages);
    let mut buffer = vec![0x5a; 64 * 1024];
    for i in 0..70u64 {
        buffer[..8].copy_from_slice(&i.to_be_bytes());
        let digest = digest_bytes(&buffer);
        indexed.write(&digest, &buffer).await.unwrap();
        indexed.write(&digest, &buffer).await.unwrap(); // Exact duplicate is idempotent.
    }
    let objects = indexed.finish().unwrap();
    assert_eq!(objects.count(), 70);
    let root = b"{\"format\":\"synthetic-untrusted-root\"}";
    let mut wire = File::options()
        .create_new(true)
        .read(true)
        .write(true)
        .open(stage.join("wire.private"))
        .unwrap();
    let (descriptor, payload) = write(
        &mut pages,
        &objects,
        root,
        &mut wire,
        6 * 1024 * 1024,
        &|| Ok(()),
    )
    .await
    .unwrap();
    assert!(payload.bytes > 4 * 1024 * 1024);
    wire.seek(SeekFrom::Start(0)).unwrap();
    let keys = age::x25519::Identity::generate();
    let signer = ed25519_dalek::SigningKey::from_bytes(&[43; 32]);
    let manifest = stream::StreamManifest {
        format: stream::FORMAT.into(),
        workspace: vcp_domain::WorkspaceId::parse("workspace").unwrap(),
        lineage: "a".repeat(64),
        sequence: 2,
        deletion: 3,
        parent: Some("b".repeat(64)),
        archive_root: descriptor.clone(),
        payload: payload.clone(),
    };
    let staging = PrivateStaging::open(&stage, std::slice::from_ref(&vault)).unwrap();
    let limits = Limits {
        plaintext_bytes: 8 * 1024 * 1024,
        payload_bytes: 6 * 1024 * 1024,
        ciphertext_bytes: 9 * 1024 * 1024,
        objects: 1024,
    };
    let mut cipher = stream::encrypt(
        &staging,
        &keys.to_public(),
        &signer,
        manifest,
        &mut wire,
        limits,
    )
    .unwrap();
    let ciphertext = vault.join("archive.age");
    let mut output = File::create(&ciphertext).unwrap();
    cipher.copy_ciphertext(&mut output).unwrap();
    drop(output);
    let trust = Trust {
        workspace: vcp_domain::WorkspaceId::parse("workspace").unwrap(),
        lineage: "a".repeat(64),
        writers: BTreeSet::from([signer.verifying_key().to_bytes()]),
        minimum_sequence: 1,
        minimum_deletion: 3,
        parent: Some("b".repeat(64)),
    };
    let mut authenticated = stream::decrypt(&staging, &ciphertext, &keys, &trust, limits).unwrap();
    let mut destination = Files::new(&target_directory);
    let unpacked = read(
        authenticated.reader().unwrap(),
        &descriptor,
        &payload,
        &mut destination,
        &|| Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(unpacked.root, root);
    assert_eq!(unpacked.objects.count(), 70);
    for i in 0..70u64 {
        buffer[..8].copy_from_slice(&i.to_be_bytes());
        assert_eq!(
            destination
                .read(&digest_bytes(&buffer), MAX_OBJECT)
                .await
                .unwrap(),
            buffer
        );
    }
    // This is data transport only; the deliberately synthetic root above cannot
    // be confused with a semantically admitted neutral archive or runnable owner.
}

async fn small() -> (Memory, ObjectSet, Vec<u8>, RootDescriptor, Object) {
    let mut pages = Memory::default();
    let mut indexed = IndexedPages::new(&mut pages);
    for value in [b"alpha".as_slice(), b"beta".as_slice()] {
        indexed.write(&digest_bytes(value), value).await.unwrap();
    }
    let set = indexed.finish().unwrap();
    let mut bytes = Vec::new();
    let (root, payload) = write(&mut pages, &set, b"root", &mut bytes, 10000, &|| Ok(()))
        .await
        .unwrap();
    (pages, set, bytes, root, payload)
}
#[tokio::test]
async fn malformed_order_lengths_root_footer_and_terminal_commitments_reject() {
    let (_, _, original, root, original_payload) = small().await;
    let first = 8 + 4 + 4 + 8;
    for fault in [
        "root",
        "order",
        "length",
        "payload",
        "count",
        "short",
        "trailing",
        "commitment",
    ] {
        let mut bytes = original.clone();
        match fault {
            "root" => bytes[12] ^= 1,
            "order" => {
                let length =
                    u32::from_be_bytes(bytes[first + 64..first + 68].try_into().unwrap()) as usize;
                let key = bytes[first..first + 64].to_vec();
                bytes[first + 68 + length..first + 68 + length + 64].copy_from_slice(&key);
            }
            "length" => bytes[first + 64..first + 68].copy_from_slice(&u32::MAX.to_be_bytes()),
            "payload" => bytes[first + 68] ^= 1,
            "count" => {
                *bytes.last_mut().unwrap() ^= 1;
            }
            "short" => {
                bytes.pop();
            }
            "trailing" => bytes.push(0),
            "commitment" => (),
            _ => unreachable!(),
        }
        let mut payload = Object {
            bytes: bytes.len() as u64,
            sha256: digest_bytes(&bytes),
        };
        if fault == "commitment" {
            payload.sha256 = "f".repeat(64);
        }
        assert!(
            read(
                bytes.as_slice(),
                &root,
                &payload,
                &mut Memory::default(),
                &|| Ok(())
            )
            .await
            .is_err(),
            "{fault}"
        );
    }
    assert!(read(
        original.as_slice(),
        &root,
        &original_payload,
        &mut Memory {
            fail: true,
            ..Default::default()
        },
        &|| Ok(())
    )
    .await
    .is_err());
}

#[tokio::test]
async fn failed_object_staging_and_cancelled_or_over_limit_pack_cannot_finish() {
    let mut pages = Memory::default();
    let mut index = IndexedPages::new(&mut pages);
    assert!(index
        .write(&"a".repeat(64), b"wrong commitment")
        .await
        .is_err());
    assert!(index.finish().is_err());
    let (mut pages, set, bytes, root, payload) = small().await;
    assert!(
        write(&mut pages, &set, b"root", &mut std::io::sink(), 1, &|| Ok(
            ()
        ))
        .await
        .is_err()
    );
    let calls = std::cell::Cell::new(0);
    assert!(read(
        bytes.as_slice(),
        &root,
        &payload,
        &mut Memory::default(),
        &|| {
            calls.set(calls.get() + 1);
            if calls.get() == 3 {
                Err(Error::Unavailable("wire cancelled"))
            } else {
                Ok(())
            }
        }
    )
    .await
    .is_err());
    // A write failure after the content object but before index publication
    // leaves the current object set unavailable, never silently incomplete.
    let mut pages = Memory {
        fail_at: Some(2),
        ..Default::default()
    };
    let mut index = IndexedPages::new(&mut pages);
    assert!(index.write(&digest_bytes(b"x"), b"x").await.is_err());
    assert!(index.finish().is_err());
    assert_eq!(pages.data.get(&digest_bytes(b"x")).unwrap(), b"x");
}
