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
        if !self.coding.is_empty() {
            return Err("process profile refresh requires fresh coding owner setup".into());
        }
        self.process_profiles.insert(profile.name().into(), profile);
        Ok(())
    }
    pub fn prepare_process(&self, binding: &ThreadBinding, request: Request) -> Result<Prepared> {
        let identity = self.tool_identity(binding, "vcp_exec")?;
        let profile = self
            .process_profiles
            .get(&request.profile)
            .ok_or("process profile is not configured")?
            .clone();
        let root = profile.executable_root_id()?;
        self.tool_read_access(&root, "vcp_exec")
            .map_err(|_| "trusted read denial prevents executable preparation")?;
        Ok(vcp_tools::process::prepare(
            self.tool_root()?,
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
        let root = self.tool_root()?;
        if root.identity != prepared.root().identity || root.path() != prepared.root().path() {
            return Ok(vcp_policy::Decision::Deny {
                origin: "current identity".into(),
                reason: "native workspace binding changed".into(),
            });
        }
        self.process_preflight(binding, prepared)
    }
}
