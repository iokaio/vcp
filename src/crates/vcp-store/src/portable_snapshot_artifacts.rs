// SPDX-License-Identifier: Apache-2.0
//! Paged neutral-history/2 artifact inventory. The caller holds the native
//! snapshot and artifact pins throughout capture. Staged files remain private
//! until full archive replay, input validation and restore authority rebinding.
use crate::{
    artifact::{read_bounded, reject_link, Spool, CHUNK_BYTES, MAX_CHUNKS},
    contract::Collection,
    history_index::{Entry, Pages, Root, Table},
    private_paths::Directory,
    CurrentStateView, Error, Result,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use vcp_domain::{
    artifact::{ArtifactDescriptor, CaptureState},
    ArtifactId, WorkspaceId,
};
use vcp_protocol::digest_bytes;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Role {
    Specification,
    Seal,
    Chunk { index: u64 },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Part {
    artifact: ArtifactId,
    role: Role,
    digest: String,
    bytes: u64,
}
impl Part {
    fn key(&self) -> String {
        let suffix = match self.role {
            Role::Specification => "0".into(),
            Role::Seal => "1".into(),
            Role::Chunk { index } => format!("2/{index:020}"),
        };
        format!("{}/{suffix}", self.artifact.as_str())
    }
    fn name(&self) -> String {
        match self.role {
            Role::Specification => "spec.json".into(),
            Role::Seal => "seal.json".into(),
            Role::Chunk { index } => format!("{index:020}-{}.chunk", self.digest),
        }
    }
    fn limit(&self) -> usize {
        match self.role {
            Role::Specification => 16384,
            Role::Seal => 32768,
            Role::Chunk { .. } => CHUNK_BYTES,
        }
    }
    fn validate(&self) -> Result<()> {
        if self.digest.len() != 64
            || !self
                .digest
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            || self.bytes == 0
            || self.bytes > self.limit() as u64
            || matches!(self.role, Role::Chunk { index } if index >= MAX_CHUNKS)
        {
            return Err(Error::Corruption("archive artifact part"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Artifacts {
    version: u32,
    parts: Root,
}
impl Artifacts {
    pub(crate) async fn capture(
        current: CurrentStateView<'_>,
        workspace: &WorkspaceId,
        spool: &Spool,
        forbidden: &[PathBuf],
        pages: &mut impl Pages,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Self> {
        let mut parts = Root::empty(Table::ArchiveParts);
        for row in current
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
        {
            check()?;
            if &row.workspace != workspace {
                return Err(Error::Access);
            }
            let descriptor: ArtifactDescriptor = row.decode()?;
            descriptor.validate()?;
            if descriptor.state == CaptureState::Purged {
                continue;
            }
            let dir = spool.root().join(descriptor.spec.id.as_str());
            let _directory = Directory::open(&dir, forbidden)?;
            reject_link(&dir.join("owner.lock"))?;
            let lock = fs::File::open(dir.join("owner.lock"))?;
            lock.try_lock_shared()
                .map_err(|_| Error::Conflict("quiesce artifact writer before snapshot"))?;
            // Pending descriptors are exact cut commitments here, not merely
            // the retained-prefix contract used by interactive Spool::verify.
            if spool.inspect(&descriptor.spec.id)? != descriptor {
                return Err(Error::Conflict(
                    "artifact advanced beyond snapshot cut; rebuild snapshot",
                ));
            }
            let mut count = 0u64;
            for entry in fs::read_dir(&dir)? {
                check()?;
                let entry = entry?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| Error::Corruption("artifact filename"))?;
                let role = match name.as_str() {
                    "spec.json" => Role::Specification,
                    "seal.json" => Role::Seal,
                    _ if name.ends_with(".chunk") => {
                        let (index, _) = name
                            .trim_end_matches(".chunk")
                            .split_once('-')
                            .ok_or(Error::Corruption("chunk name"))?;
                        Role::Chunk {
                            index: index
                                .parse()
                                .map_err(|_| Error::Corruption("chunk index"))?,
                        }
                    }
                    _ => continue,
                };
                count += 1;
                if count > MAX_CHUNKS + 2 {
                    return Err(Error::Limit("archive artifact parts"));
                }
                let mut part = Part {
                    artifact: descriptor.spec.id.clone(),
                    role,
                    digest: String::new(),
                    bytes: 0,
                };
                let bytes = read_bounded(&entry.path(), part.limit())?;
                part.digest = digest_bytes(&bytes);
                part.bytes = bytes.len() as u64;
                part.validate()?;
                pages.write(&part.digest, &bytes).await?;
                parts = parts
                    .insert(
                        pages,
                        Entry {
                            key: part.key(),
                            value: serde_json::to_value(&part)?,
                        },
                    )
                    .await?;
            }
            if spool.inspect(&descriptor.spec.id)? != descriptor {
                return Err(Error::Conflict("artifact changed during snapshot"));
            }
        }
        check()?;
        Ok(Self { version: 2, parts })
    }
    /// Copies only the final reachable inventory and content objects; obsolete
    /// persistent insertion pages never become transport inventory members.
    pub(crate) async fn copy_objects(
        &self,
        source: &mut impl Pages,
        destination: &mut impl Pages,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        self.parts.copy_pages(source, destination, check).await?;
        let mut after = None;
        let mut count = 0u64;
        loop {
            check()?;
            let rows = self.parts.page(source, after.as_deref(), 64).await?;
            if rows.is_empty() {
                break;
            }
            for row in &rows {
                let part = part(row)?;
                let bytes = read_part(source, &part).await?;
                check()?;
                destination.write(&part.digest, &bytes).await?;
                count += 1;
            }
            after = rows.last().map(|r| r.key.clone());
        }
        if count != self.parts.count() {
            return Err(Error::Corruption("archive artifact inventory count"));
        }
        Ok(())
    }
    pub(crate) async fn stage(
        &self,
        current: CurrentStateView<'_>,
        workspace: &WorkspaceId,
        pages: &mut impl Pages,
        root: &Path,
        forbidden: &[PathBuf],
        artifact_limit: u64,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Spool> {
        self.validate()?;
        check()?;
        if !root.exists() {
            fs::create_dir(root)?;
        }
        let _root = Directory::open(root, forbidden)?;
        let mut after = None;
        let mut count = 0u64;
        loop {
            check()?;
            let rows = self.parts.page(pages, after.as_deref(), 64).await?;
            if rows.is_empty() {
                break;
            }
            for row in &rows {
                check()?;
                let part = part(row)?;
                let descriptor: ArtifactDescriptor = current
                    .record(Collection::Artifact, part.artifact.as_str(), workspace)?
                    .decode()?;
                if descriptor.state == CaptureState::Purged {
                    return Err(Error::Corruption("purged archive bytes"));
                }
                let bytes = read_part(pages, &part).await?;
                let dir = root.join(part.artifact.as_str());
                if !dir.exists() {
                    fs::create_dir(&dir)?;
                }
                let _directory = Directory::open(&dir, forbidden)?;
                crate::restore_stage::immutable(&dir.join(part.name()), &bytes)?;
                count += 1;
            }
            after = rows.last().map(|r| r.key.clone());
        }
        if count != self.parts.count() {
            return Err(Error::Corruption("archive artifact inventory count"));
        }
        let spool = Spool::open(root, forbidden, artifact_limit)?;
        for row in current
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
        {
            check()?;
            if &row.workspace != workspace {
                return Err(Error::Access);
            }
            let descriptor: ArtifactDescriptor = row.decode()?;
            if descriptor.state == CaptureState::Purged {
                continue;
            }
            let dir = root.join(descriptor.spec.id.as_str());
            let _directory = Directory::open(&dir, forbidden)?;
            for name in ["owner.lock", "snapshot.lock"] {
                crate::restore_stage::immutable(&dir.join(name), &[])?;
            }
            spool.verify(&descriptor)?;
            if spool.inspect(&descriptor.spec.id)? != descriptor {
                return Err(Error::Corruption("archive artifact cut differs"));
            }
        }
        check()?;
        Ok(spool)
    }
    fn validate(&self) -> Result<()> {
        if self.version != 2 {
            return Err(Error::Incompatible);
        }
        self.parts.validate_table(Table::ArchiveParts)
    }
}
fn part(row: &Entry) -> Result<Part> {
    let part: Part = serde_json::from_value(row.value.clone())?;
    part.validate()?;
    if part.key() != row.key {
        return Err(Error::Corruption("archive artifact part identity"));
    }
    Ok(part)
}
async fn read_part(pages: &mut impl Pages, part: &Part) -> Result<Vec<u8>> {
    let bytes = pages.read(&part.digest, part.limit()).await?;
    if bytes.len() as u64 != part.bytes || digest_bytes(&bytes) != part.digest {
        return Err(Error::Corruption("archive artifact bytes"));
    }
    Ok(bytes)
}
#[cfg(test)]
#[path = "portable_snapshot_artifacts_tests.rs"]
mod tests;
