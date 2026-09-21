// SPDX-License-Identifier: Apache-2.0
//! Owner declarations are consumed once at a scheduling boundary, not replayed
//! after failed admission or restart. A claim is not a successful handoff.
use super::*;
use vcp_domain::{
    artifact::{ArtifactDescriptor, CaptureState},
    workspace::Scope,
};
use vcp_models::escalation::{Trigger, TriggerKind};

pub const SCHEMA: &str = "vcp-owner-escalation-declaration/1";
const DOCUMENT: &str = "vcp_owner_escalation_declaration_v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Kind {
    DeclaredComplexity,
    UnsupportedCapability { capability: String },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub command: CommandId,
    pub task: TaskId,
    pub expected_revision: Revision,
    pub steering: SteeringRevision,
    pub declaration: Kind,
    pub evidence: Vec<ArtifactId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum State {
    Pending,
    Superseded {
        at: Timestamp,
    },
    /// No admission is implied. Inspect the canonical escalation admissions.
    Consumed {
        at: Timestamp,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Declaration {
    pub document_type: String,
    pub id: String,
    pub revision: Revision,
    pub scope: Scope,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
    pub binding: Revision,
    pub input: Input,
    pub previous_attempt: AttemptId,
    pub artifact: ArtifactId,
    pub recorded_at: Timestamp,
    pub state: State,
}
impl Declaration {
    pub fn trigger(&self) -> Trigger {
        let mut evidence = vec![self.artifact.clone()];
        evidence.extend(self.input.evidence.clone());
        Trigger {
            kind: match self.input.declaration {
                Kind::DeclaredComplexity => TriggerKind::DeclaredComplexity,
                Kind::UnsupportedCapability { .. } => TriggerKind::UnsupportedCapability,
            },
            observations: 1,
            evidence,
        }
    }
}
fn identity(access: &Access, command: &CommandId) -> String {
    format!(
        "routing-declaration-{}",
        digest_bytes(format!("{}:{command}", access.workspace).as_bytes())
    )
}
fn retained(store: &Store, access: &Access, collection: Collection, id: &str) -> Result<()> {
    if vcp_memory::retention::purged(
        store.state(),
        &access.workspace,
        &vcp_memory::retention::Target::Record(key(collection, id)),
    )
    .map_err(err)?
    {
        return Err("owner declaration evidence was purged".into());
    }
    Ok(())
}
fn evidence(
    store: &Store,
    access: &Access,
    scope: &Scope,
    id: &ArtifactId,
) -> Result<ArtifactDescriptor> {
    let descriptor: ArtifactDescriptor = store
        .state()
        .record(Collection::Artifact, id.as_str(), &access.workspace)
        .map_err(err)?
        .decode()
        .map_err(err)?;
    if descriptor.spec.scope != *scope
        || descriptor.state != CaptureState::Complete
        || vcp_memory::retention::purged(
            store.state(),
            &access.workspace,
            &vcp_memory::retention::Target::Record(key(Collection::Artifact, id.as_str())),
        )
        .map_err(err)?
    {
        return Err("declaration evidence is not complete retained task evidence".into());
    }
    Ok(descriptor)
}
fn latest_attempt(store: &Store, access: &Access, task: &Task) -> Result<Attempt> {
    if store.state().events.len() > 100_000 {
        return Err("declaration history bound".into());
    }
    for envelope in store.state().events.iter().rev().filter(|e| {
        e.event.workspace == access.workspace
            && e.event.task.as_ref() == Some(&task.scope.task)
            && e.event.kind == EventKind::ReservationCreated
            && e.redaction.is_none()
    }) {
        let id = envelope.event.data["attempt"]["id"]
            .as_str()
            .ok_or("reservation attempt identity missing")?;
        let attempt: Attempt = store
            .state()
            .record(Collection::Attempt, id, &access.workspace)
            .map_err(err)?
            .decode()
            .map_err(err)?;
        if !matches!(attempt.role, RequestRole::Main | RequestRole::Child) {
            continue;
        }
        if attempt.scope != task.scope
            || attempt.steering != task.steering
            || attempt.redaction.is_some()
        {
            return Err("declaration needs a current task model attempt".into());
        }
        return Ok(attempt);
    }
    Err("declaration needs a prior task model attempt".into())
}
pub fn validate(store: &Store, access: &Access, input: &Input) -> Result<(Task, Attempt)> {
    authorize(store, access, false)?;
    if !access.allows_task(&input.task)
        || input.evidence.is_empty()
        || input.evidence.len() > 63
        || input.evidence.iter().collect::<BTreeSet<_>>().len() != input.evidence.len()
    {
        return Err("declaration task access or evidence bound".into());
    }
    if let Kind::UnsupportedCapability { capability } = &input.declaration {
        if capability.is_empty()
            || capability.len() > 128
            || !capability
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err("declaration needs a bounded exact capability key".into());
        }
        let registry =
            current_registry(store, access)?.ok_or("declaration requires current registry")?;
        if !registry.value.catalog.entries.iter().any(|candidate| {
            candidate.capabilities.get(capability) == Some(&vcp_models::routing::State::Supported)
        }) {
            return Err("declared capability has no explicitly supported catalog candidate".into());
        }
    }
    let task: Task = store
        .state()
        .record(Collection::Task, input.task.as_str(), &access.workspace)
        .map_err(err)?
        .decode()
        .map_err(err)?;
    if task.revision != input.expected_revision
        || task.steering != input.steering
        || task.redaction.is_some()
    {
        return Err("declaration task revision or steering changed".into());
    }
    let mut current = task.clone();
    let mut visited = BTreeSet::new();
    loop {
        if !visited.insert(current.scope.task.clone())
            || visited.len() > 64
            || current.state != TaskState::Running
            || current.root != task.root
        {
            return Err("declaration requires running task and ancestors".into());
        }
        let Some(parent) = current.parent else {
            break;
        };
        current = store
            .state()
            .record(Collection::Task, parent.as_str(), &access.workspace)
            .map_err(err)?
            .decode()
            .map_err(err)?;
    }
    for id in &input.evidence {
        evidence(store, access, &task.scope, id)?;
    }
    let attempt = latest_attempt(store, access, &task)?;
    Ok((task, attempt))
}
pub fn existing(store: &Store, access: &Access, input: &Input) -> Result<Option<Declaration>> {
    authorize(store, access, false)?;
    retained(
        store,
        access,
        Collection::Projection,
        &identity(access, &input.command),
    )?;
    let record: Option<Declaration> = read(store, access, &identity(access, &input.command))?;
    if let Some(record) = &record {
        if record.input != *input || !access.allows_task(&record.scope.task) {
            return Err("declaration command reused with different input or denied task".into());
        }
    }
    Ok(record)
}
pub fn list(store: &Store, access: &Access, task: &TaskId) -> Result<Vec<Declaration>> {
    authorize(store, access, false)?;
    if !access.allows_task(task) {
        return Err("declaration task access denied".into());
    }
    let mut declarations = Vec::new();
    for record in store.state().records.values().filter(|r| {
        r.collection == Collection::Projection
            && r.workspace == access.workspace
            && r.value["document_type"] == DOCUMENT
    }) {
        let declaration: Declaration = decode_record(record)?;
        if &declaration.scope.task == task {
            retained(store, access, Collection::Projection, &record.id)?;
            declarations.push(declaration);
        }
        if declarations.len() > 1024 {
            return Err("declaration history bound".into());
        }
    }
    Ok(declarations)
}
pub fn inspect(store: &Store, access: &Access, task: &TaskId) -> Result<serde_json::Value> {
    let declarations = list(store, access, task)?;
    let task: Task = store
        .state()
        .record(Collection::Task, task.as_str(), &access.workspace)
        .map_err(err)?
        .decode()
        .map_err(err)?;
    let admissions = admitted_escalations(store, access, &task.scope)?;
    let records: Vec<_> = declarations.into_iter().map(|declaration| {
        let admitted: Vec<_> = admissions.iter().filter(|a| a.plan.trigger.evidence.contains(&declaration.artifact))
            .map(|a| a.attempt.clone()).collect();
        let stale_reason = if declaration.state == State::Pending { current_fence(store, access, &declaration).err() } else { None };
        let disposition = match (&declaration.state, admitted.is_empty()) {
            (State::Consumed { .. }, true) => "consumed_without_admission",
            (State::Consumed { .. }, false) => "admitted",
            (State::Pending, _) if stale_reason.is_some() => "stale",
            (State::Pending, _) => "pending",
            (State::Superseded { .. }, _) => "superseded",
        };
        serde_json::json!({"declaration": declaration, "disposition": disposition, "stale_reason": stale_reason, "admitted_attempts": admitted})
    }).collect();
    Ok(serde_json::json!({"declarations": records,
        "consumed_meaning": "Consumed once for scheduling; not proof of admission. A failed scheduling attempt requires a fresh explicit declaration."}))
}
/// An admitted capability declaration remains a task requirement throughout its
/// steering revision. Later requests cannot silently return to unsupported models.
pub fn admitted_capabilities(
    store: &Store,
    access: &Access,
    scope: &Scope,
    steering: SteeringRevision,
) -> Result<BTreeSet<String>> {
    let admissions = admitted_escalations(store, access, scope)?;
    let declarations = list(store, access, &scope.task)?;
    let mut required = BTreeSet::new();
    for admission in admissions.iter().filter(|admission| {
        admission.scope == *scope
            && admission.plan.revisions.steering == steering
            && admission.plan.trigger.kind == TriggerKind::UnsupportedCapability
    }) {
        let matched: Vec<_> = declarations
            .iter()
            .filter(|declaration| {
                declaration.scope == *scope
                    && declaration.input.steering == steering
                    && matches!(declaration.state, State::Consumed { .. })
                    && admission
                        .plan
                        .trigger
                        .evidence
                        .contains(&declaration.artifact)
            })
            .collect();
        let [declaration] = matched.as_slice() else {
            return Err("admitted capability declaration unavailable or ambiguous; cannot drop task requirement".into());
        };
        let Kind::UnsupportedCapability { capability } = &declaration.input.declaration else {
            return Err("admitted capability declaration kind changed".into());
        };
        required.insert(capability.clone());
    }
    Ok(required)
}
pub async fn record(
    store: &mut Store,
    access: &Access,
    input: Input,
    artifact: ArtifactId,
    now: Timestamp,
) -> Result<Declaration> {
    authorize(store, access, true)?;
    if let Some(existing) = existing(store, access, &input)? {
        return Ok(existing);
    }
    let (task, previous) = validate(store, access, &input)?;
    let source = evidence(store, access, &task.scope, &artifact)?;
    if source.spec.schema != SCHEMA
        || source.sha256 != digest_bytes(&canonical_bytes(&input).map_err(err)?)
    {
        return Err("declaration artifact differs from typed owner input".into());
    }
    let workspace: Workspace = store
        .state()
        .record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        )
        .map_err(err)?
        .decode()
        .map_err(err)?;
    let mut mutations = Vec::new();
    for mut prior in list(store, access, &input.task)?
        .into_iter()
        .filter(|d| d.state == State::Pending)
    {
        if prior.input.expected_revision == input.expected_revision
            && prior.input.steering == input.steering
            && prior.authority == access.authority
            && prior.deletion == workspace.deletion
            && prior.binding == workspace.binding.revision
            && prior.previous_attempt == previous.id
        {
            return Err("task already has a current pending owner escalation declaration".into());
        }
        let old = store
            .state()
            .record(Collection::Projection, &prior.id, &access.workspace)
            .map_err(err)?;
        let expected = prior.revision;
        prior.revision = expected.next().map_err(err)?;
        prior.state = State::Superseded { at: now };
        let mut record = row(access, prior.id.clone(), prior.revision, &prior)?;
        record.references = old.references.clone();
        mutations.push(Mutation::Put {
            record,
            expected: Some(expected),
        });
    }
    let value = Declaration {
        document_type: DOCUMENT.into(),
        id: identity(access, &input.command),
        revision: Revision::ZERO,
        scope: task.scope,
        authority: access.authority,
        deletion: workspace.deletion,
        binding: workspace.binding.revision,
        input,
        previous_attempt: previous.id,
        artifact,
        recorded_at: now,
        state: State::Pending,
    };
    let mut record = row(access, value.id.clone(), value.revision, &value)?;
    record
        .references
        .insert(key(Collection::Task, value.scope.task.as_str()));
    record
        .references
        .insert(key(Collection::Attempt, value.previous_attempt.as_str()));
    for artifact in value
        .input
        .evidence
        .iter()
        .chain(std::iter::once(&value.artifact))
    {
        record
            .references
            .insert(key(Collection::Artifact, artifact.as_str()));
    }
    mutations.push(Mutation::Put {
        record,
        expected: None,
    });
    commit(store, access, mutations, value.input.command.clone(), now).await?;
    Ok(value)
}
/// Claim before selection: never silently resend a consumed declaration. Any
/// later error leaves `Consumed` and no admission, requiring fresh owner input.
pub async fn claim(
    store: &mut Store,
    access: &Access,
    task: &TaskId,
    now: Timestamp,
) -> Result<Option<(Declaration, Attempt)>> {
    authorize(store, access, true)?;
    let pending: Vec<_> = list(store, access, task)?
        .into_iter()
        .filter(|d| d.state == State::Pending)
        .collect();
    let current: Task = store
        .state()
        .record(Collection::Task, task.as_str(), &access.workspace)
        .map_err(err)?
        .decode()
        .map_err(err)?;
    let matching: Vec<_> = pending
        .into_iter()
        .filter(|d| {
            d.input.expected_revision == current.revision && d.input.steering == current.steering
        })
        .collect();
    if matching.len() > 1 {
        return Err("ambiguous pending owner declarations".into());
    }
    let Some(mut value) = matching.into_iter().next() else {
        return Ok(None);
    };
    let attempt = current_fence(store, access, &value)?;
    let prior = store
        .state()
        .record(Collection::Projection, &value.id, &access.workspace)
        .map_err(err)?;
    let expected = value.revision;
    value.revision = expected.next().map_err(err)?;
    value.state = State::Consumed { at: now };
    let mut record = row(access, value.id.clone(), value.revision, &value)?;
    record.references = prior.references.clone();
    commit(
        store,
        access,
        vec![Mutation::Put {
            record,
            expected: Some(expected),
        }],
        CommandId::new(),
        now,
    )
    .await?;
    Ok(Some((value, attempt)))
}
/// Inspection uses exactly the same read-only source fence as consumption.
fn current_fence(store: &Store, access: &Access, value: &Declaration) -> Result<Attempt> {
    let (task, attempt) = validate(store, access, &value.input)?;
    let workspace: Workspace = store
        .state()
        .record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        )
        .map_err(err)?
        .decode()
        .map_err(err)?;
    if value.scope != task.scope
        || value.authority != access.authority
        || value.deletion != workspace.deletion
        || value.binding != workspace.binding.revision
        || value.previous_attempt != attempt.id
    {
        return Err("owner declaration authority, source or predecessor changed".into());
    }
    let artifact = evidence(store, access, &value.scope, &value.artifact)?;
    if artifact.spec.schema != SCHEMA
        || artifact.sha256 != digest_bytes(&canonical_bytes(&value.input).map_err(err)?)
    {
        return Err("owner declaration artifact changed".into());
    }
    Ok(attempt)
}
