// SPDX-License-Identifier: Apache-2.0
//! Side-effect-free native tool preparation. A plan is data, not authority.
pub mod integration;
pub mod patch;
pub mod process;
pub mod read;
pub mod schema;
pub mod verification;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};
use vcp_domain::{policy::*, workspace::Scope, *};
use vcp_repository::{instructions::Probe, Root};

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid tool input: {0}")]
    Invalid(&'static str),
    #[error("patch: {0}")]
    Patch(String),
    #[error(transparent)]
    Repository(#[from] vcp_repository::Error),
    #[error(transparent)]
    Policy(#[from] vcp_policy::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "tool", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Read { path: String, max_bytes: u64 },
    List { path: String, max_entries: usize },
    Search { query: String, max_hits: usize },
    Patch { patch: String },
}
impl Request {
    pub fn tool(&self) -> &'static str {
        match self {
            Self::Read { .. } => "vcp_read",
            Self::List { .. } => "vcp_list",
            Self::Search { .. } => "vcp_search",
            Self::Patch { .. } => "vcp_patch",
        }
    }
}
/// Supplied from the current canonical task, never parsed from tool arguments.
pub struct Identity {
    pub scope: Scope,
    pub actor: ActorId,
    pub host: HostId,
    pub binding: Revision,
    pub authority: AuthorityRevision,
    pub steering: SteeringRevision,
    pub policy: PolicyRevision,
}
pub struct Prepared {
    authority: vcp_policy::Prepared,
    root: Root,
    request: Request,
    probes: Vec<Probe>,
    result: serde_json::Value,
    changes: Vec<patch::Change>,
    integration_index: Option<(Root, vcp_repository::FileVersion)>,
    integration_parent: Option<(vcp_repository::observation::Manifest, Vec<Probe>)>,
    integration_child: Option<TaskId>,
}
impl Prepared {
    pub fn integration_child(&self) -> Option<&TaskId> {
        self.integration_child.as_ref()
    }
    pub fn authority(&self) -> &vcp_policy::Prepared {
        &self.authority
    }
    pub fn root(&self) -> &Root {
        &self.root
    }
    pub fn changes(&self) -> &[patch::Change] {
        &self.changes
    }
    pub fn proposed_result(&self) -> &serde_json::Value {
        &self.result
    }
    pub fn revalidate(&self) -> Result<()> {
        self.revalidate_index()?;
        if let Some((manifest, probes)) = &self.integration_parent {
            vcp_repository::merge::revalidate_parent(&self.root, manifest, probes)?;
        }
        vcp_repository::instructions::revalidate_probes(
            &self.probes,
            std::slice::from_ref(&self.root),
        )?;
        match &self.request {
            Request::List { path, max_entries } => {
                if read::list(&self.root, path, *max_entries)? != self.result {
                    return Err(vcp_repository::Error::Stale.into());
                }
            }
            Request::Search { query, max_hits } => {
                if read::search(&self.root, query, *max_hits)?.0 != self.result {
                    return Err(vcp_repository::Error::Stale.into());
                }
            }
            _ => (),
        }
        Ok(())
    }
    /// Integration never writes the index, but each mutation still depends on
    /// the index version shown in its preview.
    pub fn revalidate_index(&self) -> Result<()> {
        if let Some((root, version)) = &self.integration_index {
            root.revalidate(version)?;
        }
        Ok(())
    }
    pub fn hold_index(&self) -> Result<Option<vcp_repository::path::HeldPath>> {
        let held = self
            .integration_index
            .as_ref()
            .map(|(root, version)| root.hold(Some(Path::new(&version.path)), false))
            .transpose()?;
        self.revalidate_index()?;
        Ok(held)
    }
    /// Complete proposed bytes for canonical artifact capture before dispatch.
    pub fn evidence(&self) -> Result<Vec<u8>> {
        Ok(vcp_protocol::canonical_bytes(
            &serde_json::json!({"schema_version":1,"operation":self.authority.operation(),"result":self.result,"changes":self.changes}),
        )?)
    }
}
pub fn prepare(
    root: Root,
    identity: Identity,
    request: Request,
    output_bytes: ByteCount,
) -> Result<Prepared> {
    if root.identity.workspace != identity.scope.workspace
        || root.identity.binding != identity.binding
        || output_bytes == ByteCount::ZERO
        || output_bytes.get() > 8 * 1024 * 1024
    {
        return Err(Error::Invalid("scope or output ceiling"));
    }
    let args = String::from_utf8(vcp_protocol::canonical_bytes(&request)?).unwrap();
    if args.len() > 256 * 1024 {
        return Err(Error::Invalid("arguments exceed 256 KiB"));
    }
    let mut probes = vec![];
    let mut changes = vec![];
    let result = match &request {
        Request::Read { path, max_bytes } => {
            checked_path(path, false)?;
            if *max_bytes == 0 || *max_bytes > 1024 * 1024 {
                return Err(Error::Invalid("read ceiling"));
            }
            let source = root.read(Path::new(path), *max_bytes)?;
            probes.push(probe(&root, path, Some(source.version.clone())));
            let text = std::str::from_utf8(&source.bytes).map_err(|_| {
                Error::Invalid("read requires UTF-8 text; binary capture is not enabled")
            })?;
            serde_json::json!({"text":text,"version":source.version,"complete":true})
        }
        Request::List { path, max_entries } => read::list(&root, path, *max_entries)?,
        Request::Search { query, max_hits } => {
            let (result, dependencies) = read::search(&root, query, *max_hits)?;
            probes = dependencies;
            result
        }
        Request::Patch { patch } => {
            changes = patch::prepare(&root, patch)?;
            probes = changes.iter().flat_map(|c| c.probes.clone()).collect();
            serde_json::json!({"prepared_files":changes.len()})
        }
    };
    if vcp_protocol::canonical_bytes(&result)?.len() as u64 > output_bytes.get() {
        return Err(Error::Invalid("result exceeds output ceiling"));
    }
    let write = matches!(request, Request::Patch { .. });
    let mut resources: Vec<_> = probes
        .iter()
        .map(|p| {
            Ok(Resource {
                root: root.identity.root.clone(),
                path: p.path.clone(),
                write,
                version: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(p)?),
            })
        })
        .collect::<std::result::Result<_, serde_json::Error>>()?;
    resources.sort_by(|a, b| a.path.cmp(&b.path));
    resources.dedup_by(|a, b| a.path == b.path);
    if resources.is_empty() || matches!(request, Request::List { .. } | Request::Search { .. }) {
        resources.push(Resource {
            root: root.identity.root.clone(),
            path: String::new(),
            write: false,
            version: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&result)?),
        });
    }
    let authority = vcp_policy::Prepared::new(Operation {
        scope: identity.scope,
        actor: identity.actor,
        host: identity.host,
        binding: identity.binding,
        authority: identity.authority,
        steering: identity.steering,
        policy: identity.policy,
        tool: request.tool().into(),
        schema: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&schema::definition(
            request.tool(),
        )?)?),
        arguments: args,
        invocation: Invocation::Local,
        resources,
        effects: if write {
            BTreeSet::from([EffectClass::Read, EffectClass::Write])
        } else {
            BTreeSet::from([EffectClass::Read])
        },
        required_isolation: BTreeSet::from([Isolation::PathContainment, Isolation::OutputLimit]),
        timeout_ms: Units::new(30_000),
        output_bytes,
    })?;
    Ok(Prepared {
        authority,
        root,
        request,
        probes,
        result,
        changes,
        integration_index: None,
        integration_parent: None,
        integration_child: None,
    })
}
pub(crate) fn checked_path(path: &str, empty: bool) -> Result<()> {
    if !vcp_policy::relative(path) || (!empty && path.is_empty()) {
        return Err(Error::Invalid("relative path"));
    }
    // Windows DOS devices remain special even with an extension.
    if path.split('/').any(|p| {
        let name = p.split('.').next().unwrap_or("").to_ascii_uppercase();
        matches!(
            name.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) || name
            .strip_prefix("COM")
            .or_else(|| name.strip_prefix("LPT"))
            .is_some_and(|s| s.len() == 1 && s.as_bytes()[0].is_ascii_digit())
    }) {
        return Err(Error::Invalid("Windows device name"));
    }
    if path.split('/').any(|p| p.eq_ignore_ascii_case(".git")) {
        return Err(Error::Invalid("Git metadata requires a separate tool"));
    }
    Ok(())
}
pub(crate) fn probe(
    root: &Root,
    path: &str,
    observed: Option<vcp_repository::FileVersion>,
) -> Probe {
    Probe {
        root: root.identity.root.clone(),
        binding: root.identity.binding,
        path: path.into(),
        observed,
    }
}
