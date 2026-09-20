// SPDX-License-Identifier: Apache-2.0
//! Qualified retained vectors are reusable data, never authorization for their
//! old source rows. Every reused identity must equal a fresh authorized chunk.
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};
use vcp_domain::{
    artifact::{ArtifactDescriptor, CaptureState},
    search::Generation,
    workspace::Workspace,
    *,
};
use vcp_memory::{
    embedding::{self, Encoded, Specification},
    retention::{self, Target},
    search_record::Inventory,
    vector::Component,
};
use vcp_store::{
    contract::{key, Collection},
    Store,
};
type Result<T> = std::result::Result<T, String>;
const MAX_CANDIDATES: usize = 4;
const MAX_BYTES: u64 = 8 * 1024 * 1024;

pub(super) struct Captured {
    candidates: Vec<ArtifactDescriptor>,
    spool: vcp_store::artifact::Spool,
}
impl Captured {
    pub(super) fn available(&self) -> bool {
        !self.candidates.is_empty()
    }
}
pub(super) struct Reused {
    pub path: PathBuf,
    pub checksum: String,
    pub artifacts: Vec<ArtifactId>,
    owned: Owned,
}
impl Reused {
    pub(super) fn finish(self) -> Result<()> {
        self.owned.finish()
    }
}
struct Owned {
    root: PathBuf,
    files: Vec<File>,
    directory: Option<File>,
}
impl Owned {
    fn create(root: PathBuf) -> Result<Self> {
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let handle = directory(&root)?;
        Ok(Self {
            root,
            files: Vec::new(),
            directory: Some(handle),
        })
    }
    fn file(&mut self, name: &str) -> Result<(PathBuf, &mut File)> {
        use std::os::windows::fs::OpenOptionsExt;
        let path = self.root.join(name);
        let file = OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .access_mode(0xC001_0000)
            .share_mode(1 | 2 | 4)
            .custom_flags(0x0400_0000 | 0x0020_0000)
            .open(&path)
            .map_err(|e| e.to_string())?;
        self.files.push(file);
        Ok((
            path,
            self.files.last_mut().ok_or("owned vector file missing")?,
        ))
    }
    fn cleanup(&mut self) -> Result<()> {
        use std::os::windows::io::AsRawHandle;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn SetFileInformationByHandle(
                handle: *mut std::ffi::c_void,
                class: u32,
                info: *const std::ffi::c_void,
                size: u32,
            ) -> i32;
        }
        self.files.clear();
        let Some(directory) = self.directory.as_ref() else {
            return Ok(());
        };
        let delete = 1u8;
        // SAFETY: the owned DELETE-capable directory handle remains pinned;
        // the native operation refuses nonempty directories. No path deletion.
        if unsafe {
            SetFileInformationByHandle(
                directory.as_raw_handle(),
                4,
                (&delete as *const u8).cast(),
                1,
            )
        } == 0
        {
            return Err(format!(
                "owned vector staging cleanup pending: {}",
                self.root.display()
            ));
        }
        self.directory.take();
        Ok(())
    }
    fn finish(mut self) -> Result<()> {
        self.cleanup()
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

/// Capture authorized descriptor metadata only; full retained payload reads and
/// graph validation occur under the caller's shared CPU/disk admission.
pub(super) fn capture(
    store: &Store,
    workspace: &Workspace,
    check: &dyn Fn() -> Result<()>,
) -> Result<Captured> {
    let specification = Specification::qualified()
        .digest()
        .map_err(|e| e.to_string())?;
    let mut generations = BTreeMap::new();
    for row in store.state().records.values() {
        check()?;
        if row.workspace == workspace.id
            && row.collection == Collection::Generation
            && row.value["document_type"] == vcp_domain::search::GENERATION
        {
            let generation: Generation = row.decode().map_err(|e| e.to_string())?;
            if generation.deletion == workspace.deletion
                && generation.embedding_specification == specification
            {
                if let Some(checksum) = generation.vector_checksum {
                    generations.insert(
                        generation.id,
                        (generation.scope, checksum, generation.canonical_watermark),
                    );
                }
            }
        }
    }
    let mut candidates = Vec::new();

    for row in store.state().records.values() {
        check()?;
        if row.workspace != workspace.id || row.collection != Collection::Artifact {
            continue;
        }
        let artifact: ArtifactDescriptor = row.decode().map_err(|e| e.to_string())?;
        if artifact.state != CaptureState::Complete
            || artifact.spec.schema != "vcp-backup-generation-component/1"
            || artifact.length.get() > 4 * 1024 * 1024
        {
            continue;
        }
        let Some(id) = artifact
            .spec
            .source
            .strip_prefix("generation:")
            .and_then(|id| GenerationId::parse(id).ok())
        else {
            continue;
        };
        let Some((scope, checksum, watermark)) = generations.get(&id) else {
            continue;
        };
        if artifact.spec.scope != *scope
            || &artifact.sha256 != checksum
            || !row
                .references
                .contains(&key(Collection::Generation, id.as_str()))
        {
            continue;
        }
        if !retention::recall_allowed(store.state(), &workspace.id, &Target::Record(row.key()))
            .map_err(|e| e.to_string())?
        {
            continue;
        }
        candidates.push((*watermark, artifact));
        candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.spec.id.cmp(&b.1.spec.id)));
        candidates.truncate(MAX_CANDIDATES);
    }
    let mut total = 0u64;
    let candidates = candidates
        .into_iter()
        .filter_map(|(_, artifact)| {
            if total + artifact.length.get() > MAX_BYTES {
                return None;
            }
            total += artifact.length.get();
            Some(artifact)
        })
        .collect();
    Ok(Captured {
        candidates,
        spool: store.spool().clone(),
    })
}
fn directory(path: &Path) -> Result<File> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    let handle = OpenOptions::new()
        .access_mode(0x0001_0081)
        .share_mode(1 | 2)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(path)
        .map_err(|e| e.to_string())?;
    let metadata = handle.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_dir() || metadata.file_attributes() & 0x400 != 0 {
        return Err("retained vector staging redirected".into());
    }
    Ok(handle)
}
pub(super) fn rebuild(
    captured: Captured,
    inventory: &Inventory,
    parent: &Path,
    remaining: &dyn Fn() -> Result<u64>,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<Reused>> {
    if captured.candidates.is_empty() || inventory.records.is_empty() {
        return Ok(None);
    }
    let expected = embedding::inventory_chunks(inventory).map_err(|e| e.to_string())?;
    if expected.len() > 1024 {
        return Err("retained vector row admission limit".into());
    }
    if expected.is_empty() {
        return Ok(None);
    }
    let wanted: BTreeMap<_, _> = expected
        .iter()
        .map(|chunk| (chunk.identity.id.as_str(), &chunk.identity))
        .collect();
    let root = parent.join(GenerationId::new().as_str());
    embedding::check(cancelled).map_err(|e| e.to_string())?;
    let mut owned = Owned::create(root)?;
    let result = (|| {
        let mut retained: BTreeMap<String, Encoded> = BTreeMap::new();
        let mut artifacts = Vec::new();
        for artifact in captured.candidates {
            embedding::check(cancelled).map_err(|e| e.to_string())?;
            if artifact.length.get() > remaining()? {
                return Err("retained vector input exceeds admitted disk budget".into());
            }
            let (path, file) = owned.file(&format!("{}.json", ArtifactId::new()))?;
            captured
                .spool
                .read(&artifact, &mut *file)
                .map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            let component = match Component::open(
                &path,
                &artifact.sha256,
                &inventory.workspace,
                &Specification::qualified(),
                cancelled,
            ) {
                Ok(value) => value,
                Err(_) if !cancelled() => continue,
                Err(error) => return Err(error.to_string()),
            };
            let mut used = false;
            for row in component.rows() {
                let Some(identity) = wanted.get(row.identity.id.as_str()) else {
                    continue;
                };
                if &row.identity != *identity {
                    return Err("retained vector identity differs from authorized source".into());
                }
                if retained
                    .get(&row.identity.id)
                    .is_some_and(|prior| prior != row)
                {
                    return Err("conflicting retained vectors for one identity".into());
                }
                retained.insert(row.identity.id.clone(), row.clone());
                used = true;
            }
            if used {
                artifacts.push(artifact.spec.id);
            }
            if retained.len() == expected.len() {
                break;
            }
        }
        if retained.len() != expected.len() {
            return Ok(None);
        }
        let rows = expected
            .iter()
            .map(|chunk| {
                retained
                    .remove(&chunk.identity.id)
                    .ok_or_else(|| "retained vector closure incomplete".to_string())
            })
            .collect::<Result<Vec<_>>>()?;
        let rebuilt = Component::build(
            inventory.workspace.clone(),
            Specification::qualified(),
            rows,
            cancelled,
        )
        .map_err(|e| e.to_string())?;
        let (path, file) = owned.file("vectors.json")?;
        let checksum = rebuilt
            .write_bounded(file, remaining()?, cancelled)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        Ok(Some((path, checksum, artifacts)))
    })();
    match result {
        Ok(Some((path, checksum, artifacts))) => Ok(Some(Reused {
            path,
            checksum,
            artifacts,
            owned,
        })),
        other => {
            owned.finish()?;
            other.map(|_| None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn native_owned_staging_cleans_success_and_abandonment_preserving_unknown_children() {
        let parent = tempfile::tempdir().unwrap();
        for explicit in [true, false] {
            let root = parent.path().join(GenerationId::new().as_str());
            let mut owned = Owned::create(root.clone()).unwrap();
            let (path, file) = owned.file("input.json").unwrap();
            file.write_all(b"retained").unwrap();
            file.sync_all().unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), b"retained");
            if explicit {
                owned.finish().unwrap();
            } else {
                drop(owned);
            }
            assert!(!root.exists());
        }
        let root = parent.path().join(GenerationId::new().as_str());
        let mut owned = Owned::create(root.clone()).unwrap();
        let (path, _) = owned.file("owned.json").unwrap();
        std::fs::write(root.join("unknown"), b"preserve").unwrap();
        assert!(owned.finish().unwrap_err().contains("cleanup pending"));
        assert!(!path.exists());
        assert_eq!(std::fs::read(root.join("unknown")).unwrap(), b"preserve");
    }
}
