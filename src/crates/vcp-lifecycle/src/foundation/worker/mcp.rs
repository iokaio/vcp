// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::foundation::mcp::{Provenance, Registration};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::policy::{GrantScope, Isolation};
use vcp_extensions::mcp as vcp_mcp;

#[derive(Default)]
pub(super) struct State {
    registrations: BTreeMap<String, Registration>,
    remotes: BTreeMap<String, crate::foundation::mcp::remote::RemoteRegistration>,
}
impl Context {
    pub(in crate::foundation) fn resume_mcp_approval(
        &mut self,
        binding: &ThreadBinding,
        thread: codex_protocol::ThreadId,
        runtime: &crate::Lifecycle,
        expected: Revision,
        fingerprint: vcp_domain::verification::Fingerprint,
        proofs: Vec<crate::foundation::mcp::ResumeProof>,
        commit: ResumeCommit,
        public_attempt: Option<Arc<AtomicBool>>,
    ) -> Result<CommandReceipt> {
        if let Some(receipt) = self.recheck_resume(&commit)? {
            return Ok(receipt);
        }
        if !self.owner_alive || self.authority_pending || self.capture_admission_blocked() {
            return Err("MCP resume owner is fenced".into());
        }
        self.validate_binding(binding)?;
        let state = self.engine.store().state();
        let task: Task = state
            .record(
                Collection::Task,
                binding.scope.task.as_str(),
                &binding.scope.workspace,
            )?
            .decode()?;
        if task.state != TaskState::WaitingForInput {
            return Err("idle MCP resume is only available for answered call approval".into());
        }
        let mut ancestor = task.parent.clone();
        while let Some(id) = ancestor {
            let parent: Task = state
                .record(Collection::Task, id.as_str(), &binding.scope.workspace)?
                .decode()?;
            if parent.state != TaskState::Running {
                return Err("MCP ancestor remains held".into());
            }
            ancestor = parent.parent;
        }
        let workspace: Workspace = state
            .record(
                Collection::Workspace,
                self.config.workspace.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        let policy = vcp_engine::policy::current(state, &self.config.workspace)?;
        let mut idle = Vec::new();
        let mut live = Vec::new();
        for proof in proofs {
            crate::foundation::scheduler::check_generation(runtime, thread, proof.generation)?;
            if self.engine.controller() != &proof.controller
                || self.engine.owner_epoch() != proof.owner
                || proof.job.active_process_count()? == 0
            {
                return Err("MCP idle owner or native process changed".into());
            }
            let effect: vcp_domain::effect::Effect = state
                .record(
                    Collection::Effect,
                    proof.effect.as_str(),
                    &binding.scope.workspace,
                )?
                .decode()?;
            if effect.scope != binding.scope
                || effect.state != vcp_domain::effect::EffectState::Running
                || effect.execution.as_ref() != Some(&proof.execution)
                || effect.operation_digest != proof.process.authority().digest()
            {
                return Err("MCP lifetime is not the exact owned running process".into());
            }
            let call: vcp_domain::effect::Effect = state
                .record(
                    Collection::Effect,
                    proof.call.as_str(),
                    &binding.scope.workspace,
                )?
                .decode()?;
            let approval: Approval = state
                .record(
                    Collection::Approval,
                    proof.approval.as_str(),
                    &binding.scope.workspace,
                )?
                .decode()?;
            if call.scope != binding.scope
                || call.state != vcp_domain::effect::EffectState::Validated
                || call.execution.is_some()
                || call.operation_digest != proof.authority.digest()
                || approval.state != ApprovalState::Allowed
                || approval.effect != call.id
                || approval.scope != call.scope
                || approval.effect_revision != call.revision
                || approval.operation_digest != call.operation_digest
                || approval.controller.as_ref() != Some(&proof.controller)
                || approval.owner_epoch != Some(proof.owner)
                || approval.expires_at <= now()
            {
                return Err("MCP resume requires the exact granted undelivered call".into());
            }
            self.validate_mcp_sources(binding, &proof.provenance)?;
            let registration = self
                .mcp
                .registrations
                .get(&proof.server)
                .ok_or("MCP registration unavailable")?;
            if self.resolve_mcp_registration(registration)?.digest()?
                != proof.registration_digest.as_str()
            {
                return Err("MCP registration changed before resume".into());
            }
            let current = self
                .process_profiles
                .get(proof.process.profile().name())
                .ok_or("MCP process profile unavailable")?;
            if current.digest()? != proof.process.profile().digest()? {
                return Err("MCP process profile changed before resume".into());
            }
            let root = self.tool_root()?;
            if root.identity != proof.process.root().identity
                || root.path() != proof.process.root().path()
            {
                return Err("MCP native binding changed".into());
            }
            let roots = proof.process.roots();
            let isolation = BTreeSet::from([
                Isolation::JobTree,
                Isolation::ProcessCount,
                Isolation::FilteredEnvironment,
                Isolation::Timeout,
                Isolation::OutputLimit,
            ]);
            // This evaluates only the two exact already-prepared operations as
            // evidence for a proposed transition, never as dispatch admission.
            // The ordinary task-running/source fences run again after resume.
            for operation in [proof.process.authority(), proof.authority.as_ref()] {
                let decision = vcp_engine::policy::evaluate(
                    state,
                    operation,
                    &vcp_policy::Facts {
                        workspace: &workspace,
                        scope: &binding.scope,
                        actor: &self.config.actor,
                        steering: task.steering,
                        policy: policy.revision,
                        now: now(),
                        owner_current: true,
                        task_running: true,
                        resources_current: true,
                        registered_roots: &roots,
                        isolation: &isolation,
                        host_denials: &self.config.host_tool_denials,
                    },
                )?;
                if !matches!(decision, vcp_policy::Decision::Allow { .. }) {
                    return Err("MCP granted authority no longer allows resume".into());
                }
            }
            live.push((proof.job, proof.generation));
            idle.push((proof.effect, proof.execution));
        }
        if idle.is_empty() {
            return Err("empty MCP resume proof".into());
        }
        self.resume_checked(binding, expected, fingerprint, &idle, commit, || {
            let state = runtime
                .0
                .state
                .lock()
                .map_err(|_| "lifecycle poisoned during MCP resume")?;
            for (job, generation) in live {
                if !state.attached
                    || state.sealing
                    || state.held(thread)
                    || !state.admission_current(thread, generation)
                {
                    return Err("MCP owner changed during resume observation".into());
                }
                if job.active_process_count()? == 0 {
                    return Err("MCP process exited during resume observation".into());
                }
            }
            if let Some(attempt) = public_attempt {
                attempt.store(true, Ordering::SeqCst);
            }
            Ok(state)
        })
    }
    pub fn mcp_server_names(&self) -> Vec<String> {
        self.mcp
            .registrations
            .keys()
            .chain(self.mcp.remotes.keys())
            .cloned()
            .collect()
    }
    pub fn configure_mcp(&mut self, registration: Registration) -> Result<()> {
        if !self.owner_alive || self.authority_pending || !self.coding.is_empty() {
            return Err("MCP registration requires fresh trusted owner setup".into());
        }
        if self.mcp.registrations.len() + self.mcp.remotes.len() >= 16
            || self.mcp.remotes.contains_key(&registration.name)
            || self.mcp.registrations.contains_key(&registration.name)
        {
            return Err("MCP registration ceiling or duplicate identity".into());
        }
        self.resolve_mcp_registration(&registration)?;
        self.mcp
            .registrations
            .insert(registration.name.clone(), registration);
        Ok(())
    }
    fn resolve_mcp_registration(
        &self,
        registration: &Registration,
    ) -> Result<vcp_mcp::registration::Registration> {
        let profile = self
            .process_profiles
            .get(&registration.process.profile)
            .ok_or("MCP process profile is not configured")?;
        if profile.mode() != vcp_tools::process::Mode::Direct
            || profile.terminal().is_some()
            || registration.process.input.is_some()
            || registration.process.timeout_ms == 0
            || registration.process.timeout_ms > 120_000
            || registration.process.output_bytes == 0
            || registration.process.output_bytes > 8 * 1024 * 1024
            || registration.limits.timeout_ms > registration.process.timeout_ms
            || registration.limits.total_discovery_bytes > registration.process.output_bytes
        {
            return Err("MCP requires bounded direct duplex process configuration".into());
        }
        let resolved_digest = vcp_protocol::digest_bytes(&canonical_bytes(
            &serde_json::json!({"profile":profile.digest()?,"request":registration.process}),
        )?);
        let pure = vcp_mcp::registration::Registration {
            id: registration.name.clone(),
            revision: Revision::ZERO,
            scope: GrantScope::Workspace {
                workspace: self.config.workspace.clone(),
            },
            transport: vcp_mcp::registration::Transport::Stdio {
                process_profile: registration.process.profile.clone(),
                resolved_digest,
            },
            auth_refs: BTreeSet::new(),
            allowed_tools: registration.allowed_tools.clone(),
            allowed_resources: registration.allowed_resources.clone(),
            allowed_prompts: registration.allowed_prompts.clone(),
            trusted_effects: BTreeMap::new(),
            limits: registration.limits.clone(),
            capabilities: Default::default(),
        };
        pure.validate()?;
        Ok(pure)
    }
    pub fn mcp_registration(
        &self,
        binding: &ThreadBinding,
        server: &str,
    ) -> Result<(Registration, vcp_mcp::registration::Registration)> {
        self.can_start(binding)?;
        let configured = self
            .mcp
            .registrations
            .get(server)
            .ok_or("MCP server is not explicitly configured")?
            .clone();
        let resolved = self.resolve_mcp_registration(&configured)?;
        if !resolved.scope.contains(&binding.scope) {
            return Err("MCP registration scope rejected".into());
        }
        Ok((configured, resolved))
    }
    pub fn validate_mcp_provenance(
        &self,
        binding: &ThreadBinding,
        provenance: &Provenance,
    ) -> Result<()> {
        self.can_start(binding)?;
        self.validate_mcp_sources(binding, provenance)
    }
    fn validate_mcp_sources(&self, binding: &ThreadBinding, provenance: &Provenance) -> Result<()> {
        self.validate_skills(binding)?;
        self.tool_read_access(&RootId::parse(self.config.workspace.as_str())?, "vcp_mcp")?;
        let state = self.engine.store().state();
        let workspace: Workspace = state
            .record(
                Collection::Workspace,
                self.config.workspace.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        let task: Task = state
            .record(
                Collection::Task,
                binding.scope.task.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        let policy = vcp_engine::policy::current(state, &self.config.workspace)?;
        let revisions = provenance
            .revisions
            .as_ref()
            .ok_or("MCP source revisions absent")?;
        if revisions.scope != binding.scope
            || revisions.steering != task.steering
            || revisions.policy != policy.revision
            || revisions.authority != workspace.authority
            || revisions.deletion != workspace.deletion
            || revisions.binding != workspace.binding.revision
            || revisions.skills != self.skill_revision(&binding.scope)?
        {
            return Err("MCP source authority, binding or deletion changed".into());
        }
        if let Some(context) = &provenance.context {
            let sealed = context.sealed();
            if let Some(memory) = &provenance.memory {
                self.validate_memory_context(binding, memory, sealed)?;
            }
            for root in &provenance.roots {
                self.tool_read_access(&root.identity.root, "vcp_mcp")?;
            }
            vcp_repository::instructions::revalidate_probes(
                &sealed.manifest.instruction_probes,
                &provenance.roots,
            )?;
            for part in &sealed.manifest.included {
                let descriptor: ArtifactDescriptor = self
                    .engine
                    .store()
                    .state()
                    .record(
                        Collection::Artifact,
                        part.artifact.as_str(),
                        &binding.scope.workspace,
                    )?
                    .decode()?;
                if descriptor.spec.scope != part.scope
                    || descriptor.sha256 != part.source_hash
                    || descriptor.length != part.source_length
                {
                    return Err("MCP context source identity changed".into());
                }
                vcp_audit::history::History::read_artifact(
                    self.engine.store(),
                    &self.history_access(),
                    &part.artifact,
                    &mut std::io::sink(),
                )?;
                if let Some(file) = &part.file {
                    provenance
                        .roots
                        .iter()
                        .find(|root| root.identity.root == file.root)
                        .ok_or("MCP source root unavailable")?
                        .revalidate(file)?;
                }
            }
        } else if provenance.owner_arguments.is_none() {
            return Err("MCP call lacks owner input or accounted model context provenance".into());
        }
        Ok(())
    }
    pub fn mcp_decision(
        &self,
        binding: &ThreadBinding,
        server: &str,
        registration_digest: &str,
        process: &vcp_tools::process::Prepared,
        authority: &vcp_policy::Prepared,
        provenance: &Provenance,
    ) -> Result<vcp_policy::Decision> {
        self.validate_mcp_provenance(binding, provenance)?;
        let (_, registration) = self.mcp_registration(binding, server)?;
        if registration.digest()? != registration_digest {
            return Err("MCP registration changed".into());
        }
        let startup = self.process_decision(binding, process)?;
        if !matches!(startup, vcp_policy::Decision::Allow { .. }) {
            return Ok(startup);
        }
        self.authority_decision(
            binding,
            authority,
            &process.roots(),
            true,
            &BTreeSet::from([
                Isolation::JobTree,
                Isolation::ProcessCount,
                Isolation::FilteredEnvironment,
                Isolation::Timeout,
                Isolation::OutputLimit,
            ]),
        )
    }
    pub(in crate::foundation) fn prepare_mcp_authority(
        &self,
        binding: &ThreadBinding,
        process: &vcp_tools::process::Prepared,
        operation_kind: &crate::foundation::mcp::content::PreparedOperation,
        provenance: &Provenance,
    ) -> Result<vcp_policy::Prepared> {
        self.validate_mcp_provenance(binding, provenance)?;
        let current = self.tool_identity(binding, "vcp_mcp")?;
        let mut operation = process.authority().operation().clone();
        operation.scope = current.scope;
        operation.actor = current.actor;
        operation.host = current.host;
        operation.binding = current.binding;
        operation.authority = current.authority;
        operation.steering = current.steering;
        operation.policy = current.policy;
        operation.tool = "vcp_mcp".into();
        operation.schema =
            vcp_protocol::digest_bytes(&canonical_bytes(&operation_kind.evidence())?);
        // The unisolated daemon retains the startup process's conservative
        // effect set and write-capable resources. Server readOnlyHint cannot
        // narrow this authority classification.
        operation.arguments = String::from_utf8(canonical_bytes(&serde_json::json!({
            "identity":operation_kind.evidence(),"arguments":operation_kind.arguments().map(|args|serde_json::from_slice::<serde_json::Value>(args.canonical_bytes())).transpose()?,
            "provenance":provenance.evidence()?,
        }))?)?;
        Ok(vcp_policy::Prepared::new(operation)?)
    }
}

impl Context {
    pub(in crate::foundation) fn is_remote_mcp(&self, server: &str) -> bool {
        self.mcp.remotes.contains_key(server)
    }
    pub(in crate::foundation) fn configure_mcp_remote(
        &mut self,
        registration: crate::foundation::mcp::remote::RemoteRegistration,
    ) -> Result<()> {
        if !self.owner_alive || self.authority_pending || !self.coding.is_empty() {
            return Err("MCP registration requires fresh trusted owner setup".into());
        }
        let server = registration.profile.config().server.clone();
        if registration.profile.config().workspace != self.config.workspace
            || self.mcp.remotes.len() + self.mcp.registrations.len() >= 16
            || self.mcp.remotes.contains_key(&server)
            || self.mcp.registrations.contains_key(&server)
        {
            return Err("remote MCP registration scope or identity rejected".into());
        }
        registration.pure()?;
        self.mcp.remotes.insert(server, registration);
        Ok(())
    }
    pub(in crate::foundation) fn remote_mcp_setup(
        &self,
        server: &str,
    ) -> Result<(
        crate::foundation::mcp::remote::RemoteRegistration,
        crate::foundation::mcp::remote_authority::AuthorityPin,
    )> {
        if !self.owner_alive || self.authority_pending {
            return Err("remote credential owner unavailable".into());
        }
        let registration = self
            .mcp
            .remotes
            .get(server)
            .ok_or("remote MCP server not configured")?
            .clone();
        let workspace: Workspace = self
            .engine
            .store()
            .state()
            .record(
                Collection::Workspace,
                self.config.workspace.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        Ok((
            registration,
            crate::foundation::mcp::remote_authority::AuthorityPin {
                controller: self.engine.controller().clone(),
                owner: self.engine.owner_epoch(),
                authority: workspace.authority,
                binding: workspace.binding.revision,
            },
        ))
    }
    pub(in crate::foundation) fn remote_mcp_decision(
        &self,
        binding: &ThreadBinding,
        server: &str,
        registration_digest: &str,
        authority: &vcp_policy::Prepared,
        provenance: &Provenance,
    ) -> Result<vcp_policy::Decision> {
        self.validate_mcp_provenance(binding, provenance)?;
        let (configured, _) = self.remote_mcp_setup(server)?;
        if configured.pure()?.digest()? != registration_digest {
            return Err("remote MCP registration changed".into());
        }
        self.authority_decision(
            binding,
            authority,
            &BTreeSet::from([RootId::parse(self.config.workspace.as_str())?]),
            true,
            &BTreeSet::from([Isolation::Timeout, Isolation::OutputLimit]),
        )
    }
    pub(in crate::foundation) fn prepare_remote_mcp_authority(
        &self,
        binding: &ThreadBinding,
        registration: &crate::foundation::mcp::remote::RemoteRegistration,
        arguments: serde_json::Value,
        provenance: &Provenance,
    ) -> Result<vcp_policy::Prepared> {
        self.validate_mcp_provenance(binding, provenance)?;
        let current = self.tool_identity(binding, "vcp_mcp")?;
        let operation = vcp_domain::policy::Operation {
            scope: current.scope,
            actor: current.actor,
            host: current.host,
            binding: current.binding,
            authority: current.authority,
            steering: current.steering,
            policy: current.policy,
            tool: "vcp_mcp".into(),
            schema: vcp_protocol::digest_bytes(b"vcp-mcp-http-operation-v1"),
            arguments: String::from_utf8(canonical_bytes(&arguments)?)?,
            invocation: vcp_domain::policy::Invocation::Remote {
                server_identity: registration.pure()?.digest()?,
                endpoint: registration.profile.endpoint().into(),
            },
            resources: vec![],
            effects: BTreeSet::from([
                vcp_domain::policy::EffectClass::Network,
                vcp_domain::policy::EffectClass::Opaque,
            ]),
            required_isolation: BTreeSet::from([Isolation::Timeout, Isolation::OutputLimit]),
            timeout_ms: Units::new(registration.limits.timeout_ms),
            output_bytes: ByteCount::new(registration.limits.total_discovery_bytes),
        };
        Ok(vcp_policy::Prepared::new(operation)?)
    }
}

impl Context {
    pub(in crate::foundation) fn read_mcp_cache(
        &self,
        binding: &ThreadBinding,
        artifact: &ArtifactId,
    ) -> Result<crate::foundation::mcp::ControlOutcome> {
        self.can_start(binding)?;
        self.tool_read_access(&RootId::parse(self.config.workspace.as_str())?, "vcp_mcp")?;
        let descriptor: ArtifactDescriptor = self
            .engine
            .store()
            .state()
            .record(
                Collection::Artifact,
                artifact.as_str(),
                &binding.scope.workspace,
            )?
            .decode()?;
        if descriptor.spec.scope != binding.scope {
            return Err("MCP cached resource scope rejected".into());
        }
        const CACHE_BYTES: usize = 16 * 1024 * 1024;
        if descriptor.length.get() > CACHE_BYTES as u64 {
            return Err("MCP cached resource byte ceiling".into());
        }
        struct BoundedBytes(Vec<u8>);
        impl std::io::Write for BoundedBytes {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if bytes.len() > CACHE_BYTES.saturating_sub(self.0.len()) {
                    return Err(std::io::Error::other("MCP cached resource byte ceiling"));
                }
                self.0.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut bytes = BoundedBytes(Vec::new());
        vcp_audit::history::History::read_artifact(
            self.engine.store(),
            &self.history_access(),
            artifact,
            &mut bytes,
        )?;
        let receipt: serde_json::Value = serde_json::from_slice(&bytes.0)?;
        Ok(crate::foundation::mcp::ControlOutcome {
            value: serde_json::json!({"artifact":artifact,"prior_observation":true,"external_content":true,"grants_authority":false,"receipt":receipt,"result":receipt["result"]}),
            artifacts: vec![artifact.clone()],
        })
    }
}
