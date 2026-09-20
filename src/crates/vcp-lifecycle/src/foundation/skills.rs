// SPDX-License-Identifier: Apache-2.0
//! Explicit skill configuration and task-scoped activation. Skill content is
//! captured instruction data; these controls grant no tools or process access.
use super::*;
use serde::{Deserialize, Serialize};
use vcp_extensions::{
    discovery::{Limits, MatchContext},
    skill_manifest::{SkillSource, SourceKind, SourceRegistry},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub registry: SourceRegistry,
    pub limits: Limits,
    /// Selection hints only. The host derives actual environment and available
    /// tools; configuration cannot manufacture prerequisites or authority.
    pub context: MatchContext,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Status,
    Activate { id: String, reason: String },
    Disable { id: String },
}
/// Checks canonical read authority before opening descriptor or instruction bytes.
/// A source rooted inside the workspace also inherits that root's read denials.
pub fn check_source_read_access(
    state: &State,
    config: &Config,
    source: &SkillSource,
) -> Result<vcp_repository::Root, String> {
    let check = || -> Result<vcp_repository::Root, Box<dyn std::error::Error + Send + Sync>> {
        let workspace: vcp_domain::workspace::Workspace = state
            .record(
                vcp_store::contract::Collection::Workspace,
                config.workspace.as_str(),
                &config.workspace,
            )?
            .decode()?;
        if source.root.workspace != workspace.id
            || source.root.binding != workspace.binding.revision
        {
            return Err("skill source differs from current workspace binding".into());
        }
        let policy = vcp_engine::policy::current(state, &config.workspace)?;
        let denied = |root: &vcp_domain::ids::RootId| {
            config
                .host_tool_denials
                .iter()
                .chain(policy.denials.iter())
                .any(|rule| {
                    (rule.effects.is_empty()
                        || rule
                            .effects
                            .contains(&vcp_domain::policy::EffectClass::Read))
                        && (rule.roots.is_empty() || rule.roots.contains(root))
                        && rule.tool.as_deref().is_none_or(|tool| tool == "vcp_skill")
                })
        };
        if denied(&source.root.root) {
            return Err("trusted read denial prevents skill discovery".into());
        }
        let root = vcp_repository::Root::open(source.root.clone(), &source.path)?;
        let workspace_root = vcp_repository::Root::open(
            vcp_repository::RootIdentity {
                workspace: workspace.id.clone(),
                root: vcp_domain::ids::RootId::parse(workspace.id.as_str())?,
                repository: workspace.binding.repository,
                worktree: workspace.binding.worktree,
                binding: workspace.binding.revision,
            },
            std::path::Path::new(&workspace.binding.root),
        )?;
        let contained = root.path().starts_with(workspace_root.path());
        if source.kind == SourceKind::Workspace && !contained {
            return Err("workspace skill source must remain inside workspace".into());
        }
        let overlaps_workspace = contained || workspace_root.path().starts_with(root.path());
        if overlaps_workspace && denied(&workspace_root.identity.root) {
            return Err(
                "trusted read denial prevents skill discovery through workspace root alias".into(),
            );
        }
        Ok(root)
    };
    check().map_err(|error| error.to_string())
}
/// Bounded observed setup metadata shared by CLI preview and host activation.
/// Tool names must come from the caller's actual configured adapters.
pub fn actual_match_context(
    root: &vcp_repository::Root,
    tools: std::collections::BTreeSet<String>,
) -> Result<MatchContext, String> {
    let mut cues = std::collections::BTreeSet::new();
    for marker in [
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "go.mod",
        "pom.xml",
        "build.gradle",
        "CMakeLists.txt",
        "Gemfile",
        "composer.json",
        "pubspec.yaml",
        "Package.swift",
        "global.json",
    ] {
        if root.read(std::path::Path::new(marker), 64 * 1024).is_ok() {
            cues.insert(marker.into());
        }
    }
    Ok(MatchContext {
        environment: std::env::consts::OS.into(),
        tools,
        cues,
    })
}
#[cfg(windows)]
impl CanonicalHost {
    pub fn inspect_skills(&self, registry: SourceRegistry) -> Result<serde_json::Value, String> {
        self.worker
            .run(move |context| context.inspect_skills(registry))
    }
    pub fn configure_skills(&self, configuration: Configuration) -> Result<(), String> {
        self.worker
            .run(move |context| context.configure_skills(configuration))
    }
    pub fn skill_control(
        &self,
        thread: ThreadId,
        request: Request,
    ) -> Result<serde_json::Value, String> {
        let binding = self.binding(thread)?;
        self.worker
            .run(move |context| context.skill_control(&binding, request))
    }
}
