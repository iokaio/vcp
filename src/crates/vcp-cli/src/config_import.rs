// SPDX-License-Identifier: Apache-2.0
//! Explicit offline import. The original profile remains the authority ceiling.
pub mod store;
use clap::{Args, Subcommand, ValueEnum};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};
use vcp_extensions::r#import::{
    compatibility::Format,
    normalize::{Context, Preferences, Server, Transport},
};
use vcp_protocol::{canonical_bytes, digest_bytes};
use vcp_repository::{path::HeldPath, Root, RootIdentity};

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Subcommand)]
pub enum ConfigCommand {
    /// Import a tested foreign subset as restrictions under the native profile.
    Import {
        #[command(subcommand)]
        command: Command,
    },
}
#[derive(Clone, Debug, ValueEnum)]
pub enum SourceFormat {
    #[value(name = "codex-8b78600d-v1")]
    Codex,
    #[value(name = "gemini-6a466a7e-v1")]
    Gemini,
}
impl SourceFormat {
    fn format(&self) -> Format {
        match self {
            Self::Codex => Format::Codex8b78600d,
            Self::Gemini => Format::Gemini6a466a7e,
        }
    }
    fn label(&self) -> &'static str {
        match self {
            Self::Codex => "codex-8b78600d-v1",
            Self::Gemini => "gemini-6a466a7e-v1",
        }
    }
}
#[derive(Clone, Debug, Args)]
pub struct SourceOptions {
    #[arg(long)]
    pub source: PathBuf,
    #[arg(long)]
    pub source_root: PathBuf,
    #[arg(long, value_enum)]
    pub source_format: SourceFormat,
}
#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    /// Read and map one explicit source; no configuration or history is written.
    Preview {
        #[command(flatten)]
        source: SourceOptions,
    },
    /// Apply only selected field IDs from an unchanged preview.
    Apply {
        #[command(flatten)]
        source: SourceOptions,
        #[arg(long)]
        preview: String,
        #[arg(long, required = true)]
        select: Vec<String>,
    },
    /// Inspect the selected revision and whether the native profile changed.
    Status,
    /// Preview a prior import revision under the current native authority ceiling.
    RollbackPreview {
        #[arg(long)]
        revision: u64,
    },
    /// Append a new legal revision; never restore old grants or raw credentials.
    Rollback {
        #[arg(long)]
        revision: u64,
        #[arg(long)]
        preview: String,
    },
}

struct SourceSnapshot {
    _pin: HeldPath,
    bytes: Vec<u8>,
    provenance: Value,
}
fn source_snapshot(options: &SourceOptions) -> Result<SourceSnapshot> {
    for path in [&options.source, &options.source_root] {
        if path.components().any(|p| matches!(p, Component::ParentDir)) {
            return Err("source paths cannot contain parent traversal".into());
        }
    }
    let directory =
        std::path::absolute(&options.source_root).map_err(|_| "source root unavailable")?;
    let path = std::path::absolute(&options.source).map_err(|_| "source path unavailable")?;
    let relative = path
        .strip_prefix(&directory)
        .map_err(|_| "source must be inside the explicit source root")?;
    let root = Root::open(
        RootIdentity {
            workspace: vcp_domain::WorkspaceId::parse("config-import")
                .map_err(|e| e.to_string())?,
            root: vcp_domain::RootId::parse("config-import").map_err(|e| e.to_string())?,
            repository: "config-import".into(),
            worktree: "config-import".into(),
            binding: vcp_domain::Revision::ZERO,
        },
        &directory,
    )
    .map_err(|_| "source root must be a local directory without redirected ancestors")?;
    let pin = root
        .hold(Some(relative), false)
        .map_err(|_| "source must be a regular unaliased file inside its root")?;
    store::single_link(&std::fs::File::open(&path).map_err(|_| "source file unavailable")?)?;
    let source = root
        .read(relative, 524_288)
        .map_err(|_| "source is unavailable, redirected or exceeds 512 KiB")?;
    Ok(SourceSnapshot {
        _pin: pin,
        bytes: source.bytes,
        provenance: json!({"path":path,"allowed_root":directory,"format":options.source_format.label(),"file_version":source.version}),
    })
}

fn base_profile(bytes: &[u8], workspace: &Path) -> Result<crate::settings::Profile> {
    let profile: crate::settings::Profile =
        serde_json::from_slice(bytes).map_err(|_| "invalid native profile or unknown setting")?;
    if profile.version != 1
        || profile
            .workspace
            .canonicalize()
            .map_err(|_| "profile workspace unavailable")?
            != workspace
    {
        return Err("native profile version or workspace binding does not match".into());
    }
    Ok(profile)
}

pub(crate) fn context(profile: &crate::settings::Profile) -> Result<Context> {
    let mut servers = Vec::new();
    for server in &profile.mcp_http {
        servers.push(Server {
            name: server.name.clone(),
            transport: Transport::Http {
                endpoint: server.endpoint.clone(),
                credential_environment: server.credential.as_ref().map(|c| c.environment.clone()),
            },
            allowed_tools: server.allowed_tools.clone(),
            timeout_ms: server.limits.timeout_ms,
        });
    }
    for server in &profile.mcp {
        let matches: Vec<_> = profile
            .processes
            .iter()
            .filter(|p| p.name == server.process.profile)
            .collect();
        if matches.len() != 1 {
            return Err("MCP process profile is missing or ambiguous".into());
        }
        let process = matches[0];
        // Commands are identity data only. No executable is opened or started.
        let command = process
            .executable
            .to_str()
            .ok_or("MCP executable path must be Unicode")?
            .to_owned();
        let cwd = if server.process.directory.is_empty() {
            profile.workspace.clone()
        } else {
            vcp_repository::path::relative(Path::new(&server.process.directory))
                .map_err(|_| "MCP working directory escapes the workspace")?;
            profile.workspace.join(&server.process.directory)
        };
        servers.push(Server {
            name: server.name.clone(),
            transport: Transport::Stdio {
                command,
                args: server.process.arguments.clone(),
                cwd: cwd
                    .to_str()
                    .ok_or("MCP working directory must be Unicode")?
                    .to_owned(),
            },
            allowed_tools: server.allowed_tools.clone(),
            timeout_ms: server.limits.timeout_ms.min(server.process.timeout_ms),
        });
    }
    Ok(Context { servers })
}

/// Imported preferences may restrict a native grant, never introduce one.
pub(crate) fn materialize(
    profile: &mut crate::settings::Profile,
    preferences: &Preferences,
) -> Result<()> {
    let effective = vcp_extensions::r#import::normalize::effective(&context(profile)?, preferences)
        .map_err(|e| e.to_string())?;
    for server in &mut profile.mcp_http {
        if let Some(restriction) = effective.get(&server.name) {
            if let Some(tools) = &restriction.allowed_tools {
                server.allowed_tools = tools.clone();
            }
            if let Some(timeout) = restriction.timeout_ms {
                server.limits.timeout_ms = timeout;
            }
        }
    }
    for server in &mut profile.mcp {
        if let Some(restriction) = effective.get(&server.name) {
            if let Some(tools) = &restriction.allowed_tools {
                server.allowed_tools = tools.clone();
            }
            if let Some(timeout) = restriction.timeout_ms {
                server.limits.timeout_ms = timeout;
                server.process.timeout_ms = server.process.timeout_ms.min(timeout);
            }
        }
    }
    Ok(())
}
fn fingerprint(current: Option<&store::Snapshot>) -> Value {
    match current.filter(|s| s.revision != 0) {
        Some(s) => json!({"revision":s.revision,"revision_sha256":s.revision_sha256}),
        None => json!({"revision":0,"revision_sha256":"none"}),
    }
}
fn identified(mut value: Value) -> Result<Value> {
    let id = digest_bytes(&canonical_bytes(&value).map_err(|e| e.to_string())?);
    value["preview_id"] = id.into();
    Ok(value)
}
fn import_preview(
    bytes: &[u8],
    current: Option<&store::Snapshot>,
    workspace: &Path,
    source: &SourceSnapshot,
    format: &SourceFormat,
) -> Result<(Value, vcp_extensions::r#import::preview::Preview, Context)> {
    let context = context(&base_profile(bytes, workspace)?)?;
    let preferences = current.map(|s| s.preferences.clone()).unwrap_or_default();
    let mapping = vcp_extensions::r#import::preview::preview(
        &source.bytes,
        &format.format(),
        &context,
        &preferences,
    )
    .map_err(|e| e.to_string())?;
    let value = identified(
        json!({"version":1,"kind":"import_preview","base_sha256":digest_bytes(bytes),"current":fingerprint(current),"source":source.provenance,"mapping":mapping}),
    )?;
    Ok((value, mapping, context))
}
fn rollback_preview(
    bytes: &[u8],
    current: Option<&store::Snapshot>,
    target: &store::Snapshot,
    workspace: &Path,
) -> Result<(Value, Preferences)> {
    let context = context(&base_profile(bytes, workspace)?)?;
    let old = current.map(|s| s.preferences.clone()).unwrap_or_default();
    let effective = Preferences {
        servers: vcp_extensions::r#import::normalize::effective(&context, &target.preferences)
            .map_err(|e| e.to_string())?,
    };
    let value = identified(
        json!({"version":1,"kind":"rollback_preview","base_sha256":digest_bytes(bytes),"current":fingerprint(current),"target":fingerprint(Some(target)),"old":old,"new":effective,"authority":"current native profile remains the ceiling"}),
    )?;
    Ok((value, effective))
}
fn require_preview(value: &Value, expected: &str) -> Result<()> {
    if value["preview_id"].as_str() != Some(expected) {
        return Err("preview changed; review a fresh preview before applying".into());
    }
    Ok(())
}

pub fn execute(command: &Command, profile: &Path, workspace: &Path) -> Result<Value> {
    let workspace = workspace
        .canonicalize()
        .map_err(|_| "workspace unavailable")?;
    let profile = std::path::absolute(profile).map_err(|_| "profile path unavailable")?;
    let profile = profile.as_path();
    let (base, current) = store::read(profile, &workspace)?;
    let native = base_profile(&base, &workspace)?;
    let selected_path = profile.canonicalize().map_err(|_| "profile unavailable")?;
    let mut history_name = selected_path
        .file_name()
        .ok_or("profile filename unavailable")?
        .to_os_string();
    history_name.push(".vcp-imports");
    let history_path = selected_path.with_file_name(history_name);
    for root in &native.sync_roots {
        let root = root
            .canonicalize()
            .map_err(|_| "declared sync root unavailable")?;
        if crate::settings::within(&selected_path, &root)
            || crate::settings::within(&history_path, &root)
            || crate::settings::within(&root, &history_path)
        {
            return Err("configuration import history must be outside declared sync roots".into());
        }
    }
    match command {
        Command::Status => Ok(
            json!({"base_sha256":digest_bytes(&base),"current":current,"base_changed":current.as_ref().is_some_and(|s| s.base_sha256 != digest_bytes(&base))}),
        ),
        Command::Preview { source } => {
            let snapshot = source_snapshot(source)?;
            Ok(import_preview(
                &base,
                current.as_ref(),
                &workspace,
                &snapshot,
                &source.source_format,
            )?
            .0)
        }
        Command::Apply {
            source,
            preview,
            select,
        } => {
            let snapshot = source_snapshot(source)?;
            require_preview(
                &import_preview(
                    &base,
                    current.as_ref(),
                    &workspace,
                    &snapshot,
                    &source.source_format,
                )?
                .0,
                preview,
            )?;
            let mut journal = store::Store::open(profile, &workspace)?;
            let previous = journal.snapshot().clone();
            let (view, mapping, context) = import_preview(
                journal.base_bytes(),
                Some(&previous),
                &workspace,
                &snapshot,
                &source.source_format,
            )?;
            require_preview(&view, preview)?;
            let selected: BTreeSet<_> = select.iter().cloned().collect();
            if selected.is_empty() || selected.len() != select.len() {
                return Err("select at least one unique field ID".into());
            }
            let preferences = vcp_extensions::r#import::apply::apply(
                &mapping,
                &selected,
                &context,
                &previous.preferences,
            )
            .map_err(|e| e.to_string())?;
            let base_hash = journal.base_sha256().to_owned();
            let next = journal.publish(
                previous.revision,
                &previous.revision_sha256,
                &base_hash,
                preferences,
                store::Provenance::Import {
                    format: match source.source_format {
                        SourceFormat::Codex => store::SourceFormat::Codex,
                        SourceFormat::Gemini => store::SourceFormat::Gemini,
                    },
                    source_version: source.source_format.label().into(),
                    source_sha256: digest_bytes(&snapshot.bytes),
                    mapping_version: 1,
                    source_path: snapshot.provenance["path"]
                        .as_str()
                        .ok_or("source path encoding")?
                        .into(),
                    allowed_root: snapshot.provenance["allowed_root"]
                        .as_str()
                        .ok_or("source root encoding")?
                        .into(),
                },
                selected,
                None,
            )?;
            Ok(json!({"kind":"import_applied","current":next,"original_profile_preserved":true}))
        }
        Command::RollbackPreview { revision } => {
            let target = store::read_revision(profile, &workspace, *revision)?;
            Ok(rollback_preview(&base, current.as_ref(), &target, &workspace)?.0)
        }
        Command::Rollback { revision, preview } => {
            let target = store::read_revision(profile, &workspace, *revision)?;
            require_preview(
                &rollback_preview(&base, current.as_ref(), &target, &workspace)?.0,
                preview,
            )?;
            let mut journal = store::Store::open(profile, &workspace)?;
            let previous = journal.snapshot().clone();
            let target = journal.revision(*revision)?;
            let (view, preferences) =
                rollback_preview(journal.base_bytes(), Some(&previous), &target, &workspace)?;
            require_preview(&view, preview)?;
            let base_hash = journal.base_sha256().to_owned();
            let next = journal.publish(
                previous.revision,
                &previous.revision_sha256,
                &base_hash,
                preferences,
                store::Provenance::Rollback {
                    revision: *revision,
                },
                BTreeSet::new(),
                Some(*revision),
            )?;
            Ok(
                json!({"kind":"import_rolled_back","current":next,"original_profile_preserved":true}),
            )
        }
    }
}
