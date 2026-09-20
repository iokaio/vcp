// SPDX-License-Identifier: Apache-2.0
//! Explicit skill controls. Descriptions are data; activation grants no authority.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use vcp_domain::{Revision, RootId};
use vcp_extensions::skill_manifest::SourceKind;
use vcp_lifecycle::foundation::skills::Request;
#[cfg(windows)]
pub mod offline;

pub const HELP: &str =
    "/skills list [--offset <row>] | /skills activate <qualified-id> [reason] | /skills disable <qualified-id>";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    List { offset: usize },
    Activate { id: String, reason: String },
    Disable { id: String },
}
#[derive(Clone, Debug, clap::Subcommand)]
pub enum OfflineCommand {
    /// Inspect configured descriptors and setup diagnostics without inference.
    List {
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },
}

fn identity(value: &str) -> Result<String, String> {
    if value.is_empty() || value.len() > 2048 || value.chars().any(char::is_control) {
        return Err("Skill identity must be nonempty, within 2048 bytes, without controls.".into());
    }
    Ok(value.to_owned())
}

pub fn parse(words: &[&str]) -> Result<Command, String> {
    match words {
        [] | ["list"] => Ok(Command::List { offset: 0 }),
        ["list", "--offset", offset] => Ok(Command::List {
            offset: offset
                .parse()
                .map_err(|_| "Skill list offset must be an unsigned integer.")?,
        }),
        ["activate", id, reason @ ..] => {
            let reason = if reason.is_empty() {
                "explicit user request through /skills activate".to_owned()
            } else {
                reason.join(" ")
            };
            if reason.len() > 2048 || reason.chars().any(char::is_control) {
                return Err(
                    "Skill activation reason exceeds 2048 bytes or contains controls.".into(),
                );
            }
            Ok(Command::Activate {
                id: identity(id)?,
                reason,
            })
        }
        ["disable", id] => Ok(Command::Disable { id: identity(id)? }),
        _ => Err(format!("Use explicit skill controls: {HELP}")),
    }
}

pub fn execute(
    command: Command,
    mut service: impl FnMut(Request) -> Result<serde_json::Value, String>,
) -> Result<String, String> {
    let offset = match &command {
        Command::List { offset } => Some(*offset),
        _ => None,
    };
    let request = match command {
        Command::List { .. } => Request::Status,
        Command::Activate { id, reason } => Request::Activate { id, reason },
        Command::Disable { id } => Request::Disable { id },
    };
    let mut result = service(request)?;
    if let Some(offset) = offset {
        if !result["catalog"].is_null() {
            let mut catalog: vcp_extensions::discovery::Catalog =
                serde_json::from_value(result["catalog"].take()).map_err(|e| e.to_string())?;
            if let Some(disabled) = result["state"]["disabled"].as_array() {
                catalog.disabled.extend(
                    disabled
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(str::to_owned),
                );
            }
            let context = serde_json::from_value(result["context"].clone()).ok();
            result["catalog"] = catalog_page(&catalog, context.as_ref(), offset);
        }
    }
    let state = if result.get("state").is_some() {
        &mut result["state"]
    } else {
        &mut result
    };
    if let Some(active) = state
        .get_mut("active")
        .and_then(serde_json::Value::as_object_mut)
    {
        for skill in active.values_mut() {
            if let Some(fields) = skill.as_object_mut() {
                fields.retain(|key, _| {
                    matches!(
                        key.as_str(),
                        "qualified_id" | "source_id" | "version" | "reason" | "activation" | "body"
                    )
                });
            }
        }
    }
    let disabled_page = state
        .get("disabled")
        .and_then(serde_json::Value::as_array)
        .map(|disabled| {
            let offset = offset.unwrap_or(0);
            let next = offset.saturating_add(32);
            (
                disabled.len(),
                disabled
                    .iter()
                    .skip(offset)
                    .take(32)
                    .cloned()
                    .collect::<Vec<_>>(),
                (next < disabled.len()).then_some(next),
            )
        });
    if let Some((total, page, next)) = disabled_page {
        state["disabled"] = serde_json::json!(page);
        state["disabled_total"] = serde_json::json!(total);
        state["disabled_next_offset"] = serde_json::json!(next);
        state["next_disabled_command"] =
            serde_json::json!(next.map(|next| format!("/skills list --offset {next}")));
    }
    // Preserve every setup diagnostic and exact source/version/reason. The
    // terminal's existing pager bounds each displayed slice and escapes controls.
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub version: u32,
    pub revision: Revision,
    pub sources: Vec<Source>,
    #[serde(default)]
    pub disabled: std::collections::BTreeSet<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub id: String,
    pub kind: SourceKind,
    pub enabled: bool,
    /// Stable root registration supplied by the trusted user configuration.
    pub root_id: RootId,
    pub path: PathBuf,
}

impl Configuration {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.sources.len() > 32 || self.disabled.len() > 4096 {
            return Err(
                "Skill configuration requires version 1 and at most 32 explicit sources.".into(),
            );
        }
        let mut ids = std::collections::BTreeSet::new();
        let mut roots = std::collections::BTreeSet::new();
        for source in &self.sources {
            if source.id == vcp_extensions::catalog::SOURCE_ID
                || source.root_id.as_str() == vcp_extensions::catalog::ROOT_ID
            {
                return Err(
                    "The vcp-builtin source and root IDs are reserved for packaged assets.".into(),
                );
            }
            if source.id.is_empty()
                || source.id.len() > 128
                || !source.id.bytes().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
                })
                || !ids.insert(&source.id)
                || !roots.insert(&source.root_id)
                || !source.path.is_absolute()
                || source.path.as_os_str().len() > 4096
                || source
                    .path
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
            {
                return Err("Skill sources require unique stable IDs/root IDs and explicit absolute paths without parent traversal.".into());
            }
        }
        Ok(())
    }

    pub fn prepare(
        &self,
        config: &vcp_lifecycle::foundation::Config,
        tools: std::collections::BTreeSet<String>,
    ) -> Result<vcp_lifecycle::foundation::skills::Configuration, String> {
        self.validate()?;
        let workspace = std::path::Path::new(&config.binding.root);
        let identity = vcp_repository::RootIdentity {
            workspace: config.workspace.clone(),
            root: RootId::parse(config.workspace.as_str()).map_err(|e| e.to_string())?,
            repository: config.binding.repository.clone(),
            worktree: config.binding.worktree.clone(),
            binding: config.binding.revision,
        };
        let context = vcp_extensions::discovery::MatchContext {
            environment: std::env::consts::OS.into(),
            tools,
            cues: Default::default(),
        };
        let mut sources = Vec::new();
        for source in &self.sources {
            if source.kind == SourceKind::Workspace && !within_workspace(&source.path, workspace) {
                return Err(format!(
                    "Workspace skill source {} must be inside the registered workspace.",
                    source.id
                ));
            }
            sources.push(vcp_extensions::skill_manifest::SkillSource {
                id: source.id.clone(),
                kind: source.kind,
                enabled: source.enabled,
                root: vcp_repository::RootIdentity {
                    root: source.root_id.clone(),
                    ..identity.clone()
                },
                path: source.path.clone(),
            });
        }
        let registry = vcp_extensions::skill_manifest::SourceRegistry {
            version: self.version,
            revision: self.revision,
            sources,
            disabled: self.disabled.clone(),
        };
        registry.validate().map_err(|e| e.to_string())?;
        Ok(vcp_lifecycle::foundation::skills::Configuration {
            registry,
            limits: Default::default(),
            context,
        })
    }
}

/// Resolves packaged assets only beside the running executable. Metadata probes
/// do not read descriptors; canonical source policy precedes all content reads.
pub fn prepare(
    profile: &crate::settings::Profile,
    config: &vcp_lifecycle::foundation::Config,
) -> Result<vcp_lifecycle::foundation::skills::Configuration, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let directory = executable
        .parent()
        .ok_or("Executable installation directory is unavailable")?;
    prepare_at(
        profile.skills.as_ref(),
        config,
        available_tools(profile),
        directory,
    )
}

fn prepare_at(
    configured: Option<&Configuration>,
    config: &vcp_lifecycle::foundation::Config,
    tools: std::collections::BTreeSet<String>,
    installation: &std::path::Path,
) -> Result<vcp_lifecycle::foundation::skills::Configuration, String> {
    let empty = Configuration {
        version: 1,
        revision: Revision::ZERO,
        sources: vec![],
        disabled: Default::default(),
    };
    let mut prepared = configured.unwrap_or(&empty).prepare(config, tools)?;
    let path = installation.join("skills").join("builtin");
    if matches!(std::fs::symlink_metadata(&path), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
    {
        return Ok(prepared);
    }
    if prepared.registry.sources.len() >= 32 {
        return Err("Packaged built-in skills require one source slot; configure at most 31 additional sources.".into());
    }
    prepared
        .registry
        .sources
        .push(vcp_extensions::skill_manifest::SkillSource {
            id: vcp_extensions::catalog::SOURCE_ID.into(),
            kind: SourceKind::Builtin,
            enabled: true,
            root: vcp_repository::RootIdentity {
                workspace: config.workspace.clone(),
                root: RootId::parse(vcp_extensions::catalog::ROOT_ID).map_err(|e| e.to_string())?,
                repository: config.binding.repository.clone(),
                worktree: config.binding.worktree.clone(),
                binding: config.binding.revision,
            },
            path,
        });
    prepared.registry.validate().map_err(|e| e.to_string())?;
    Ok(prepared)
}

fn missing_builtin(
    catalog: &mut vcp_extensions::discovery::Catalog,
    registry: &vcp_extensions::skill_manifest::SourceRegistry,
) {
    if !registry
        .sources
        .iter()
        .any(|source| source.id == vcp_extensions::catalog::SOURCE_ID)
    {
        catalog.diagnostics.push(vcp_extensions::discovery::Diagnostic {
            source_id: vcp_extensions::catalog::SOURCE_ID.into(), path: "skills/builtin".into(), code: "builtin_assets_missing".into(),
            message: "Bundled assets are missing beside this executable. Install the complete package; configured skills remain available.".into(),
        });
    }
}

fn within_workspace(path: &std::path::Path, workspace: &std::path::Path) -> bool {
    // Native handles validate containment again without following reparse
    // points. Canonical workspace bindings use Windows' verbatim prefix while
    // explicitly configured local paths may use the equivalent drive spelling.
    let path = path.to_string_lossy();
    let workspace = workspace.to_string_lossy();
    crate::settings::within(
        std::path::Path::new(path.strip_prefix(r"\\?\").unwrap_or(&path)),
        std::path::Path::new(workspace.strip_prefix(r"\\?\").unwrap_or(&workspace)),
    )
}

pub fn available_tools(profile: &crate::settings::Profile) -> std::collections::BTreeSet<String> {
    let mut tools = [
        "vcp_read",
        "vcp_list",
        "vcp_search",
        "vcp_patch",
        "vcp_verify",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain(
        profile
            .processes
            .iter()
            .filter(|process| process.executable.is_file())
            .map(|process| process.name.clone()),
    )
    .collect::<std::collections::BTreeSet<_>>();
    if profile
        .processes
        .iter()
        .any(|process| process.executable.is_file())
    {
        tools.insert("vcp_exec".into());
    }
    tools
}

pub fn inspect(
    profile: &crate::settings::Profile,
    config: &vcp_lifecycle::foundation::Config,
    state: &vcp_store::contract::State,
    offset: usize,
) -> Result<serde_json::Value, String> {
    let mut configuration = prepare(profile, config)?;
    let mut integrity = None;
    for source in configuration
        .registry
        .sources
        .iter()
        .filter(|source| source.enabled)
    {
        let root =
            vcp_lifecycle::foundation::skills::check_source_read_access(state, config, source)?;
        if source.id == vcp_extensions::catalog::SOURCE_ID {
            integrity = Some(vcp_extensions::catalog::verify(&root).map_err(|e| e.to_string())?);
        }
    }
    let cue_root = vcp_lifecycle::foundation::skills::check_source_read_access(
        state,
        config,
        &vcp_extensions::skill_manifest::SkillSource {
            id: "project-cues".into(),
            kind: SourceKind::Workspace,
            enabled: true,
            path: PathBuf::from(&config.binding.root),
            root: vcp_repository::RootIdentity {
                workspace: config.workspace.clone(),
                root: RootId::parse(config.workspace.as_str()).map_err(|e| e.to_string())?,
                repository: config.binding.repository.clone(),
                worktree: config.binding.worktree.clone(),
                binding: config.binding.revision,
            },
        },
    )?;
    configuration.context = vcp_lifecycle::foundation::skills::actual_match_context(
        &cue_root,
        configuration.context.tools,
    )?;
    let mut catalog =
        vcp_extensions::discovery::discover(&configuration.registry, &configuration.limits)
            .map_err(|e| e.to_string())?;
    if let Some(integrity) = &integrity {
        vcp_extensions::catalog::verify_discovery(integrity, &catalog)
            .map_err(|e| e.to_string())?;
    }
    missing_builtin(&mut catalog, &configuration.registry);
    let mut result = catalog_page(&catalog, Some(&configuration.context), offset);
    result["integrity"] = serde_json::to_value(integrity).map_err(|e| e.to_string())?;
    result["sources"] = serde_json::json!(configuration
        .registry
        .sources
        .iter()
        .map(
            |source| serde_json::json!({"id":source.id,"kind":source.kind,"enabled":source.enabled})
        )
        .collect::<Vec<_>>());
    if let Some(next) = result["next_offset"].as_u64() {
        result["next_command"] = serde_json::json!(format!("vcp skills list --offset {next}"));
    }
    result["configured"] = serde_json::json!(true);
    result["configuration"] =
        serde_json::json!("trusted profile on disk; use /skills list for the active session");
    result["context"] = serde_json::to_value(configuration.context).map_err(|e| e.to_string())?;
    result["activation"] = serde_json::json!(
        "Use /skills activate in an active session; inspection starts no inference."
    );
    Ok(result)
}

pub(super) fn catalog_page(
    catalog: &vcp_extensions::discovery::Catalog,
    context: Option<&vcp_extensions::discovery::MatchContext>,
    offset: usize,
) -> serde_json::Value {
    const PAGE: usize = 32;
    let candidates = catalog.skills.iter().skip(offset).take(PAGE).map(|skill|serde_json::json!({
        "qualified_id":skill.qualified_id,"source":skill.source_id,"source_kind":skill.source_kind,
        "version":skill.descriptor.version,"description":skill.descriptor.description,
        "license":skill.descriptor.license,"attribution":skill.descriptor.source,
        "cues":skill.descriptor.cues,"environments":skill.descriptor.environments,
        "required_tools":skill.descriptor.required_tools,"compatible":context.map(|context|skill.compatible(context)),
        "matches_project":context.map(|context|skill.matches(context)),"disabled":catalog.disabled.contains(&skill.qualified_id),
    })).collect::<Vec<_>>();
    let total = catalog.skills.len().max(catalog.diagnostics.len());
    let next = offset.saturating_add(PAGE);
    serde_json::json!({"revision":catalog.revision,"registry_digest":catalog.registry_digest,"skills":candidates,"diagnostics":catalog.diagnostics.iter().skip(offset).take(PAGE).collect::<Vec<_>>(),"reads":catalog.reads,"offset":offset,"total_skills":catalog.skills.len(),"total_diagnostics":catalog.diagnostics.len(),"next_offset":(next<total).then_some(next),"next_command":(next<total).then(||format!("/skills list --offset {next}"))})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn controls_require_explicit_selection_and_preserve_attribution() {
        assert_eq!(parse(&[]).unwrap(), Command::List { offset: 0 });
        assert!(parse(&["activate"]).is_err());
        assert!(parse(&["disable", "source::package::skill", "extra"]).is_err());
        assert!(parse(&["install", "remote"]).is_err());
        assert!(parse(&["activate", "id", &"x".repeat(2049)]).is_err());
        let output = execute(
            parse(&["activate", "user::rust::review", "inspect", "changes"]).unwrap(),
            |request| {
                let Request::Activate { id, reason } = request else {
                    panic!("activation required")
                };
                assert_eq!(id, "user::rust::review");
                assert_eq!(reason, "inspect changes");
                Ok(serde_json::json!({"id":id,"version":"1.2","source":"user","reason":reason}))
            },
        )
        .unwrap();
        assert!(
            output.contains("1.2") && output.contains("user") && output.contains("inspect changes")
        );
    }
    #[test]
    fn explicit_setup_failures_remain_visible() {
        let error = execute(parse(&["activate", "missing"]).unwrap(), |_| {
            Err("missing or incompatible skill: missing".into())
        })
        .unwrap_err();
        assert!(error.contains("missing or incompatible"));
        let empty = Configuration {
            version: 1,
            revision: Revision::ZERO,
            sources: vec![],
            disabled: Default::default(),
        };
        empty.validate().unwrap();
        assert!(serde_json::from_value::<Configuration>(
            serde_json::json!({"version":1,"revision":"0","sources":[],"scan_home":true})
        )
        .is_err());
    }
    #[test]
    fn catalog_diagnostics_remain_reachable_in_bounded_pages() {
        use vcp_extensions::discovery::{Catalog, Diagnostic};
        let catalog = Catalog {
            registry_digest: "a".repeat(64),
            revision: Revision::ZERO,
            skills: vec![],
            disabled: Default::default(),
            reads: Default::default(),
            diagnostics: (0..65)
                .map(|index| Diagnostic {
                    source_id: "project".into(),
                    path: format!("package-{index}"),
                    code: "setup".into(),
                    message: "Missing supported descriptor".into(),
                })
                .collect(),
        };
        let first = catalog_page(&catalog, None, 0);
        assert_eq!(first["diagnostics"].as_array().unwrap().len(), 32);
        assert_eq!(first["next_offset"], 32);
        let last = catalog_page(&catalog, None, 64);
        assert_eq!(last["diagnostics"][0]["path"], "package-64");
        assert!(last["next_offset"].is_null());
        assert!(catalog_page(&catalog, None, usize::MAX)["skills"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(parse(&["list", "--offset", "-1"]).is_err());
    }
    #[test]
    fn source_config_rejects_ambient_scans_and_duplicate_registrations() {
        let root = RootId::new();
        let source = Source {
            id: "project".into(),
            kind: SourceKind::Workspace,
            enabled: true,
            root_id: root,
            path: std::env::temp_dir(),
        };
        let mut config = Configuration {
            version: 1,
            revision: Revision::ZERO,
            sources: vec![source.clone()],
            disabled: Default::default(),
        };
        config.validate().unwrap();
        config.sources.push(source);
        assert!(config.validate().is_err());
        config.sources.pop();
        config.sources[0].path = PathBuf::from("../ambient");
        assert!(config.validate().is_err());
        config.sources[0].path = std::env::temp_dir();
        config.sources[0].id = "bad\nsource".into();
        assert!(config.validate().is_err());
        config.sources[0].id = vcp_extensions::catalog::SOURCE_ID.into();
        assert!(config.validate().unwrap_err().contains("reserved"));
        config.sources[0].id = "project".into();
        config.sources[0].root_id = RootId::parse(vcp_extensions::catalog::ROOT_ID).unwrap();
        assert!(config.validate().unwrap_err().contains("reserved"));
    }
    #[test]
    fn maximum_disabled_history_is_paged_without_losing_the_tail() {
        let disabled = (0..1024)
            .map(|index| format!("source::{}::{index:04}", "x".repeat(2000)))
            .collect::<Vec<_>>();
        let response =
            serde_json::json!({"state":{"active":{},"disabled":disabled},"catalog":null});
        let first = execute(Command::List { offset: 0 }, |_| Ok(response.clone())).unwrap();
        assert!(
            first.len() < 128 * 1024,
            "bounded notice must not reach terminal's 1MiB truncation"
        );
        let first: serde_json::Value = serde_json::from_str(&first).unwrap();
        assert_eq!(first["state"]["disabled_total"], 1024);
        assert_eq!(first["state"]["disabled"].as_array().unwrap().len(), 32);
        assert_eq!(first["state"]["disabled_next_offset"], 32);
        let last = execute(Command::List { offset: 992 }, |_| Ok(response.clone())).unwrap();
        let last: serde_json::Value = serde_json::from_str(&last).unwrap();
        assert!(last["state"]["disabled"][31]
            .as_str()
            .unwrap()
            .ends_with("::1023"));
        assert!(last["state"]["disabled_next_offset"].is_null());
    }
    #[test]
    fn standalone_list_requires_no_objective_budget_or_provider_profile() {
        use clap::Parser;
        let workspace = tempfile::tempdir().unwrap();
        let cli = crate::args::Cli::try_parse_from([
            "vcp",
            "--workspace",
            workspace.path().to_str().unwrap(),
            "skills",
            "list",
            "--offset",
            "32",
        ])
        .unwrap()
        .validate(None)
        .unwrap();
        assert!(matches!(
            cli.command,
            crate::args::ValidatedCommand::Skills(OfflineCommand::List { offset: 32 })
        ));
        assert!(
            crate::args::Cli::try_parse_from(["vcp", "skills", "activate", "foreign"]).is_err()
        );
    }
}
