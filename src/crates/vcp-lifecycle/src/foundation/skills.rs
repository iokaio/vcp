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
    check_source_access(state, config, source, false)?
        .ok_or_else(|| "skill source unavailable".into())
}
fn check_source_access(
    state: &State,
    config: &Config,
    source: &SkillSource,
    allow_missing: bool,
) -> Result<Option<vcp_repository::Root>, String> {
    let check = || -> Result<_, Box<dyn std::error::Error + Send + Sync>> {
        if (source.id == vcp_extensions::catalog::SOURCE_ID
            || source.root.root.as_str() == vcp_extensions::catalog::ROOT_ID)
            && (source.id != vcp_extensions::catalog::SOURCE_ID
                || source.root.root.as_str() != vcp_extensions::catalog::ROOT_ID
                || source.kind != SourceKind::Builtin)
        {
            return Err("reserved builtin source and root identity cannot be reassigned".into());
        }
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
        let root = match vcp_repository::Root::open(source.root.clone(), &source.path) {
            Ok(root) => root,
            Err(vcp_repository::Error::Io(error))
                if allow_missing
                    && source.kind == SourceKind::User
                    && source.id != vcp_extensions::catalog::SOURCE_ID
                    && error.kind() == std::io::ErrorKind::NotFound =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };
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
        Ok(Some(root))
    };
    check().map_err(|error| error.to_string())
}
/// Missing optional user roots are source diagnostics. Authorization failures and
/// missing shipped assets remain fatal. Exclude missing roots from discovery
/// so a concurrently created directory cannot bypass the access check.
pub fn discover_authorized(
    state: &State,
    config: &Config,
    registry: &SourceRegistry,
    limits: &Limits,
) -> Result<
    (
        vcp_extensions::discovery::Catalog,
        Option<vcp_extensions::catalog::Verification>,
        std::collections::BTreeSet<String>,
    ),
    String,
> {
    let digest = registry.digest().map_err(|error| error.to_string())?;
    let mut available = registry.clone();
    let mut missing = Vec::new();
    let mut missing_sources = std::collections::BTreeSet::new();
    let mut integrity = None;
    for source in registry.sources.iter().filter(|source| source.enabled) {
        match check_source_access(state, config, source, true)? {
            Some(root) if source.id == vcp_extensions::catalog::SOURCE_ID => {
                integrity = Some(
                    vcp_extensions::catalog::verify(&root).map_err(|error| error.to_string())?,
                );
            }
            Some(_) => {}
            None => {
                missing_sources.insert(source.id.clone());
                available
                    .sources
                    .retain(|candidate| candidate.id != source.id);
                missing.push(vcp_extensions::discovery::Diagnostic {
                    source_id: source.id.clone(),
                    path: String::new(),
                    code: "source_unavailable".into(),
                    message: "Optional skill source directory does not exist; configure skills again after restoring it.".into(),
                });
            }
        }
    }
    let mut catalog = vcp_extensions::discovery::discover(&available, limits)
        .map_err(|error| error.to_string())?;
    // Activation compares against the owner's complete configured registry.
    catalog.registry_digest = digest;
    catalog.diagnostics.extend(missing);
    if let Some(verified) = &integrity {
        vcp_extensions::catalog::verify_discovery(verified, &catalog)
            .map_err(|error| error.to_string())?;
    }
    Ok((catalog, integrity, missing_sources))
}
/// Root files whose presence becomes a cue of the same name.
pub const ROOT_MARKERS: [&str; 22] = [
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
    "build.gradle.kts",
    "settings.gradle",
    "settings.gradle.kts",
    "requirements.txt",
    "setup.py",
    "Pipfile",
    "deno.json",
    "deno.jsonc",
    "go.work",
    "meson.build",
];
/// Root project files matched by extension become `*.ext` cues.
pub const ROOT_PATTERN_EXTENSIONS: [&str; 6] =
    ["sln", "slnx", "csproj", "fsproj", "vcxproj", "vbproj"];
/// Bounded observed setup metadata shared by CLI preview and host activation.
/// Tool names must come from the caller's actual configured adapters.
pub fn actual_match_context(
    root: &vcp_repository::Root,
    tools: std::collections::BTreeSet<String>,
) -> Result<MatchContext, String> {
    let mut cues = std::collections::BTreeSet::new();
    for marker in ROOT_MARKERS {
        // Existence, not contents: a large manifest is still a marker, and no
        // marker bytes are read on every turn.
        if root.hold(Some(std::path::Path::new(marker)), false).is_ok() {
            cues.insert(marker.into());
        }
    }
    cues.extend(root_pattern_cues(root));
    Ok(MatchContext {
        environment: std::env::consts::OS.into(),
        tools,
        cues,
    })
}
/// Project files named only by extension (`*.sln`) become `*.ext` cues. Only
/// ordinary root files count; a root above the entry bound is skipped entirely
/// so the result never depends on directory enumeration order.
fn root_pattern_cues(root: &vcp_repository::Root) -> std::collections::BTreeSet<String> {
    const ROOT_ENTRY_BOUND: usize = 4096;
    let mut cues = std::collections::BTreeSet::new();
    let Ok(_held) = root.hold(None, true) else {
        return cues;
    };
    let Ok(entries) = std::fs::read_dir(root.path()) else {
        return cues;
    };
    let entries: Vec<_> = entries.take(ROOT_ENTRY_BOUND + 1).collect();
    if entries.len() > ROOT_ENTRY_BOUND {
        return cues;
    }
    for entry in entries.into_iter().flatten() {
        let name = entry.file_name();
        let Some(extension) = std::path::Path::new(&name)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
        else {
            continue;
        };
        if ROOT_PATTERN_EXTENSIONS.contains(&extension.as_str())
            && std::fs::symlink_metadata(entry.path()).is_ok_and(|m| m.file_type().is_file())
        {
            cues.insert(format!("*.{extension}"));
        }
    }
    cues
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
    /// Returns one verified on-demand reference of an active skill (ADR-071).
    pub(in crate::foundation) fn skill_read(
        &self,
        thread: ThreadId,
        request: super::worker::skills::ReadRequest,
    ) -> Result<super::worker::skills::SkillRead, String> {
        let binding = self.binding(thread)?;
        self.worker
            .run(move |context| context.skill_read(&binding, &request))
    }
    /// Resolves an active skill's verified file resource into the exact patch
    /// that creates it; the caller submits that patch through normal authority.
    pub(in crate::foundation) fn skill_materialization(
        &self,
        thread: ThreadId,
        request: super::worker::skills::MaterializeRequest,
    ) -> Result<super::worker::skills::Materialization, String> {
        let binding = self.binding(thread)?;
        self.worker
            .run(move |context| context.skill_materialization(&binding, &request))
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn root(path: &std::path::Path) -> vcp_repository::Root {
        vcp_repository::Root::open(
            vcp_repository::RootIdentity {
                workspace: vcp_domain::WorkspaceId::new(),
                root: vcp_domain::RootId::new(),
                repository: "fixture".into(),
                worktree: "fixture".into(),
                binding: vcp_domain::Revision::ZERO,
            },
            path,
        )
        .unwrap()
    }

    #[test]
    fn root_markers_and_project_file_patterns_become_cues() {
        let temp = tempfile::tempdir().unwrap();
        for name in [
            "build.gradle.kts",
            "requirements.txt",
            "deno.jsonc",
            "go.work",
            "App.SLN",
            "Tool.csproj",
            "meson.build",
            "Native.vcxproj",
            "Legacy.vbproj",
        ] {
            std::fs::write(temp.path().join(name), b"fixture").unwrap();
        }
        // A directory named like a project file is not a project file.
        std::fs::create_dir(temp.path().join("Folder.fsproj")).unwrap();
        std::fs::create_dir(temp.path().join("nested")).unwrap();
        std::fs::write(temp.path().join("nested/Deep.slnx"), b"fixture").unwrap();
        let context = actual_match_context(&root(temp.path()), Default::default()).unwrap();
        let cues: Vec<_> = context.cues.iter().map(String::as_str).collect();
        assert_eq!(
            cues,
            [
                "*.csproj",
                "*.sln",
                "*.vbproj",
                "*.vcxproj",
                "build.gradle.kts",
                "deno.jsonc",
                "go.work",
                "meson.build",
                "requirements.txt"
            ]
        );
    }

    #[test]
    fn marker_constants_match_the_shared_marker_file() {
        // src/skills/markers.json also drives the builtin fixture author.
        let shared: serde_json::Value =
            serde_json::from_str(include_str!("../../../../skills/markers.json")).unwrap();
        let list =
            |key: &str| -> Vec<String> { serde_json::from_value(shared[key].clone()).unwrap() };
        assert_eq!(list("root_markers"), ROOT_MARKERS);
        assert_eq!(list("root_pattern_extensions"), ROOT_PATTERN_EXTENSIONS);
    }

    #[test]
    fn shipped_cues_are_host_emittable_and_descriptions_fit_discovery() {
        let builtin = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/builtin");
        let emittable: std::collections::BTreeSet<String> = ROOT_MARKERS
            .iter()
            .map(|m| m.to_string())
            .chain(ROOT_PATTERN_EXTENSIONS.iter().map(|e| format!("*.{e}")))
            .collect();
        let mut metadata = 0usize;
        for entry in vcp_extensions::catalog::embedded().unwrap().skills {
            let descriptor: vcp_extensions::skill_manifest::SkillDescriptor =
                serde_json::from_slice(&std::fs::read(builtin.join(&entry.descriptor)).unwrap())
                    .unwrap();
            for cue in &descriptor.cues {
                assert!(
                    emittable.contains(cue),
                    "{}: unemittable cue {cue}",
                    entry.id
                );
            }
            assert!(
                descriptor.description.contains("Use when"),
                "{}: description must say when to use it",
                entry.id
            );
            metadata += serde_json::to_vec(&serde_json::json!({"id":format!("vcp-builtin::{0}::{0}", entry.id),"description":descriptor.description,"required_tools":descriptor.required_tools})).unwrap().len() + 1;
        }
        // worker/skills.rs lists description metadata within 60 KiB.
        assert!(metadata < 60 * 1024, "{metadata}");
    }

    #[test]
    fn a_marker_larger_than_a_read_bound_is_still_detected() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("package.json"), vec![b' '; 256 * 1024]).unwrap();
        let context = actual_match_context(&root(temp.path()), Default::default()).unwrap();
        assert!(context.cues.contains("package.json"));
    }

    #[test]
    fn oversized_root_skips_pattern_detection_deterministically() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("App.sln"), b"fixture").unwrap();
        std::fs::write(temp.path().join("Cargo.toml"), b"fixture").unwrap();
        for index in 0..4096 {
            std::fs::write(temp.path().join(format!("f{index}")), b"").unwrap();
        }
        let context = actual_match_context(&root(temp.path()), Default::default()).unwrap();
        assert!(context.cues.contains("Cargo.toml"));
        assert!(!context.cues.contains("*.sln"));
    }
}
