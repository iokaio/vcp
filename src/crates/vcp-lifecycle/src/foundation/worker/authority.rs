// SPDX-License-Identifier: Apache-2.0
use super::*;
impl Context {
    pub fn check_external_command(&self, command: &Command) -> Result<()> {
        if self.authority_pending
            && !matches!(
                command,
                Command::AdvanceEffect { .. } | Command::AttachArtifact { .. }
            )
        {
            return Err("authority change is stopping work".into());
        }
        Ok(())
    }
    pub fn validate_authority_change(
        &self,
        command: &Command,
        task: &Option<TaskId>,
        expected: Revision,
    ) -> Result<()> {
        if !self.owner_alive
            || self.authority_pending
            || !super::super::authority::changes_authority(command)
        {
            return Err("authority change unavailable or unsupported command".into());
        }
        let (collection, id) = match command {
            Command::Steer { .. } => (
                Collection::Task,
                task.as_ref().ok_or("steering needs task")?.as_str(),
            ),
            Command::SetPolicy { .. } => (Collection::Access, self.config.workspace.as_str()),
            Command::SetGrant { grant } => (Collection::Access, grant.id.as_str()),
            _ => (Collection::Workspace, self.config.workspace.as_str()),
        };
        if !matches!(command, Command::Steer { .. } | Command::SetGrant { .. }) && task.is_some() {
            return Err("authority command has an unexpected task target".into());
        }
        let revision = self
            .engine
            .store()
            .state()
            .records
            .get(&key(collection, id))
            .map(|r| r.revision)
            .unwrap_or(Revision::ZERO);
        if revision != expected {
            return Err("stale authority command revision".into());
        }
        if let Command::Steer { objective } = command {
            let current: Task = self
                .engine
                .store()
                .state()
                .record(Collection::Task, id, &self.config.workspace)?
                .decode()?;
            current.steer(expected, objective.clone())?;
        }
        Ok(())
    }
    pub fn mark_authority_pending(&mut self) {
        self.authority_pending = true;
    }
    pub fn clear_authority_pending(&mut self) {
        self.authority_pending = false;
    }
    pub fn pause_for_authority(
        &mut self,
        command: &Command,
        selected: &Option<TaskId>,
        expected: Revision,
    ) -> Result<Revision> {
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
        self.capture(&root.scope, Channel::Evidence, &canonical_bytes(&serde_json::json!({"command":command,"task":selected,"expected":expected,"state":"stopping; command not applied"}))?, "vcp-authority-change-intent/1")?;
        let tasks: Vec<Task> = self
            .engine
            .store()
            .state()
            .records
            .values()
            .filter(|r| r.collection == Collection::Task)
            .map(Record::decode)
            .collect::<std::result::Result<_, _>>()?;
        let mut adjusted = expected;
        for task in tasks {
            if !task.state.terminal() && task.state != TaskState::Paused {
                self.command(
                    Command::Transition {
                        next: TaskState::Paused,
                        reason: "authority change requires deliberate continuation".into(),
                        verification: None,
                    },
                    Some(task.scope.task.clone()),
                    task.revision,
                )?;
                if matches!(command, Command::Steer { .. })
                    && selected.as_ref() == Some(&task.scope.task)
                {
                    adjusted = task.revision.next()?;
                }
            }
        }
        Ok(adjusted)
    }
    pub fn finish_authority_change(
        &mut self,
        command: Command,
        task: Option<TaskId>,
        expected: Revision,
    ) -> Result<CommandReceipt> {
        if !self.owner_alive || !self.authority_pending {
            return Err("authority owner changed while stopping".into());
        }
        let result = self.command(command, task, expected);
        self.authority_pending = false;
        result
    }
}
