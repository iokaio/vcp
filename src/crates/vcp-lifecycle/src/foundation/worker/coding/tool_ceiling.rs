// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::foundation::coding::CanonicalTools;

fn validate_history(recorded: Option<&CanonicalTools>, prior: &[CanonicalTools]) -> Result<()> {
    let effective = recorded.cloned().unwrap_or_default();
    if prior.iter().any(|tools| tools != &effective) {
        return Err(
            "canonical tool ceiling is missing or differs from recorded coding history".into(),
        );
    }
    Ok(())
}

impl Context {
    fn recorded_canonical_tools(&self) -> Result<(Option<CanonicalTools>, bool)> {
        let scope = Scope {
            workspace: self.config.workspace.clone(),
            session: self.config.session.clone(),
            task: self.config.root_task.clone(),
        };
        let mut recorded: Option<CanonicalTools> = None;
        let mut prior = Vec::new();
        for record in self
            .engine
            .store()
            .state()
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
        {
            let descriptor: ArtifactDescriptor = record.decode()?;
            if descriptor.spec.scope == scope
                && descriptor.spec.schema == "canonical-tool-ceiling/1"
            {
                if recorded.is_some() {
                    return Err("ambiguous canonical tool ceiling".into());
                }
                recorded = Some(serde_json::from_slice(
                    &self.coding_artifact(&descriptor.spec.id)?,
                )?);
            }
            if descriptor.spec.scope.workspace == scope.workspace
                && descriptor.spec.scope.session == scope.session
                && descriptor.spec.schema == "canonical-coding-configuration/1"
            {
                let task: Task = self
                    .engine
                    .store()
                    .state()
                    .record(
                        Collection::Task,
                        descriptor.spec.scope.task.as_str(),
                        &scope.workspace,
                    )?
                    .decode()?;
                if task.root != scope.task {
                    continue;
                }
                let config: serde_json::Value =
                    serde_json::from_slice(&self.coding_artifact(&descriptor.spec.id)?)?;
                let config = config
                    .as_object()
                    .ok_or("invalid historical coding configuration object")?;
                prior.push(
                    config
                        .get("canonical_tools")
                        .map(|value| serde_json::from_value::<CanonicalTools>(value.clone()))
                        .transpose()?
                        .unwrap_or_default(),
                );
            }
        }
        validate_history(recorded.as_ref(), &prior)?;
        Ok((recorded, !prior.is_empty()))
    }
    pub fn configure_canonical_tools(&mut self, tools: CanonicalTools) -> Result<()> {
        if !self.owner_alive
            || self.authority_pending
            || !self.coding.is_empty()
            || !self.streams.is_empty()
        {
            return Err("canonical tool ceiling requires fresh idle owner setup".into());
        }
        let (recorded, has_history) = self.recorded_canonical_tools()?;
        if let Some(recorded) = recorded {
            if recorded != tools {
                return Err("canonical tool ceiling differs from original accepted task".into());
            }
        } else {
            if !tools.is_all() && has_history {
                return Err("legacy coding history retains its original default tool ceiling; start a new task to narrow it".into());
            }
            let scope = Scope {
                workspace: self.config.workspace.clone(),
                session: self.config.session.clone(),
                task: self.config.root_task.clone(),
            };
            self.capture(
                &scope,
                Channel::Evidence,
                &canonical_bytes(&tools)?,
                "canonical-tool-ceiling/1",
            )?;
        }
        Ok(())
    }
    pub fn canonical_tools_for(&self, task: &TaskId) -> Result<CanonicalTools> {
        let current: Task = self
            .engine
            .store()
            .state()
            .record(Collection::Task, task.as_str(), &self.config.workspace)?
            .decode()?;
        if current.root != self.config.root_task || current.scope.session != self.config.session {
            return Err("canonical tool ceiling task scope mismatch".into());
        }
        // Legacy tasks without this artifact retain the established seven-tool ceiling.
        Ok(self.recorded_canonical_tools()?.0.unwrap_or_default())
    }
    pub fn startup_canonical_tools(&mut self, task: &TaskId) -> Result<CanonicalTools> {
        let tools = self.canonical_tools_for(task)?;
        if self.recorded_canonical_tools()?.0.is_none() {
            self.configure_canonical_tools(tools.clone())?;
        }
        Ok(tools)
    }
    pub(super) fn require_coding_tool(&self, binding: &ThreadBinding, name: &str) -> Result<()> {
        let state = self
            .coding
            .get(&binding.scope.task)
            .ok_or("coding setup missing")?;
        if !state.config.canonical_tools.contains(name) {
            return Err("tool is outside the owner's canonical model tool ceiling".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_tool_ceiling_rejects_missing_or_changed_restricted_history() {
        let narrow: CanonicalTools =
            serde_json::from_value(serde_json::json!(["vcp_read"])).unwrap();
        assert!(validate_history(None, &[narrow.clone()]).is_err());
        assert!(validate_history(Some(&CanonicalTools::default()), &[narrow.clone()]).is_err());
        assert!(validate_history(Some(&narrow), &[CanonicalTools::default()]).is_err());
        assert!(validate_history(Some(&narrow), &[narrow.clone()]).is_ok());
        assert!(validate_history(None, &[CanonicalTools::default()]).is_ok());
        assert!(validate_history(None, &[]).is_ok());
    }
}
