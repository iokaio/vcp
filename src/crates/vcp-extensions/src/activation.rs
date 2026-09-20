// SPDX-License-Identifier: Apache-2.0
use crate::{
    discovery::{Catalog, Limits, MatchContext, ReadCounts},
    skill_manifest::*,
    Error, Result,
};
use std::path::Path;
use vcp_domain::Revision;
use vcp_repository::{FileVersion, Root, Source};

/// Capturable content, not executable configuration or trusted instructions.
pub struct ActivatedSkill {
    pub qualified_id: String,
    pub version: String,
    pub reason: String,
    pub registry_digest: String,
    pub registry_revision: Revision,
    pub source_id: String,
    pub source: SkillSource,
    pub descriptor_version: FileVersion,
    pub body: Source,
    pub resources: Vec<Source>,
    pub reads: ReadCounts,
}
pub fn activate(
    registry: &SourceRegistry,
    catalog: &Catalog,
    id: &str,
    context: &MatchContext,
    reason: &str,
    limits: &Limits,
) -> Result<ActivatedSkill> {
    limits.validate()?;
    text(reason, 2048)?;
    if registry.digest()? != catalog.registry_digest || registry.revision != catalog.revision {
        return Err(Error::Stale);
    }
    let skill = catalog.resolve(id, context)?;
    let source = registry
        .sources
        .iter()
        .find(|s| s.id == skill.source_id && s.enabled)
        .ok_or(Error::Stale)?;
    let descriptor_path = Path::new(&skill.package).join(DESCRIPTOR_NAME);
    let qualified = format!(
        "{}::{}::{}",
        source.id,
        if skill.package.is_empty() {
            "."
        } else {
            &skill.package
        },
        skill.descriptor.id
    );
    if source.kind != skill.source_kind
        || qualified != skill.qualified_id
        || vcp_repository::path::relative(&descriptor_path)? != skill.descriptor_version.path
    {
        return Err(Error::Stale);
    }
    let root = Root::open(source.root.clone(), &source.path)?;
    // The descriptor cache is usable only while its selected revision is current.
    let descriptor = root.read(
        Path::new(&skill.descriptor_version.path),
        limits.descriptor_bytes,
    )?;
    if descriptor.version != skill.descriptor_version {
        return Err(Error::Stale);
    }
    let parsed: SkillDescriptor = serde_json::from_slice(&descriptor.bytes)?;
    parsed.validate()?;
    if parsed != skill.descriptor {
        return Err(Error::Stale);
    }
    let mut reads = ReadCounts {
        descriptors: 1,
        descriptor_bytes: descriptor.bytes.len() as u64,
        ..Default::default()
    };
    let mut load = |content: &ContentRef, limit: u64, is_body: bool| -> Result<Source> {
        content.validate()?;
        let path = Path::new(&skill.package).join(&content.path);
        let value = root.read(&path, limit)?;
        if value.version.sha256 != content.sha256 {
            return Err(Error::Stale);
        }
        if is_body {
            reads.bodies += 1;
            reads.body_bytes += value.bytes.len() as u64;
        } else {
            reads.resources += 1;
            reads.resource_bytes += value.bytes.len() as u64;
        }
        if reads.body_bytes + reads.resource_bytes > limits.total_activation_bytes {
            return Err(Error::Limit("total activation bytes"));
        }
        Ok(value)
    };
    let body = load(&parsed.body, limits.body_bytes, true)?;
    // Bodies are instructions; binary resources may still be captured as artifacts.
    std::str::from_utf8(&body.bytes)
        .map_err(|_| Error::Metadata("skill body must be UTF-8".into()))?;
    let resources = parsed
        .resources
        .iter()
        .map(|resource| load(resource, limits.resource_bytes, false))
        .collect::<Result<Vec<_>>>()?;
    let mut activated = ActivatedSkill {
        qualified_id: skill.qualified_id.clone(),
        version: parsed.version,
        reason: reason.into(),
        registry_digest: catalog.registry_digest.clone(),
        registry_revision: catalog.revision,
        source_id: source.id.clone(),
        source: source.clone(),
        descriptor_version: descriptor.version,
        body,
        resources,
        reads,
    };
    revalidate(registry, &activated)?;
    activated.reads.revalidations = 2 + activated.resources.len() as u64;
    activated.reads.revalidation_bytes = activated.descriptor_version.bytes.get()
        + activated.body.version.bytes.get()
        + activated
            .resources
            .iter()
            .map(|r| r.version.bytes.get())
            .sum::<u64>();
    Ok(activated)
}
/// Recheck only the activated source and content, so unrelated source updates do
/// not invalidate this dependency. Current registry disable/removal still wins.
pub fn revalidate(registry: &SourceRegistry, activated: &ActivatedSkill) -> Result<()> {
    registry.validate()?;
    let source = registry
        .sources
        .iter()
        .find(|s| s.id == activated.source_id && s.enabled)
        .ok_or(Error::Stale)?;
    if source != &activated.source || registry.disabled.contains(&activated.qualified_id) {
        return Err(Error::Stale);
    }
    let root = Root::open(source.root.clone(), &source.path)?;
    root.revalidate(&activated.descriptor_version)?;
    root.revalidate(&activated.body.version)?;
    for resource in &activated.resources {
        root.revalidate(&resource.version)?;
    }
    Ok(())
}
