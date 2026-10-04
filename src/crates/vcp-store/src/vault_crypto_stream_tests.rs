// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::cell::Cell;

struct Pattern<'a> {
    remaining: usize,
    calls: &'a Cell<usize>,
    largest: &'a Cell<usize>,
}
impl Read for Pattern<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        self.calls.set(self.calls.get() + 1);
        self.largest.set(self.largest.get().max(output.len()));
        let count = output.len().min(self.remaining).min(997);
        output[..count].fill(0x5a);
        self.remaining -= count;
        Ok(count)
    }
}
fn payload(size: usize) -> Object {
    let mut digest = Sha256::new();
    let buffer = [0x5a; 4096];
    let mut remaining = size;
    while remaining > 0 {
        let take = remaining.min(buffer.len());
        digest.update(&buffer[..take]);
        remaining -= take;
    }
    Object {
        bytes: size as u64,
        sha256: format!("{:x}", digest.finalize()),
    }
}
fn manifest(size: usize) -> StreamManifest {
    StreamManifest {
        format: FORMAT.into(),
        workspace: WorkspaceId::parse("workspace").unwrap(),
        lineage: "a".repeat(64),
        sequence: 2,
        deletion: 3,
        parent: Some("b".repeat(64)),
        archive_root: RootDescriptor {
            format: "vcp-neutral-history/2".into(),
            sha256: "c".repeat(64),
            descriptor_bytes: 512,
        },
        payload: payload(size),
    }
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
    fs::create_dir_all(&stage).unwrap();
    fs::create_dir_all(&vault).unwrap();
    PrivateStaging::open(&stage, &[vault]).unwrap()
}
fn limits() -> Limits {
    Limits {
        plaintext_bytes: 8 * 1024 * 1024,
        payload_bytes: 6 * 1024 * 1024,
        ciphertext_bytes: 9 * 1024 * 1024,
        objects: 1024,
    }
}
fn encrypted(root: &Path, identity: &Identity, writer: &SigningKey, size: usize) -> PathBuf {
    let calls = Cell::new(0);
    let largest = Cell::new(0);
    let stage = staging(root);
    let mut encrypted = encrypt(
        &stage,
        &identity.to_public(),
        writer,
        manifest(size),
        Pattern {
            remaining: size,
            calls: &calls,
            largest: &largest,
        },
        limits(),
    )
    .unwrap();
    assert_eq!(encrypted.manifest, manifest(size));
    assert_eq!(encrypted.writer, writer.verifying_key().to_bytes());
    assert!(largest.get() <= FRAME_BYTES);
    assert!(calls.get() > size / 997);
    let path = root.join("hydrated.age");
    let mut output = File::create(&path).unwrap();
    encrypted.copy_ciphertext(&mut output).unwrap();
    output.sync_all().unwrap();
    assert_eq!(output.metadata().unwrap().len(), encrypted.bytes);
    assert!(encrypted.path.parent().unwrap().ends_with("stage"));
    path
}

#[test]
fn framed_transport_streams_beyond_v1_archive_bound_without_payload_map() {
    let root = tempfile::tempdir().unwrap();
    let identity = Identity::generate();
    let writer = SigningKey::from_bytes(&[42; 32]);
    let size = 5 * 1024 * 1024 + 17;
    let path = encrypted(root.path(), &identity, &writer, size);
    let stage = staging(root.path());
    let mut result = decrypt(&stage, &path, &identity, &trust(&writer), limits()).unwrap();
    assert_eq!(result.manifest, manifest(size));
    assert_eq!(result.writer, writer.verifying_key().to_bytes());
    let mut reader = result.reader().unwrap();
    let mut buffer = [0u8; 4096];
    let mut total = 0;
    loop {
        let count = reader.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        assert!(buffer[..count].iter().all(|byte| *byte == 0x5a));
        total += count;
    }
    assert_eq!(total, size);
    // Format negotiation is explicit. The unchanged v1 decoder rejects this
    // fully valid v2 ciphertext instead of treating its bytes as old inventory.
    assert!(super::super::decrypt(&path, &identity, &trust(&writer), limits()).is_err());
}

#[test]
fn full_authentication_freshness_and_failure_leave_no_plaintext_capability() {
    let root = tempfile::tempdir().unwrap();
    let identity = Identity::generate();
    let writer = SigningKey::from_bytes(&[42; 32]);
    let path = encrypted(root.path(), &identity, &writer, FRAME_BYTES + 7);
    let stage = staging(root.path());
    for fault in [
        "workspace",
        "lineage",
        "sequence",
        "deletion",
        "parent",
        "writer",
    ] {
        let mut changed = trust(&writer);
        match fault {
            "workspace" => changed.workspace = WorkspaceId::new(),
            "lineage" => changed.lineage = "d".repeat(64),
            "sequence" => changed.minimum_sequence = 2,
            "deletion" => changed.minimum_deletion = 4,
            "parent" => changed.parent = None,
            "writer" => changed.writers.clear(),
            _ => unreachable!(),
        }
        assert!(
            decrypt(&stage, &path, &identity, &changed, limits()).is_err(),
            "{fault}"
        );
    }
    let bytes = fs::read(&path).unwrap(); // Small corruption fixture only.
    for fault in ["truncated", "final_tag", "trailing"] {
        let mut damaged = bytes.clone();
        match fault {
            "truncated" => {
                damaged.truncate(damaged.len() - 1);
            }
            "final_tag" => {
                *damaged.last_mut().unwrap() ^= 1;
            }
            "trailing" => damaged.push(0),
            _ => unreachable!(),
        }
        let damaged_path = root.path().join(format!("{fault}.age"));
        fs::write(&damaged_path, damaged).unwrap();
        assert!(
            decrypt(&stage, &damaged_path, &identity, &trust(&writer), limits()).is_err(),
            "{fault}"
        );
        assert_eq!(
            fs::read_dir(root.path().join("stage")).unwrap().count(),
            0,
            "failed private plaintext must not survive the owned handle"
        );
    }
}

#[test]
fn frame_contract_rejects_bad_lengths_hashes_count_footer_and_input_commitment() {
    let object = payload(37);
    let mut encoded = Vec::new();
    write_frames(&mut encoded, &[0x5a; 37][..], &object).unwrap();
    for fault in [
        "length",
        "hash",
        "footer_count",
        "footer_length",
        "footer_hash",
        "extra",
        "short",
    ] {
        let mut bytes = encoded.clone();
        match fault {
            "length" => bytes[..4].copy_from_slice(&u32::MAX.to_be_bytes()),
            "hash" => bytes[4] ^= 1,
            "footer_count" => bytes[4 + 32 + 37 + 4 + 7] ^= 1,
            "footer_length" => bytes[4 + 32 + 37 + 4 + 8 + 7] ^= 1,
            "footer_hash" => {
                *bytes.last_mut().unwrap() ^= 1;
            }
            "extra" => bytes.push(0),
            "short" => {
                bytes.pop();
            }
            _ => unreachable!(),
        }
        assert!(
            read_frames(bytes.as_slice(), std::io::sink(), &object).is_err(),
            "{fault}"
        );
    }
    assert!(write_frames(std::io::sink(), &[0x5a; 36][..], &object).is_err());
    assert!(write_frames(std::io::sink(), &[0x5a; 38][..], &object).is_err());
    assert!(write_frames(std::io::sink(), &[0; 37][..], &object).is_err());
}

#[test]
fn input_failure_and_ciphertext_capacity_cannot_finalize() {
    // Use a terminal input failure: Interrupted is deliberately retried by the
    // normal Read contract and cannot represent a cancellation request.
    struct Failed;
    impl Read for Failed {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::UnexpectedEof.into())
        }
    }
    let root = tempfile::tempdir().unwrap();
    let stage = staging(root.path());
    let identity = Identity::generate();
    let writer = SigningKey::from_bytes(&[42; 32]);
    assert!(encrypt(
        &stage,
        &identity.to_public(),
        &writer,
        manifest(37),
        Failed,
        limits()
    )
    .is_err());
    let mut small = limits();
    small.ciphertext_bytes = 32;
    assert!(encrypt(
        &stage,
        &identity.to_public(),
        &writer,
        manifest(37),
        &[0x5a; 37][..],
        small
    )
    .is_err());
    assert_eq!(fs::read_dir(root.path().join("stage")).unwrap().count(), 0);
}

#[test]
fn signed_root_is_immutable_and_cancellation_discards_tentative_plaintext() {
    let root = tempfile::tempdir().unwrap();
    let stage = staging(root.path());
    let identity = Identity::generate();
    let writer = SigningKey::from_bytes(&[42; 32]);
    let value = manifest(FRAME_BYTES + 7);
    let body = canonical_bytes(&value).unwrap();
    let mut header = Header {
        writer: writer.verifying_key().to_bytes(),
        manifest: value,
        signature: writer
            .sign(&[DOMAIN, body.as_slice()].concat())
            .to_bytes()
            .to_vec(),
    };
    trust_manifest(&header, &trust(&writer), limits()).unwrap();
    header.manifest.archive_root.sha256 = "f".repeat(64);
    assert!(trust_manifest(&header, &trust(&writer), limits()).is_err());
    let path = encrypted(root.path(), &identity, &writer, FRAME_BYTES + 7);
    let checks = Cell::new(0);
    let check = || {
        checks.set(checks.get() + 1);
        if checks.get() == 3 {
            Err(Error::Unavailable("synthetic cancellation"))
        } else {
            Ok(())
        }
    };
    // Header and first frame have been accepted when the second frame check
    // fails. No authenticated object or persistent tentative plaintext escapes.
    assert!(matches!(
        decrypt_checked(&stage, &path, &identity, &trust(&writer), limits(), &check),
        Err(Error::Unavailable("synthetic cancellation"))
    ));
    assert_eq!(checks.get(), 3);
    assert_eq!(fs::read_dir(root.path().join("stage")).unwrap().count(), 0);
    let mut bounded = limits();
    bounded.payload_bytes = FRAME_BYTES + 7;
    bounded.plaintext_bytes = FRAME_BYTES + 7; // Framing/header overhead exceeds this exact payload bound.
    assert!(decrypt(&stage, &path, &identity, &trust(&writer), bounded).is_err());
    assert_eq!(fs::read_dir(root.path().join("stage")).unwrap().count(), 0);
}
