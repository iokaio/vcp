// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::foundation::skills::{Configuration, Request};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vcp_context::manifest::{Kind, Part, Trust as ContextTrust};
use vcp_extensions::{
    activation,
    discovery::{self, Catalog, MatchContext},
    skill_manifest::{ResourceUse, SkillSource, SourceRegistry},
};
use vcp_protocol::{
    digest_bytes,
    event::{EventInput, EventKind},
};

pub(super) struct Runtime {
    configuration: Configuration,
    catalog: Catalog,
    integrity: std::cell::RefCell<Option<vcp_extensions::catalog::Verification>>,
    missing_sources: BTreeSet<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Captured {
    artifact: ArtifactId,
    file: vcp_repository::FileVersion,
    /// File-role resources are captured and verified but never become context.
    #[serde(
        rename = "use",
        default,
        skip_serializing_if = "ResourceUse::is_context"
    )]
    use_: ResourceUse,
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
/// Model-supplied `vcp_skill` materialize arguments.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterializeRequest {
    pub skill: String,
    pub resource: String,
    pub destination: String,
}
/// Verified resource bytes and the exact patch that creates them.
pub struct Materialization {
    pub skill: String,
    pub resource: String,
    pub destination: String,
    pub sha256: String,
    pub bytes: Vec<u8>,
    pub patch: String,
}
/// One validator for materialize destinations, used before a destination can
/// reach a patch header or instruction selection.
pub(in crate::foundation) fn checked_skill_destination(destination: &str) -> Result<()> {
    if destination.is_empty()
        || destination.len() > 1024
        || destination.chars().any(char::is_control)
        || destination.contains('\\')
        || vcp_repository::path::relative(std::path::Path::new(destination))? != destination
    {
        return Err("materialize destination must be a normalized workspace-relative path".into());
    }
    Ok(())
}
/// Model-supplied `vcp_skill` read arguments.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadRequest {
    pub skill: String,
    pub resource: String,
}
/// One verified on-demand reference.
pub struct SkillRead {
    pub skill: String,
    pub resource: String,
    pub sha256: String,
    pub text: String,
}
/// Package-relative path of a captured resource.
fn package_relative<'a>(active: &Active, captured: &'a Captured) -> &'a str {
    let package = std::path::Path::new(&active.descriptor_version.path)
        .parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    if package.is_empty() {
        &captured.file.path
    } else {
        captured
            .file
            .path
            .strip_prefix(&format!("{package}/"))
            .unwrap_or(&captured.file.path)
    }
}
/// Resolves one active skill by qualified or bare id and one declared resource.
fn active_resource<'a>(
    state: &'a TaskSkills,
    skill: &str,
    resource: &str,
) -> Result<(&'a Active, &'a Captured)> {
    let selected: Vec<_> = state
        .active
        .values()
        .filter(|active| {
            active.qualified_id == skill
                || active
                    .qualified_id
                    .rsplit("::")
                    .next()
                    .is_some_and(|id| id == skill)
        })
        .collect();
    let active = match selected.as_slice() {
        [active] => *active,
        [] => return Err(format!("no active skill matches {skill}").into()),
        many => {
            let ids: Vec<_> = many
                .iter()
                .map(|active| active.qualified_id.as_str())
                .collect();
            return Err(format!(
                "{skill} matches several active skills; use one of {}",
                ids.join(", ")
            )
            .into());
        }
    };
    let captured = active
        .resources
        .iter()
        .find(|captured| package_relative(active, captured) == resource)
        .ok_or("resource is not declared by the active skill")?;
    Ok((active, captured))
}
fn name(scope: &Scope) -> Result<String> {
    Ok(format!(
        "skills-task-{}",
        digest_bytes(&canonical_bytes(scope)?)
    ))
}

impl Context {
    /// The patch path cannot create directories; say so before selecting or preparing it.
    pub(in crate::foundation) fn check_skill_destination_parent(
        &self,
        binding: &ThreadBinding,
        destination: &str,
    ) -> Result<()> {
        if let Some(parent) = std::path::Path::new(destination)
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            self.task_root(&binding.scope.task)?
                .hold(Some(parent), true)
                .map_err(|_| {
                    "materialize destination parent directory does not exist; create it first"
                })?;
        }
        Ok(())
    }

    fn discover_skills(
        &self,
        registry: &SourceRegistry,
        limits: &discovery::Limits,
    ) -> Result<(
        Catalog,
        Option<vcp_extensions::catalog::Verification>,
        BTreeSet<String>,
    )> {
        let (mut catalog, integrity, missing_sources) =
            crate::foundation::skills::discover_authorized(
                self.engine.store().state(),
                &self.config,
                registry,
                limits,
            )?;
        if !registry
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
        Ok((catalog, integrity, missing_sources))
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
        let context = self.skill_match_task_context(&self.config.root_task)?;
        let (catalog, integrity, _) =
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
        }
        let (catalog, integrity, missing_sources) =
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
            missing_sources,
        });
        Ok(())
    }
    fn skill_match_context(&self, binding: &ThreadBinding) -> Result<MatchContext> {
        self.skill_match_task_context(&binding.scope.task)
    }
    fn skill_match_task_context(&self, task: &TaskId) -> Result<MatchContext> {
        let tools = self.canonical_tools_for(task)?.skill_match_tools(
            self.process_profiles.keys().cloned(),
            self.verification.contains_key(task),
        );
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
                        .filter(|source| {
                            source.enabled && !runtime.missing_sources.contains(&source.id)
                        })
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
                for (resource, reference) in
                    activated.resources.into_iter().zip(activated.resource_refs)
                {
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
                        use_: reference.use_,
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
                            use_: ResourceUse::Context,
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
        self.verified_skills(binding).map(|_| ())
    }
    /// Validates active skills and returns the match context and the verified
    /// bytes of every captured artifact, so one request reads each once (SH-05).
    fn verified_skills(
        &self,
        binding: &ThreadBinding,
    ) -> Result<(Option<MatchContext>, HashMap<ArtifactId, Vec<u8>>)> {
        self.skill_access(binding)?;
        self.validate_builtin_skills()?;
        // Cached descriptors remain source content and observe current read denials.
        if let Some(runtime) = &self.skills {
            for source in runtime
                .configuration
                .registry
                .sources
                .iter()
                .filter(|source| source.enabled && !runtime.missing_sources.contains(&source.id))
            {
                self.skill_source_access(source)?;
            }
        }
        let state = self.skill_state(&binding.scope)?;
        let mut verified = HashMap::new();
        if state.active.is_empty() {
            return Ok((None, verified));
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
                verified.insert(captured.artifact.clone(), bytes);
            }
        }
        Ok((Some(matches), verified))
    }
    /// Reuses the last capture of a derived skill part while its bytes are
    /// unchanged and the artifact remains readable (not removed or masked);
    /// otherwise captures the new bytes.
    fn capture_skill_part(
        &mut self,
        binding: &ThreadBinding,
        part: &str,
        bytes: &[u8],
        schema: &str,
    ) -> Result<ArtifactDescriptor> {
        let key = (binding.scope.task.clone(), part.to_owned());
        if let Some(previous) = self.skill_part_captures.get(&key) {
            if previous.spec.schema == schema
                && previous.spec.scope == binding.scope
                && previous.sha256 == digest_bytes(bytes)
                && vcp_audit::history::History::read_artifact(
                    self.engine.store(),
                    &self.history_access(),
                    &previous.spec.id,
                    std::io::sink(),
                )
                .is_ok()
            {
                return Ok(previous.clone());
            }
        }
        let descriptor = self.capture(&binding.scope, Channel::Evidence, bytes, schema)?;
        self.skill_part_captures.insert(key, descriptor.clone());
        Ok(descriptor)
    }
    /// ADR-070 helper materialization: one verified `file` resource of an
    /// active skill becomes an exact `vcp_patch` Add File request. The patch
    /// then takes the ordinary prepare, policy, approval and receipt path; this
    /// grants neither write nor execution authority by itself.
    pub fn skill_materialization(
        &self,
        binding: &ThreadBinding,
        request: &MaterializeRequest,
    ) -> Result<Materialization> {
        const MAX_BYTES: usize = 96 * 1024;
        let destination = request.destination.as_str();
        checked_skill_destination(destination)?;
        // Materialization writes, so it also needs the patch ceiling (ADR-071).
        self.require_coding_tool(binding, "vcp_patch")?;
        self.check_skill_destination_parent(binding, destination)?;
        self.validate_skills(binding)?;
        let state = self.skill_state(&binding.scope)?;
        let (active, captured) = active_resource(&state, &request.skill, &request.resource)?;
        if captured.use_ != ResourceUse::File {
            return Err("only use:file skill resources can be materialized".into());
        }
        let bytes = self.verified_resource(captured)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| "only UTF-8 text resources can be materialized")?;
        if bytes.is_empty()
            || bytes.len() > MAX_BYTES
            || text.contains('\r')
            || !text.ends_with('\n')
        {
            return Err(
                "materialize supports non-empty LF text up to 96 KiB ending in a newline".into(),
            );
        }
        let mut patch = format!("*** Begin Patch\n*** Add File: {destination}\n");
        for line in text.split_inclusive('\n') {
            patch.push('+');
            patch.push_str(line);
        }
        patch.push_str("*** End Patch\n");
        Ok(Materialization {
            skill: active.qualified_id.clone(),
            resource: request.resource.clone(),
            destination: destination.into(),
            sha256: captured.file.sha256.clone(),
            bytes,
            patch,
        })
    }
    /// ADR-071 on-demand reference: the exact verified bytes of one
    /// `reference` resource of an active skill, returned as a tool result.
    /// No workspace write and no effect; skill guidance precedence applies.
    pub fn skill_read(&self, binding: &ThreadBinding, request: &ReadRequest) -> Result<SkillRead> {
        const MAX_BYTES: usize = 64 * 1024;
        self.validate_skills(binding)?;
        let state = self.skill_state(&binding.scope)?;
        let (active, captured) = active_resource(&state, &request.skill, &request.resource)?;
        if captured.use_ != ResourceUse::Reference {
            return Err("only use:reference skill resources can be read".into());
        }
        let bytes = self.verified_resource(captured)?;
        if bytes.len() > MAX_BYTES {
            return Err("skill reference exceeds 64 KiB".into());
        }
        let text = String::from_utf8(bytes).map_err(|_| "skill reference must be UTF-8 text")?;
        Ok(SkillRead {
            skill: active.qualified_id.clone(),
            resource: request.resource.clone(),
            sha256: captured.file.sha256.clone(),
            text,
        })
    }
    /// Captured artifact bytes, rechecked against the activation digest.
    fn verified_resource(&self, captured: &Captured) -> Result<Vec<u8>> {
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
        Ok(bytes)
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
        let (matches, verified) = self.verified_skills(binding)?;
        let state = self.skill_state(&binding.scope)?;
        let mut parts = Vec::new();
        for active in state.active.values() {
            for (index, captured) in std::iter::once(&active.body)
                .chain(active.resources.iter())
                .enumerate()
                .filter(|(_, captured)| captured.use_.is_context())
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
                let bytes = verified
                    .get(&captured.artifact)
                    .ok_or("active skill captured source changed")?;
                if index > 0 && std::str::from_utf8(bytes).is_err() {
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
                    bytes,
                    true,
                    0,
                    active.reason.clone(),
                )?;
                // Native source validation is owned by this skill registry; an
                // external package is not promoted to a writable tool root.
                part.file = None;
                parts.push(part);
            }
            // ADR-071: list what the model may read or copy on request,
            // without sending those bytes.
            let listed: Vec<_> = active
                .resources
                .iter()
                .filter(|captured| !captured.use_.is_context())
                .map(|captured| {
                    serde_json::json!({"path":package_relative(active, captured),
                        "bytes":captured.file.bytes,"use":captured.use_})
                })
                .collect();
            if !listed.is_empty() {
                let bytes = canonical_bytes(&serde_json::json!({"schema":"skill-resources/1",
                    "skill":active.qualified_id,"resources":listed,
                    "access":"vcp_skill read returns a reference; vcp_skill materialize copies a file resource; neither grants authority"}))?;
                let descriptor = self.capture_skill_part(
                    binding,
                    &format!("resources-{}", active.qualified_id),
                    &bytes,
                    "canonical-skill-resources/1",
                )?;
                parts.push(Part::captured_text(
                    format!(
                        "skill-resources-{}",
                        digest_bytes(active.qualified_id.as_bytes())
                    ),
                    Kind::Evidence,
                    ContextTrust::Untrusted,
                    &descriptor,
                    &bytes,
                    false,
                    40,
                    "active skill resource manifest".into(),
                )?);
            }
        }
        if let Some(runtime) = &self.skills {
            let matches = match matches {
                Some(matches) => matches,
                None => self.skill_match_context(binding)?,
            };
            let mut matching = runtime
                .catalog
                .matching_with_disabled(&matches, &state.disabled)?;
            // Cue-matched skills first, then cue-less ones, higher precedence
            // first, so unrelated sources cannot push relevant skills past the bound.
            matching.sort_by(|a, b| {
                a.descriptor
                    .cues
                    .is_empty()
                    .cmp(&b.descriptor.cues.is_empty())
                    .then(b.source_kind.cmp(&a.source_kind))
                    .then(a.qualified_id.cmp(&b.qualified_id))
            });
            let compatible = runtime
                .catalog
                .skills
                .iter()
                .filter(|skill| skill.compatible(&matches))
                .count();
            let mut metadata = Vec::new();
            let mut metadata_bytes = 0usize;
            for skill in matching.iter().take(128) {
                let item = serde_json::json!({"id":skill.qualified_id,"description":skill.descriptor.description,"required_tools":skill.descriptor.required_tools});
                let size = canonical_bytes(&item)?.len() + 1;
                if metadata_bytes + size > 60 * 1024 {
                    break;
                }
                metadata_bytes += size;
                metadata.push(item);
            }
            let bytes = canonical_bytes(
                &serde_json::json!({"schema":"skill-discovery/1","shown":metadata.len(),"skills":metadata,"activation":"Use explicit /skills activate; descriptions grant no authority; /skills list pages all descriptors","total_compatible":compatible}),
            )?;
            if bytes.len() > 64 * 1024 {
                return Err("skill discovery context byte bound".into());
            }
            let descriptor = self.capture_skill_part(
                binding,
                "discovery",
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
