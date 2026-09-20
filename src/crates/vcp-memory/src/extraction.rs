// SPDX-License-Identifier: Apache-2.0
//! Optional extraction consumes an already captured, accounted model response.
//! This module cannot create a request, repair output, change policy or dispatch.
use crate::{
    access::{self, Access},
    Error, Result,
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
};
use vcp_domain::{
    accounting::{Attempt, Ledger, RequestRole, Reservation, ReservationState, Settlement},
    artifact::{ArtifactDescriptor, Channel},
    ids::*,
    memory::*,
    task::{Task, TaskState},
    workspace::Scope,
};
use vcp_protocol::{canonical_bytes, digest_bytes};
use vcp_store::{contract::Collection, Store};

#[derive(Clone, Debug)]
pub struct Limits {
    pub response_bytes: usize,
    pub candidate_bytes: usize,
    pub evidence_bytes: usize,
    pub depth: usize,
    pub candidates: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            response_bytes: 1024 * 1024,
            candidate_bytes: 256 * 1024,
            evidence_bytes: 4 * 1024 * 1024,
            depth: 32,
            candidates: 16,
        }
    }
}
impl Limits {
    fn validate(&self) -> Result<()> {
        if self.response_bytes == 0
            || self.response_bytes > 16 * 1024 * 1024
            || self.candidate_bytes == 0
            || self.candidate_bytes > 256 * 1024
            || self.evidence_bytes == 0
            || self.evidence_bytes > 64 * 1024 * 1024
            || self.depth == 0
            || self.depth > 64
            || self.candidates == 0
            || self.candidates > 32
        {
            return Err(invalid(
                "extraction limits are outside the bounded parser contract",
            ));
        }
        Ok(())
    }
}

/// Trusted host inputs. None of these fields can be supplied by candidate JSON.
pub struct ExtractionContext {
    pub origin: EventId,
    pub attempt: AttemptId,
    pub output_artifact: ArtifactId,
    pub extractor: String,
    pub applicability: Applicability,
    pub evidence: Vec<EvidenceRef>,
    pub retention: String,
    pub limits: Limits,
    /// An explicitly admitted maintenance budget may differ from the origin root.
    pub maintenance_root: Option<TaskId>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Output {
    schema_version: u32,
    candidates: Vec<Candidate>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Candidate {
    output_key: String,
    subject: String,
    predicate: String,
    statement: String,
    value: ClaimValue,
    evidence: Vec<ArtifactId>,
}

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

/// Count structural depth before serde allocates nested values. Braces inside
/// strings (including escaped quotes/backslashes) do not count as containers.
fn check_depth(bytes: &[u8], limit: usize) -> Result<()> {
    let mut depth = 0usize;
    let mut string = false;
    let mut escape = false;
    for &byte in bytes {
        if string {
            if escape {
                escape = false;
            } else if byte == b'\\' {
                escape = true;
            } else if byte == b'"' {
                string = false;
            }
        } else {
            match byte {
                b'"' => string = true,
                b'{' | b'[' => {
                    depth += 1;
                    if depth > limit {
                        return Err(invalid("model candidate nesting exceeds extraction limit"));
                    }
                }
                b'}' | b']' => {
                    depth = depth
                        .checked_sub(1)
                        .ok_or_else(|| invalid("unbalanced model candidate JSON"))?;
                }
                _ => (),
            }
        }
    }
    if depth != 0 || string {
        return Err(invalid("incomplete model candidate JSON"));
    }
    Ok(())
}

fn parse(bytes: &[u8], limits: &Limits) -> Result<Output> {
    limits.validate()?;
    if bytes.len() > limits.candidate_bytes {
        return Err(invalid(
            "model candidate output exceeds extraction byte limit",
        ));
    }
    check_depth(bytes, limits.depth)?;
    let output: Output = serde_json::from_slice(bytes)
        .map_err(|_| invalid("malformed or unsupported model candidate output"))?;
    if output.schema_version != 1 || output.candidates.len() > limits.candidates {
        return Err(invalid(
            "model candidate version or count exceeds extraction contract",
        ));
    }
    let mut keys = BTreeSet::new();
    for candidate in &output.candidates {
        if candidate.output_key.is_empty()
            || candidate.output_key.len() > 128
            || !candidate
                .output_key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            || !keys.insert(&candidate.output_key)
        {
            return Err(invalid(
                "candidate output keys must be unique stable identifiers",
            ));
        }
        if candidate.evidence.is_empty()
            || candidate.evidence.len() >= MAX_EVIDENCE
            || candidate.evidence.iter().collect::<BTreeSet<_>>().len() != candidate.evidence.len()
        {
            return Err(invalid(
                "candidate evidence references must be unique and bounded",
            ));
        }
    }
    Ok(output)
}

struct BoundedBytes {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for BoundedBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other("extraction capture byte limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn descriptor(store: &Store, access: &Access, id: &ArtifactId) -> Result<ArtifactDescriptor> {
    let artifact: ArtifactDescriptor = store
        .state()
        .record(Collection::Artifact, id.as_str(), &access.workspace)?
        .decode()?;
    if !access.allows_task(&artifact.spec.scope.task) {
        return Err(Error::Access);
    }
    if !crate::proof::complete_capture(&artifact) {
        return Err(invalid("extraction requires complete retained capture"));
    }
    Ok(artifact)
}

/// Parse exactly the retained response associated with a settled memory attempt.
/// Returned proposals still require repository::propose's current gates and
/// atomic commit. This function performs no mutation and accepts no repair call.
pub fn validate(
    store: &Store,
    access: &Access,
    context: &ExtractionContext,
) -> Result<Vec<Proposal>> {
    context.limits.validate()?;
    context.applicability.validate()?;
    let workspace = access::authorize(store.state(), access, true)?;
    if context.extractor.trim().is_empty()
        || context.extractor.len() > 256
        || context.extractor.contains('\0')
        || context.retention.trim().is_empty()
        || context.retention.len() > 256
        || context.retention.contains('\0')
        || context.evidence.len() >= MAX_EVIDENCE
    {
        return Err(invalid("invalid host extraction specification"));
    }
    if context.applicability.repository != workspace.binding.repository
        || context.applicability.worktree != workspace.binding.worktree
    {
        return Err(Error::Access);
    }
    let origin = store
        .state()
        .events
        .iter()
        .find(|event| event.event.id == context.origin)
        .ok_or_else(|| invalid("extraction origin is not retained"))?;
    let task_id = origin
        .event
        .task
        .as_ref()
        .ok_or_else(|| invalid("extraction requires a task-scoped origin"))?;
    if origin.event.workspace != access.workspace || !access.allows_task(task_id) {
        return Err(Error::Access);
    }
    let source: Task = store
        .state()
        .record(Collection::Task, task_id.as_str(), &access.workspace)?
        .decode()?;
    let scope = Scope {
        workspace: access.workspace.clone(),
        session: origin.event.session.clone(),
        task: task_id.clone(),
    };
    if source.scope != scope {
        return Err(Error::Access);
    }
    let attempt: Attempt = store
        .state()
        .record(
            Collection::Attempt,
            context.attempt.as_str(),
            &access.workspace,
        )?
        .decode()?;
    if !access.allows_task(&attempt.scope.task)
        || attempt.role != RequestRole::Memory
        || attempt.send_intent.is_none()
        || !matches!(
            attempt.phase,
            ReservationState::Settled | ReservationState::ExplicitlyResolved
        )
        || (attempt.root != source.root && context.maintenance_root.as_ref() != Some(&attempt.root))
    {
        return Err(invalid(
            "model extraction requires its own settled, admitted memory attempt",
        ));
    }
    let mut task: Task = store
        .state()
        .record(
            Collection::Task,
            attempt.scope.task.as_str(),
            &access.workspace,
        )?
        .decode()?;
    if task.scope != attempt.scope || task.steering != attempt.steering {
        return Err(invalid("extraction attempt task or steering is stale"));
    }
    let mut visited = BTreeSet::new();
    loop {
        if !visited.insert(task.scope.task.clone())
            || visited.len() > 256
            || task.state != TaskState::Running
            || task.root != attempt.root
            || !access.allows_task(&task.scope.task)
        {
            return Err(invalid("extraction attempt task ancestry is fenced"));
        }
        let Some(parent) = task.parent else {
            if task.scope.task != attempt.root {
                return Err(invalid("extraction attempt root mismatch"));
            }
            break;
        };
        task = store
            .state()
            .record(Collection::Task, parent.as_str(), &access.workspace)?
            .decode()?;
    }
    let reservation: Reservation = store
        .state()
        .record(
            Collection::Reservation,
            attempt.reservation.as_str(),
            &access.workspace,
        )?
        .decode()?;
    let ledger: Ledger = store
        .state()
        .record(Collection::Ledger, attempt.root.as_str(), &access.workspace)?
        .decode()?;
    if reservation.attempt != attempt.id
        || reservation.scope != attempt.scope
        || reservation.root != attempt.root
        || reservation.role != RequestRole::Memory
        || ledger.scope.task != attempt.root
    {
        return Err(invalid("extraction attempt accounting scope mismatch"));
    }
    let mut response_link = false;
    for row in
        store.state().records.values().filter(|row| {
            row.collection == Collection::Settlement && row.workspace == access.workspace
        })
    {
        let settlement: Settlement = row.decode()?;
        response_link |= settlement.attempt == attempt.id
            && settlement.scope == attempt.scope
            && settlement.applied
            && settlement.observation.final_usage
            && settlement.observation.raw == context.output_artifact;
    }
    if !response_link {
        return Err(invalid(
            "candidate capture is not linked to the accounted attempt response",
        ));
    }
    let response = descriptor(store, access, &context.output_artifact)?;
    if response.spec.scope != attempt.scope
        || response.spec.channel != Channel::Response
        || response.length.get() > context.limits.response_bytes as u64
    {
        return Err(invalid("invalid or oversized extraction response capture"));
    }
    let mut bytes = BoundedBytes {
        bytes: vec![],
        limit: context.limits.response_bytes,
    };
    vcp_audit::history::History::read_artifact(
        store,
        &access.history(),
        &context.output_artifact,
        &mut bytes,
    )
    .map_err(|_| invalid("extraction response capture is unavailable, denied or corrupt"))?;
    let tools = vcp_models::request::Tools::parse(&serde_json::json!([]))
        .map_err(|_| invalid("empty extraction tool schema unavailable"))?;
    let mut stream = vcp_models::stream::Stream::new(tools);
    stream
        .push(&bytes.bytes)
        .map_err(|_| invalid("malformed captured extraction stream"))?;
    let completed = stream
        .finish()
        .map_err(|_| invalid("incomplete captured extraction stream"))?;
    if completed.status != vcp_models::stream::Status::Completed
        || !completed.calls.is_empty()
        || completed.completed_messages.len() != 1
        || attempt.provider_request.as_deref() != Some(completed.response_id.as_str())
    {
        return Err(invalid(
            "extraction requires one completed assistant message and no tool calls",
        ));
    }
    let text = completed
        .completed_messages
        .values()
        .next()
        .ok_or_else(|| invalid("extraction message missing"))?;
    let output = parse(text.as_bytes(), &context.limits)?;
    let mut evidence = BTreeMap::new();
    let mut evidence_bytes = 0u64;
    for reference in &context.evidence {
        reference.validate()?;
        if reference.artifact == context.output_artifact
            || evidence
                .insert(reference.artifact.clone(), reference.clone())
                .is_some()
        {
            return Err(invalid(
                "host evidence set is duplicated or self-referential",
            ));
        }
        let source = descriptor(store, access, &reference.artifact)?;
        evidence_bytes = evidence_bytes
            .checked_add(source.length.get())
            .ok_or_else(|| invalid("extraction evidence byte overflow"))?;
        if evidence_bytes > context.limits.evidence_bytes as u64 {
            return Err(invalid("extraction evidence exceeds host byte limit"));
        }
        if reference.sha256 != source.sha256
            || reference
                .range
                .as_ref()
                .is_some_and(|range| range.end > source.length)
        {
            return Err(invalid(
                "host evidence version or range differs from retained source",
            ));
        }
        vcp_audit::history::History::read_artifact(
            store,
            &access.history(),
            &reference.artifact,
            std::io::sink(),
        )
        .map_err(|_| invalid("extraction source is unavailable, denied or corrupt"))?;
    }
    let mut proposals = Vec::new();
    for candidate in output.candidates {
        let mut references = candidate
            .evidence
            .iter()
            .map(|id| {
                evidence.get(id).cloned().ok_or_else(|| {
                    invalid("candidate invented an evidence ID outside its authorized set")
                })
            })
            .collect::<Result<Vec<_>>>()?;
        for id in candidate.value.artifacts() {
            if !candidate.evidence.contains(id) {
                return Err(invalid(
                    "candidate structured source is outside its cited evidence set",
                ));
            }
        }
        match &candidate.value {
            ClaimValue::ModuleRelationship { from, to, .. } => {
                for endpoint in [from, to] {
                    if !context.applicability.roots.contains(&endpoint.root)
                        || !context.applicability.paths.contains(&endpoint.path)
                        || !references.iter().any(|reference| {
                            reference.artifact == endpoint.artifact
                                && reference.sha256 == endpoint.sha256
                        })
                    {
                        return Err(invalid("candidate fabricated source scope or revision"));
                    }
                }
            }
            ClaimValue::Command {
                verification: Some(id),
                ..
            }
            | ClaimValue::VerifiedFix {
                verification: id, ..
            } => {
                if !references
                    .iter()
                    .any(|reference| reference.verification.as_ref() == Some(id))
                {
                    return Err(invalid("candidate invented a verification identity"));
                }
            }
            ClaimValue::UserPreference { .. } => {
                return Err(invalid("model extraction cannot assert explicit user preferences; use the deterministic user-input adapter"))
            }
            _ => (),
        }
        references.push(EvidenceRef {
            artifact: context.output_artifact.clone(),
            sha256: response.sha256.clone(),
            range: None,
            source: None,
            verification: None,
            kind: EvidenceKind::ModelInference,
        });
        let identity = digest_bytes(&canonical_bytes(&(
            &access.workspace,
            &context.origin,
            &context.extractor,
            &candidate.output_key,
        ))?);
        let id = |kind: &str| -> Result<String> {
            Ok(digest_bytes(&canonical_bytes(&(kind, &identity))?))
        };
        let mut value = candidate.value;
        if let ClaimValue::Architecture { inference, .. } = &mut value {
            *inference = true;
        }
        let proposal = Proposal {
            id: ProposalId::parse(id("proposal")?)?,
            command: CommandId::parse(id("command")?)?,
            claim: ClaimId::parse(id("claim")?)?,
            scope: scope.clone(),
            actor: access.actor.clone(),
            epochs: Epochs {
                authority: workspace.authority,
                deletion: workspace.deletion,
                policy: access::policy(store.state(), &access.workspace)?,
            },
            registry_version: REGISTRY_VERSION,
            extractor: context.extractor.clone(),
            output_key: candidate.output_key,
            origins: vec![context.origin.clone()],
            subject: candidate.subject,
            predicate: candidate.predicate,
            statement: candidate.statement,
            value,
            applicability: context.applicability.clone(),
            evidence: references,
            predecessor: None,
            correction_reason: None,
            retention: context.retention.clone(),
        };
        proposal.validate()?;
        access::proposal_scope(store.state(), access, &proposal)?;
        if crate::history::proposal_removed(store.state(), &access.workspace, &proposal)? {
            return Err(Error::Access);
        }
        proposals.push(proposal);
    }
    Ok(proposals)
}

#[cfg(test)]
mod parser_tests {
    use super::*;
    #[test]
    fn structural_depth_ignores_escaped_string_braces_and_bounds_real_containers() {
        check_depth(br#"{"text":"{[\\\"x\" ]}"}"#, 1).unwrap();
        assert!(check_depth(b"[[[[]]]]", 3).is_err());
        assert!(check_depth(b"[", 3).is_err());
        assert!(check_depth(b"]", 3).is_err());
    }
    #[test]
    fn unsupported_envelope_authority_and_size_are_not_repairable_by_parser() {
        assert!(parse(
            br#"{"schema_version":1,"candidates":[],"policy":"allow"}"#,
            &Limits::default()
        )
        .is_err());
        assert!(parse(b"not json", &Limits::default()).is_err());
        let limits = Limits {
            candidate_bytes: 2,
            ..Limits::default()
        };
        assert!(parse(br#"{"schema_version":1,"candidates":[]}"#, &limits).is_err());
    }
}
