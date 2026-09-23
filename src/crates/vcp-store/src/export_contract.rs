// SPDX-License-Identifier: Apache-2.0
//! Derived exports are usable only while every captured source remains visible.
//! This is a read guard, not retention immunity or secret-content sanitization.
use crate::{contract::*, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use vcp_domain::{artifact::ArtifactDescriptor, task::Task, workspace::*, *};
use vcp_protocol::{canonical_bytes, digest_bytes, event::EventKind};

pub const PAYLOAD_SCHEMA: &str = "vcp-session-export/1";
pub const MANIFEST_SCHEMA: &str = "vcp-session-export-visibility/1";
pub const MAX_EVENTS: usize = 4096;
pub const MAX_ARTIFACTS: usize = 128;
pub const MAX_BYTES: usize = 4 * 1024 * 1024;

/// Host-rendered output, never deserialized from a public request.
pub struct Rendered {
    pub sources: Sources,
    pub capture: vcp_protocol::methods::CaptureScope,
    pub payload: Vec<u8>,
    pub omissions: Vec<String>,
    pub complete: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Dependency {
    collection: Collection,
    id: String,
    digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sources {
    scope: Scope,
    task: Option<TaskId>,
    authority: AuthorityRevision,
    watermark: Watermark,
    tasks: BTreeSet<TaskId>,
    dependencies: Vec<Dependency>,
    events_digest: String,
    policy_digest: String,
}

fn hash<T: Serialize>(value: &T) -> Result<String> {
    let bytes = bounded_canonical(value, MAX_BYTES)?;
    Ok(digest_bytes(&bytes))
}
fn bounded_canonical<T: Serialize>(value: &T, limit: usize) -> Result<Vec<u8>> {
    struct Count {
        remaining: usize,
        exceeded: bool,
    }
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.remaining {
                self.exceeded = true;
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "export source limit",
                ));
            }
            self.remaining -= bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    // Preflight borrows strings/trees and allocates no serialized buffer.
    // canonical_bytes subsequently clones/sorts JSON, so it is called only
    // once that entire input has proved to fit the remaining source budget.
    let mut count = Count {
        remaining: limit,
        exceeded: false,
    };
    if let Err(error) = serde_json::to_writer(&mut count, value) {
        return if count.exceeded {
            Err(Error::Limit("export source metadata bytes"))
        } else {
            Err(error.into())
        };
    }
    let bytes = canonical_bytes(value)?;
    if bytes.len() > limit {
        return Err(Error::Limit("export source metadata bytes"));
    }
    Ok(bytes)
}
fn hash_sequence<'a, T: Serialize + 'a>(values: impl IntoIterator<Item = &'a T>) -> Result<String> {
    let mut digests = Vec::new();
    let mut remaining = MAX_BYTES;
    for value in values {
        if digests.len() == MAX_EVENTS {
            return Err(Error::Limit("export source count"));
        }
        // One already-bounded canonical record/event at a time, never one
        // allocation containing the entire retained raw history.
        let bytes = bounded_canonical(value, remaining)?;
        remaining = remaining
            .checked_sub(bytes.len())
            .ok_or(Error::Limit("export source metadata bytes"))?;
        digests.push(digest_bytes(&bytes));
    }
    hash(&digests)
}
fn policies(state: &State, workspace: &WorkspaceId) -> Result<String> {
    // Lease churn does not revoke content. Every other authority document and
    // retention marker does: no new grant/policy/tombstone can evade the guard.
    hash_sequence(state.records.values().filter(|row| {
        &row.workspace == workspace
            && (row.collection == Collection::Tombstone
                || (row.collection == Collection::Access
                    && row.value["document_type"] != vcp_domain::controller::DOCUMENT_TYPE))
    }))
}
impl Sources {
    pub fn capture(
        state: &State,
        scope: Scope,
        task: Option<TaskId>,
        authority: AuthorityRevision,
    ) -> Result<Self> {
        let workspace: Workspace = state
            .record(
                Collection::Workspace,
                scope.workspace.as_str(),
                &scope.workspace,
            )?
            .decode()?;
        workspace.validate()?;
        if workspace.authority != authority {
            return Err(Error::Access);
        }
        let session: Session = state
            .record(
                Collection::Session,
                scope.session.as_str(),
                &scope.workspace,
            )?
            .decode()?;
        if session.id != scope.session || session.workspace != scope.workspace {
            return Err(Error::Access);
        }
        let mut tasks = BTreeSet::new();
        let mut records = vec![
            state.record(
                Collection::Workspace,
                scope.workspace.as_str(),
                &scope.workspace,
            )?,
            state.record(
                Collection::Session,
                scope.session.as_str(),
                &scope.workspace,
            )?,
        ];
        for row in state
            .records
            .values()
            .filter(|r| r.workspace == scope.workspace && r.collection == Collection::Task)
        {
            let value: Task = row.decode()?;
            value.validate()?;
            if value.scope.workspace != row.workspace
                || value.scope.task.as_str() != row.id
                || value.revision != row.revision
            {
                return Err(Error::Corruption("export task identity"));
            }
            if value.scope.session == scope.session
                && task.as_ref().is_none_or(|id| id == &value.scope.task)
            {
                if value.redaction.is_some() {
                    return Err(Error::Access);
                }
                tasks.insert(value.scope.task);
                if tasks.len() > MAX_EVENTS {
                    return Err(Error::Limit("export tasks"));
                }
                records.push(row);
            }
        }
        if !tasks.contains(&scope.task) || task.as_ref().is_some_and(|id| id != &scope.task) {
            return Err(Error::Access);
        }
        let mut count = 0;
        for row in state
            .records
            .values()
            .filter(|r| r.workspace == scope.workspace && r.collection == Collection::Artifact)
        {
            let value: ArtifactDescriptor = row.decode()?;
            value.validate()?;
            if value.spec.scope.workspace != row.workspace || value.spec.id.as_str() != row.id {
                return Err(Error::Corruption("export artifact identity"));
            }
            if value.spec.scope.session == scope.session && tasks.contains(&value.spec.scope.task) {
                // Never recursively copy aggregate exports or forecasts.
                if reserved(&value.spec.schema)
                    || value.spec.schema == "vcp-optimization-forecast-v1"
                {
                    continue;
                }
                count += 1;
                if count > MAX_ARTIFACTS {
                    return Err(Error::Limit("export artifacts"));
                }
                records.push(row);
            }
        }
        let mut remaining = MAX_BYTES;
        let mut source = Self {
            scope,
            task,
            authority,
            watermark: state.watermark,
            tasks,
            dependencies: records
                .into_iter()
                .map(|r| {
                    let bytes = bounded_canonical(r, remaining)?;
                    remaining = remaining
                        .checked_sub(bytes.len())
                        .ok_or(Error::Limit("export dependency bytes"))?;
                    Ok(Dependency {
                        collection: r.collection,
                        id: r.id.clone(),
                        digest: digest_bytes(&bytes),
                    })
                })
                .collect::<Result<_>>()?,
            events_digest: String::new(),
            policy_digest: String::new(),
        };
        source.events_digest = hash_sequence(source.events(state)?)?;
        source.policy_digest = policies(state, &source.scope.workspace)?;
        Ok(source)
    }
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn task(&self) -> Option<&TaskId> {
        self.task.as_ref()
    }
    pub fn tasks(&self) -> &BTreeSet<TaskId> {
        &self.tasks
    }
    pub fn watermark(&self) -> Watermark {
        self.watermark
    }
    pub fn events<'a>(
        &self,
        state: &'a State,
    ) -> Result<Vec<&'a vcp_protocol::event::EventEnvelope>> {
        let mut events = Vec::new();
        for event in state.events.iter().filter(|e| {
            e.watermark <= self.watermark
                && e.event.workspace == self.scope.workspace
                && e.event.session == self.scope.session
                && (e
                    .event
                    .task
                    .as_ref()
                    .is_some_and(|id| self.tasks.contains(id))
                    || (self.task.is_none() && e.event.task.is_none()))
        }) {
            if events.len() == MAX_EVENTS {
                return Err(Error::Limit("export events"));
            }
            events.push(event);
        }
        Ok(events)
    }
    pub fn artifacts(&self, state: &State) -> Result<Vec<ArtifactDescriptor>> {
        self.dependencies
            .iter()
            .filter(|d| d.collection == Collection::Artifact)
            .map(|d| {
                state
                    .record(Collection::Artifact, &d.id, &self.scope.workspace)?
                    .decode()
            })
            .collect()
    }
    pub fn validate_current(
        &self,
        state: &State,
        authority: AuthorityRevision,
        allowed: Option<&BTreeSet<TaskId>>,
    ) -> Result<()> {
        if authority != self.authority
            || self.watermark > state.watermark
            || self.dependencies.len() > MAX_ARTIFACTS + MAX_EVENTS + 2
            || self.tasks.is_empty()
            || !self.tasks.contains(&self.scope.task)
            || allowed.is_some_and(|ids| !self.tasks.is_subset(ids))
        {
            return Err(Error::Access);
        }
        let mut seen = BTreeSet::new();
        let mut task_ids = BTreeSet::new();
        let mut workspace_count = 0;
        let mut session_count = 0;
        let mut artifact_count = 0;
        for dep in &self.dependencies {
            if !seen.insert(key(dep.collection, &dep.id)) {
                return Err(Error::Access);
            }
            match dep.collection {
                Collection::Workspace if dep.id == self.scope.workspace.as_str() => {
                    workspace_count += 1
                }
                Collection::Session if dep.id == self.scope.session.as_str() => session_count += 1,
                Collection::Task => {
                    let row = state.record(Collection::Task, &dep.id, &self.scope.workspace)?;
                    let task: Task = row.decode()?;
                    task.validate()?;
                    if task.scope.workspace != self.scope.workspace
                        || task.scope.session != self.scope.session
                        || task.scope.task.as_str() != dep.id
                        || task.revision != row.revision
                        || task.redaction.is_some()
                    {
                        return Err(Error::Access);
                    }
                    task_ids.insert(task.scope.task);
                }
                Collection::Artifact => {
                    artifact_count += 1;
                    let value: ArtifactDescriptor = state
                        .record(Collection::Artifact, &dep.id, &self.scope.workspace)?
                        .decode()?;
                    value.validate()?;
                    if value.spec.scope.workspace != self.scope.workspace
                        || value.spec.scope.session != self.scope.session
                        || value.spec.id.as_str() != dep.id
                        || !self.tasks.contains(&value.spec.scope.task)
                        || reserved(&value.spec.schema)
                        || value.spec.schema == "vcp-optimization-forecast-v1"
                    {
                        return Err(Error::Access);
                    }
                }
                _ => return Err(Error::Access),
            }
        }
        if workspace_count != 1
            || session_count != 1
            || task_ids != self.tasks
            || artifact_count > MAX_ARTIFACTS
            || self
                .task
                .as_ref()
                .is_some_and(|id| self.tasks.len() != 1 || id != &self.scope.task)
        {
            return Err(Error::Access);
        }
        let workspace: Workspace = state
            .record(
                Collection::Workspace,
                self.scope.workspace.as_str(),
                &self.scope.workspace,
            )?
            .decode()?;
        workspace.validate()?;
        let session: Session = state
            .record(
                Collection::Session,
                self.scope.session.as_str(),
                &self.scope.workspace,
            )?
            .decode()?;
        if workspace.authority != authority
            || workspace.id != self.scope.workspace
            || session.id != self.scope.session
            || session.workspace != self.scope.workspace
        {
            return Err(Error::Access);
        }
        for dep in &self.dependencies {
            if hash(state.record(dep.collection, &dep.id, &self.scope.workspace)?)? != dep.digest {
                return Err(Error::Access);
            }
        }
        if hash_sequence(self.events(state)?)? != self.events_digest
            || policies(state, &self.scope.workspace)? != self.policy_digest
        {
            return Err(Error::Access);
        }
        Ok(())
    }
}

/// Typed provenance in the one atomic acceptance event; payload and manifest
/// both depend on this proof. No reader trusts spool metadata alone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Acceptance {
    pub schema_version: u32,
    pub sources: Sources,
    pub capture: vcp_protocol::methods::CaptureScope,
    pub artifact: ArtifactDescriptor,
    pub visibility_manifest: ArtifactDescriptor,
    pub complete: bool,
}

pub fn reserved(schema: &str) -> bool {
    schema.starts_with("vcp-session-export")
}

pub fn validate_read(
    state: &State,
    authority: AuthorityRevision,
    allowed: Option<&BTreeSet<TaskId>>,
    artifact: &ArtifactDescriptor,
) -> Result<()> {
    let has_provenance = state.events.iter().any(|e| {
        e.event.workspace == artifact.spec.scope.workspace
            && e.event.artifacts.contains(&artifact.spec.id)
            && e.event.data.get("session_export").is_some()
    });
    if !reserved(&artifact.spec.schema) && !has_provenance {
        return Ok(());
    }
    acceptance(state, artifact)?
        .sources
        .validate_current(state, authority, allowed)
}

// Authenticate immutable acceptance separately from current disclosure. Retention
// must follow copies even after their original source hashes or permissions change.
fn acceptance(state: &State, artifact: &ArtifactDescriptor) -> Result<Acceptance> {
    if artifact.spec.schema != PAYLOAD_SCHEMA && artifact.spec.schema != MANIFEST_SCHEMA {
        return Err(Error::Access);
    }
    let mut found = None;
    for event in state.events.iter().filter(|e| {
        e.event.workspace == artifact.spec.scope.workspace
            && e.event.artifacts.contains(&artifact.spec.id)
    }) {
        if event.event.kind != EventKind::ArtifactAttached || event.redaction.is_some() {
            continue;
        }
        let Some(value) = event.event.data.get("session_export") else {
            continue;
        };
        let accepted: Acceptance = serde_json::from_value(value.clone())?;
        let receipt = state
            .commands
            .get(&command_key(
                &event.event.workspace,
                &event.event.correlation,
            ))
            .ok_or(Error::Access)?;
        if found.is_some()
            || accepted.schema_version != 1
            || accepted.sources.scope != artifact.spec.scope
            || event.event.session != artifact.spec.scope.session
            || event.event.task.as_ref() != Some(&artifact.spec.scope.task)
            || receipt.watermark != event.watermark
            || receipt.first_event != event.sequence
            || receipt.last_event != event.sequence
            || accepted.sources.watermark.next()? != event.watermark
            || !matches!(
                receipt.result,
                vcp_protocol::command::CommandResult::Accepted { .. }
            )
            || (artifact != &accepted.artifact && artifact != &accepted.visibility_manifest)
            || event.event.data["schema_version"] != 1
            || accepted.artifact.spec.schema != PAYLOAD_SCHEMA
            || accepted.visibility_manifest.spec.schema != MANIFEST_SCHEMA
            || accepted.artifact.spec.id == accepted.visibility_manifest.spec.id
            || event.event.artifacts
                != [
                    accepted.artifact.spec.id.clone(),
                    accepted.visibility_manifest.spec.id.clone(),
                ]
        {
            return Err(Error::Access);
        }
        for descriptor in [&accepted.artifact, &accepted.visibility_manifest] {
            if descriptor.spec.scope != artifact.spec.scope
                || descriptor.state != vcp_domain::artifact::CaptureState::Complete
            {
                return Err(Error::Access);
            }
            let current: ArtifactDescriptor = state
                .record(
                    Collection::Artifact,
                    descriptor.spec.id.as_str(),
                    &artifact.spec.scope.workspace,
                )?
                .decode()?;
            if current != *descriptor {
                return Err(Error::Access);
            }
            let facts = event.event.data["facts"].as_array().ok_or(Error::Access)?;
            let revision = serde_json::to_value(Revision::ZERO)?;
            let value = serde_json::to_value(descriptor)?;
            if facts.len() != 2
                || !facts.iter().any(|fact| {
                    fact["collection"] == "artifact"
                        && fact["id"] == descriptor.spec.id.as_str()
                        && fact["revision"] == revision
                        && fact["value"] == value
                })
            {
                return Err(Error::Access);
            }
        }
        found = Some(accepted);
    }
    found.ok_or(Error::Access)
}

/// Trusted retention lineage only: this never authorizes reading source bytes.
pub struct ExportDependencies {
    pub artifacts: [ArtifactId; 2],
    pub records: BTreeSet<String>,
    pub events: BTreeSet<EventId>,
}

/// Follow every still-retained export's frozen lineage, including exports whose
/// read guard has become stale. Malformed/missing acceptance cannot make a copy
/// disappear from the physical-purge obligation. Callers must authorize the full
/// transitive closure before changing any source or dependent output.
pub fn retention_dependencies(
    state: &State,
    workspace: &WorkspaceId,
) -> Result<Vec<ExportDependencies>> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for row in state
        .records
        .values()
        .filter(|row| row.workspace == *workspace && row.collection == Collection::Artifact)
    {
        let artifact: ArtifactDescriptor = row.decode()?;
        if artifact.state == vcp_domain::artifact::CaptureState::Purged {
            continue;
        }
        let marked = state.events.iter().any(|event| {
            event.event.workspace == *workspace
                && event.event.artifacts.contains(&artifact.spec.id)
                && event.event.data.get("session_export").is_some()
        });
        if !reserved(&artifact.spec.schema) && !marked {
            continue;
        }
        let accepted = acceptance(state, &artifact)?;
        if !seen.insert(accepted.artifact.spec.id.clone()) {
            continue;
        }
        if result.len() == MAX_EVENTS {
            return Err(Error::Limit("retained export lineage count"));
        }
        let sources = &accepted.sources;
        if sources.scope.workspace != *workspace
            || sources.tasks.is_empty()
            || sources.tasks.len() > MAX_EVENTS
            || !sources.tasks.contains(&sources.scope.task)
            || sources.dependencies.len() > MAX_EVENTS + MAX_ARTIFACTS + 2
            || sources
                .task
                .as_ref()
                .is_some_and(|task| task != &sources.scope.task || sources.tasks.len() != 1)
        {
            return Err(Error::Access);
        }
        let mut records = BTreeSet::new();
        let mut tasks = BTreeSet::new();
        let mut artifacts = 0usize;
        for dep in &sources.dependencies {
            if !vcp_domain::accounting::valid_hash(&dep.digest)
                || !records.insert(key(dep.collection, &dep.id))
            {
                return Err(Error::Access);
            }
            match dep.collection {
                Collection::Workspace if dep.id == workspace.as_str() => (),
                Collection::Session if dep.id == sources.scope.session.as_str() => (),
                Collection::Task => {
                    tasks.insert(TaskId::parse(&dep.id)?);
                }
                Collection::Artifact => {
                    ArtifactId::parse(&dep.id)?;
                    artifacts += 1;
                }
                _ => return Err(Error::Access),
            }
        }
        if tasks != sources.tasks
            || artifacts > MAX_ARTIFACTS
            || !records.contains(&key(Collection::Workspace, workspace.as_str()))
            || !records.contains(&key(Collection::Session, sources.scope.session.as_str()))
        {
            return Err(Error::Access);
        }
        // Rewrites preserve event identity/scope. Current contents and hashes may
        // already be redacted; those changes must not erase the lineage edge.
        let events = sources
            .events(state)?
            .into_iter()
            .map(|event| event.event.id.clone())
            .collect();
        result.push(ExportDependencies {
            artifacts: [
                accepted.artifact.spec.id,
                accepted.visibility_manifest.spec.id,
            ],
            records,
            events,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oversized_serialization_never_reaches_canonical_clone() {
        struct Observed {
            text: String,
            calls: std::cell::Cell<usize>,
        }
        impl Serialize for Observed {
            fn serialize<S: serde::Serializer>(
                &self,
                serializer: S,
            ) -> std::result::Result<S::Ok, S::Error> {
                self.calls.set(self.calls.get() + 1);
                serializer.serialize_str(&self.text)
            }
        }
        let input = Observed {
            text: "x".repeat(MAX_BYTES + 1),
            calls: std::cell::Cell::new(0),
        };
        assert!(matches!(
            bounded_canonical(&input, MAX_BYTES),
            Err(Error::Limit(_))
        ));
        assert_eq!(
            input.calls.get(),
            1,
            "canonical Value construction must never start after capped preflight rejects"
        );
    }
}
