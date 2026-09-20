// SPDX-License-Identifier: Apache-2.0
use super::*;

/// Validated frozen component bytes. Keeps the reader pin until the caller has
/// captured the bytes and their direct canonical source references together.
pub struct SnapshotFiles {
    view: View,
    files: BTreeMap<String, Vec<u8>>,
    sources: BTreeSet<String>,
}
impl SnapshotFiles {
    pub fn manifest(&self) -> &Generation {
        &self.view.manifest
    }
    pub fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }
    pub fn source_references(&self) -> &BTreeSet<String> {
        &self.sources
    }
}
impl Publisher {
    pub fn snapshot_files(&self, view: View, cancel: &AtomicBool) -> Result<SnapshotFiles> {
        if !Arc::ptr_eq(&view._pin.pins, &self.pins) || view._pin.id != view.manifest.id {
            return Err(Error::Conflict("snapshot generation manager mismatch"));
        }
        embedding::check(&|| cancel.load(Ordering::Acquire))?;
        let directory = self.directory(&view.manifest.id)?;
        let _frozen = freeze_files(&directory)?;
        let saved: Generation =
            serde_json::from_slice(&bounded(&directory.join("manifest.json"), 1024 * 1024)?)?;
        if saved != view.manifest {
            return Err(Error::Conflict("snapshot generation manifest changed"));
        }
        let mut files = BTreeMap::new();
        let inventory = bounded(&directory.join("inventory.json"), 4 * 1024 * 1024)?;
        if digest_bytes(&inventory) != saved.inventory_checksum {
            return Err(Error::Conflict("snapshot inventory checksum"));
        }
        let decoded: Inventory = serde_json::from_slice(&inventory)?;
        decoded.validate()?;
        if decoded != view.inventory {
            return Err(Error::Conflict("snapshot inventory changed"));
        }
        files.insert("inventory.json".into(), inventory);
        let lexical = bounded(&directory.join("lexical/vcp-lexical.json"), 1024 * 1024)?;
        if digest_bytes(&lexical) != saved.lexical_checksum {
            return Err(Error::Conflict("snapshot lexical checksum"));
        }
        let manifest: serde_json::Value = serde_json::from_slice(&lexical)?;
        let components = manifest["components"]
            .as_array()
            .ok_or(Error::Conflict("snapshot lexical inventory"))?;
        if components.len() > 256 {
            return Err(Error::Conflict("snapshot lexical component limit"));
        }
        for component in components {
            embedding::check(&|| cancel.load(Ordering::Acquire))?;
            let name = component["name"]
                .as_str()
                .ok_or(Error::Conflict("snapshot component name"))?;
            if name.is_empty() || name.contains(['/', '\\', ':']) || name == "." || name == ".." {
                return Err(Error::Conflict("snapshot component path"));
            }
            let bytes = bounded(&directory.join("lexical").join(name), 4 * 1024 * 1024)?;
            if component["bytes"].as_u64() != Some(bytes.len() as u64)
                || component["sha256"].as_str() != Some(digest_bytes(&bytes).as_str())
            {
                return Err(Error::Conflict("snapshot component checksum"));
            }
            if files.insert(format!("lexical/{name}"), bytes).is_some() {
                return Err(Error::Conflict("snapshot duplicate component"));
            }
            if files.values().map(Vec::len).sum::<usize>() > 16 * 1024 * 1024 {
                return Err(Error::Conflict("snapshot generation byte limit"));
            }
        }
        files.insert("lexical/vcp-lexical.json".into(), lexical);
        if let Some(expected) = &saved.vector_checksum {
            let bytes = bounded(&directory.join("vectors.json"), 4 * 1024 * 1024)?;
            if digest_bytes(&bytes) != *expected {
                return Err(Error::Conflict("snapshot vector checksum"));
            }
            files.insert("vectors.json".into(), bytes);
        }
        if files.values().map(Vec::len).sum::<usize>() > 16 * 1024 * 1024 {
            return Err(Error::Conflict("snapshot generation byte limit"));
        }
        let sources = decoded
            .records
            .iter()
            .map(|record| match &record.source {
                TextSource::Artifact { id } => key(Collection::Artifact, id.as_str()),
                TextSource::Claim { version, .. } => key(Collection::Claim, version.as_str()),
            })
            .collect();
        Ok(SnapshotFiles {
            view,
            files,
            sources,
        })
    }
}
