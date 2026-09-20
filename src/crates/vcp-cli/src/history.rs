// SPDX-License-Identifier: Apache-2.0
//! One selector parser for browsing, previews and explicit saved policies.
use clap::{Args, Subcommand, ValueEnum};
use vcp_domain::{retention_selector::*, *};
use vcp_lifecycle::foundation::history_retention::Request;
type Result<T> = std::result::Result<T, String>;

pub fn terminal_request(words: Vec<String>, workspace: &WorkspaceId) -> Result<Request> {
    use clap::Parser;
    let parsed = crate::args::Cli::try_parse_from(std::iter::once("vcp".to_owned()).chain(words))
        .map_err(|e| e.to_string())?;
    if parsed.workspace != std::path::Path::new(".")
        || parsed.data_dir.is_some()
        || parsed.config.is_some()
        || parsed.control_stdin
    {
        return Err("terminal controls use the active workspace; use a separate CLI invocation to select another owner".into());
    }
    match parsed.command {
        Some(crate::args::Command::History { command }) => command.request(workspace),
        Some(crate::args::Command::Prune { command }) => command.request(),
        Some(crate::args::Command::Retention { command }) => command.request(),
        Some(crate::args::Command::Memory {
            command: crate::args::Memory::Inspect(inspect),
        }) => inspect.request(),
        Some(crate::args::Command::Memory {
            command: crate::args::Memory::Prune(preview),
        }) => preview.memory_request(workspace),
        _ => Err("history, prune or retention control expected".into()),
    }
}

#[derive(Debug, Args)]
pub struct MemoryInspect {
    pub claim: String,
    #[arg(long, default_value_t = 16, value_parser = clap::value_parser!(u32).range(1..=32))]
    pub limit: u32,
    #[arg(long)]
    pub cursor: Option<String>,
}
impl MemoryInspect {
    pub fn request(&self) -> Result<Request> {
        if self.cursor.as_ref().is_some_and(|value| value.len() > 8192) {
            return Err("memory cursor too large".into());
        }
        Ok(Request::Memory {
            claim: ClaimId::parse(&self.claim).map_err(|e| e.to_string())?,
            limit: self.limit,
            cursor: self
                .cursor
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(|_| "invalid memory cursor")?,
        })
    }
}
pub fn next_request(request: Request, result: &serde_json::Value) -> Result<Option<Request>> {
    Ok(match request {
        Request::History { mut query } if !result["next_cursor"].is_null() => {
            query.cursor = Some(
                serde_json::from_value(result["next_cursor"].clone()).map_err(|e| e.to_string())?,
            );
            Some(Request::History { query })
        }
        Request::Memory { claim, limit, .. } if !result["next_cursor"].is_null() => {
            Some(Request::Memory {
                claim,
                limit,
                cursor: Some(
                    serde_json::from_value(result["next_cursor"].clone())
                        .map_err(|e| e.to_string())?,
                ),
            })
        }
        Request::PreviewPage { preview, limit, .. } => {
            let offset = result["next_offset"]
                .as_u64()
                .map(u32::try_from)
                .transpose()
                .map_err(|_| "preview offset overflow")?;
            offset.map(|offset| Request::PreviewPage {
                preview,
                limit,
                offset,
            })
        }
        _ => None,
    })
}

#[derive(Clone, Debug, Default, Args)]
pub struct Filter {
    /// Versioned selector JSON; combines with the explicit fields using AND.
    /// Raw browsing rejects root, claim kind/status, and supersession predicates.
    #[arg(long)]
    pub selector: Option<String>,
    /// Inclusive timestamp; date-only input requires --utc-offset-minutes.
    #[arg(long)]
    pub since: Option<String>,
    /// Exclusive timestamp; use RFC3339 with Z/offset or an explicit date offset.
    #[arg(long)]
    pub before: Option<String>,
    /// Fixed offset, never an inferred local timezone or ambiguous DST zone.
    #[arg(long,allow_hyphen_values=true,value_parser=clap::value_parser!(i16).range(-840..=840))]
    pub utc_offset_minutes: Option<i16>,
    #[arg(long)]
    pub task: Option<String>,
    /// Source root for pruning; unavailable in raw history list/search.
    #[arg(long)]
    pub root: Option<String>,
    #[arg(long)]
    pub path: Option<String>,
    #[arg(long)]
    pub actor: Option<String>,
    #[arg(long)]
    pub agent: Option<String>,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub provider: Option<String>,
    #[arg(long)]
    pub event: Option<String>,
    /// Typed claim kind for pruning (snake_case); unavailable in raw browsing.
    #[arg(long)]
    pub claim: Option<String>,
}
impl Filter {
    pub fn normalized(&self, workspace: &WorkspaceId) -> Result<Selector> {
        let mut tree = vec![Tree::Match(Criterion::Workspace(workspace.clone()))];
        if let Some(json) = &self.selector {
            if json.len() > 32768 {
                return Err("selector exceeds 32 KiB".into());
            }
            let s: Selector = serde_json::from_str(json).map_err(|e| e.to_string())?;
            tree.push(s.normalized().map_err(|e| e.to_string())?.tree);
        }
        let parse =
            |s: &str| InstantSpec::parse(s, self.utc_offset_minutes).map_err(|e| e.to_string());
        if self.since.is_some() || self.before.is_some() {
            tree.push(Tree::Match(Criterion::Date(TimeWindow {
                lower: self
                    .since
                    .as_deref()
                    .map(|s| {
                        parse(s).map(|instant| Bound {
                            instant,
                            inclusive: true,
                        })
                    })
                    .transpose()?,
                upper: self
                    .before
                    .as_deref()
                    .map(|s| {
                        parse(s).map(|instant| Bound {
                            instant,
                            inclusive: false,
                        })
                    })
                    .transpose()?,
            })));
        }
        macro_rules! id {
            ($field:ident,$ty:ident,$variant:ident) => {
                if let Some(s) = &self.$field {
                    tree.push(Tree::Match(Criterion::$variant(
                        $ty::parse(s).map_err(|e| e.to_string())?,
                    )));
                }
            };
        }
        id!(task, TaskId, Task);
        id!(root, RootId, Root);
        id!(actor, ActorId, Actor);
        id!(agent, AgentId, Agent);
        for criterion in [
            self.path.clone().map(Criterion::Path),
            self.model.clone().map(Criterion::Model),
            self.provider.clone().map(Criterion::Provider),
            self.event.clone().map(Criterion::Event),
        ]
        .into_iter()
        .flatten()
        {
            tree.push(Tree::Match(criterion));
        }
        if let Some(kind) = &self.claim {
            tree.push(Tree::Match(Criterion::Claim(
                serde_json::from_value(serde_json::json!(kind))
                    .map_err(|_| "unknown claim kind")?,
            )));
        }
        Selector {
            schema_version: 1,
            tree: Tree::All(tree),
        }
        .normalized()
        .map_err(|e| e.to_string())
    }
}
#[derive(Clone, Debug, Args)]
pub struct Browse {
    /// Expand presentation-compacted events; this never restores purged content.
    #[arg(long)]
    pub expand_compacted: bool,
    #[command(flatten)]
    pub filter: Filter,
    #[arg(long,default_value_t=32,value_parser=clap::value_parser!(u32).range(1..=128))]
    pub limit: u32,
    /// Exact JSON cursor from the preceding page. New events are reported separately.
    #[arg(long)]
    pub cursor: Option<String>,
    /// Backlink from a retained artifact ID to its source history events.
    #[arg(long)]
    pub artifact: Option<String>,
}
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Action {
    Exclude,
    RestoreRecall,
    Compact,
    Purge,
}
impl Action {
    fn service(self) -> vcp_memory::retention::Action {
        match self {
            Self::Exclude => vcp_memory::retention::Action::Exclude,
            Self::RestoreRecall => vcp_memory::retention::Action::RestoreRecall,
            Self::Compact => vcp_memory::retention::Action::Compact,
            Self::Purge => vcp_memory::retention::Action::Purge,
        }
    }
}
#[derive(Debug, Args)]
pub struct Preview {
    #[command(flatten)]
    pub filter: Filter,
    /// Persist an exact preview; this does not apply the selected action.
    #[arg(long, required = true)]
    pub preview: bool,
    #[arg(long, value_enum, default_value = "purge")]
    pub action: Action,
}
#[derive(Debug, Subcommand)]
pub enum History {
    List(Browse),
    Search {
        text: String,
        #[command(flatten)]
        browse: Browse,
    },
    Prune(Preview),
}
#[derive(Debug, Subcommand)]
pub enum Prune {
    Show {
        preview_id: String,
        #[arg(long, default_value_t = 0)]
        offset: u32,
        #[arg(long,default_value_t=64,value_parser=clap::value_parser!(u32).range(1..=128))]
        limit: u32,
    },
    Apply {
        preview_id: String,
    },
    Cleanup {
        receipt_id: String,
    },
}
#[derive(Debug, Subcommand)]
pub enum Retention {
    Show,
    Set {
        /// Required current saved revision, omitted only for the first saved policy.
        #[arg(long)]
        expected_revision: Option<u64>,
        #[arg(long,default_value_t=7,value_parser=clap::value_parser!(u32).range(1..=365))]
        notice_repeat_days: u32,
        /// Versioned Automatic JSON {selector,action,cadence_days}; explicit opt-in.
        #[arg(
            long,
            conflicts_with = "notification_only",
            required_unless_present = "notification_only"
        )]
        automatic: Option<String>,
        #[arg(long)]
        notification_only: bool,
    },
}
impl Browse {
    fn request(&self, workspace: &WorkspaceId, text: Option<String>) -> Result<Request> {
        if text
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.len() > 512 || s.contains('\0'))
        {
            return Err("search text must be 1..512 bytes".into());
        }
        if self.cursor.as_ref().is_some_and(|c| c.len() > 8192) {
            return Err("history cursor too large".into());
        }
        Ok(Request::History {
            query: vcp_audit::history_query::Query {
                selector: self.filter.normalized(workspace)?,
                text,
                limit: self.limit,
                cursor: self
                    .cursor
                    .as_deref()
                    .map(serde_json::from_str)
                    .transpose()
                    .map_err(|_| "invalid history cursor")?,
                expand_compacted: self.expand_compacted,
                artifact: self
                    .artifact
                    .as_ref()
                    .map(ArtifactId::parse)
                    .transpose()
                    .map_err(|e| e.to_string())?,
            },
        })
    }
}
impl Preview {
    pub fn memory_request(&self, workspace: &WorkspaceId) -> Result<Request> {
        use vcp_domain::memory::ClaimKind;
        let Request::Preview { selector, action } = self.request(workspace)? else {
            return Err("invalid preview".into());
        };
        let claims = [
            ClaimKind::Command,
            ClaimKind::ModuleRelationship,
            ClaimKind::Architecture,
            ClaimKind::EnvironmentConstraint,
            ClaimKind::VerifiedFix,
            ClaimKind::UserPreference,
        ];
        let selector = Selector {
            schema_version: 1,
            tree: Tree::All(vec![
                selector.tree,
                Tree::Any(
                    claims
                        .into_iter()
                        .map(|kind| Tree::Match(Criterion::Claim(kind)))
                        .collect(),
                ),
            ]),
        }
        .normalized()
        .map_err(|e| e.to_string())?;
        Ok(Request::Preview { selector, action })
    }
    pub fn request(&self, workspace: &WorkspaceId) -> Result<Request> {
        if !self.preview {
            return Err("pruning requires --preview first".into());
        }
        Ok(Request::Preview {
            selector: self.filter.normalized(workspace)?,
            action: self.action.service(),
        })
    }
}
impl History {
    pub fn request(&self, workspace: &WorkspaceId) -> Result<Request> {
        match self {
            Self::List(b) => b.request(workspace, None),
            Self::Search { text, browse } => browse.request(workspace, Some(text.clone())),
            Self::Prune(p) => p.request(workspace),
        }
    }
}
impl Prune {
    pub fn request(&self) -> Result<Request> {
        let (id, cleanup) = match self {
            Self::Apply { preview_id } => (preview_id, false),
            Self::Show { preview_id, .. } => (preview_id, false),
            Self::Cleanup { receipt_id } => (receipt_id, true),
        };
        let hash = if cleanup {
            id.strip_prefix("prune-")
                .ok_or("exact prune receipt identity required")?
        } else {
            id
        };
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err("exact preview/receipt SHA-256 identity required".into());
        }
        if let Self::Show {
            preview_id,
            offset,
            limit,
        } = self
        {
            return Ok(Request::PreviewPage {
                preview: preview_id.clone(),
                offset: *offset,
                limit: *limit,
            });
        }
        Ok(if cleanup {
            Request::Cleanup {
                receipt: id.clone(),
            }
        } else {
            Request::Apply {
                preview: id.clone(),
            }
        })
    }
}
impl Retention {
    pub fn request(&self) -> Result<Request> {
        Ok(match self {
            Self::Show => Request::Policy,
            Self::Set {
                expected_revision,
                notice_repeat_days,
                automatic,
                ..
            } => {
                if automatic.as_ref().is_some_and(|v| v.len() > 32768) {
                    return Err("automatic policy too large".into());
                }
                Request::SetPolicy {
                    expected: expected_revision.map(Revision::new),
                    notice_repeat_days: *notice_repeat_days,
                    automatic: automatic
                        .as_deref()
                        .map(serde_json::from_str)
                        .transpose()
                        .map_err(|_| "invalid automatic policy")?,
                }
            }
        })
    }
}
