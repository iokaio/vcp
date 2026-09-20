// SPDX-License-Identifier: Apache-2.0
//! Shipped catalog identity. Only metadata is embedded; instruction bodies stay lazy.
use crate::{discovery::Catalog, skill_manifest::*, Error, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};
use vcp_repository::{FileVersion, Root};

pub const SOURCE_ID: &str = "vcp-builtin";
pub const ROOT_ID: &str = "vcp-builtin";
pub const INSTALL_PATH: &str = "skills/builtin";
pub const EMBEDDED: &[u8] = include_bytes!("../../../skills/builtin/catalog.json");

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub version: String,
    pub coverage: ContentRef,
    pub skills: Vec<Entry>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub version: String,
    pub descriptor: String,
    pub descriptor_sha256: String,
    pub body: ContentRef,
    #[serde(default)]
    pub resources: Vec<ContentRef>,
    pub source: String,
    pub license: String,
}
/// Successful integrity reads only, separate from normal discovery/activation IO.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reads {
    pub metadata_files: u64,
    pub metadata_bytes: u64,
    pub descriptors: u64,
    pub descriptor_bytes: u64,
    pub revalidations: u64,
    pub revalidation_bytes: u64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Verification {
    pub manifest: Manifest,
    pub catalog_sha256: String,
    pub reads: Reads,
    pub dependencies: Vec<FileVersion>,
}

pub fn embedded() -> Result<Manifest> {
    let manifest: Manifest = serde_json::from_slice(EMBEDDED)?;
    manifest.validate()?;
    Ok(manifest)
}
impl Manifest {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 || self.skills.is_empty() || self.skills.len() > 128 {
            return Err(Error::Metadata("bundled catalog version or count".into()));
        }
        text(&self.version, 128)?;
        self.coverage.validate()?;
        let mut paths = BTreeSet::from(["catalog.json".to_owned(), self.coverage.path.clone()]);
        if paths.len() != 2 {
            return Err(Error::Metadata("catalog coverage path collision".into()));
        }
        let mut ids = BTreeSet::new();
        for entry in &self.skills {
            identifier(&entry.id)?;
            if !ids.insert(&entry.id) || entry.descriptor != format!("{}/skill.json", entry.id) {
                return Err(Error::Metadata(
                    "unique package-qualified catalog descriptors required".into(),
                ));
            }
            ContentRef {
                path: entry.descriptor.clone(),
                sha256: entry.descriptor_sha256.clone(),
            }
            .validate()?;
            text(&entry.version, 128)?;
            text(&entry.source, 2048)?;
            text(&entry.license, 128)?;
            if entry.resources.len() > 32 {
                return Err(Error::Limit("catalog resources"));
            }
            if !paths.insert(entry.descriptor.clone()) {
                return Err(Error::Metadata("catalog path collision".into()));
            }
            for content in std::iter::once(&entry.body).chain(&entry.resources) {
                content.validate()?;
                if !paths.insert(format!("{}/{}", entry.id, content.path)) {
                    return Err(Error::Metadata("catalog path collision".into()));
                }
            }
        }
        Ok(())
    }
}

/// The caller must authorize the root before invoking this native reader.
/// Verifies metadata only; neither body nor resource bytes are read here.
pub fn verify(root: &Root) -> Result<Verification> {
    let manifest = embedded()?;
    let catalog = root.read(Path::new("catalog.json"), 256 * 1024)?;
    if catalog.bytes != EMBEDDED {
        return Err(Error::Metadata(
            "installed builtin catalog differs from this binary's catalog identity".into(),
        ));
    }
    let mut result = Verification {
        manifest,
        catalog_sha256: vcp_protocol::digest_bytes(EMBEDDED),
        reads: Reads {
            metadata_files: 1,
            metadata_bytes: catalog.bytes.len() as u64,
            ..Default::default()
        },
        dependencies: vec![catalog.version],
    };
    let coverage = root.read(Path::new(&result.manifest.coverage.path), 1024 * 1024)?;
    if coverage.version.sha256 != result.manifest.coverage.sha256 {
        return Err(Error::Metadata("builtin coverage digest mismatch".into()));
    }
    result.reads.metadata_files += 1;
    result.reads.metadata_bytes += coverage.bytes.len() as u64;
    result.dependencies.push(coverage.version);
    for entry in &result.manifest.skills {
        let source = root.read(Path::new(&entry.descriptor), 64 * 1024)?;
        if source.version.sha256 != entry.descriptor_sha256 {
            return Err(Error::Metadata(format!(
                "builtin descriptor digest mismatch: {}",
                entry.id
            )));
        }
        let descriptor: SkillDescriptor = serde_json::from_slice(&source.bytes)?;
        descriptor.validate()?;
        check_entry(entry, &descriptor)?;
        result.reads.descriptors += 1;
        result.reads.descriptor_bytes += source.bytes.len() as u64;
        result.dependencies.push(source.version);
    }
    Ok(result)
}
fn check_entry(entry: &Entry, descriptor: &SkillDescriptor) -> Result<()> {
    if descriptor.id != entry.id
        || descriptor.version != entry.version
        || descriptor.source != entry.source
        || descriptor.license != entry.license
        || descriptor.body != entry.body
        || descriptor.resources != entry.resources
    {
        return Err(Error::Metadata(format!(
            "builtin descriptor and catalog disagree: {}",
            entry.id
        )));
    }
    Ok(())
}
/// Rejects descriptors inserted into the reserved source outside the shipped inventory.
pub fn verify_discovery(verified: &Verification, catalog: &Catalog) -> Result<()> {
    if catalog
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.source_id == SOURCE_ID)
    {
        return Err(Error::Metadata(
            "builtin discovery contains an invalid or unexpected package".into(),
        ));
    }
    let selected: Vec<_> = catalog
        .skills
        .iter()
        .filter(|skill| skill.source_id == SOURCE_ID)
        .collect();
    if selected.len() != verified.manifest.skills.len() {
        return Err(Error::Metadata(
            "builtin discovered inventory differs from shipped catalog".into(),
        ));
    }
    for skill in selected {
        let entry = verified
            .manifest
            .skills
            .iter()
            .find(|entry| entry.id == skill.descriptor.id)
            .ok_or_else(|| Error::Metadata("unlisted builtin skill".into()))?;
        if skill.source_kind != SourceKind::Builtin
            || skill.package != entry.id
            || skill.descriptor_version.path != entry.descriptor
            || skill.descriptor_version.sha256 != entry.descriptor_sha256
        {
            return Err(Error::Metadata(
                "builtin discovery identity mismatch".into(),
            ));
        }
        check_entry(entry, &skill.descriptor)?;
    }
    Ok(())
}
/// Successful metadata rereads are accounted independently from body activation.
pub fn revalidate(root: &Root, verified: &Verification) -> Result<Reads> {
    let mut reads = Reads::default();
    for file in &verified.dependencies {
        root.revalidate(file)?;
        reads.revalidations += 1;
        reads.revalidation_bytes += file.bytes.get();
    }
    Ok(reads)
}
