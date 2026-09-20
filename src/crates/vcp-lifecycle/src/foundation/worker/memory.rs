// SPDX-License-Identifier: Apache-2.0
use super::*;

impl Context {
    pub fn promote_memory_response(
        &mut self,
        context: vcp_memory::extraction::ExtractionContext,
    ) -> Result<Vec<vcp_memory::repository::MemoryCommit>> {
        if !self.owner_alive || self.authority_pending || self.interrupted_capture {
            return Err("memory extraction is fenced".into());
        }
        let root: Task = self
            .engine
            .store()
            .state()
            .record(
                Collection::Task,
                self.config.root_task.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        let attempt: Attempt = self
            .engine
            .store()
            .state()
            .record(
                Collection::Attempt,
                context.attempt.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        if attempt.root != self.config.root_task {
            return Err("memory response belongs to another canonical owner root".into());
        }
        if root.state != TaskState::Running
            || context
                .maintenance_root
                .as_ref()
                .is_some_and(|id| id != &root.scope.task)
        {
            return Err(
                "memory extraction requires the active root or its explicit maintenance budget"
                    .into(),
            );
        }
        let access = self.memory_access();
        let candidates = match vcp_memory::extraction::validate(
            self.engine.store(),
            &access,
            &context,
        ) {
            Ok(candidates) => candidates,
            Err(error) => {
                let data = serde_json::json!({"schema_version":1,"component":"memory_extraction",
                    "status":"validation_failed","attempt_id":attempt.id,"output_artifact":context.output_artifact,
                    "extractor":context.extractor,"reason":"captured candidate output failed current scope, accounting, evidence or schema validation"});
                if !self
                    .engine
                    .store()
                    .state()
                    .events
                    .iter()
                    .any(|event| event.event.data == data)
                {
                    let tx = Transaction {
                        id: TransactionId::new(),
                        expected_watermark: self.engine.store().state().watermark,
                        mutations: vec![],
                        command: None,
                        events: vec![vcp_protocol::event::EventInput {
                            id: EventId::new(),
                            workspace: attempt.scope.workspace.clone(),
                            session: attempt.scope.session.clone(),
                            task: Some(attempt.scope.task.clone()),
                            actor: self.config.actor.clone(),
                            correlation: CommandId::new(),
                            causation: None,
                            timestamp: now(),
                            kind: vcp_protocol::event::EventKind::Diagnostic,
                            artifacts: vec![],
                            data,
                            metadata: None,
                        }],
                    };
                    self.runtime
                        .block_on(self.engine.store_mut().transact(tx))?;
                }
                return Err(error.into());
            }
        };
        let mut commits = Vec::new();
        for proposal in candidates {
            commits.push(self.runtime.block_on(vcp_memory::repository::propose(
                self.engine.store_mut(),
                &access,
                proposal,
                now(),
            ))?);
        }
        Ok(commits)
    }

    pub fn memory_access(&self) -> vcp_memory::access::Access {
        vcp_memory::access::Access {
            workspace: self.config.workspace.clone(),
            actor: self.config.actor.clone(),
            authority: self.access.authority,
            read: true,
            write: true,
            tasks: None,
        }
    }

    pub fn memory_step(&mut self) -> Result<vcp_memory::runner::Progress> {
        if !self.owner_alive || self.authority_pending || self.interrupted_capture {
            return Err("memory maintenance is fenced".into());
        }
        let Some(record) = self
            .engine
            .store()
            .state()
            .records
            .get(&key(Collection::Task, self.config.root_task.as_str()))
        else {
            return Ok(vcp_memory::runner::Progress::default());
        };
        let root: Task = record.decode()?;
        if !matches!(
            root.state,
            TaskState::Running | TaskState::Completed | TaskState::Failed
        ) {
            return Ok(vcp_memory::runner::Progress::default());
        }
        let access = self.memory_access();
        Ok(self.runtime.block_on(vcp_memory::runner::step(
            self.engine.store_mut(),
            &access,
            &root.scope,
            now(),
        ))?)
    }

    /// Preserve an admitted command's receipt even if independent maintenance
    /// fails. The failure remains canonical and cannot masquerade as progress.
    pub(super) fn memory_after_command(&mut self) {
        if !self.owner_alive || self.authority_pending || self.interrupted_capture {
            return;
        }
        if self.memory_step().is_err() {
            let event = vcp_protocol::event::EventInput {
                id: EventId::new(),
                workspace: self.config.workspace.clone(),
                session: self.config.session.clone(),
                task: Some(self.config.root_task.clone()),
                actor: self.config.actor.clone(),
                correlation: CommandId::new(),
                causation: None,
                timestamp: now(),
                kind: vcp_protocol::event::EventKind::Diagnostic,
                artifacts: vec![],
                data: serde_json::json!({"schema_version":1,"component":"memory_ingestion",
                    "status":"maintenance_failed","reason":"bounded ingestion step failed; inspect retained queue and canonical source state before retry"}),
                metadata: None,
            };
            let tx = Transaction {
                id: TransactionId::new(),
                expected_watermark: self.engine.store().state().watermark,
                mutations: vec![],
                events: vec![event],
                command: None,
            };
            if self
                .runtime
                .block_on(self.engine.store_mut().transact(tx))
                .is_err()
            {
                eprintln!("memory maintenance diagnostic could not be retained");
            }
        }
    }
}
