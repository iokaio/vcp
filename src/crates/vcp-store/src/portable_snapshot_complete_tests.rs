// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    artifact::{ArtifactWriter, CHUNK_BYTES},
    contract::{State, Transaction},
    durable_owner::Outcome,
    history_catalog::Catalog,
    history_index::io::Files,
    original_commits::OriginalCommits,
    private_paths::Directory,
    snapshot_inputs::Checkpoint,
    vault_crypto::{stream, Limits, PrivateStaging, Trust},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Seek, SeekFrom},
};
#[path = "../tests/common/mod.rs"]
mod common;
fn directory(root: &Path, name: &str, forbidden: &[PathBuf]) -> Directory {
    let path = root.join(name);
    fs::create_dir(&path).unwrap();
    Directory::open(&path, forbidden).unwrap()
}
async fn apply(
    pages: &mut impl Pages,
    owner: &mut DurableOwner,
    state: &mut State,
    tx: Transaction,
) {
    let (next, commit) = state.prepare(&tx).unwrap();
    let Outcome::Prepared(prepared) = owner.prepare(pages, &tx).await.unwrap() else {
        panic!("new transaction")
    };
    assert_eq!(prepared.commit(), &commit);
    *owner = owner
        .advance(
            pages,
            &prepared,
            &serde_json::to_vec_pretty(&commit).unwrap(),
        )
        .await
        .unwrap();
    *state = next;
}
async fn fixture(
    pages: &mut impl Pages,
    spool: &Spool,
    large: bool,
) -> (DurableOwner, Inputs, Vec<ArtifactDescriptor>) {
    let mut state = State::default();
    let catalog = Catalog::from_validated_state(pages, &state).await.unwrap();
    let originals = OriginalCommits::from_validated_base(pages, None)
        .await
        .unwrap();
    let mut owner = DurableOwner::from_replayed(pages, &state, None, catalog, originals)
        .await
        .unwrap();
    apply(pages, &mut owner, &mut state, common::initial()).await;
    let mut descriptors = Vec::new();
    for mode in 0..3 {
        let spec = common::spec();
        let id = spec.id.clone();
        let mut writer = spool.create(spec).unwrap();
        for index in 0..if large && mode == 0 { 70 } else { 1 } {
            let mut bytes = vec![index as u8; if large && mode == 0 { CHUNK_BYTES } else { 71 }];
            bytes[0] = 0xff;
            writer.write_chunk(&bytes).unwrap();
        }
        let descriptor = match mode {
            0 => writer.finalize().unwrap(),
            1 => writer.abort().unwrap(),
            _ => {
                drop(writer);
                spool.inspect(&id).unwrap()
            }
        };
        let tx = common::attach(&state, descriptor.clone(), None);
        apply(pages, &mut owner, &mut state, tx).await;
        descriptors.push(descriptor);
    }
    let mut spec = common::spec();
    spec.schema = "vcp-workspace-checkpoint/1".into();
    let mut writer = spool.create(spec).unwrap();
    writer.write_chunk(&canonical_bytes(&serde_json::json!({"manifest":{"version":1,"bounded_scan_complete":true,"identity":{"workspace":"workspace","binding":"0","repository":"fixture","worktree":"main"},"files":[],"exclusions":[{"path":"excluded","reason":"fixture"}]},"sources":[]})).unwrap()).unwrap();
    let checkpoint = writer.finalize().unwrap();
    let tx = common::attach(&state, checkpoint.clone(), None);
    apply(pages, &mut owner, &mut state, tx).await;
    let inputs = Inputs {
        checkpoint: Some(Checkpoint {
            manifest: checkpoint.spec.id.clone(),
            sources: BTreeMap::new(),
            git: None,
        }),
        generations: Vec::new(),
    };
    descriptors.push(checkpoint);
    (owner, inputs, descriptors)
}
#[tokio::test]
async fn full_archive_encrypts_authenticates_replays_and_preserves_evidence_and_inputs() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    let forbidden = vec![vault.clone()];
    let source_dir = directory(temp.path(), "source-pages", &forbidden);
    let scratch_dir = directory(temp.path(), "scratch-pages", &forbidden);
    let packed_dir = directory(temp.path(), "packed-pages", &forbidden);
    let target_dir = directory(temp.path(), "target-pages", &forbidden);
    let replay_dir = directory(temp.path(), "replay-pages", &forbidden);
    let closure_dir = directory(temp.path(), "closure-pages", &forbidden);
    let stage_dir = directory(temp.path(), "stage", &forbidden);
    let spool = Spool::open(
        &temp.path().join("source-spool"),
        &forbidden,
        crate::artifact::DEFAULT_ARTIFACT_LIMIT,
    )
    .unwrap();
    let mut source = Files::new(&source_dir);
    let mut scratch = Files::new(&scratch_dir);
    let mut packed = Files::new(&packed_dir);
    let (owner, inputs, descriptors) = fixture(&mut source, &spool, true).await;
    let _pins = descriptors
        .iter()
        .map(|d| spool.pin(&d.spec.id).unwrap())
        .collect::<Vec<_>>();
    let archive = Archive::capture(
        &owner,
        &mut source,
        &mut scratch,
        &common::workspace().id,
        &spool,
        &forbidden,
        &inputs,
        &|| Ok(()),
    )
    .await
    .unwrap();
    let objects = archive
        .collect(&mut scratch, &mut packed, &|| Ok(()))
        .await
        .unwrap();
    let mut wire_file = fs::File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(stage_dir.path.join("wire.private"))
        .unwrap();
    let (root, payload) = wire::write(
        &mut packed,
        &objects,
        &archive.root().unwrap(),
        &mut wire_file,
        8 * 1024 * 1024,
        &|| Ok(()),
    )
    .await
    .unwrap();
    assert!(payload.bytes > 4 * 1024 * 1024);
    wire_file.seek(SeekFrom::Start(0)).unwrap();
    let keys = age::x25519::Identity::generate();
    let signer = ed25519_dalek::SigningKey::from_bytes(&[61; 32]);
    let staging = PrivateStaging::open(&stage_dir.path, &forbidden).unwrap();
    let limits = Limits {
        plaintext_bytes: 10 * 1024 * 1024,
        payload_bytes: 8 * 1024 * 1024,
        ciphertext_bytes: 11 * 1024 * 1024,
        objects: 4096,
    };
    let mut encrypted = stream::encrypt(
        &staging,
        &keys.to_public(),
        &signer,
        stream::StreamManifest {
            format: stream::FORMAT.into(),
            workspace: common::workspace().id,
            lineage: "a".repeat(64),
            sequence: 2,
            deletion: 0,
            parent: None,
            archive_root: root.clone(),
            payload: payload.clone(),
        },
        &mut wire_file,
        limits,
    )
    .unwrap();
    let cipher_path = vault.join("archive.age");
    let mut cipher_file = fs::File::create(&cipher_path).unwrap();
    encrypted.copy_ciphertext(&mut cipher_file).unwrap();
    drop(cipher_file);
    let trust = Trust {
        workspace: common::workspace().id,
        lineage: "a".repeat(64),
        writers: BTreeSet::from([signer.verifying_key().to_bytes()]),
        minimum_sequence: 1,
        minimum_deletion: 0,
        parent: None,
    };
    let mut authenticated = stream::decrypt(&staging, &cipher_path, &keys, &trust, limits).unwrap();
    let mut target = Files::new(&target_dir);
    let unpacked = wire::read(
        authenticated.reader().unwrap(),
        &root,
        &payload,
        &mut target,
        &|| Ok(()),
    )
    .await
    .unwrap();
    let restored = Archive::restore(
        unpacked,
        &mut target,
        &mut Files::new(&replay_dir),
        &mut Files::new(&closure_dir),
        &temp.path().join("restored-spool"),
        &forbidden,
        crate::artifact::DEFAULT_ARTIFACT_LIMIT,
        &trust.workspace,
        &|| Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(
        restored.owner.semantic().current(),
        owner.semantic().current()
    );
    assert_eq!(restored.inputs, inputs);
    assert!(restored.coverage.workspace_checkpoint);
    assert_eq!(restored.coverage.workspace_exclusions.len(), 1);
    for descriptor in descriptors {
        assert_eq!(
            restored.spool.inspect(&descriptor.spec.id).unwrap(),
            descriptor
        );
        let mut before = Vec::new();
        let mut after = Vec::new();
        spool.read(&descriptor, &mut before).unwrap();
        restored.spool.read(&descriptor, &mut after).unwrap();
        assert_eq!(after, before);
    }
}

#[tokio::test]
async fn authenticated_shape_never_bypasses_closure_coverage_or_selected_input_validation() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    let forbidden = vec![vault];
    let source_dir = directory(temp.path(), "source", &forbidden);
    let scratch_dir = directory(temp.path(), "scratch", &forbidden);
    let spool = Spool::open(
        &temp.path().join("spool"),
        &forbidden,
        crate::artifact::DEFAULT_ARTIFACT_LIMIT,
    )
    .unwrap();
    let mut source = Files::new(&source_dir);
    let mut scratch = Files::new(&scratch_dir);
    let (owner, inputs, descriptors) = fixture(&mut source, &spool, false).await;
    let _pins = descriptors
        .iter()
        .map(|d| spool.pin(&d.spec.id).unwrap())
        .collect::<Vec<_>>();
    let original = Archive::capture(
        &owner,
        &mut source,
        &mut scratch,
        &common::workspace().id,
        &spool,
        &forbidden,
        &inputs,
        &|| Ok(()),
    )
    .await
    .unwrap();
    for fault in [
        "extra-object",
        "coverage",
        "input-schema",
        "authority",
        "workspace",
        "cancel",
    ] {
        let packed_dir = directory(temp.path(), &format!("{fault}-packed"), &forbidden);
        let replay_dir = directory(temp.path(), &format!("{fault}-replay"), &forbidden);
        let closure_dir = directory(temp.path(), &format!("{fault}-closure"), &forbidden);
        let mut packed = Files::new(&packed_dir);
        let mut archive = original.clone();
        if fault == "coverage" {
            archive.coverage_digest = "0".repeat(64);
        }
        if fault == "input-schema" {
            let mut invalid = inputs.clone();
            invalid.checkpoint.as_mut().unwrap().manifest = descriptors[0].spec.id.clone();
            archive.inputs = InputArchive::capture(&invalid, &mut scratch, &|| Ok(()))
                .await
                .unwrap();
        }
        let mut indexed = IndexedPages::new(&mut packed);
        archive
            .canonical
            .copy_objects(&mut scratch, &mut indexed, &|| Ok(()))
            .await
            .unwrap();
        archive
            .artifacts
            .copy_objects(&mut scratch, &mut indexed, &|| Ok(()))
            .await
            .unwrap();
        archive
            .inputs
            .copy_objects(&mut scratch, &mut indexed, &|| Ok(()))
            .await
            .unwrap();
        if fault == "extra-object" {
            let bytes = b"valid hash, unreachable content";
            indexed.write(&digest_bytes(bytes), bytes).await.unwrap();
        }
        let objects = indexed.finish().unwrap();
        if fault == "authority" {
            archive.authority = "live_authority".into();
        }
        let root = archive.root().unwrap();
        // Components below represent already authenticated/staged wire data;
        // authentication must never be confused with semantic admission.
        let untrusted = UntrustedObjects { root, objects };
        let expected_workspace = if fault == "workspace" {
            WorkspaceId::new()
        } else {
            common::workspace().id
        };
        let restored_path = temp.path().join(format!("{fault}-restored"));
        let error = Archive::restore(
            untrusted,
            &mut packed,
            &mut Files::new(&replay_dir),
            &mut Files::new(&closure_dir),
            &restored_path,
            &forbidden,
            crate::artifact::DEFAULT_ARTIFACT_LIMIT,
            &expected_workspace,
            &|| {
                if fault == "cancel" {
                    Err(Error::Unavailable("cancel"))
                } else {
                    Ok(())
                }
            },
        )
        .await
        .err()
        .expect("invalid archive must not yield restored capability");
        match fault {
            "extra-object" => assert!(matches!(
                error,
                Error::Corruption("neutral object closure count")
            )),
            "coverage" => assert!(matches!(
                error,
                Error::Corruption("archive coverage differs")
            )),
            "input-schema" | "authority" => assert!(matches!(error, Error::Incompatible)),
            "workspace" => assert!(matches!(error, Error::Access)),
            "cancel" => {
                assert!(matches!(error, Error::Unavailable("cancel")));
                assert!(!restored_path.exists());
            }
            _ => unreachable!(),
        }
    }
}
