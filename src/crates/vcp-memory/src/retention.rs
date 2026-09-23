// SPDX-License-Identifier: Apache-2.0
//! Exact revision-bound retention. Preview is read-only; apply must run on the
//! controller's serialized deletion/dispatch worker. No broad selector is rerun
//! by cleanup, and no payload is removed before its tombstone is durable.
use crate::{
    access::{self, Access},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{
    artifact::{ArtifactDescriptor, CaptureState},
    memory::{ProposalRecord, Version},
    retention_selector::{Facts, Selector, Truth},
    task::Task,
    workspace::Scope,
    *,
};
use vcp_protocol::{
    canonical_bytes, digest_bytes,
    event::{EventInput, EventKind},
};
use vcp_store::{
    contract::{key, CanonicalStore, Collection, Mutation, Record, State, Transaction},
    Store,
};
#[path = "retention_sources.rs"]
mod sources;
const PREVIEW: &str = "vcp_retention_preview_v1";
const JOB: &str = "vcp_retention_job_v1";
const DECISION: &str = "vcp_retention_decision_v1";
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Exclude,
    RestoreRecall,
    Compact,
    Purge,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Target {
    Record(String),
    Event(EventId),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Protected {
    pub target: Target,
    pub reason: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrunePreview {
    pub version: u32,
    pub id: String,
    pub workspace: WorkspaceId,
    pub actor: ActorId,
    pub authority: AuthorityRevision,
    pub watermark: Watermark,
    pub deletion: DeletionEpoch,
    pub selector: Selector,
    pub action: Action,
    pub selected: BTreeSet<Target>,
    pub dependent: BTreeSet<Target>,
    pub protected: Vec<Protected>,
    pub source_digest: String,
    pub retained_bytes: u64,
    pub bytes_are_exact: bool,
    pub created_at: Timestamp,
    pub backup_copies: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PruneReceipt {
    pub schema_version: u32,
    pub document_type: String,
    pub id: String,
    pub workspace: WorkspaceId,
    pub revision: Revision,
    pub preview: PrunePreview,
    pub deletion: DeletionEpoch,
    pub applied_at: Timestamp,
    pub logical_unavailable: bool,
    pub rewrite_complete: bool,
    pub local_cleanup_complete: bool,
    pub cleanup: Option<vcp_store::rewrite::Cleanup>,
    pub pending_generations: Vec<GenerationId>,
    pub index_intents: Vec<IndexIntentId>,
    pub backup_copies: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub schema_version: u32,
    pub recall_excluded: bool,
    pub compacted: bool,
    pub purged: bool,
    pub document_type: String,
    pub workspace: WorkspaceId,
    pub revision: Revision,
    pub target: Target,
    pub action: Action,
    pub deletion: DeletionEpoch,
}

/// Preview persistence itself is not a new selected history fact. Every other
/// record/event/command-copy mutation remains part of the source commitment.
fn source_digest(state: &State) -> Result<String> {
    let records: Vec<_> = state
        .records
        .iter()
        .filter(|(_, r)| r.value["document_type"] != PREVIEW)
        .collect();
    let events: Vec<_> = state
        .events
        .iter()
        .filter(|e| e.event.data["document_type"] != PREVIEW)
        .collect();
    digest(&(records, events, &state.commands))
}
pub async fn save_preview(
    store: &mut Store,
    access: &Access,
    value: &PrunePreview,
    now: Timestamp,
) -> Result<()> {
    access::authorize(store.state(), access, true)?;
    if value.workspace != access.workspace || value.actor != access.actor || access.tasks.is_some()
    {
        return Err(Error::Access);
    }
    let id = format!("preview-{}", value.id);
    if let Some(row) = store.state().records.get(&key(Collection::Projection, &id)) {
        if row.value["preview"] != serde_json::to_value(value)? {
            return Err(Error::Conflict("saved preview identity"));
        }
        return Ok(());
    }
    if self::preview(
        store,
        access,
        value.selector.clone(),
        value.action,
        value.created_at,
    )? != *value
    {
        return Err(Error::Conflict("stale preview before save"));
    }
    let scope = scope_for(store.state(), &access.workspace)?;
    let record = Record::typed(
        Collection::Projection,
        id,
        access.workspace.clone(),
        Revision::ZERO,
        &serde_json::json!({"schema_version":1,"document_type":PREVIEW,"preview":value}),
    )?;
    let event = event(
        &scope,
        access,
        now,
        serde_json::json!({"document_type":PREVIEW,"id":value.id}),
    );
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put {
                record,
                expected: None,
            }],
            events: vec![event],
            command: None,
        })
        .await?;
    Ok(())
}
pub fn load_preview(store: &Store, access: &Access, id: &str) -> Result<PrunePreview> {
    access::authorize(store.state(), access, false)?;
    if access.tasks.is_some() || !vcp_domain::accounting::valid_hash(id) {
        return Err(Error::Access);
    }
    let row = store.state().record(
        Collection::Projection,
        &format!("preview-{id}"),
        &access.workspace,
    )?;
    if row.value["document_type"] != PREVIEW {
        return Err(Error::Invalid("preview type".into()));
    }
    let preview: PrunePreview = serde_json::from_value(row.value["preview"].clone())?;
    if preview.actor != access.actor || preview.workspace != access.workspace {
        return Err(Error::Access);
    }
    Ok(preview)
}
fn digest<T: Serialize>(v: &T) -> Result<String> {
    Ok(digest_bytes(&canonical_bytes(v)?))
}
fn decision_id(target: &Target) -> Result<String> {
    Ok(format!("recall-{}", digest(target)?))
}
fn targets(preview: &PrunePreview) -> BTreeSet<Target> {
    preview
        .selected
        .union(&preview.dependent)
        .cloned()
        .collect()
}
fn empty_facts(workspace: &WorkspaceId) -> Facts<'_> {
    Facts {
        workspace,
        timestamp: None,
        roots: None,
        paths: None,
        task: None,
        actor: None,
        agent: None,
        model: None,
        provider: None,
        event: None,
        claim: None,
        task_status: None,
        claim_status: None,
        superseded: None,
    }
}
fn scope(state: &State, target: &Target) -> Result<Option<Scope>> {
    match target {
        Target::Event(id) => Ok(state
            .events
            .iter()
            .find(|e| &e.event.id == id)
            .and_then(|e| {
                e.event.task.as_ref().map(|task| Scope {
                    workspace: e.event.workspace.clone(),
                    session: e.event.session.clone(),
                    task: task.clone(),
                })
            })),
        Target::Record(k) => {
            let row = state
                .records
                .get(k)
                .ok_or(Error::Conflict("retention target missing"))?;
            Ok(match row.collection {
                Collection::Task => Some(row.decode::<Task>()?.scope),
                Collection::Artifact => Some(row.decode::<ArtifactDescriptor>()?.spec.scope),
                _ => row
                    .value
                    .get("scope")
                    .map(|s| serde_json::from_value(s.clone()))
                    .transpose()?,
            })
        }
    }
}
fn valid_record(row: &Record) -> bool {
    if row.collection == Collection::Projection
        && row.value["document_type"] == vcp_domain::forecast::SOURCES
    {
        return true;
    }
    matches!(
        row.collection,
        Collection::Task
            | Collection::Artifact
            | Collection::Turn
            | Collection::Effect
            | Collection::Verification
            | Collection::Attempt
            | Collection::Settlement
    ) || matches!(
        row.value["document_type"].as_str(),
        Some(
            "vcp_memory_proposal_v1"
                | "vcp_memory_version_v1"
                | "vcp_memory_result_v1"
                | vcp_domain::memory_review::SUBMISSION
                | vcp_domain::memory_review::DECISION
        )
    ) || row.collection == Collection::Projection
        && row.value["document_type"]
            .as_str()
            .is_some_and(vcp_domain::redaction::advisory_document)
}
fn already_redacted(row: &Record) -> bool {
    if row.value["document_type"] == vcp_domain::forecast::REDACTED {
        return true;
    }
    if row.collection == Collection::Attempt {
        return row
            .decode::<vcp_domain::accounting::Attempt>()
            .is_ok_and(|attempt| {
                attempt.redaction.is_some()
                    && attempt.redacted_at_revision == Some(attempt.revision)
            });
    }
    row.value.get("redaction").is_some_and(|v| !v.is_null())
        || row.value["state"] == "purged"
        || row.value["document_type"]
            .as_str()
            .is_some_and(|s| s.starts_with("vcp_memory_redacted_"))
}
fn record_proposal(row: &Record) -> Result<Option<vcp_domain::memory::Proposal>> {
    Ok(match row.value["document_type"].as_str() {
        Some("vcp_memory_proposal_v1") => Some(row.decode::<ProposalRecord>()?.proposal),
        Some("vcp_memory_version_v1") => Some(row.decode::<Version>()?.proposal),
        Some(vcp_domain::memory_review::SUBMISSION) => Some(
            row.decode::<vcp_domain::memory_review::Submission>()?
                .candidate,
        ),
        _ => None,
    })
}

fn context_dependencies(
    store: &Store,
    workspace: &WorkspaceId,
) -> Result<BTreeMap<Target, BTreeSet<Target>>> {
    let mut dependencies = BTreeMap::<Target, BTreeSet<Target>>::new();
    let artifacts: Vec<ArtifactDescriptor> = store
        .state()
        .records
        .values()
        .filter(|r| r.workspace == *workspace && r.collection == Collection::Artifact)
        .map(|r| r.decode())
        .collect::<vcp_store::Result<_>>()?;
    let mut bytes_read = 0u64;
    for artifact in &artifacts {
        if artifact.state == CaptureState::Purged
            || !matches!(
                artifact.spec.schema.as_str(),
                "memory-context/1" | "context-manifest/1"
            )
        {
            continue;
        }
        bytes_read = bytes_read
            .checked_add(artifact.length.get())
            .ok_or(Error::Conflict("context lineage bytes overflow"))?;
        if artifact.length.get() > 1024 * 1024 || bytes_read > 8 * 1024 * 1024 {
            return Err(Error::Conflict(
                "context lineage exceeds qualified scan limit",
            ));
        }
        if artifact.state != CaptureState::Complete {
            return Err(Error::Conflict("unfinished context lineage capture"));
        }
        let mut bytes = Vec::new();
        store.spool().read(artifact, &mut bytes)?;
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| Error::Conflict("context lineage JSON"))?;
        let target = Target::Record(key(Collection::Artifact, artifact.spec.id.as_str()));
        let mut refs = BTreeSet::new();
        if artifact.spec.schema == "memory-context/1" {
            let passages = value
                .as_array()
                .ok_or(Error::Conflict("context passage lineage shape"))?;
            if passages.len() > 64 {
                return Err(Error::Conflict("context passage lineage bound"));
            }
            for passage in passages {
                let source: crate::search_record::TextSource =
                    serde_json::from_value(passage["source"].clone())
                        .map_err(|_| Error::Conflict("context source reference"))?;
                refs.insert(match source {
                    crate::search_record::TextSource::Artifact { id } => {
                        Target::Record(key(Collection::Artifact, id.as_str()))
                    }
                    crate::search_record::TextSource::Claim { version, .. } => {
                        Target::Record(key(Collection::Claim, version.as_str()))
                    }
                });
                let evidence: Vec<ArtifactId> = serde_json::from_value(passage["evidence"].clone())
                    .map_err(|_| Error::Conflict("context evidence reference"))?;
                refs.extend(
                    evidence
                        .iter()
                        .map(|id| Target::Record(key(Collection::Artifact, id.as_str()))),
                );
            }
        } else {
            let parts = value["included"]
                .as_array()
                .ok_or(Error::Conflict("context manifest lineage shape"))?;
            if parts.len() > 256 {
                return Err(Error::Conflict("context manifest lineage bound"));
            }
            for part in parts {
                let id: ArtifactId = serde_json::from_value(part["artifact"].clone())
                    .map_err(|_| Error::Conflict("context part reference"))?;
                refs.insert(Target::Record(key(Collection::Artifact, id.as_str())));
            }
            let digest = value["request_sha256"]
                .as_str()
                .ok_or(Error::Conflict("context request commitment"))?;
            if !vcp_domain::accounting::valid_hash(digest) {
                return Err(Error::Conflict("context request digest"));
            }
            for request in artifacts.iter().filter(|a| {
                a.spec.scope == artifact.spec.scope
                    && a.sha256 == digest
                    && a.spec.schema == "responses-request/1"
            }) {
                dependencies
                    .entry(Target::Record(key(
                        Collection::Artifact,
                        request.spec.id.as_str(),
                    )))
                    .or_default()
                    .insert(target.clone());
            }
        }
        dependencies.entry(target).or_default().extend(refs);
    }
    for row in store
        .state()
        .records
        .values()
        .filter(|r| r.workspace == *workspace && r.collection == Collection::Settlement)
    {
        let settlement: vcp_domain::accounting::Settlement = row.decode()?;
        let attempt: vcp_domain::accounting::Attempt = store
            .state()
            .record(Collection::Attempt, settlement.attempt.as_str(), workspace)?
            .decode()?;
        dependencies
            .entry(Target::Record(key(
                Collection::Artifact,
                settlement.observation.raw.as_str(),
            )))
            .or_default()
            .insert(Target::Record(key(
                Collection::Artifact,
                attempt.request.as_str(),
            )));
    }
    Ok(dependencies)
}
/// Materialize exact IDs against one canonical cut. Unknown metadata never
/// matches negation. Protection categories deliberately contain no source text.
pub fn preview(
    store: &Store,
    access: &Access,
    selector: Selector,
    action: Action,
    now: Timestamp,
) -> Result<PrunePreview> {
    let workspace = access::authorize(store.state(), access, false)?;
    if access.tasks.is_some() {
        return Err(Error::Access);
    }
    let selector = selector.normalized()?;
    let state = store.state();
    let source_metadata = sources::metadata(store, access, &selector.tree)?;
    let mut selected = BTreeSet::new();
    let superseded: BTreeSet<_> = state
        .records
        .values()
        .filter(|r| {
            r.workspace == access.workspace && r.value["document_type"] == "vcp_memory_version_v1"
        })
        .filter_map(|r| {
            r.value["proposal"]["predecessor"]
                .as_str()
                .map(str::to_owned)
        })
        .collect();
    for event in state.events.iter().filter(|e| {
        e.event.workspace == access.workspace
            && e.redaction.is_none()
            && e.event.data["document_type"] != PREVIEW
    }) {
        let name = serde_json::to_value(&event.event.kind)?;
        let mut facts = empty_facts(&access.workspace);
        facts.timestamp = Some(event.event.timestamp);
        facts.task = event.event.task.as_ref();
        facts.actor = Some(&event.event.actor);
        facts.event = name.as_str();
        let task = event
            .event
            .task
            .as_ref()
            .map(|id| {
                state
                    .record(Collection::Task, id.as_str(), &access.workspace)?
                    .decode::<Task>()
            })
            .transpose()?;
        facts.task_status = task.as_ref().map(|t| t.state);
        if let Some(meta) = &event.event.metadata {
            facts.agent = meta.agent.as_ref();
            facts.model = meta.model.as_deref();
            facts.provider = meta.provider.as_deref();
            facts.paths = Some(&meta.paths);
        }
        if selector.evaluate(&facts)? == Truth::Match {
            selected.insert(Target::Event(event.event.id.clone()));
        }
    }
    for row in state
        .records
        .values()
        .filter(|r| r.workspace == access.workspace && valid_record(r) && !already_redacted(r))
    {
        let target = Target::Record(row.key());
        let own = scope(state, &target)?;
        let mut facts = empty_facts(&access.workspace);
        facts.task = own.as_ref().map(|s| &s.task);
        let task = own
            .as_ref()
            .map(|s| {
                state
                    .record(Collection::Task, s.task.as_str(), &access.workspace)?
                    .decode::<Task>()
            })
            .transpose()?;
        facts.task_status = task.as_ref().map(|t| t.state);
        facts.timestamp = state
            .events
            .iter()
            .filter(|event| event.event.workspace == access.workspace)
            .find(|event| match row.collection {
                Collection::Artifact => {
                    event.event.artifacts.iter().any(|id| id.as_str() == row.id)
                }
                Collection::Task => event
                    .event
                    .task
                    .as_ref()
                    .is_some_and(|id| id.as_str() == row.id),
                _ => event
                    .event
                    .data
                    .get("facts")
                    .or_else(|| event.event.data.get("records"))
                    .and_then(|value| value.as_array())
                    .is_some_and(|facts| {
                        facts.iter().any(|fact| {
                            serde_json::from_value::<Record>(fact.clone()).is_ok_and(|record| {
                                record.key() == row.key() && record.workspace == row.workspace
                            })
                        })
                    }),
            })
            .map(|event| event.event.timestamp);
        if row.collection == Collection::Artifact {
            if let Some(metadata) = source_metadata.get(row.id.as_str()) {
                facts.roots = Some(&metadata.roots);
                facts.paths = Some(&metadata.paths);
            }
        }
        let proposal = record_proposal(row)?;
        if let Some(p) = &proposal {
            facts.actor = Some(&p.actor);
            facts.claim = Some(p.value.kind());
            facts.paths = Some(&p.applicability.paths);
            facts.roots = (!p.applicability.roots.is_empty()).then_some(&p.applicability.roots);
            if row.value["document_type"] == "vcp_memory_version_v1" {
                let v: Version = row.decode()?;
                facts.timestamp = Some(v.recorded_at);
                facts.claim_status = Some(v.resolution.outcome);
                facts.superseded = Some(superseded.contains(v.id.as_str()));
            } else if row.value["document_type"] == vcp_domain::memory_review::SUBMISSION {
                let submission: vcp_domain::memory_review::Submission = row.decode()?;
                facts.timestamp = Some(submission.recorded_at);
                facts.claim_status = Some(vcp_domain::memory::Outcome::AwaitingReview);
            } else {
                let p: ProposalRecord = row.decode()?;
                facts.timestamp = Some(p.recorded_at);
                facts.claim_status = Some(p.resolution.outcome);
            }
        }
        let review_decision = if row.value["document_type"] == vcp_domain::memory_review::DECISION {
            Some(row.decode::<vcp_domain::memory_review::Decision>()?)
        } else {
            None
        };
        if let Some(decision) = &review_decision {
            facts.actor = Some(&decision.actor);
            facts.timestamp = Some(decision.recorded_at);
            facts.claim_status = Some(decision.resolution.outcome);
        }
        if selector.evaluate(&facts)? == Truth::Match {
            selected.insert(target);
        }
    }
    if selected.len() > 8192 {
        return Err(Error::Invalid(
            "retention selection exceeds 8192 objects".into(),
        ));
    }
    let contexts = if selected.is_empty() {
        BTreeMap::new()
    } else {
        context_dependencies(store, &access.workspace)?
    };
    let mut closure = selected.clone();
    // Lineage closure uses canonical references and captured context manifests;
    // request hashes bind copied provider bodies without scanning their text.
    let mut passes = 0usize;
    loop {
        passes += 1;
        if passes > 64 {
            return Err(Error::Conflict("retention dependency closure depth limit"));
        }
        let before = closure.len();
        for (dependent, sources) in &contexts {
            if sources.iter().any(|source| closure.contains(source)) {
                closure.insert(dependent.clone());
            }
        }
        for event in state.events.iter().filter(|e| {
            e.event.workspace == access.workspace
                && e.redaction.is_none()
                && e.event.data["document_type"] != PREVIEW
        }) {
            let event_target = Target::Event(event.event.id.clone());
            let task_selected = event.event.task.as_ref().is_some_and(|t| {
                closure.contains(&Target::Record(key(Collection::Task, t.as_str())))
            });
            let artifact_selected =
                event.event.artifacts.iter().any(|a| {
                    closure.contains(&Target::Record(key(Collection::Artifact, a.as_str())))
                });
            let fact_selected = event
                .event
                .data
                .get("facts").or_else(||event.event.data.get("records"))
                .and_then(|v| v.as_array())
                .is_some_and(|facts| {
                    facts.iter().any(|v| {
                        v.get("id").and_then(|v| v.as_str()).is_some_and(|id| {
                            closure.iter().any(|target|matches!(target,Target::Record(k) if k.rsplit_once(':').is_some_and(|(_,value)|value==id)))
                        })
                    })
                });
            if task_selected || artifact_selected || fact_selected {
                closure.insert(event_target.clone());
            }
            if closure.contains(&event_target) {
                if let Some(facts) = event
                    .event
                    .data
                    .get("facts")
                    .or_else(|| event.event.data.get("records"))
                    .and_then(|v| v.as_array())
                {
                    for fact in facts {
                        if let Some(id) = fact.get("id").and_then(|v| v.as_str()) {
                            for row in state.records.values().filter(|r| {
                                r.workspace == access.workspace
                                    && r.id == id
                                    && valid_record(r)
                                    && !already_redacted(r)
                            }) {
                                closure.insert(Target::Record(row.key()));
                            }
                        }
                    }
                }
                for id in &event.event.artifacts {
                    closure.insert(Target::Record(key(Collection::Artifact, id.as_str())));
                }
            }
        }
        for row in state
            .records
            .values()
            .filter(|r| r.workspace == access.workspace && valid_record(r) && !already_redacted(r))
        {
            let target = Target::Record(row.key());
            let own = scope(state, &target)?;
            let own_selected = own.as_ref().is_some_and(|s| {
                closure.contains(&Target::Record(key(Collection::Task, s.task.as_str())))
            });
            let mut depends = own_selected;
            if row.collection == Collection::Projection
                && row.value["document_type"] == vcp_domain::forecast::SOURCES
            {
                let sources: vcp_domain::forecast::Sources = row.decode()?;
                depends |= sources
                    .source_events
                    .iter()
                    .any(|id| closure.contains(&Target::Event(id.clone())));
            }
            if row.collection == Collection::Task {
                let task: Task = row.decode()?;
                depends |= task
                    .objectives
                    .iter()
                    .any(|o| closure.contains(&Target::Event(o.source.clone())));
            }

            depends |= row
                .required_references()?
                .iter()
                .any(|reference| closure.contains(&Target::Record(reference.clone())));
            if let Some(p) = record_proposal(row)? {
                if closure.contains(&target) {
                    closure.insert(Target::Record(key(Collection::Claim, p.id.as_str())));
                }
                depends |= p.predecessor.as_ref().is_some_and(|id| {
                    closure.contains(&Target::Record(key(Collection::Claim, id.as_str())))
                }) || p
                    .origins
                    .iter()
                    .any(|e| closure.contains(&Target::Event(e.clone())))
                    || p.evidence.iter().any(|e| {
                        closure.contains(&Target::Record(key(
                            Collection::Artifact,
                            e.artifact.as_str(),
                        )))
                    })
                    || closure.contains(&Target::Record(key(Collection::Claim, p.id.as_str())));
            }
            if row.value["document_type"] == "vcp_memory_result_v1" {
                if let Some(id) = row.value["proposal"].as_str() {
                    depends |= closure.contains(&Target::Record(key(Collection::Claim, id)));
                }
            }
            if depends {
                closure.insert(target);
            }
        }
        if closure.len() > 8192 {
            return Err(Error::Invalid(
                "retention selection exceeds 8192 objects".into(),
            ));
        }
        if before == closure.len() {
            break;
        }
    }
    let mut protected = Vec::new();
    if action == Action::Purge {
        for target in &closure {
            let own = scope(state, target)?;
            if store
                .retention_protection(&access.workspace, own.as_ref().map(|s| &s.task))
                .is_err()
            {
                protected.push(Protected {
                    target: target.clone(),
                    reason: "active recovery or unsettled liability".into(),
                });
            }
            if let Target::Record(k) = target {
                if let Some(row) = state.records.get(k) {
                    if row.collection == Collection::Artifact {
                        let a: ArtifactDescriptor = row.decode()?;
                        if !matches!(a.state, CaptureState::Complete | CaptureState::Aborted) {
                            protected.push(Protected {
                                target: target.clone(),
                                reason: "capture has not terminated".into(),
                            });
                        }
                    }
                }
            }
        }
        // Apply is atomic across the entire explicit preview batch. A protected
        // dependency blocks that batch; unrelated rows can use a narrower preview.
        if !protected.is_empty() {
            for target in &closure {
                if !protected.iter().any(|p| &p.target == target) {
                    protected.push(Protected {
                        target: target.clone(),
                        reason: "atomic preview batch blocked; narrow selector or reconcile protected dependencies".into(),
                    });
                }
            }
        }
    }
    let retained_bytes = closure.iter().try_fold(0u64, |sum, t| -> Result<u64> {
        Ok(sum.saturating_add(match t {
            Target::Record(k) => {
                let row = &state.records[k];
                if row.collection == Collection::Artifact {
                    row.decode::<ArtifactDescriptor>()?.length.get()
                } else {
                    canonical_bytes(row)?.len() as u64
                }
            }
            Target::Event(id) => state
                .events
                .iter()
                .find(|e| &e.event.id == id)
                .map(canonical_bytes)
                .transpose()?
                .map_or(0, |b| b.len() as u64),
        }))
    })?;
    let backup_copies = state
        .records
        .values()
        .filter(|r| r.workspace == access.workspace && r.collection == Collection::SnapshotPin)
        .map(|r| r.id.clone())
        .collect();
    let mut result = PrunePreview {
        version: 1,
        id: String::new(),
        workspace: access.workspace.clone(),
        actor: access.actor.clone(),
        authority: workspace.authority,
        watermark: state.watermark,
        deletion: workspace.deletion,
        selector,
        action,
        selected: selected.clone(),
        dependent: closure.difference(&selected).cloned().collect(),
        protected,
        source_digest: source_digest(state)?,
        retained_bytes,
        bytes_are_exact: false,
        created_at: now,
        backup_copies,
    };
    result.id = digest(&result)?;
    Ok(result)
}
fn event(scope: &Scope, access: &Access, now: Timestamp, data: serde_json::Value) -> EventInput {
    EventInput {
        id: EventId::new(),
        workspace: access.workspace.clone(),
        session: scope.session.clone(),
        task: Some(scope.task.clone()),
        actor: access.actor.clone(),
        correlation: CommandId::new(),
        causation: None,
        timestamp: now,
        kind: EventKind::RetentionChanged,
        artifacts: vec![],
        data,
        metadata: None,
    }
}
fn scope_for(state: &State, workspace: &WorkspaceId) -> Result<Scope> {
    state
        .records
        .values()
        .find(|r| r.workspace == *workspace && r.collection == Collection::Task)
        .ok_or(Error::Conflict("retention requires retained task scope"))?
        .decode::<Task>()
        .map(|t| t.scope)
        .map_err(Into::into)
}
/// Stable retry returns the original job. A stale preview cannot broaden IDs.
pub async fn apply(
    store: &mut Store,
    access: &Access,
    preview: &PrunePreview,
    now: Timestamp,
) -> Result<PruneReceipt> {
    let mut workspace = access::authorize(store.state(), access, true)?;
    if access.tasks.is_some()
        || preview.actor != access.actor
        || preview.workspace != access.workspace
    {
        return Err(Error::Access);
    }
    if let Some(row) = store.state().records.get(&key(
        Collection::Projection,
        &format!("prune-{}", preview.id),
    )) {
        let existing: PruneReceipt = row.decode()?;
        if existing.preview != *preview {
            return Err(Error::Conflict("preview identity mismatch"));
        }
        return Ok(existing);
    }
    if workspace.authority != preview.authority
        || workspace.deletion != preview.deletion
        || store.state().watermark < preview.watermark
        || source_digest(store.state())? != preview.source_digest
    {
        return Err(Error::Conflict("stale preview; create a new preview"));
    }
    let mut actual = self::preview(
        store,
        access,
        preview.selector.clone(),
        preview.action,
        preview.created_at,
    )?;
    actual.watermark = preview.watermark;
    actual.id.clear();
    actual.id = digest(&actual)?;
    if &actual != preview {
        return Err(Error::Conflict("preview selection or identity changed"));
    }
    if preview.selected.is_empty() {
        return Err(Error::Conflict("preview selects no retained objects"));
    }
    if !preview.protected.is_empty() {
        return Err(Error::Conflict(
            "reconcile protected dependency closure before purge",
        ));
    }
    let state = store.state();
    let scope = scope_for(state, &access.workspace)?;
    let old_revision = workspace.revision;
    workspace.revision = workspace.revision.next()?;
    workspace.deletion = workspace.deletion.next()?;
    let mut mutations = vec![Mutation::Put {
        record: Record::typed(
            Collection::Workspace,
            workspace.id.to_string(),
            workspace.id.clone(),
            workspace.revision,
            &workspace,
        )?,
        expected: Some(old_revision),
    }];
    for target in targets(preview) {
        let id = decision_id(&target)?;
        let previous = state.records.get(&key(Collection::Projection, &id));
        if previous.is_some_and(|r| r.value["purged"] == true) && preview.action != Action::Purge {
            return Err(Error::Conflict("purge cannot be reversed"));
        }
        let revision = previous.map_or(Ok(Revision::ZERO), |r| r.revision.next())?;
        let prior = previous.map(|r| r.decode::<Decision>()).transpose()?;
        let value = Decision {
            schema_version: 1,
            recall_excluded: match preview.action {
                Action::Exclude | Action::Purge => true,
                Action::RestoreRecall => false,
                Action::Compact => prior.as_ref().is_some_and(|d| d.recall_excluded),
            },
            compacted: preview.action == Action::Compact
                || prior.as_ref().is_some_and(|d| d.compacted),
            purged: preview.action == Action::Purge || prior.as_ref().is_some_and(|d| d.purged),
            document_type: DECISION.into(),
            workspace: access.workspace.clone(),
            revision,
            target: target.clone(),
            action: preview.action,
            deletion: workspace.deletion,
        };
        mutations.push(Mutation::Put {
            record: Record::typed(
                Collection::Projection,
                id,
                access.workspace.clone(),
                revision,
                &value,
            )?,
            expected: previous.map(|r| r.revision),
        });
        if preview.action == Action::Purge {
            if let Target::Record(k) = &target {
                let row = &state.records[k];
                if row.collection == Collection::Artifact {
                    let artifact: ArtifactDescriptor = row.decode()?;
                    let mask = vcp_audit::history::RetentionMask {
                        schema_version: 1,
                        workspace: access.workspace.clone(),
                        session: artifact.spec.scope.session,
                        first: SessionSeq::ZERO,
                        last: SessionSeq::ZERO,
                        artifacts: vec![artifact.spec.id],
                        deletion: workspace.deletion,
                        reason: "explicit artifact purge".into(),
                    };
                    mutations.push(Mutation::Put {
                        record: Record::typed(
                            Collection::Tombstone,
                            format!("artifact-{}", digest(&(preview.id.clone(), k))?),
                            access.workspace.clone(),
                            Revision::ZERO,
                            &mask,
                        )?,
                        expected: None,
                    });
                }
            }
            if let Target::Event(id) = &target {
                let envelope = state
                    .events
                    .iter()
                    .find(|e| &e.event.id == id)
                    .ok_or(Error::Conflict("preview event missing"))?;
                let mask = vcp_audit::history::RetentionMask {
                    schema_version: 1,
                    workspace: access.workspace.clone(),
                    session: envelope.event.session.clone(),
                    first: envelope.sequence,
                    last: envelope.sequence,
                    artifacts: envelope.event.artifacts.clone(),
                    deletion: workspace.deletion,
                    reason: "explicit retention purge".into(),
                };
                mutations.push(Mutation::Put {
                    record: Record::typed(
                        Collection::Tombstone,
                        format!("purge-{}", digest(&(preview.id.clone(), id))?),
                        access.workspace.clone(),
                        Revision::ZERO,
                        &mask,
                    )?,
                    expected: None,
                });
            }
        }
    }
    let transaction = TransactionId::new();
    let mut index_intents = Vec::new();
    let versions: Vec<_> = targets(preview)
        .iter()
        .filter_map(|target| match target {
            Target::Record(k) => state.records.get(k),
            _ => None,
        })
        .filter(|row| row.value["document_type"] == "vcp_memory_version_v1")
        .map(|row| ClaimVersionId::parse(row.id.clone()))
        .collect::<std::result::Result<_, _>>()?;
    let sequence = state
        .records
        .values()
        .filter(|r| {
            r.workspace == access.workspace
                && matches!(
                    r.value["document_type"].as_str(),
                    Some("vcp_memory_result_v1" | vcp_domain::redaction::RESULT)
                )
        })
        .map(|r| serde_json::from_value::<MemorySeq>(r.value["memory_seq"].clone()))
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .max()
        .unwrap_or(MemorySeq::ZERO);
    let batches: Vec<&[ClaimVersionId]> = if versions.is_empty() {
        vec![&[]]
    } else {
        versions.chunks(64).collect()
    };
    for batch in batches {
        let intent = vcp_domain::memory::IndexIntent {
            deletion: Some(workspace.deletion),
            document_type: vcp_domain::memory::DocumentType::IndexIntent,
            schema_version: 1,
            id: IndexIntentId::new(),
            scope: scope.clone(),
            revision: Revision::ZERO,
            transaction: transaction.clone(),
            versions: batch.to_vec(),
            supersedes: vec![],
            memory_seq: sequence,
            canonical_watermark: state.watermark.next()?,
            status: vcp_domain::memory::IndexStatus::Pending,
        };
        index_intents.push(intent.id.clone());
        mutations.push(Mutation::Put {
            record: Record::typed(
                Collection::IndexIntent,
                intent.id.as_str(),
                access.workspace.clone(),
                Revision::ZERO,
                &intent,
            )?,
            expected: None,
        });
    }
    let receipt = PruneReceipt {
        schema_version: 1,
        document_type: JOB.into(),
        id: format!("prune-{}", preview.id),
        workspace: access.workspace.clone(),
        revision: Revision::ZERO,
        preview: preview.clone(),
        deletion: workspace.deletion,
        applied_at: now,
        logical_unavailable: preview.action == Action::Purge,
        rewrite_complete: preview.action != Action::Purge,
        local_cleanup_complete: preview.action != Action::Purge,
        cleanup: None,
        index_intents,
        pending_generations: state
            .records
            .values()
            .filter(|r| {
                r.workspace == access.workspace
                    && r.collection == Collection::Generation
                    && r.value["document_type"] == vcp_domain::search::GENERATION
            })
            .map(|r| GenerationId::parse(r.id.clone()))
            .collect::<std::result::Result<_, _>>()?,
        backup_copies: preview.backup_copies.clone(),
    };
    mutations.push(Mutation::Put {
        record: Record::typed(
            Collection::Projection,
            &receipt.id,
            access.workspace.clone(),
            Revision::ZERO,
            &receipt,
        )?,
        expected: None,
    });
    let facts: Vec<_> = mutations
        .iter()
        .filter_map(|m| {
            if let Mutation::Put { record, .. } = m {
                Some(record)
            } else {
                None
            }
        })
        .collect();
    let event = event(
        &scope,
        access,
        now,
        serde_json::json!({"version":1,"preview":preview.id,"action":preview.action,"facts":facts}),
    );
    store
        .transact(Transaction {
            id: transaction,
            expected_watermark: state.watermark,
            mutations,
            events: vec![event],
            command: None,
        })
        .await?;
    Ok(receipt)
}
/// Current recall eligibility is independent of raw history/presentation. A
/// later RestoreRecall can reverse exclusion but never a purge decision.
pub fn decision(
    state: &State,
    workspace: &WorkspaceId,
    target: &Target,
) -> Result<Option<Decision>> {
    let Some(row) = state
        .records
        .get(&key(Collection::Projection, &decision_id(target)?))
    else {
        return Ok(None);
    };
    if row.workspace != *workspace {
        return Err(Error::Access);
    }
    let value: Decision = row.decode()?;
    if value.schema_version != 1
        || value.document_type != DECISION
        || value.target != *target
        || value.workspace != *workspace
    {
        return Err(Error::Invalid("retention decision identity".into()));
    }
    Ok(Some(value))
}
pub fn purged(state: &State, workspace: &WorkspaceId, target: &Target) -> Result<bool> {
    Ok(decision(state, workspace, target)?.is_some_and(|value| value.purged))
}
pub fn recall_allowed(state: &State, workspace: &WorkspaceId, target: &Target) -> Result<bool> {
    Ok(decision(state, workspace, target)?
        .is_none_or(|value| !value.recall_excluded && !value.purged))
}
/// Restartable exact cleanup. The returned receipt never equates logical masks
/// with erasure and keeps preexisting snapshot/cloud identities explicit.
pub async fn cleanup(
    store: &mut Store,
    access: &Access,
    id: &str,
    now: Timestamp,
) -> Result<PruneReceipt> {
    access::authorize(store.state(), access, true)?;
    if access.tasks.is_some() {
        return Err(Error::Access);
    }
    let mut job: PruneReceipt = store
        .state()
        .record(Collection::Projection, id, &access.workspace)?
        .decode()?;
    if job.document_type != JOB {
        return Err(Error::Invalid("retention job type".into()));
    }
    if job.preview.action != Action::Purge {
        return Ok(job);
    }
    if !job.rewrite_complete {
        let mut records = BTreeSet::new();
        let mut events = BTreeSet::new();
        let mut tasks = BTreeSet::new();
        for target in targets(&job.preview) {
            match target {
                Target::Event(id) => {
                    if store
                        .state()
                        .events
                        .iter()
                        .any(|e| e.event.id == id && e.redaction.is_none())
                    {
                        events.insert(id);
                    }
                }
                Target::Record(k) => {
                    if let Some(row) = store.state().records.get(&k) {
                        if !already_redacted(row) {
                            if row.collection == Collection::Task {
                                tasks.insert(TaskId::parse(row.id.clone())?);
                            }
                            records.insert(k);
                        }
                    }
                }
            }
        }
        if !records.is_empty() || !events.is_empty() {
            let candidate = store.retention_candidate(&records, &events, &tasks)?;
            store.rewrite_base(candidate, &[]).await?;
        }
        job.rewrite_complete = true;
    }
    job.cleanup = Some(store.cleanup_rewrites()?);
    let directory = store.canonical_anchor().join("search-generations");
    let roots_pinned = job
        .cleanup
        .as_ref()
        .is_some_and(|r| !r.pinned.is_empty() || !r.failed.is_empty());
    if !roots_pinned {
        let publisher = match std::fs::symlink_metadata(&directory) {
            Ok(_) => Some(crate::publication::Publisher::new(&directory)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(vcp_store::Error::Io(error).into()),
        };
        let policy = crate::publication::GarbagePolicy {
            retention_idle: true,
            historical_deletion_allowed: true,
            retained: BTreeSet::new(),
        };
        let mut pending = Vec::new();
        for generation in &job.pending_generations {
            let location = store.state().records.get(&key(
                Collection::Projection,
                &format!("generation-location-{generation}"),
            ));
            let owned = location.is_some_and(|row| {
                row.workspace == access.workspace
                    && row.revision == Revision::ZERO
                    && row.value["schema_version"] == 1
                    && row.value["document_type"] == "vcp_local_generation_location_v1"
                    && row.value["workspace"] == serde_json::json!(access.workspace)
                    && row.value["generation"] == serde_json::json!(generation)
                    && row.value["revision"] == serde_json::json!(Revision::ZERO)
                    && row.value["owned_relative_root"] == "search-generations"
                    && row
                        .references
                        .contains(&key(Collection::Generation, generation.as_str()))
            });
            // Absence is evidence only at the root attested by the canonical
            // publication receipt. Legacy/external locations remain explicit.
            if !owned {
                pending.push(generation.clone());
            } else if let Some(publisher) = &publisher {
                if !publisher.collect(store, access, generation, &policy)? {
                    pending.push(generation.clone());
                }
            }
        }
        job.pending_generations = pending;
    }
    let result = job
        .cleanup
        .as_ref()
        .ok_or(Error::Conflict("missing cleanup report"))?;
    job.local_cleanup_complete =
        result.pinned.is_empty() && result.failed.is_empty() && job.pending_generations.is_empty();
    let expected = job.revision;
    job.revision = job.revision.next()?;
    let scope = scope_for(store.state(), &access.workspace)?;
    let record = Record::typed(
        Collection::Projection,
        &job.id,
        access.workspace.clone(),
        job.revision,
        &job,
    )?;
    let event = event(
        &scope,
        access,
        now,
        serde_json::json!({"version":1,"retention_cleanup":job.id,"facts":[record]}),
    );
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put {
                record,
                expected: Some(expected),
            }],
            events: vec![event],
            command: None,
        })
        .await?;
    Ok(job)
}
