// SPDX-License-Identifier: Apache-2.0
//! Bounded P0 adapter: a flat synthetic tariff, one prepared file and local
//! governed facts share the lifecycle journal. Not a production storage schema.
use crate::{Error, Lifecycle, Work};
use codex_extension_api::*;
use codex_protocol::ThreadId;
use munarium_core::{gates::run_gates, types::*};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::OpenOptions,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Charge {
    pub intent: u64,
    pub thread: ThreadId,
    pub units: u64,
    pub settled: bool,
    pub usage: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Ledger {
    pub ceiling: u64,
    pub per_request: u64,
    pub requests: Vec<Charge>,
    pub facts: Vec<Value>,
    pub effects: Vec<Value>,
}
impl Ledger {
    pub fn allocated(&self) -> Result<u64, String> {
        self.requests.iter().try_fold(0u64, |n, row| {
            n.checked_add(row.units).ok_or("budget overflow".into())
        })
    }
    pub(crate) fn check_admission(&self) -> Result<(), String> {
        if self
            .allocated()?
            .checked_add(self.per_request)
            .is_none_or(|n| n > self.ceiling)
        {
            return Err("shared fixture budget exhausted".into());
        }
        Ok(())
    }
    pub(crate) fn validate(&self, work: &[Work]) -> Result<(), String> {
        if self.per_request == 0
            || self.per_request > self.ceiling
            || self.allocated()? > self.ceiling
        {
            return Err("invalid budget".into());
        }
        let mut ids = std::collections::HashSet::new();
        for row in &self.requests {
            if row.units != self.per_request
                || !ids.insert(row.intent)
                || !work
                    .iter()
                    .any(|w| w.id == row.intent && w.thread == row.thread && w.kind == "Model")
            {
                return Err("reservation lacks unique canonical model intent".into());
            }
            if row.settled
                && !work
                    .iter()
                    .any(|w| w.id == row.intent && w.receipt.is_some())
            {
                return Err("settlement lacks atomic receipt".into());
            }
        }
        for value in &self.facts {
            serde_json::from_value::<Claim>(value.clone()).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

impl Lifecycle {
    /// Set once before startup. All registered root/child requests consume this
    /// single durable ceiling. Reopening cannot replace it with a fresh balance.
    pub fn configure_integration(&self, ceiling: u64, per_request: u64) -> Result<(), Error> {
        let mut state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
        if !state.attached
            || state.journal.is_none()
            || state.journal_closed
            || state.startups_in_flight != 0
            || state.root.is_some()
            || state.integration.is_some()
            || !state.work.is_empty()
            || per_request == 0
            || per_request > ceiling
        {
            return Err(Error::RevalidationFailed);
        }
        state.integration = Some(Ledger {
            ceiling,
            per_request,
            requests: vec![],
            facts: vec![],
            effects: vec![],
        });
        state.advance()?;
        state.checkpoint()
    }
    pub fn integration_ledger(&self) -> Result<Ledger, Error> {
        self.0
            .state
            .lock()
            .map_err(|_| Error::Poisoned)?
            .integration
            .clone()
            .ok_or(Error::RevalidationFailed)
    }
}

#[derive(Clone)]
pub struct WorkspaceTools {
    host: Lifecycle,
    root: PathBuf,
    verifier: PathBuf,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Read,
    Prepare { patch: String },
    Apply { patch: String, expected: String },
    Verify,
    Remember { workspace: String, value: String },
    Recall { workspace: String },
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Explicit offline qualification profile. Retains the Responses transport
/// used by the OpenRouter candidate, but can only address the injected loopback
/// fixture. No ambient credentials, retries, hooks, remote memory or telemetry.
pub fn configure_fixture_provider(config: &mut codex_core::config::Config) {
    let url = config
        .model_provider
        .base_url
        .as_deref()
        .expect("explicit fixture URL");
    assert!(is_fixture_endpoint(url), "P0 profile accepts loopback only");
    for feature in codex_features::FEATURES {
        config
            .features
            .disable(feature.id)
            .expect("fixture feature ceiling");
    }
    config.analytics_enabled = Some(false);
    config.otel = Default::default();
    config.otel.metrics_exporter = config.otel.exporter.clone();
    config.notify = None;
    config.orchestrator_skills_enabled = false;
    config.orchestrator_mcp_enabled = false;
    assert!(
        config.mcp_servers.get().is_empty(),
        "fixture cannot load MCP credentials"
    );
    let provider = &mut config.model_provider;
    provider.name = "OpenRouter Responses candidate (offline fixture)".into();
    provider.env_key = None;
    provider.env_http_headers = None;
    provider.http_headers = None;
    provider.auth = None;
    provider.aws = None;
    provider.requires_openai_auth = false;
    provider.experimental_bearer_token = Some("public-synthetic-p0-token".into());
    provider.request_max_retries = Some(0);
    provider.stream_max_retries = Some(0);
    provider.supports_websockets = false;
}

fn is_fixture_endpoint(url: &str) -> bool {
    url.strip_prefix("http://")
        .and_then(|rest| rest.split('/').next())
        .and_then(|authority| authority.parse::<std::net::SocketAddr>().ok())
        .is_some_and(|address| {
            address.ip() == std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
                && address.port() != 0
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn fixture_profile_rejects_ambiguous_authority_and_volatile_accounting() {
        assert!(is_fixture_endpoint("http://127.0.0.1:12345/v1"));
        for url in [
            "http://127.0.0.1:12345@outside.invalid/v1",
            "https://127.0.0.1:12345/v1",
            "http://127.0.0.1:0/v1",
            "http://outside.invalid/v1",
        ] {
            assert!(!is_fixture_endpoint(url));
        }
        let (host, owner) = Lifecycle::new(std::time::Duration::from_secs(1));
        assert!(host.configure_integration(100, 100).is_err());
        owner.close().await.unwrap();
    }
}
impl WorkspaceTools {
    pub fn new(host: Lifecycle, root: &Path, verifier: &Path) -> std::io::Result<Self> {
        let root = root.canonicalize()?;
        if !verifier.is_absolute() || !verifier.is_file() {
            return Err(std::io::Error::other("explicit native verifier required"));
        }
        Ok(Self {
            host,
            root,
            verifier: verifier.into(),
        })
    }
    fn file(&self) -> Result<std::fs::File, String> {
        let path = self.root.join("fixture.txt");
        let metadata = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("reparse target denied".into());
            }
        }
        if !metadata.is_file()
            || metadata.len() > 65536
            || path.canonicalize().map_err(|e| e.to_string())? != path
        {
            return Err("outside bounded file scope".into());
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        file.try_lock().map_err(|e| e.to_string())?;
        Ok(file)
    }
    fn patch(patch: &str, before: &str) -> Result<String, String> {
        if patch.len() > 65536 {
            return Err("patch limit".into());
        }
        let parsed = codex_apply_patch::parse_patch(patch).map_err(|e| e.to_string())?;
        let [codex_apply_patch::Hunk::UpdateFile {
            path,
            move_path: None,
            chunks,
        }] = parsed.hunks.as_slice()
        else {
            return Err("only one existing file update is qualified".into());
        };
        let [chunk] = chunks.as_slice() else {
            return Err("only one whole-file chunk is qualified".into());
        };
        if path != Path::new("fixture.txt")
            || chunk.change_context.is_some()
            || format!("{}\n", chunk.old_lines.join("\n")) != before
        {
            return Err("patch source differs from prepared content".into());
        }
        Ok(format!("{}\n", chunk.new_lines.join("\n")))
    }
    /// Exposed for negative contract tests; actual loop dispatch also has the
    /// retained tool gate and durable work intent before entering this adapter.
    pub async fn execute(&self, arguments: &str) -> Result<Value, String> {
        if arguments.len() > 131072 {
            return Err("argument limit".into());
        }
        let request: Request = serde_json::from_str(arguments).map_err(|e| e.to_string())?;
        let root_id = self
            .host
            .root()
            .map_err(|e| format!("{e:?}"))?
            .ok_or("missing root")?;
        if matches!(request, Request::Verify) {
            #[cfg(windows)]
            {
                let process = self
                    .host
                    .spawn_process(
                        root_id,
                        &self.verifier,
                        &["verify".into(), self.root.as_os_str().into()],
                        &self.root,
                        &Default::default(),
                        4096,
                    )
                    .map_err(|e| e.to_string())?;
                let outcome =
                    tokio::time::timeout(std::time::Duration::from_secs(10), process.wait())
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| e.to_string())?;
                let result = json!({"op":"verify","exit_code":outcome.exit_code,"stdout":String::from_utf8_lossy(&outcome.stdout.bytes)});
                let mut state = self.host.0.state.lock().map_err(|_| "poisoned")?;
                if state.journal_closed {
                    return Err("owner closed before result".into());
                }
                state
                    .integration
                    .as_mut()
                    .ok_or("missing ledger")?
                    .effects
                    .push(result.clone());
                state.advance().map_err(|e| format!("{e:?}"))?;
                state.checkpoint().map_err(|e| format!("{e:?}"))?;
                return Ok(result);
            }
            #[cfg(not(windows))]
            return Err("native Windows verification required".into());
        }
        // Same owner lock covers revision revalidation, bounded file effect and
        // canonical acknowledgement. Partial write errors leave dispatch unknown.
        let mut state = self.host.0.state.lock().map_err(|_| "poisoned")?;
        if !state.attached || state.held(root_id) {
            return Err("owner paused or closed".into());
        }
        let workspace = state.workspace.clone();
        let revision = state.revision;
        let ledger = state.integration.as_mut().ok_or("missing ledger")?;
        let result = match request {
            Request::Recall {
                workspace: requested,
            } => {
                if requested != workspace {
                    return Err("workspace mismatch".into());
                }
                json!({"facts":ledger.facts,"authority":"evidence only"})
            }
            Request::Remember {
                workspace: requested,
                value,
            } => {
                if requested != workspace || value.len() > 4096 {
                    return Err("workspace or value limit".into());
                }
                let facts: Vec<Claim> = ledger
                    .facts
                    .iter()
                    .map(|v| serde_json::from_value(v.clone()).unwrap())
                    .collect();
                let snapshot = MeshSnapshot {
                    facts,
                    ..Default::default()
                };
                let candidate = Candidate {
                    scope_path: Some(workspace.clone()),
                    claims: vec![ProposedClaim {
                        claim_type: ClaimType::Fact,
                        subject: "fixture".into(),
                        key: "description".into(),
                        value: value.clone(),
                        supersedes_id: None,
                    }],
                    ..Default::default()
                };
                let findings = run_gates(&snapshot, &candidate);
                let status = if findings.iter().any(|f| f.severity == Severity::Block) {
                    ClaimStatus::Disputed
                } else {
                    ClaimStatus::Accepted
                };
                let claim = Claim {
                    id: format!("claim-{revision}"),
                    version_id: workspace.clone(),
                    seq: revision,
                    claim_type: ClaimType::Fact,
                    subject: "fixture".into(),
                    key: "description".into(),
                    value,
                    scope_path: Some(workspace),
                    status,
                    provenance: Provenance::Emergent,
                    supersedes_id: None,
                    entity_id: None,
                    evidence: Some(json!({"origin":"model proposal","revision":revision})),
                    confidence: None,
                    shape_ref: None,
                    origin: None,
                };
                ledger
                    .facts
                    .push(serde_json::to_value(&claim).map_err(|e| e.to_string())?);
                json!({"claim":claim,"findings":findings})
            }
            Request::Read | Request::Prepare { .. } | Request::Apply { .. } => {
                let mut file = self.file()?;
                let mut before = String::new();
                (&mut file)
                    .take(65537)
                    .read_to_string(&mut before)
                    .map_err(|e| e.to_string())?;
                if before.len() > 65536 {
                    return Err("file grew beyond ceiling".into());
                }
                match request {
                    Request::Read => json!({"text":before,"sha256":digest(before.as_bytes())}),
                    Request::Prepare { patch } => {
                        let after = Self::patch(&patch, &before)?;
                        json!({"op":"prepare","expected":digest(before.as_bytes()),"patch_sha256":digest(patch.as_bytes()),"result_sha256":digest(after.as_bytes())})
                    }
                    Request::Apply { patch, expected } => {
                        if digest(before.as_bytes()) != expected {
                            return Err("stale prepared revision; user edits preserved".into());
                        }
                        let patch_hash = digest(patch.as_bytes());
                        if !ledger.effects.iter().any(|row| {
                            row["op"] == "prepare"
                                && row["expected"] == expected
                                && row["patch_sha256"] == patch_hash
                        }) {
                            return Err("exact patch arguments were not prepared".into());
                        }
                        if ledger.effects.iter().any(|row| {
                            row["op"] == "apply"
                                && row["before"] == expected
                                && row["patch_sha256"] == patch_hash
                        }) {
                            return Err("prepared mutation already consumed".into());
                        }
                        let after = Self::patch(&patch, &before)?;
                        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
                        file.write_all(after.as_bytes())
                            .map_err(|e| e.to_string())?;
                        file.set_len(after.len() as u64)
                            .map_err(|e| e.to_string())?;
                        file.sync_all().map_err(|e| e.to_string())?;
                        json!({"op":"apply","before":expected,"after":digest(after.as_bytes()),"bytes":after.len(),"patch_sha256":patch_hash})
                    }
                    _ => unreachable!(),
                }
            }
            Request::Verify => unreachable!(),
        };
        ledger.effects.push(result.clone());
        state.advance().map_err(|e| format!("{e:?}"))?;
        state.checkpoint().map_err(|e| format!("{e:?}"))?;
        Ok(result)
    }
}
impl ToolContributor for WorkspaceTools {
    fn tools(
        &self,
        _: &ExtensionData,
        _: &ExtensionData,
    ) -> Vec<Arc<dyn for<'call> ToolExecutor<ToolCall<'call>>>> {
        vec![Arc::new(self.clone())]
    }
}
impl<'call> ToolExecutor<ToolCall<'call>> for WorkspaceTools {
    fn tool_name(&self) -> ToolName {
        ToolName::plain("vcp_workspace")
    }
    fn spec(&self) -> ToolSpec {
        ToolSpec::Function(ResponsesApiTool {
            name: "vcp_workspace".into(),
            description: "Bounded P0 read/prepare/apply/verify/remember/recall operations.".into(),
            strict: false,
            parameters: Default::default(),
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
            let result = self
                .execute(&arguments)
                .await
                .map_err(FunctionCallError::RespondToModel)?;
            Ok(Box::new(JsonToolOutput::new(result)) as Box<dyn ToolOutput>)
        })
    }
}
