// SPDX-License-Identifier: Apache-2.0
mod common;
use common::*;
use vcp_domain::artifact::*;
use vcp_store::artifact::*;

#[test]
fn reference_and_spool_retain_full_binary_stream_and_idempotent_seal() {
    let temporary = tempfile::tempdir().unwrap();
    let spool = Spool::open(&temporary.path().join("spool"), &[], DEFAULT_ARTIFACT_LIMIT).unwrap();
    for length in [0, 1, 4097, CHUNK_BYTES * 5 + 17] {
        let spec = spec();
        let bytes: Vec<_> = (0..length).map(|i| (i % 256) as u8).collect();
        let mut memory = MemoryWriter::new(spec.clone(), DEFAULT_ARTIFACT_LIMIT).unwrap();
        let mut local = spool.create(spec).unwrap();
        for chunk in bytes.chunks(CHUNK_BYTES) {
            assert_eq!(
                memory.write_chunk(chunk).unwrap(),
                local.write_chunk(chunk).unwrap()
            );
        }
        let expected = memory.finalize().unwrap();
        let actual = local.finalize().unwrap();
        assert_eq!(expected, actual);
        assert_eq!(local.finalize().unwrap(), actual);
        assert!(local.abort().is_err());
        assert!(local.write_chunk(b"late").is_err());
        drop(local);
        let mut retained = Vec::new();
        spool.read(&actual, &mut retained).unwrap();
        assert_eq!(retained, bytes);
        assert_eq!(spool.inspect(&actual.spec.id).unwrap(), actual);
    }
}
#[test]
fn interrupted_writer_capacity_failure_and_old_prefix_are_explicit() {
    let temporary = tempfile::tempdir().unwrap();
    let spool = Spool::open(temporary.path(), &[], 5).unwrap();
    let spec = spec();
    let mut local = spool.create(spec.clone()).unwrap();
    let mut memory = MemoryWriter::new(spec.clone(), 5).unwrap();
    local.write_chunk(b"abc").unwrap();
    memory.write_chunk(b"abc").unwrap();
    let prefix = spool.inspect(&spec.id).unwrap();
    assert_eq!(prefix.state, CaptureState::Pending);
    local.write_chunk(b"de").unwrap();
    memory.write_chunk(b"de").unwrap();
    let mut original = Vec::new();
    spool.read(&prefix, &mut original).unwrap();
    assert_eq!(original, b"abc");
    assert!(local.write_chunk(b"f").is_err());
    assert!(memory.write_chunk(b"f").is_err());
    assert!(local.finalize().is_err());
    assert!(memory.finalize().is_err());
    assert_eq!(local.abort().unwrap(), memory.abort().unwrap());
    drop(local);
    assert!(spool
        .inspect(&spec.id)
        .unwrap()
        .spec
        .omissions
        .contains(&Omission::CaptureFailure));
    let another = common::spec();
    let mut interrupted = spool.create(another.clone()).unwrap();
    interrupted.write_chunk(b"raw\0").unwrap();
    drop(interrupted);
    let reopened = Spool::open(temporary.path(), &[], 5).unwrap();
    let partial = reopened.inspect(&another.id).unwrap();
    assert_eq!(partial.state, CaptureState::Pending);
    assert_eq!(partial.length.get(), 4);
    assert!(reopened
        .unfinished()
        .unwrap()
        .iter()
        .any(|d| d.spec.id == another.id));
}
#[test]
fn corrupt_chunks_and_failed_finalization_cannot_claim_success() {
    let temporary = tempfile::tempdir().unwrap();
    let spool = Spool::open(temporary.path(), &[], 100).unwrap();
    let spec = spec();
    let mut writer = spool.create(spec.clone()).unwrap();
    writer.write_chunk(b"retained").unwrap();
    let directory = spool.root().join(spec.id.as_str());
    std::fs::create_dir(directory.join("seal.json")).unwrap();
    assert!(writer.finalize().is_err());
    assert!(writer.write_chunk(b"late").is_err());
    std::fs::remove_dir(directory.join("seal.json")).unwrap();
    drop(writer);
    assert_eq!(
        spool.inspect(&spec.id).unwrap().state,
        CaptureState::Pending
    );
    let chunk = std::fs::read_dir(&directory)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "chunk"))
        .unwrap();
    std::fs::write(chunk, b"modified").unwrap();
    assert!(spool.inspect(&spec.id).is_err());
}
#[test]
fn forbidden_destination_is_rejected_before_creating_plaintext_and_channels_stay_distinct() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("cloud");
    std::fs::create_dir(&vault).unwrap();
    let target = vault.join("new/spool");
    assert!(Spool::open(&target, &[vault], 100).is_err());
    assert!(!target.exists());
    let spool = Spool::open(&temporary.path().join("local"), &[], 100).unwrap();
    let mut out = spec();
    out.channel = Channel::Stdout;
    let mut err = spec();
    err.channel = Channel::Stderr;
    let mut a = spool.create(out).unwrap();
    let mut b = spool.create(err).unwrap();
    a.write_chunk(b"stdout").unwrap();
    b.write_chunk(b"stderr").unwrap();
    let ad = a.finalize().unwrap();
    let bd = b.finalize().unwrap();
    assert_ne!(ad.spec.id, bd.spec.id);
    let mut read = Vec::new();
    spool.read(&bd, &mut read).unwrap();
    assert_eq!(read, b"stderr");
}
