// SPDX-License-Identifier: Apache-2.0
mod continuity;
mod instructions;
use super::*;
use crate::foundation::coding::CodingConfig;
use codex_extension_api::{AllowedTools, ToolName};
use vcp_context::{
    manifest::{Content, Kind, Part, Revisions, Trust as ContextTrust},
    selection::{Utf8ByteCeiling, assemble},
};
use vcp_models::{
    request,
    stream::{Call, ResultBody, Status},
};

pub(super) struct Loop {
    config: CodingConfig,
    operating: Part,
    history: Vec<Part>,
    history_sources: Vec<ArtifactId>,
    pairs: u64,
    revisions: Option<Revisions>,
    calls: Vec<Eligible>,
    probes: Vec<vcp_repository::instructions::Probe>,
    final_response: Option<Vec<ArtifactId>>,
    continuity: Option<continuity::Continuity>,
    instruction_parents: Option<Vec<vcp_repository::Root>>,
}
struct Eligible {
    attempt: AttemptId,
    call: Call,
    admitted: bool,
    sources: Vec<ArtifactId>,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Pair {
    sequence: u64,
    attempt: AttemptId,
    parts: [Part; 2],
    sources: Vec<ArtifactId>,
}

impl Context {
    pub(super) fn coding_remaining(&self) -> Option<Duration> {
        self.coding
            .values()
            .map(|state| state.config.deadline)
            .min()
            .map(|deadline| Duration::from_millis(deadline.get().saturating_sub(now().get())))
    }
    pub(super) fn check_coding_bounds(&self) -> Result<()> {
        let Some(limit) = self
            .coding
            .values()
            .map(|state| state.config.max_requests)
            .min()
        else {
            return Ok(());
        };
        for state in self.coding.values() {
            state
                .config
                .validate(now())
                .map_err(|e| -> Failure { e.into() })?;
        }
        let attempts = self
            .engine
            .store()
            .state()
            .records
            .values()
            .filter(|r| r.collection == Collection::Attempt)
            .map(Record::decode::<Attempt>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if attempts
            .iter()
            .filter(|attempt| attempt.root == self.config.root_task)
            .count()
            >= limit as usize
        {
            return Err("canonical root request limit reached".into());
        }
        Ok(())
    }
    fn coding_artifact(&self, id: &ArtifactId) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        vcp_audit::history::History::read_artifact(
            self.engine.store(),
            &self.history_access(),
            id,
            &mut bytes,
        )?;
        Ok(bytes)
    }
    fn coding_part(
        &mut self,
        scope: &Scope,
        kind: Kind,
        trust: ContextTrust,
        content: Content,
    ) -> Result<Part> {
        let bytes = content.bytes()?;
        let descriptor = self.capture(
            scope,
            Channel::Evidence,
            &bytes,
            "canonical-coding-content/1",
        )?;
        let mut part = Part::captured_text(
            descriptor.spec.id.to_string(),
            kind,
            trust,
            &descriptor,
            &bytes,
            true,
            0,
            "canonical coding source".into(),
        )?;
        part.content = content;
        Ok(part)
    }
    pub fn configure_coding(
        &mut self,
        binding: &ThreadBinding,
        config: CodingConfig,
    ) -> Result<()> {
        self.can_start(binding)?;
        if self.provider.is_none()
            || !self.streams.is_empty()
            || self.coding.contains_key(&binding.scope.task)
        {
            return Err(
                "coding setup requires an idle configured provider and a fresh owner task binding"
                    .into(),
            );
        }
        config
            .validate(now())
            .map_err(|e| -> Failure { e.into() })?;
        let operating = self.coding_part(
            &binding.scope,
            Kind::Operating,
            ContextTrust::Operating,
            Content::Text {
                text: config.operating.clone(),
            },
        )?;
        // Restore references, never execution permits. Every referenced source
        // is re-read through current history access during each assembly.
        let descriptors = self
            .engine
            .store()
            .state()
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
            .map(Record::decode::<ArtifactDescriptor>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut pairs = Vec::new();
        let mut history_sources = Vec::new();
        for descriptor in descriptors {
            if descriptor.spec.scope == binding.scope
                && descriptor.spec.schema == "canonical-coding-pair/1"
            {
                pairs.push(serde_json::from_slice::<Pair>(
                    &self.coding_artifact(&descriptor.spec.id)?,
                )?);
                history_sources.push(descriptor.spec.id);
            }
        }
        pairs.sort_by_key(|p| p.sequence);
        let mut history = Vec::new();
        for (index, pair) in pairs.iter().enumerate() {
            if pair.sequence != index as u64 || pair.parts.iter().any(|p| p.scope != binding.scope)
            {
                return Err("coding history is incomplete or belongs to another scope".into());
            }
            history.extend(pair.parts.clone());
            history_sources.extend(pair.sources.clone());
        }
        self.capture(
            &binding.scope,
            Channel::Evidence,
            &canonical_bytes(&config)?,
            "canonical-coding-configuration/1",
        )?;
        self.coding.insert(
            binding.scope.task.clone(),
            Loop {
                config,
                operating,
                history,
                history_sources,
                pairs: pairs.len() as u64,
                revisions: None,
                calls: vec![],
                probes: vec![],
                final_response: None,
                continuity: None,
                instruction_parents: None,
            },
        );
        Ok(())
    }
    pub(super) fn assemble_coding_context(&mut self, binding: &ThreadBinding) -> Result<()> {
        self.can_start(binding)?;
        if binding.role == RequestRole::Compaction {
            return Err("canonical compaction adapter required".into());
        }
        let current = self.context_revisions(binding)?;
        let state = self
            .coding
            .get(&binding.scope.task)
            .ok_or("coding setup missing")?;
        state
            .config
            .validate(now())
            .map_err(|e| -> Failure { e.into() })?;
        if !state.calls.is_empty() {
            return Err("completed response still has unconsumed calls".into());
        }
        let affected = state.config.affected_paths.clone();
        for source in &state.history_sources {
            self.coding_artifact(source)?;
        }
        let mut parts = vec![state.operating.clone()];
        parts.extend(state.history.clone());
        // Apply the most conservative read ceiling of every registered tool.
        // This prevents selecting a less restrictive tool name to read context.
        for name in [
            "vcp_read",
            "vcp_list",
            "vcp_search",
            "vcp_patch",
            "vcp_exec",
            "vcp_verify",
        ] {
            self.tool_identity(binding, name)?;
        }
        let root = self.tool_root()?;
        let parents = self.instruction_parents(binding)?;
        let instructions = root.instructions(&affected, &parents, 256 * 1024)?;
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
        let objective = task
            .objectives
            .last()
            .ok_or("canonical objective missing")?;
        parts.push(self.coding_part(
            &binding.scope,
            Kind::Objective,
            ContextTrust::User,
            Content::Text {
                text: String::from_utf8(canonical_bytes(objective)?)?,
            },
        )?);
        parts.push(self.coding_part(
            &binding.scope,
            Kind::TaskState,
            ContextTrust::Observed,
            Content::Text {
                text: String::from_utf8(canonical_bytes(&task)?)?,
            },
        )?);
        for instruction in instructions.documents {
            let mut part = self.coding_part(
                &binding.scope,
                Kind::ProjectInstruction,
                ContextTrust::Project,
                Content::Text {
                    text: String::from_utf8(instruction.source.bytes)?,
                },
            )?;
            part.file = Some(instruction.source.version);
            part.applicable_paths = instruction.applies_to;
            parts.push(part);
        }
        // Keep the conversation after current authority-bearing sources.
        parts.sort_by_key(|p| matches!(p.kind, Kind::ToolCall | Kind::ToolResult));
        let snapshot = self
            .provider
            .as_ref()
            .ok_or("provider missing")?
            .snapshot
            .clone();
        let schemas = crate::foundation::coding::schemas();
        let envelope = request::envelope(
            &snapshot,
            self.config.output_ceiling,
            Units::new(512),
            now(),
        )?;
        let probes = instructions.probes;
        let parts = self.compact_coding_parts(binding, parts, &current, |parts| {
            Ok(request::encode(parts, &envelope, &schemas, &snapshot)?)
        })?;
        let sealed = assemble(
            parts,
            current.clone(),
            envelope,
            schemas.clone(),
            probes.clone(),
            &Utf8ByteCeiling,
            |parts, envelope, schemas| {
                request::encode(parts, envelope, schemas, &snapshot).map_err(|_| {
                    vcp_context::manifest::Error::Incompatible("canonical provider codec")
                })
            },
        )?;
        let mut roots = parents;
        roots.push(root);
        self.prepare_context(binding, sealed, schemas, roots)?;
        self.coding.get_mut(&binding.scope.task).unwrap().revisions = Some(current);
        self.coding.get_mut(&binding.scope.task).unwrap().probes = probes;
        self.coding
            .get_mut(&binding.scope.task)
            .unwrap()
            .final_response = None;
        Ok(())
    }
    pub(super) fn complete_coding_response(
        &mut self,
        binding: &ThreadBinding,
        attempt: &AttemptId,
        response: ResultBody,
        sources: Vec<ArtifactId>,
    ) -> Result<()> {
        let current = self.context_revisions(binding)?;
        let state = self
            .coding
            .get_mut(&binding.scope.task)
            .ok_or("coding setup missing")?;
        if state.revisions.as_ref() != Some(&current)
            || response.status != Status::Completed
            || response.calls.len() > 64
        {
            self.pause_root("coding response is incomplete or context authority changed")?;
            return Err("coding response cannot authorize tools".into());
        }
        if !state.calls.is_empty() {
            return Err("coding calls would overwrite pending response".into());
        }
        if response.calls.iter().any(|call| {
            state
                .history
                .iter()
                .any(|part| matches!(&part.content, Content::ToolCall {id, ..} if id == &call.id))
        }) {
            self.pause_root("provider reused an observed coding call identity")?;
            return Err("coding response reused a historical call identity".into());
        }
        state.final_response = if response.calls.is_empty() {
            Some(sources.clone())
        } else {
            None
        };
        if response.calls.len() > 1 && response.calls.iter().any(|call| call.name == "vcp_verify") {
            // Retained async tasks need not acquire their execution lock in
            // response order. Never infer that a mixed check precedes/follows
            // sibling effects; retain explicit unexecuted pairs for reassembly.
            for call in response.calls {
                self.record_coding_result(binding, attempt.clone(), call,
                    serde_json::json!({"executed":false,"complete":false,"reason":"verification requires an isolated response"}),sources.clone())?;
            }
            return Ok(());
        }
        state.calls = response
            .calls
            .into_iter()
            .map(|call| Eligible {
                attempt: attempt.clone(),
                call,
                admitted: false,
                sources: sources.clone(),
            })
            .collect();
        Ok(())
    }
    pub fn coding_completion(&self, binding: &ThreadBinding) -> Result<VerificationId> {
        let current = self.context_revisions(binding)?;
        let state = self
            .coding
            .get(&binding.scope.task)
            .ok_or("coding setup missing")?;
        if !state.calls.is_empty() || state.revisions.as_ref() != Some(&current) {
            return Err("coding turn has pending calls or stale authority".into());
        }
        let sources = state
            .final_response
            .as_ref()
            .ok_or("no accounted final coding response")?;
        for source in sources {
            self.coding_artifact(source)?;
        }
        self.latest_verification(binding)
    }
    pub fn admit_coding_tool(
        &mut self,
        binding: &ThreadBinding,
        id: &str,
        name: &ToolName,
    ) -> Result<()> {
        // Reject invented/replayed call identities before a stale-call path can
        // change task state. Only a real completed-response call owns that path.
        if !self.coding.get(&binding.scope.task).is_some_and(|s| {
            s.calls.iter().any(|e| {
                e.call.id == id
                    && !e.admitted
                    && AllowedTools(vec![ToolName::plain(e.call.name.clone())]).contains(name)
            })
        }) {
            return Err("call is not an unconsumed accounted completed response".into());
        }
        let freshness = self.context_revisions(binding).and_then(|current| {
            self.validate_coding_sources(binding)?;
            if self
                .coding
                .get(&binding.scope.task)
                .and_then(|s| s.revisions.as_ref())
                != Some(&current)
            {
                return Err("coding tool context changed".into());
            }
            Ok(current)
        });
        let current = match freshness {
            Ok(current) => current,
            Err(error) => {
                self.reject_coding_call(binding, id, name, &error.to_string())?;
                self.pause_root("stale coding call requires deliberate source/authority refresh")?;
                return Err(error);
            }
        };
        let state = self
            .coding
            .get_mut(&binding.scope.task)
            .ok_or("registered coding tools are disabled")?;
        state
            .config
            .validate(now())
            .map_err(|e| -> Failure { e.into() })?;
        if state.revisions.as_ref() != Some(&current) {
            return Err("coding tool context changed".into());
        }
        let eligible = state
            .calls
            .iter_mut()
            .find(|e| {
                e.call.id == id
                    && AllowedTools(vec![ToolName::plain(e.call.name.clone())]).contains(name)
            })
            .ok_or("call is not in an accounted completed response")?;
        if eligible.admitted {
            return Err("coding call already admitted".into());
        }
        eligible.admitted = true;
        Ok(())
    }
    pub fn consume_coding_call(
        &mut self,
        binding: &ThreadBinding,
        id: &str,
        name: &str,
        arguments: &str,
    ) -> Result<(AttemptId, Call, Vec<ArtifactId>)> {
        let eligible = self
            .coding
            .get(&binding.scope.task)
            .and_then(|s| {
                s.calls
                    .iter()
                    .find(|e| e.call.id == id && e.call.name == name && e.admitted)
            })
            .ok_or("one-use completed call admission required")?;
        if arguments.len() > 256 * 1024
            || serde_json::from_str::<serde_json::Value>(arguments)? != eligible.call.arguments
        {
            return Err("wrapper arguments differ from captured response".into());
        }
        let freshness = self.context_revisions(binding).and_then(|current| {
            self.validate_coding_sources(binding)?;
            if self
                .coding
                .get(&binding.scope.task)
                .and_then(|s| s.revisions.as_ref())
                != Some(&current)
            {
                return Err("coding tool context changed".into());
            }
            Ok(current)
        });
        let current = match freshness {
            Ok(current) => current,
            Err(error) => {
                self.reject_coding_call(binding, id, &ToolName::plain(name), &error.to_string())?;
                self.pause_root("stale coding call requires deliberate source/authority refresh")?;
                return Err(error);
            }
        };
        let state = self
            .coding
            .get_mut(&binding.scope.task)
            .ok_or("coding setup missing")?;
        state
            .config
            .validate(now())
            .map_err(|e| -> Failure { e.into() })?;
        if state.revisions.as_ref() != Some(&current) {
            return Err("coding tool context changed".into());
        }
        let index = state
            .calls
            .iter()
            .position(|e| e.call.id == id && e.call.name == name && e.admitted)
            .ok_or("one-use completed call admission required")?;
        if arguments.len() > 256 * 1024
            || serde_json::from_str::<serde_json::Value>(arguments)?
                != state.calls[index].call.arguments
        {
            return Err("wrapper arguments differ from captured response".into());
        }
        let eligible = state.calls.remove(index);
        Ok((eligible.attempt, eligible.call, eligible.sources))
    }
    fn validate_coding_sources(&self, binding: &ThreadBinding) -> Result<()> {
        self.validate_continuity_sources(binding)?;
        if self
            .coding_remaining()
            .is_some_and(|remaining| remaining.is_zero())
        {
            return Err("canonical coding deadline elapsed".into());
        }
        for name in [
            "vcp_read",
            "vcp_list",
            "vcp_search",
            "vcp_patch",
            "vcp_exec",
            "vcp_verify",
        ] {
            self.tool_identity(binding, name)?;
        }
        let state = self
            .coding
            .get(&binding.scope.task)
            .ok_or("coding setup missing")?;
        let mut roots = self.instruction_parents(binding)?;
        roots.push(self.tool_root()?);
        vcp_repository::instructions::revalidate_probes(&state.probes, &roots)?;
        Ok(())
    }
    fn reject_coding_call(
        &mut self,
        binding: &ThreadBinding,
        id: &str,
        name: &ToolName,
        reason: &str,
    ) -> Result<()> {
        let Some(state) = self.coding.get_mut(&binding.scope.task) else {
            return Ok(());
        };
        let Some(index) = state.calls.iter().position(|e| {
            e.call.id == id
                && AllowedTools(vec![ToolName::plain(e.call.name.clone())]).contains(name)
        }) else {
            return Ok(());
        };
        let call = state.calls.remove(index);
        self.record_coding_result(
            binding,
            call.attempt,
            call.call,
            serde_json::json!({"executed":false,"complete":false,"reason":reason,"stale":true}),
            call.sources,
        )
    }
    pub fn select_coding_paths(&mut self, binding: &ThreadBinding, call: &Call) -> Result<bool> {
        let paths = match call.name.as_str() {
            "vcp_verify" => self.verification_paths(binding)?,
            "vcp_read" => vec![std::path::PathBuf::from(
                call.arguments["path"].as_str().ok_or("read path missing")?,
            )],
            "vcp_patch" => {
                let patch = call.arguments["patch"].as_str().ok_or("patch missing")?;
                let parsed = codex_apply_patch::parse_patch(patch)?;
                let mut paths = Vec::new();
                for hunk in parsed.hunks {
                    match hunk {
                        codex_apply_patch::Hunk::AddFile { path, .. }
                        | codex_apply_patch::Hunk::DeleteFile { path } => paths.push(path),
                        codex_apply_patch::Hunk::UpdateFile {
                            path, move_path, ..
                        } => {
                            paths.push(path);
                            paths.extend(move_path);
                        }
                    }
                }
                paths
            }
            "vcp_exec" => vec![
                std::path::PathBuf::from(
                    call.arguments["directory"]
                        .as_str()
                        .ok_or("process directory missing")?,
                )
                .join(".vcp-context-scope"),
            ],
            _ => return Ok(true),
        };
        self.validate_coding_sources(binding)?;
        let instructions = self.tool_root()?.instructions(
            &paths,
            &self.instruction_parents(binding)?,
            256 * 1024,
        )?;
        let remaining = self.coding_remaining().ok_or("coding deadline missing")?;
        let state = self
            .coding
            .get_mut(&binding.scope.task)
            .ok_or("coding setup missing")?;
        if call.name == "vcp_exec"
            && call.arguments["timeout_ms"]
                .as_u64()
                .is_none_or(|limit| u128::from(limit) > remaining.as_millis())
        {
            return Err("process deadline exceeds remaining coding time".into());
        }
        let covered = instructions
            .probes
            .iter()
            .all(|probe| state.probes.contains(probe));
        let mut affected = state.config.affected_paths.clone();
        affected.extend(paths);
        affected.sort();
        affected.dedup();
        if affected.len() > 256 {
            return Err("coding instruction selection limit".into());
        }
        state.config.affected_paths = affected;
        // Newly discovered nested guidance must reach the model before a
        // reissued operation. The rejected call is still retained as history.
        Ok(covered)
    }
    pub fn record_coding_result(
        &mut self,
        binding: &ThreadBinding,
        attempt: AttemptId,
        call: Call,
        result: serde_json::Value,
        mut sources: Vec<ArtifactId>,
    ) -> Result<()> {
        // Results may be recorded while held; they do not resume or dispatch.
        let call_part = self.coding_part(
            &binding.scope,
            Kind::ToolCall,
            ContextTrust::Observed,
            Content::ToolCall {
                id: call.id.clone(),
                name: call.name,
                arguments: call.arguments,
            },
        )?;
        let result_part = self.coding_part(
            &binding.scope,
            Kind::ToolResult,
            ContextTrust::Untrusted,
            Content::ToolResult {
                id: call.id,
                output: String::from_utf8(canonical_bytes(&result)?)?,
            },
        )?;
        let state = self
            .coding
            .get(&binding.scope.task)
            .ok_or("coding setup missing")?;
        let pair = Pair {
            sequence: state.pairs,
            attempt,
            parts: [call_part, result_part],
            sources: sources.clone(),
        };
        let receipt = self.capture(
            &binding.scope,
            Channel::Evidence,
            &canonical_bytes(&pair)?,
            "canonical-coding-pair/1",
        )?;
        sources.push(receipt.spec.id);
        let state = self.coding.get_mut(&binding.scope.task).unwrap();
        state.pairs += 1;
        state.history.extend(pair.parts);
        state.history_sources.extend(sources);
        Ok(())
    }
}
