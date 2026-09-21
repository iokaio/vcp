// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_repository::Root;

impl Context {
    pub fn configure_instruction_roots(
        &mut self,
        binding: &ThreadBinding,
        parents: Vec<Root>,
    ) -> Result<()> {
        self.can_start(binding)?;
        let state = self
            .coding
            .get(&binding.scope.task)
            .ok_or("coding setup missing")?;
        if parents.len() > 32
            || state.instruction_parents.is_some()
            || state.revisions.is_some()
            || !state.calls.is_empty()
            || self.verification.contains_key(&binding.scope.task)
        {
            return Err(
                "parent instruction grants require fresh coding setup before verification".into(),
            );
        }
        self.validate_instruction_roots(binding, &parents)?;
        // The existing loader rejects duplicate identities and non-ancestors,
        // checks native directory identity, and reads only AGENTS.md.
        self.task_root(&binding.scope.task)?.instructions(
            &state.config.affected_paths,
            &parents,
            256 * 1024,
        )?;
        let grants: Vec<_> = parents.iter().map(|root| {
            serde_json::json!({"identity":root.identity,"path":root.path(),"access":"AGENTS.md only"})
        }).collect();
        self.capture(
            &binding.scope,
            Channel::Evidence,
            &canonical_bytes(&grants)?,
            "canonical-parent-instruction-grants/1",
        )?;
        self.coding
            .get_mut(&binding.scope.task)
            .unwrap()
            .instruction_parents = Some(parents);
        Ok(())
    }

    pub(crate) fn instruction_parents(&self, binding: &ThreadBinding) -> Result<Vec<Root>> {
        let parents = self
            .coding
            .get(&binding.scope.task)
            .and_then(|state| state.instruction_parents.as_ref())
            .cloned()
            .unwrap_or_default();
        self.validate_instruction_roots(binding, &parents)?;
        Ok(parents)
    }

    pub(crate) fn validate_instruction_roots(
        &self,
        binding: &ThreadBinding,
        parents: &[Root],
    ) -> Result<()> {
        for root in parents {
            if root.identity.workspace != binding.scope.workspace
                || root.identity.binding != self.config.binding.revision
            {
                return Err("parent instruction grant differs from canonical binding".into());
            }
            for tool in [
                "vcp_read",
                "vcp_list",
                "vcp_search",
                "vcp_patch",
                "vcp_exec",
                "vcp_verify",
            ] {
                self.tool_read_access(&root.identity.root, tool)?;
            }
        }
        Ok(())
    }
}
