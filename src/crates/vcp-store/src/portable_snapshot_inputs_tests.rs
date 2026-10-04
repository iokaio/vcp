// SPDX-License-Identifier: Apache-2.0
use super::*;
#[derive(Default)]
struct Memory {
    rows: BTreeMap<String, Vec<u8>>,
    max_read: usize,
}
impl Pages for Memory {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        self.max_read = self.max_read.max(limit);
        let bytes = self
            .rows
            .get(digest)
            .ok_or(Error::Corruption("input fixture missing"))?;
        if bytes.len() > limit {
            return Err(Error::Limit("input fixture read"));
        }
        Ok(bytes.clone())
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        if let Some(prior) = self.rows.insert(digest.into(), bytes.into()) {
            assert_eq!(prior, bytes);
        }
        Ok(())
    }
}
#[tokio::test]
async fn large_selected_inputs_use_small_root_and_rows_with_exact_roundtrip() {
    let mut inputs = Inputs {
        checkpoint: Some(Checkpoint {
            manifest: ArtifactId::new(),
            sources: BTreeMap::new(),
            git: Some(GitArtifacts {
                status: ArtifactId::new(),
                index: ArtifactId::new(),
                staged_diff: ArtifactId::new(),
                unstaged_diff: ArtifactId::new(),
            }),
        }),
        generations: Vec::new(),
    };
    for i in 0..100 {
        inputs.checkpoint.as_mut().unwrap().sources.insert(
            format!("src/{i:04}/{}.rs", "é漢".repeat(600)),
            ArtifactId::new(),
        );
    }
    for i in 0..3 {
        let mut files = BTreeMap::new();
        for n in 0..70 {
            files.insert(format!("{i}/{n}.json"), ArtifactId::new());
        }
        inputs.generations.push(GenerationInput {
            id: GenerationId::new(),
            inventory: ArtifactId::new(),
            lexical_manifest: ArtifactId::new(),
            vectors: (i % 2 == 0).then(ArtifactId::new),
            lexical_files: files,
        });
    }
    assert!(canonical_bytes(&inputs).unwrap().len() > 128 * 1024);
    let mut source = Memory::default();
    let archive = InputArchive::capture(&inputs, &mut source, &|| Ok(()))
        .await
        .unwrap();
    assert!(canonical_bytes(&archive).unwrap().len() < 1024);
    let mut destination = Memory::default();
    archive
        .copy_objects(&mut source, &mut destination, &|| Ok(()))
        .await
        .unwrap();
    source.rows.clear();
    assert_eq!(
        archive.read(&mut destination, &|| Ok(())).await.unwrap(),
        inputs
    );
    assert!(destination.max_read <= 128 * 1024);
    for inputs in [
        Inputs::default(),
        Inputs {
            checkpoint: None,
            generations: Vec::new(),
        },
    ] {
        let archive = InputArchive::capture(&inputs, &mut destination, &|| Ok(()))
            .await
            .unwrap();
        assert_eq!(
            archive.read(&mut destination, &|| Ok(())).await.unwrap(),
            inputs
        );
    }
}
#[tokio::test]
async fn input_codec_rejects_row_reordering_duplicate_paths_missing_bytes_and_cancel() {
    let mut pages = Memory::default();
    let mut rows = Root::empty(Table::ArchiveInputs);
    append(
        &mut rows,
        &mut pages,
        Row::Header {
            checkpoint: Some(Header {
                manifest: ArtifactId::new(),
                git: None,
            }),
        },
        &|| Ok(()),
    )
    .await
    .unwrap();
    let header = rows.clone();
    for _ in 0..2 {
        append(
            &mut rows,
            &mut pages,
            Row::Source {
                path: "src/a.rs".into(),
                artifact: ArtifactId::new(),
            },
            &|| Ok(()),
        )
        .await
        .unwrap();
    }
    let invalid = InputArchive { version: 2, rows };
    assert!(invalid.read(&mut pages, &|| Ok(())).await.is_err());
    let mut rows = Root::empty(Table::ArchiveInputs);
    append(
        &mut rows,
        &mut pages,
        Row::Lexical {
            path: "src/a.rs".into(),
            artifact: ArtifactId::new(),
        },
        &|| Ok(()),
    )
    .await
    .unwrap();
    assert!(InputArchive { version: 2, rows }
        .read(&mut pages, &|| Ok(()))
        .await
        .is_err());
    let good = InputArchive {
        version: 2,
        rows: header,
    };
    assert!(good
        .read(&mut pages, &|| Err(Error::Unavailable("cancel")))
        .await
        .is_err());
    let row = good.rows.page(&mut pages, None, 1).await.unwrap();
    let locator: Locator = serde_json::from_value(row[0].value.clone()).unwrap();
    pages.rows.remove(&locator.digest);
    assert!(good.read(&mut pages, &|| Ok(())).await.is_err());
}
