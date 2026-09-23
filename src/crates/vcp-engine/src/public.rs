// SPDX-License-Identifier: Apache-2.0
//! Public mutation translation into the canonical command handler.
//! This module does not advertise a transport or an execution-host capability.
use crate::{controller::ControllerToken, Access, Engine, HostFacts};
use serde::{Deserialize, Serialize};
use vcp_domain::{
    ids::*,
    revision::*,
    task::{Objective, Task, TaskState, Turn, TurnState},
    workspace::{Session, Workspace},
};
use vcp_protocol::{
    command::{Approval, ApprovalState, Command, CommandEnvelope, CommandReceipt},
    methods::{ApprovalDecision, Call, Counter},
};
use vcp_store::contract::{CanonicalStore, Collection};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum PublicError {
    #[error("current mutation access denied")]
    Access,
    #[error("invalid public method parameters")]
    InvalidParameters,
    #[error("method requires an unavailable execution-host capability")]
    CapabilityUnavailable,
    #[error("command identity was already used with different semantics")]
    CommandConflict,
    #[error("operation revision or current authority changed")]
    StaleState,
    #[error("target unavailable in the authorized scope")]
    Unavailable,
    #[error("command outcome is uncertain; reconcile the durable command before retrying")]
    OutcomeUnknown,
}

fn number(value: &Counter) -> Result<u64, PublicError> {
    value
        .as_str()
        .parse()
        .map_err(|_| PublicError::InvalidParameters)
}

/// Host-local admission, never accepted from a wire representation.
#[derive(Debug)]
pub enum PublicAdmission {
    Replay(CommandReceipt),
    Ready(PreparedPublicCommand),
}

#[derive(Debug)]
pub struct PreparedPublicCommand {
    call: Call,
    digest: String,
    command: CommandEnvelope,
    authority: AuthorityRevision,
    controller: Option<(ControllerId, ControllerToken)>,
}
impl PreparedPublicCommand {
    pub fn call(&self) -> &Call {
        &self.call
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn id(&self) -> &CommandId {
        &self.command.id
    }
    pub fn payload(&self) -> &Command {
        &self.command.payload
    }
    pub fn task(&self) -> Option<&TaskId> {
        self.command.task.as_ref()
    }
    pub fn expected(&self) -> Revision {
        self.command.expected
    }
    pub fn steering(&self) -> SteeringRevision {
        self.command.steering
    }
    fn lease_identity(&self) -> Option<(ControllerId, Revision, Revision)> {
        self.controller
            .as_ref()
            .map(|(connection, token)| (connection.clone(), token.generation(), token.revision()))
    }
}

/// Proof of the selected task's canonical pause, bound to one public admission.
/// The host must also fence and drain external work before committing steering.
#[derive(Debug)]
pub struct PublicPauseProof {
    command: CommandId,
    digest: String,
    task: Task,
    receipt: Option<CommandReceipt>,
    owner: ControllerId,
    epoch: OwnerEpoch,
    authority: AuthorityRevision,
    controller: Option<(ControllerId, Revision, Revision)>,
}
impl PublicPauseProof {
    pub fn receipt(&self) -> Option<&CommandReceipt> {
        self.receipt.as_ref()
    }
    pub fn revision(&self) -> Revision {
        self.task.revision
    }
}

fn public_error(error: crate::Error) -> PublicError {
    match error {
        crate::Error::Access => PublicError::Access,
        crate::Error::Owner
        | crate::Error::Target
        | crate::Error::Host
        | crate::Error::Domain(_)
        | crate::Error::Policy(_) => PublicError::StaleState,
        crate::Error::Protocol(_) | crate::Error::Json(_) => PublicError::InvalidParameters,
        crate::Error::Store(vcp_store::Error::Conflict(_)) => PublicError::StaleState,
        crate::Error::Store(_) => PublicError::OutcomeUnknown,
    }
}

impl<S: CanonicalStore> Engine<S> {
    pub async fn handle_public(
        &mut self,
        call: Call,
        access: &Access,
        host: &HostFacts,
    ) -> Result<CommandReceipt, PublicError> {
        match self.prepare_public(call, access, host)? {
            PublicAdmission::Replay(receipt) => Ok(receipt),
            PublicAdmission::Ready(prepared) => self.commit_public(prepared, access, host).await,
        }
    }

    pub fn prepare_public(
        &self,
        call: Call,
        access: &Access,
        host: &HostFacts,
    ) -> Result<PublicAdmission, PublicError> {
        self.prepare_public_inner(call, access, host, None)
    }

    pub fn prepare_controlled_public(
        &self,
        call: Call,
        access: &Access,
        host: &HostFacts,
        connection: &ControllerId,
        token: &ControllerToken,
    ) -> Result<PublicAdmission, PublicError> {
        self.check_controller(access, connection, token)
            .map_err(|_| PublicError::Access)?;
        let mut admission = self.prepare_public(call, access, host)?;
        if let PublicAdmission::Ready(prepared) = &mut admission {
            if matches!(prepared.call, Call::TurnSteer(_)) {
                let task = self.public_task(prepared, access)?;
                if task.state != TaskState::Paused {
                    // Both the trusted pause and steering must fit before the host holds work.
                    task.revision
                        .next()
                        .and_then(Revision::next)
                        .map_err(|_| PublicError::StaleState)?;
                }
            }
            prepared.controller = Some((connection.clone(), token.clone()));
        }
        Ok(admission)
    }

    fn prepare_public_inner(
        &self,
        call: Call,
        access: &Access,
        host: &HostFacts,
        paused_revision: Option<Revision>,
    ) -> Result<PublicAdmission, PublicError> {
        self.authorize(access).map_err(|_| PublicError::Access)?;
        if !access.write {
            return Err(PublicError::Access);
        }
        call.validate()
            .map_err(|_| PublicError::InvalidParameters)?;
        let (scope, mutation) = match &call {
            Call::SessionCreate(p) => (&p.scope, &p.mutation),
            Call::TurnSteer(p) => (&p.scope, &p.mutation),
            Call::ApprovalRespond(p) => (&p.scope, &p.mutation),
            _ => return Err(PublicError::CapabilityUnavailable),
        };
        if scope.workspace.as_str() != access.workspace.as_str()
            || scope.session.as_str() != access.session.as_str()
        {
            return Err(PublicError::Access);
        }
        let state = self.store().state();
        let workspace: Workspace = state
            .record(
                Collection::Workspace,
                access.workspace.as_str(),
                &access.workspace,
            )
            .map_err(|_| PublicError::Access)?
            .decode()
            .map_err(|_| PublicError::Unavailable)?;
        let session: Session = state
            .record(
                Collection::Session,
                access.session.as_str(),
                &access.workspace,
            )
            .map_err(|_| PublicError::Access)?
            .decode()
            .map_err(|_| PublicError::Unavailable)?;
        if workspace.id != access.workspace
            || session.id != access.session
            || session.workspace != access.workspace
        {
            return Err(PublicError::Access);
        }
        let id = CommandId::parse(mutation.command_id.as_str())
            .map_err(|_| PublicError::InvalidParameters)?;
        let digest = call
            .digest(access.actor.as_str())
            .map_err(|_| PublicError::InvalidParameters)?;
        // Access and scope precede lookup. Original parameters remain valid retry
        // identity even after their state revisions or engine owner have changed.
        if let Some(receipt) = state
            .command(&access.workspace, &id, &digest)
            .map_err(|_| PublicError::CommandConflict)?
        {
            return Ok(PublicAdmission::Replay(receipt));
        }
        let expected =
            paused_revision.unwrap_or(Revision::new(number(&mutation.expected_revision)?));
        let steering = SteeringRevision::new(number(&mutation.steering_revision)?);
        let (task, payload) = match &call {
            Call::SessionCreate(p) => {
                if expected != Revision::ZERO
                    || steering != SteeringRevision::ZERO
                    || number(&p.configuration_revision)? != session.configuration.get()
                {
                    return Err(PublicError::StaleState);
                }
                (
                    None,
                    Command::CreateSession {
                        id: SessionId::parse(p.new_session.as_str())
                            .map_err(|_| PublicError::InvalidParameters)?,
                        fork_through: None,
                    },
                )
            }
            Call::TurnSteer(p) => {
                let task_id =
                    TaskId::parse(p.task.as_str()).map_err(|_| PublicError::InvalidParameters)?;
                let task: Task = state
                    .record(Collection::Task, task_id.as_str(), &access.workspace)
                    .map_err(|_| PublicError::Unavailable)?
                    .decode()
                    .map_err(|_| PublicError::Unavailable)?;
                let turn: Turn = state
                    .record(Collection::Turn, p.turn.as_str(), &access.workspace)
                    .map_err(|_| PublicError::Unavailable)?
                    .decode()
                    .map_err(|_| PublicError::Unavailable)?;
                if task.scope.session != access.session
                    || task.scope.workspace != access.workspace
                    || turn.scope != task.scope
                {
                    return Err(PublicError::Unavailable);
                }
                if task.revision != expected
                    || task.steering != steering
                    || task.scope.task != task_id
                    || turn.id.as_str() != p.turn.as_str()
                    || turn.steering != steering
                    || matches!(
                        turn.state,
                        TurnState::Completed | TurnState::Failed | TurnState::Cancelled
                    )
                {
                    return Err(PublicError::StaleState);
                }
                let objective = Objective {
                    text: p.objective.clone(),
                    constraints: p.constraints.clone(),
                    acceptance: p.acceptance.clone(),
                    source: EventId::new(),
                    steering,
                };
                task.steer(expected, objective.clone())
                    .map_err(|_| PublicError::StaleState)?;
                (Some(task_id), Command::Steer { objective })
            }

            Call::ApprovalRespond(p) => {
                let task_id =
                    TaskId::parse(p.task.as_str()).map_err(|_| PublicError::InvalidParameters)?;
                let approval: Approval = state
                    .record(Collection::Approval, p.approval.as_str(), &access.workspace)
                    .map_err(|_| PublicError::Unavailable)?
                    .decode()
                    .map_err(|_| PublicError::Unavailable)?;
                if approval.scope.workspace != access.workspace
                    || approval.scope.session != access.session
                    || approval.scope.task != task_id
                {
                    return Err(PublicError::Unavailable);
                }
                let policy = crate::policy::optional(state, &access.workspace)
                    .map_err(|_| PublicError::Unavailable)?;
                let policy_revision = policy.map_or(PolicyRevision::ZERO, |policy| policy.revision);
                if policy_revision.get() != number(&p.policy_revision)?
                    || host.policy != policy_revision
                    || approval.policy != policy_revision
                    || approval.state != ApprovalState::Pending
                {
                    return Err(PublicError::StaleState);
                }
                (
                    Some(task_id),
                    Command::Decide {
                        id: ApprovalId::parse(p.approval.as_str())
                            .map_err(|_| PublicError::InvalidParameters)?,
                        operation_digest: p.operation_digest.clone(),
                        effect_revision: Revision::new(number(&p.effect_revision)?),
                        allow: matches!(p.decision, ApprovalDecision::Allow),
                    },
                )
            }
            _ => return Err(PublicError::CapabilityUnavailable),
        };
        let command = CommandEnvelope {
            version: 1,
            id,
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            task,
            caller: access.actor.clone(),
            controller: self.controller().clone(),
            owner_epoch: self.owner_epoch(),
            expected,
            steering,
            payload,
        };
        Ok(PublicAdmission::Ready(PreparedPublicCommand {
            call,
            digest,
            command,
            authority: access.authority,
            controller: None,
        }))
    }

    fn check_public_prepared(
        &self,
        prepared: &PreparedPublicCommand,
        access: &Access,
    ) -> Result<(), PublicError> {
        self.authorize(access).map_err(|_| PublicError::Access)?;
        if !access.write
            || access.actor != prepared.command.caller
            || access.workspace != prepared.command.workspace
            || access.session != prepared.command.session
            || access.authority != prepared.authority
        {
            return Err(PublicError::Access);
        }
        if prepared.command.controller != *self.controller()
            || prepared.command.owner_epoch != self.owner_epoch()
        {
            return Err(PublicError::StaleState);
        }
        if let Some((connection, token)) = &prepared.controller {
            self.check_controller(access, connection, token)
                .map_err(|_| PublicError::Access)?;
        }
        Ok(())
    }

    pub async fn commit_public(
        &mut self,
        prepared: PreparedPublicCommand,
        access: &Access,
        host: &HostFacts,
    ) -> Result<CommandReceipt, PublicError> {
        self.check_public_prepared(&prepared, access)?;
        if prepared.controller.is_some() && matches!(prepared.call, Call::TurnSteer(_)) {
            return Err(PublicError::CapabilityUnavailable);
        }
        match self.prepare_public(prepared.call, access, host)? {
            PublicAdmission::Replay(receipt) => Ok(receipt),
            PublicAdmission::Ready(current) => self
                .handle_with_digest(current.command, access, host, Some(current.digest))
                .await
                .map_err(public_error),
        }
    }

    fn public_task(
        &self,
        prepared: &PreparedPublicCommand,
        access: &Access,
    ) -> Result<Task, PublicError> {
        let task = prepared.task().ok_or(PublicError::InvalidParameters)?;
        self.store()
            .state()
            .record(Collection::Task, task.as_str(), &access.workspace)
            .map_err(|_| PublicError::Unavailable)?
            .decode()
            .map_err(|_| PublicError::Unavailable)
    }

    pub async fn pause_public_for_authority(
        &mut self,
        prepared: &PreparedPublicCommand,
        access: &Access,
        host: &HostFacts,
    ) -> Result<PublicPauseProof, PublicError> {
        self.check_public_prepared(prepared, access)?;
        if !matches!(prepared.call, Call::TurnSteer(_)) {
            return Err(PublicError::CapabilityUnavailable);
        }
        // Revalidate the original expected revision and objective before changing state.
        if !matches!(
            self.prepare_public(prepared.call.clone(), access, host)?,
            PublicAdmission::Ready(_)
        ) {
            return Err(PublicError::StaleState);
        }
        let before = self.public_task(prepared, access)?;
        let receipt = if before.state == TaskState::Paused {
            None
        } else {
            let mut pause = prepared.command.clone();
            pause.id = CommandId::new();
            pause.payload = Command::Transition {
                next: TaskState::Paused,
                reason: "authority change requires deliberate continuation".into(),
                verification: None,
            };
            Some(
                self.handle(pause, access, host)
                    .await
                    .map_err(public_error)?,
            )
        };
        let task = self.public_task(prepared, access)?;
        if task.state != TaskState::Paused || task.steering != prepared.steering() {
            return Err(PublicError::OutcomeUnknown);
        }
        Ok(PublicPauseProof {
            command: prepared.id().clone(),
            digest: prepared.digest.clone(),
            task,
            receipt,
            owner: prepared.command.controller.clone(),
            epoch: prepared.command.owner_epoch,
            authority: prepared.authority,
            controller: prepared.lease_identity(),
        })
    }

    pub async fn commit_public_after_pause(
        &mut self,
        prepared: PreparedPublicCommand,
        proof: PublicPauseProof,
        access: &Access,
        host: &HostFacts,
    ) -> Result<CommandReceipt, PublicError> {
        self.check_public_prepared(&prepared, access)?;
        if !matches!(prepared.call, Call::TurnSteer(_))
            || proof.command != *prepared.id()
            || proof.digest != prepared.digest
            || proof.owner != prepared.command.controller
            || proof.epoch != prepared.command.owner_epoch
            || proof.authority != prepared.authority
            || proof.controller != prepared.lease_identity()
        {
            return Err(PublicError::StaleState);
        }
        if let Some(receipt) = self
            .store()
            .state()
            .command(&access.workspace, prepared.id(), prepared.digest())
            .map_err(|_| PublicError::CommandConflict)?
        {
            return Ok(receipt);
        }
        if self.public_task(&prepared, access)? != proof.task {
            return Err(PublicError::StaleState);
        }
        let expected = if let Some(receipt) = &proof.receipt {
            if self
                .store()
                .state()
                .command(&access.workspace, &receipt.command, &receipt.digest)
                .map_err(|_| PublicError::StaleState)?
                .as_ref()
                != Some(receipt)
            {
                return Err(PublicError::StaleState);
            }
            prepared
                .expected()
                .next()
                .map_err(|_| PublicError::StaleState)?
        } else {
            prepared.expected()
        };
        if proof.task.revision != expected
            || proof.task.state != TaskState::Paused
            || proof.task.steering != prepared.steering()
        {
            return Err(PublicError::StaleState);
        }
        match self.prepare_public_inner(prepared.call, access, host, Some(expected))? {
            PublicAdmission::Replay(receipt) => Ok(receipt),
            PublicAdmission::Ready(current) => self
                .handle_with_digest(current.command, access, host, Some(current.digest))
                .await
                .map_err(public_error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vcp_domain::{
        artifact::{ArtifactSpec, Channel},
        verification::Fingerprint,
        workspace::{Binding, Scope},
    };
    use vcp_protocol::methods;
    use vcp_store::{artifact::ArtifactWriter, BackendKind, Store};

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
    fn id(value: &str) -> methods::Id {
        value.to_owned().try_into().unwrap()
    }
    fn scope() -> methods::Scope {
        methods::Scope {
            workspace: id("workspace"),
            session: id("session"),
        }
    }
    fn mutation(command: &str, expected: u64, steering: u64) -> methods::Mutation {
        methods::Mutation {
            command_id: id(command),
            expected_revision: expected.into(),
            steering_revision: steering.into(),
        }
    }
    fn facts() -> HostFacts {
        HostFacts {
            may_execute: true,
            ..HostFacts::inspect(Timestamp::new(10))
        }
    }
    fn objective(text: &str) -> Objective {
        Objective {
            text: text.into(),
            constraints: vec![],
            acceptance: vec![],
            source: EventId::new(),
            steering: SteeringRevision::ZERO,
        }
    }
    async fn internal(
        engine: &mut Engine<Store>,
        payload: Command,
        task: Option<TaskId>,
        expected: u64,
        steering: u64,
    ) -> CommandReceipt {
        let access = access();
        let envelope = CommandEnvelope {
            version: 1,
            id: CommandId::new(),
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            task,
            caller: access.actor.clone(),
            controller: engine.controller().clone(),
            owner_epoch: engine.owner_epoch(),
            expected: Revision::new(expected),
            steering: SteeringRevision::new(steering),
            payload,
        };
        engine.handle(envelope, &access, &facts()).await.unwrap()
    }
    async fn setup(path: &std::path::Path, backend: BackendKind) -> Engine<Store> {
        let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
        internal(
            &mut engine,
            Command::Initialize {
                binding: Binding {
                    host: HostId::parse("host").unwrap(),
                    root: "C:/public-fixture".into(),
                    repository: "fixture".into(),
                    worktree: "main".into(),
                    revision: Revision::ZERO,
                },
            },
            None,
            0,
            0,
        )
        .await;
        engine
    }
    async fn task(engine: &mut Engine<Store>) -> TaskId {
        let task = TaskId::parse("task").unwrap();
        internal(
            engine,
            Command::CreateTask {
                root: task.clone(),
                parent: None,
                fork_origin: None,
                objective: objective("original"),
                fingerprint: Fingerprint {
                    repository: "a".repeat(64),
                    buffers: "b".repeat(64),
                    environment: "c".repeat(64),
                },
                editing: false,
                required_checks: vec![],
            },
            Some(task.clone()),
            0,
            0,
        )
        .await;
        task
    }
    fn create_call(command: &str) -> Call {
        Call::SessionCreate(methods::SessionCreate {
            scope: scope(),
            mutation: mutation(command, 0, 0),
            new_session: id("created-session"),
            configuration_revision: 0.into(),
        })
    }

    fn ready(admission: PublicAdmission) -> PreparedPublicCommand {
        match admission {
            PublicAdmission::Ready(value) => value,
            _ => panic!("expected new admission"),
        }
    }

    fn steer_call(command: &str, revision: u64) -> Call {
        Call::TurnSteer(methods::TurnSteer {
            scope: scope(),
            mutation: mutation(command, revision, 0),
            task: id("task"),
            turn: id("turn"),
            objective: "controlled objective".into(),
            constraints: vec![],
            acceptance: vec![],
        })
    }

    #[tokio::test]
    async fn controlled_pause_preserves_identity_and_fences_stale_admission() {
        for backend in [BackendKind::Sqlite, BackendKind::Files] {
            let temp = tempfile::tempdir().unwrap();
            let mut engine = setup(temp.path(), backend).await;
            let connection = ControllerId::new();
            engine
                .acquire_controller(&access(), &connection, CommandId::new(), None, facts().now)
                .await
                .unwrap();
            let token = engine.controller_token(&access(), &connection).unwrap();
            let task = task(&mut engine).await;
            start_turn(&mut engine, &task).await;
            let current: Task = engine
                .store()
                .state()
                .record(Collection::Task, task.as_str(), &access().workspace)
                .unwrap()
                .decode()
                .unwrap();
            let watermark = engine.store().state().watermark;
            assert!(matches!(
                engine.prepare_controlled_public(
                    steer_call("stale", 0),
                    &access(),
                    &facts(),
                    &connection,
                    &token
                ),
                Err(PublicError::StaleState)
            ));
            assert_eq!(engine.store().state().watermark, watermark);
            let call = steer_call("controlled-steer", current.revision.get());
            let prepared = ready(
                engine
                    .prepare_controlled_public(
                        call.clone(),
                        &access(),
                        &facts(),
                        &connection,
                        &token,
                    )
                    .unwrap(),
            );
            assert_eq!(
                engine.commit_public(prepared, &access(), &facts()).await,
                Err(PublicError::CapabilityUnavailable)
            );
            assert_eq!(engine.store().state().watermark, watermark);
            let prepared = ready(
                engine
                    .prepare_controlled_public(
                        call.clone(),
                        &access(),
                        &facts(),
                        &connection,
                        &token,
                    )
                    .unwrap(),
            );
            let proof = engine
                .pause_public_for_authority(&prepared, &access(), &facts())
                .await
                .unwrap();
            assert!(proof.receipt().is_some());
            assert_eq!(proof.revision(), current.revision.next().unwrap());
            let digest = prepared.digest().to_owned();
            let receipt = engine
                .commit_public_after_pause(prepared, proof, &access(), &facts())
                .await
                .unwrap();
            assert_eq!(receipt.digest, digest);
            assert_eq!(receipt.command.as_str(), "controlled-steer");
            let after: Task = engine
                .store()
                .state()
                .record(Collection::Task, task.as_str(), &access().workspace)
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(after.state, TaskState::Paused);
            assert_eq!(after.objectives.len(), current.objectives.len() + 1);
            let watermark = engine.store().state().watermark;
            assert!(matches!(
                engine
                    .prepare_controlled_public(
                        call.clone(),
                        &access(),
                        &facts(),
                        &connection,
                        &token
                    )
                    .unwrap(),
                PublicAdmission::Replay(_)
            ));
            assert_eq!(engine.store().state().watermark, watermark);
            engine.into_store().close().await.unwrap();
            let mut engine =
                Engine::new(Store::open(temp.path(), backend, &[]).await.unwrap()).unwrap();
            assert_eq!(
                engine
                    .handle_public(call, &access(), &facts())
                    .await
                    .unwrap(),
                receipt
            );
            assert_eq!(engine.store().state().watermark, watermark);
            engine.into_store().close().await.unwrap();
        }
    }

    #[tokio::test]
    async fn controlled_proof_cannot_survive_release_or_denied_current_access() {
        for backend in [BackendKind::Sqlite, BackendKind::Files] {
            let temp = tempfile::tempdir().unwrap();
            let mut engine = setup(temp.path(), backend).await;
            let connection = ControllerId::new();
            engine
                .acquire_controller(&access(), &connection, CommandId::new(), None, facts().now)
                .await
                .unwrap();
            let token = engine.controller_token(&access(), &connection).unwrap();
            let task = task(&mut engine).await;
            start_turn(&mut engine, &task).await;
            let current: Task = engine
                .store()
                .state()
                .record(Collection::Task, task.as_str(), &access().workspace)
                .unwrap()
                .decode()
                .unwrap();
            let call = steer_call("lost-controller", current.revision.get());
            let prepared = ready(
                engine
                    .prepare_controlled_public(call, &access(), &facts(), &connection, &token)
                    .unwrap(),
            );
            let mut observer = access();
            observer.write = false;
            let watermark = engine.store().state().watermark;
            assert!(matches!(
                engine
                    .pause_public_for_authority(&prepared, &observer, &facts())
                    .await,
                Err(PublicError::Access)
            ));
            assert_eq!(engine.store().state().watermark, watermark);
            let proof = engine
                .pause_public_for_authority(&prepared, &access(), &facts())
                .await
                .unwrap();
            engine
                .release_controller(
                    &access(),
                    &connection,
                    CommandId::new(),
                    &token,
                    token.revision(),
                    vcp_domain::controller::Reason::Released,
                    facts().now,
                )
                .await
                .unwrap();
            let watermark = engine.store().state().watermark;
            assert_eq!(
                engine
                    .commit_public_after_pause(prepared, proof, &access(), &facts())
                    .await,
                Err(PublicError::Access)
            );
            assert_eq!(engine.store().state().watermark, watermark);
            engine.into_store().close().await.unwrap();
        }
    }

    #[tokio::test]
    async fn pause_proof_rejects_other_identity_and_changed_task_and_owner() {
        for backend in [BackendKind::Sqlite, BackendKind::Files] {
            let temp = tempfile::tempdir().unwrap();
            let mut engine = setup(temp.path(), backend).await;
            let task = task(&mut engine).await;
            start_turn(&mut engine, &task).await;
            let current: Task = engine
                .store()
                .state()
                .record(Collection::Task, task.as_str(), &access().workspace)
                .unwrap()
                .decode()
                .unwrap();
            let first = ready(
                engine
                    .prepare_public(
                        steer_call("first", current.revision.get()),
                        &access(),
                        &facts(),
                    )
                    .unwrap(),
            );
            let other = ready(
                engine
                    .prepare_public(
                        steer_call("other", current.revision.get()),
                        &access(),
                        &facts(),
                    )
                    .unwrap(),
            );
            let proof = engine
                .pause_public_for_authority(&first, &access(), &facts())
                .await
                .unwrap();
            let paused = proof.revision();
            let watermark = engine.store().state().watermark;
            assert_eq!(
                engine
                    .commit_public_after_pause(other, proof, &access(), &facts())
                    .await,
                Err(PublicError::StaleState)
            );
            assert_eq!(engine.store().state().watermark, watermark);
            let prepared = ready(
                engine
                    .prepare_public(
                        steer_call("already-paused", paused.get()),
                        &access(),
                        &facts(),
                    )
                    .unwrap(),
            );
            let proof = engine
                .pause_public_for_authority(&prepared, &access(), &facts())
                .await
                .unwrap();
            assert!(proof.receipt().is_none());
            assert_eq!(engine.store().state().watermark, watermark);
            engine
                .handle_public(steer_call("intervening", paused.get()), &access(), &facts())
                .await
                .unwrap();
            let watermark = engine.store().state().watermark;
            assert_eq!(
                engine
                    .commit_public_after_pause(prepared, proof, &access(), &facts())
                    .await,
                Err(PublicError::StaleState)
            );
            assert_eq!(engine.store().state().watermark, watermark);
            let prepared = ready(
                engine
                    .prepare_public(create_call("stale-owner"), &access(), &facts())
                    .unwrap(),
            );
            engine.into_store().close().await.unwrap();
            let mut engine =
                Engine::new(Store::open(temp.path(), backend, &[]).await.unwrap()).unwrap();
            assert_eq!(
                engine.commit_public(prepared, &access(), &facts()).await,
                Err(PublicError::StaleState)
            );
            engine.into_store().close().await.unwrap();
        }
    }

    #[tokio::test]
    async fn public_identity_replays_after_restart_and_rechecks_access_on_both_stores() {
        for backend in [BackendKind::Sqlite, BackendKind::Files] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("public");
            let mut engine = setup(&root, backend).await;
            let call = create_call("stable-command");
            let receipt = engine
                .handle_public(call.clone(), &access(), &facts())
                .await
                .unwrap();
            assert_eq!(
                receipt.digest,
                call.digest(access().actor.as_str()).unwrap()
            );
            let watermark = engine.store().state().watermark;
            assert_eq!(
                engine
                    .handle_public(call.clone(), &access(), &facts())
                    .await
                    .unwrap(),
                receipt
            );
            let mut changed = call.clone();
            if let Call::SessionCreate(p) = &mut changed {
                p.configuration_revision = 1.into();
            }
            assert_eq!(
                engine.handle_public(changed, &access(), &facts()).await,
                Err(PublicError::CommandConflict)
            );
            let mut observer = access();
            observer.write = false;
            assert_eq!(
                engine
                    .handle_public(call.clone(), &observer, &facts())
                    .await,
                Err(PublicError::Access)
            );
            let mut stale = access();
            stale.authority = AuthorityRevision::new(1);
            assert_eq!(
                engine.handle_public(call.clone(), &stale, &facts()).await,
                Err(PublicError::Access)
            );
            let mut different_actor = access();
            different_actor.actor = ActorId::new();
            assert_eq!(
                engine
                    .handle_public(call.clone(), &different_actor, &facts())
                    .await,
                Err(PublicError::CommandConflict)
            );
            assert_eq!(engine.store().state().watermark, watermark);
            let old_controller = engine.controller().clone();
            engine.into_store().close().await.unwrap();
            let mut reopened =
                Engine::new(Store::open(&root, backend, &[]).await.unwrap()).unwrap();
            assert_ne!(*reopened.controller(), old_controller);
            assert_eq!(
                reopened
                    .handle_public(call, &access(), &facts())
                    .await
                    .unwrap(),
                receipt
            );
            assert_eq!(reopened.store().state().watermark, watermark);

            let mut direct = setup(&temp.path().join("internal"), backend).await;
            internal(
                &mut direct,
                Command::CreateSession {
                    id: SessionId::parse("created-session").unwrap(),
                    fork_through: None,
                },
                None,
                0,
                0,
            )
            .await;
            let row = |engine: &Engine<Store>| {
                engine
                    .store()
                    .state()
                    .record(Collection::Session, "created-session", &access().workspace)
                    .unwrap()
                    .value
                    .clone()
            };
            assert_eq!(row(&reopened), row(&direct));
            assert_eq!(
                reopened.store().state().events.len(),
                direct.store().state().events.len()
            );
            reopened.into_store().close().await.unwrap();
            direct.into_store().close().await.unwrap();
        }
    }

    #[tokio::test]
    async fn concurrent_public_duplicate_has_one_durable_effect_and_invalid_methods_do_not_mutate()
    {
        for backend in [BackendKind::Sqlite, BackendKind::Files] {
            let temp = tempfile::tempdir().unwrap();
            let engine =
                std::sync::Arc::new(tokio::sync::Mutex::new(setup(temp.path(), backend).await));
            let call = create_call("concurrent-command");
            let invoke = |engine: std::sync::Arc<tokio::sync::Mutex<Engine<Store>>>, call: Call| async move {
                engine
                    .lock()
                    .await
                    .handle_public(call, &access(), &facts())
                    .await
            };
            let (first, second) = tokio::join!(
                invoke(engine.clone(), call.clone()),
                invoke(engine.clone(), call)
            );
            assert_eq!(first.unwrap(), second.unwrap());
            let mut locked = engine.lock().await;
            assert_eq!(
                locked
                    .store()
                    .state()
                    .records
                    .values()
                    .filter(|row| row.collection == Collection::Session)
                    .count(),
                2
            );
            assert_eq!(locked.store().state().commands.len(), 2);
            let watermark = locked.store().state().watermark;
            let unsupported = Call::TaskCancel(methods::TaskCancel {
                scope: scope(),
                mutation: mutation("unsafe-bare-transition", 0, 0),
                task: id("task"),
                reason: "stop".into(),
            });
            assert_eq!(
                locked.handle_public(unsupported, &access(), &facts()).await,
                Err(PublicError::CapabilityUnavailable)
            );
            let mut wrong_scope = create_call("wrong-scope");
            if let Call::SessionCreate(p) = &mut wrong_scope {
                p.scope.session = id("created-session");
            }
            assert_eq!(
                locked.handle_public(wrong_scope, &access(), &facts()).await,
                Err(PublicError::Access)
            );
            let mut wrong_config = create_call("wrong-configuration");
            if let Call::SessionCreate(p) = &mut wrong_config {
                p.configuration_revision = 1.into();
            }
            assert_eq!(
                locked
                    .handle_public(wrong_config, &access(), &facts())
                    .await,
                Err(PublicError::StaleState)
            );
            assert_eq!(locked.store().state().watermark, watermark);
            drop(locked);
            std::sync::Arc::try_unwrap(engine)
                .ok()
                .unwrap()
                .into_inner()
                .into_store()
                .close()
                .await
                .unwrap();
        }
    }

    async fn start_turn(engine: &mut Engine<Store>, task: &TaskId) {
        internal(
            engine,
            Command::Transition {
                next: vcp_domain::task::TaskState::Running,
                reason: "fixture".into(),
                verification: None,
            },
            Some(task.clone()),
            0,
            0,
        )
        .await;
        let mut writer = engine
            .store()
            .spool()
            .create(ArtifactSpec {
                id: ArtifactId::parse("trigger").unwrap(),
                scope: Scope {
                    workspace: access().workspace,
                    session: access().session,
                    task: task.clone(),
                },
                media_type: "text/plain".into(),
                schema: "turn-trigger/1".into(),
                source: "fixture".into(),
                channel: Channel::Evidence,
                retention: "history".into(),
                omissions: vec![],
            })
            .unwrap();
        writer.write_chunk(b"fixture trigger").unwrap();
        let descriptor = writer.finalize().unwrap();
        internal(
            engine,
            Command::AttachArtifact { descriptor },
            Some(task.clone()),
            0,
            0,
        )
        .await;
        internal(
            engine,
            Command::StartTurn {
                id: TurnId::parse("turn").unwrap(),
                trigger: ArtifactId::parse("trigger").unwrap(),
            },
            Some(task.clone()),
            1,
            0,
        )
        .await;
    }

    fn normalize(mut value: serde_json::Value) -> serde_json::Value {
        fn visit(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Object(map) => {
                    map.remove("cause");
                    map.remove("source");
                    for value in map.values_mut() {
                        visit(value);
                    }
                }
                serde_json::Value::Array(values) => {
                    for value in values {
                        visit(value);
                    }
                }
                _ => {}
            }
        }
        visit(&mut value);
        value
    }

    #[tokio::test]
    async fn steering_matches_internal_semantics_and_retry_does_not_steer_twice() {
        for backend in [BackendKind::Sqlite, BackendKind::Files] {
            let temp = tempfile::tempdir().unwrap();
            let mut public = setup(&temp.path().join("public"), backend).await;
            let mut direct = setup(&temp.path().join("direct"), backend).await;
            let public_task = task(&mut public).await;
            let direct_task = task(&mut direct).await;
            start_turn(&mut public, &public_task).await;
            start_turn(&mut direct, &direct_task).await;
            let current: Task = public
                .store()
                .state()
                .record(Collection::Task, public_task.as_str(), &access().workspace)
                .unwrap()
                .decode()
                .unwrap();
            let call = Call::TurnSteer(methods::TurnSteer {
                scope: scope(),
                mutation: mutation("steer-once", current.revision.get(), current.steering.get()),
                task: id("task"),
                turn: id("turn"),
                objective: "new objective".into(),
                constraints: vec![],
                acceptance: vec![],
            });
            let receipt = public
                .handle_public(call.clone(), &access(), &facts())
                .await
                .unwrap();
            internal(
                &mut direct,
                Command::Steer {
                    objective: objective("new objective"),
                },
                Some(direct_task),
                current.revision.get(),
                0,
            )
            .await;
            for (collection, id) in [(Collection::Task, "task"), (Collection::Turn, "turn")] {
                let value = |engine: &Engine<Store>| {
                    normalize(
                        engine
                            .store()
                            .state()
                            .record(collection, id, &access().workspace)
                            .unwrap()
                            .value
                            .clone(),
                    )
                };
                assert_eq!(value(&public), value(&direct));
            }
            let watermark = public.store().state().watermark;
            assert_eq!(
                public
                    .handle_public(call.clone(), &access(), &facts())
                    .await
                    .unwrap(),
                receipt
            );
            let mut stale = call;
            if let Call::TurnSteer(p) = &mut stale {
                p.mutation.command_id = id("stale-steer");
            }
            assert_eq!(
                public.handle_public(stale, &access(), &facts()).await,
                Err(PublicError::StaleState)
            );
            assert_eq!(public.store().state().watermark, watermark);
            assert_eq!(
                public
                    .store()
                    .state()
                    .records
                    .values()
                    .filter(|row| row.collection == Collection::Effect)
                    .count(),
                0
            );
            public.into_store().close().await.unwrap();
            direct.into_store().close().await.unwrap();
        }
    }

    #[tokio::test]
    async fn approval_revision_expiry_and_replay_use_canonical_boundaries() {
        for backend in [BackendKind::Sqlite, BackendKind::Files] {
            let temp = tempfile::tempdir().unwrap();
            let mut engine = setup(temp.path(), backend).await;
            let task = task(&mut engine).await;
            let effect = ToolRunId::parse("effect").unwrap();
            internal(
                &mut engine,
                Command::ProposeEffect {
                    id: effect.clone(),
                    operation_digest: "a".repeat(64),
                },
                Some(task.clone()),
                0,
                0,
            )
            .await;
            let approval = Approval {
                id: ApprovalId::parse("approval").unwrap(),
                scope: Scope {
                    workspace: access().workspace,
                    session: access().session,
                    task: task.clone(),
                },
                effect,
                effect_revision: Revision::ZERO,
                steering: SteeringRevision::ZERO,
                operation_digest: "a".repeat(64),
                actor: access().actor,
                policy: PolicyRevision::ZERO,
                expires_at: Timestamp::new(20),
                state: ApprovalState::Pending,
                revision: Revision::ZERO,
                controller: Some(engine.controller().clone()),
                owner_epoch: Some(engine.owner_epoch()),
                authority: Some(AuthorityRevision::ZERO),
                binding: Some(Revision::ZERO),
            };
            internal(&mut engine, Command::Ask { approval }, Some(task), 0, 0).await;
            let call = Call::ApprovalRespond(methods::ApprovalRespond {
                scope: scope(),
                mutation: mutation("decision", 0, 0),
                task: id("task"),
                approval: id("approval"),
                operation_digest: "a".repeat(64),
                effect_revision: 0.into(),
                policy_revision: 0.into(),
                decision: ApprovalDecision::Deny,
            });
            let before = engine.store().state().watermark;
            let mut stale = call.clone();
            if let Call::ApprovalRespond(p) = &mut stale {
                p.effect_revision = 1.into();
            }
            assert_eq!(
                engine.handle_public(stale, &access(), &facts()).await,
                Err(PublicError::StaleState)
            );
            let mut stale = call.clone();
            if let Call::ApprovalRespond(p) = &mut stale {
                p.policy_revision = 1.into();
            }
            assert_eq!(
                engine.handle_public(stale, &access(), &facts()).await,
                Err(PublicError::StaleState)
            );
            let expired = HostFacts::inspect(Timestamp::new(20));
            assert_eq!(
                engine
                    .handle_public(call.clone(), &access(), &expired)
                    .await,
                Err(PublicError::StaleState)
            );
            assert_eq!(engine.store().state().watermark, before);
            let receipt = engine
                .handle_public(call.clone(), &access(), &facts())
                .await
                .unwrap();
            assert_eq!(
                engine
                    .handle_public(call.clone(), &access(), &expired)
                    .await
                    .unwrap(),
                receipt
            );
            let mut repeated = call;
            if let Call::ApprovalRespond(p) = &mut repeated {
                p.mutation.command_id = id("new-decision-again");
            }
            assert_eq!(
                engine.handle_public(repeated, &access(), &facts()).await,
                Err(PublicError::StaleState)
            );
            let approval: Approval = engine
                .store()
                .state()
                .record(Collection::Approval, "approval", &access().workspace)
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(approval.state, ApprovalState::Denied);
            assert_eq!(
                engine
                    .store()
                    .state()
                    .records
                    .values()
                    .filter(|row| row.collection == Collection::Effect)
                    .count(),
                1
            );
            engine.into_store().close().await.unwrap();
        }
    }
}
