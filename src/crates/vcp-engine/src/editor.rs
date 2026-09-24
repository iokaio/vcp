// SPDX-License-Identifier: Apache-2.0
//! Durable editor metadata. Native preparation/policy and draft bytes stay in the lifecycle host.
use crate::{Access, Engine, controller::ControllerToken, public::PublicError};
use std::collections::BTreeMap;
use vcp_domain::{
    editor::{self as domain, BufferFile, BufferState, ChangeSet, FileState, Observation},
    effect::{Effect, EffectState},
    ids::*,
    revision::*,
    task::{Task, TaskState},
    workspace::{Scope, Trust, Workspace},
};
use vcp_protocol::{
    command::{CommandReceipt, CommandResult},
    editor as wire,
    event::{EventInput, EventKind},
    methods::{self, Call},
};
use vcp_store::contract::{
    CanonicalStore, Collection, Mutation, ReceiptInput, Record, State, Transaction, key,
};
type Result<T> = std::result::Result<T, PublicError>;
fn invalid<E>(_: E) -> PublicError {
    PublicError::InvalidParameters
}
fn unavailable<E>(_: E) -> PublicError {
    PublicError::Unavailable
}
fn counter(v: &methods::Counter) -> Result<u64> {
    v.as_str().parse().map_err(invalid)
}
fn id(v: &str) -> Result<methods::Id> {
    v.to_owned().try_into().map_err(invalid)
}
pub struct EditorPrepareFacts {
    pub generation: String,
    pub root: RootId,
    pub host: HostId,
    pub binding: Revision,
    pub policy: PolicyRevision,
    pub files: Vec<domain::File>,
    pub now: Timestamp,
}
pub struct EditorDispatchFacts {
    pub generation: String,
    pub observation: Observation,
    pub policy: PolicyRevision,
    pub now: Timestamp,
}
pub struct EditorResultFacts {
    pub observation: Observation,
    pub matched_disk: bool,
    pub now: Timestamp,
}
pub struct EditorObserveFacts {
    pub closed: Vec<String>,
    pub observations: Vec<(Observation, bool)>,
    pub now: Timestamp,
}
pub struct EditorCommit {
    pub receipt: CommandReceipt,
    pub change: ChangeSet,
    pub replay: bool,
}
pub fn change_id(p: &wire::EditorPrepare) -> Result<String> {
    Ok(p.mutation.command_id.as_str().to_owned())
}
fn buffer_id(scope: &Scope) -> Result<String> {
    Ok(format!(
        "editor-buffers-{}",
        vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(scope).map_err(invalid)?)
    ))
}
fn buffers(state: &State, scope: &Scope) -> Result<BufferState> {
    let name = buffer_id(scope)?;
    match state.records.get(&key(Collection::Projection, &name)) {
        None => Ok(BufferState {
            document_type: domain::BUFFERS.into(),
            schema_version: domain::SCHEMA_VERSION,
            id: name,
            scope: scope.clone(),
            revision: Revision::ZERO,
            files: BTreeMap::new(),
        }),
        Some(row) => {
            let value: BufferState = row.decode().map_err(unavailable)?;
            value.validate().map_err(unavailable)?;
            if value.scope != *scope
                || row.workspace != scope.workspace
                || row.revision != value.revision
            {
                return Err(PublicError::Unavailable);
            }
            Ok(value)
        }
    }
}
pub fn buffer_status(state: &State, scope: &Scope) -> Result<(String, bool)> {
    let value = buffers(state, scope)?;
    if value.files.is_empty() {
        return Ok((
            vcp_protocol::digest_bytes(b"no-editor-buffers/native-cli-v1"),
            false,
        ));
    }
    Ok((
        vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&value.files).map_err(invalid)?),
        value.unverified(),
    ))
}
fn buffer_update(
    state: &State,
    scope: &Scope,
    values: impl IntoIterator<Item = (Observation, bool)>,
) -> Result<(BufferState, Option<Revision>)> {
    let mut next = buffers(state, scope)?;
    let expected = state
        .records
        .contains_key(&key(Collection::Projection, &next.id))
        .then_some(next.revision);
    if let Some(revision) = expected {
        next.revision = revision.next().map_err(invalid)?;
    }
    for (observation, matched) in values {
        observation.validate().map_err(invalid)?;
        next.files.insert(
            observation.path.clone(),
            BufferFile {
                uncertain: !matched,
                observation,
            },
        );
    }
    next.validate().map_err(invalid)?;
    Ok((next, expected))
}
fn record<T: serde::Serialize>(
    collection: Collection,
    name: &str,
    scope: &Scope,
    revision: Revision,
    value: &T,
) -> Result<Record> {
    Record::typed(collection, name, scope.workspace.clone(), revision, value).map_err(invalid)
}
impl<S: CanonicalStore> Engine<S> {
    pub fn editor_task(
        &self,
        access: &Access,
        scope: &methods::Scope,
        task: &methods::Id,
    ) -> Result<Task> {
        let value = self.read_task(access, scope, task).map_err(unavailable)?;
        if value.redaction.is_some() {
            return Err(PublicError::Unavailable);
        }
        Ok(value)
    }
    pub fn editor_replay(
        &self,
        access: &Access,
        connection: &ControllerId,
        token: &ControllerToken,
        call: &Call,
    ) -> Result<Option<CommandReceipt>> {
        self.check_controller(access, connection, token)
            .map_err(|_| PublicError::Access)?;
        call.validate().map_err(invalid)?;
        let (scope, task) = match call {
            Call::EditorContext(p) => (&p.scope, &p.task),
            Call::EditorPrepare(p) => (&p.scope, &p.task),
            Call::EditorDispatch(p) => (&p.scope, &p.task),
            Call::EditorChangeResult(p) => (&p.scope, &p.task),
            _ => return Err(PublicError::InvalidParameters),
        };
        self.editor_task(access, scope, task)?;
        let command = CommandId::parse(
            call.command_id()
                .ok_or(PublicError::InvalidParameters)?
                .as_str(),
        )
        .map_err(invalid)?;
        self.store()
            .state()
            .command(
                &access.workspace,
                &command,
                &call.digest(access.actor.as_str()).map_err(invalid)?,
            )
            .map_err(|_| PublicError::CommandConflict)
    }
    pub fn editor_read(&self, access: &Access, p: &wire::EditorChangeRead) -> Result<ChangeSet> {
        let task = self.editor_task(access, &p.scope, &p.task)?;
        let row = self
            .store()
            .state()
            .record(Collection::Projection, p.change.as_str(), &access.workspace)
            .map_err(unavailable)?;
        let change: ChangeSet = row.decode().map_err(unavailable)?;
        change.validate().map_err(unavailable)?;
        if change.scope != task.scope
            || change.id != p.change.as_str()
            || change.revision != row.revision
        {
            return Err(PublicError::Unavailable);
        }
        Ok(change)
    }
    fn editor_workspace(&self, access: &Access) -> Result<Workspace> {
        self.store()
            .state()
            .record(
                Collection::Workspace,
                access.workspace.as_str(),
                &access.workspace,
            )
            .map_err(unavailable)?
            .decode()
            .map_err(unavailable)
    }
    fn editor_revision(
        &self,
        task: &Task,
        mutation: &methods::Mutation,
        revision: Revision,
    ) -> Result<()> {
        if task.steering.get() != counter(&mutation.steering_revision)?
            || revision.get() != counter(&mutation.expected_revision)?
        {
            return Err(PublicError::StaleState);
        }
        Ok(())
    }
    async fn editor_commit(
        &mut self,
        access: &Access,
        call: &Call,
        scope: &Scope,
        now: Timestamp,
        revision: Revision,
        mut records: Vec<(Record, Option<Revision>)>,
        kind: EventKind,
    ) -> Result<CommandReceipt> {
        let command = CommandId::parse(
            call.command_id()
                .ok_or(PublicError::InvalidParameters)?
                .as_str(),
        )
        .map_err(invalid)?;
        let event_id = EventId::new();
        for (row, _) in &mut records {
            if matches!(row.collection, Collection::Task | Collection::Effect) {
                row.value["cause"] = serde_json::to_value(&event_id).map_err(invalid)?;
            }
        }
        let event = EventInput {
            id: event_id,
            workspace: scope.workspace.clone(),
            session: scope.session.clone(),
            task: Some(scope.task.clone()),
            actor: access.actor.clone(),
            correlation: command.clone(),
            causation: None,
            timestamp: now,
            kind,
            artifacts: vec![],
            data: serde_json::json!({"schema_version":1,"facts":records.iter().map(|(row,_)|serde_json::json!({"collection":row.collection,"id":row.id,"revision":row.revision,"value":row.value})).collect::<Vec<_>>()}),
            metadata: None,
        };
        let watermark = self.store().state().watermark;
        let receipt = self
            .store_mut()
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: watermark,
                mutations: records
                    .into_iter()
                    .map(|(record, expected)| Mutation::Put { record, expected })
                    .collect(),
                events: vec![event],
                command: Some(ReceiptInput {
                    command,
                    workspace: scope.workspace.clone(),
                    session: scope.session.clone(),
                    digest: call.digest(access.actor.as_str()).map_err(invalid)?,
                    result: CommandResult::Accepted { revision },
                }),
            })
            .await
            .map_err(|_| PublicError::OutcomeUnknown)?;
        receipt.command.ok_or(PublicError::OutcomeUnknown)
    }
    pub async fn editor_observe(
        &mut self,
        p: &wire::EditorContext,
        access: &Access,
        connection: &ControllerId,
        token: &ControllerToken,
        facts: &EditorObserveFacts,
    ) -> Result<CommandReceipt> {
        let call = Call::EditorContext(p.clone());
        if let Some(receipt) = self.editor_replay(access, connection, token, &call)? {
            return Ok(receipt);
        }
        let mut task = self.editor_task(access, &p.scope, &p.task)?;
        self.editor_revision(&task, &p.mutation, task.revision)?;
        if facts.observations.len() != p.documents.len() {
            return Err(PublicError::InvalidParameters);
        }
        for ((actual, _), input) in facts.observations.iter().zip(&p.documents) {
            check_observation(actual, input)?;
        }
        let (mut next, expected) = buffer_update(
            self.store().state(),
            &task.scope,
            facts.observations.clone(),
        )?;
        if facts.closed
            != p.closed
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect::<Vec<_>>()
        {
            return Err(PublicError::InvalidParameters);
        }
        for closed in &facts.closed {
            let path = next
                .files
                .iter()
                .find(|(_, file)| file.observation.id == *closed)
                .map(|(path, _)| path.clone())
                .ok_or(PublicError::StaleState)?;
            if facts
                .observations
                .iter()
                .any(|(observed, _)| observed.path == path)
            {
                return Err(PublicError::InvalidParameters);
            }
            next.files.remove(&path);
        }
        // Superseding an observation withdraws only an undispatched proposal.
        // In-flight and unknown executions remain recovery obligations.
        let mut retired = Vec::new();
        for row in self.store().state().records.values().filter(|row| {
            row.collection == Collection::Projection
                && row.workspace == access.workspace
                && row.value["document_type"] == domain::CHANGE
        }) {
            let mut change: ChangeSet = row.decode().map_err(unavailable)?;
            if change.scope != task.scope {
                continue;
            }
            let previous = change.revision;
            let mut changed = false;
            for file in &mut change.files {
                if file.state != FileState::Prepared || file.execution.is_some() {
                    continue;
                }
                let replacement = facts
                    .observations
                    .iter()
                    .map(|(observation, _)| observation)
                    .find(|observation| {
                        observation.path == file.observation.path
                            && observation.root == file.observation.root
                            && *observation != &file.observation
                    });
                let closed = facts.closed.contains(&file.observation.id);
                if replacement.is_none() && !closed {
                    continue;
                }
                let mut effect: Effect = self
                    .store()
                    .state()
                    .record(Collection::Effect, file.effect.as_str(), &access.workspace)
                    .map_err(unavailable)?
                    .decode()
                    .map_err(unavailable)?;
                if effect.scope != task.scope
                    || effect.operation_digest != file.operation_digest
                    || effect.execution.is_some()
                    || !matches!(
                        effect.state,
                        EffectState::Proposed | EffectState::Validated | EffectState::Authorized
                    )
                {
                    continue;
                }
                let effect_revision = effect.revision;
                effect = effect
                    .transition(
                        effect.revision,
                        effect.steering,
                        EffectState::Cancelled,
                        EventId::new(),
                        if closed {
                            "editor proposal withdrawn because its observed document closed"
                        } else {
                            "editor proposal withdrawn because its source observation changed"
                        }
                        .into(),
                    )
                    .map_err(invalid)?;
                file.state = FileState::Rejected;
                // Before-dispatch rejection has no execution. On close the last
                // known observation is retained as historical evidence only.
                file.observed = Some(replacement.unwrap_or(&file.observation).clone());
                retired.push((
                    record(
                        Collection::Effect,
                        effect.id.as_str(),
                        &task.scope,
                        effect.revision,
                        &effect,
                    )?,
                    Some(effect_revision),
                ));
                changed = true;
            }
            if changed {
                change.revision = previous.next().map_err(invalid)?;
                retired.push((
                    record(
                        Collection::Projection,
                        &change.id,
                        &task.scope,
                        change.revision,
                        &change,
                    )?,
                    Some(previous),
                ));
                if retired.len() > 256 {
                    return Err(PublicError::Unavailable);
                }
            }
        }
        let prior = task.revision;
        task.revision = prior.next().map_err(invalid)?;
        task.fingerprint.buffers = if next.files.is_empty() {
            vcp_protocol::digest_bytes(b"no-editor-buffers/native-cli-v1")
        } else {
            vcp_protocol::digest_bytes(
                &vcp_protocol::canonical_bytes(&next.files).map_err(invalid)?,
            )
        };
        task.reason="editor observations changed; disk-only verification does not cover dirty or uncertain buffers".into();
        let mut records = vec![
            (
                record(
                    Collection::Projection,
                    &next.id,
                    &task.scope,
                    next.revision,
                    &next,
                )?,
                expected,
            ),
            (
                record(
                    Collection::Task,
                    task.scope.task.as_str(),
                    &task.scope,
                    task.revision,
                    &task,
                )?,
                Some(prior),
            ),
        ];
        records.extend(retired);
        self.editor_commit(
            access,
            &call,
            &task.scope,
            facts.now,
            task.revision,
            records,
            EventKind::FingerprintObserved,
        )
        .await
    }
    pub async fn editor_prepare(
        &mut self,
        p: &wire::EditorPrepare,
        access: &Access,
        connection: &ControllerId,
        token: &ControllerToken,
        facts: &EditorPrepareFacts,
    ) -> Result<EditorCommit> {
        let call = Call::EditorPrepare(p.clone());
        let name = change_id(p)?;
        if let Some(receipt) = self.editor_replay(access, connection, token, &call)? {
            let change = self.editor_read(
                access,
                &wire::EditorChangeRead {
                    scope: p.scope.clone(),
                    task: p.task.clone(),
                    change: id(&name)?,
                },
            )?;
            return Ok(EditorCommit {
                receipt,
                change,
                replay: true,
            });
        }
        let task = self.editor_task(access, &p.scope, &p.task)?;
        self.editor_revision(&task, &p.mutation, task.revision)?;
        let workspace = self.editor_workspace(access)?;
        if task.state != TaskState::Running
            || workspace.trust != Trust::Trusted
            || workspace.binding.host != facts.host
            || workspace.binding.revision != facts.binding
            || p.generation.as_str() != facts.generation
            || facts.files.len() != p.files.len()
        {
            return Err(PublicError::StaleState);
        }
        let policy = crate::policy::optional(self.store().state(), &access.workspace)
            .map_err(unavailable)?
            .map_or(PolicyRevision::ZERO, |p| p.revision);
        if policy != facts.policy {
            return Err(PublicError::StaleState);
        }
        for (file, input) in facts.files.iter().zip(&p.files) {
            let effect: Effect = self
                .store()
                .state()
                .record(Collection::Effect, file.effect.as_str(), &access.workspace)
                .map_err(unavailable)?
                .decode()
                .map_err(unavailable)?;
            if file.state != FileState::Prepared
                || file.execution.is_some()
                || file.observed.is_some()
                || file.observation.id != input.observation.as_str()
                || effect.scope != task.scope
                || effect.steering != task.steering
                || effect.state != EffectState::Validated
                || effect.operation_digest != file.operation_digest
                || file.edits_digest
                    != vcp_protocol::digest_bytes(
                        &vcp_protocol::canonical_bytes(&input.edits).map_err(invalid)?,
                    )
            {
                return Err(PublicError::StaleState);
            }
        }
        let change = ChangeSet {
            document_type: domain::CHANGE.into(),
            schema_version: domain::SCHEMA_VERSION,
            id: name,
            scope: task.scope.clone(),
            revision: Revision::ZERO,
            actor: access.actor.clone(),
            connection: connection.clone(),
            generation: facts.generation.clone(),
            root: facts.root.clone(),
            host: facts.host.clone(),
            binding: facts.binding,
            authority: access.authority,
            policy: facts.policy,
            steering: task.steering,
            files: facts.files.clone(),
        };
        change.validate().map_err(invalid)?;
        let receipt = self
            .editor_commit(
                access,
                &call,
                &task.scope,
                facts.now,
                change.revision,
                vec![(
                    record(
                        Collection::Projection,
                        &change.id,
                        &task.scope,
                        change.revision,
                        &change,
                    )?,
                    None,
                )],
                EventKind::EffectTransition,
            )
            .await?;
        Ok(EditorCommit {
            receipt,
            change,
            replay: false,
        })
    }
    pub async fn editor_dispatch(
        &mut self,
        p: &wire::EditorDispatch,
        access: &Access,
        connection: &ControllerId,
        token: &ControllerToken,
        facts: &EditorDispatchFacts,
    ) -> Result<EditorCommit> {
        let call = Call::EditorDispatch(p.clone());
        let mut change = self.editor_read(
            access,
            &wire::EditorChangeRead {
                scope: p.scope.clone(),
                task: p.task.clone(),
                change: p.change.clone(),
            },
        )?;
        if let Some(receipt) = self.editor_replay(access, connection, token, &call)? {
            return Ok(EditorCommit {
                receipt,
                change,
                replay: true,
            });
        }
        let task = self.editor_task(access, &p.scope, &p.task)?;
        self.editor_revision(&task, &p.mutation, change.revision)?;
        let workspace = self.editor_workspace(access)?;
        if task.state != TaskState::Running
            || workspace.trust != Trust::Trusted
            || change.actor != access.actor
            || change.connection != *connection
            || change.generation != p.generation.as_str()
            || change.generation != facts.generation
            || change.authority != access.authority
            || change.binding != workspace.binding.revision
            || change.host != workspace.binding.host
            || change.policy != facts.policy
            || change.steering != task.steering
        {
            return Err(PublicError::StaleState);
        }
        let policy = crate::policy::optional(self.store().state(), &access.workspace)
            .map_err(unavailable)?
            .map_or(PolicyRevision::ZERO, |p| p.revision);
        if policy != facts.policy {
            return Err(PublicError::StaleState);
        }
        let file = change
            .files
            .get_mut(p.file as usize)
            .ok_or(PublicError::InvalidParameters)?;
        if file.state != FileState::Prepared || file.observation != facts.observation {
            return Err(PublicError::StaleState);
        }
        let mut effect: Effect = self
            .store()
            .state()
            .record(Collection::Effect, file.effect.as_str(), &access.workspace)
            .map_err(unavailable)?
            .decode()
            .map_err(unavailable)?;
        if effect.scope != task.scope
            || effect.steering != task.steering
            || effect.state != EffectState::Authorized
            || effect.operation_digest != file.operation_digest
        {
            return Err(PublicError::StaleState);
        }
        let before_effect = effect.revision;
        let execution = ExecutionId::new();
        effect = effect
            .transition(
                effect.revision,
                task.steering,
                EffectState::DispatchRecorded,
                EventId::new(),
                "one version-bound editor file dispatch committed".into(),
            )
            .map_err(invalid)?;
        effect.execution = Some(execution.clone());
        file.state = FileState::Dispatched;
        file.execution = Some(execution);
        let (buffers, expected) = buffer_update(
            self.store().state(),
            &task.scope,
            [(facts.observation.clone(), false)],
        )?;
        let mut updated_task = task.clone();
        updated_task.revision = task.revision.next().map_err(invalid)?;
        updated_task.fingerprint.buffers = vcp_protocol::digest_bytes(
            &vcp_protocol::canonical_bytes(&buffers.files).map_err(invalid)?,
        );
        updated_task.reason = "editor dispatch invalidated prior buffer verification".into();
        let prior = change.revision;
        change.revision = prior.next().map_err(invalid)?;
        let receipt = self
            .editor_commit(
                access,
                &call,
                &task.scope,
                facts.now,
                change.revision,
                vec![
                    (
                        record(
                            Collection::Projection,
                            &change.id,
                            &task.scope,
                            change.revision,
                            &change,
                        )?,
                        Some(prior),
                    ),
                    (
                        record(
                            Collection::Effect,
                            effect.id.as_str(),
                            &task.scope,
                            effect.revision,
                            &effect,
                        )?,
                        Some(before_effect),
                    ),
                    (
                        record(
                            Collection::Projection,
                            &buffers.id,
                            &task.scope,
                            buffers.revision,
                            &buffers,
                        )?,
                        expected,
                    ),
                    (
                        record(
                            Collection::Task,
                            task.scope.task.as_str(),
                            &task.scope,
                            updated_task.revision,
                            &updated_task,
                        )?,
                        Some(task.revision),
                    ),
                ],
                EventKind::EffectTransition,
            )
            .await?;
        Ok(EditorCommit {
            receipt,
            change,
            replay: false,
        })
    }
    pub async fn editor_result(
        &mut self,
        p: &wire::EditorChangeResult,
        access: &Access,
        connection: &ControllerId,
        token: &ControllerToken,
        facts: &EditorResultFacts,
    ) -> Result<EditorCommit> {
        let call = Call::EditorChangeResult(p.clone());
        let mut change = self.editor_read(
            access,
            &wire::EditorChangeRead {
                scope: p.scope.clone(),
                task: p.task.clone(),
                change: p.change.clone(),
            },
        )?;
        if let Some(receipt) = self.editor_replay(access, connection, token, &call)? {
            return Ok(EditorCommit {
                receipt,
                change,
                replay: true,
            });
        }
        let task = self.editor_task(access, &p.scope, &p.task)?;
        self.editor_revision(&task, &p.mutation, change.revision)?;
        if change.actor != access.actor || change.generation != p.generation.as_str() {
            return Err(PublicError::StaleState);
        }
        let file = change
            .files
            .get_mut(p.file as usize)
            .ok_or(PublicError::InvalidParameters)?;
        if !matches!(file.state, FileState::Dispatched | FileState::Unknown)
            || file.execution.as_ref().map(|e| e.as_str()) != Some(p.execution.as_str())
        {
            return Err(PublicError::StaleState);
        }
        check_observation(&facts.observation, &p.document)?;
        if facts.observation.root != change.root
            || facts.observation.path != file.observation.path
            || facts.observation.uri != file.observation.uri
        {
            return Err(PublicError::StaleState);
        }
        let applied = matches!(p.outcome, wire::EditorOutcome::Applied);
        if applied
            && (facts.observation.content_sha256 != file.after_sha256
                || facts.observation.host != file.observation.host
                || facts.observation.open_id != file.observation.open_id
                || facts.observation.version <= file.observation.version)
        {
            return Err(PublicError::StaleState);
        }
        let mut effect: Effect = self
            .store()
            .state()
            .record(Collection::Effect, file.effect.as_str(), &access.workspace)
            .map_err(unavailable)?
            .decode()
            .map_err(unavailable)?;
        if effect.scope != task.scope
            || effect.execution.as_ref().map(|e| e.as_str()) != Some(p.execution.as_str())
        {
            return Err(PublicError::StaleState);
        }
        // A crash before any response leaves dispatch intent, not a fabricated
        // running state. Persist uncertainty before resolving actual evidence.
        if effect.state == EffectState::DispatchRecorded {
            let prior = effect.revision;
            let event_id = EventId::new();
            effect = effect
                .transition(
                    prior,
                    effect.steering,
                    EffectState::OutcomeUnknown,
                    event_id.clone(),
                    "editor receipt reconciliation after recorded dispatch".into(),
                )
                .map_err(invalid)?;
            let row = record(
                Collection::Effect,
                effect.id.as_str(),
                &task.scope,
                effect.revision,
                &effect,
            )?;
            let watermark = self.store().state().watermark;
            let event = EventInput {
                id: event_id,
                workspace: task.scope.workspace.clone(),
                session: task.scope.session.clone(),
                task: Some(task.scope.task.clone()),
                actor: access.actor.clone(),
                correlation: CommandId::parse(p.mutation.command_id.as_str()).map_err(invalid)?,
                causation: None,
                timestamp: facts.now,
                kind: EventKind::EffectTransition,
                artifacts: vec![],
                data: serde_json::json!({"schema_version":1,"facts":[{"collection":row.collection,"id":row.id,"revision":row.revision,"value":row.value}]}),
                metadata: None,
            };
            self.store_mut()
                .transact(Transaction {
                    id: TransactionId::new(),
                    expected_watermark: watermark,
                    mutations: vec![Mutation::Put {
                        expected: Some(prior),
                        record: row,
                    }],
                    events: vec![event],
                    command: None,
                })
                .await
                .map_err(|_| PublicError::OutcomeUnknown)?;
        }
        let before_effect = effect.revision;
        let desired = match p.outcome {
            wire::EditorOutcome::Applied => EffectState::Succeeded,
            wire::EditorOutcome::Rejected => EffectState::Failed,
            wire::EditorOutcome::Unknown => EffectState::OutcomeUnknown,
        };
        if effect.state != desired {
            effect = effect
                .transition(
                    effect.revision,
                    effect.steering,
                    desired,
                    EventId::new(),
                    "version-bound editor receipt observed; disk and buffer states remain distinct"
                        .into(),
                )
                .map_err(invalid)?;
        }
        file.state = match p.outcome {
            wire::EditorOutcome::Applied => FileState::Applied,
            wire::EditorOutcome::Rejected => FileState::Rejected,
            wire::EditorOutcome::Unknown => FileState::Unknown,
        };
        file.observed = Some(facts.observation.clone());
        let (buffers, expected) = buffer_update(
            self.store().state(),
            &task.scope,
            [(
                facts.observation.clone(),
                facts.matched_disk && !matches!(p.outcome, wire::EditorOutcome::Unknown),
            )],
        )?;
        let prior = change.revision;
        change.revision = prior.next().map_err(invalid)?;
        let mut updated_task = task.clone();
        updated_task.revision = task.revision.next().map_err(invalid)?;
        updated_task.fingerprint.buffers = vcp_protocol::digest_bytes(
            &vcp_protocol::canonical_bytes(&buffers.files).map_err(invalid)?,
        );
        updated_task.reason =
            "editor receipt changed buffer fingerprint; disk-only checks are separate".into();
        let mut records = vec![
            (
                record(
                    Collection::Task,
                    task.scope.task.as_str(),
                    &task.scope,
                    updated_task.revision,
                    &updated_task,
                )?,
                Some(task.revision),
            ),
            (
                record(
                    Collection::Projection,
                    &change.id,
                    &task.scope,
                    change.revision,
                    &change,
                )?,
                Some(prior),
            ),
            (
                record(
                    Collection::Projection,
                    &buffers.id,
                    &task.scope,
                    buffers.revision,
                    &buffers,
                )?,
                expected,
            ),
        ];
        if before_effect != effect.revision {
            records.push((
                record(
                    Collection::Effect,
                    effect.id.as_str(),
                    &task.scope,
                    effect.revision,
                    &effect,
                )?,
                Some(before_effect),
            ));
        }
        let receipt = self
            .editor_commit(
                access,
                &call,
                &task.scope,
                facts.now,
                change.revision,
                records,
                EventKind::EffectTransition,
            )
            .await?;
        Ok(EditorCommit {
            receipt,
            change,
            replay: false,
        })
    }
}
fn check_observation(actual: &Observation, input: &wire::DocumentObservation) -> Result<()> {
    actual.validate().map_err(invalid)?;
    if actual.host != input.host.as_str()
        || actual.open_id != input.open_id.as_str()
        || actual.uri != input.uri
        || actual.path != input.relative_path
        || actual.version.get() != counter(&input.version)?
        || actual.content_sha256 != input.content_sha256
        || actual.dirty != input.dirty
    {
        return Err(PublicError::InvalidParameters);
    }
    Ok(())
}
pub fn change_view(change: &ChangeSet, buffers_unverified: bool) -> Result<wire::ChangeView> {
    Ok(wire::ChangeView {
        scope: methods::Scope {
            workspace: id(change.scope.workspace.as_str())?,
            session: id(change.scope.session.as_str())?,
        },
        task: id(change.scope.task.as_str())?,
        change: id(&change.id)?,
        revision: change.revision.get().into(),
        generation: id(&change.generation)?,
        root: id(change.root.as_str())?,
        binding_revision: change.binding.get().into(),
        authority_revision: change.authority.get().into(),
        policy_revision: change.policy.get().into(),
        steering_revision: change.steering.get().into(),
        files: change
            .files
            .iter()
            .enumerate()
            .map(|(index, file)| {
                Ok(wire::FileView {
                    file: index as u32,
                    observation: id(&file.observation.id)?,
                    effect: id(file.effect.as_str())?,
                    uri: file.observation.uri.clone(),
                    relative_path: file.observation.path.clone(),
                    host: id(&file.observation.host)?,
                    open_id: id(&file.observation.open_id)?,
                    version: file.observation.version.get().into(),
                    content_sha256: file.observation.content_sha256.clone(),
                    disk_sha256: file.observation.disk_sha256.clone(),
                    disk_fingerprint: file.observation.disk_fingerprint.clone(),
                    after_sha256: file.after_sha256.clone(),
                    edits_digest: file.edits_digest.clone(),
                    operation_digest: file.operation_digest.clone(),
                    state: match file.state {
                        FileState::Prepared => wire::FileState::Prepared,
                        FileState::Dispatched => wire::FileState::Dispatched,
                        FileState::Applied => wire::FileState::Applied,
                        FileState::Rejected => wire::FileState::Rejected,
                        FileState::Unknown => wire::FileState::Unknown,
                    },
                    execution: file
                        .execution
                        .as_ref()
                        .map(|e| id(e.as_str()))
                        .transpose()?,
                    observed_observation: file.observed.as_ref().map(|o| id(&o.id)).transpose()?,
                    observed_version: file.observed.as_ref().map(|o| o.version.get().into()),
                    observed_sha256: file.observed.as_ref().map(|o| o.content_sha256.clone()),
                    dirty: file.observed.as_ref().unwrap_or(&file.observation).dirty,
                })
            })
            .collect::<Result<Vec<_>>>()?,
        buffers_unverified,
    })
}

#[cfg(test)]
mod tests;
