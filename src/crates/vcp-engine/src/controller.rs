// SPDX-License-Identifier: Apache-2.0
//! Persisted local controller ownership, separate from execution authorization.
//!
//! The authenticated host supplies `Access` and a connection identity, never wire
//! assertions. Before acquisition or release it must fence admission and drain
//! owned work through the lifecycle owner. The canonical non-Running check below
//! is necessary, not proof that external processes or provider requests drained.
//! A lease does not resume work or replace policy/budget/effect admission.
use crate::{query::Query, Access, Engine};
use serde::Serialize;
use vcp_domain::{
    controller::{Holder, Lease, Reason, DOCUMENT_TYPE},
    ids::*,
    revision::*,
    task::{Task, TaskState},
};
use vcp_protocol::{
    canonical_bytes,
    command::{CommandReceipt, CommandResult},
    digest_bytes,
    event::{EventInput, EventKind},
};
use vcp_store::contract::{
    controller_lease_id, key, CanonicalStore, Collection, Mutation, ReceiptInput, Record,
    Transaction,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ControllerError {
    #[error("current controller access denied")]
    Access,
    #[error("another connection or process holds the controller lease")]
    Held,
    #[error("controller lease or revision is stale")]
    Stale,
    #[error("controller command identity was reused with different semantics")]
    CommandConflict,
    #[error("canonical session tasks must stop running before ownership changes")]
    RunningTasks,
    #[error("invalid controller operation")]
    InvalidOperation,
    #[error("canonical controller state is unavailable or invalid")]
    InvalidState,
    #[error("controller command outcome is unknown; reconcile original identity")]
    OutcomeUnknown,
}

type Result<T> = std::result::Result<T, ControllerError>;

/// A host-local proof of an observed holder. Fields are private, and this type
/// cannot be deserialized. Every use must call `Engine::check_controller` again.
/// Cloning a token never extends its lifetime or grants a second connection.
#[derive(Clone, Debug)]
pub struct ControllerToken {
    workspace: WorkspaceId,
    session: SessionId,
    actor: ActorId,
    connection: ControllerId,
    authority: AuthorityRevision,
    generation: Revision,
    revision: Revision,
    process_owner: ControllerId,
    owner_epoch: OwnerEpoch,
}

impl ControllerToken {
    pub fn generation(&self) -> Revision {
        self.generation
    }
    pub fn revision(&self) -> Revision {
        self.revision
    }
}

#[derive(Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum Operation {
    Acquire {
        expected_revision: Option<Revision>,
    },
    Release {
        expected_revision: Revision,
        generation: Revision,
        reason: Reason,
    },
    Recover {
        expected_revision: Revision,
        generation: Revision,
    },
}

impl<S: CanonicalStore> Engine<S> {
    fn controller_access(&self, access: &Access) -> Result<()> {
        if !access.write {
            return Err(ControllerError::Access);
        }
        // Shared queries validate actual workspace/session bindings as well as
        // current read authority; bootstrap is never enough to acquire a lease.
        self.query(
            access,
            &Query::Session {
                session: access.session.clone(),
            },
        )
        .map_err(|_| ControllerError::Access)?;
        Ok(())
    }

    fn load_controller(&self, access: &Access) -> Result<Option<Lease>> {
        let id = controller_lease_id(&access.workspace, &access.session)
            .map_err(|_| ControllerError::InvalidState)?;
        let Some(record) = self
            .store()
            .state()
            .records
            .get(&key(Collection::Access, &id))
        else {
            return Ok(None);
        };
        let lease: Lease = record.decode().map_err(|_| ControllerError::InvalidState)?;
        lease
            .validate()
            .map_err(|_| ControllerError::InvalidState)?;
        if record.workspace != access.workspace
            || record.revision != lease.revision
            || record.id != id
            || lease.workspace != access.workspace
            || lease.session != access.session
            || lease.id != id
        {
            return Err(ControllerError::InvalidState);
        }
        Ok(Some(lease))
    }

    /// Current scoped lease facts, for a host already authorized to control this
    /// session. Inspecting them neither acquires ownership nor mints a token.
    pub fn controller_lease(&self, access: &Access) -> Result<Option<Lease>> {
        self.controller_access(access)?;
        self.load_controller(access)
    }

    /// Read-only scoped ownership facts. This never constructs a controller token.
    pub fn read_controller(&self, access: &Access) -> Result<Option<Lease>> {
        self.query(
            access,
            &Query::Session {
                session: access.session.clone(),
            },
        )
        .map_err(|_| ControllerError::Access)?;
        self.load_controller(access)
    }

    /// Check replay before a live host changes admission. A replayed acquisition
    /// must not mint a refreshed token after policy changes or lease release.
    pub fn controller_acquire_receipt(
        &self,
        access: &Access,
        connection: &ControllerId,
        command: &CommandId,
        expected: Option<Revision>,
    ) -> Result<Option<CommandReceipt>> {
        self.controller_access(access)?;
        let digest = self.controller_digest(
            access,
            connection,
            &Operation::Acquire {
                expected_revision: expected,
            },
        )?;
        self.controller_receipt(access, command, &digest)
    }

    /// Access-before-replay validation lets the live host return an already
    /// durable explicit release without holding the retained owner a second time.
    pub fn controller_release_receipt(
        &self,
        access: &Access,
        connection: &ControllerId,
        command: &CommandId,
        expected: Revision,
        generation: Revision,
    ) -> Result<Option<CommandReceipt>> {
        self.controller_access(access)?;
        let digest = self.controller_digest(
            access,
            connection,
            &Operation::Release {
                expected_revision: expected,
                generation,
                reason: Reason::Released,
            },
        )?;
        self.controller_receipt(access, command, &digest)
    }

    /// Reconciliation remains available after a separate acquisition. Returning
    /// this receipt neither releases nor renews the current process's lease.
    pub fn controller_recover_receipt(
        &self,
        access: &Access,
        connection: &ControllerId,
        command: &CommandId,
        expected: Revision,
        generation: Revision,
    ) -> Result<Option<CommandReceipt>> {
        self.controller_access(access)?;
        let digest = self.controller_digest(
            access,
            connection,
            &Operation::Recover {
                expected_revision: expected,
                generation,
            },
        )?;
        self.controller_receipt(access, command, &digest)
    }

    fn controller_digest(
        &self,
        access: &Access,
        connection: &ControllerId,
        operation: &Operation,
    ) -> Result<String> {
        canonical_bytes(&serde_json::json!({
            "domain":"vcp-controller-lease/1", "actor":access.actor,
            "connection":connection,"workspace":access.workspace,"session":access.session,"operation":operation
        })).map(|bytes|digest_bytes(&bytes)).map_err(|_|ControllerError::InvalidOperation)
    }

    fn controller_receipt(
        &self,
        access: &Access,
        command: &CommandId,
        digest: &str,
    ) -> Result<Option<CommandReceipt>> {
        self.store()
            .state()
            .command(&access.workspace, command, digest)
            .map_err(|_| ControllerError::CommandConflict)
    }

    fn controller_quiescent(&self, access: &Access) -> Result<()> {
        for record in self
            .store()
            .state()
            .records
            .values()
            .filter(|r| r.collection == Collection::Task && r.workspace == access.workspace)
        {
            // Decode failures fail closed, even before deciding session scope.
            let task: Task = record.decode().map_err(|_| ControllerError::InvalidState)?;
            if task.scope.workspace != access.workspace || task.scope.task.as_str() != record.id {
                return Err(ControllerError::InvalidState);
            }
            if task.scope.session == access.session && task.state == TaskState::Running {
                return Err(ControllerError::RunningTasks);
            }
        }
        Ok(())
    }

    /// Acquire only an unheld, quiescent session at the expected lease revision.
    /// Even an exact duplicate returns only its receipt. Obtain a current token
    /// separately; replaying an old acquisition never revives ownership.
    pub async fn acquire_controller(
        &mut self,
        access: &Access,
        connection: &ControllerId,
        command: CommandId,
        expected: Option<Revision>,
        now: Timestamp,
    ) -> Result<CommandReceipt> {
        self.controller_access(access)?;
        let digest = self.controller_digest(
            access,
            connection,
            &Operation::Acquire {
                expected_revision: expected,
            },
        )?;
        if let Some(receipt) = self.controller_receipt(access, &command, &digest)? {
            return Ok(receipt);
        }
        let current = self.load_controller(access)?;
        if current
            .as_ref()
            .and_then(|lease| lease.holder.as_ref())
            .is_some()
        {
            return Err(ControllerError::Held);
        }
        if current.as_ref().map(|lease| lease.revision) != expected {
            return Err(ControllerError::Stale);
        }
        self.controller_quiescent(access)?;
        let lease = Lease {
            document_type: DOCUMENT_TYPE.into(),
            schema_version: 1,
            id: controller_lease_id(&access.workspace, &access.session)
                .map_err(|_| ControllerError::InvalidState)?,
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            revision: current
                .as_ref()
                .map_or(Ok(Revision::ZERO), |lease| lease.revision.next())
                .map_err(|_| ControllerError::InvalidState)?,
            generation: current
                .as_ref()
                .map_or(Ok(Revision::new(1)), |lease| lease.generation.next())
                .map_err(|_| ControllerError::InvalidState)?,
            holder: Some(Holder {
                actor: access.actor.clone(),
                connection: connection.clone(),
                process_owner: self.controller().clone(),
                owner_epoch: self.owner_epoch(),
            }),
            reason: Reason::Acquired,
        };
        self.commit_controller(access, command, digest, current.as_ref(), lease, now)
            .await
    }

    /// Mint from current state only. Connection identity must come from the
    /// authenticated transport, not a caller-selected request field.
    pub fn controller_token(
        &self,
        access: &Access,
        connection: &ControllerId,
    ) -> Result<ControllerToken> {
        self.controller_access(access)?;
        let lease = self
            .load_controller(access)?
            .ok_or(ControllerError::Stale)?;
        let holder = lease.holder.as_ref().ok_or(ControllerError::Stale)?;
        if holder.actor != access.actor
            || &holder.connection != connection
            || &holder.process_owner != self.controller()
            || holder.owner_epoch != self.owner_epoch()
        {
            return Err(ControllerError::Stale);
        }
        Ok(ControllerToken {
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            actor: access.actor.clone(),
            connection: connection.clone(),
            authority: access.authority,
            generation: lease.generation,
            revision: lease.revision,
            process_owner: holder.process_owner.clone(),
            owner_epoch: holder.owner_epoch,
        })
    }

    fn token_scope(
        access: &Access,
        connection: &ControllerId,
        token: &ControllerToken,
    ) -> Result<()> {
        if token.workspace != access.workspace
            || token.session != access.session
            || token.actor != access.actor
            || &token.connection != connection
        {
            return Err(ControllerError::Access);
        }
        Ok(())
    }

    /// Recheck immediately at mutation admission. A receipt is not a token, and
    /// a token is not an executable effect grant or a resume instruction.
    pub fn check_controller(
        &self,
        access: &Access,
        connection: &ControllerId,
        token: &ControllerToken,
    ) -> Result<()> {
        self.controller_access(access)?;
        Self::token_scope(access, connection, token)?;
        let current = self.controller_token(access, connection)?;
        if token.authority != current.authority
            || token.generation != current.generation
            || token.revision != current.revision
            || token.process_owner != current.process_owner
            || token.owner_epoch != current.owner_epoch
        {
            return Err(ControllerError::Stale);
        }
        Ok(())
    }

    /// Explicit release or known connection loss, after host admission fencing
    /// and drain. Transfer is release followed by a separate acquisition.
    #[allow(clippy::too_many_arguments)]
    pub async fn release_controller(
        &mut self,
        access: &Access,
        connection: &ControllerId,
        command: CommandId,
        token: &ControllerToken,
        expected: Revision,
        reason: Reason,
        now: Timestamp,
    ) -> Result<CommandReceipt> {
        self.controller_access(access)?;
        Self::token_scope(access, connection, token)?;
        if !matches!(reason, Reason::Released | Reason::ConnectionLost) {
            return Err(ControllerError::InvalidOperation);
        }
        let digest = self.controller_digest(
            access,
            connection,
            &Operation::Release {
                expected_revision: expected,
                generation: token.generation,
                reason,
            },
        )?;
        if let Some(receipt) = self.controller_receipt(access, &command, &digest)? {
            return Ok(receipt);
        }
        self.check_controller(access, connection, token)?;
        let current = self
            .load_controller(access)?
            .ok_or(ControllerError::Stale)?;
        if current.revision != expected {
            return Err(ControllerError::Stale);
        }
        self.controller_quiescent(access)?;
        let mut next = current.clone();
        next.revision = next
            .revision
            .next()
            .map_err(|_| ControllerError::InvalidState)?;
        next.holder = None;
        next.reason = reason;
        self.commit_controller(access, command, digest, Some(&current), next, now)
            .await
    }

    /// Recover only a previous process's held lease. The authenticated host must
    /// first fence/drain/reconcile its lifecycle; this does not infer liveness
    /// from a pathname or PID and cannot replace canonical writer-lock ownership.
    /// Recovery releases; a distinct acquisition and deliberate resume follow.
    pub async fn recover_controller(
        &mut self,
        access: &Access,
        connection: &ControllerId,
        command: CommandId,
        expected: Revision,
        generation: Revision,
        now: Timestamp,
    ) -> Result<CommandReceipt> {
        self.controller_access(access)?;
        let digest = self.controller_digest(
            access,
            connection,
            &Operation::Recover {
                expected_revision: expected,
                generation,
            },
        )?;
        if let Some(receipt) = self.controller_receipt(access, &command, &digest)? {
            return Ok(receipt);
        }
        let current = self
            .load_controller(access)?
            .ok_or(ControllerError::Stale)?;
        if current.revision != expected || current.generation != generation {
            return Err(ControllerError::Stale);
        }
        let holder = current.holder.as_ref().ok_or(ControllerError::Stale)?;
        if &holder.process_owner == self.controller() && holder.owner_epoch == self.owner_epoch() {
            return Err(ControllerError::Held);
        }
        self.controller_quiescent(access)?;
        let mut next = current.clone();
        next.revision = next
            .revision
            .next()
            .map_err(|_| ControllerError::InvalidState)?;
        next.holder = None;
        next.reason = Reason::ProcessLost;
        self.commit_controller(access, command, digest, Some(&current), next, now)
            .await
    }

    async fn commit_controller(
        &mut self,
        access: &Access,
        command: CommandId,
        digest: String,
        previous: Option<&Lease>,
        next: Lease,
        now: Timestamp,
    ) -> Result<CommandReceipt> {
        next.validate().map_err(|_| ControllerError::InvalidState)?;
        if let Some(previous) = previous {
            previous
                .validate_transition(&next)
                .map_err(|_| ControllerError::InvalidState)?;
        }
        let record = Record::typed(
            Collection::Access,
            next.id.clone(),
            access.workspace.clone(),
            next.revision,
            &next,
        )
        .map_err(|_| ControllerError::InvalidState)?;
        let event = EventInput {
            id: EventId::new(),
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            task: None,
            actor: access.actor.clone(),
            correlation: command.clone(),
            causation: None,
            timestamp: now,
            kind: EventKind::AccessChanged,
            artifacts: vec![],
            metadata: None,
            data: serde_json::json!({"schema_version":1,"facts":[{"collection":record.collection,"id":record.id,"revision":record.revision,"value":record.value}]}),
        };
        let transaction = Transaction {
            id: TransactionId::new(),
            expected_watermark: self.store().state().watermark,
            mutations: vec![Mutation::Put {
                expected: previous.map(|lease| lease.revision),
                record,
            }],
            events: vec![event],
            command: Some(ReceiptInput {
                command,
                workspace: access.workspace.clone(),
                session: access.session.clone(),
                digest,
                result: CommandResult::Accepted {
                    revision: next.revision,
                },
            }),
        };
        self.store_mut()
            .transact(transaction)
            .await
            .map_err(|_| ControllerError::OutcomeUnknown)?
            .command
            .ok_or(ControllerError::OutcomeUnknown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HostFacts;
    use vcp_domain::{
        task::{Objective, ResumeEvidence},
        verification::Fingerprint,
        workspace::Binding,
    };
    use vcp_protocol::command::{Command, CommandEnvelope};
    use vcp_store::{BackendKind, Store};

    fn access() -> Access {
        Access {
            actor: ActorId::parse("lease-owner").unwrap(),
            workspace: WorkspaceId::parse("workspace").unwrap(),
            session: SessionId::parse("session").unwrap(),
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            bootstrap: true,
        }
    }
    fn connection(name: &str) -> ControllerId {
        ControllerId::parse(name).unwrap()
    }

    #[tokio::test]
    async fn observer_facts_and_release_replay_validate_current_access_before_receipts() {
        let temporary = tempfile::tempdir().unwrap();
        let mut engine = setup(temporary.path(), BackendKind::Files).await;
        let owner = access();
        let connection = connection("controller");
        let acquired = engine
            .acquire_controller(&owner, &connection, command("acquire"), None, now())
            .await
            .unwrap();
        let mut observer = access();
        observer.write = false;
        assert!(engine.read_controller(&observer).unwrap().is_some());
        assert!(matches!(
            engine.controller_acquire_receipt(&observer, &connection, &command("acquire"), None),
            Err(ControllerError::Access)
        ));
        assert_eq!(
            engine
                .controller_acquire_receipt(&owner, &connection, &command("acquire"), None)
                .unwrap(),
            Some(acquired)
        );
        let token = engine.controller_token(&owner, &connection).unwrap();
        let released = engine
            .release_controller(
                &owner,
                &connection,
                command("release"),
                &token,
                token.revision(),
                Reason::Released,
                now(),
            )
            .await
            .unwrap();
        let watermark = engine.store().state().watermark;
        assert_eq!(
            engine
                .controller_release_receipt(
                    &owner,
                    &connection,
                    &command("release"),
                    token.revision(),
                    token.generation()
                )
                .unwrap(),
            Some(released)
        );
        assert!(matches!(
            engine.controller_release_receipt(
                &observer,
                &connection,
                &command("release"),
                token.revision(),
                token.generation()
            ),
            Err(ControllerError::Access)
        ));
        assert!(matches!(
            engine.controller_release_receipt(
                &owner,
                &connection,
                &command("release"),
                token.revision(),
                Revision::new(2)
            ),
            Err(ControllerError::CommandConflict)
        ));
        observer.authority = AuthorityRevision::new(1);
        assert!(matches!(
            engine.read_controller(&observer),
            Err(ControllerError::Access)
        ));
        assert_eq!(engine.store().state().watermark, watermark);
        engine.into_store().close().await.unwrap();
    }
    fn command(name: &str) -> CommandId {
        CommandId::parse(name).unwrap()
    }
    fn now() -> Timestamp {
        Timestamp::new(10)
    }

    async fn internal(engine: &mut Engine<Store>, payload: Command, task: Option<TaskId>) {
        let access = access();
        let current: Option<Task> = task.as_ref().and_then(|id| {
            engine
                .store()
                .state()
                .record(Collection::Task, id.as_str(), &access.workspace)
                .ok()
                .map(|r| r.decode().unwrap())
        });
        let envelope = CommandEnvelope {
            version: 1,
            id: CommandId::new(),
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            task,
            caller: access.actor.clone(),
            controller: engine.controller().clone(),
            owner_epoch: engine.owner_epoch(),
            expected: current.as_ref().map_or(Revision::ZERO, |t| t.revision),
            steering: current
                .as_ref()
                .map_or(SteeringRevision::ZERO, |t| t.steering),
            payload,
        };
        let host = HostFacts {
            now: now(),
            policy: PolicyRevision::ZERO,
            may_execute: true,
            resume: Some(ResumeEvidence {
                workspace_current: true,
                policy_current: true,
                budget_current: true,
                effects_reconciled: true,
                owner_current: true,
            }),
        };
        engine.handle(envelope, &access, &host).await.unwrap();
    }
    async fn setup(path: &std::path::Path, backend: BackendKind) -> Engine<Store> {
        let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
        internal(
            &mut engine,
            Command::Initialize {
                binding: Binding {
                    host: HostId::parse("host").unwrap(),
                    root: "C:/lease-fixture".into(),
                    repository: "fixture".into(),
                    worktree: "main".into(),
                    revision: Revision::ZERO,
                },
            },
            None,
        )
        .await;
        engine
    }
    async fn task(engine: &mut Engine<Store>) -> TaskId {
        let id = TaskId::parse("task").unwrap();
        internal(
            engine,
            Command::CreateTask {
                root: id.clone(),
                parent: None,
                fork_origin: None,
                objective: Objective {
                    text: "lease fixture".into(),
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
            Some(id.clone()),
        )
        .await;
        id
    }
    async fn transition(engine: &mut Engine<Store>, task: &TaskId, next: TaskState) {
        internal(
            engine,
            Command::Transition {
                next,
                reason: "host fixture lifecycle boundary".into(),
                verification: None,
            },
            Some(task.clone()),
        )
        .await;
    }

    #[tokio::test]
    async fn competing_connections_and_observers_cannot_acquire_or_replay_authority() {
        for backend in [BackendKind::Files, BackendKind::Sqlite] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("store");
            let mut engine = setup(&root, backend).await;
            // Recovery cannot be used by another simultaneous canonical writer.
            assert!(Store::open(&root, backend, &[]).await.is_err());
            let a = connection("connection-a");
            let b = connection("connection-b");
            let acquired = engine
                .acquire_controller(&access(), &a, command("acquire"), None, now())
                .await
                .unwrap();
            let token = engine.controller_token(&access(), &a).unwrap();
            assert_eq!(token.generation(), Revision::new(1));
            assert_eq!(token.revision(), Revision::ZERO);
            engine.check_controller(&access(), &a, &token).unwrap();
            let before = engine.store().state().watermark;
            assert_eq!(
                engine
                    .acquire_controller(&access(), &a, command("acquire"), None, now())
                    .await
                    .unwrap(),
                acquired
            );
            assert_eq!(
                engine
                    .acquire_controller(
                        &access(),
                        &b,
                        command("competitor"),
                        Some(Revision::ZERO),
                        now()
                    )
                    .await,
                Err(ControllerError::Held)
            );
            assert_eq!(
                engine
                    .acquire_controller(&access(), &b, command("acquire"), None, now())
                    .await,
                Err(ControllerError::CommandConflict)
            );
            assert!(engine.controller_token(&access(), &b).is_err());
            assert_eq!(
                engine.check_controller(&access(), &b, &token),
                Err(ControllerError::Access)
            );
            let mut observer = access();
            observer.write = false;
            assert_eq!(
                engine
                    .acquire_controller(&observer, &a, command("acquire"), None, now())
                    .await,
                Err(ControllerError::Access)
            );
            assert!(engine.controller_token(&observer, &a).is_err());
            assert_eq!(
                engine.check_controller(&observer, &a, &token),
                Err(ControllerError::Access)
            );
            let mut revoked = access();
            revoked.read = false;
            assert_eq!(
                engine
                    .acquire_controller(&revoked, &a, command("acquire"), None, now())
                    .await,
                Err(ControllerError::Access)
            );
            let mut stale = access();
            stale.authority = AuthorityRevision::new(1);
            assert_eq!(
                engine.check_controller(&stale, &a, &token),
                Err(ControllerError::Access)
            );
            let mut actor = access();
            actor.actor = ActorId::parse("different-actor").unwrap();
            assert_eq!(
                engine
                    .acquire_controller(&actor, &a, command("acquire"), None, now())
                    .await,
                Err(ControllerError::CommandConflict)
            );
            assert!(engine.controller_token(&actor, &a).is_err());
            assert_eq!(engine.store().state().watermark, before);
            let events: Vec<_> = engine
                .store()
                .state()
                .events
                .iter()
                .filter(|e| e.event.correlation == command("acquire"))
                .collect();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].event.kind, EventKind::AccessChanged);
            assert_eq!(events[0].event.data["schema_version"], 1);
            let fact: Lease =
                serde_json::from_value(events[0].event.data["facts"][0]["value"].clone()).unwrap();
            assert_eq!(fact, engine.controller_lease(&access()).unwrap().unwrap());
            engine.into_store().close().await.unwrap();
        }
    }

    #[tokio::test]
    async fn refreshed_access_does_not_refresh_a_previously_issued_token() {
        for backend in [BackendKind::Files, BackendKind::Sqlite] {
            let temp = tempfile::tempdir().unwrap();
            let mut engine = setup(&temp.path().join("store"), backend).await;
            let connection = connection("authority-connection");
            engine
                .acquire_controller(&access(), &connection, command("acquire"), None, now())
                .await
                .unwrap();
            let old = engine.controller_token(&access(), &connection).unwrap();
            internal(
                &mut engine,
                Command::SetWorkspaceTrust {
                    trust: vcp_domain::workspace::Trust::Trusted,
                },
                None,
            )
            .await;
            let mut refreshed = access();
            let workspace: vcp_domain::workspace::Workspace = engine
                .store()
                .state()
                .record(
                    Collection::Workspace,
                    refreshed.workspace.as_str(),
                    &refreshed.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            refreshed.authority = workspace.authority;
            assert_ne!(refreshed.authority, old.authority);
            assert_eq!(
                engine.check_controller(&refreshed, &connection, &old),
                Err(ControllerError::Stale)
            );
            let fresh = engine.controller_token(&refreshed, &connection).unwrap();
            engine
                .check_controller(&refreshed, &connection, &fresh)
                .unwrap();
            assert_eq!(fresh.generation(), old.generation());
            engine.into_store().close().await.unwrap();
        }
    }

    #[tokio::test]
    async fn release_acquire_generation_and_old_receipts_never_recreate_ownership() {
        for backend in [BackendKind::Files, BackendKind::Sqlite] {
            let temp = tempfile::tempdir().unwrap();
            let mut engine = setup(&temp.path().join("store"), backend).await;
            let a = connection("connection-a");
            let b = connection("connection-b");
            let acquired = engine
                .acquire_controller(&access(), &a, command("acquire-a"), None, now())
                .await
                .unwrap();
            let token = engine.controller_token(&access(), &a).unwrap();
            let released = engine
                .release_controller(
                    &access(),
                    &a,
                    command("release-a"),
                    &token,
                    Revision::ZERO,
                    Reason::Released,
                    now(),
                )
                .await
                .unwrap();
            let before = engine.store().state().watermark;
            assert_eq!(
                engine
                    .release_controller(
                        &access(),
                        &a,
                        command("release-a"),
                        &token,
                        Revision::ZERO,
                        Reason::Released,
                        now()
                    )
                    .await
                    .unwrap(),
                released
            );
            assert_eq!(
                engine
                    .acquire_controller(&access(), &a, command("acquire-a"), None, now())
                    .await
                    .unwrap(),
                acquired
            );
            assert!(engine.controller_token(&access(), &a).is_err());
            assert!(engine.check_controller(&access(), &a, &token).is_err());
            assert_eq!(engine.store().state().watermark, before);
            assert_eq!(
                engine
                    .release_controller(
                        &access(),
                        &a,
                        command("release-a"),
                        &token,
                        Revision::ZERO,
                        Reason::ConnectionLost,
                        now()
                    )
                    .await,
                Err(ControllerError::CommandConflict)
            );
            let mut observer = access();
            observer.write = false;
            assert_eq!(
                engine
                    .release_controller(
                        &observer,
                        &a,
                        command("release-a"),
                        &token,
                        Revision::ZERO,
                        Reason::Released,
                        now()
                    )
                    .await,
                Err(ControllerError::Access)
            );
            engine
                .acquire_controller(
                    &access(),
                    &b,
                    command("acquire-b"),
                    Some(Revision::new(1)),
                    now(),
                )
                .await
                .unwrap();
            let next = engine.controller_token(&access(), &b).unwrap();
            assert_eq!(next.generation(), Revision::new(2));
            assert_eq!(next.revision(), Revision::new(2));
            let before = engine.store().state().watermark;
            assert_eq!(
                engine
                    .acquire_controller(&access(), &a, command("acquire-a"), None, now())
                    .await
                    .unwrap(),
                acquired
            );
            assert_eq!(
                engine
                    .release_controller(
                        &access(),
                        &a,
                        command("release-a"),
                        &token,
                        Revision::ZERO,
                        Reason::Released,
                        now()
                    )
                    .await
                    .unwrap(),
                released
            );
            engine.check_controller(&access(), &b, &next).unwrap();
            assert!(engine.controller_token(&access(), &a).is_err());
            assert_eq!(engine.store().state().watermark, before);
            engine
                .release_controller(
                    &access(),
                    &b,
                    command("lost-b"),
                    &next,
                    next.revision(),
                    Reason::ConnectionLost,
                    now(),
                )
                .await
                .unwrap();
            assert_eq!(
                engine.controller_lease(&access()).unwrap().unwrap().reason,
                Reason::ConnectionLost
            );
            engine.into_store().close().await.unwrap();
        }
    }

    #[tokio::test]
    async fn restart_recovery_requires_quiescence_and_a_separate_new_acquisition() {
        for backend in [BackendKind::Files, BackendKind::Sqlite] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("store");
            let mut engine = setup(&root, backend).await;
            let task = task(&mut engine).await;
            transition(&mut engine, &task, TaskState::Running).await;
            let a = connection("connection-a");
            let b = connection("connection-b");
            assert_eq!(
                engine
                    .acquire_controller(&access(), &a, command("acquire-a"), None, now())
                    .await,
                Err(ControllerError::RunningTasks)
            );
            transition(&mut engine, &task, TaskState::Paused).await;
            let acquired = engine
                .acquire_controller(&access(), &a, command("acquire-a"), None, now())
                .await
                .unwrap();
            let old_token = engine.controller_token(&access(), &a).unwrap();
            assert_eq!(
                engine
                    .recover_controller(
                        &access(),
                        &b,
                        command("recover"),
                        Revision::ZERO,
                        Revision::new(1),
                        now()
                    )
                    .await,
                Err(ControllerError::Held)
            );
            transition(&mut engine, &task, TaskState::Running).await;
            assert_eq!(
                engine
                    .release_controller(
                        &access(),
                        &a,
                        command("release"),
                        &old_token,
                        Revision::ZERO,
                        Reason::Released,
                        now()
                    )
                    .await,
                Err(ControllerError::RunningTasks)
            );
            engine.into_store().close().await.unwrap();
            let mut engine = Engine::new(Store::open(&root, backend, &[]).await.unwrap()).unwrap();
            assert!(engine.check_controller(&access(), &a, &old_token).is_err());
            assert!(engine.controller_token(&access(), &a).is_err());
            let watermark = engine.store().state().watermark;
            assert_eq!(
                engine
                    .acquire_controller(&access(), &a, command("acquire-a"), None, now())
                    .await
                    .unwrap(),
                acquired
            );
            assert_eq!(engine.store().state().watermark, watermark);
            assert_eq!(
                engine
                    .acquire_controller(
                        &access(),
                        &b,
                        command("acquire-b"),
                        Some(Revision::ZERO),
                        now()
                    )
                    .await,
                Err(ControllerError::Held)
            );
            assert_eq!(
                engine
                    .recover_controller(
                        &access(),
                        &b,
                        command("recover"),
                        Revision::ZERO,
                        Revision::new(1),
                        now()
                    )
                    .await,
                Err(ControllerError::RunningTasks)
            );
            transition(&mut engine, &task, TaskState::Paused).await;
            let recovered = engine
                .recover_controller(
                    &access(),
                    &b,
                    command("recover"),
                    Revision::ZERO,
                    Revision::new(1),
                    now(),
                )
                .await
                .unwrap();
            assert!(engine.controller_token(&access(), &b).is_err());
            let released = engine.controller_lease(&access()).unwrap().unwrap();
            assert_eq!(released.reason, Reason::ProcessLost);
            assert!(released.holder.is_none());
            assert_eq!(released.generation, Revision::new(1));
            engine
                .acquire_controller(
                    &access(),
                    &b,
                    command("acquire-b"),
                    Some(released.revision),
                    now(),
                )
                .await
                .unwrap();
            let token = engine.controller_token(&access(), &b).unwrap();
            assert_eq!(token.generation(), Revision::new(2));
            let watermark = engine.store().state().watermark;
            assert_eq!(
                engine
                    .recover_controller(
                        &access(),
                        &b,
                        command("recover"),
                        Revision::ZERO,
                        Revision::new(1),
                        now()
                    )
                    .await
                    .unwrap(),
                recovered
            );
            assert_eq!(engine.store().state().watermark, watermark);
            engine.check_controller(&access(), &b, &token).unwrap();
            let stored: Task = engine
                .store()
                .state()
                .record(Collection::Task, task.as_str(), &access().workspace)
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(
                stored.state,
                TaskState::Paused,
                "ownership changes must not resume work"
            );
            engine.into_store().close().await.unwrap();
        }
    }

    #[tokio::test]
    async fn session_scopes_are_independent_and_tokens_cannot_cross_them() {
        for backend in [BackendKind::Files, BackendKind::Sqlite] {
            let temp = tempfile::tempdir().unwrap();
            let mut engine = setup(&temp.path().join("store"), backend).await;
            let connection = connection("connection");
            engine
                .acquire_controller(&access(), &connection, command("first"), None, now())
                .await
                .unwrap();
            let first = engine.controller_token(&access(), &connection).unwrap();
            let second = SessionId::parse("second-session").unwrap();
            internal(
                &mut engine,
                Command::CreateSession {
                    id: second.clone(),
                    fork_through: None,
                },
                None,
            )
            .await;
            let mut other = access();
            other.session = second;
            assert_eq!(
                engine.check_controller(&other, &connection, &first),
                Err(ControllerError::Access)
            );
            assert!(engine.controller_token(&other, &connection).is_err());
            engine
                .acquire_controller(&other, &connection, command("second"), None, now())
                .await
                .unwrap();
            let second = engine.controller_token(&other, &connection).unwrap();
            engine
                .check_controller(&other, &connection, &second)
                .unwrap();
            engine
                .check_controller(&access(), &connection, &first)
                .unwrap();
            assert_ne!(
                engine.controller_lease(&access()).unwrap().unwrap().id,
                engine.controller_lease(&other).unwrap().unwrap().id
            );
            let before = engine.store().state().watermark;
            assert_eq!(
                engine
                    .acquire_controller(&other, &connection, command("first"), None, now())
                    .await,
                Err(ControllerError::CommandConflict)
            );
            assert_eq!(engine.store().state().watermark, before);
            engine.into_store().close().await.unwrap();
        }
    }
}
