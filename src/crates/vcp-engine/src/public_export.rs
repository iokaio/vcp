// SPDX-License-Identifier: Apache-2.0
//! Scoped, atomic derived export admission. Rendering is host-owned and cannot
//! carry filesystem destinations, credentials, controller claims, or execution.
use crate::{controller::ControllerToken, public::PublicError, Access, Engine};
use std::result::Result;
use vcp_domain::{artifact::*, workspace::*, *};
use vcp_protocol::{
    command::{CommandReceipt, CommandResult},
    event::{EventInput, EventKind},
    methods::{self, Call},
};
use vcp_store::{
    artifact::{ArtifactWriter, CHUNK_BYTES},
    contract::*,
    export_contract::{self, Acceptance, Rendered, Sources},
    Store,
};

/// Current trusted host disclosure policy, not deserializable caller input.
/// Enabling artifacts permits arbitrary retained source bytes, not sanitized
/// content. Policy changes must update canonical policy/authority revisions.
pub struct ExportDisclosure {
    pub policy: PolicyRevision,
    pub history: bool,
    pub artifacts: bool,
}
pub struct PublicExportOutcome {
    pub receipt: CommandReceipt,
    pub view: methods::ExportView,
    pub replayed: bool,
}
pub enum PublicExportAdmission {
    Replay(PublicExportOutcome),
    Ready(PreparedPublicExport),
}
/// Capture faults require the lifecycle owner to stop admission until recovery.
/// Validation/source errors and uncertain canonical commits retain their original
/// public classification; they must not be mistaken for an interrupted spool.
#[derive(Debug)]
pub enum PublicExportCommitError {
    Public(PublicError),
    CaptureFault,
}
impl From<PublicError> for PublicExportCommitError {
    fn from(error: PublicError) -> Self {
        Self::Public(error)
    }
}
pub struct PreparedPublicExport {
    request: methods::SessionExport,
    actor: ActorId,
    connection: ControllerId,
    token: ControllerToken,
    sources: Sources,
}
impl PreparedPublicExport {
    pub fn sources(&self) -> &Sources {
        &self.sources
    }
    pub fn capture(&self) -> methods::CaptureScope {
        self.request.capture.clone()
    }
}
fn invalid<T>(_: T) -> PublicError {
    PublicError::InvalidParameters
}
fn unavailable<T>(_: T) -> PublicError {
    PublicError::Unavailable
}
fn view(
    request: &methods::SessionExport,
    acceptance: &Acceptance,
) -> Result<methods::ExportView, PublicError> {
    Ok(methods::ExportView {
        scope: request.scope.clone(),
        artifact: acceptance
            .artifact
            .spec
            .id
            .to_string()
            .try_into()
            .map_err(invalid)?,
        visibility_manifest: acceptance
            .visibility_manifest
            .spec
            .id
            .to_string()
            .try_into()
            .map_err(invalid)?,
        complete: acceptance.complete,
    })
}
impl Engine<Store> {
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_public_export(
        &self,
        request: methods::SessionExport,
        access: &Access,
        connection: &ControllerId,
        token: &ControllerToken,
        disclosure: &ExportDisclosure,
        anchor: &TaskId,
    ) -> Result<PublicExportAdmission, PublicError> {
        self.check_controller(access, connection, token)
            .map_err(|_| PublicError::Access)?;
        let call = Call::SessionExport(request.clone());
        call.validate().map_err(invalid)?;
        if request.scope.workspace.as_str() != access.workspace.as_str()
            || request.scope.session.as_str() != access.session.as_str()
            || !disclosure.history
            || (request.capture == methods::CaptureScope::VisibleHistoryAndArtifacts
                && !disclosure.artifacts)
        {
            return Err(PublicError::Access);
        }
        let policy = crate::policy::optional(self.store().state(), &access.workspace)
            .map_err(unavailable)?
            .map_or(PolicyRevision::ZERO, |p| p.revision);
        if policy != disclosure.policy {
            return Err(PublicError::Access);
        }
        let command = CommandId::parse(request.mutation.command_id.as_str()).map_err(invalid)?;
        let digest = call.digest(access.actor.as_str()).map_err(invalid)?;
        let state = self.store().state();
        if let Some(receipt) = state
            .commands
            .get(&command_key(&access.workspace, &command))
        {
            if receipt.digest != digest {
                return Err(PublicError::CommandConflict);
            }
            let mut proof = None;
            for event in state.events.iter().filter(|e| {
                e.watermark == receipt.watermark
                    && e.event.correlation == command
                    && e.event.workspace == access.workspace
                    && e.event.session == access.session
            }) {
                if let Some(value) = event.event.data.get("session_export") {
                    if proof.is_some() {
                        return Err(PublicError::Unavailable);
                    }
                    proof = Some(
                        serde_json::from_value::<Acceptance>(value.clone()).map_err(unavailable)?,
                    );
                }
            }
            let accepted = proof.ok_or(PublicError::Unavailable)?;
            if accepted.capture != request.capture
                || accepted.sources.task().map(TaskId::as_str)
                    != request.task.as_ref().map(methods::Id::as_str)
            {
                return Err(PublicError::Unavailable);
            }
            export_contract::validate_read(state, access.authority, None, &accepted.artifact)
                .map_err(unavailable)?;
            return Ok(PublicExportAdmission::Replay(PublicExportOutcome {
                receipt: receipt.clone(),
                view: view(&request, &accepted)?,
                replayed: true,
            }));
        }
        let selected = request
            .task
            .as_ref()
            .map(|id| TaskId::parse(id.as_str()).map_err(invalid))
            .transpose()?;
        let task = selected.as_ref().unwrap_or(anchor).clone();
        let scope = Scope {
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            task,
        };
        let expected = request
            .mutation
            .expected_revision
            .as_str()
            .parse::<u64>()
            .map_err(invalid)?;
        let steering = request
            .mutation
            .steering_revision
            .as_str()
            .parse::<u64>()
            .map_err(invalid)?;
        if selected.is_some() {
            let current: vcp_domain::task::Task = state
                .record(Collection::Task, scope.task.as_str(), &scope.workspace)
                .map_err(unavailable)?
                .decode()
                .map_err(unavailable)?;
            if current.scope != scope {
                return Err(PublicError::Access);
            }
            if current.revision.get() != expected || current.steering.get() != steering {
                return Err(PublicError::StaleState);
            }
        } else {
            let current: Session = state
                .record(
                    Collection::Session,
                    scope.session.as_str(),
                    &scope.workspace,
                )
                .map_err(unavailable)?
                .decode()
                .map_err(unavailable)?;
            if current.revision.get() != expected || steering != 0 {
                return Err(PublicError::StaleState);
            }
        }
        let sources =
            Sources::capture(state, scope, selected, access.authority).map_err(unavailable)?;
        Ok(PublicExportAdmission::Ready(PreparedPublicExport {
            request,
            actor: access.actor.clone(),
            connection: connection.clone(),
            token: token.clone(),
            sources,
        }))
    }

    /// Re-admits before any spool write. Both descriptors and original-command
    /// receipt become visible in one transaction. A failed commit can leave
    /// complete unreferenced spool objects, never a canonical half-export.
    pub async fn commit_public_export(
        &mut self,
        prepared: PreparedPublicExport,
        access: &Access,
        disclosure: &ExportDisclosure,
        rendered: Rendered,
        now: Timestamp,
    ) -> Result<PublicExportOutcome, PublicExportCommitError> {
        if prepared.actor != access.actor {
            return Err(PublicError::Access.into());
        }
        let ready = match self.prepare_public_export(
            prepared.request.clone(),
            access,
            &prepared.connection,
            &prepared.token,
            disclosure,
            &prepared.sources.scope().task,
        )? {
            PublicExportAdmission::Replay(value) => return Ok(value),
            PublicExportAdmission::Ready(value) => value,
        };
        if ready.sources != prepared.sources
            || rendered.sources != ready.sources
            || rendered.capture != ready.request.capture
        {
            return Err(PublicError::StaleState.into());
        }
        if rendered.payload.len() > export_contract::MAX_BYTES
            || rendered.omissions.len()
                > export_contract::MAX_EVENTS + export_contract::MAX_ARTIFACTS + 2
            || rendered.omissions.iter().any(|s| s.len() > 512)
            || (rendered.complete && !rendered.omissions.is_empty())
        {
            return Err(PublicError::InvalidParameters.into());
        }
        let capture =
            |schema: &str, bytes: &[u8]| -> Result<ArtifactDescriptor, PublicExportCommitError> {
                let mut writer = self
                    .store()
                    .spool()
                    .create(ArtifactSpec {
                        id: ArtifactId::new(),
                        scope: ready.sources.scope().clone(),
                        media_type: "application/json".into(),
                        schema: schema.into(),
                        source: "canonical bounded session export".into(),
                        channel: Channel::Evidence,
                        retention: "history".into(),
                        omissions: vec![],
                    })
                    .map_err(|_| PublicExportCommitError::CaptureFault)?;
                for chunk in bytes.chunks(CHUNK_BYTES) {
                    writer
                        .write_chunk(chunk)
                        .map_err(|_| PublicExportCommitError::CaptureFault)?;
                }
                writer
                    .finalize()
                    .map_err(|_| PublicExportCommitError::CaptureFault)
            };
        let artifact = capture(export_contract::PAYLOAD_SCHEMA, &rendered.payload)?;
        let manifest=serde_json::to_vec(&serde_json::json!({"schema":export_contract::MANIFEST_SCHEMA,"sources":ready.sources,"capture":rendered.capture,
            "artifact":artifact,"complete":rendered.complete,"omissions":rendered.omissions,"secret_sanitization":false})).map_err(unavailable)?;
        if manifest.len().saturating_add(rendered.payload.len()) > export_contract::MAX_BYTES {
            return Err(PublicError::InvalidParameters.into());
        }
        let visibility_manifest = capture(export_contract::MANIFEST_SCHEMA, &manifest)?;
        let acceptance = Acceptance {
            schema_version: 1,
            sources: ready.sources.clone(),
            capture: rendered.capture,
            artifact,
            visibility_manifest,
            complete: rendered.complete,
        };
        let command =
            CommandId::parse(ready.request.mutation.command_id.as_str()).map_err(invalid)?;
        let digest = Call::SessionExport(ready.request.clone())
            .digest(access.actor.as_str())
            .map_err(invalid)?;
        let mut mutations = Vec::new();
        let mut facts = Vec::new();
        for descriptor in [&acceptance.artifact, &acceptance.visibility_manifest] {
            let record = Record::typed(
                Collection::Artifact,
                descriptor.spec.id.as_str(),
                access.workspace.clone(),
                Revision::ZERO,
                descriptor,
            )
            .map_err(unavailable)?;
            facts.push(serde_json::json!({"collection":record.collection,"id":record.id,"revision":record.revision,"value":record.value}));
            mutations.push(Mutation::Put {
                expected: None,
                record,
            });
        }
        let transaction = Transaction {
            id: TransactionId::new(),
            expected_watermark: ready.sources.watermark(),
            mutations,
            events: vec![EventInput {
                id: EventId::new(),
                workspace: access.workspace.clone(),
                session: access.session.clone(),
                task: Some(ready.sources.scope().task.clone()),
                actor: access.actor.clone(),
                correlation: command.clone(),
                causation: None,
                timestamp: now,
                kind: EventKind::ArtifactAttached,
                artifacts: vec![
                    acceptance.artifact.spec.id.clone(),
                    acceptance.visibility_manifest.spec.id.clone(),
                ],
                data: serde_json::json!({"schema_version":1,"session_export":acceptance,"facts":facts}),
                metadata: None,
            }],
            command: Some(ReceiptInput {
                command,
                workspace: access.workspace.clone(),
                session: access.session.clone(),
                digest,
                result: CommandResult::Accepted {
                    revision: Revision::ZERO,
                },
            }),
        };
        let receipt = self
            .store_mut()
            .transact(transaction)
            .await
            .map_err(|e| match e {
                vcp_store::Error::Conflict(_) => PublicError::StaleState,
                _ => PublicError::OutcomeUnknown,
            })?
            .command
            .ok_or(PublicError::OutcomeUnknown)?;
        Ok(PublicExportOutcome {
            receipt,
            view: view(&ready.request, &acceptance)?,
            replayed: false,
        })
    }
}
