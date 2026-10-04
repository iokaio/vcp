// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    artifact::ArtifactWriter,
    contract::{Record, State},
    history_index::io::Files,
};
use vcp_domain::Revision;
#[path = "../tests/common/mod.rs"]
mod common;

fn directory(path: &Path, forbidden: &[PathBuf]) -> Result<Directory> {
    fs::create_dir(path)?;
    Directory::open(path, forbidden)
}
fn retain(state: &mut State, descriptor: &ArtifactDescriptor) {
    let row = Record::typed(
        Collection::Artifact,
        descriptor.spec.id.to_string(),
        descriptor.spec.scope.workspace.clone(),
        Revision::ZERO,
        descriptor,
    )
    .unwrap();
    state.records.insert(row.key(), row);
}
fn fixture(root: &Path) -> (Spool, State, Vec<(ArtifactDescriptor, Vec<u8>)>) {
    let spool = Spool::open(root, &[], crate::artifact::DEFAULT_ARTIFACT_LIMIT).unwrap();
    let mut state = State::default().prepare(&common::initial()).unwrap().0;
    let mut retained = Vec::new();
    for mode in 0..4 {
        let mut writer = spool.create(common::spec()).unwrap();
        let mut content = Vec::new();
        for index in 0..if mode == 0 { 70 } else { 3 } {
            let mut bytes = vec![index as u8; if mode == 0 { CHUNK_BYTES } else { 31 }];
            bytes[0] = 0xff;
            writer.write_chunk(&bytes).unwrap();
            content.extend_from_slice(&bytes);
        }
        let mut descriptor = match mode {
            1 => writer.abort().unwrap(),
            2 => {
                let id = writer.finalize().unwrap().spec.id;
                drop(writer);
                fs::remove_file(root.join(id.as_str()).join("seal.json")).unwrap();
                spool.inspect(&id).unwrap()
            }
            _ => writer.finalize().unwrap(),
        };
        if mode == 3 {
            descriptor.state = CaptureState::Purged;
            descriptor.retained.clear();
        }
        retain(&mut state, &descriptor);
        retained.push((descriptor, content));
    }
    (spool, state, retained)
}
#[tokio::test]
async fn paged_artifacts_retain_large_binary_complete_aborted_pending_and_purged_states() {
    let temp = tempfile::tempdir().unwrap();
    let forbidden = vec![temp.path().join("vault")];
    fs::create_dir(&forbidden[0]).unwrap();
    let (spool, state, retained) = fixture(&temp.path().join("source"));
    let scratch_dir = directory(&temp.path().join("scratch"), &forbidden).unwrap();
    let final_dir = directory(&temp.path().join("final"), &forbidden).unwrap();
    let mut scratch = Files::new(&scratch_dir);
    let mut final_pages = Files::new(&final_dir);
    let archive = Artifacts::capture(
        (&state).into(),
        &common::workspace().id,
        &spool,
        &forbidden,
        &mut scratch,
        &|| Ok(()),
    )
    .await
    .unwrap();
    assert!(archive.parts.count() > 64);
    archive
        .copy_objects(&mut scratch, &mut final_pages, &|| Ok(()))
        .await
        .unwrap();
    // The native source is no longer accessible to the independent staged replay.
    drop(spool);
    let staged = archive
        .stage(
            (&state).into(),
            &common::workspace().id,
            &mut final_pages,
            &temp.path().join("restored"),
            &forbidden,
            crate::artifact::DEFAULT_ARTIFACT_LIMIT,
            &|| Ok(()),
        )
        .await
        .unwrap();
    for (descriptor, expected) in retained {
        if descriptor.state == CaptureState::Purged {
            assert!(!staged.root().join(descriptor.spec.id.as_str()).exists());
            continue;
        }
        assert_eq!(staged.inspect(&descriptor.spec.id).unwrap(), descriptor);
        let mut bytes = Vec::new();
        staged.read(&descriptor, &mut bytes).unwrap();
        assert_eq!(bytes, expected);
        for name in ["spec.json", "seal.json"] {
            let original = temp
                .path()
                .join("source")
                .join(descriptor.spec.id.as_str())
                .join(name);
            let restored = staged.root().join(descriptor.spec.id.as_str()).join(name);
            assert_eq!(original.exists(), restored.exists());
            if original.exists() {
                assert_eq!(fs::read(original).unwrap(), fs::read(restored).unwrap());
            }
        }
    }
}
#[tokio::test]
async fn artifact_inventory_rejects_missing_bytes_scope_cut_changes_and_cancellation() {
    let temp = tempfile::tempdir().unwrap();
    let forbidden = vec![temp.path().join("vault")];
    fs::create_dir(&forbidden[0]).unwrap();
    let (spool, state, retained) = fixture(&temp.path().join("source"));
    let dir = directory(&temp.path().join("pages"), &forbidden).unwrap();
    let mut pages = Files::new(&dir);
    let archive = Artifacts::capture(
        (&state).into(),
        &common::workspace().id,
        &spool,
        &forbidden,
        &mut pages,
        &|| Ok(()),
    )
    .await
    .unwrap();
    assert!(Artifacts::capture(
        (&state).into(),
        &common::workspace().id,
        &spool,
        &forbidden,
        &mut pages,
        &|| Err(Error::Unavailable("cancel"))
    )
    .await
    .is_err());
    assert!(archive
        .stage(
            (&state).into(),
            &WorkspaceId::new(),
            &mut pages,
            &temp.path().join("foreign"),
            &forbidden,
            crate::artifact::DEFAULT_ARTIFACT_LIMIT,
            &|| Ok(())
        )
        .await
        .is_err());
    assert!(archive
        .stage(
            (&state).into(),
            &common::workspace().id,
            &mut pages,
            &temp.path().join("cancel"),
            &forbidden,
            crate::artifact::DEFAULT_ARTIFACT_LIMIT,
            &|| Err(Error::Unavailable("cancel"))
        )
        .await
        .is_err());
    assert!(!temp.path().join("cancel").exists());
    let pending = &retained[2].0;
    let mut changed = state.clone();
    let mut false_prefix = pending.clone();
    false_prefix.length = vcp_domain::ByteCount::ZERO;
    false_prefix.sha256 = digest_bytes(&[]);
    false_prefix.retained[0].end = vcp_domain::ByteCount::ZERO;
    retain(&mut changed, &false_prefix);
    assert!(Artifacts::capture(
        (&changed).into(),
        &common::workspace().id,
        &spool,
        &forbidden,
        &mut pages,
        &|| Ok(())
    )
    .await
    .is_err());
    assert!(archive
        .stage(
            (&changed).into(),
            &common::workspace().id,
            &mut pages,
            &temp.path().join("wrong-prefix"),
            &forbidden,
            crate::artifact::DEFAULT_ARTIFACT_LIMIT,
            &|| Ok(())
        )
        .await
        .is_err());
    let first = archive.parts.page(&mut pages, None, 1).await.unwrap();
    let part = part(&first[0]).unwrap();
    fs::remove_file(dir.path.join(format!("{}.json", part.digest))).unwrap();
    assert!(archive
        .stage(
            (&state).into(),
            &common::workspace().id,
            &mut pages,
            &temp.path().join("missing"),
            &forbidden,
            crate::artifact::DEFAULT_ARTIFACT_LIMIT,
            &|| Ok(())
        )
        .await
        .is_err());
}
