// SPDX-License-Identifier: Apache-2.0
//! Explicit qualification binary, excluded from ordinary production builds.
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::BTreeSet,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
use vcp_domain::{
    accounting::*,
    task::{Objective, TaskState},
    verification::Fingerprint,
    workspace::*,
    *,
};
use vcp_lifecycle::foundation::{
    conformance::{self, Probe},
    CanonicalHost, Config, ThreadBinding,
};
use vcp_models::{
    catalog::CandidateMetadata,
    stream::{ResultBody, Status},
};
use vcp_protocol::{canonical_bytes, command::Command, digest_bytes};
use vcp_store::{contract::Collection, BackendKind};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    catalog: PathBuf,
    catalog_sha256: String,
    model: String,
    endpoint: String,
    request_price_limit: String,
    cap_usd: String,
    max_output_tokens: u32,
    observed_at: Timestamp,
    valid_until: Timestamp,
}
fn fresh_file(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(&canonical_bytes(value)?)?;
    file.sync_all()?;
    Ok(())
}
fn now() -> Timestamp {
    vcp_cli::settings::now()
}
fn reject_links(path: &Path) -> Result<()> {
    for p in path.ancestors().filter(|p| p.exists()) {
        let metadata = std::fs::symlink_metadata(p)?;
        if metadata.file_type().is_symlink() {
            return Err("symlinked qualification path".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("reparse-point qualification path".into());
            }
        }
    }
    Ok(())
}
fn setup(
    output: &Path,
    spec: &Spec,
    candidate: &CandidateMetadata,
) -> Result<(
    CanonicalHost,
    vcp_lifecycle::foundation::CanonicalOwner,
    ThreadBinding,
)> {
    let workspace = output.join("workspace");
    std::fs::create_dir(&workspace)?;
    let workspace = workspace.canonicalize()?;
    let config = Config {
        canonical_root: output.join("canonical"),
        backend: BackendKind::Files,
        workspace: WorkspaceId::new(),
        session: SessionId::new(),
        binding: Binding {
            host: HostId::new(),
            root: workspace.to_string_lossy().into_owned(),
            repository: "public-synthetic-conformance".into(),
            worktree: "isolated".into(),
            revision: Revision::ZERO,
        },
        actor: ActorId::new(),
        root_task: TaskId::new(),
        cap: Money {
            currency: "USD".to_owned().try_into()?,
            micros: vcp_cli::args::parse_usd(&spec.cap_usd)?,
        },
        protected: Micros::ZERO,
        price: candidate.price.clone(),
        input_ceiling: candidate.max_input,
        output_ceiling: Units::new(u64::from(spec.max_output_tokens)),
        artifact_limit: ByteCount::new(vcp_store::artifact::DEFAULT_ARTIFACT_LIMIT),
        max_transport_retries: 0,
        host_tool_denials: vec![],
    };
    let (host, owner) = CanonicalHost::open(config.clone())?;
    let fingerprint = Fingerprint {
        repository: digest_bytes(b"public-conformance-probe"),
        buffers: digest_bytes(b"no-editor"),
        environment: digest_bytes(b"explicit-qualification-binary"),
    };
    host.command(Command::CreateTask{root:config.root_task.clone(),parent:None,fork_origin:None,objective:Objective{text:"Qualify only the fixed public echo-tool Responses probe; no filesystem effects, retries or shipping model groups.".into(),constraints:vec![],acceptance:vec!["capture actual provider charge and exact fixed tool continuation".into()],source:EventId::new(),steering:SteeringRevision::ZERO},fingerprint,editing:false,required_checks:vec![]},Some(config.root_task.clone()),Revision::ZERO)?;
    host.command(
        Command::Transition {
            next: TaskState::Running,
            reason: "explicit capped conformance probe".into(),
            verification: None,
        },
        Some(config.root_task.clone()),
        Revision::ZERO,
    )?;
    host.initialize_root_budget()?;
    let binding = ThreadBinding {
        scope: Scope {
            workspace: config.workspace,
            session: config.session,
            task: config.root_task,
        },
        agent: AgentId::new(),
        role: RequestRole::Main,
    };
    Ok((host, owner, binding))
}
async fn request(
    host: &CanonicalHost,
    binding: &ThreadBinding,
    candidate: &CandidateMetadata,
    probe: Probe,
    client: &reqwest::Client,
    endpoint: &str,
    key: &str,
) -> Result<ResultBody> {
    let mut lease = host.admit_conformance(binding.clone(), candidate.clone(), probe)?;
    let bytes = canonical_bytes(lease.body())?;
    let mut response = client
        .post(endpoint)
        .bearer_auth(key)
        .header("content-type", "application/json")
        .body(bytes)
        .send()
        .await?;
    if !response.status().is_success() {
        // Preserve observed HTTP error bytes; unknown cost remains reserved.
        let mut total = 0usize;
        while let Some(chunk) = response.chunk().await? {
            total = total.checked_add(chunk.len()).ok_or("response overflow")?;
            if total > 1024 * 1024 {
                break;
            }
            let _ = lease.capture(&chunk);
        }
        return Err("provider HTTP error; canonical liability retained, no retry".into());
    }
    while let Some(chunk) = response.chunk().await? {
        lease.capture(&chunk)?;
    }
    Ok(lease.finish()?)
}
async fn execute(
    spec: &Spec,
    output: &Path,
    key: &str,
    endpoint: &str,
) -> Result<serde_json::Value> {
    let cap = vcp_cli::args::parse_usd(&spec.cap_usd)?;
    if cap == Micros::ZERO
        || cap.get() > 25_000_000
        || spec.max_output_tokens == 0
        || spec.max_output_tokens > 512
        || spec.valid_until <= now()
        || spec.observed_at > now()
        || spec
            .valid_until
            .get()
            .saturating_sub(spec.observed_at.get())
            > 86_400_000
    {
        return Err("probe cap, output or dated catalog bounds rejected".into());
    }
    reject_links(&spec.catalog)?;
    let raw = vcp_cli::settings::read_bounded(&spec.catalog, 4 * 1024 * 1024)?;
    if digest_bytes(&raw) != spec.catalog_sha256 {
        return Err("authorized raw catalog identity changed".into());
    }
    let candidate = CandidateMetadata::from_endpoints(
        &raw,
        spec.observed_at,
        spec.valid_until,
        spec.model.clone(),
        spec.endpoint.clone(),
        spec.request_price_limit.clone(),
        BTreeSet::from(["tools".into(), "max_tokens".into()]),
    )?;
    let (host, owner, binding) = setup(output, spec, &candidate)?;
    fresh_file(&output.join("candidate.json"), &candidate)?;
    let mut catalog = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("catalog.json"))?;
    catalog.write_all(&raw)?;
    catalog.sync_all()?;
    drop(catalog);
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .no_proxy()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(120))
        .build()?;
    let result = async {
        let first = request(
            &host,
            &binding,
            &candidate,
            Probe::ToolCall,
            &client,
            endpoint,
            key,
        )
        .await?;
        if first.status != Status::Completed
            || first.served_model.as_deref() != Some(&spec.model)
            || first.calls.len() != 1
            || first.calls[0].name != "vcp_conformance_echo"
            || first.calls[0].arguments != json!({"marker":conformance::MARKER})
        {
            return Err::<_, Box<dyn std::error::Error>>(
                "first response failed exact model/tool contract; charge retained".into(),
            );
        }
        let second = request(
            &host,
            &binding,
            &candidate,
            Probe::Continuation(first.calls[0].clone()),
            &client,
            endpoint,
            key,
        )
        .await?;
        if second.status != Status::Completed
            || second.served_model.as_deref() != Some(&spec.model)
            || !second.calls.is_empty()
            || second
                .completed_messages
                .values()
                .cloned()
                .collect::<Vec<_>>()
                .join("")
                .trim()
                != conformance::FINAL
        {
            return Err("continuation failed exact final marker contract; charge retained".into());
        }
        Ok(vec![first, second])
    }
    .await;
    let state = host.snapshot()?;
    let records: Vec<_> = state
        .records
        .values()
        .filter(|r| {
            matches!(
                r.collection,
                Collection::Task
                    | Collection::Attempt
                    | Collection::Reservation
                    | Collection::Settlement
                    | Collection::Ledger
                    | Collection::Artifact
            )
        })
        .collect();
    fresh_file(&output.join("canonical-records.json"), &records)?;
    let ledger: Ledger = state
        .record(
            Collection::Ledger,
            binding.scope.task.as_str(),
            &binding.scope.workspace,
        )?
        .decode()?;
    let accounted =
        ledger.active == Micros::ZERO && ledger.unresolved == Micros::ZERO && !ledger.overrun;
    let report = match result {
        Ok(responses) => {
            json!({"schema":"p6-provider-conformance/1","status":if accounted{"observed"}else{"failed"},"scope":binding.scope,"candidate":candidate,"responses":responses,
            "actual_cost_micros":if accounted{Some(ledger.settled)}else{None},"ledger":ledger,
            "responses_text_tools":accounted,"byte_ceiling_qualified":false,
            "provider_preferences_qualified":accounted && responses.iter().all(|r|r.served_provider.as_deref()==Some(spec.endpoint.as_str())),
            "limitations":["Two fixed synthetic probes, no statistical role/group quality claim.","Provider policy not proven when exact served endpoint is absent; requested pin is not served identity.","Byte count is not tokenizer qualification; downstream admission must retain full-endpoint input bounds.","Conformance report does not publish a production Snapshot or profile."]})
        }
        Err(error) => {
            json!({"schema":"p6-provider-conformance/1","status":"failed","scope":binding.scope,"candidate":candidate,"actual_cost_micros":if accounted{Some(ledger.settled)}else{None},"ledger":ledger,"error":error.to_string(),"responses_text_tools":false,"byte_ceiling_qualified":false,"provider_preferences_qualified":false})
        }
    };
    fresh_file(&output.join("result.json"), &report)?;
    owner.close().await?;
    Ok(report)
}
#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err("Usage: vcp-provider-conformance <spec.json> <new-private-output-directory> <authorized-spec-sha256>".into());
    }
    let spec_file = PathBuf::from(&args[0]);
    reject_links(&spec_file)?;
    let bytes = vcp_cli::settings::read_bounded(&spec_file, 1024 * 1024)?;
    if args[2].to_str() != Some(digest_bytes(&bytes).as_str()) {
        return Err("explicit authorization must match spec SHA256".into());
    }
    let spec: Spec = serde_json::from_slice(&bytes)?;
    let output = std::path::absolute(PathBuf::from(&args[1]))?;
    reject_links(&output)?;
    if output
        .components()
        .any(|p| matches!(p, std::path::Component::ParentDir))
    {
        return Err("parent traversal in output path".into());
    }
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .ok_or("repository root unavailable")?;
    if output.starts_with(repository) || repository.starts_with(&output) {
        return Err("live output must be private and outside the source repository".into());
    }
    let output = vcp_cli::settings::local_path(&output, &repository.canonicalize()?)?;
    std::fs::create_dir(&output)?;
    let binary = std::env::current_exe()?;
    let binary_bytes = vcp_cli::settings::read_bounded(&binary, 1024 * 1024 * 1024)?;
    fresh_file(
        &output.join("claim.json"),
        &json!({"spec_sha256":digest_bytes(&bytes),"binary_sha256":digest_bytes(&binary_bytes),"source_sha256":{
            "binary":digest_bytes(include_bytes!("vcp-provider-conformance.rs")),
            "lease":digest_bytes(include_bytes!("../../../vcp-lifecycle/src/foundation/conformance.rs")),
            "settlement":digest_bytes(include_bytes!("../../../vcp-lifecycle/src/foundation/worker/conformance.rs")),
            "catalog":digest_bytes(include_bytes!("../../../vcp-models/src/catalog.rs"))
        },"spec":spec,"claimed_at":now(),"scope":"one-shot max two requests; shared user budget is coordinator-owned"}),
    )?;
    let key = std::env::var("OPENROUTER_API_KEY").map_err(|_| "OPENROUTER_API_KEY is required")?;
    let report = execute(
        &spec,
        &output,
        &key,
        "https://openrouter.ai/api/v1/responses",
    )
    .await?;
    println!(
        "{}",
        json!({"status":report["status"],"actual_cost_micros":report["actual_cost_micros"],"result":output.join("result.json")})
    );
    if report["status"] != "observed" {
        std::process::exit(1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };
    #[tokio::test]
    async fn fixed_tool_probe_uses_canonical_observed_cost_and_preserves_unknown() {
        for missing_cost in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let catalog = temp.path().join("input-catalog.json");
            std::fs::write(&catalog,canonical_bytes(&json!({"data":{"id":"fixture/probe","endpoints":[{"tag":"fixture/region","status":0,"context_length":32000,"max_prompt_tokens":24000,"max_completion_tokens":8000,"supported_parameters":["tools","max_tokens"],"pricing":{"prompt":"0","completion":"0","request":"0.001"}}]}})).unwrap()).unwrap();
            let spec = Spec {
                catalog_sha256: digest_bytes(&std::fs::read(&catalog).unwrap()),
                catalog,
                model: "fixture/probe".into(),
                endpoint: "fixture/region".into(),
                request_price_limit: "0.001".into(),
                cap_usd: "0.01".into(),
                max_output_tokens: 128,
                observed_at: now(),
                valid_until: Timestamp::new(now().get() + 60_000),
            };
            let output = temp.path().join("output");
            std::fs::create_dir(&output).unwrap();
            let server = MockServer::start().await;
            Mock::given(method("POST")).and(path("/responses")).respond_with(move |request:&wiremock::Request| {
                let body:serde_json::Value=serde_json::from_slice(&request.body).unwrap();assert_eq!(body["provider"]["only"],json!(["fixture/region"]));assert_eq!(body["provider"]["allow_fallbacks"],false);
                let continuation=body["input"].as_array().unwrap().len()>1;
                let content=if continuation{json!([{"type":"message","id":"final-message","role":"assistant","content":[{"type":"output_text","text":conformance::FINAL}]}])}else{json!([{"type":"function_call","id":"tool-item","call_id":"call-1","name":"vcp_conformance_echo","arguments":serde_json::to_string(&json!({"marker":conformance::MARKER})).unwrap()}])};
                let mut response=json!({"id":if continuation{"response-2"}else{"response-1"},"status":"completed","model":"fixture/probe","provider":"fixture/region","output":content,"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.000007}});
                if missing_cost {response["usage"].as_object_mut().unwrap().remove("cost");}
                ResponseTemplate::new(200).insert_header("content-type","text/event-stream").set_body_string(format!("data: {}\n\n",json!({"type":"response.completed","response":response})))
            }).expect(if missing_cost{1}else{2}).mount(&server).await;
            let result = execute(
                &spec,
                &output,
                "synthetic-loopback-credential",
                &format!("{}/responses", server.uri()),
            )
            .await
            .unwrap();
            assert_eq!(result["byte_ceiling_qualified"], false);
            if missing_cost {
                assert_eq!(result["status"], "failed");
                assert!(result["actual_cost_micros"].is_null());
                assert_eq!(result["ledger"]["unresolved"], "1000");
            } else {
                assert_eq!(result["status"], "observed");
                assert_eq!(result["actual_cost_micros"], "14");
                assert_eq!(result["provider_preferences_qualified"], true);
            }
            let rows: Vec<serde_json::Value> = serde_json::from_slice(
                &std::fs::read(output.join("canonical-records.json")).unwrap(),
            )
            .unwrap();
            let attempts: Vec<_> = rows
                .iter()
                .filter(|r| r["collection"] == "attempt")
                .collect();
            assert_eq!(attempts.len(), if missing_cost { 1 } else { 2 });
            for attempt in attempts {
                assert_eq!(attempt["value"]["quote"]["bounds"]["input"], "72000");
                assert_eq!(attempt["value"]["quote"]["bounds"]["cache_read"], "24000");
                assert_eq!(attempt["value"]["quote"]["amount"]["micros"], "1000");
                assert!(attempt["value"]["send_intent"].is_string());
                if !missing_cost {
                    assert_eq!(attempt["value"]["charged"], "7");
                }
            }
        }
    }
}
