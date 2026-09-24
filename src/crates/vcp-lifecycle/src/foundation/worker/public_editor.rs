// SPDX-License-Identifier: Apache-2.0
//! Connection-owned transient drafts and the native prepared editor broker.
use super::{public_connection::PublicConnection, *};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{effect::*, policy::*};
use vcp_engine::{controller::ControllerToken, editor as service};
use vcp_protocol::{
    editor as wire,
    jsonrpc::RpcError,
    methods::{self, Call, ResultValue},
};

mod native;
mod text;

#[derive(Default)]
pub(super) struct EditorState {
    observations: BTreeMap<String, CachedObservation>,
    changes: BTreeMap<String, CachedChange>,
    generation: Option<methods::Id>,
    replies: BTreeMap<String, (String, ResultValue)>,
}
impl EditorState {
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }
}
#[derive(Clone)]
struct CachedObservation {
    view: wire::Observation,
    scope: Scope,
    version: vcp_repository::FileVersion,
    content: Option<String>,
    matched_disk: bool,
}
#[derive(Clone)]
struct CachedFile {
    observation: CachedObservation,
    prepared: vcp_policy::Prepared,
}
#[derive(Clone)]
struct CachedChange {
    generation: methods::Id,
    files: Vec<CachedFile>,
}

fn id(value: impl ToString) -> Result<methods::Id> {
    value
        .to_string()
        .try_into()
        .map_err(|_| "editor identity is invalid".into())
}

fn same_document(
    left: &methods::DocumentObservation,
    right: &methods::DocumentObservation,
) -> bool {
    left.host == right.host
        && left.open_id == right.open_id
        && left.uri == right.uri
        && left.relative_path == right.relative_path
        && left.version == right.version
        && left.content_sha256 == right.content_sha256
        && left.dirty == right.dirty
        && left.eol == right.eol
        && left.encoding == right.encoding
}

impl CachedObservation {
    fn canonical(&self) -> Result<vcp_domain::editor::Observation> {
        let document = &self.view.document;
        Ok(vcp_domain::editor::Observation {
            id: self.view.id.as_str().to_owned(),
            host: document.host.as_str().to_owned(),
            open_id: document.open_id.as_str().to_owned(),
            uri: document.uri.clone(),
            path: document.relative_path.clone(),
            version: Revision::new(document.version.as_str().parse()?),
            content_sha256: document.content_sha256.clone(),
            dirty: document.dirty,
            root: self.version.root.clone(),
            disk_sha256: self.version.sha256.clone(),
            disk_fingerprint: self.view.disk_fingerprint.clone(),
            eol: serde_json::to_value(&document.eol)?
                .as_str()
                .ok_or("editor EOL")?
                .into(),
            encoding: serde_json::to_value(&document.encoding)?
                .as_str()
                .ok_or("editor encoding")?
                .into(),
        })
    }
}

impl Context {
    pub(super) fn editor_verification_buffers(
        &self,
        binding: &ThreadBinding,
    ) -> Result<(String, bool)> {
        let state = self.engine.store().state();
        let (digest, mut unverified) = service::buffer_status(state, &binding.scope)?;
        // Like unresolved effects, a sibling's uncertain edits cannot silently
        // disappear from the enclosing workspace's completion fence.
        for record in state.records.values().filter(|record| {
            record.collection == Collection::Projection
                && record.workspace == binding.scope.workspace
        }) {
            if record
                .value
                .get("document_type")
                .and_then(serde_json::Value::as_str)
                == Some(vcp_domain::editor::BUFFERS)
            {
                let buffers: vcp_domain::editor::BufferState = record.decode()?;
                buffers.validate()?;
                unverified |= buffers.unverified();
            }
        }
        Ok((digest, unverified))
    }

    fn editor_observation(
        &mut self,
        scope: &Scope,
        document: &methods::DocumentObservation,
    ) -> Result<CachedObservation> {
        wire::validate_document(document)?;
        let workspace: Workspace = self
            .engine
            .store()
            .state()
            .record(
                Collection::Workspace,
                scope.workspace.as_str(),
                &scope.workspace,
            )?
            .decode()?;
        if workspace.trust != Trust::Trusted || workspace.binding != self.config.binding {
            return Err(
                "trusted current workspace binding required for editor observations".into(),
            );
        }
        let root = self.task_root(&scope.task)?;
        self.tool_read_access(&root.identity.root, "vcp_editor_edit")?;
        let source = native::observe(&root, document)?;
        let mut metadata = document.clone();
        let disk_text = native::logical_disk(&source.bytes, &document.encoding);
        let matched_disk = disk_text.as_ref().is_ok_and(|text| {
            vcp_protocol::digest_bytes(text.as_bytes()) == document.content_sha256
        });
        let content = document.content.clone().or_else(|| {
            disk_text
                .as_ref()
                .ok()
                .filter(|text| matched_disk && text.len() <= 65536)
                .cloned()
        });
        if !document.dirty && disk_text.is_ok() && !matched_disk {
            return Err("clean editor content differs from native disk".into());
        }
        metadata.content = None;
        metadata.capture = false;
        metadata.disk_sha256 = Some(source.version.sha256.clone());
        let artifact = if document.capture {
            Some(id(self
                .capture(
                    scope,
                    Channel::Evidence,
                    content
                        .as_ref()
                        .ok_or("explicit capture requires supplied content")?
                        .as_bytes(),
                    "editor-explicit-capture/1",
                )?
                .spec
                .id)?)
        } else {
            None
        };
        let view = wire::Observation {
            id: id(ArtifactId::new())?,
            document: metadata,
            root: id(&root.identity.root)?,
            disk_fingerprint: vcp_protocol::digest_bytes(&canonical_bytes(&source.version)?),
            artifact,
        };
        Ok(CachedObservation {
            view,
            scope: scope.clone(),
            version: source.version,
            content,
            matched_disk,
        })
    }

    fn editor_operation(
        &self,
        binding: &ThreadBinding,
        observation: &CachedObservation,
        edits: &[wire::TextEdit],
        after_hash: &str,
    ) -> Result<vcp_policy::Prepared> {
        let identity = self.tool_identity(binding, "vcp_editor_edit")?;
        let observed = observation.canonical()?;
        let arguments = canonical_bytes(&serde_json::json!({
            "observation": observed, "after_sha256":after_hash,
            "edits_digest":vcp_protocol::digest_bytes(&canonical_bytes(&edits)?),
            "mode":"unsaved_buffer"
        }))?;
        vcp_policy::Prepared::new(Operation {
            scope: binding.scope.clone(),
            actor: identity.actor,
            host: identity.host,
            binding: identity.binding,
            authority: identity.authority,
            steering: identity.steering,
            policy: identity.policy,
            tool: "vcp_editor_edit".into(),
            schema: vcp_protocol::digest_bytes(&canonical_bytes(&serde_json::json!({
                "name":"editor-edit/1",
                "arguments":["observation", "after_sha256", "edits_digest", "mode"],
                "representation":"unsaved_buffer"
            }))?),
            arguments: String::from_utf8(arguments)?,
            invocation: Invocation::Local,
            resources: vec![Resource {
                root: observation.version.root.clone(),
                path: observation.version.path.clone(),
                write: true,
                version: vcp_protocol::digest_bytes(&canonical_bytes(&observed)?),
            }],
            effects: BTreeSet::from([EffectClass::Write]),
            required_isolation: BTreeSet::from([
                Isolation::PathContainment,
                Isolation::OutputLimit,
            ]),
            timeout_ms: Units::new(30_000),
            output_bytes: ByteCount::new(65_536),
        })
        .map_err(Into::into)
    }
}

impl PublicConnection {
    pub(super) fn editor_call(
        &mut self,
        call: Call,
        current: &Access,
    ) -> std::result::Result<ResultValue, RpcError> {
        call.validate().map_err(|_| RpcError::invalid_params())?;
        let failure = || {
            vcp_protocol::errors::ApplicationError {
            code: vcp_protocol::errors::Code::VersionConflict,
            retry: vcp_protocol::errors::Retry::AfterRevalidation,
            operation: call.command_id().cloned(),
            explanation: "editor operation requires current connection, document, task and authority observations".into(),
            reconciliation: None,
        }.into_rpc()
        };
        let public_error =
            |error| vcp_engine::rpc::public_error(error, call.command_id().cloned(), false);
        let (host, access, connection, token) = self
            .rpc_context(current)
            .map_err(|_| public_error(vcp_engine::public::PublicError::Access))?;
        let state = self.editor_state.clone();
        let connected = self.connected.clone();
        let bindings = host.bindings.clone();
        let request = call.clone();
        host.worker
            .run(move |context| {
                Ok((|| -> Result<ResultValue> {
                    if !connected.load(Ordering::SeqCst) {
                        return Err("editor connection closed".into());
                    }
                    context
                        .public_authorize(
                            &access,
                            &connection,
                            token.as_ref(),
                            request.is_mutation(),
                        )
                        .map_err(|_| vcp_engine::public::PublicError::Access)?;
                    let mut editor = state.lock().map_err(|_| "editor cache unavailable")?;
                    if let Call::EditorChangeRead(read) = &request {
                        let change = context.engine.editor_read(&access, read)?;
                        let (_, unverified) =
                            service::buffer_status(context.engine.store().state(), &change.scope)?;
                        return Ok(ResultValue::EditorChange(service::change_view(
                            &change, unverified,
                        )?));
                    }
                    let token = token
                        .as_ref()
                        .ok_or("editor changes require controller ownership")?;
                    let command = request
                        .command_id()
                        .ok_or("editor mutation identity missing")?
                        .as_str()
                        .to_owned();
                    let digest = vcp_protocol::digest_bytes(&canonical_bytes(&request)?);
                    if context
                        .engine
                        .editor_replay(&access, &connection, token, &request)?
                        .is_some()
                    {
                        if let Some((original, reply)) = editor
                            .replies
                            .get(&command)
                            .filter(|_| matches!(request, Call::EditorContext(_)))
                        {
                            if original != &digest {
                                return Err("editor command identity conflicts".into());
                            }
                            let mut reply = reply.clone();
                            if let ResultValue::EditorDispatch(value) = &mut reply {
                                value.apply = false;
                            }
                            return Ok(reply);
                        }
                        let read =
                            match &request {
                                Call::EditorPrepare(p) => wire::EditorChangeRead {
                                    scope: p.scope.clone(),
                                    task: p.task.clone(),
                                    change: id(service::change_id(p)?)?,
                                },
                                Call::EditorDispatch(p) => wire::EditorChangeRead {
                                    scope: p.scope.clone(),
                                    task: p.task.clone(),
                                    change: p.change.clone(),
                                },
                                Call::EditorChangeResult(p) => wire::EditorChangeRead {
                                    scope: p.scope.clone(),
                                    task: p.task.clone(),
                                    change: p.change.clone(),
                                },
                                _ => return Err(
                                    "editor context expired; register a fresh observation command"
                                        .into(),
                                ),
                            };
                        let change = context.engine.editor_read(&access, &read)?;
                        let (_, unverified) =
                            service::buffer_status(context.engine.store().state(), &change.scope)?;
                        let view = service::change_view(&change, unverified)?;
                        if let Call::EditorDispatch(p) = &request {
                            let execution = change
                                .files
                                .get(p.file as usize)
                                .and_then(|file| file.execution.as_ref())
                                .ok_or("editor committed dispatch identity unavailable")?;
                            return Ok(ResultValue::EditorDispatch(wire::DispatchView {
                                change: view,
                                file: p.file,
                                execution: id(execution)?,
                                apply: false,
                            }));
                        }
                        return Ok(ResultValue::EditorChange(view));
                    }
                    let (scope, task) = match &request {
                        Call::EditorContext(p) => (&p.scope, &p.task),
                        Call::EditorPrepare(p) => (&p.scope, &p.task),
                        Call::EditorDispatch(p) => (&p.scope, &p.task),
                        Call::EditorChangeResult(p) => (&p.scope, &p.task),
                        _ => return Err("unsupported editor method".into()),
                    };
                    let task = context.engine.editor_task(&access, scope, task)?;
                    let binding = bindings
                        .lock()
                        .map_err(|_| "editor host bindings unavailable")?
                        .values()
                        .find(|binding| binding.scope == task.scope)
                        .cloned();
                    let reply = context.editor_mutation(
                        &request,
                        &access,
                        &connection,
                        token,
                        &mut editor,
                        &task,
                        binding.as_ref(),
                    )?;
                    if editor.replies.len() >= 64 {
                        if let Some(key) = editor.replies.keys().next().cloned() {
                            editor.replies.remove(&key);
                        }
                    }
                    editor.replies.insert(command, (digest, reply.clone()));
                    Ok(reply)
                })())
            })
            .map_err(|_| public_error(vcp_engine::public::PublicError::OutcomeUnknown))?
            .map_err(|error| {
                error
                    .downcast_ref::<vcp_engine::public::PublicError>()
                    .map(|error| public_error(*error))
                    .unwrap_or_else(failure)
            })
    }
}

impl Context {
    fn editor_mutation(
        &mut self,
        call: &Call,
        access: &Access,
        connection: &ControllerId,
        token: &ControllerToken,
        editor: &mut EditorState,
        task: &Task,
        binding: Option<&ThreadBinding>,
    ) -> Result<ResultValue> {
        // Reject stale client command preconditions before creating proposal
        // artifacts, effects, or explicit draft captures. Engine commit repeats
        // this check against the serialized current state.
        let (mutation, expected) = match call {
            Call::EditorContext(p) => (&p.mutation, task.revision),
            Call::EditorPrepare(p) => (&p.mutation, task.revision),
            Call::EditorDispatch(p) => (
                &p.mutation,
                self.engine
                    .editor_read(
                        access,
                        &wire::EditorChangeRead {
                            scope: p.scope.clone(),
                            task: p.task.clone(),
                            change: p.change.clone(),
                        },
                    )?
                    .revision,
            ),
            Call::EditorChangeResult(p) => (
                &p.mutation,
                self.engine
                    .editor_read(
                        access,
                        &wire::EditorChangeRead {
                            scope: p.scope.clone(),
                            task: p.task.clone(),
                            change: p.change.clone(),
                        },
                    )?
                    .revision,
            ),
            _ => return Err("unsupported editor mutation".into()),
        };
        if mutation.expected_revision.as_str().parse::<u64>()? != expected.get()
            || mutation.steering_revision.as_str().parse::<u64>()? != task.steering.get()
        {
            return Err(vcp_engine::public::PublicError::StaleState.into());
        }
        match call {
            Call::EditorContext(request) => {
                let mut observations = Vec::new();
                let mut paths = BTreeSet::new();
                let mut closed = Vec::new();
                for retired in &request.closed {
                    let previous = editor
                        .observations
                        .get(retired.as_str())
                        .filter(|value| value.scope == task.scope)
                        .ok_or("editor close observation is no longer current")?;
                    if !paths.insert(previous.version.path.clone()) {
                        return Err("duplicate editor close resource".into());
                    }
                    // Closing is an authenticated editor attestation, not proof
                    // inferred from dirty=false. Independently re-observe the
                    // current disk without requiring the obsolete disk hash.
                    let mut document = previous.view.document.clone();
                    document.content = None;
                    document.disk_sha256 = None;
                    document.capture = false;
                    document.dirty = true;
                    self.editor_observation(&task.scope, &document)?;
                    closed.push(retired.as_str().to_owned());
                }
                for document in &request.documents {
                    if !paths.insert(document.relative_path.clone()) {
                        return Err("duplicate editor resource".into());
                    }
                    let mut observed = self.editor_observation(&task.scope, document)?;
                    if let Some(previous) = editor.observations.values().find(|old| {
                        old.scope == task.scope
                            && same_document(&old.view.document, &observed.view.document)
                            && old.version == observed.version
                    }) {
                        observed.view.id = previous.view.id.clone();
                    }
                    observations.push(observed);
                }
                let mut next = editor.observations.clone();
                for retired in &closed {
                    next.remove(retired);
                }
                for observation in &observations {
                    next.retain(|_, old| {
                        old.scope != task.scope || old.version.path != observation.version.path
                    });
                    next.insert(observation.view.id.as_str().to_owned(), observation.clone());
                }
                if next.len() > 16 {
                    return Err("editor tracked document limit".into());
                }
                let facts = service::EditorObserveFacts {
                    observations: observations
                        .iter()
                        .map(|value| Ok((value.canonical()?, value.matched_disk)))
                        .collect::<Result<Vec<_>>>()?,
                    closed,
                    now: now(),
                };
                self.check_public_controller(access, connection, token)?;
                self.runtime.block_on(
                    self.engine
                        .editor_observe(request, access, connection, token, &facts),
                )?;
                let current = self
                    .engine
                    .editor_task(access, &request.scope, &request.task)?;
                let generation = match &editor.generation {
                    Some(value) => value.clone(),
                    None => id(ControllerId::new())?,
                };
                editor.generation = Some(generation.clone());
                editor.observations = next;
                Ok(ResultValue::EditorContext(wire::ContextView {
                    generation,
                    revision: current.revision.get().into(),
                    observations: observations.into_iter().map(|value| value.view).collect(),
                }))
            }
            Call::EditorPrepare(request) => {
                let binding = binding.ok_or("editor preparation needs live task binding")?;
                self.can_start(binding)?;
                if editor.generation.as_ref() != Some(&request.generation) {
                    return Err("editor generation changed".into());
                }
                let mut files = Vec::new();
                let mut prepared_files = Vec::new();
                let mut decisions = Vec::new();
                for file in &request.files {
                    let observation = editor
                        .observations
                        .get(file.observation.as_str())
                        .filter(|value| value.scope == task.scope)
                        .ok_or("editor observation expired")?
                        .clone();
                    let root = self.task_root(&task.scope.task)?;
                    root.revalidate(&observation.version)?;
                    let content = observation
                        .content
                        .as_ref()
                        .ok_or("editor preparation requires current captured transient content")?;
                    let after =
                        text::replace(content, &file.edits, &observation.view.document.eol)?;
                    let after_hash = vcp_protocol::digest_bytes(after.as_bytes());
                    let operation =
                        self.editor_operation(binding, &observation, &file.edits, &after_hash)?;
                    let decision = self.authority_decision(
                        binding,
                        &operation,
                        &BTreeSet::from([root.identity.root]),
                        true,
                        &BTreeSet::from([Isolation::PathContainment, Isolation::OutputLimit]),
                    )?;
                    if matches!(decision, vcp_policy::Decision::Deny { .. }) {
                        return Err("editor policy denied preparation".into());
                    }
                    prepared_files.push((
                        CachedFile {
                            observation,
                            prepared: operation,
                        },
                        after_hash,
                        vcp_protocol::digest_bytes(&canonical_bytes(&file.edits)?),
                    ));
                    decisions.push(decision);
                }
                for (index, ((file, after_hash, edits_digest), decision)) in
                    prepared_files.iter().zip(&decisions).enumerate()
                {
                    let evidence = canonical_bytes(
                        &serde_json::json!({"operation":file.prepared.operation()}),
                    )?;
                    let effect = ToolRunId::parse(format!(
                        "editor-effect-{}",
                        vcp_protocol::digest_bytes(&canonical_bytes(&(
                            &request.scope,
                            &request.task,
                            &request.mutation.command_id,
                            index
                        ))?)
                    ))?;
                    let (effect, _, _, _) = self.propose_editor_authority(
                        binding,
                        &file.prepared,
                        &evidence,
                        decision.clone(),
                        effect,
                    )?;
                    files.push(vcp_domain::editor::File {
                        effect,
                        observation: file.observation.canonical()?,
                        operation_digest: file.prepared.digest().into(),
                        after_sha256: after_hash.clone(),
                        edits_digest: edits_digest.clone(),
                        state: vcp_domain::editor::FileState::Prepared,
                        execution: None,
                        observed: None,
                    });
                }
                let first = prepared_files
                    .first()
                    .ok_or("empty editor proposal")?
                    .0
                    .prepared
                    .operation();
                let facts = service::EditorPrepareFacts {
                    generation: request.generation.as_str().to_owned(),
                    root: prepared_files[0].0.observation.version.root.clone(),
                    host: first.host.clone(),
                    binding: first.binding,
                    policy: first.policy,
                    files,
                    now: now(),
                };
                self.check_public_controller(access, connection, token)?;
                let committed = self.runtime.block_on(
                    self.engine
                        .editor_prepare(request, access, connection, token, &facts),
                )?;
                let change = committed.change;
                for ((file, _, _), (decision, persisted)) in prepared_files
                    .iter()
                    .zip(decisions.iter().zip(&change.files))
                {
                    if matches!(decision, vcp_policy::Decision::Question { .. }) {
                        self.ask_editor_authority(binding, &file.prepared, &persisted.effect)?;
                    }
                }
                if editor.changes.len() >= 4 {
                    if let Some(key) = editor.changes.keys().next().cloned() {
                        editor.changes.remove(&key);
                    }
                }
                editor.changes.insert(
                    change.id.clone(),
                    CachedChange {
                        generation: request.generation.clone(),
                        files: prepared_files
                            .into_iter()
                            .map(|(file, _, _)| file)
                            .collect(),
                    },
                );
                let (_, unverified) =
                    service::buffer_status(self.engine.store().state(), &change.scope)?;
                Ok(ResultValue::EditorChange(service::change_view(
                    &change, unverified,
                )?))
            }
            Call::EditorDispatch(request) => {
                let binding = binding.ok_or("editor dispatch needs live task binding")?;
                self.can_start(binding)?;
                let cached = editor
                    .changes
                    .get(request.change.as_str())
                    .ok_or("editor proposal expired")?;
                if cached.generation != request.generation
                    || editor.generation.as_ref() != Some(&request.generation)
                {
                    return Err("editor generation changed".into());
                }
                let file = cached
                    .files
                    .get(request.file as usize)
                    .ok_or("editor file missing")?;
                let current = editor
                    .observations
                    .get(file.observation.view.id.as_str())
                    .filter(|value| {
                        value.scope == task.scope && value.version == file.observation.version
                    })
                    .ok_or("editor observation changed")?;
                let root = self.task_root(&task.scope.task)?;
                root.revalidate(&file.observation.version)?;
                let decision = self.authority_decision(
                    binding,
                    &file.prepared,
                    &BTreeSet::from([root.identity.root]),
                    true,
                    &BTreeSet::from([Isolation::PathContainment, Isolation::OutputLimit]),
                )?;
                if !matches!(decision, vcp_policy::Decision::Allow { .. }) {
                    return Err("editor dispatch needs current policy approval".into());
                }
                let read = wire::EditorChangeRead {
                    scope: request.scope.clone(),
                    task: request.task.clone(),
                    change: request.change.clone(),
                };
                let change = self.engine.editor_read(access, &read)?;
                let selected = change
                    .files
                    .get(request.file as usize)
                    .ok_or("editor file missing")?;
                if selected.state != vcp_domain::editor::FileState::Prepared {
                    return Err("editor dispatch already recorded; reconcile".into());
                }
                let effect: Effect = self
                    .engine
                    .store()
                    .state()
                    .record(
                        Collection::Effect,
                        selected.effect.as_str(),
                        &task.scope.workspace,
                    )?
                    .decode()?;
                self.validate_skill_plan(binding, &effect.observed_changes)?;
                if effect.state == EffectState::Validated {
                    self.tool_advance(
                        binding,
                        &selected.effect,
                        EffectState::Authorized,
                        None,
                        effect.observed_changes.clone(),
                        "current editor operation authority revalidated",
                    )?;
                } else if effect.state != EffectState::Authorized {
                    return Err("editor effect is no longer dispatchable".into());
                }
                let facts = service::EditorDispatchFacts {
                    generation: request.generation.as_str().to_owned(),
                    observation: current.canonical()?,
                    policy: file.prepared.operation().policy,
                    now: now(),
                };
                self.check_public_controller(access, connection, token)?;
                let committed = self.runtime.block_on(
                    self.engine
                        .editor_dispatch(request, access, connection, token, &facts),
                )?;
                let (_, unverified) =
                    service::buffer_status(self.engine.store().state(), &committed.change.scope)?;
                let execution = committed.change.files[request.file as usize]
                    .execution
                    .as_ref()
                    .ok_or("editor dispatch identity missing")?;
                Ok(ResultValue::EditorDispatch(wire::DispatchView {
                    execution: id(execution)?,
                    file: request.file,
                    apply: !committed.replay,
                    change: service::change_view(&committed.change, unverified)?,
                }))
            }
            Call::EditorChangeResult(request) => {
                let observed = self.editor_observation(&task.scope, &request.document);
                let (canonical, matched_disk, cached) = match observed {
                    Ok(mut observed) => {
                        if let Some(previous) = editor.observations.values().find(|old| {
                            old.scope == task.scope
                                && same_document(&old.view.document, &observed.view.document)
                                && old.version == observed.version
                        }) {
                            observed.view.id = previous.view.id.clone();
                        }
                        (observed.canonical()?, observed.matched_disk, Some(observed))
                    }
                    Err(_)
                        if matches!(request.outcome, wire::EditorOutcome::Unknown)
                            && !request.document.capture =>
                    {
                        // A deleted/renamed/closed document cannot yield a fresh
                        // native proof. Record uncertainty against the original
                        // dispatch, never reinterpret it as a successful apply.
                        let change = self.engine.editor_read(
                            access,
                            &wire::EditorChangeRead {
                                scope: request.scope.clone(),
                                task: request.task.clone(),
                                change: request.change.clone(),
                            },
                        )?;
                        let original = &change
                            .files
                            .get(request.file as usize)
                            .ok_or("editor file missing")?
                            .observation;
                        let document = &request.document;
                        if document.uri != original.uri || document.relative_path != original.path {
                            return Err(
                                "unknown editor receipt resource differs from dispatch".into()
                            );
                        }
                        let mut canonical = original.clone();
                        canonical.id = ArtifactId::new().to_string();
                        canonical.host = document.host.as_str().to_owned();
                        canonical.open_id = document.open_id.as_str().to_owned();
                        canonical.version = Revision::new(document.version.as_str().parse()?);
                        canonical.content_sha256 = document.content_sha256.clone();
                        canonical.dirty = document.dirty;
                        (canonical, false, None)
                    }
                    Err(error) => return Err(error),
                };
                let facts = service::EditorResultFacts {
                    observation: canonical,
                    matched_disk,
                    now: now(),
                };
                self.check_public_controller(access, connection, token)?;
                let committed = self.runtime.block_on(
                    self.engine
                        .editor_result(request, access, connection, token, &facts),
                )?;
                editor.observations.retain(|_, old| {
                    old.scope != task.scope || old.version.path != request.document.relative_path
                });
                if let Some(observed) = cached {
                    editor
                        .observations
                        .insert(observed.view.id.as_str().to_owned(), observed);
                }
                let (_, unverified) =
                    service::buffer_status(self.engine.store().state(), &committed.change.scope)?;
                Ok(ResultValue::EditorChange(service::change_view(
                    &committed.change,
                    unverified,
                )?))
            }
            _ => Err("unsupported editor mutation".into()),
        }
    }
}
