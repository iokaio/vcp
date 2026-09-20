// SPDX-License-Identifier: Apache-2.0
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use vcp_domain::Revision;
use vcp_repository::RootIdentity;

pub const SCHEMA_VERSION: u32 = 1;
pub const DESCRIPTOR_NAME: &str = "skill.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Builtin,
    User,
    Workspace,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillSource {
    pub id: String,
    pub kind: SourceKind,
    pub enabled: bool,
    pub root: RootIdentity,
    /// Explicit package collection root. Never inferred from the user's home.
    pub path: PathBuf,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRegistry {
    pub version: u32,
    pub revision: Revision,
    pub sources: Vec<SkillSource>,
    #[serde(default)]
    pub disabled: BTreeSet<String>,
}
impl SourceRegistry {
    pub fn validate(&self) -> Result<()> {
        if self.version != SCHEMA_VERSION || self.sources.len() > 32 || self.disabled.len() > 4096 {
            return Err(Error::Limit("source registry version or count"));
        }
        for id in &self.disabled {
            text(id, 2048)?;
        }
        let mut ids = BTreeSet::new();
        let mut roots = BTreeSet::new();
        for source in &self.sources {
            identifier(&source.id)?;
            if !ids.insert(&source.id)
                || !roots.insert(&source.root.root)
                || !source.path.is_absolute()
                || source.path.as_os_str().len() > 4096
                || source
                    .path
                    .components()
                    .any(|p| matches!(p, std::path::Component::ParentDir))
            {
                return Err(Error::Metadata(
                    "unique source/root IDs and explicit absolute local roots required".into(),
                ));
            }
            text(&source.root.repository, 256)?;
            text(&source.root.worktree, 256)?;
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String> {
        self.validate()?;
        let mut ordered = self.clone();
        ordered.sources.sort_by(|a, b| a.id.cmp(&b.id));
        crate::digest(&ordered)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentRef {
    pub path: String,
    pub sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillDescriptor {
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub description: String,
    /// Attribution/source URI, never fetched or executed.
    pub source: String,
    pub license: String,
    /// Exact supported VCP skill contract version. Foreign import is not implied.
    pub vcp_version: u32,
    pub cues: BTreeSet<String>,
    pub environments: BTreeSet<String>,
    pub required_tools: BTreeSet<String>,
    pub body: ContentRef,
    pub resources: Vec<ContentRef>,
}
pub(crate) fn text(value: &str, maximum: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(Error::Metadata(format!(
            "text must be 1..{maximum} bytes without controls"
        )));
    }
    Ok(())
}
pub(crate) fn identifier(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
    {
        return Err(Error::Metadata(
            "ID must be 1..128 lowercase ASCII letters, digits, dot, underscore or hyphen".into(),
        ));
    }
    Ok(())
}
impl ContentRef {
    pub fn validate(&self) -> Result<()> {
        if self.path.len() > 1024 || self.path.contains('\\') || self.path.contains(':') {
            return Err(Error::Metadata(
                "portable relative content path required".into(),
            ));
        }
        let normalized = vcp_repository::path::relative(Path::new(&self.path))?;
        if normalized != self.path || self.path.split('/').any(|p| matches!(p, "." | ".." | "")) {
            return Err(Error::Metadata("normalized content path required".into()));
        }
        if self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::Metadata(
                "lowercase SHA-256 content digest required".into(),
            ));
        }
        Ok(())
    }
}
impl SkillDescriptor {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION || self.vcp_version != SCHEMA_VERSION {
            return Err(Error::Metadata(
                "unsupported skill/VCP contract version".into(),
            ));
        }
        identifier(&self.id)?;
        text(&self.version, 128)?;
        text(&self.description, 1024)?;
        text(&self.source, 2048)?;
        text(&self.license, 256)?;
        for set in [&self.cues, &self.environments, &self.required_tools] {
            if set.len() > 32 {
                return Err(Error::Limit("matching requirements"));
            }
            for value in set {
                text(value, 128)?;
            }
        }
        if self.resources.len() > 32 {
            return Err(Error::Limit("resources"));
        }
        let mut paths = BTreeSet::new();
        for content in std::iter::once(&self.body).chain(&self.resources) {
            content.validate()?;
            if content.path.eq_ignore_ascii_case(DESCRIPTOR_NAME)
                || !paths.insert(content.path.to_ascii_lowercase())
            {
                return Err(Error::Metadata(
                    "content aliases descriptor or another resource".into(),
                ));
            }
        }
        Ok(())
    }
}
