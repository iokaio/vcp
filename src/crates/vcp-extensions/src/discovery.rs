// SPDX-License-Identifier: Apache-2.0
use crate::{skill_manifest::*, Error, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
use vcp_domain::Revision;
use vcp_repository::{FileVersion, Root};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub max_depth: usize,
    pub max_entries: usize,
    pub max_descriptors: usize,
    pub descriptor_bytes: u64,
    pub total_descriptor_bytes: u64,
    pub body_bytes: u64,
    pub resource_bytes: u64,
    pub total_activation_bytes: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_depth: 4,
            max_entries: 8192,
            max_descriptors: 1024,
            descriptor_bytes: 16 * 1024,
            total_descriptor_bytes: 4 * 1024 * 1024,
            body_bytes: 256 * 1024,
            resource_bytes: 256 * 1024,
            total_activation_bytes: 1024 * 1024,
        }
    }
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        if self.max_depth > 16
            || !(1..=65536).contains(&self.max_entries)
            || !(1..=4096).contains(&self.max_descriptors)
            || !(1..=65536).contains(&self.descriptor_bytes)
            || !(1..=16 * 1024 * 1024).contains(&self.total_descriptor_bytes)
            || !(1..=4 * 1024 * 1024).contains(&self.body_bytes)
            || !(1..=4 * 1024 * 1024).contains(&self.resource_bytes)
            || !(1..=16 * 1024 * 1024).contains(&self.total_activation_bytes)
        {
            return Err(Error::Limit("configured discovery/activation limits"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadCounts {
    pub directory_entries: u64,
    pub descriptors: u64,
    pub descriptor_bytes: u64,
    pub bodies: u64,
    pub body_bytes: u64,
    pub resources: u64,
    pub resource_bytes: u64,
    pub revalidations: u64,
    pub revalidation_bytes: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveredSkill {
    pub qualified_id: String,
    pub source_id: String,
    pub source_kind: SourceKind,
    pub package: String,
    pub descriptor: SkillDescriptor,
    pub descriptor_version: FileVersion,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub source_id: String,
    pub path: String,
    pub code: String,
    pub message: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub registry_digest: String,
    pub revision: Revision,
    pub skills: Vec<DiscoveredSkill>,
    pub diagnostics: Vec<Diagnostic>,
    pub disabled: BTreeSet<String>,
    pub reads: ReadCounts,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchContext {
    pub environment: String,
    pub tools: BTreeSet<String>,
    /// Caller-observed project cues, not paths this module will traverse.
    pub cues: BTreeSet<String>,
}
impl MatchContext {
    pub fn validate(&self) -> Result<()> {
        text(&self.environment, 128)?;
        if self.tools.len() > 1024 || self.cues.len() > 1024 {
            return Err(Error::Limit("match context"));
        }
        for value in self.tools.iter().chain(&self.cues) {
            text(value, 128)?;
        }
        Ok(())
    }
}
impl DiscoveredSkill {
    pub fn compatible(&self, context: &MatchContext) -> bool {
        (self.descriptor.environments.is_empty()
            || self.descriptor.environments.contains(&context.environment))
            && self.descriptor.required_tools.is_subset(&context.tools)
    }
    pub fn matches(&self, context: &MatchContext) -> bool {
        self.compatible(context)
            && (self.descriptor.cues.is_empty() || !self.descriptor.cues.is_disjoint(&context.cues))
    }
}
impl Catalog {
    /// Cached descriptors carry exact file versions. Activation revalidates the
    /// chosen descriptor; this catalog never contains body or resource bytes.
    pub fn digest(&self) -> Result<String> {
        crate::digest(&(
            &self.registry_digest,
            self.revision,
            &self.skills,
            &self.diagnostics,
            &self.disabled,
        ))
    }
    /// Explicit selection bypasses cue suggestions, never environment/tool requirements.
    pub fn resolve(&self, id: &str, context: &MatchContext) -> Result<&DiscoveredSkill> {
        self.resolve_with_disabled(id, context, &BTreeSet::new())
    }
    fn resolve_with_disabled(
        &self,
        id: &str,
        context: &MatchContext,
        disabled: &BTreeSet<String>,
    ) -> Result<&DiscoveredSkill> {
        context.validate()?;
        let candidates: Vec<_> = self
            .skills
            .iter()
            .filter(|skill| skill.qualified_id == id || skill.descriptor.id == id)
            .collect();
        // A disabled override must not hide a lower-precedence enabled copy.
        let mut selected: Vec<_> = candidates
            .iter()
            .copied()
            .filter(|skill| {
                !self.disabled.contains(&skill.qualified_id)
                    && !disabled.contains(&skill.qualified_id)
            })
            .collect();
        if selected.is_empty() {
            if let Some(skill) = candidates.first() {
                return Err(Error::Unavailable(format!(
                    "disabled skill {}",
                    skill.qualified_id
                )));
            }
        }
        let precedence = selected.iter().map(|skill| skill.source_kind).max();
        selected.retain(|skill| Some(skill.source_kind) == precedence);
        let skill = match selected.as_slice() {
            [skill] => *skill,
            [] => return Err(Error::Unavailable(format!("missing skill {id}"))),
            _ => {
                let candidates = selected
                    .iter()
                    .take(8)
                    .map(|skill| skill.qualified_id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(Error::Unavailable(format!(
                    "ambiguous skill {id}; select a qualified identity from: {candidates}{}",
                    if selected.len() > 8 {
                        format!(" ({} more; use /skills list)", selected.len() - 8)
                    } else {
                        String::new()
                    }
                )));
            }
        };
        if !skill.compatible(context) {
            return Err(Error::Unavailable(format!(
                "{} requires environment {:?} and tools {:?}",
                skill.qualified_id, skill.descriptor.environments, skill.descriptor.required_tools
            )));
        }
        Ok(skill)
    }
    pub fn matching(&self, context: &MatchContext) -> Result<Vec<&DiscoveredSkill>> {
        self.matching_with_disabled(context, &BTreeSet::new())
    }
    /// Apply task-local disables before precedence, just like registry disables.
    /// Filtering the selected result afterwards would hide an enabled fallback.
    pub fn matching_with_disabled(
        &self,
        context: &MatchContext,
        disabled: &BTreeSet<String>,
    ) -> Result<Vec<&DiscoveredSkill>> {
        context.validate()?;
        Ok(self
            .skills
            .iter()
            .filter(|skill| {
                skill.matches(context)
                    && self
                        .resolve_with_disabled(&skill.descriptor.id, context, disabled)
                        .is_ok_and(|selected| selected.qualified_id == skill.qualified_id)
            })
            .collect())
    }
}
struct Walker<'a> {
    source: &'a SkillSource,
    root: Root,
    limits: &'a Limits,
    catalog: &'a mut Catalog,
    attempted: usize,
    entries: u64,
    descriptor_bytes: u64,
}
impl Walker<'_> {
    fn diagnostic(&mut self, path: &Path, code: &str, error: impl std::fmt::Display) {
        // Input lengths are bounded before diagnostic construction.
        self.catalog.diagnostics.push(Diagnostic {
            source_id: self.source.id.clone(),
            path: path.to_string_lossy().replace('\\', "/"),
            code: code.into(),
            message: error.to_string().chars().take(2048).collect(),
        });
    }
    fn walk(&mut self, relative: &Path, depth: usize) -> Result<()> {
        let held = self
            .root
            .hold((!relative.as_os_str().is_empty()).then_some(relative), true)?;
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(self.root.path().join(relative))? {
            self.catalog.reads.directory_entries += 1;
            self.entries += 1;
            if self.entries > self.limits.max_entries as u64 {
                return Err(Error::Limit("directory entries"));
            }
            entries.push(entry?);
        }
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = relative.join(entry.file_name());
            if let Err(error) = vcp_repository::path::relative(&path) {
                self.diagnostic(&path, "invalid_path", error);
                continue;
            }
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                self.diagnostic(&path, "link_denied", "linked skill content is not followed");
                continue;
            }
            if kind.is_dir() {
                if depth >= self.limits.max_depth {
                    self.diagnostic(
                        &path,
                        "depth_limit",
                        "directory omitted at configured depth",
                    );
                    continue;
                }
                if let Err(error) = self.walk(&path, depth + 1) {
                    if matches!(error, Error::Limit(_)) {
                        return Err(error);
                    }
                    self.diagnostic(&path, "directory_denied", error);
                }
            } else if entry.file_name() == DESCRIPTOR_NAME {
                self.attempted += 1;
                if self.attempted > self.limits.max_descriptors {
                    return Err(Error::Limit("descriptor count"));
                }
                self.descriptor(&path)?;
            }
        }
        drop(held);
        Ok(())
    }
    fn descriptor(&mut self, path: &Path) -> Result<()> {
        let source = match self.root.read(path, self.limits.descriptor_bytes) {
            Ok(source) => source,
            Err(error) => {
                self.diagnostic(path, "descriptor_read", error);
                return Ok(());
            }
        };
        self.catalog.reads.descriptors += 1;
        self.catalog.reads.descriptor_bytes += source.bytes.len() as u64;
        self.descriptor_bytes += source.bytes.len() as u64;
        if self.descriptor_bytes > self.limits.total_descriptor_bytes {
            return Err(Error::Limit("total descriptor bytes"));
        }
        let descriptor: SkillDescriptor =
            match serde_json::from_slice::<SkillDescriptor>(&source.bytes)
                .map_err(Error::from)
                .and_then(|value| {
                    value.validate()?;
                    Ok(value)
                }) {
                Ok(value) => value,
                Err(error) => {
                    self.diagnostic(path, "malformed_descriptor", error);
                    return Ok(());
                }
            };
        let package = path
            .parent()
            .unwrap_or(Path::new(""))
            .to_string_lossy()
            .replace('\\', "/");
        let qualified_id = format!(
            "{}::{}::{}",
            self.source.id,
            if package.is_empty() { "." } else { &package },
            descriptor.id
        );
        self.catalog.skills.push(DiscoveredSkill {
            qualified_id,
            source_id: self.source.id.clone(),
            source_kind: self.source.kind,
            package,
            descriptor,
            descriptor_version: source.version,
        });
        Ok(())
    }
}
pub fn discover(registry: &SourceRegistry, limits: &Limits) -> Result<Catalog> {
    limits.validate()?;
    let mut catalog = Catalog {
        registry_digest: registry.digest()?,
        revision: registry.revision,
        skills: vec![],
        diagnostics: vec![],
        disabled: registry.disabled.clone(),
        reads: ReadCounts::default(),
    };
    let mut sources: Vec<_> = registry
        .sources
        .iter()
        .filter(|source| source.enabled)
        .collect();
    sources.sort_by(|a, b| a.id.cmp(&b.id));
    // Each source has its own budget. A source that cannot be opened or that
    // exceeds its limits contributes no skills and one diagnostic, so it cannot
    // take down discovery for the others. The builtin source stays fail-closed
    // because catalog verification rejects any diagnostic from it.
    for source in sources {
        let before = catalog.skills.len();
        let outcome = match Root::open(source.root.clone(), &source.path) {
            Ok(root) => Walker {
                source,
                root,
                limits,
                catalog: &mut catalog,
                attempted: 0,
                entries: 0,
                descriptor_bytes: 0,
            }
            .walk(&PathBuf::new(), 0),
            Err(error) => Err(error.into()),
        };
        if let Err(error) = outcome {
            catalog.skills.truncate(before);
            catalog.diagnostics.push(Diagnostic {
                source_id: source.id.clone(),
                path: String::new(),
                code: if matches!(error, Error::Limit(_)) {
                    "source_limit"
                } else {
                    "source_unavailable"
                }
                .into(),
                message: error.to_string().chars().take(2048).collect(),
            });
        }
    }
    catalog
        .skills
        .sort_by(|a, b| a.qualified_id.cmp(&b.qualified_id));
    let mut duplicates: BTreeMap<(&str, SourceKind), Vec<&str>> = BTreeMap::new();
    for skill in catalog
        .skills
        .iter()
        .filter(|skill| !catalog.disabled.contains(&skill.qualified_id))
    {
        duplicates
            .entry((&skill.descriptor.id, skill.source_kind))
            .or_default()
            .push(&skill.qualified_id);
    }
    // Cross-precedence overrides are legitimate but should never be silent.
    let mut kinds: BTreeMap<&str, Vec<&DiscoveredSkill>> = BTreeMap::new();
    for skill in catalog
        .skills
        .iter()
        .filter(|skill| !catalog.disabled.contains(&skill.qualified_id))
    {
        kinds.entry(&skill.descriptor.id).or_default().push(skill);
    }
    let mut shadowed = Vec::new();
    for (id, skills) in kinds {
        let Some(top) = skills.iter().map(|skill| skill.source_kind).max() else {
            continue;
        };
        for skill in skills.iter().filter(|skill| skill.source_kind < top) {
            shadowed.push(Diagnostic {
                source_id: String::new(),
                path: String::new(),
                code: "shadowed".into(),
                message: format!(
                    "{id}: {} is shadowed by a higher-precedence source; select it by qualified identity",
                    skill.qualified_id
                ),
            });
        }
    }
    for ((id, _), qualified) in duplicates {
        if qualified.len() > 1 {
            catalog.diagnostics.push(Diagnostic {
                source_id: String::new(),
                path: String::new(),
                code: "ambiguous_id".into(),
                message: format!(
                    "{id}: {} definitions at same precedence; use qualified identity",
                    qualified.len()
                ),
            });
        }
    }
    catalog.diagnostics.extend(shadowed);
    Ok(catalog)
}
