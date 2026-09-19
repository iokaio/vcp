// SPDX-License-Identifier: Apache-2.0
//! Retained tool wrappers; canonical assembly owns the actual provider body.
use super::*;
use codex_extension_api::*;
use serde_json::{json, Value};

/// Explicit seams for later memory/group implementations. The P2 driver cannot
/// infer readiness or silently create a helper model request.
#[derive(Clone, Copy, Debug, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    NotReady,
    FixedQualifiedModel,
}
#[derive(Clone, Debug, serde::Serialize, PartialEq, Eq)]
pub struct Capabilities {
    pub memory: CapabilityState,
    pub routing: CapabilityState,
}
pub fn capabilities() -> Capabilities {
    Capabilities {
        memory: CapabilityState::NotReady,
        routing: CapabilityState::FixedQualifiedModel,
    }
}

/// Trusted per-owner setup. It cannot be deserialized from model arguments.
#[derive(Clone, serde::Serialize)]
pub struct CodingConfig {
    pub operating: String,
    pub affected_paths: Vec<PathBuf>,
    /// Cumulative root requests, including children and helpers, across reopen.
    pub max_requests: u32,
    /// Absolute UTC milliseconds; owner setup permits at most one hour.
    pub deadline: Timestamp,
}
impl CodingConfig {
    pub(crate) fn validate(&self, now: Timestamp) -> Result<(), String> {
        if self.operating.trim().is_empty()
            || self.operating.len() > 64 * 1024
            || self.affected_paths.is_empty()
            || self.affected_paths.len() > 256
            || self.max_requests == 0
            || self.max_requests > 128
            || self.deadline <= now
            || self.deadline.get().saturating_sub(now.get()) > 3_600_000
        {
            return Err("coding configuration bounds or deadline rejected".into());
        }
        for path in &self.affected_paths {
            vcp_repository::path::relative(path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
pub fn schemas() -> Value {
    let mut schemas = vcp_tools::schema::definitions();
    schemas
        .as_array_mut()
        .unwrap()
        .push(vcp_tools::process::definition());
    schemas.as_array_mut().unwrap().push(json!({
        "type":"function","name":"vcp_verify","strict":true,
        "description":"Run the owner's configured acceptance checks against current sources. For unchanged analysis, cite complete same-task artifact IDs. This records evidence; it cannot declare completion.",
        "parameters":{"type":"object","properties":{"citations":{"type":"array","items":{"type":"string"}}},"required":["citations"],"additionalProperties":false}
    }));
    schemas
}
pub fn allowed_tools() -> AllowedTools {
    AllowedTools(
        [
            "vcp_read",
            "vcp_list",
            "vcp_search",
            "vcp_patch",
            "vcp_exec",
            "vcp_verify",
        ]
        .into_iter()
        .map(ToolName::plain)
        .collect(),
    )
}
impl CanonicalHost {
    /// Record the actual submitted user input before asking the retained
    /// controller to start a turn. Request/tool callbacks advance its stages.
    pub fn begin_coding_turn(&self, thread: ThreadId, input: String) -> Result<TurnId, String> {
        let binding = self.binding(thread)?;
        self.worker
            .run(move |context| context.begin_coding_turn(&binding, input))
    }
    /// Explicit read grants for ancestor AGENTS.md files only. Configure after
    /// coding setup and before its first request or verification baseline.
    pub fn configure_instruction_roots(
        &self,
        thread: ThreadId,
        parents: Vec<vcp_repository::Root>,
    ) -> Result<(), String> {
        let binding = self.binding(thread)?;
        self.worker
            .run(move |context| context.configure_instruction_roots(&binding, parents))
    }
    /// Owning turn driver calls this after retained TurnComplete. The host picks
    /// the latest observed verification; model arguments cannot select old proof.
    pub fn complete_coding_turn(&self, thread: ThreadId) -> Result<CommandReceipt, String> {
        self.complete_when_quiescent(thread, None)
    }
    /// Explicit owner setup; never supplied by a model tool. Requires an
    /// existing verification baseline so current diff/base stays authoritative.
    pub fn configure_continuity(
        &self,
        thread: ThreadId,
        config: vcp_context::compaction::Config,
    ) -> Result<(), String> {
        let binding = self.binding(thread)?;
        self.worker
            .run(move |context| context.configure_continuity(&binding, config))
    }
    pub fn configure_coding(&self, thread: ThreadId, config: CodingConfig) -> Result<(), String> {
        let binding = self.binding(thread)?;
        self.worker
            .run(move |context| context.configure_coding(&binding, config))
    }
}
struct Wrapper {
    host: CanonicalHost,
    thread: ThreadId,
    name: String,
    definition: Value,
}
impl ToolContributor for CanonicalHost {
    fn tools(
        &self,
        thread: &ExtensionData,
        _: &ExtensionData,
    ) -> Vec<Arc<dyn for<'call> ToolExecutor<ToolCall<'call>>>> {
        let Ok(thread) = ThreadId::from_string(thread.level_id()) else {
            return vec![];
        };
        schemas()
            .as_array()
            .unwrap()
            .iter()
            .map(|definition| {
                Arc::new(Wrapper {
                    host: self.clone(),
                    thread,
                    name: definition["name"].as_str().unwrap().into(),
                    definition: definition.clone(),
                }) as Arc<dyn for<'call> ToolExecutor<ToolCall<'call>>>
            })
            .collect()
    }
}
impl<'call> ToolExecutor<ToolCall<'call>> for Wrapper {
    fn supports_parallel_tool_calls(&self) -> bool {
        // Only VCP's prepared resource scheduler may decide which effects
        // overlap. Verification remains an isolated retained operation.
        self.name != "vcp_verify"
    }
    fn tool_name(&self) -> ToolName {
        ToolName::plain(self.name.clone())
    }
    fn spec(&self) -> ToolSpec {
        ToolSpec::Function(ResponsesApiTool {
            name: self.name.clone(),
            description: self.definition["description"].as_str().unwrap().into(),
            strict: true,
            parameters: serde_json::from_value(self.definition["parameters"].clone())
                .expect("registered bounded schema"),
            output_schema: None,
            defer_loading: None,
        })
    }
    fn handle<'a>(&'a self, call: ToolCall<'call>) -> ToolExecutorFuture<'a>
    where
        'call: 'a,
    {
        Box::pin(async move {
            let ToolPayload::Function { arguments } = call.payload else {
                return Err(FunctionCallError::RespondToModel(
                    "function arguments required".into(),
                ));
            };
            let binding = self
                .host
                .binding(self.thread)
                .map_err(FunctionCallError::RespondToModel)?;
            let scoped = binding.clone();
            let id = call.call_id.to_string();
            let name = self.name.clone();
            let input = arguments.clone();
            let (attempt, normalized, mut sources) = self
                .host
                .worker
                .run(move |context| context.consume_coding_call(&scoped, &id, &name, &input))
                .map_err(FunctionCallError::RespondToModel)?;
            let result: Result<Value, String> = async {
                let scoped = binding.clone();
                let selected = normalized.clone();
                if !self.host.worker.run(move |context| context.select_coding_paths(&scoped, &selected))? {
                    return Ok(json!({"executed":false,"reason":"New instruction scope selected. Review the refreshed context before issuing this operation again."}));
                }
                if self.name == "vcp_verify" {
                    #[derive(serde::Deserialize)]
                    #[serde(deny_unknown_fields)]
                    struct Input { citations: Vec<ArtifactId> }
                    let input: Input = serde_json::from_str(&arguments).map_err(|e| e.to_string())?;
                    let report = self.host.verify(self.thread, input.citations).await?;
                    sources.extend(report.outputs.clone());
                    sources.extend(report.checks.iter().map(|c| c.output.clone()));
                    return Ok(json!({"verification":report,"complete":false}));
                }
                if self.name == "vcp_exec" {
                    let request = vcp_tools::process::Request::from_arguments(&arguments).map_err(|e| e.to_string())?;
                    let proposal = self.host.prepare_process(self.thread, request)?;
                    if !matches!(proposal.decision, vcp_policy::Decision::Allow { .. }) {
                        return Ok(json!({"executed":false,"decision":format!("{:?}", proposal.decision),"question":proposal.question,"effect":proposal.effect()}));
                    }
                    let outcome = self.host.schedule_process(proposal).await?.wait().await?;
                    sources.extend([outcome.evidence.spec.id.clone(), outcome.stdout.spec.id.clone(), outcome.stderr.spec.id.clone()]);
                    return Ok(json!({"effect":outcome.effect,"evidence":outcome.evidence.spec.id,"exit_code":outcome.exit_code,"reason":outcome.reason,
                        "stdout":{"artifact":outcome.stdout.spec.id,"bytes":outcome.stdout.length,"tail":String::from_utf8_lossy(&outcome.stdout_tail),"truncated":outcome.stdout.length.get()>outcome.stdout_tail.len() as u64},
                        "stderr":{"artifact":outcome.stderr.spec.id,"bytes":outcome.stderr.length,"tail":String::from_utf8_lossy(&outcome.stderr_tail),"truncated":outcome.stderr.length.get()>outcome.stderr_tail.len() as u64}}));
                }
                let request = vcp_tools::Request::from_call(&self.name, &arguments).map_err(|e| e.to_string())?;
                let proposal = self.host.prepare_tool(self.thread, request)?;
                if !matches!(proposal.decision, vcp_policy::Decision::Allow { .. }) {
                    return Ok(json!({"executed":false,"decision":format!("{:?}", proposal.decision),"question":proposal.question,"effect":proposal.effect()}));
                }
                let outcome = self.host.schedule_tool(proposal).await?;
                sources.push(outcome.evidence.spec.id.clone());
                Ok(json!({"effect":outcome.effect,"evidence":outcome.evidence.spec.id,"result":outcome.result}))
            }.await;
            let result = match result {
                Ok(result) => result,
                Err(error) => json!({"error":error,"complete":false}),
            };
            let recorded = result.clone();
            self.host
                .worker
                .run_cleanup(move |context| {
                    context.record_coding_result(&binding, attempt, normalized, recorded, sources)
                })
                .inspect_err(|_| self.host.worker.fence())
                .map_err(FunctionCallError::RespondToModel)?;
            Ok(Box::new(JsonToolOutput::new(result)) as Box<dyn ToolOutput>)
        })
    }
}
