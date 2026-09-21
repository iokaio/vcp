// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::collections::BTreeSet;
use vcp_domain::policy::Isolation;
use vcp_tools::process::{Prepared, Profile, Request};
impl Context {
    pub fn configure_process(&mut self, profile: Profile) -> Result<()> {
        if !self.owner_alive || self.authority_pending {
            return Err("owner is closed".into());
        }
        if !self.coding.is_empty() || !self.verification.is_empty() {
            return Err("process profile refresh requires fresh coding owner setup".into());
        }
        self.process_profiles.insert(profile.name().into(), profile);
        Ok(())
    }
    pub fn prepare_process(&self, binding: &ThreadBinding, request: Request) -> Result<Prepared> {
        self.child_process_scope(binding)?;
        let identity = self.tool_identity(binding, "vcp_exec")?;
        let profile = self
            .process_profiles
            .get(&request.profile)
            .ok_or("process profile is not configured")?
            .clone();
        if request.timeout_ms == 0 || request.timeout_ms > profile.max_timeout_ms() {
            return Err(format!(
                "profile {} requested duration {}ms exceeds ceiling {}ms",
                profile.name(),
                request.timeout_ms,
                profile.max_timeout_ms()
            )
            .into());
        }
        if self
            .coding_remaining()
            .is_some_and(|remaining| u128::from(request.timeout_ms) > remaining.as_millis())
        {
            return Err(format!(
                "profile {} requested duration exceeds remaining coding task deadline",
                profile.name()
            )
            .into());
        }
        let root = profile.executable_root_id()?;
        self.tool_read_access(&root, "vcp_exec")
            .map_err(|_| "trusted read denial prevents executable preparation")?;
        Ok(vcp_tools::process::prepare(
            self.task_root(&binding.scope.task)?,
            identity,
            profile,
            request,
        )?)
    }
    pub fn process_preflight(
        &self,
        binding: &ThreadBinding,
        prepared: &Prepared,
    ) -> Result<vcp_policy::Decision> {
        if self.coding_remaining().is_some_and(|remaining| {
            u128::from(prepared.authority().operation().timeout_ms.get()) > remaining.as_millis()
        }) {
            return Err("prepared process exceeds remaining coding deadline".into());
        }
        let current = self
            .process_profiles
            .get(prepared.profile().name())
            .map(|p| p.digest())
            .transpose()?;
        let mut isolation = BTreeSet::from([
            Isolation::JobTree,
            Isolation::ProcessCount,
            Isolation::FilteredEnvironment,
            Isolation::Timeout,
            Isolation::OutputLimit,
        ]);
        if codex_utils_pty::owned_pty_supported() {
            isolation.insert(Isolation::Pty);
        }
        let decision = self.authority_decision(
            binding,
            prepared.authority(),
            &prepared.roots(),
            current.as_deref() == Some(prepared.profile().digest()?.as_str()),
            &isolation,
        )?;
        if !matches!(decision, vcp_policy::Decision::Deny { .. }) {
            self.tool_identity(binding, "vcp_exec")?;
            self.tool_read_access(&prepared.profile().executable_root_id()?, "vcp_exec")?;
        }
        Ok(decision)
    }
    pub fn process_decision(
        &self,
        binding: &ThreadBinding,
        prepared: &Prepared,
    ) -> Result<vcp_policy::Decision> {
        let decision = self.process_preflight(binding, prepared)?;
        if matches!(decision, vcp_policy::Decision::Deny { .. }) {
            return Ok(decision);
        }
        let root = self.task_root(&binding.scope.task)?;
        if root.identity != prepared.root().identity || root.path() != prepared.root().path() {
            return Ok(vcp_policy::Decision::Deny {
                origin: "current identity".into(),
                reason: "native workspace binding changed".into(),
            });
        }
        self.process_preflight(binding, prepared)
    }
}
