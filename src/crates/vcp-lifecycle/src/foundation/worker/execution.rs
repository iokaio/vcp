// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::collections::BTreeSet;
use vcp_domain::policy::{EffectClass, Isolation};
use vcp_tools::process::{Prepared, Profile, Request};
impl Context {
    pub fn configure_process(&mut self, profile: Profile) -> Result<()> {
        if !self.owner_alive || self.authority_pending {
            return Err("owner is closed".into());
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
        let policy =
            vcp_engine::policy::current(self.engine.store().state(), &binding.scope.workspace)?;
        if policy.denials.iter().any(|rule| {
            (rule.effects.is_empty() || rule.effects.contains(&EffectClass::Read))
                && (rule.roots.is_empty() || rule.roots.contains(&root))
                && rule.tool.as_deref().is_none_or(|name| name == "vcp_exec")
        }) {
            return Err("trusted read denial prevents executable preparation".into());
        }
        Ok(vcp_tools::process::prepare(
            self.tool_root()?,
            identity,
            profile,
            request,
        )?)
    }
    pub fn process_decision(
        &self,
        binding: &ThreadBinding,
        prepared: &Prepared,
    ) -> Result<vcp_policy::Decision> {
        let root = self.tool_root()?;
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
        self.authority_decision(
            binding,
            prepared.authority(),
            &prepared.roots(),
            current.as_deref() == Some(prepared.profile().digest()?.as_str())
                && root.identity == prepared.root().identity
                && root.path() == prepared.root().path(),
            &isolation,
        )
    }
}
