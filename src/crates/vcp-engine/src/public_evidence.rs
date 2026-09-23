// SPDX-License-Identifier: Apache-2.0
//! Public inspectors project retained references, never raw internal records.
//! A target selects an artifact identity, not an inferred turn/attempt linkage.
use crate::{query::QueryError, Access, Engine};
use serde::{Deserialize, Serialize};
use vcp_domain::{
    artifact::{ArtifactDescriptor, CaptureState, Channel, Omission},
    ids::*,
    retention::RetentionMask,
    revision::*,
    workspace::Workspace,
};
use vcp_protocol::methods::{self, Call};
use vcp_store::contract::{CanonicalStore, Collection, State};

fn masked(
    state: &State,
    workspace: &WorkspaceId,
    artifact: &ArtifactId,
) -> Result<bool, QueryError> {
    for record in state
        .records
        .values()
        .filter(|row| row.collection == Collection::Tombstone && &row.workspace == workspace)
    {
        let mask: RetentionMask = record.decode().map_err(|_| QueryError::InvalidData)?;
        if mask.artifacts.contains(artifact) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum View {
    Context,
    Routing,
}
impl View {
    fn schema(self) -> &'static str {
        match self {
            Self::Context => "context-manifest/1",
            Self::Routing => "routing-selection/1",
        }
    }
}

/// Revision precondition only. Every page independently checks current access.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    version: u32,
    actor: ActorId,
    authority: AuthorityRevision,
    deletion: DeletionEpoch,
    watermark: Watermark,
    query: String,
    after: ArtifactId,
}

impl<S: CanonicalStore> Engine<S> {
    /// Retained manifest references for this exact task. No live context assembly.
    pub fn public_context(
        &self,
        access: &Access,
        request: &methods::Inspect,
    ) -> Result<methods::EvidencePage, QueryError> {
        self.public_evidence(access, request, View::Context)
    }

    /// Observed routing selections only; a fixed provider has no invented decision.
    pub fn public_routing(
        &self,
        access: &Access,
        request: &methods::Inspect,
    ) -> Result<methods::EvidencePage, QueryError> {
        self.public_evidence(access, request, View::Routing)
    }

    fn public_evidence(
        &self,
        access: &Access,
        request: &methods::Inspect,
        view: View,
    ) -> Result<methods::EvidencePage, QueryError> {
        let task = self.read_task(access, &request.scope, &request.task)?;
        match view {
            View::Context => Call::ContextInspect(request.clone()),
            View::Routing => Call::RoutingExplain(request.clone()),
        }
        .validate()
        .map_err(|_| QueryError::Limit)?;
        if task.redaction.is_some() {
            return Err(QueryError::Unavailable);
        }
        let state = self.store().state();
        let workspace: Workspace = state
            .record(
                Collection::Workspace,
                access.workspace.as_str(),
                &access.workspace,
            )
            .map_err(|_| QueryError::Access)?
            .decode()
            .map_err(|_| QueryError::InvalidData)?;
        let query = vcp_protocol::digest_bytes(
            &vcp_protocol::canonical_bytes(&(
                view,
                &request.scope,
                &request.task,
                &request.target,
                request.limit,
            ))
            .map_err(|_| QueryError::InvalidData)?,
        );
        let cursor: Option<Cursor> = request
            .cursor
            .as_ref()
            .map(|value| {
                let cursor: Cursor =
                    serde_json::from_str(value).map_err(|_| QueryError::StaleCursor)?;
                if cursor.version != 1
                    || cursor.actor != access.actor
                    || cursor.authority != access.authority
                    || cursor.deletion != workspace.deletion
                    || cursor.watermark != state.watermark
                    || cursor.query != query
                {
                    return Err(QueryError::StaleCursor);
                }
                Ok(cursor)
            })
            .transpose()?;
        let mut gap = state.events.iter().any(|event| {
            event.event.workspace == access.workspace
                && event.event.session == access.session
                && event.event.task.as_ref() == Some(&task.scope.task)
                && event.redaction.is_some()
        });
        for record in state.records.values().filter(|row| {
            row.collection == Collection::Tombstone && row.workspace == access.workspace
        }) {
            let mask: RetentionMask = record.decode().map_err(|_| QueryError::InvalidData)?;
            mask.validate().map_err(|_| QueryError::InvalidData)?;
            if mask.workspace != access.workspace || mask.deletion > workspace.deletion {
                return Err(QueryError::InvalidData);
            }
            gap |= mask.session == access.session
                && state.events.iter().any(|event| {
                    event.event.workspace == access.workspace
                        && event.event.session == access.session
                        && event.event.task.as_ref() == Some(&task.scope.task)
                        && mask.first <= event.sequence
                        && event.sequence <= mask.last
                });
        }
        let mut rows = Vec::with_capacity(request.limit as usize + 1);
        let mut found = false;
        let mut after_found = cursor.is_none();
        for record in state.records.values().filter(|row| {
            row.collection == Collection::Artifact && row.workspace == access.workspace
        }) {
            // Narrow to this task before decoding: a foreign task's malformed or
            // hidden private payload is not part of this public result.
            if record
                .value
                .pointer("/spec/scope/session")
                .and_then(|v| v.as_str())
                != Some(access.session.as_str())
                || record
                    .value
                    .pointer("/spec/scope/task")
                    .and_then(|v| v.as_str())
                    != Some(task.scope.task.as_str())
            {
                continue;
            }
            let artifact: ArtifactDescriptor =
                record.decode().map_err(|_| QueryError::InvalidData)?;
            artifact.validate().map_err(|_| QueryError::InvalidData)?;
            if artifact.spec.scope != task.scope || artifact.spec.id.as_str() != record.id {
                return Err(QueryError::InvalidData);
            }
            if artifact.spec.schema != view.schema() || artifact.spec.channel != Channel::Evidence {
                continue;
            }
            if request
                .target
                .as_ref()
                .is_some_and(|id| id.as_str() != artifact.spec.id.as_str())
            {
                continue;
            }
            found = true;
            if artifact.state == CaptureState::Purged
                || masked(state, &access.workspace, &artifact.spec.id)?
            {
                if request.target.is_some() {
                    return Err(QueryError::Unavailable);
                }
                gap = true;
                continue;
            }
            // Mandatory credential/recovery-secret exclusions are outside the
            // public evidence contract. Missing observed content still makes
            // the selection incomplete, even when its retained bytes finalized.
            if artifact.state != CaptureState::Complete
                || artifact.spec.omissions.iter().any(|omission| {
                    !matches!(
                        omission,
                        Omission::AuthenticationHeaders | Omission::RecoveryMaterial
                    )
                })
            {
                gap = true;
            }
            if let Some(cursor) = &cursor {
                if artifact.spec.id == cursor.after {
                    after_found = true;
                }
                if artifact.spec.id <= cursor.after {
                    continue;
                }
            }
            // Scan the entire authorized metadata selection for omitted evidence,
            // but retain at most one page plus a lookahead reference.
            if rows.len() <= request.limit as usize {
                let id: methods::Id = artifact
                    .spec
                    .id
                    .to_string()
                    .try_into()
                    .map_err(|_| QueryError::InvalidData)?;
                rows.push(methods::EvidenceRow {
                    id: id.clone(),
                    schema: artifact.spec.schema,
                    revision: record.revision.get().into(),
                    content: methods::EvidenceReference {
                        artifact: id,
                        offset: 0.into(),
                        length: artifact.length.get().into(),
                        sha256: artifact.sha256,
                    },
                });
            }
        }
        if request.target.is_some() && !found {
            return Err(QueryError::Unavailable);
        }
        if !after_found {
            return Err(QueryError::StaleCursor);
        }
        let more = rows.len() > request.limit as usize;
        rows.truncate(request.limit as usize);
        let next_cursor = if more {
            let after = rows.last().ok_or(QueryError::InvalidData)?;
            let cursor = Cursor {
                version: 1,
                actor: access.actor.clone(),
                authority: access.authority,
                deletion: workspace.deletion,
                watermark: state.watermark,
                query,
                after: ArtifactId::parse(after.id.as_str()).map_err(|_| QueryError::InvalidData)?,
            };
            let value = serde_json::to_string(&cursor).map_err(|_| QueryError::InvalidData)?;
            if value.len() > 4096 {
                return Err(QueryError::Limit);
            }
            Some(value)
        } else {
            None
        };
        Ok(methods::EvidencePage {
            scope: request.scope.clone(),
            task: request.task.clone(),
            watermark: state.watermark.get().into(),
            rows,
            next_cursor,
            complete: found && !gap && !more,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HostFacts;
    use vcp_domain::{
        artifact::ArtifactSpec,
        task::Objective,
        verification::Fingerprint,
        workspace::{Binding, Scope},
    };
    use vcp_protocol::command::{Command, CommandEnvelope};
    use vcp_store::{
        artifact::ArtifactWriter,
        contract::{Mutation, Record, Transaction},
        BackendKind, Store,
    };
    fn id(value: &str) -> Result<methods::Id, String> {
        value.to_owned().try_into().map_err(|e| format!("{e}"))
    }
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
    async fn artifact(
        engine: &mut Engine<Store>,
        name: &str,
        bytes: &[u8],
        aborted: bool,
        schema: &str,
        task_name: &str,
        channel: Channel,
    ) -> ArtifactDescriptor {
        artifact_with_omissions(
            engine,
            name,
            bytes,
            aborted,
            schema,
            task_name,
            channel,
            vec![Omission::AuthenticationHeaders, Omission::RecoveryMaterial],
        )
        .await
    }

    async fn artifact_with_omissions(
        engine: &mut Engine<Store>,
        name: &str,
        bytes: &[u8],
        aborted: bool,
        schema: &str,
        task_name: &str,
        channel: Channel,
        omissions: Vec<Omission>,
    ) -> ArtifactDescriptor {
        let task = TaskId::parse(task_name).unwrap();
        let mut writer = engine
            .store()
            .spool()
            .create(ArtifactSpec {
                id: ArtifactId::parse(name).unwrap(),
                scope: Scope {
                    workspace: access().workspace,
                    session: access().session,
                    task: task.clone(),
                },
                media_type: "application/octet-stream".into(),
                schema: schema.into(),
                source: "fixture".into(),
                channel,
                retention: "history".into(),
                omissions,
            })
            .unwrap();
        for chunk in bytes.chunks(65_536) {
            writer.write_chunk(chunk).unwrap();
        }
        let descriptor = if aborted {
            writer.abort().unwrap()
        } else {
            writer.finalize().unwrap()
        };
        command(
            engine,
            Command::AttachArtifact {
                descriptor: descriptor.clone(),
            },
            Some(task),
        )
        .await;
        descriptor
    }
    fn request() -> methods::Inspect {
        methods::Inspect {
            scope: scope(),
            task: id("task").unwrap(),
            target: None,
            cursor: None,
            limit: 1,
        }
    }

    #[tokio::test]
    async fn secret_exclusions_preserve_public_completeness_but_missing_content_does_not() {
        for backend in [BackendKind::Sqlite, BackendKind::Files] {
            let temp = tempfile::tempdir().unwrap();
            let mut engine = fixture(temp.path(), backend).await;
            for (name, schema) in [
                ("complete-context", "context-manifest/1"),
                ("complete-route", "routing-selection/1"),
            ] {
                let descriptor = artifact(
                    &mut engine,
                    name,
                    b"public evidence",
                    false,
                    schema,
                    "task",
                    Channel::Evidence,
                )
                .await;
                assert_eq!(descriptor.state, CaptureState::Complete);
                assert_eq!(
                    descriptor.spec.omissions,
                    vec![Omission::AuthenticationHeaders, Omission::RecoveryMaterial]
                );
                let mut selected = request();
                selected.target = Some(id(name).unwrap());
                let page = if schema == "context-manifest/1" {
                    engine.public_context(&access(), &selected)
                } else {
                    engine.public_routing(&access(), &selected)
                }
                .unwrap();
                assert!(page.complete && page.next_cursor.is_none());
                assert_eq!(page.rows.len(), 1);
            }
            for (name, omission) in [
                ("tail", Omission::UnobservedTail),
                ("failure", Omission::CaptureFailure),
                ("abort", Omission::ExplicitAbort),
            ] {
                let descriptor = artifact_with_omissions(
                    &mut engine,
                    name,
                    b"observed prefix",
                    false,
                    "context-manifest/1",
                    "task",
                    Channel::Evidence,
                    vec![
                        Omission::AuthenticationHeaders,
                        Omission::RecoveryMaterial,
                        omission,
                    ],
                )
                .await;
                assert_eq!(descriptor.state, CaptureState::Complete);
                let mut selected = request();
                selected.target = Some(id(name).unwrap());
                let page = engine.public_context(&access(), &selected).unwrap();
                assert!(!page.complete && page.next_cursor.is_none());
                assert_eq!(page.rows.len(), 1);
            }
            engine.into_store().close().await.unwrap();
        }
    }

    #[tokio::test]
    async fn references_are_exact_scoped_allowlisted_paged_and_restartable_on_both_stores() {
        for backend in [BackendKind::Sqlite, BackendKind::Files] {
            let temp = tempfile::tempdir().unwrap();
            let mut engine = fixture(temp.path(), backend).await;
            let mut request = request();
            let empty = engine.public_context(&access(), &request).unwrap();
            assert!(empty.rows.is_empty() && !empty.complete && empty.next_cursor.is_none());
            for (name, schema, task, channel) in [
                (
                    "a-manifest",
                    "context-manifest/1",
                    "task",
                    Channel::Evidence,
                ),
                (
                    "b-manifest",
                    "context-manifest/1",
                    "task",
                    Channel::Evidence,
                ),
                ("route", "routing-selection/1", "task", Channel::Evidence),
                (
                    "wrong-version",
                    "context-manifest/2",
                    "task",
                    Channel::Evidence,
                ),
                (
                    "wrong-channel",
                    "context-manifest/1",
                    "task",
                    Channel::Response,
                ),
                ("foreign", "context-manifest/1", "other", Channel::Evidence),
            ] {
                artifact(
                    &mut engine,
                    name,
                    b"private stored content",
                    false,
                    schema,
                    task,
                    channel,
                )
                .await;
            }
            let watermark = engine.store().state().watermark;
            let first = engine.public_context(&access(), &request).unwrap();
            assert_eq!(first.rows.len(), 1);
            assert_eq!(first.rows[0].id.as_str(), "a-manifest");
            assert_eq!(first.rows[0].content.offset.as_str(), "0");
            assert_eq!(first.rows[0].content.length.as_str(), "22");
            assert_eq!(
                first.rows[0].content.sha256,
                vcp_protocol::digest_bytes(b"private stored content")
            );
            assert!(!serde_json::to_string(&first)
                .unwrap()
                .contains("private stored content"));
            assert!(!first.complete);
            request.cursor = first.next_cursor.clone();
            let second = engine.public_context(&access(), &request).unwrap();
            assert_eq!(second.rows[0].id.as_str(), "b-manifest");
            assert!(second.complete && second.next_cursor.is_none());
            assert_eq!(second.watermark, first.watermark);
            let mut other_actor = access();
            other_actor.actor = ActorId::parse("other-actor").unwrap();
            assert_eq!(
                engine.public_context(&other_actor, &request),
                Err(QueryError::StaleCursor)
            );
            assert_eq!(
                engine.public_routing(&access(), &request),
                Err(QueryError::StaleCursor)
            );
            request.limit = 2;
            assert_eq!(
                engine.public_context(&access(), &request),
                Err(QueryError::StaleCursor)
            );
            request.limit = 1;
            assert_eq!(engine.store().state().watermark, watermark);
            engine.into_store().close().await.unwrap();
            let engine =
                Engine::new(Store::open(temp.path(), backend, &[]).await.unwrap()).unwrap();
            assert_eq!(engine.public_context(&access(), &request).unwrap(), second);
            request.cursor = None;
            let route = engine.public_routing(&access(), &request).unwrap();
            assert_eq!(route.rows[0].id.as_str(), "route");
            assert!(route.complete);
            for target in [
                "foreign",
                "wrong-version",
                "wrong-channel",
                "task",
                "route",
                "absent",
            ] {
                request.target = Some(id(target).unwrap());
                assert_eq!(
                    engine.public_context(&access(), &request),
                    Err(QueryError::Unavailable)
                );
            }
            request.target = Some(id("a-manifest").unwrap());
            assert!(engine.public_context(&access(), &request).unwrap().complete);
            let mut denied = access();
            denied.read = false;
            assert_eq!(
                engine.public_context(&denied, &request),
                Err(QueryError::Access)
            );
            request.limit = 129;
            assert_eq!(
                engine.public_context(&access(), &request),
                Err(QueryError::Limit)
            );
            engine.into_store().close().await.unwrap();
        }
    }

    #[tokio::test]
    async fn partial_and_masked_evidence_never_claim_complete_and_changed_cursors_fail() {
        for backend in [BackendKind::Sqlite, BackendKind::Files] {
            let temp = tempfile::tempdir().unwrap();
            let mut engine = fixture(temp.path(), backend).await;
            let mut request = request();
            for (name, aborted) in [("a-complete", false), ("b-aborted", true)] {
                artifact(
                    &mut engine,
                    name,
                    b"observed prefix",
                    aborted,
                    "context-manifest/1",
                    "task",
                    Channel::Evidence,
                )
                .await;
            }
            let first = engine.public_context(&access(), &request).unwrap();
            request.cursor = first.next_cursor;
            let second = engine.public_context(&access(), &request).unwrap();
            assert_eq!(second.rows[0].id.as_str(), "b-aborted");
            assert!(!second.complete && second.next_cursor.is_none());
            let mask = RetentionMask {
                schema_version: 1,
                workspace: access().workspace,
                session: access().session,
                first: SessionSeq::ZERO,
                last: SessionSeq::ZERO,
                artifacts: vec![ArtifactId::parse("a-complete").unwrap()],
                deletion: DeletionEpoch::ZERO,
                reason: "logical suppression before physical rewrite".into(),
            };
            put(&mut engine, Collection::Tombstone, "mask", &mask).await;
            assert_eq!(
                engine.public_context(&access(), &request),
                Err(QueryError::StaleCursor)
            );
            request.cursor = None;
            request.limit = 128;
            let masked = engine.public_context(&access(), &request).unwrap();
            assert_eq!(masked.rows.len(), 1);
            assert_eq!(masked.rows[0].id.as_str(), "b-aborted");
            assert!(!masked.complete);
            request.target = Some(id("a-complete").unwrap());
            assert_eq!(
                engine.public_context(&access(), &request),
                Err(QueryError::Unavailable)
            );
            request.target = Some(id("b-aborted").unwrap());
            assert!(!engine.public_context(&access(), &request).unwrap().complete);
            request.cursor = Some("{\"version\":1}".into());
            assert_eq!(
                engine.public_context(&access(), &request),
                Err(QueryError::StaleCursor)
            );
            let mut revoked = access();
            revoked.authority = AuthorityRevision::new(1);
            assert_eq!(
                engine.public_context(&revoked, &request),
                Err(QueryError::Access)
            );
            engine.into_store().close().await.unwrap();
        }
    }
}
