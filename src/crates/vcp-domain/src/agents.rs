// SPDX-License-Identifier: Apache-2.0
//! Durable child assignments are ceilings, never executable capabilities.
use crate::{policy::EffectClass, workspace::Scope, *};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const GRAPH: &str = "vcp_task_graph_v1";
pub fn graph_id(root: &TaskId) -> String {
    // Collection + typed document provide the namespace; retaining the opaque
    // root ID also supports the full 96-byte imported-ID range without truncation.
    root.to_string()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphLimits {
    pub depth: u32,
    pub concurrency: u32,
    pub nodes: u32,
}
impl Default for GraphLimits {
    fn default() -> Self {
        Self {
            depth: 4,
            concurrency: 4,
            nodes: 64,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildMode {
    ReadOnly,
    IsolatedWrite,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildPath {
    pub root: RootId,
    pub path: String,
    pub write: bool,
}
impl ChildPath {
    pub fn covers(&self, other: &Self) -> bool {
        self.root == other.root && (!other.write || self.write) && prefix(&self.path, &other.path)
    }
    pub fn overlaps(&self, other: &Self) -> bool {
        self.root == other.root
            && (self.write || other.write)
            && (!self.path.is_ascii()
                || !other.path.is_ascii()
                || prefix(&self.path, &other.path)
                || prefix(&other.path, &self.path))
    }
}
pub fn prefix(parent: &str, child: &str) -> bool {
    // Match the policy grant boundary: Unicode spelling is exact until native
    // ordinal path identity is qualified; Unicode lowercase is not Windows folding.
    if !parent.is_ascii() || !child.is_ascii() {
        return parent.is_empty()
            || child == parent
            || child
                .strip_prefix(parent)
                .is_some_and(|rest| rest.starts_with('/'));
    }
    let parent = parent.to_lowercase();
    let child = child.to_lowercase();
    parent.is_empty() || child == parent || child.starts_with(&(parent + "/"))
}
fn safe_path(path: &str) -> bool {
    path.is_empty()
        || (path.len() <= 4096
            && path.split('/').all(|part| {
                let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
                !part.is_empty()
                    && !matches!(part, "." | "..")
                    && !part.ends_with(['.', ' '])
                    && !part.chars().any(|c| {
                        c.is_control()
                            || matches!(c, '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
                    })
                    && !matches!(
                        stem.as_str(),
                        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
                    )
                    && !stem
                        .strip_prefix("COM")
                        .or_else(|| stem.strip_prefix("LPT"))
                        .is_some_and(|s| s.len() == 1 && s.as_bytes()[0].is_ascii_digit())
            }))
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildSpec {
    pub parent: TaskId,
    pub actor: ActorId,
    pub parent_steering: SteeringRevision,
    pub dependencies: BTreeSet<TaskId>,
    pub mode: ChildMode,
    pub role: String,
    /// Exact qualified model ID. This is a fixed ceiling, not a routing label.
    pub model_policy: String,
    /// Source-root paths. A host may map them only into the registered child root.
    pub paths: Vec<ChildPath>,
    pub effects: BTreeSet<EffectClass>,
    pub authority: AuthorityRevision,
    pub policy: PolicyRevision,
    pub binding: Revision,
    pub grants: BTreeMap<GrantId, Revision>,
    pub allocation: Micros,
    pub deadline: Timestamp,
    pub snapshot: ArtifactId,
    pub snapshot_digest: String,
    /// Immutable host registration artifact; no path supplied by a model is authority.
    pub registration: Option<ArtifactId>,
    pub registration_digest: Option<String>,
    pub isolated_root: Option<RootId>,
}
impl ChildSpec {
    pub fn validate(&self) -> Result<()> {
        if self.role.trim().is_empty()
            || self.role.len() > 128
            || self.model_policy.trim().is_empty()
            || self.model_policy.len() > 256
            || self.paths.is_empty()
            || self.paths.len() > 128
            || self.paths.iter().any(|p| !safe_path(&p.path))
            || self.effects.is_empty()
            || self.dependencies.len() > 128
            || self.grants.len() > 128
            || self.allocation == Micros::ZERO
            || !accounting::valid_hash(&self.snapshot_digest)
            || self.registration.is_some() != self.registration_digest.is_some()
            || self
                .registration_digest
                .as_ref()
                .is_some_and(|digest| !accounting::valid_hash(digest))
            || (self.mode == ChildMode::ReadOnly
                && (self.paths.iter().any(|p| p.write)
                    || self.effects != BTreeSet::from([EffectClass::Read])))
            || (self.mode == ChildMode::IsolatedWrite
                && (self.registration.is_none() || self.isolated_root.is_none()))
        {
            return Err(Error::Invalid("child assignment"));
        }
        Ok(())
    }
    pub fn within(&self, parent: &Self) -> bool {
        self.model_policy == parent.model_policy
            && self.effects.is_subset(&parent.effects)
            && self.allocation <= parent.allocation
            && self
                .paths
                .iter()
                .all(|p| parent.paths.iter().any(|allowed| allowed.covers(p)))
            && self.deadline <= parent.deadline
            && (parent.mode != ChildMode::ReadOnly || self.mode == ChildMode::ReadOnly)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskGraph {
    pub document_type: String,
    pub schema_version: u32,
    pub scope: Scope,
    pub revision: Revision,
    pub limits: GraphLimits,
    pub children: BTreeMap<TaskId, ChildSpec>,
    /// Host-written receipts after native materialization and verification.
    pub ready: BTreeMap<TaskId, WorkspaceReady>,
    /// Append-only result evidence; a child report cannot complete its parent.
    #[serde(default)]
    pub results: BTreeMap<TaskId, Vec<ChildResultRef>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildResultRef {
    pub packet: ArtifactId,
    pub plan: ArtifactId,
    pub effect: Option<ToolRunId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceReady {
    pub snapshot_digest: String,
    pub registration_digest: Option<String>,
    pub native_identity: String,
}
impl TaskGraph {
    pub fn validate(&self) -> Result<()> {
        if self.document_type != GRAPH
            || self.schema_version != 1
            || !(1..=8).contains(&self.limits.depth)
            || !(1..=16).contains(&self.limits.concurrency)
            || !(1..=128).contains(&self.limits.nodes)
            || self.children.len() > self.limits.nodes as usize
            || self.children.contains_key(&self.scope.task)
            || self.results.iter().any(|(id, results)| {
                !self.children.contains_key(id) || results.is_empty() || results.len() > 128
            })
            || self.ready.iter().any(|(id, ready)| {
                self.children.get(id).is_none_or(|child| {
                    ready.snapshot_digest != child.snapshot_digest
                        || ready.registration_digest != child.registration_digest
                        || ready.native_identity.is_empty()
                        || ready.native_identity.len() > 256
                })
            })
        {
            return Err(Error::Invalid("task graph bounds"));
        }
        for (id, child) in &self.children {
            child.validate()?;
            if child.parent == *id
                || child.dependencies.contains(id)
                || child.dependencies.contains(&self.scope.task)
                || child
                    .dependencies
                    .iter()
                    .any(|d| !self.children.contains_key(d))
            {
                return Err(Error::Invalid("child graph edge"));
            }
            let mut cursor = id;
            let mut seen = BTreeSet::new();
            while cursor != &self.scope.task {
                if !seen.insert(cursor.clone()) || seen.len() > self.limits.depth as usize {
                    return Err(Error::Invalid("child depth or ancestry cycle"));
                }
                let row = self
                    .children
                    .get(cursor)
                    .ok_or(Error::Invalid("missing child parent"))?;
                if let Some(parent) = self.children.get(&row.parent) {
                    if !row.within(parent) {
                        return Err(Error::Scope);
                    }
                }
                cursor = &row.parent;
            }
            if child
                .dependencies
                .iter()
                .any(|dependency| seen.contains(dependency))
            {
                return Err(Error::Invalid(
                    "child cannot await completion of an ancestor",
                ));
            }
            self.visit(id, &mut BTreeSet::new(), &mut BTreeSet::new())?;
        }
        Ok(())
    }
    fn visit(
        &self,
        id: &TaskId,
        active: &mut BTreeSet<TaskId>,
        done: &mut BTreeSet<TaskId>,
    ) -> Result<()> {
        if done.contains(id) {
            return Ok(());
        }
        if !active.insert(id.clone()) {
            return Err(Error::Invalid("dependency cycle"));
        }
        let child = self
            .children
            .get(id)
            .ok_or(Error::Invalid("dependency target"))?;
        for dependency in &child.dependencies {
            self.visit(dependency, active, done)?;
        }
        if child.parent != self.scope.task {
            self.visit(&child.parent, active, done)?;
        }
        active.remove(id);
        done.insert(id.clone());
        Ok(())
    }
}
