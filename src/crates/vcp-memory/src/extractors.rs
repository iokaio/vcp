// SPDX-License-Identifier: Apache-2.0
//! Deterministic extraction from authorized canonical observations, never prose.
use crate::{
    access::{self, Access},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{
    artifact::ArtifactDescriptor,
    ids::*,
    memory::*,
    revision::*,
    task::{Task, TaskState},
    verification::{CheckOutcome, Verification},
    workspace::{Scope, Workspace},
};
use vcp_protocol::{
    canonical_bytes, digest_bytes,
    event::{EventEnvelope, EventKind},
};
use vcp_store::{
    contract::{Collection, Record},
    Store,
};

pub const SPEC: &str = "deterministic/1";
pub const MAX_PROPOSALS: usize = 16;
pub const MAX_ARTIFACTS: usize = 64;
pub const MAX_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractionFinding {
    pub code: String,
    pub message: String,
    pub artifact: Option<ArtifactId>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extraction {
    pub proposals: Vec<Proposal>,
    pub findings: Vec<ExtractionFinding>,
}
impl Extraction {
    fn finding(&mut self, code: &str, message: &str, artifact: Option<ArtifactId>) {
        // At most one finding per inspected input plus the final event summary.
        if self.findings.len() < MAX_ARTIFACTS + MAX_PROPOSALS + 1 {
            self.findings.push(ExtractionFinding {
                code: code.into(),
                message: message.into(),
                artifact,
            });
        }
    }
}

/// An explicit typed observation. Content cannot choose caller, workspace,
/// authority, extractor identity, proposal identity or an execution grant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub output_key: String,
    pub subject: String,
    pub predicate: String,
    pub statement: String,
    pub value: ClaimValue,
    pub applicability: Applicability,
    pub evidence: Vec<EvidenceRef>,
    pub correction: Option<Correction>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Correction {
    pub claim: ClaimId,
    pub predecessor: ClaimVersionId,
    pub reason: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observations {
    pub schema_version: u32,
    pub observations: Vec<Observation>,
}

/// Stable across restart and worker lease retries, without persistent side effects.
pub fn output_identity(workspace: &WorkspaceId, origin: &EventId, key: &str) -> Result<String> {
    Ok(digest_bytes(&canonical_bytes(&(
        workspace, origin, SPEC, key,
    ))?))
}

struct Reader<'a> {
    store: &'a Store,
    access: &'a Access,
    bytes: u64,
    cache: BTreeMap<ArtifactId, (ArtifactDescriptor, Vec<u8>)>,
}
impl Reader<'_> {
    fn read(&mut self, id: &ArtifactId) -> Result<Option<(ArtifactDescriptor, Vec<u8>)>> {
        if let Some(value) = self.cache.get(id) {
            return Ok(Some(value.clone()));
        }
        let Some(row) = self
            .store
            .state()
            .records
            .get(&vcp_store::contract::key(Collection::Artifact, id.as_str()))
        else {
            return Ok(None);
        };
        if row.workspace != self.access.workspace {
            return Err(Error::Access);
        }
        let descriptor: ArtifactDescriptor = row.decode()?;
        if !self.access.allows_task(&descriptor.spec.scope.task) {
            return Err(Error::Access);
        }
        if !crate::proof::complete_capture(&descriptor)
            || self.cache.len() >= MAX_ARTIFACTS
            || descriptor.length.get() > MAX_BYTES.saturating_sub(self.bytes)
        {
            return Ok(None);
        }
        let mut bytes = Vec::with_capacity(descriptor.length.get() as usize);
        match vcp_audit::history::History::read_artifact(
            self.store,
            &self.access.history(),
            id,
            &mut bytes,
        ) {
            Ok(_) => {}
            Err(vcp_audit::Error::Access) => return Err(Error::Access),
            Err(_) => return Ok(None),
        }
        self.bytes += descriptor.length.get();
        self.cache
            .insert(id.clone(), (descriptor.clone(), bytes.clone()));
        Ok(Some((descriptor, bytes)))
    }
}

fn proposal(
    workspace: &Workspace,
    access: &Access,
    event: &EventEnvelope,
    scope: &Scope,
    observation: Observation,
) -> Result<Proposal> {
    let identity = output_identity(&workspace.id, &event.event.id, &observation.output_key)?;
    let (claim, predecessor, correction_reason) = match observation.correction {
        Some(correction) => (
            correction.claim,
            Some(correction.predecessor),
            Some(correction.reason),
        ),
        None => (
            ClaimId::parse(digest_bytes(&canonical_bytes(&("claim", &identity))?))?,
            None,
            None,
        ),
    };
    Ok(Proposal {
        id: ProposalId::parse(digest_bytes(&canonical_bytes(&("proposal", &identity))?))?,
        command: CommandId::parse(digest_bytes(&canonical_bytes(&("command", &identity))?))?,
        claim,
        scope: scope.clone(),
        actor: access.actor.clone(),
        epochs: Epochs {
            authority: workspace.authority,
            deletion: workspace.deletion,
            policy: PolicyRevision::ZERO,
        },
        registry_version: REGISTRY_VERSION,
        extractor: SPEC.into(),
        output_key: observation.output_key,
        origins: vec![event.event.id.clone()],
        subject: observation.subject,
        predicate: observation.predicate,
        statement: observation.statement,
        value: observation.value,
        applicability: observation.applicability,
        evidence: observation.evidence,
        predecessor,
        correction_reason,
        retention: "workspace".into(),
    })
}

fn add_observation(
    reader: &mut Reader<'_>,
    workspace: &Workspace,
    event: &EventEnvelope,
    task: &Task,
    observation: Observation,
    result: &mut Extraction,
) -> Result<()> {
    if result.proposals.len() >= MAX_PROPOSALS {
        result.finding("limit", "proposal batch limit reached", None);
        return Ok(());
    }
    let mut candidate = proposal(workspace, reader.access, event, &task.scope, observation)?;
    candidate.epochs.policy = access::policy(reader.store.state(), &workspace.id)?;
    if candidate.validate().is_err() {
        result.finding(
            "invalid_observation",
            "typed observation violates the bounded registry",
            None,
        );
        return Ok(());
    }
    if result
        .proposals
        .iter()
        .any(|p| p.output_key == candidate.output_key)
    {
        result.finding(
            "duplicate_output",
            "typed observations reuse an output key",
            None,
        );
        return Ok(());
    }
    // These gates check provenance before any candidate reaches the repository;
    // repository governance independently repeats source/authority checks.
    if let ClaimValue::UserPreference {
        explicit_origin,
        key,
        value,
    } = &candidate.value
    {
        if !crate::repository::preference_matches(
            reader.store.state(),
            reader.access,
            explicit_origin,
            key,
            value,
        )? {
            result.finding(
                "invalid_user_origin",
                "preference requires explicit canonical user input",
                None,
            );
            return Ok(());
        }
        if !candidate.origins.contains(explicit_origin) {
            candidate.origins.push(explicit_origin.clone());
        }
    }
    for reference in &candidate.evidence {
        let Some((descriptor, _)) = reader.read(&reference.artifact)? else {
            result.finding(
                "missing_evidence",
                "evidence is unavailable, pruned, incomplete or over the batch limit",
                Some(reference.artifact.clone()),
            );
            return Ok(());
        };
        if descriptor.sha256 != reference.sha256
            || reference
                .range
                .as_ref()
                .is_some_and(|r| r.end > descriptor.length)
        {
            result.finding(
                "invalid_evidence",
                "evidence digest or span differs from retained content",
                Some(reference.artifact.clone()),
            );
            return Ok(());
        }
    }
    result.proposals.push(candidate);
    Ok(())
}

#[derive(Deserialize)]
struct CheckReceipt {
    plan: CheckPlan,
    outcome: CheckOutcome,
    applicability: String,
}
#[derive(Deserialize)]
struct CheckPlan {
    runner: String,
    origin: Option<CheckOrigin>,
    directory: String,
    request: CheckRequest,
    not_run: Option<String>,
}
#[derive(Deserialize)]
struct CheckOrigin {
    sha256: String,
}
#[derive(Deserialize)]
struct CheckRequest {
    arguments: Vec<String>,
    directory: String,
}

fn native_verification(
    reader: &mut Reader<'_>,
    workspace: &Workspace,
    event: &EventEnvelope,
    task: &Task,
    result: &mut Extraction,
) -> Result<()> {
    let Some(facts) = event
        .event
        .data
        .get("facts")
        .and_then(serde_json::Value::as_array)
    else {
        result.finding(
            "missing_verification",
            "verification event has no canonical facts",
            None,
        );
        return Ok(());
    };
    for value in facts.iter().take(MAX_PROPOSALS) {
        let Some(record) = fact_record(value, &workspace.id) else {
            continue;
        };
        if record.collection != Collection::Verification {
            continue;
        }
        let Some(current) = reader.store.state().records.get(&record.key()) else {
            continue;
        };
        if current.revision != record.revision
            || current.value != record.value
            || current.workspace != record.workspace
        {
            result.finding(
                "changed_verification",
                "verification fact differs from canonical record",
                None,
            );
            continue;
        }
        let verification: Verification = record.decode()?;
        if verification.scope.workspace != workspace.id
            || !reader.access.allows_task(&verification.scope.task)
        {
            return Err(Error::Access);
        }
        for check in verification.checks.iter().take(MAX_PROPOSALS) {
            let Some((check_artifact, bytes)) = reader.read(&check.output)? else {
                result.finding(
                    "missing_check",
                    "verification check receipt unavailable",
                    Some(check.output.clone()),
                );
                continue;
            };
            if check_artifact.spec.schema != "verification-check/1" {
                result.finding(
                    "unsupported_check",
                    "check has no native command receipt",
                    Some(check.output.clone()),
                );
                continue;
            }
            let Ok(receipt) = serde_json::from_slice::<CheckReceipt>(&bytes) else {
                result.finding(
                    "invalid_check",
                    "native command receipt is malformed",
                    Some(check.output.clone()),
                );
                continue;
            };
            if !matches!(receipt.plan.runner.as_str(), "node" | "cargo")
                || receipt.plan.arguments_invalid()
                || receipt.plan.not_run.is_some()
                || receipt.applicability != "current"
                || receipt.outcome != check.outcome
            {
                result.finding(
                    "unsupported_check",
                    "native command was not observed with complete current metadata",
                    Some(check.output.clone()),
                );
                continue;
            }
            let Some(origin) = receipt.plan.origin else {
                result.finding(
                    "missing_configuration",
                    "check does not retain a configuration identity",
                    Some(check.output.clone()),
                );
                continue;
            };
            let mut configuration = None;
            for id in verification.outputs.iter().take(MAX_ARTIFACTS) {
                // Filter by digest before loading unrelated captured outputs.
                let Some(row) = reader
                    .store
                    .state()
                    .records
                    .get(&vcp_store::contract::key(Collection::Artifact, id.as_str()))
                else {
                    continue;
                };
                if row.workspace != workspace.id {
                    return Err(Error::Access);
                }
                let descriptor: ArtifactDescriptor = row.decode()?;
                if descriptor.sha256 == origin.sha256 {
                    if let Some((descriptor, _)) = reader.read(id)? {
                        configuration = Some(descriptor);
                        break;
                    }
                }
            }
            let Some(configuration) = configuration else {
                result.finding(
                    "missing_configuration",
                    "check configuration bytes unavailable in verification outputs",
                    Some(check.output.clone()),
                );
                continue;
            };
            let mut argv = vec![receipt.plan.runner];
            argv.extend(receipt.plan.request.arguments);
            let cwd = if receipt.plan.directory.is_empty() {
                ".".into()
            } else {
                receipt.plan.directory
            };
            let evidence = vec![
                EvidenceRef {
                    artifact: check.output.clone(),
                    sha256: check_artifact.sha256,
                    range: None,
                    source: Some(verification.fingerprint.clone()),
                    verification: Some(verification.id.clone()),
                    kind: EvidenceKind::Verification,
                },
                EvidenceRef {
                    artifact: configuration.spec.id.clone(),
                    sha256: configuration.sha256,
                    range: None,
                    source: Some(verification.fingerprint.clone()),
                    verification: None,
                    kind: EvidenceKind::Configuration,
                },
            ];
            add_observation(
                reader,
                workspace,
                event,
                task,
                Observation {
                    output_key: format!("check-{}", check.output),
                    subject: check.specification.clone(),
                    predicate: "observed_test_command".into(),
                    statement: format!("Observed test command: {}", check.specification),
                    value: ClaimValue::Command {
                        purpose: CommandPurpose::Test,
                        argv,
                        cwd,
                        configuration: configuration.spec.id,
                        outcome: Some(check.outcome.clone()),
                        verification: Some(verification.id.clone()),
                    },
                    applicability: Applicability {
                        repository: workspace.binding.repository.clone(),
                        worktree: workspace.binding.worktree.clone(),
                        roots: vec![RootId::parse(workspace.id.as_str())?],
                        paths: vec![],
                        symbols: vec![],
                        branch: None,
                        fingerprint: Some(verification.fingerprint.clone()),
                        conditions: BTreeMap::new(),
                        valid_from: None,
                        valid_until: None,
                    },
                    evidence,
                    correction: None,
                },
                result,
            )?;
        }
        if verification.checks.len() > MAX_PROPOSALS || verification.outputs.len() > MAX_ARTIFACTS {
            result.finding(
                "limit",
                "verification extraction exceeds bounded input inventory",
                None,
            );
        }
    }
    Ok(())
}
impl CheckPlan {
    fn arguments_invalid(&self) -> bool {
        self.directory != self.request.directory
            || self.request.arguments.len() > 63
            || self
                .request
                .arguments
                .iter()
                .any(|s| s.len() > 4096 || s.contains('\0'))
    }
}

/// Pure discovery over a canonical event. Caller persists jobs, findings and
/// governed proposal receipts; no worker, model call or mutation starts here.
pub fn extract(store: &Store, access: &Access, event: &EventEnvelope) -> Result<Extraction> {
    let workspace = access::authorize(store.state(), access, false)?;
    if event.event.workspace != workspace.id
        || event
            .event
            .task
            .as_ref()
            .is_some_and(|t| !access.allows_task(t))
    {
        return Err(Error::Access);
    }
    if !store.state().events.iter().any(|saved| saved == event) {
        return Err(Error::Invalid(
            "extraction requires an unmodified canonical event".into(),
        ));
    }
    let mut result = Extraction::default();
    for row in store
        .state()
        .records
        .values()
        .filter(|r| r.workspace == workspace.id && r.collection == Collection::Tombstone)
    {
        let mask: vcp_audit::history::RetentionMask = row.decode()?;
        if mask.session == event.event.session
            && event.sequence >= mask.first
            && event.sequence <= mask.last
        {
            result.finding(
                "pruned_origin",
                "origin content is unavailable under current retention",
                None,
            );
            return Ok(result);
        }
    }
    let Some(task_id) = &event.event.task else {
        result.finding(
            "workspace_observation",
            "workspace event retained without a task-scoped claim",
            None,
        );
        return Ok(result);
    };
    let task: Task = store
        .state()
        .record(Collection::Task, task_id.as_str(), &workspace.id)?
        .decode()?;
    if task.scope.session != event.event.session {
        return Err(Error::Access);
    }
    let mut reader = Reader {
        store,
        access,
        bytes: 0,
        cache: BTreeMap::new(),
    };
    if event.event.kind == EventKind::VerificationRecorded {
        native_verification(&mut reader, &workspace, event, &task, &mut result)?;
    }
    let mut seen = BTreeSet::new();
    for id in event.event.artifacts.iter().take(MAX_ARTIFACTS) {
        if !seen.insert(id.clone()) {
            continue;
        }
        let Some((descriptor, bytes)) = reader.read(id)? else {
            result.finding(
                "missing_artifact",
                "observed artifact unavailable or exceeds extraction budget",
                Some(id.clone()),
            );
            continue;
        };
        if descriptor.spec.schema == "memory-observations/1" {
            let Ok(observations) = serde_json::from_slice::<Observations>(&bytes) else {
                result.finding(
                    "invalid_observation",
                    "typed observation artifact is malformed",
                    Some(id.clone()),
                );
                continue;
            };
            if observations.schema_version != 1 || observations.observations.len() > MAX_PROPOSALS {
                result.finding(
                    "limit",
                    "typed observation version or batch exceeds supported bounds",
                    Some(id.clone()),
                );
                continue;
            }
            for observation in observations.observations {
                add_observation(
                    &mut reader,
                    &workspace,
                    event,
                    &task,
                    observation,
                    &mut result,
                )?;
            }
        } else if matches!(
            descriptor.spec.schema.as_str(),
            "verification-source/1" | "handoff-source/1" | "vcp-memory-change/1"
        ) {
            result.finding("source_observed", "retained source or change evidence observed; no unsupported semantic claim inferred", Some(id.clone()));
        }
    }
    if event.event.artifacts.len() > MAX_ARTIFACTS {
        result.finding(
            "limit",
            "event artifact inventory exceeds extraction batch",
            None,
        );
    }
    if result.proposals.is_empty() && result.findings.is_empty() {
        let observed_task = event
            .event
            .data
            .get("facts")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|value| fact_record(value, &workspace.id))
            .find(|r| r.collection == Collection::Task && r.id == task.scope.task.as_str())
            .and_then(|r| r.decode::<Task>().ok());
        let observed = observed_task.as_ref().unwrap_or(&task);
        let (code, message) = match event.event.kind {
            EventKind::FingerprintObserved => (
                "external_observation",
                "source fingerprint changed; unobserved actor remains unknown",
            ),
            EventKind::TaskTransition | EventKind::TurnTransition if observed.parent.is_some() => (
                "child_observation",
                "child state retained with original task identity",
            ),
            EventKind::TaskTransition | EventKind::TurnTransition => (
                "work_observation",
                match observed.state {
                    TaskState::Completed => "completed work retained",
                    TaskState::Failed | TaskState::Cancelled | TaskState::Paused => {
                        "failed, cancelled or interrupted work retained"
                    }
                    _ => "work state retained",
                },
            ),
            _ => (
                "unsupported_observation",
                "activity retained; no supported explicit fact to promote",
            ),
        };
        result.finding(code, message, None);
    }
    Ok(result)
}

// Engine facts intentionally omit enclosing workspace and explicit references;
// their immutable event envelope supplies scope. They are not full store rows.
fn fact_record(value: &serde_json::Value, workspace: &WorkspaceId) -> Option<Record> {
    let mut value = value.clone();
    let object = value.as_object_mut()?;
    object
        .entry("workspace")
        .or_insert_with(|| serde_json::json!(workspace));
    object
        .entry("references")
        .or_insert_with(|| serde_json::json!([]));
    serde_json::from_value(value).ok()
}
