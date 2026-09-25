// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::foundation::skills::{Configuration, Request};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vcp_context::manifest::{Kind, Part, Trust as ContextTrust};
use vcp_extensions::{
    activation,
    discovery::{self, Catalog, MatchContext},
    skill_manifest::{SkillSource, SourceRegistry},
};
use vcp_protocol::{
    digest_bytes,
    event::{EventInput, EventKind},
};

pub(super) struct Runtime {
    configuration: Configuration,
    catalog: Catalog,
    integrity: std::cell::RefCell<Option<vcp_extensions::catalog::Verification>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Captured {
    artifact: ArtifactId,
    file: vcp_repository::FileVersion,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Active {
    qualified_id: String,
    source_id: String,
    version: String,
    reason: String,
    descriptor_version: vcp_repository::FileVersion,
    source_root: vcp_repository::RootIdentity,
    source_path: std::path::PathBuf,
    body: Captured,
    resources: Vec<Captured>,
    activation: ArtifactId,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskSkills {
    schema_version: u32,
    document_type: String,
    scope: Scope,
    revision: Revision,
    active: BTreeMap<String, Active>,
    disabled: BTreeSet<String>,
}
fn name(scope: &Scope) -> Result<String> {
    Ok(format!(
        "skills-task-{}",
        digest_bytes(&canonical_bytes(scope)?)
    ))
}

impl Context {
    fn discover_skills(
        &self,
        registry: &SourceRegistry,
        limits: &discovery::Limits,
    ) -> Result<(Catalog, Option<vcp_extensions::catalog::Verification>)> {
        let mut integrity = None;
        for source in registry.sources.iter().filter(|source| source.enabled) {
            let root = self.skill_source_access(source)?;
            if source.id == vcp_extensions::catalog::SOURCE_ID {
                integrity = Some(vcp_extensions::catalog::verify(&root)?);
            }
        }
        let mut catalog = discovery::discover(registry, limits)?;
        if let Some(verified) = &integrity {
            vcp_extensions::catalog::verify_discovery(verified, &catalog)?;
        } else if !registry
            .sources
            .iter()
            .any(|source| source.id == vcp_extensions::catalog::SOURCE_ID)
        {
            catalog.diagnostics.push(discovery::Diagnostic {
                source_id: vcp_extensions::catalog::SOURCE_ID.into(), path: String::new(),
                code: "builtin_source_unavailable".into(),
                message: "Bundled skill source is not registered; packaged assets may be missing (for example a bare development binary).".into(),
            });
        }
        Ok((catalog, integrity))
    }
    fn validate_builtin_skills(&self) -> Result<()> {
        let Some(runtime) = &self.skills else {
            return Ok(());
        };
        let Some(source) = runtime
            .configuration
            .registry
            .sources
            .iter()
            .find(|source| source.id == vcp_extensions::catalog::SOURCE_ID && source.enabled)
        else {
            return Ok(());
        };
        let root = self.skill_source_access(source)?;
        let mut integrity = runtime.integrity.borrow_mut();
        let verified = integrity
            .as_mut()
            .ok_or("builtin catalog integrity evidence missing")?;
        let reads = vcp_extensions::catalog::revalidate(&root, verified)?;
        verified.reads.revalidations = verified
            .reads
            .revalidations
            .checked_add(reads.revalidations)
            .ok_or("builtin integrity read counter overflow")?;
        verified.reads.revalidation_bytes = verified
            .reads
            .revalidation_bytes
            .checked_add(reads.revalidation_bytes)
            .ok_or("builtin integrity byte counter overflow")?;
        Ok(())
    }
    fn skill_source_access(&self, source: &SkillSource) -> Result<vcp_repository::Root> {
        crate::foundation::skills::check_source_read_access(
            self.engine.store().state(),
            &self.config,
            source,
        )
        .map_err(Into::into)
    }
    pub fn inspect_skills(&self, registry: SourceRegistry) -> Result<serde_json::Value> {
        if !self.owner_alive || self.authority_pending {
            return Err("skill inspection requires current owner".into());
        }
        registry.validate()?;
        for source in registry.sources.iter().filter(|source| source.enabled) {
            self.skill_source_access(source)?;
        }
        let context = self.skill_match_task_context(&self.config.root_task)?;
        let (catalog, integrity) =
            self.discover_skills(&registry, &discovery::Limits::default())?;
        Ok(serde_json::json!({"catalog":catalog,"context":context,"integrity":integrity}))
    }
    fn skill_state(&self, scope: &Scope) -> Result<TaskSkills> {
        let id = name(scope)?;
        let Some(row) = self
            .engine
            .store()
            .state()
            .records
            .get(&key(Collection::Projection, &id))
        else {
            return Ok(TaskSkills {
                schema_version: 1,
                document_type: "vcp_task_skills_v1".into(),
                scope: scope.clone(),
                revision: Revision::ZERO,
                active: BTreeMap::new(),
                disabled: BTreeSet::new(),
            });
        };
        let state: TaskSkills = row.decode()?;
        if state.schema_version != 1
            || state.scope != *scope
            || row.workspace != scope.workspace
            || state.revision != row.revision.next()?
            || state.document_type != "vcp_task_skills_v1"
        {
            return Err("skill state scope or revision mismatch".into());
        }
        Ok(state)
    }
    pub(super) fn skill_revision(&self, scope: &Scope) -> Result<Revision> {
        Ok(self.skill_state(scope)?.revision)
    }
    fn skill_access(&self, binding: &ThreadBinding) -> Result<()> {
        if !self.owner_alive
            || self.authority_pending
            || binding.scope.workspace != self.config.workspace
            || binding.scope.session != self.config.session
        {
            return Err("skill controls require current scoped owner".into());
        }
        let access = self.history_access();
        if !access.read
            || access
                .tasks
                .as_ref()
                .is_some_and(|tasks| !tasks.contains(&binding.scope.task))
        {
            return Err("skill task access denied".into());
        }
        let task: Task = self
            .engine
            .store()
            .state()
            .record(
                Collection::Task,
                binding.scope.task.as_str(),
                &binding.scope.workspace,
            )?
            .decode()?;
        if task.scope != binding.scope {
            return Err("skill task scope mismatch".into());
        }
        Ok(())
    }
    pub fn configure_skills(&mut self, configuration: Configuration) -> Result<()> {
        if !self.owner_alive || self.authority_pending {
            return Err("skill configuration requires current owner".into());
        }
        configuration.registry.validate()?;
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
        for source in &configuration.registry.sources {
            if source.root.workspace != workspace.id
                || source.root.binding != workspace.binding.revision
            {
                return Err("skill source differs from current workspace binding".into());
            }
            if source.enabled {
                self.skill_source_access(source)?;
            }
        }
        let (catalog, integrity) =
            self.discover_skills(&configuration.registry, &configuration.limits)?;
        let scope = Scope {
            workspace: self.config.workspace.clone(),
            session: self.config.session.clone(),
            task: self.config.root_task.clone(),
        };
        self.capture(
            &scope,
            Channel::Evidence,
            &canonical_bytes(
                &serde_json::json!({"registry":configuration.registry,"catalog":catalog,"integrity":integrity}),
            )?,
            "canonical-skill-discovery/1",
        )?;
        self.skills = Some(Runtime {
            configuration,
            catalog,
            integrity: std::cell::RefCell::new(integrity),
        });
        Ok(())
    }
    fn skill_match_context(&self, binding: &ThreadBinding) -> Result<MatchContext> {
        self.skill_match_task_context(&binding.scope.task)
    }
    fn skill_match_task_context(&self, task: &TaskId) -> Result<MatchContext> {
        let ceiling = self.canonical_tools_for(task)?;
        let mut tools: BTreeSet<String> = ["vcp_read", "vcp_list", "vcp_search", "vcp_patch"]
            .into_iter()
            .filter(|name| ceiling.contains(name))
            .map(str::to_owned)
            .collect();
        if !self.process_profiles.is_empty() && ceiling.contains("vcp_exec") {
            tools.insert("vcp_exec".into());
            tools.extend(self.process_profiles.keys().cloned());
        }
        if self.verification.contains_key(task) && ceiling.contains("vcp_verify") {
            tools.insert("vcp_verify".into());
        }
        let root = self.task_root(task)?;
        self.tool_read_access(&root.identity.root, "vcp_skill")?;
        crate::foundation::skills::actual_match_context(&root, tools).map_err(|e| e.into())
    }
    fn save_skill_state(
        &mut self,
        state: TaskSkills,
        reason: &str,
        artifacts: Vec<ArtifactId>,
        qualified_id: &str,
    ) -> Result<()> {
        let id = name(&state.scope)?;
        let previous = self
            .engine
            .store()
            .state()
            .records
            .get(&key(Collection::Projection, &id))
            .map(|r| r.revision);
        let mut row = Record::typed(
            Collection::Projection,
            id,
            state.scope.workspace.clone(),
            previous
                .map(|revision| revision.next())
                .transpose()?
                .unwrap_or(Revision::ZERO),
            &state,
        )?;
        row.references
            .insert(key(Collection::Task, state.scope.task.as_str()));
        for active in state.active.values() {
            for artifact in std::iter::once(&active.body.artifact)
                .chain(active.resources.iter().map(|r| &r.artifact))
                .chain(std::iter::once(&active.activation))
            {
                row.references
                    .insert(key(Collection::Artifact, artifact.as_str()));
            }
        }
        let event = EventInput {
            id: EventId::new(),
            workspace: state.scope.workspace.clone(),
            session: state.scope.session.clone(),
            task: Some(state.scope.task.clone()),
            actor: self.config.actor.clone(),
            correlation: CommandId::new(),
            causation: None,
            timestamp: now(),
            kind: EventKind::Diagnostic,
            artifacts,
            data: serde_json::json!({"version":1,"skill_revision":state.revision,"qualified_id":qualified_id,"reason":reason}),
            metadata: None,
        };
        let watermark = self.engine.store().state().watermark;
        self.runtime
            .block_on(self.engine.store_mut().transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: watermark,
                mutations: vec![Mutation::Put {
                    record: row,
                    expected: previous,
                }],
                events: vec![event],
                command: None,
            }))?;
        Ok(())
    }
    pub fn skill_control(
        &mut self,
        binding: &ThreadBinding,
        request: Request,
    ) -> Result<serde_json::Value> {
        self.skill_access(binding)?;
        let mut state = self.skill_state(&binding.scope)?;
        match request {
            Request::Status => {
                self.validate_builtin_skills()?;
                if let Some(runtime) = &self.skills {
                    for source in runtime
                        .configuration
                        .registry
                        .sources
                        .iter()
                        .filter(|source| source.enabled)
                    {
                        self.skill_source_access(source)?;
                    }
                }
                let catalog = self.skills.as_ref().map(|s| &s.catalog);
                let context = self
                    .skills
                    .as_ref()
                    .map(|_| self.skill_match_context(binding))
                    .transpose()?;
                let integrity = self
                    .skills
                    .as_ref()
                    .and_then(|runtime| runtime.integrity.borrow().clone());
                return Ok(
                    serde_json::json!({"state":state,"catalog":catalog,"configured":self.skills.is_some(),"context":context,"integrity":integrity}),
                );
            }
            Request::Disable { id } => {
                if id.len() > 2048 || id.trim().is_empty() {
                    return Err("bounded qualified skill identity required".into());
                }
                let selected = if state.active.contains_key(&id) {
                    id
                } else {
                    let runtime = self.skills.as_ref().ok_or("skill configuration required")?;
                    runtime
                        .catalog
                        .resolve(&id, &self.skill_match_context(binding)?)?
                        .qualified_id
                        .clone()
                };
                state.active.remove(&selected);
                state.disabled.insert(selected.clone());
                if state.disabled.len() > 1024 {
                    return Err("disabled skill bound".into());
                }
                state.revision = state.revision.next()?;
                self.save_skill_state(
                    state,
                    "skill disabled; already dispatched effects retain normal reconciliation",
                    vec![],
                    &selected,
                )?;
            }
            Request::Activate { id, reason } => {
                self.validate_builtin_skills()?;
                let matches = self.skill_match_context(binding)?;
                let runtime = self.skills.as_ref().ok_or("skill configuration required")?;
                let selected = runtime.catalog.resolve(&id, &matches)?;
                if state.active.len() >= 32 && !state.active.contains_key(&selected.qualified_id) {
                    return Err("active skill bound".into());
                }
                let source = runtime
                    .configuration
                    .registry
                    .sources
                    .iter()
                    .find(|s| s.id == selected.source_id)
                    .ok_or("skill source unavailable")?;
                self.skill_source_access(source)?;
                let activated = activation::activate(
                    &runtime.configuration.registry,
                    &runtime.catalog,
                    &id,
                    &matches,
                    &reason,
                    &runtime.configuration.limits,
                )?;
                let source = runtime
                    .configuration
                    .registry
                    .sources
                    .iter()
                    .find(|s| s.id == activated.source_id)
                    .ok_or("skill source unavailable")?;
                let source_root = source.root.clone();
                let source_path = source.path.clone();
                self.tool_read_access(&source_root.root, "vcp_skill")?;
                let body = self.capture(
                    &binding.scope,
                    Channel::Evidence,
                    &activated.body.bytes,
                    "canonical-active-skill-body/1",
                )?;
                let mut resources = Vec::new();
                let mut artifacts = vec![body.spec.id.clone()];
                for resource in activated.resources {
                    let captured = self.capture(
                        &binding.scope,
                        Channel::Evidence,
                        &resource.bytes,
                        "canonical-active-skill-resource/1",
                    )?;
                    artifacts.push(captured.spec.id.clone());
                    resources.push(Captured {
                        artifact: captured.spec.id,
                        file: resource.version,
                    });
                }
                let activation=self.capture(&binding.scope,Channel::Evidence,&canonical_bytes(&serde_json::json!({"scope":binding.scope,"qualified_id":activated.qualified_id,"version":activated.version,"reason":activated.reason,"registry_digest":activated.registry_digest,"registry_revision":activated.registry_revision,"source_id":activated.source_id,"descriptor":activated.descriptor_version,"body":body.spec.id,"resources":resources}))?,"canonical-skill-activation/1")?;
                artifacts.push(activation.spec.id.clone());
                state.disabled.remove(&activated.qualified_id);
                let qualified_id = activated.qualified_id.clone();
                state.active.insert(
                    activated.qualified_id.clone(),
                    Active {
                        qualified_id: activated.qualified_id,
                        source_id: activated.source_id,
                        version: activated.version,
                        reason: activated.reason,
                        descriptor_version: activated.descriptor_version,
                        source_root,
                        source_path,
                        body: Captured {
                            artifact: body.spec.id,
                            file: activated.body.version,
                        },
                        resources,
                        activation: activation.spec.id,
                    },
                );
                if state.active.len() > 32 {
                    return Err("active skill bound".into());
                }
                state.revision = state.revision.next()?;
                self.save_skill_state(
                    state,
                    "skill activated from exact captured source",
                    artifacts,
                    &qualified_id,
                )?;
            }
        }
        Ok(serde_json::to_value(self.skill_state(&binding.scope)?)?)
    }
    pub(super) fn validate_skills(&self, binding: &ThreadBinding) -> Result<()> {
        self.skill_access(binding)?;
        self.validate_builtin_skills()?;
        // Cached descriptors remain source content and observe current read denials.
        if let Some(runtime) = &self.skills {
            for source in runtime
                .configuration
                .registry
                .sources
                .iter()
                .filter(|source| source.enabled)
            {
                self.skill_source_access(source)?;
            }
        }
        let state = self.skill_state(&binding.scope)?;
        if state.active.is_empty() {
            return Ok(());
        }
        let runtime = self
            .skills
            .as_ref()
            .ok_or("active skills require explicit source configuration after reopen")?;
        let matches = self.skill_match_context(binding)?;
        for active in state.active.values() {
            let selected = runtime.catalog.resolve(&active.qualified_id, &matches)?;
            if selected.descriptor_version != active.descriptor_version {
                return Err(
                    "active skill descriptor changed; reactivate the current version".into(),
                );
            }
            let source = runtime
                .configuration
                .registry
                .sources
                .iter()
                .find(|s| s.id == active.source_id && s.enabled)
                .ok_or("active skill source disabled or removed")?;
            if source.root != active.source_root
                || source.path != active.source_path
                || runtime
                    .configuration
                    .registry
                    .disabled
                    .contains(&active.qualified_id)
            {
                return Err("active skill source identity changed or disabled".into());
            }
            let root = self.skill_source_access(source)?;
            root.revalidate(&active.descriptor_version)?;
            for captured in std::iter::once(&active.body).chain(active.resources.iter()) {
                root.revalidate(&captured.file)?;
                let mut bytes = Vec::new();
                vcp_audit::history::History::read_artifact(
                    self.engine.store(),
                    &self.history_access(),
                    &captured.artifact,
                    &mut bytes,
                )?;
                if digest_bytes(&bytes) != captured.file.sha256 {
                    return Err("active skill captured source changed".into());
                }
            }
        }
        Ok(())
    }
    pub(super) fn validate_skill_plan(
        &self,
        binding: &ThreadBinding,
        artifacts: &[ArtifactId],
    ) -> Result<()> {
        self.validate_skills(binding)?;
        let revision = self.skill_revision(&binding.scope)?;
        for id in artifacts {
            let descriptor: ArtifactDescriptor = self
                .engine
                .store()
                .state()
                .record(Collection::Artifact, id.as_str(), &binding.scope.workspace)?
                .decode()?;
            if descriptor.spec.schema != "vcp-prepared-tool-v2" {
                continue;
            }
            let mut bytes = Vec::new();
            vcp_audit::history::History::read_artifact(
                self.engine.store(),
                &self.history_access(),
                id,
                &mut bytes,
            )?;
            let value: serde_json::Value = serde_json::from_slice(&bytes)?;
            let prepared: Revision = value
                .get("skills_revision")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()?
                .unwrap_or(Revision::ZERO);
            if prepared != revision {
                return Err("skill activation changed since tool preparation".into());
            }
        }
        Ok(())
    }
    pub(super) fn skill_parts(&mut self, binding: &ThreadBinding) -> Result<Vec<Part>> {
        self.validate_skills(binding)?;
        let state = self.skill_state(&binding.scope)?;
        let mut parts = Vec::new();
        for active in state.active.values() {
            for (index, captured) in std::iter::once(&active.body)
                .chain(active.resources.iter())
                .enumerate()
            {
                let descriptor: ArtifactDescriptor = self
                    .engine
                    .store()
                    .state()
                    .record(
                        Collection::Artifact,
                        captured.artifact.as_str(),
                        &binding.scope.workspace,
                    )?
                    .decode()?;
                let mut bytes = Vec::new();
                vcp_audit::history::History::read_artifact(
                    self.engine.store(),
                    &self.history_access(),
                    &captured.artifact,
                    &mut bytes,
                )?;
                if index > 0 && std::str::from_utf8(&bytes).is_err() {
                    continue;
                }
                let mut part = Part::captured_text(
                    format!(
                        "skill-{}-{index}",
                        digest_bytes(active.qualified_id.as_bytes())
                    ),
                    Kind::Skill,
                    ContextTrust::ActiveSkill,
                    &descriptor,
                    &bytes,
                    true,
                    0,
                    active.reason.clone(),
                )?;
                // Native source validation is owned by this skill registry; an
                // external package is not promoted to a writable tool root.
                part.file = None;
                parts.push(part);
            }
        }
        if let Some(runtime) = &self.skills {
            let matches = self.skill_match_context(binding)?;
            let matching = runtime.catalog.matching(&matches)?;
            let mut metadata = Vec::new();
            let mut metadata_bytes = 0usize;
            for skill in matching
                .iter()
                .filter(|skill| !state.disabled.contains(&skill.qualified_id))
                .take(128)
            {
                let item = serde_json::json!({"id":skill.qualified_id,"description":skill.descriptor.description,"required_tools":skill.descriptor.required_tools});
                let size = canonical_bytes(&item)?.len() + 1;
                if metadata_bytes + size > 60 * 1024 {
                    break;
                }
                metadata_bytes += size;
                metadata.push(item);
            }
            let bytes = canonical_bytes(
                &serde_json::json!({"schema":"skill-discovery/1","shown":metadata.len(),"skills":metadata,"activation":"Use explicit /skills activate; descriptions grant no authority; /skills list pages all descriptors","total":runtime.catalog.skills.len()}),
            )?;
            if bytes.len() > 64 * 1024 {
                return Err("skill discovery context byte bound".into());
            }
            let descriptor = self.capture(
                &binding.scope,
                Channel::Evidence,
                &bytes,
                "canonical-skill-discovery-context/1",
            )?;
            parts.push(Part::captured_text(
                "skill-discovery".into(),
                Kind::Evidence,
                ContextTrust::Untrusted,
                &descriptor,
                &bytes,
                false,
                50,
                "description-only configured skill discovery".into(),
            )?);
        }
        Ok(parts)
    }
}
