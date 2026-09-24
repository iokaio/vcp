// SPDX-License-Identifier: Apache-2.0
//! Authorized presentation summaries, never raw operations or internal records.
use crate::{query::QueryError, Access, Engine};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vcp_domain::{
    artifact::{ArtifactDescriptor, CaptureState, Channel},
    effect::Effect,
    ids::*,
    retention::RetentionMask,
    revision::*,
    workspace::Workspace,
};
use vcp_protocol::{
    command::Approval,
    methods::{self, Call},
};
use vcp_store::contract::{CanonicalStore, Collection, State};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    scope: methods::Scope,
    actor: ActorId,
    authority: AuthorityRevision,
    deletion: DeletionEpoch,
    watermark: Watermark,
    task: TaskId,
    after: String,
}
fn id(value: &str) -> Result<methods::Id, QueryError> {
    value
        .to_owned()
        .try_into()
        .map_err(|_| QueryError::InvalidData)
}
fn text(value: &str) -> methods::PresentationText {
    let mut chars = value
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t');
    let text = chars.by_ref().take(4096).collect();
    methods::PresentationText {
        text,
        truncated: chars.next().is_some(),
    }
}
fn criteria(values: &[String]) -> Option<Vec<String>> {
    (values.len() <= 64
        && values
            .iter()
            .all(|value| value.chars().count() <= 4096 && !value.contains('\0'))
        && values.iter().map(|v| v.len()).sum::<usize>() <= 16384)
        .then(|| values.to_vec())
}
fn reference(
    state: &State,
    scope: &vcp_domain::workspace::Scope,
    artifact: &ArtifactId,
) -> Result<Option<methods::EvidenceReference>, QueryError> {
    let Some(row) = state.records.values().find(|r| {
        r.collection == Collection::Artifact
            && r.workspace == scope.workspace
            && r.id == artifact.as_str()
    }) else {
        return Ok(None);
    };
    let descriptor: ArtifactDescriptor = row.decode().map_err(|_| QueryError::InvalidData)?;
    descriptor.validate().map_err(|_| QueryError::InvalidData)?;
    if descriptor.spec.id != *artifact || descriptor.spec.scope != *scope {
        return Err(QueryError::InvalidData);
    }
    if descriptor.state == CaptureState::Purged {
        return Ok(None);
    }
    for row in state
        .records
        .values()
        .filter(|r| r.collection == Collection::Tombstone && r.workspace == scope.workspace)
    {
        let mask: RetentionMask = row.decode().map_err(|_| QueryError::InvalidData)?;
        mask.validate().map_err(|_| QueryError::InvalidData)?;
        if mask.workspace != scope.workspace {
            return Err(QueryError::InvalidData);
        }
        if mask.artifacts.contains(artifact) {
            return Ok(None);
        }
    }
    Ok(Some(methods::EvidenceReference {
        artifact: id(artifact.as_str())?,
        offset: 0.into(),
        length: descriptor.length.get().into(),
        sha256: descriptor.sha256,
    }))
}
impl<S: CanonicalStore> Engine<S> {
    pub fn public_presentation(
        &self,
        access: &Access,
        request: &methods::Inspect,
        now: Timestamp,
    ) -> Result<methods::TaskPresentation, QueryError> {
        let task = self.read_task(access, &request.scope, &request.task)?;
        Call::TaskPresentation(request.clone())
            .validate()
            .map_err(|_| QueryError::Limit)?;
        if request.target.is_some() || task.redaction.is_some() {
            return Err(QueryError::Unavailable);
        }
        let state = self.store().state();
        let workspace: Workspace = state
            .record(
                Collection::Workspace,
                access.workspace.as_str(),
                &access.workspace,
            )
            .map_err(|_| QueryError::Unavailable)?
            .decode()
            .map_err(|_| QueryError::InvalidData)?;
        let cursor: Option<Cursor> = request
            .cursor
            .as_ref()
            .map(|v| serde_json::from_str(v).map_err(|_| QueryError::StaleCursor))
            .transpose()?;
        if let Some(c) = &cursor {
            if c.scope != request.scope
                || c.actor != access.actor
                || c.authority != access.authority
                || c.deletion != workspace.deletion
                || c.watermark != state.watermark
                || c.task != task.scope.task
            {
                return Err(QueryError::StaleCursor);
            }
        }
        let view =
            crate::rpc::task_view(state, task.clone()).map_err(|_| QueryError::InvalidData)?;
        let graph = crate::agents::graph(state, &task.scope, &task.root)
            .map_err(|_| QueryError::InvalidData)?;
        let assignment = graph
            .as_ref()
            .and_then(|g| g.children.get(&task.scope.task));
        let mut questions = Vec::new();
        for input in &view.pending_inputs {
            let approval: Approval = state
                .record(Collection::Approval, input.id.as_str(), &access.workspace)
                .map_err(|_| QueryError::InvalidData)?
                .decode()
                .map_err(|_| QueryError::InvalidData)?;
            let effect: Effect = state
                .record(
                    Collection::Effect,
                    approval.effect.as_str(),
                    &access.workspace,
                )
                .map_err(|_| QueryError::InvalidData)?
                .decode()
                .map_err(|_| QueryError::InvalidData)?;
            if effect.scope != task.scope || effect.id != approval.effect {
                return Err(QueryError::InvalidData);
            }
            questions.push(methods::PresentationQuestion {
                input: input.clone(),
                effect: id(approval.effect.as_str())?,
                expires_at: approval.expires_at.get().into(),
                actionable: effect.redaction.is_none()
                    && crate::questions::actionable(state, &approval, now)
                        .map_err(|_| QueryError::InvalidData)?,
                summary: text(if effect.redaction.is_some() {
                    "Operation content unavailable."
                } else {
                    &effect.reason
                }),
            });
        }
        let mut rows = BTreeMap::new();
        let mut after_found = cursor.is_none();
        let mut evidence_complete = true;
        for record in state
            .records
            .values()
            .filter(|r| r.workspace == access.workspace)
        {
            let (key, row) = match record.collection {
                Collection::Effect => {
                    let effect: Effect = record.decode().map_err(|_| QueryError::InvalidData)?;
                    if effect.scope != task.scope {
                        continue;
                    }
                    if effect.id.as_str() != record.id || effect.revision != record.revision {
                        return Err(QueryError::InvalidData);
                    }
                    if effect.redaction.is_some() {
                        evidence_complete = false;
                        continue;
                    }
                    let mut evidence = Vec::new();
                    for artifact in effect.observed_changes.iter().take(16) {
                        match reference(state, &task.scope, artifact)? {
                            Some(r) => evidence.push(r),
                            None => evidence_complete = false,
                        }
                    }
                    if effect.observed_changes.len() > 16 {
                        evidence_complete = false
                    }
                    (
                        format!("effect:{}", record.id),
                        methods::PresentationRow::Effect {
                            id: id(&record.id)?,
                            task: request.task.clone(),
                            state: serde_json::to_value(effect.state)
                                .map_err(|_| QueryError::InvalidData)?
                                .as_str()
                                .ok_or(QueryError::InvalidData)?
                                .to_owned(),
                            reason: text(&effect.reason),
                            evidence,
                        },
                    )
                }
                Collection::Artifact => {
                    let artifact: ArtifactDescriptor =
                        record.decode().map_err(|_| QueryError::InvalidData)?;
                    if artifact.spec.scope != task.scope
                        || !matches!(
                            artifact.spec.channel,
                            Channel::Evidence | Channel::ChildTranscript
                        )
                    {
                        continue;
                    }
                    if artifact.spec.id.as_str() != record.id {
                        return Err(QueryError::InvalidData);
                    }
                    let Some(reference) = reference(state, &task.scope, &artifact.spec.id)? else {
                        evidence_complete = false;
                        continue;
                    };
                    if artifact.state != CaptureState::Complete
                        || artifact.spec.omissions.iter().any(|o| {
                            !matches!(
                                o,
                                vcp_domain::artifact::Omission::AuthenticationHeaders
                                    | vcp_domain::artifact::Omission::RecoveryMaterial
                            )
                        })
                    {
                        evidence_complete = false
                    }
                    (
                        format!("evidence:{}", record.id),
                        methods::PresentationRow::Evidence {
                            id: id(&record.id)?,
                            task: request.task.clone(),
                            schema: artifact.spec.schema,
                            evidence: vec![reference],
                        },
                    )
                }
                _ => continue,
            };
            if let Some(c) = &cursor {
                if key == c.after {
                    after_found = true
                }
                if key <= c.after {
                    continue;
                }
            }
            rows.insert(key, row);
            if rows.len() > request.limit as usize + 1 {
                rows.pop_last();
            }
        }
        if !after_found {
            return Err(QueryError::StaleCursor);
        }
        let more = rows.len() > request.limit as usize;
        if more {
            rows.pop_last();
        }
        let next_cursor = if more {
            Some(
                serde_json::to_string(&Cursor {
                    scope: request.scope.clone(),
                    actor: access.actor.clone(),
                    authority: access.authority,
                    deletion: workspace.deletion,
                    watermark: state.watermark,
                    task: task.scope.task.clone(),
                    after: rows
                        .last_key_value()
                        .ok_or(QueryError::InvalidData)?
                        .0
                        .clone(),
                })
                .map_err(|_| QueryError::InvalidData)?,
            )
        } else {
            None
        };
        let result = methods::TaskPresentation {
            task: view,
            watermark: state.watermark.get().into(),
            objective: task.objectives.last().map(|o| text(&o.text)),
            objective_constraints: task
                .objectives
                .last()
                .and_then(|o| criteria(&o.constraints)),
            objective_acceptance: task.objectives.last().and_then(|o| criteria(&o.acceptance)),
            model: methods::PresentationModel {
                id: None,
                group: None,
                source: methods::PresentationSource::Unavailable,
            },
            role: assignment.map(|a| text(&a.role)),
            model_policy: assignment.map(|a| text(&a.model_policy)),
            commentary: methods::PresentationSource::Unavailable,
            questions,
            rows: rows.into_values().collect(),
            next_cursor,
            complete: !more && evidence_complete,
        };
        if serde_json::to_vec(&result)
            .map_err(|_| QueryError::InvalidData)?
            .len()
            > crate::query::MAX_RESULT_BYTES
        {
            return Err(QueryError::Limit);
        }
        Ok(result)
    }
}

/// Native store enrichment reads only captured child transcript bytes. The spool
/// verifies the retained identity/hash, including bytes outside the rendered prefix.
impl Engine<vcp_store::Store> {
    pub fn public_presentation_with_content(
        &self,
        access: &Access,
        request: &methods::Inspect,
        now: Timestamp,
    ) -> Result<methods::TaskPresentation, QueryError> {
        let mut page = self.public_presentation(access, request, now)?;
        let mut preview_bytes = 0u64;
        for row in &mut page.rows {
            let methods::PresentationRow::Evidence {
                id: artifact_id,
                task,
                ..
            } = row
            else {
                continue;
            };
            let artifact: ArtifactDescriptor = self
                .store()
                .state()
                .record(
                    Collection::Artifact,
                    artifact_id.as_str(),
                    &access.workspace,
                )
                .map_err(|_| QueryError::Unavailable)?
                .decode()
                .map_err(|_| QueryError::InvalidData)?;
            if artifact.spec.channel != Channel::ChildTranscript
                || artifact.spec.schema != "retained-full-output/1"
                || artifact.spec.source != "retained-codex"
            {
                continue;
            }
            if artifact.length.get() > 65536
                || preview_bytes.saturating_add(artifact.length.get()) > 262144
            {
                page.complete = false;
                continue;
            }
            preview_bytes += artifact.length.get();
            let mut sink = Prefix(Vec::new());
            if self.store().spool().read(&artifact, &mut sink).is_err() {
                page.complete = false;
                continue;
            }
            let bytes = &sink.0;
            let value = match std::str::from_utf8(bytes) {
                Ok(value) => value,
                Err(error)
                    if error.error_len().is_none()
                        && artifact.length.get() > bytes.len() as u64 =>
                {
                    std::str::from_utf8(&bytes[..error.valid_up_to()])
                        .map_err(|_| QueryError::InvalidData)?
                }
                Err(_) => {
                    page.complete = false;
                    continue;
                }
            };
            let mut rendered = text(value);
            rendered.truncated |= artifact.length.get() > bytes.len() as u64
                || artifact.state != CaptureState::Complete;
            *row = methods::PresentationRow::Commentary {
                id: artifact_id.clone(),
                task: task.clone(),
                turn: None,
                evidence: Some(methods::EvidenceReference {
                    artifact: artifact_id.clone(),
                    offset: 0.into(),
                    length: artifact.length.get().into(),
                    sha256: artifact.sha256.clone(),
                }),
                text: rendered,
            };
            page.commentary = methods::PresentationSource::Observed;
        }
        if serde_json::to_vec(&page)
            .map_err(|_| QueryError::InvalidData)?
            .len()
            > crate::query::MAX_RESULT_BYTES
        {
            return Err(QueryError::Limit);
        }
        Ok(page)
    }
}
struct Prefix(Vec<u8>);
impl std::io::Write for Prefix {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let take = bytes.len().min(16384usize.saturating_sub(self.0.len()));
        self.0.extend_from_slice(&bytes[..take]);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HostFacts;
    use vcp_domain::{task::Objective, verification::Fingerprint, workspace::Binding};
    use vcp_protocol::command::{Command, CommandEnvelope};
    use vcp_store::{
        contract::{Mutation, Record, Transaction},
        BackendKind, Store,
    };
    fn access() -> Access {
        Access {
            actor: ActorId::parse("owner").unwrap(),
            workspace: WorkspaceId::parse("workspace").unwrap(),
            session: SessionId::parse("session").unwrap(),
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            bootstrap: true,
        }
    }
    fn scope() -> methods::Scope {
        methods::Scope {
            workspace: id("workspace").unwrap(),
            session: id("session").unwrap(),
        }
    }
    async fn command(engine: &mut Engine<Store>, payload: Command, task: Option<TaskId>) {
        engine
            .handle(
                CommandEnvelope {
                    version: 1,
                    id: CommandId::new(),
                    workspace: access().workspace,
                    session: access().session,
                    task,
                    caller: access().actor,
                    controller: engine.controller().clone(),
                    owner_epoch: engine.owner_epoch(),
                    expected: Revision::ZERO,
                    steering: SteeringRevision::ZERO,
                    payload,
                },
                &access(),
                &HostFacts::inspect(Timestamp::new(1)),
            )
            .await
            .unwrap();
    }
    async fn fixture(path: &std::path::Path, backend: BackendKind) -> Engine<Store> {
        let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
        command(
            &mut engine,
            Command::Initialize {
                binding: Binding {
                    host: HostId::new(),
                    root: "C:/public-read-fixture".into(),
                    repository: "fixture".into(),
                    worktree: "main".into(),
                    revision: Revision::ZERO,
                },
            },
            None,
        )
        .await;
        for name in ["task", "other"] {
            let task = TaskId::parse(name).unwrap();
            command(
                &mut engine,
                Command::CreateTask {
                    root: task.clone(),
                    parent: None,
                    fork_origin: None,
                    objective: Objective {
                        text: "public read fixture".into(),
                        constraints: vec![],
                        acceptance: vec![],
                        source: EventId::new(),
                        steering: SteeringRevision::ZERO,
                    },
                    fingerprint: Fingerprint {
                        repository: "a".repeat(64),
                        buffers: "b".repeat(64),
                        environment: "c".repeat(64),
                    },
                    editing: false,
                    required_checks: vec![],
                },
                Some(task),
            )
            .await;
        }
        engine
    }
    async fn put(
        engine: &mut Engine<Store>,
        collection: Collection,
        name: &str,
        value: &impl serde::Serialize,
    ) {
        let transaction = Transaction {
            id: TransactionId::new(),
            expected_watermark: engine.store().state().watermark,
            mutations: vec![Mutation::Put {
                expected: None,
                record: Record::typed(collection, name, access().workspace, Revision::ZERO, value)
                    .unwrap(),
            }],
            events: vec![],
            command: None,
        };
        engine.store_mut().transact(transaction).await.unwrap();
    }

    #[tokio::test]
    async fn public_presentation_bounds_pages_and_rechecks_current_authority() {
        for backend in [BackendKind::Files, BackendKind::Sqlite] {
            let dir = tempfile::tempdir().unwrap();
            let mut engine = fixture(dir.path(), backend).await;
            let request = methods::Inspect {
                scope: scope(),
                task: id("task").unwrap(),
                target: None,
                cursor: None,
                limit: 1,
            };
            for name in ["effect-a", "effect-b"] {
                let effect = Effect {
                    redaction: None,
                    id: ToolRunId::parse(name).unwrap(),
                    scope: vcp_domain::workspace::Scope {
                        workspace: access().workspace,
                        session: access().session,
                        task: TaskId::parse("task").unwrap(),
                    },
                    revision: Revision::ZERO,
                    steering: SteeringRevision::ZERO,
                    state: vcp_domain::effect::EffectState::Proposed,
                    operation_digest: "a".repeat(64),
                    execution: None,
                    exit_code: None,
                    observed_changes: vec![],
                    cause: EventId::new(),
                    reason: "untrusted <script>text</script>".into(),
                };
                put(&mut engine, Collection::Effect, name, &effect).await;
            }
            let approval = Approval {
                id: ApprovalId::parse("question").unwrap(),
                scope: vcp_domain::workspace::Scope {
                    workspace: access().workspace,
                    session: access().session,
                    task: TaskId::parse("task").unwrap(),
                },
                effect: ToolRunId::parse("effect-a").unwrap(),
                effect_revision: Revision::ZERO,
                steering: SteeringRevision::ZERO,
                operation_digest: "a".repeat(64),
                actor: access().actor,
                policy: PolicyRevision::ZERO,
                expires_at: Timestamp::new(100),
                state: vcp_protocol::command::ApprovalState::Pending,
                revision: Revision::ZERO,
                controller: None,
                owner_epoch: None,
                authority: None,
                binding: None,
            };
            put(&mut engine, Collection::Approval, "question", &approval).await;
            let first = engine
                .public_presentation(&access(), &request, Timestamp::new(2))
                .unwrap();
            assert_eq!(first.objective.unwrap().text, "public read fixture");
            assert_eq!(first.model.source, methods::PresentationSource::Unavailable);
            assert_eq!(first.questions.len(), 1);
            assert_eq!(first.questions[0].effect.as_str(), "effect-a");
            assert_eq!(
                first.questions[0].input.operation_digest.as_deref(),
                Some("a".repeat(64).as_str())
            );
            assert_eq!(
                first.questions[0]
                    .input
                    .effect_revision
                    .as_ref()
                    .unwrap()
                    .as_str(),
                "0"
            );
            assert_eq!(first.questions[0].expires_at.as_str(), "100");
            assert!(
                !first.questions[0].actionable,
                "historical question is not a grant"
            );
            assert!(first.questions[0].summary.text.contains("untrusted"));
            assert_eq!(first.rows.len(), 1);
            assert!(!first.complete);
            let mut next = request.clone();
            next.cursor = first.next_cursor;
            let last = engine
                .public_presentation(&access(), &next, Timestamp::new(2))
                .unwrap();
            assert_eq!(last.rows.len(), 1);
            assert!(last.complete);
            assert!(last.next_cursor.is_none());
            let mut foreign = access();
            foreign.session = SessionId::new();
            assert!(engine
                .public_presentation(&foreign, &request, Timestamp::new(2))
                .is_err());
            foreign = access();
            foreign.authority = AuthorityRevision::new(1);
            assert!(engine
                .public_presentation(&foreign, &next, Timestamp::new(2))
                .is_err());
            let mut forged = next.clone();
            let mut cursor: serde_json::Value =
                serde_json::from_str(forged.cursor.as_ref().unwrap()).unwrap();
            cursor["scope"]["workspace"] = serde_json::json!("foreign");
            forged.cursor = Some(serde_json::to_string(&cursor).unwrap());
            assert!(matches!(
                engine.public_presentation(&access(), &forged, Timestamp::new(2)),
                Err(QueryError::StaleCursor)
            ));
            let mut other = next.clone();
            other.task = id("other").unwrap();
            assert!(matches!(
                engine.public_presentation(&access(), &other, Timestamp::new(2)),
                Err(QueryError::StaleCursor)
            ));
            let mut changed: Effect = engine
                .store()
                .state()
                .record(Collection::Effect, "effect-a", &access().workspace)
                .unwrap()
                .decode()
                .unwrap();
            changed.id = ToolRunId::parse("effect-c").unwrap();
            put(&mut engine, Collection::Effect, "effect-c", &changed).await;
            assert!(matches!(
                engine.public_presentation(&access(), &next, Timestamp::new(2)),
                Err(QueryError::StaleCursor)
            ));
        }
    }
    #[test]
    fn text_is_bounded_with_explicit_truncation() {
        let rendered = text(&"x".repeat(4097));
        assert_eq!(rendered.text.chars().count(), 4096);
        assert!(rendered.truncated);
        assert!(!text("short").truncated);
    }
    #[tokio::test]
    async fn retained_commentary_is_attributed_bounded_and_masked_on_both_stores() {
        use vcp_domain::artifact::{ArtifactSpec, Omission};
        use vcp_store::artifact::ArtifactWriter;
        for backend in [BackendKind::Files, BackendKind::Sqlite] {
            let dir = tempfile::tempdir().unwrap();
            let mut engine = fixture(dir.path(), backend).await;
            let mut writer = engine
                .store()
                .spool()
                .create(ArtifactSpec {
                    id: ArtifactId::parse("transcript").unwrap(),
                    scope: vcp_domain::workspace::Scope {
                        workspace: access().workspace,
                        session: access().session,
                        task: TaskId::parse("task").unwrap(),
                    },
                    media_type: "application/octet-stream".into(),
                    schema: "retained-full-output/1".into(),
                    source: "retained-codex".into(),
                    channel: Channel::ChildTranscript,
                    retention: "history".into(),
                    omissions: vec![Omission::AuthenticationHeaders, Omission::RecoveryMaterial],
                })
                .unwrap();
            writer
                .write_chunk(format!("<script>untrusted</script>{}", "x".repeat(20000)).as_bytes())
                .unwrap();
            let descriptor = writer.finalize().unwrap();
            command(
                &mut engine,
                Command::AttachArtifact { descriptor },
                Some(TaskId::parse("task").unwrap()),
            )
            .await;
            let request = methods::Inspect {
                scope: scope(),
                task: id("task").unwrap(),
                target: None,
                cursor: None,
                limit: 128,
            };
            let page = engine
                .public_presentation_with_content(&access(), &request, Timestamp::new(2))
                .unwrap();
            assert_eq!(page.commentary, methods::PresentationSource::Observed);
            assert!(page.complete);
            let methods::PresentationRow::Commentary {
                task, text, turn, ..
            } = &page.rows[0]
            else {
                panic!("retained transcript")
            };
            assert_eq!(task.as_str(), "task");
            assert!(turn.is_none());
            assert!(text.truncated);
            assert!(text.text.starts_with("<script>"));
            assert_eq!(text.text.chars().count(), 4096);
            let mut denied = access();
            denied.read = false;
            assert!(engine
                .public_presentation_with_content(&denied, &request, Timestamp::new(2))
                .is_err());
            let mask = RetentionMask {
                schema_version: 1,
                workspace: access().workspace,
                session: access().session,
                first: SessionSeq::ZERO,
                last: SessionSeq::ZERO,
                artifacts: vec![ArtifactId::parse("transcript").unwrap()],
                deletion: DeletionEpoch::ZERO,
                reason: "retention suppression".into(),
            };
            put(&mut engine, Collection::Tombstone, "mask", &mask).await;
            let page = engine
                .public_presentation_with_content(&access(), &request, Timestamp::new(2))
                .unwrap();
            assert!(page.rows.is_empty());
            assert!(!page.complete);
            assert_eq!(page.commentary, methods::PresentationSource::Unavailable);
            let mut writer = engine
                .store()
                .spool()
                .create(ArtifactSpec {
                    id: ArtifactId::parse("oversized").unwrap(),
                    scope: vcp_domain::workspace::Scope {
                        workspace: access().workspace,
                        session: access().session,
                        task: TaskId::parse("task").unwrap(),
                    },
                    media_type: "application/octet-stream".into(),
                    schema: "retained-full-output/1".into(),
                    source: "retained-codex".into(),
                    channel: Channel::ChildTranscript,
                    retention: "history".into(),
                    omissions: vec![Omission::AuthenticationHeaders, Omission::RecoveryMaterial],
                })
                .unwrap();
            writer.write_chunk(&vec![b'x'; 65536]).unwrap();
            writer.write_chunk(b"x").unwrap();
            let descriptor = writer.finalize().unwrap();
            command(
                &mut engine,
                Command::AttachArtifact { descriptor },
                Some(TaskId::parse("task").unwrap()),
            )
            .await;
            let page = engine
                .public_presentation_with_content(&access(), &request, Timestamp::new(2))
                .unwrap();
            assert!(!page.complete);
            assert!(
                matches!(&page.rows[0], methods::PresentationRow::Evidence { id, .. } if id.as_str() == "oversized")
            );
        }
    }
}
