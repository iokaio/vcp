// SPDX-License-Identifier: Apache-2.0
//! Bounded row codec for the existing selected-input contract. It preserves the
//! caller's bounded Inputs value without serializing the whole map into a root
//! descriptor. Decoding is structural only; Inputs::validate_with remains the
//! required semantic gate against replayed current records and staged artifacts.
use crate::{
    history_index::{Entry, Pages, Root, Table},
    snapshot_inputs::{Checkpoint, GenerationInput, GitArtifacts, Inputs},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vcp_domain::{ArtifactId, GenerationId};
use vcp_protocol::{canonical_bytes, digest_bytes};
const MAX_ROW: usize = 32768;
const MAX_ROWS: u64 = 1 + 4096 + 64 + 64 * 256;
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Row {
    Header {
        checkpoint: Option<Header>,
    },
    Source {
        path: String,
        artifact: ArtifactId,
    },
    Generation {
        id: GenerationId,
        inventory: ArtifactId,
        lexical_manifest: ArtifactId,
        vectors: Option<ArtifactId>,
    },
    Lexical {
        path: String,
        artifact: ArtifactId,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    manifest: ArtifactId,
    git: Option<GitArtifacts>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Locator {
    bytes: u64,
    digest: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InputArchive {
    version: u32,
    rows: Root,
}
impl InputArchive {
    pub(crate) async fn capture(
        inputs: &Inputs,
        pages: &mut impl Pages,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Self> {
        if inputs.generations.len() > 64
            || inputs
                .checkpoint
                .as_ref()
                .is_some_and(|c| c.sources.len() > 4096)
            || inputs
                .generations
                .iter()
                .any(|g| g.lexical_files.len() > 256)
        {
            return Err(Error::Limit("archive input cardinality"));
        }
        let mut rows = Root::empty(Table::ArchiveInputs);
        append(
            &mut rows,
            pages,
            Row::Header {
                checkpoint: inputs.checkpoint.as_ref().map(|c| Header {
                    manifest: c.manifest.clone(),
                    git: c.git.clone(),
                }),
            },
            check,
        )
        .await?;
        if let Some(checkpoint) = &inputs.checkpoint {
            for (path, artifact) in &checkpoint.sources {
                path_valid(path)?;
                append(
                    &mut rows,
                    pages,
                    Row::Source {
                        path: path.clone(),
                        artifact: artifact.clone(),
                    },
                    check,
                )
                .await?;
            }
        }
        for generation in &inputs.generations {
            append(
                &mut rows,
                pages,
                Row::Generation {
                    id: generation.id.clone(),
                    inventory: generation.inventory.clone(),
                    lexical_manifest: generation.lexical_manifest.clone(),
                    vectors: generation.vectors.clone(),
                },
                check,
            )
            .await?;
            for (path, artifact) in &generation.lexical_files {
                path_valid(path)?;
                append(
                    &mut rows,
                    pages,
                    Row::Lexical {
                        path: path.clone(),
                        artifact: artifact.clone(),
                    },
                    check,
                )
                .await?;
            }
        }
        check()?;
        Ok(Self { version: 2, rows })
    }
    pub(crate) async fn read(
        &self,
        pages: &mut impl Pages,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Inputs> {
        self.validate()?;
        let mut inputs = Inputs::default();
        let mut after = None;
        let mut count = 0u64;
        loop {
            check()?;
            let rows = self.rows.page(pages, after.as_deref(), 64).await?;
            if rows.is_empty() {
                break;
            }
            for row in &rows {
                check()?;
                if row.key != format!("{count:020}") {
                    return Err(Error::Corruption("archive input ordinal"));
                }
                let (_, bytes) = read_row(pages, row).await?;
                let decoded: Row = serde_json::from_slice(&bytes)?;
                if canonical_bytes(&decoded)? != bytes {
                    return Err(Error::Corruption("archive input row encoding"));
                }
                match decoded {
                    Row::Header { checkpoint } if count == 0 => {
                        inputs.checkpoint = checkpoint.map(|c| Checkpoint {
                            manifest: c.manifest,
                            git: c.git,
                            sources: BTreeMap::new(),
                        })
                    }
                    Row::Source { path, artifact }
                        if count > 0 && inputs.generations.is_empty() =>
                    {
                        path_valid(&path)?;
                        let checkpoint = inputs
                            .checkpoint
                            .as_mut()
                            .ok_or(Error::Corruption("archive input source without checkpoint"))?;
                        if checkpoint.sources.len() >= 4096
                            || checkpoint.sources.insert(path, artifact).is_some()
                        {
                            return Err(Error::Corruption(
                                "archive duplicate or excessive input sources",
                            ));
                        }
                    }
                    Row::Generation {
                        id,
                        inventory,
                        lexical_manifest,
                        vectors,
                    } if count > 0 => {
                        if inputs.generations.len() >= 64 {
                            return Err(Error::Limit("archive generations"));
                        }
                        inputs.generations.push(GenerationInput {
                            id,
                            inventory,
                            lexical_manifest,
                            vectors,
                            lexical_files: BTreeMap::new(),
                        });
                    }
                    Row::Lexical { path, artifact } if count > 0 => {
                        path_valid(&path)?;
                        let generation = inputs.generations.last_mut().ok_or(Error::Corruption(
                            "archive lexical input without generation",
                        ))?;
                        if generation.lexical_files.len() >= 256
                            || generation.lexical_files.insert(path, artifact).is_some()
                        {
                            return Err(Error::Corruption(
                                "archive duplicate or excessive lexical inputs",
                            ));
                        }
                    }
                    _ => return Err(Error::Corruption("archive input row order")),
                }
                count += 1;
            }
            after = rows.last().map(|r| r.key.clone());
        }
        if count != self.rows.count() {
            return Err(Error::Corruption("archive input row count"));
        }
        Ok(inputs)
    }
    pub(crate) async fn copy_objects(
        &self,
        source: &mut impl Pages,
        destination: &mut impl Pages,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        self.rows.copy_pages(source, destination, check).await?;
        let mut after = None;
        let mut count = 0u64;
        loop {
            check()?;
            let rows = self.rows.page(source, after.as_deref(), 64).await?;
            if rows.is_empty() {
                break;
            }
            for row in &rows {
                check()?;
                let (locator, bytes) = read_row(source, row).await?;
                destination.write(&locator.digest, &bytes).await?;
                count += 1;
            }
            after = rows.last().map(|r| r.key.clone());
        }
        if count != self.rows.count() {
            return Err(Error::Corruption("archive input row count"));
        }
        Ok(())
    }
    fn validate(&self) -> Result<()> {
        if self.version != 2 {
            return Err(Error::Incompatible);
        }
        self.rows.validate_table(Table::ArchiveInputs)?;
        if self.rows.count() == 0 || self.rows.count() > MAX_ROWS {
            return Err(Error::Limit("archive input rows"));
        }
        Ok(())
    }
}
fn path_valid(path: &str) -> Result<()> {
    if !crate::snapshot_inputs::relative(path) {
        return Err(Error::Corruption("archive input relative path"));
    }
    Ok(())
}
async fn append(
    root: &mut Root,
    pages: &mut impl Pages,
    row: Row,
    check: &dyn Fn() -> Result<()>,
) -> Result<()> {
    check()?;
    let bytes = canonical_bytes(&row)?;
    if bytes.len() > MAX_ROW || root.count() >= MAX_ROWS {
        return Err(Error::Limit("archive input rows"));
    }
    let digest = digest_bytes(&bytes);
    pages.write(&digest, &bytes).await?;
    *root = root
        .insert(
            pages,
            Entry {
                key: format!("{:020}", root.count()),
                value: serde_json::to_value(Locator {
                    bytes: bytes.len() as u64,
                    digest,
                })?,
            },
        )
        .await?;
    Ok(())
}
async fn read_row(pages: &mut impl Pages, row: &Entry) -> Result<(Locator, Vec<u8>)> {
    let locator: Locator = serde_json::from_value(row.value.clone())?;
    if locator.bytes == 0
        || locator.bytes > MAX_ROW as u64
        || locator.digest.len() != 64
        || !locator
            .digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::Corruption("archive input locator"));
    }
    let bytes = pages.read(&locator.digest, MAX_ROW).await?;
    if bytes.len() as u64 != locator.bytes || digest_bytes(&bytes) != locator.digest {
        return Err(Error::Corruption("archive input bytes"));
    }
    Ok((locator, bytes))
}
#[cfg(test)]
#[path = "portable_snapshot_inputs_tests.rs"]
mod tests;
