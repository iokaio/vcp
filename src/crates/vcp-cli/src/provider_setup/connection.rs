// SPDX-License-Identifier: Apache-2.0
//! One accounted account-connection request; not model-set qualification.
use super::*;
use vcp_models::catalog::{compatibility, Snapshot};

pub const OUTPUT_TOKENS: u32 = 128;
const API: &str = "https://openrouter.ai/api/v1";

#[derive(Clone, Debug)]
pub struct PreparedModel {
    pub snapshot: Snapshot,
    pub catalog: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionReport {
    pub model: String,
    pub endpoint: String,
    pub response: String,
    pub reported_cost_micros: Micros,
    pub evidence: PathBuf,
    pub verified: Vec<String>,
}

/// Metadata only. This never invokes a model or transmits project material.
pub async fn refresh(model: &str, key: &str) -> std::result::Result<PreparedModel, String> {
    refresh_with(model, key, API)
        .await
        .map_err(|e| e.to_string().replace(key, "[redacted]"))
}

async fn refresh_with(model: &str, key: &str, api: &str) -> Result<PreparedModel> {
    validate_key(key)?;
    if model.len() > 256
        || !model.contains('/')
        || !model
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-._/".contains(&c))
        || model
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err("bounded exact OpenRouter model ID required".into());
    }
    let observed = now();
    let client = production::client()?;
    let mut url = reqwest::Url::parse(&format!("{api}/models/"))?;
    url.path_segments_mut()
        .map_err(|_| "provider catalog URL")?
        .pop_if_empty()
        .extend(model.split('/'))
        .push("endpoints");
    let catalog = production::get(&client, url, Some(key)).await?;
    let mut snapshots = compatibility::snapshots(&catalog, observed)?;
    if snapshots
        .iter()
        .any(|snapshot| snapshot.compatibility.model != model)
    {
        return Err("provider catalog returned a different model".into());
    }
    // Conservative reservation is a deterministic cost ordering, not a quality
    // score. Keep exact endpoint identities and reject unknown price classes.
    let mut priced = Vec::new();
    for snapshot in snapshots.drain(..) {
        if snapshot.max_output < Units::new(u64::from(OUTPUT_TOKENS)) {
            continue;
        }
        let price = reservation_snapshot(&snapshot)?;
        priced.push((price, snapshot.compatibility.endpoint.clone(), snapshot));
    }
    priced.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    let snapshot = priced
        .into_iter()
        .next()
        .ok_or("no endpoint supports the bounded connection response")?
        .2;
    Ok(PreparedModel { snapshot, catalog })
}

fn validate_key(key: &str) -> Result<()> {
    if key.is_empty() || key.len() > 16384 || key.chars().any(char::is_control) {
        return Err("valid provider credential required".into());
    }
    Ok(())
}

fn reservation_snapshot(snapshot: &Snapshot) -> Result<Micros> {
    Ok(conformance::reservation(
        snapshot,
        Units::new(u64::from(OUTPUT_TOKENS)),
    )?)
}

/// The same full-input/cache/output/request bound used by canonical admission.
pub fn reservation(prepared: &PreparedModel) -> std::result::Result<Micros, String> {
    reservation_snapshot(&prepared.snapshot).map_err(|e| e.to_string())
}

/// A retained successful result resumes setup without another paid request.
/// None means no directory, a proven pre-dispatch canonical state, or a
/// completed failed test with fully settled cost.
/// An incomplete result fails closed: keep its pending marker and reconcile its
/// canonical accounting before a user can authorize another connection test.
pub async fn retained_result(
    output: &Path,
) -> std::result::Result<Option<ConnectionReport>, String> {
    retained(output).await.map_err(|e| e.to_string())
}

async fn retained(output: &Path) -> Result<Option<ConnectionReport>> {
    reject_links(output)?;
    if !output.exists() {
        return Ok(None);
    }
    let root = crate::settings::registry_root(output)?;
    let _pin = root.hold(None, true)?;
    let raw = match root.read(Path::new("result.json"), 4 * 1024 * 1024) {
        Ok(raw) => raw,
        Err(vcp_repository::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return recover_canonical(output, &root).await;
        }
        Err(_) => {
            return Err(
                "connection result is unavailable or redirected; preserve its evidence".into(),
            )
        }
    };
    let value: serde_json::Value = match serde_json::from_slice(&raw.bytes) {
        Ok(value) => value,
        // A crash can leave this convenience summary partially written. Keep
        // those bytes unchanged and use only replayed canonical accounting to
        // decide whether the original request completed or never dispatched.
        Err(_) => return recover_canonical(output, &root).await,
    };
    if value["schema"] != "vcp-connection-test/1"
        || value["ledger"]["active"] != "0"
        || value["ledger"]["unresolved"] != "0"
        || value["ledger"]["overrun"] != false
    {
        return Err("connection test has unresolved liability; reconcile canonical accounting before another test".into());
    }
    match value["status"].as_str() {
        Some("observed") => Ok(Some(serde_json::from_value(value["connection"].clone())?)),
        Some("failed") => Ok(None),
        _ => Err("connection evidence status unavailable".into()),
    }
}

/// Recover the publication gap from replayed accounting, never the optional
/// JSON summary. Opening only an existing initialized store avoids mistaking
/// missing evidence for a request that was never sent.
async fn recover_canonical(
    output: &Path,
    root: &vcp_repository::Root,
) -> Result<Option<ConnectionReport>> {
    let authorization: serde_json::Value =
        serde_json::from_slice(&root.read(Path::new("authorization.json"), 64 * 1024)?.bytes)?;
    if authorization["schema"] != "vcp-connection-authorization/1"
        || authorization["max_requests"] != 1
        || authorization["retries"] != 0
        || authorization["prompt"] != conformance::CONNECTION_PROMPT
    {
        return Err(
            "connection recovery requires the original single-request authorization".into(),
        );
    }
    let spec: Spec = serde_json::from_value(authorization["spec"].clone())?;
    let catalog = root
        .read(Path::new("endpoints.json"), 4 * 1024 * 1024)?
        .bytes;
    if digest_bytes(&catalog) != spec.catalog_sha256 || spec.max_output_tokens != OUTPUT_TOKENS {
        return Err("connection recovery authorization changed".into());
    }
    let canonical = output.join("canonical");
    reject_links(&canonical)?;
    let canonical_root = crate::settings::registry_root(&canonical)?;
    let _pin = canonical_root.hold(None, true)?;
    // Store::open can initialize an empty root. Both durable files must already
    // exist before recovery opens it; replay verifies the journal and artifacts.
    canonical_root.read(Path::new("format.json"), 1024)?;
    // The store owns the journal's write handle during replay; retaining the
    // repository's read-only file guard would forbid that open on Windows.
    drop(canonical_root.hold(Some(Path::new("canonical.frames")), false)?);
    let store = vcp_store::Store::open(&canonical, BackendKind::Files, &[]).await?;
    let recovered = recover_state(&store, &spec, output);
    store.close().await?;
    recovered
}

fn recover_state(
    store: &vcp_store::Store,
    spec: &Spec,
    output: &Path,
) -> Result<Option<ConnectionReport>> {
    use vcp_domain::artifact::{ArtifactDescriptor, CaptureState};
    let records = &store.state().records;
    let ledgers: Vec<Ledger> = records
        .values()
        .filter(|row| row.collection == Collection::Ledger)
        .map(|row| row.decode())
        .collect::<std::result::Result<_, _>>()?;
    let tasks: Vec<vcp_domain::task::Task> = records
        .values()
        .filter(|row| row.collection == Collection::Task)
        .map(|row| row.decode())
        .collect::<std::result::Result<_, _>>()?;
    let (ledger, task) = match (ledgers.as_slice(), tasks.as_slice()) {
        ([ledger], [task]) => (ledger, task),
        _ => return Err("connection recovery requires one canonical root task and ledger".into()),
    };
    if ledger.scope != task.scope
        || task.parent.is_some()
        || task.root != task.scope.task
        || ledger.cap != crate::args::parse_usd(&spec.cap_usd)?
        || ledger.currency.code() != "USD"
        || ledger.active != Micros::ZERO
        || ledger.unresolved != Micros::ZERO
        || ledger.overrun
    {
        return Err(
            "connection recovery has unresolved or mismatched canonical accounting; no retry"
                .into(),
        );
    }
    let attempts: Vec<Attempt> = records
        .values()
        .filter(|row| row.collection == Collection::Attempt)
        .map(|row| row.decode())
        .collect::<std::result::Result<_, _>>()?;
    if attempts.is_empty()
        && ledger.settled == Micros::ZERO
        && !records.values().any(|row| {
            matches!(
                row.collection,
                Collection::Reservation | Collection::Settlement
            )
        })
    {
        // A replayed initialized ledger with no attempt proves admission never
        // occurred. Missing directories or summary files alone prove nothing.
        return Ok(None);
    }
    let attempt = match attempts.as_slice() {
        [attempt] => attempt,
        _ => return Err("connection recovery requires exactly one canonical attempt".into()),
    };
    if attempt.scope != ledger.scope
        || attempt.root != task.root
        || attempt.phase != ReservationState::Settled
        || attempt.charged != ledger.settled
        || attempt.quote.price.model != spec.model
        || attempt.quote.price.provider != spec.endpoint
        || attempt.role != RequestRole::Main
        || attempt.previous.is_some()
    {
        return Err("connection attempt is not the original settled request; no retry".into());
    }
    let artifacts: Vec<ArtifactDescriptor> = records
        .values()
        .filter(|row| row.collection == Collection::Artifact)
        .map(|row| row.decode())
        .collect::<std::result::Result<_, _>>()?;
    let read = |artifact: &ArtifactDescriptor| -> Result<Vec<u8>> {
        if artifact.spec.scope != ledger.scope
            || artifact.state != CaptureState::Complete
            || artifact.length > ByteCount::new(4 * 1024 * 1024)
        {
            return Err("connection recovery requires complete bounded canonical evidence".into());
        }
        let mut bytes = Vec::new();
        store.spool().read(artifact, &mut bytes)?;
        Ok(bytes)
    };
    let request = artifacts
        .iter()
        .find(|artifact| artifact.spec.id == attempt.request)
        .ok_or("connection request evidence missing")?;
    let body: serde_json::Value = serde_json::from_slice(&read(request)?)?;
    if body["model"] != spec.model
        || body["max_output_tokens"] != OUTPUT_TOKENS
        || body["input"]
            != json!([{"type":"message","role":"user","content":[{"type":"input_text","text":conformance::CONNECTION_PROMPT}]}])
        || body.get("tools").is_some()
        || body["provider"]["only"] != json!([spec.endpoint])
        || body["provider"]["allow_fallbacks"] != false
    {
        return Err("canonical request differs from the authorized connection test".into());
    }
    let responses: Vec<_> = artifacts
        .iter()
        .filter(|artifact| artifact.spec.schema == "conformance-normalized-response/1")
        .collect();
    let response: ResultBody = match responses.as_slice() {
        [artifact] => serde_json::from_slice(&read(artifact)?)?,
        _ => return Err("connection recovery requires one retained normalized response".into()),
    };
    if attempt.provider_request.as_deref() != Some(response.response_id.as_str())
        || response
            .usage
            .as_ref()
            .and_then(|usage| usage.cost.as_ref())
            .is_none_or(|cost| cost.currency != ledger.currency || cost.micros != ledger.settled)
    {
        return Err("connection response does not match the settled charge".into());
    }
    Ok(connection_report(spec, output, ledger, response))
}

fn connection_report(
    spec: &Spec,
    output: &Path,
    ledger: &Ledger,
    response: ResultBody,
) -> Option<ConnectionReport> {
    if response.status != Status::Completed
        || response.served_model.as_deref() != Some(spec.model.as_str())
        || !response.calls.is_empty()
        || ledger.active != Micros::ZERO
        || ledger.unresolved != Micros::ZERO
        || ledger.overrun
    {
        return None;
    }
    let text = response
        .completed_messages
        .values()
        .cloned()
        .collect::<Vec<_>>()
        .join("");
    (!text.trim().is_empty() && text.len() <= 8192).then_some(ConnectionReport {
        model: spec.model.clone(), endpoint: spec.endpoint.clone(), response: text,
        reported_cost_micros: ledger.settled, evidence: output.to_path_buf(),
        verified: vec!["This credential completed one text request to the displayed model.".into(),
            "The response decoded successfully and its reported charge settled in canonical accounting.".into(),
            "This test did not verify tool use, model quality, other set members or served-provider attribution.".into()],
    })
}

/// The caller persists `output` as pending before entering this function. It
/// must call retained_result on that path before authorizing a later attempt.
pub async fn test(
    prepared: &PreparedModel,
    budget_usd: &str,
    output: &Path,
    workspace: Option<&Path>,
    key: &str,
) -> std::result::Result<ConnectionReport, String> {
    test_with(prepared, budget_usd, output, workspace, key, API)
        .await
        .map_err(|e| e.to_string().replace(key, "[redacted]"))
}

async fn test_with(
    prepared: &PreparedModel,
    budget_usd: &str,
    output: &Path,
    workspace: Option<&Path>,
    key: &str,
    api: &str,
) -> Result<ConnectionReport> {
    validate_key(key)?;
    let cap = crate::args::parse_usd(budget_usd)?;
    if cap.get() > 25_000_000 {
        return Err("connection test cap must be at most 25 USD".into());
    }
    if reservation_snapshot(&prepared.snapshot)? > cap {
        return Err(
            "connection test cap is below its conservative reservation; no request sent".into(),
        );
    }
    let snapshot = &prepared.snapshot;
    if Snapshot::from_endpoints(
        &prepared.catalog,
        snapshot.observed_at,
        snapshot.valid_until,
        snapshot.compatibility.clone(),
    )? != *snapshot
    {
        return Err("connection snapshot differs from its fresh catalog".into());
    }
    // Account-only setup has no project boundary. The repository/sync/drive
    // checks remain active; the sentinel is not a user-selected workspace.
    let sentinel = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".account-only-boundary");
    let boundary = workspace.unwrap_or(&sentinel);
    reject_links(output)?;
    let output = crate::settings::local_path(output, boundary)?;
    let parent = output
        .parent()
        .ok_or("connection evidence parent required")?;
    let parent_root = crate::settings::registry_root(parent)?;
    let _parent_pin = parent_root.hold(None, true)?;
    std::fs::create_dir(&output).map_err(|_| {
        "connection evidence must be a new directory in private local account storage"
    })?;
    let root = crate::settings::registry_root(&output)?;
    let _pin = root.hold(None, true)?;
    production::fresh_bytes(&output.join("endpoints.json"), &prepared.catalog)?;
    fresh_file(&output.join("snapshot.json"), snapshot)?;
    let spec = Spec {
        catalog: output.join("endpoints.json"),
        catalog_sha256: digest_bytes(&prepared.catalog),
        model: snapshot.compatibility.model.clone(),
        endpoint: snapshot.compatibility.endpoint.clone(),
        request_price_limit: snapshot.compatibility.request_price_limit.clone(),
        cap_usd: budget_usd.into(),
        max_output_tokens: OUTPUT_TOKENS,
        observed_at: snapshot.observed_at,
        valid_until: snapshot.valid_until,
    };
    fresh_file(
        &output.join("authorization.json"),
        &json!({
            "schema":"vcp-connection-authorization/1", "spec":spec, "max_requests":1,"retries":0,
            "prompt":conformance::CONNECTION_PROMPT,"authorized_at":now(),
            "scope":"one account connection test; no per-model or model-set qualification"
        }),
    )?;
    let candidate = CandidateMetadata::from_endpoints(
        &prepared.catalog,
        spec.observed_at,
        spec.valid_until,
        spec.model.clone(),
        spec.endpoint.clone(),
        spec.request_price_limit.clone(),
        compatibility::required_parameters(),
    )?;
    let (host, owner, binding) = setup(&output, &spec, &candidate,
        "Run one fixed public greeting to test account access, response decoding and reported cost. No tools or model-set qualification.")?;
    let client = production::client()?;
    let result = request(
        &host,
        &binding,
        &candidate,
        Probe::Connection,
        &client,
        &format!("{api}/responses"),
        key,
    )
    .await;
    let state = host.snapshot()?;
    let ledger: Ledger = state
        .record(
            Collection::Ledger,
            binding.scope.task.as_str(),
            &binding.scope.workspace,
        )?
        .decode()?;
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
    let connection = result
        .ok()
        .and_then(|response| connection_report(&spec, &output, &ledger, response));
    fresh_file(
        &output.join("result.json"),
        &json!({"schema":"vcp-connection-test/1",
        "status":if connection.is_some(){"observed"}else{"failed"},"connection":connection,
        "ledger":ledger,"max_requests":1,"retries":0,
        "compatibility_evidence":snapshot.compatibility.id}),
    )?;
    owner.close().await?;
    connection.ok_or_else(|| "connection test failed; preserve its evidence and inspect canonical accounting before retrying".into())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn catalog() -> serde_json::Value {
        json!({"data":{"id":"fixture/model","endpoints":[
            {"tag":"fixture/region","status":0,"context_length":32000,"max_prompt_tokens":24000,"max_completion_tokens":8000,"supported_parameters":["tools","tool_choice","max_tokens"],"pricing":{"prompt":"0","completion":"0","request":"0.001"}}
        ]}})
    }

    async fn metadata(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/models/fixture/model/endpoints"))
            .respond_with(ResponseTemplate::new(200).set_body_json(catalog()))
            .expect(1)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn interrupted_publication_recovers_canonical_success_without_another_request() {
        for outcome in ["success", "missing_cost", "blank", "before_dispatch"] {
            let server = MockServer::start().await;
            metadata(&server).await;
            Mock::given(method("POST"))
                .and(path("/responses"))
                .respond_with(move |_: &wiremock::Request| {
                    let mut response = json!({"id":"retained-response","status":"completed","model":"fixture/model",
                        "output":[{"type":"message","id":"message","role":"assistant","content":[{"type":"output_text","text":"Retained greeting."}]}],
                        "usage":{"input_tokens":10,"output_tokens":5,"total_tokens":15,"cost":0.000007}});
                    if outcome == "missing_cost" {
                        response["usage"].as_object_mut().unwrap().remove("cost");
                    } else if outcome == "blank" {
                        response["output"][0]["content"][0]["text"] = json!(" ");
                    }
                    ResponseTemplate::new(200).set_body_string(format!(
                        "data: {}\n\n",
                        json!({"type":"response.completed","response":response})
                    ))
                })
                .expect(if outcome == "before_dispatch" { 0 } else { 1 })
                .mount(&server)
                .await;
            let temporary = tempfile::tempdir().unwrap();
            let output = temporary.path().join("connection");
            std::fs::create_dir(&output).unwrap();
            let prepared = refresh_with("fixture/model", "synthetic-connection-key", &server.uri())
                .await
                .unwrap();
            let snapshot = &prepared.snapshot;
            let spec = Spec {
                catalog: output.join("endpoints.json"),
                catalog_sha256: digest_bytes(&prepared.catalog),
                model: snapshot.compatibility.model.clone(),
                endpoint: snapshot.compatibility.endpoint.clone(),
                request_price_limit: snapshot.compatibility.request_price_limit.clone(),
                cap_usd: "0.01".into(),
                max_output_tokens: OUTPUT_TOKENS,
                observed_at: snapshot.observed_at,
                valid_until: snapshot.valid_until,
            };
            production::fresh_bytes(&spec.catalog, &prepared.catalog).unwrap();
            fresh_file(
                &output.join("authorization.json"),
                &json!({
                    "schema":"vcp-connection-authorization/1", "spec":spec,
                    "max_requests":1,"retries":0,"prompt":conformance::CONNECTION_PROMPT
                }),
            )
            .unwrap();
            let candidate = CandidateMetadata::from_endpoints(
                &prepared.catalog,
                spec.observed_at,
                spec.valid_until,
                spec.model.clone(),
                spec.endpoint.clone(),
                spec.request_price_limit.clone(),
                compatibility::required_parameters(),
            )
            .unwrap();
            let (host, owner, binding) =
                setup(&output, &spec, &candidate, "Test publication interruption").unwrap();
            if outcome != "before_dispatch" {
                let result = request(
                    &host,
                    &binding,
                    &candidate,
                    Probe::Connection,
                    &production::client().unwrap(),
                    &format!("{}/responses", server.uri()),
                    "synthetic-connection-key",
                )
                .await;
                assert_eq!(result.is_err(), outcome == "missing_cost");
            }
            // End at the durable boundary before either convenience summary is
            // published. Only canonical records and retained artifacts survive.
            owner.close().await.unwrap();
            // Closing orchestration does not drop the host's canonical worker.
            // Recovery must refuse its still-owned journal; a restarted process
            // has released every host handle before replaying that journal.
            assert!(retained_result(&output).await.is_err());
            drop(host);
            assert!(!output.join("result.json").exists());
            assert!(!output.join("canonical-records.json").exists());
            for partial in [
                None,
                Some(b"".as_slice()),
                Some(br#"{"schema":"vcp-connection-test/1","ledger":{"settled":"#.as_slice()),
            ] {
                if let Some(bytes) = partial {
                    std::fs::write(output.join("result.json"), bytes).unwrap();
                }
                for _ in 0..2 {
                    let recovered = retained_result(&output).await;
                    match outcome {
                        "success" => {
                            let report = recovered.unwrap().unwrap();
                            assert_eq!(report.response, "Retained greeting.");
                            assert_eq!(report.reported_cost_micros, Micros::new(7));
                            assert_eq!(report.model, "fixture/model");
                            assert_eq!(report.endpoint, "fixture/region");
                        }
                        "missing_cost" => assert!(recovered.is_err()),
                        _ => assert!(recovered.unwrap().is_none()),
                    }
                    if let Some(bytes) = partial {
                        assert_eq!(std::fs::read(output.join("result.json")).unwrap(), bytes);
                    }
                }
            }
            assert_eq!(
                server.received_requests().await.unwrap().len(),
                if outcome == "before_dispatch" { 1 } else { 2 }
            );
            if outcome == "success" {
                let authorization = output.join("authorization.json");
                let original = std::fs::read(&authorization).unwrap();
                let mut changed: serde_json::Value = serde_json::from_slice(&original).unwrap();
                changed["spec"]["model"] = json!("other/model");
                std::fs::write(&authorization, canonical_bytes(&changed).unwrap()).unwrap();
                assert!(retained_result(&output).await.is_err());
                std::fs::write(&authorization, original).unwrap();
                assert!(retained_result(&output).await.unwrap().is_some());
            }
            // Absence of an initialized store cannot become evidence of a
            // pre-dispatch attempt, and recovery must not create a new store.
            let absent = temporary.path().join("uninitialized");
            std::fs::create_dir(&absent).unwrap();
            std::fs::copy(
                output.join("authorization.json"),
                absent.join("authorization.json"),
            )
            .unwrap();
            std::fs::copy(&spec.catalog, absent.join("endpoints.json")).unwrap();
            assert!(retained_result(&absent).await.is_err());
            assert!(!absent.join("canonical").exists());
            std::fs::write(absent.join("result.json"), b"{").unwrap();
            assert!(retained_result(&absent).await.is_err());
            assert!(!absent.join("canonical").exists());
        }
    }

    #[tokio::test]
    async fn single_connection_preserves_cost_scope_and_unknown_liability() {
        for outcome in [
            "success",
            "missing_cost",
            "reflected",
            "escaped_reflection",
            "cr_reflection",
            "multiline_reflection",
            "large_split_reflection",
            "wrong_model",
            "blank",
            "overrun",
        ] {
            let server = MockServer::start().await;
            metadata(&server).await;
            Mock::given(method("POST")).and(path("/responses"))
                .respond_with(move |request: &wiremock::Request| {
                    assert_eq!(request.headers["authorization"], "Bearer synthetic-connection-key");
                    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                    assert_eq!(body["model"], "fixture/model");
                    assert_eq!(body["max_output_tokens"], OUTPUT_TOKENS);
                    assert_eq!(body["input"][0]["content"][0]["text"], conformance::CONNECTION_PROMPT);
                    assert_eq!(body["provider"]["only"], json!(["fixture/region"]));
                    assert_eq!(body["provider"]["allow_fallbacks"], false);
                    assert_eq!(body["provider"]["require_parameters"], true);
                    assert_eq!(body["provider"]["data_collection"], "deny");
                    assert_eq!(body["store"], false);
                    assert!(body.get("tools").is_none());
                    let mut response = json!({"id":"connection-response","status":"completed","model":"fixture/model",
                        "output":[{"type":"message","id":"message","role":"assistant","content":[{"type":"output_text","text":"Hello from the model."}]}],
                        "usage":{"input_tokens":10,"output_tokens":5,"total_tokens":15,"cost":0.000007}});
                    match outcome {
                        "missing_cost" => {response["usage"].as_object_mut().unwrap().remove("cost");},
                        "reflected" | "escaped_reflection" => {response["output"][0]["content"][0]["text"] = json!("synthetic-connection-key");},
                        "cr_reflection" | "multiline_reflection" => {response["ignored"] = json!({"nested":["synthetic-connection-key"]});},
                        "large_split_reflection" => {
                            response["ignored_padding"] = json!("x".repeat(70_000));
                            response["output"][0]["content"] = json!([
                                {"type":"output_text","text":"synthetic-"},
                                {"type":"output_text","text":"connection-key"}
                            ]);
                        },
                        "wrong_model" => {response["model"] = json!("other/model");},
                        "blank" => {response["output"][0]["content"][0]["text"] = json!(" ");},
                        "overrun" => {response["usage"]["cost"] = json!(0.02);},
                        _ => {},
                    }
                    let mut body = if outcome == "multiline_reflection" {
                        format!("data: {{\"type\":\"response.completed\",\ndata: \"response\":{response}}}\n\n")
                    } else {
                        format!("data: {}\n\n", json!({"type":"response.completed","response":response}))
                    };
                    if outcome == "cr_reflection" {
                        body = format!("\u{feff}{}", body.replace('\n', "\r"));
                    }
                    if matches!(outcome, "escaped_reflection" | "cr_reflection" | "multiline_reflection") {
                        body = body.replace("synthetic-connection-key", "synthetic-\\u0063onnection-key");
                    }
                    ResponseTemplate::new(200).set_body_string(body)
                }).expect(1).mount(&server).await;
            let temporary = tempfile::tempdir().unwrap();
            let output = temporary.path().join("connection");
            let prepared = refresh_with("fixture/model", "synthetic-connection-key", &server.uri())
                .await
                .unwrap();
            assert_eq!(reservation(&prepared).unwrap(), Micros::new(1000));
            let result = test_with(
                &prepared,
                "0.01",
                &output,
                None,
                "synthetic-connection-key",
                &server.uri(),
            )
            .await;
            let value: serde_json::Value =
                serde_json::from_slice(&std::fs::read(output.join("result.json")).unwrap())
                    .unwrap();
            assert_eq!(value["max_requests"], 1);
            assert_eq!(value["retries"], 0);
            if outcome == "success" {
                let report = result.unwrap();
                assert_eq!(report.response, "Hello from the model.");
                assert_eq!(report.reported_cost_micros, Micros::new(7));
                assert!(report
                    .verified
                    .iter()
                    .any(|text| text.contains("did not verify tool use")));
                assert_eq!(
                    retained_result(&output).await.unwrap().unwrap().response,
                    report.response
                );
            } else {
                assert!(result.is_err(), "{outcome}");
                if matches!(outcome, "wrong_model" | "blank") {
                    assert!(retained_result(&output).await.unwrap().is_none());
                    assert_eq!(value["ledger"]["settled"], "7");
                } else {
                    assert!(retained_result(&output).await.is_err());
                }
            }
            let records: Vec<serde_json::Value> = serde_json::from_slice(
                &std::fs::read(output.join("canonical-records.json")).unwrap(),
            )
            .unwrap();
            let attempts: Vec<_> = records
                .iter()
                .filter(|row| row["collection"] == "attempt")
                .collect();
            assert_eq!(attempts.len(), 1);
            assert_eq!(attempts[0]["value"]["quote"]["amount"]["micros"], "1000");
            assert_eq!(attempts[0]["value"]["quote"]["bounds"]["input"], "72000");
            assert_eq!(attempts[0]["value"]["quote"]["bounds"]["output"], "128");
            if outcome.contains("reflect") {
                assert_eq!(value["ledger"]["unresolved"], "1000");
                assert_eq!(value["ledger"]["settled"], "0");
                for row in records.iter().filter(|row| {
                    row["collection"] == "artifact" && row["value"]["spec"]["channel"] == "response"
                }) {
                    assert_eq!(
                        row["value"]["length"], "0",
                        "reflected response was captured"
                    );
                }
            }
            // Same pending directory never replays inference, including success.
            assert!(test_with(
                &prepared,
                "0.01",
                &output,
                None,
                "synthetic-connection-key",
                &server.uri()
            )
            .await
            .is_err());
            fn no_key(path: &Path) {
                for item in std::fs::read_dir(path).unwrap() {
                    let path = item.unwrap().path();
                    if path.is_dir() {
                        no_key(&path);
                    } else {
                        let bytes = std::fs::read(path).unwrap();
                        assert!(!bytes
                            .windows(b"synthetic-\\u0063onnection-key".len())
                            .any(|part| part == b"synthetic-\\u0063onnection-key"));
                        assert!(reject_credential(&bytes, "synthetic-connection-key").is_ok());
                    }
                }
            }
            no_key(&output);
            assert_eq!(server.received_requests().await.unwrap().len(), 2);
        }
    }

    #[tokio::test]
    async fn connection_rejects_small_budget_stale_metadata_and_changed_catalog_before_dispatch() {
        let server = MockServer::start().await;
        metadata(&server).await;
        let mut prepared = refresh_with("fixture/model", "synthetic-connection-key", &server.uri())
            .await
            .unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let output = temporary.path().join("connection");
        for budget in ["0", "0.000999", "25.000001"] {
            assert!(test_with(
                &prepared,
                budget,
                &output,
                None,
                "synthetic-connection-key",
                &server.uri()
            )
            .await
            .is_err());
            assert!(!output.exists());
        }
        prepared.catalog = b"{}".to_vec();
        assert!(test_with(
            &prepared,
            "0.01",
            &output,
            None,
            "synthetic-connection-key",
            &server.uri()
        )
        .await
        .is_err());
        assert!(!output.exists());
        prepared.snapshot.valid_until = Timestamp::new(now().get().saturating_sub(1));
        assert!(test_with(
            &prepared,
            "0.01",
            &output,
            None,
            "synthetic-connection-key",
            &server.uri()
        )
        .await
        .is_err());
        assert!(!output.exists());
        assert!(retained_result(&output).await.unwrap().is_none());
        std::fs::create_dir(&output).unwrap();
        assert!(retained_result(&output).await.is_err());
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }
}
