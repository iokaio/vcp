// SPDX-License-Identifier: Apache-2.0
//! User-selected, bounded production renewal. No endpoint override or retry.
use super::*;

#[derive(Debug, clap::Args)]
pub struct Provider {
    /// Built-in model set supplying the model and endpoint; see `vcp setup estimate`.
    #[arg(long, conflicts_with_all = ["model", "endpoint"])]
    pub set: Option<String>,
    /// Role within --set whose model is verified; defaults to main.
    #[arg(long, value_enum, requires = "set")]
    pub role: Option<crate::model_sets::Role>,
    /// Exact OpenRouter model ID, for example organization/model.
    #[arg(long, required_unless_present = "set", requires = "endpoint")]
    pub model: Option<String>,
    /// Exact endpoint tag in the complete model endpoint catalog.
    #[arg(long, required_unless_present = "set", requires = "model")]
    pub endpoint: Option<String>,
    /// Maximum USD per provider request, including absent catalog request
    /// pricing; with --set it defaults to 0.001.
    #[arg(long, required_unless_present = "set")]
    pub request_price_limit: Option<String>,
    /// Authorize at most two fixed conformance requests within this total USD cap.
    #[arg(long)]
    pub budget_usd: String,
    /// New private directory outside workspaces and sync roots; never
    /// overwritten. Defaults to a new folder under the data folder's `providers`.
    #[arg(long)]
    pub output: Option<PathBuf>,
}

/// The exact model, endpoint and request ceiling a provider command verifies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub model: String,
    pub endpoint: String,
    pub request_price_limit: String,
}

impl Provider {
    pub fn target(&self) -> std::result::Result<Target, String> {
        // clap skips `requires` when a conflicting argument is present.
        if self.set.is_none() && self.role.is_some() {
            return Err("--role applies only with --set".into());
        }
        if let Some(id) = &self.set {
            let set = crate::model_sets::find(id)?;
            let member = set
                .member(self.role.unwrap_or(crate::model_sets::Role::Main))
                .ok_or("model set has no main member")?;
            return Ok(Target {
                model: member.model.clone(),
                endpoint: member.endpoint.clone(),
                request_price_limit: self
                    .request_price_limit
                    .clone()
                    .unwrap_or_else(|| crate::model_sets::REQUEST_PRICE_LIMIT.into()),
            });
        }
        match (&self.model, &self.endpoint, &self.request_price_limit) {
            (Some(model), Some(endpoint), Some(limit)) => Ok(Target {
                model: model.clone(),
                endpoint: endpoint.clone(),
                request_price_limit: limit.clone(),
            }),
            _ => Err("pass --set, or --model, --endpoint and --request-price-limit".into()),
        }
    }
}

struct Request {
    target: Target,
    budget_usd: String,
    output: PathBuf,
}

fn identifier(part: &str) -> bool {
    !part.is_empty()
        && part.len() <= 256
        && part
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-._/".contains(&c))
        && part
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
}

/// Bounds checked before any network request or file creation.
pub fn valid_selection(
    model: &str,
    endpoint: &str,
    request_price_limit: &str,
    budget_usd: &str,
) -> std::result::Result<(), String> {
    if !identifier(model) || !identifier(endpoint) {
        return Err("bounded exact model ID and endpoint tag required".into());
    }
    let cap = crate::args::parse_usd(budget_usd)?;
    let price =
        vcp_models::catalog::usd_micros(request_price_limit).map_err(|error| error.to_string())?;
    if cap == Micros::ZERO || cap.get() > 25_000_000 || price > cap.get() {
        return Err("provider setup budget must be greater than zero and at most 25 USD; request ceiling must fit it".into());
    }
    Ok(())
}

fn selection(request: &Request) -> Result<()> {
    let target = &request.target;
    valid_selection(
        &target.model,
        &target.endpoint,
        &target.request_price_limit,
        &request.budget_usd,
    )?;
    Ok(())
}

/// A new generation folder name under `<data>\providers`.
fn default_output(data: &Path, workspace: &Path, target: &Target) -> Result<PathBuf> {
    let slug = |text: &str| -> String {
        text.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .take(80)
            .collect()
    };
    let directory = crate::settings::local_path(data, workspace)?.join("providers");
    std::fs::create_dir_all(&directory)
        .map_err(|_| "providers folder cannot be created in the private data folder")?;
    Ok(directory.join(format!(
        "{}--{}--{}",
        slug(&target.model),
        slug(&target.endpoint),
        CommandId::new().as_str()
    )))
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .no_proxy()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(120))
        .build()?)
}

async fn get(client: &reqwest::Client, url: reqwest::Url, key: Option<&str>) -> Result<Vec<u8>> {
    let mut request = client.get(url);
    if let Some(key) = key {
        request = request.bearer_auth(key);
    }
    let mut response = request
        .send()
        .await
        .map_err(|_| "provider metadata request failed; no automatic retry")?;
    if !response.status().is_success() {
        return Err(
            "provider metadata unavailable or credential rejected; no automatic retry".into(),
        );
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "provider metadata read failed")?
    {
        if bytes.len().saturating_add(chunk.len()) > 4 * 1024 * 1024 {
            return Err("provider metadata exceeds 4 MiB".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    if let Some(key) = key {
        reject_credential(&bytes, key)?;
    }
    Ok(bytes)
}

fn fresh_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// The fixed provider origin. Tests inject a local origin through the private
/// `*_with` seams; no CLI or profile field can change it.
pub(crate) const OPENROUTER_API: &str = "https://openrouter.ai/api/v1";

/// The public endpoint catalog for one model. Estimates fetch it without a
/// credential; provider setup sends the credential and rejects reflection.
pub(crate) async fn fetch_catalog(model: &str, key: Option<&str>, api: &str) -> Result<Vec<u8>> {
    if !identifier(model) {
        return Err("bounded exact model ID required".into());
    }
    let mut url = reqwest::Url::parse(&format!("{api}/models/"))?;
    url.path_segments_mut()
        .map_err(|_| "provider catalog URL")?
        .pop_if_empty()
        .extend(model.split('/'))
        .push("endpoints");
    get(&client()?, url, key).await
}

/// The credential comes only from the process environment; it is never a field
/// in the persisted authorization spec. Every inference attempt uses the owner
/// ledger and retains unresolved liability on failure or interruption.
pub async fn run(
    request: &Provider,
    workspace: &Path,
    data: impl FnOnce() -> std::result::Result<PathBuf, String>,
) -> std::result::Result<serde_json::Value, String> {
    let target = request.target()?;
    valid_selection(
        &target.model,
        &target.endpoint,
        &target.request_price_limit,
        &request.budget_usd,
    )?;
    let key = credential()?;
    let output = match &request.output {
        Some(output) => output.clone(),
        None => {
            let canonical = workspace
                .canonicalize()
                .map_err(|_| "setup requires an existing accessible workspace directory")?;
            default_output(&data()?, &canonical, &target).map_err(|e| e.to_string())?
        }
    };
    let request = Request {
        target,
        budget_usd: request.budget_usd.clone(),
        output,
    };
    let result = run_with(&request, workspace, &key, OPENROUTER_API).await;
    // Do not pass arbitrary transport or provider error text to terminal logs.
    result.map_err(|error| error.to_string().replace(&key, "[redacted]"))
}

fn credential() -> std::result::Result<String, String> {
    std::env::var("OPENROUTER_API_KEY")
        .ok()
        .filter(|key| !key.is_empty() && key.len() <= 16384 && !key.chars().any(char::is_control))
        .ok_or_else(|| "supply OPENROUTER_API_KEY through a masked prompt or credential manager in this process; never use command arguments".into())
}

/// Resume receipt retrieval after delayed metadata, without repeating a model
/// call, extending expiry or changing the recorded authorization.
pub async fn complete(
    directory: &Path,
    workspace: &Path,
) -> std::result::Result<serde_json::Value, String> {
    let key = credential()?;
    let result = complete_with(directory, workspace, &key, OPENROUTER_API).await;
    result.map_err(|error| error.to_string().replace(&key, "[redacted]"))
}

async fn complete_with(
    directory: &Path,
    workspace: &Path,
    key: &str,
    api: &str,
) -> Result<serde_json::Value> {
    let workspace = workspace.canonicalize()?;
    let output = crate::settings::local_path(directory, &workspace)?;
    reject_links(&output)?;
    let root = crate::settings::registry_root(&output)?;
    let _pin = root.hold(None, true)?;
    let read =
        |name: &str| -> Result<Vec<u8>> { Ok(root.read(Path::new(name), 4 * 1024 * 1024)?.bytes) };
    let spec: Spec = serde_json::from_slice(&read("spec.json")?)?;
    let authorization: serde_json::Value = serde_json::from_slice(&read("authorization.json")?)?;
    if spec.catalog != output.join("endpoints.json")
        || spec.valid_until <= now()
        || authorization["spec_sha256"] != digest_bytes(&canonical_bytes(&spec)?)
    {
        return Err(
            "setup spec changed or expired; retained inference cannot be replayed or extended"
                .into(),
        );
    }
    let report = serde_json::from_slice(&read("result.json")?)?;
    finish(&spec, &report, &output, key, api).await
}

// `api` is a private dependency-injection seam for offline HTTP tests. The only
// production caller above supplies the fixed HTTPS origin; no CLI/profile field
// can change it.
async fn run_with(
    request: &Request,
    workspace: &Path,
    key: &str,
    api: &str,
) -> Result<serde_json::Value> {
    selection(request)?;
    let workspace = workspace.canonicalize()?;
    let output = crate::settings::local_path(&request.output, &workspace)?;
    reject_links(&output)?;
    std::fs::create_dir(&output).map_err(|_| {
        "setup output must be a new directory beneath an existing private local parent"
    })?;
    let root = crate::settings::registry_root(&output)?;
    let _pin = root.hold(None, true)?;
    let observed = now();
    let target = &request.target;
    let raw = fetch_catalog(&target.model, Some(key), api).await?;
    let catalog = output.join("endpoints.json");
    fresh_bytes(&catalog, &raw)?;
    let spec = Spec {
        catalog,
        catalog_sha256: digest_bytes(&raw),
        model: target.model.clone(),
        endpoint: target.endpoint.clone(),
        request_price_limit: target.request_price_limit.clone(),
        cap_usd: request.budget_usd.clone(),
        max_output_tokens: 512,
        observed_at: observed,
        valid_until: Timestamp::new(observed.get().saturating_add(12 * 60 * 60 * 1000)),
    };
    let spec_path = output.join("spec.json");
    fresh_file(&spec_path, &spec)?;
    fresh_file(
        &output.join("authorization.json"),
        &json!({
            "schema":"vcp-provider-setup/1", "spec_sha256":digest_bytes(&canonical_bytes(&spec)?),
            "max_requests":2,"retries":0,"cap_usd":request.budget_usd,
            "authorized_at":now(),"scope":"explicit setup provider command; one bounded probe pair"
        }),
    )?;
    let report = execute(&spec, &output, key, &format!("{api}/responses")).await?;
    finish(&spec, &report, &output, key, api).await
}

async fn finish(
    spec: &Spec,
    report: &serde_json::Value,
    output: &Path,
    key: &str,
    api: &str,
) -> Result<serde_json::Value> {
    if report["status"] != "observed" {
        return Err("provider conformance failed; preserve result.json and canonical accounting, reconcile unresolved liability before authorizing another setup".into());
    }
    let responses: Vec<ResultBody> = serde_json::from_value(report["responses"].clone())?;
    if responses.len() != 2 {
        return Err("provider conformance pair incomplete".into());
    }
    let mut sources = Vec::new();
    let client = client()?;
    for (index, response) in responses.iter().enumerate() {
        let mut url = reqwest::Url::parse(&format!("{api}/generation"))?;
        url.query_pairs_mut()
            .append_pair("id", &response.response_id);
        let path = output.join(format!("generation-{index}.json"));
        let bytes = if path.exists() {
            reject_links(&path)?;
            crate::settings::read_bounded(&path, 1024 * 1024)?
        } else {
            let bytes=get(&client,url,Some(key)).await.map_err(|_| "generation receipt unavailable; preserve this directory and use vcp setup provider-complete --directory <directory>; this does not repeat inference")?;
            fresh_bytes(&path, &bytes)?;
            bytes
        };
        sources.push(json!({"path":path,"sha256":digest_bytes(&bytes)}));
    }
    let source = |path: &Path| -> Result<serde_json::Value> {
        Ok(
            json!({"path":path,"sha256":digest_bytes(&crate::settings::read_bounded(path,4*1024*1024)?)}),
        )
    };
    let qualification = json!({
        "probe_spec":source(&output.join("spec.json"))?,"report":source(&output.join("result.json"))?,
        "generations":sources,"catalog":source(&spec.catalog)?,
        "observed_at":now(),"valid_until":spec.valid_until
    });
    let qualification_path = output.join("qualification.json");
    let qualification = if qualification_path.exists() {
        reject_links(&qualification_path)?;
        let recorded: serde_json::Value = serde_json::from_slice(&crate::settings::read_bounded(
            &qualification_path,
            1024 * 1024,
        )?)?;
        for name in [
            "probe_spec",
            "report",
            "generations",
            "catalog",
            "valid_until",
        ] {
            if recorded[name] != qualification[name] {
                return Err("recorded qualification inputs changed; completion cannot extend or replace the original evidence".into());
            }
        }
        recorded
    } else {
        fresh_file(&qualification_path, &qualification)?;
        qualification
    };
    qualify::run(
        &qualification_path,
        &output.join("qualified"),
        &digest_bytes(&canonical_bytes(&qualification)?),
    )?;
    Ok(
        json!({"status":"qualified","directory":output,"catalog":spec.catalog,
        "model":spec.model,"endpoint":spec.endpoint,
        "snapshot":output.join("qualified/snapshot.json"),"valid_until":spec.valid_until,
        "actual_cost_micros":report["actual_cost_micros"],"max_requests":2,"retries":0}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn metadata_rejects_redirects_errors_and_reflected_credentials() {
        use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        for (name, response) in [
            (
                "redirect",
                ResponseTemplate::new(302)
                    .insert_header("location", format!("{}/forbidden", server.uri())),
            ),
            ("unauthorized", ResponseTemplate::new(401)),
            (
                "reflection",
                ResponseTemplate::new(200).set_body_string("synthetic-setup-secret"),
            ),
        ] {
            Mock::given(path(format!("/{name}")))
                .respond_with(response)
                .expect(1)
                .mount(&server)
                .await;
            let error = get(
                &client().unwrap(),
                reqwest::Url::parse(&format!("{}/{name}", server.uri())).unwrap(),
                Some("synthetic-setup-secret"),
            )
            .await
            .unwrap_err();
            assert!(!error.to_string().contains("synthetic-setup-secret"));
        }
        assert_eq!(server.received_requests().await.unwrap().len(), 3);
    }
    #[cfg(windows)]
    #[tokio::test]
    async fn production_renewal_joins_actual_receipts_and_never_publishes_missing_receipt() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };
        for missing_receipt in [false, true] {
            let unavailable =
                std::sync::Arc::new(std::sync::atomic::AtomicBool::new(missing_receipt));
            let receipt_unavailable = unavailable.clone();
            let server = MockServer::start().await;
            let temp = tempfile::tempdir().unwrap();
            let workspace = temp.path().join("workspace");
            std::fs::create_dir(&workspace).unwrap();
            let request = Request {
                target: Target {
                    model: "fixture/probe".into(),
                    endpoint: "fixture".into(),
                    request_price_limit: "0.001".into(),
                },
                budget_usd: "0.01".into(),
                output: temp.path().join("renewal"),
            };
            let catalog = json!({"data":{"id":"fixture/probe","endpoints":[{"tag":"fixture","provider_name":"Fixture Provider","model_id":"fixture/probe","name":"Fixture Provider | fixture/revision","status":0,"context_length":32000,"max_prompt_tokens":24000,"max_completion_tokens":8000,"supported_parameters":["tools","tool_choice","max_tokens"],"pricing":{"prompt":"0","completion":"0","request":"0.001"}}]}});
            Mock::given(method("GET"))
                .and(path("/models/fixture/probe/endpoints"))
                .respond_with(ResponseTemplate::new(200).set_body_json(catalog))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("POST")).and(path("/responses")).respond_with(|request:&wiremock::Request|{
                assert_eq!(request.headers["authorization"],"Bearer synthetic-setup-secret");
                let body:serde_json::Value=serde_json::from_slice(&request.body).unwrap();
                assert_eq!(body["provider"]["allow_fallbacks"],false);
                let continuation=body["input"].as_array().unwrap().len()>1;
                let output=if continuation {json!([{"type":"message","id":"final-message","role":"assistant","content":[{"type":"output_text","text":conformance::FINAL}]}])}else {json!([{"type":"function_call","id":"tool-item","call_id":"call-1","name":"vcp_conformance_echo","arguments":serde_json::to_string(&json!({"marker":conformance::MARKER})).unwrap()}])};
                ResponseTemplate::new(200).set_body_string(format!("data: {}\n\n",json!({"type":"response.completed","response":{"id":if continuation{"response-2"}else{"response-1"},"status":"completed","model":"fixture/probe","provider":"Fixture Provider","output":output,"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.000007}}})))
            }).expect(2).mount(&server).await;
            Mock::given(method("GET")).and(path("/generation")).respond_with(move |request:&wiremock::Request| {
                assert_eq!(request.headers["authorization"],"Bearer synthetic-setup-secret");
                if receipt_unavailable.load(std::sync::atomic::Ordering::SeqCst) {return ResponseTemplate::new(404);}
                let id=request.url.query_pairs().find(|(name,_)|name=="id").unwrap().1.into_owned();
                ResponseTemplate::new(200).set_body_json(json!({"data":{"id":id,"model":"fixture/revision","provider_name":"Fixture Provider","cancelled":false,"streamed":true,"is_byok":false,"total_cost":0.000007,"provider_responses":[{"endpoint_id":"internal-endpoint","provider_name":"Fixture Provider","model_permaslug":"fixture/revision","status":200,"is_byok":false}]}}))
            }).expect(if missing_receipt{3}else{2}).mount(&server).await;
            let result = run_with(
                &request,
                &workspace,
                "synthetic-setup-secret",
                &server.uri(),
            )
            .await;
            let report: serde_json::Value =
                serde_json::from_slice(&std::fs::read(request.output.join("result.json")).unwrap())
                    .unwrap();
            assert_eq!(report["actual_cost_micros"], "14");
            assert_eq!(report["ledger"]["unresolved"], "0");
            let snapshot = request.output.join("qualified/snapshot.json");
            if missing_receipt {
                assert!(result.is_err());
                assert!(!snapshot.exists());
                unavailable.store(false, std::sync::atomic::Ordering::SeqCst);
                let result = complete_with(
                    &request.output,
                    &workspace,
                    "synthetic-setup-secret",
                    &server.uri(),
                )
                .await
                .unwrap();
                assert_eq!(result["status"], "qualified");
                assert!(snapshot.exists());
            } else {
                assert_eq!(result.unwrap()["status"], "qualified");
                let snapshot: vcp_models::catalog::Snapshot =
                    serde_json::from_slice(&std::fs::read(snapshot).unwrap()).unwrap();
                assert!(snapshot.current(now()).is_ok());
                assert!(snapshot.compatibility.provider_preferences_qualified);
                assert!(!snapshot.compatibility.byte_ceiling_qualified);
            }
            for file in [
                "spec.json",
                "authorization.json",
                "result.json",
                "canonical-records.json",
            ] {
                assert!(
                    !String::from_utf8(std::fs::read(request.output.join(file)).unwrap())
                        .unwrap()
                        .contains("synthetic-setup-secret")
                );
            }
        }
    }
    #[test]
    fn provider_selection_is_explicit_and_bounded_before_network_or_files() {
        let valid = |model: &str, cap: &str| valid_selection(model, "provider/region", "0.01", cap);
        assert!(valid("owner/model", "1").is_ok());
        for model in [
            "../model",
            "owner//model",
            "owner/model?key=secret",
            "owner/model\n",
            "https://host/model",
        ] {
            assert!(valid(model, "1").is_err());
        }
        for cap in ["0", "25.000001", "-1", "NaN"] {
            assert!(valid("owner/model", cap).is_err());
        }
        assert!(valid_selection("owner/model", "provider", "2", "1").is_err());
    }

    #[test]
    fn sets_supply_the_target_and_explicit_flags_still_work() {
        use clap::Parser;
        let parse = |args: &[&str]| {
            let mut argv = vec!["vcp", "setup", "provider"];
            argv.extend(args);
            crate::args::Cli::try_parse_from(argv)
        };
        let target = |args: &[&str]| match parse(args).unwrap().command {
            Some(crate::args::Command::Setup {
                command: Some(crate::onboarding::Command::Provider(provider)),
            }) => provider.target(),
            _ => panic!("expected setup provider"),
        };
        let quick = target(&["--set", "quick", "--budget-usd", "7"]).unwrap();
        assert_eq!(
            quick,
            Target {
                model: "qwen/qwen3.8-max-0902".into(),
                endpoint: "alibaba".into(),
                request_price_limit: "0.001".into()
            }
        );
        let child = target(&["--set", "qwen", "--role", "child", "--budget-usd", "2"]).unwrap();
        assert_eq!(child.model, "qwen/qwen3.8-27b");
        let explicit = target(&[
            "--model",
            "owner/model",
            "--endpoint",
            "provider",
            "--request-price-limit",
            "0.01",
            "--budget-usd",
            "1",
        ])
        .unwrap();
        assert_eq!(explicit.model, "owner/model");
        assert!(target(&["--set", "unknown", "--budget-usd", "1"]).is_err());
        for invalid in [
            &["--budget-usd", "1"][..],
            &[
                "--set",
                "quick",
                "--model",
                "owner/model",
                "--budget-usd",
                "1",
            ],
            &["--set", "quick"],
            &["--set", "quick", "--budget-usd", "1", "--api-key", "secret"],
        ] {
            assert!(parse(invalid).is_err(), "{invalid:?}");
        }
        // clap accepts a conflicting --model here; the target rejects it.
        assert!(target(&[
            "--role",
            "child",
            "--model",
            "a/b",
            "--endpoint",
            "e",
            "--request-price-limit",
            "0.01",
            "--budget-usd",
            "1",
        ])
        .unwrap_err()
        .contains("--role applies only with --set"));
    }
}
