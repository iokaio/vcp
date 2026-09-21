// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_lifecycle::foundation::conformance::{cohort, native};
use vcp_models::decision::Operation;

#[cfg(test)]
pub(super) async fn execute(
    spec: &Spec,
    output: &Path,
    key: &str,
    endpoint: &str,
) -> Result<serde_json::Value> {
    execute_kind(spec, output, key, endpoint, None).await
}
async fn execute_kind(
    spec: &Spec,
    output: &Path,
    key: &str,
    endpoint: &str,
    operation: Option<Operation>,
) -> Result<serde_json::Value> {
    reject_links(&spec.catalog)?;
    let raw = vcp_cli::settings::read_bounded(&spec.catalog, 64 * 1024)?;
    let catalog_value: serde_json::Value = serde_json::from_slice(&raw)?;
    let served_provider = catalog_value["data"]["endpoints"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|r| r["tag"].as_str() == Some(spec.endpoint.as_str()))
        })
        .and_then(|r| r["provider_name"].as_str())
        .map(str::to_owned);
    if digest_bytes(&raw) != spec.catalog_sha256
        || spec.observed_at > now()
        || spec.valid_until <= now()
        || spec
            .valid_until
            .get()
            .saturating_sub(spec.observed_at.get())
            > 86_400_000
        || spec.max_output_tokens
            != if operation == Some(Operation::ConventionalChat) {
                512
            } else {
                1
            }
        || vcp_cli::args::parse_usd(&spec.cap_usd)?.get() == 0
        || vcp_cli::args::parse_usd(&spec.cap_usd)?.get() > 25_000_000
    {
        return Err("native spec catalog/time/sentinel bounds".into());
    }
    let cohort_candidate = operation.map(|operation| cohort::Candidate {
        raw: raw.clone(),
        model: spec.model.clone(),
        provider: spec.endpoint.clone(),
        request_cap: spec.request_price_limit.clone(),
        observed: spec.observed_at,
        expires: spec.valid_until,
        operation,
    });
    let candidate = if let Some(c) = &cohort_candidate {
        c.metadata()?
    } else {
        native::candidate(
            &raw,
            &spec.model,
            &spec.endpoint,
            &spec.request_price_limit,
            spec.valid_until,
        )?
    };
    let (host, owner, binding) = setup(
        output,
        spec,
        &candidate,
        if operation.is_some() {
            "Observe the frozen M4 repeated-strategy cohort and actual charge; no retries, tools, changed coding strategy or installed evaluator qualification."
        } else {
            "Observe one fixed public native Decisions mixed batch and actual charge; no retries, tools or installed evaluator qualification."
        },
    )?;
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
    let start = std::time::Instant::now();
    let result = async {
        let mut lease = if let Some(c) = cohort_candidate {
            host.admit_decision_cohort(binding.clone(), c)?
        } else {
            host.admit_native_conformance(
                binding.clone(),
                raw,
                spec.model.clone(),
                spec.endpoint.clone(),
                spec.request_price_limit.clone(),
                spec.valid_until,
            )?
        };
        let mut response = client
            .post(endpoint)
            .bearer_auth(key)
            .header("content-type", "application/json")
            .body(canonical_bytes(lease.body())?)
            .send()
            .await?;
        let status = response.status();
        while let Some(chunk) = response.chunk().await? {
            lease.capture(&chunk)?;
        }
        // Preserve any trustworthy observed charge even for HTTP failures.
        let raw = lease.finish()?;
        if !status.is_success() {
            return Err::<serde_json::Value, Box<dyn std::error::Error>>(
                format!("native HTTP {status}; response captured").into(),
            );
        }
        Ok(raw)
    }
    .await;
    let elapsed = start.elapsed().as_millis();
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
    let mut report = match result {
        Ok(response) => {
            json!({"schema":"p6-native-decision-conformance/1","status":if accounted{"observed"}else{"failed"},"response":response,"candidate":candidate,"scope":binding.scope,"elapsed_ms":elapsed,"actual_cost_micros":if accounted{Some(ledger.settled)}else{None},"ledger":ledger,"qualified":false,"limitations":["One fixed native mixed batch; observed is not per-purpose or provider-control qualification.","Actual raw probabilities, optional fields and identity require review before installation."]})
        }
        Err(error) => {
            json!({"schema":"p6-native-decision-conformance/1","status":"failed","error":error.to_string(),"candidate":candidate,"scope":binding.scope,"elapsed_ms":elapsed,"actual_cost_micros":if accounted{Some(ledger.settled)}else{None},"ledger":ledger,"qualified":false})
        }
    };
    if let Some(operation) = operation {
        report["schema"] = json!("p6-decision-cohort/1");
        report["operation"] = json!(operation);
        report["workload_sha256"] = json!(cohort::workload_hash());
        report["limitations"]=json!(["One fixed synthetic M4 cohort batch; no task-level outcome or installed evaluator qualification.","Native probabilities and conventional Boolean/null answers remain distinct; all invalid batches abstain."]);
        if report["status"] == "observed" {
            let identity_ok = if operation == Operation::JevDecisions {
                report["response"]["model"] == "typesafe/jev-1.13-20260917"
                    && report["response"]["provider"] == "TypeSafe"
            } else {
                report["response"]["model"].as_str() == Some(spec.model.as_str())
                    && served_provider
                        .as_deref()
                        .is_some_and(|p| report["response"]["provider"].as_str() == Some(p))
            };
            let decoded = if identity_ok {
                cohort::answers(&report["response"], operation)
            } else {
                Err("cohort exact served model/provider unavailable or mismatched".into())
            };
            match decoded {
                Ok(answers) => report["probe_answers"] = answers,
                Err(error) => {
                    report["answer_error"] = json!(error);
                    report["probe_answers"] = serde_json::Value::Null;
                }
            }
        }
    }
    fresh_file(&output.join("result.json"), &report)?;
    owner.close().await?;
    Ok(report)
}

pub(super) async fn run(args: &[std::ffi::OsString]) -> Result<()> {
    run_kind(args, None).await
}
pub(super) async fn run_cohort(args: &[std::ffi::OsString], operation: Operation) -> Result<()> {
    run_kind(args, Some(operation)).await
}
async fn run_kind(args: &[std::ffi::OsString], operation: Option<Operation>) -> Result<()> {
    if args.len() != 3 {
        return Err(
            "Usage: --native <spec.json> <new-private-directory> <authorized-spec-sha256>".into(),
        );
    }
    let spec_path = Path::new(&args[0]);
    reject_links(spec_path)?;
    let bytes = vcp_cli::settings::read_bounded(spec_path, 1024 * 1024)?;
    if args[2].to_str() != Some(digest_bytes(&bytes).as_str()) {
        return Err("native authorization spec hash mismatch".into());
    }
    let spec: Spec = serde_json::from_slice(&bytes)?;
    let output = std::path::absolute(PathBuf::from(&args[1]))?;
    reject_links(&output)?;
    if output
        .components()
        .any(|p| matches!(p, std::path::Component::ParentDir))
    {
        return Err("native output traversal".into());
    }
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .ok_or("repository root")?
        .canonicalize()?;
    if output.starts_with(&repository) || repository.starts_with(&output) {
        return Err("native output must be outside repository".into());
    }
    let output = vcp_cli::settings::local_path(&output, &repository)?;
    std::fs::create_dir(&output)?;
    let binary = std::env::current_exe()?;
    fresh_file(
        &output.join("claim.json"),
        &json!({"spec_sha256":digest_bytes(&bytes),"binary_sha256":digest_bytes(&vcp_cli::settings::read_bounded(&binary,1024*1024*1024)?),"source_sha256":{"binary":digest_bytes(include_bytes!("native.rs")),"lease":digest_bytes(include_bytes!("../../../../vcp-lifecycle/src/foundation/conformance/native.rs")),"settlement":digest_bytes(include_bytes!("../../../../vcp-lifecycle/src/foundation/worker/conformance.rs")),"admission":digest_bytes(include_bytes!("../../../../vcp-lifecycle/src/foundation/worker.rs")),"native_bound":digest_bytes(include_bytes!("../../../../vcp-models/src/decision/native_bound.rs"))},"spec":spec,"claimed_at":now(),"scope":"exactly one closed synthetic decision request; shared budget owned by coordinator"}),
    )?;
    let key = std::env::var("OPENROUTER_API_KEY").map_err(|_| "OPENROUTER_API_KEY is required")?;
    fresh_file(
        &output.join("operation.json"),
        &json!({"operation":operation,"cohort_source_sha256":digest_bytes(include_bytes!("../../../../vcp-lifecycle/src/foundation/conformance/cohort.rs")),"workload_sha256":cohort::workload_hash(),"mode":if operation.is_some(){"frozen_cohort"}else{"mixed_batch_bootstrap"}}),
    )?;
    let endpoint = if operation == Some(Operation::ConventionalChat) {
        "https://openrouter.ai/api/v1/chat/completions"
    } else {
        "https://openrouter.ai/api/alpha/decisions"
    };
    let report = execute_kind(&spec, &output, &key, endpoint, operation).await?;
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
    async fn matched_cohorts_keep_labels_out_of_requests_and_settle_distinct_answers() {
        for operation in [Operation::JevDecisions, Operation::ConventionalChat] {
            let temp = tempfile::tempdir().unwrap();
            let is_native = operation == Operation::JevDecisions;
            let model = if is_native {
                "typesafe/jev-1.13"
            } else {
                "fixture/comparator"
            };
            let provider = if is_native { "typesafe" } else { "fixture" };
            let raw=canonical_bytes(&json!({"data":{"id":model,"architecture":{"modality":if is_native{"text->decisions"}else{"text->text"}},"endpoints":[{"tag":provider,"provider_name":if is_native{"TypeSafe"}else{"Fixture"},"status":0,"context_length":32000,"max_completion_tokens":1024,"supported_parameters":["tools","max_tokens","response_format","structured_outputs"],"pricing":{"prompt":"0.000000042","completion":if is_native{"0"}else{"0.000001"}}}]}})).unwrap();
            let catalog = temp.path().join("catalog.json");
            std::fs::write(&catalog, &raw).unwrap();
            let spec = Spec {
                catalog,
                catalog_sha256: digest_bytes(&raw),
                model: model.into(),
                endpoint: provider.into(),
                request_price_limit: "0.001".into(),
                cap_usd: "0.01".into(),
                max_output_tokens: if is_native { 1 } else { 512 },
                observed_at: now(),
                valid_until: Timestamp::new(now().get() + 60000),
            };
            let output = temp.path().join("output");
            std::fs::create_dir(&output).unwrap();
            let server = MockServer::start().await;
            Mock::given(method("POST")).and(path("/cohort")).respond_with(move|request:&wiremock::Request| {
                let body:serde_json::Value=serde_json::from_slice(&request.body).unwrap();
                let wire=String::from_utf8(request.body.clone()).unwrap();assert!(!wire.contains("stall_label"));assert!(!wire.contains("benign-repetition"));assert!(!wire.contains("known_cost"));assert!(body.get("tools").is_none());assert!(body.get("max_output_tokens").is_none());
                let answers:serde_json::Map<_,_>=(0..12).map(|i|(format!("c{i:02}"),if is_native{json!({"type":"noul","noul":0.8})}else{json!(true)})).collect();
                let response=if is_native {assert_eq!(body["questions"].as_object().unwrap().len(),12);json!({"id":"native-cohort","model":"typesafe/jev-1.13-20260917","provider":"TypeSafe","answers":answers,"usage":{"input_tokens":500,"output_tokens":48,"cost":0.000022}})}else{assert_eq!(body["max_tokens"],512);assert_eq!(body["response_format"]["json_schema"]["strict"],true);json!({"id":"comparator-cohort","model":"fixture/comparator","provider":"Fixture","choices":[{"finish_reason":"stop","message":{"role":"assistant","content":serde_json::to_string(&answers).unwrap()}}],"usage":{"prompt_tokens":600,"completion_tokens":80,"cost":0.00011}})};
                ResponseTemplate::new(200).set_body_json(response)
            }).expect(1).mount(&server).await;
            let report = execute_kind(
                &spec,
                &output,
                "fixture-key",
                &format!("{}/cohort", server.uri()),
                Some(operation),
            )
            .await
            .unwrap();
            assert_eq!(report["status"], "observed");
            assert_eq!(report["probe_answers"].as_object().unwrap().len(), 12);
            assert_eq!(report["qualified"], false);
            assert_eq!(
                report["actual_cost_micros"],
                if is_native { "22" } else { "110" }
            );
            if is_native {
                assert_eq!(report["probe_answers"]["c00"], 0.8);
            } else {
                assert_eq!(report["probe_answers"]["c00"], true);
            }
            server.verify().await;
        }
        let duplicate = json!({"choices":[{"finish_reason":"stop","message":{"content":"{\"c00\":true,\"c00\":false}"}}]});
        assert!(cohort::answers(&duplicate, Operation::ConventionalChat).is_err());
    }
    #[tokio::test]
    async fn native_probe_captures_real_cost_and_retains_unknown_without_replay() {
        for missing_cost in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let raw=canonical_bytes(&json!({"data":{"id":"typesafe/jev-1.13","architecture":{"modality":"text->decisions"},"endpoints":[{"tag":"typesafe","status":0,"context_length":32000,"pricing":{"prompt":"0.000000042","completion":"0"}}]}})).unwrap();
            let catalog = temp.path().join("catalog.json");
            std::fs::write(&catalog, &raw).unwrap();
            let spec = Spec {
                catalog,
                catalog_sha256: digest_bytes(&raw),
                model: "typesafe/jev-1.13".into(),
                endpoint: "typesafe".into(),
                request_price_limit: "0.001".into(),
                cap_usd: "0.01".into(),
                max_output_tokens: 1,
                observed_at: now(),
                valid_until: Timestamp::new(now().get() + 60000),
            };
            let output = temp.path().join("output");
            std::fs::create_dir(&output).unwrap();
            let server = MockServer::start().await;
            Mock::given(method("POST")).and(path("/decisions")).respond_with(move |r:&wiremock::Request| {
                let body:serde_json::Value=serde_json::from_slice(&r.body).unwrap();
                assert!(body.get("max_tokens").is_none());assert!(body.get("max_output_tokens").is_none());assert!(body.get("tools").is_none());assert_eq!(body["provider"]["max_price"]["completion"],"0");
                let mut raw=json!({"id":"native-1","model":"typesafe/jev-1.13-20260917","provider":"TypeSafe","answers":{"repeated":{"type":"noul","noul":0.99},"action":{"type":"choice","choice":"review"},"severity":{"type":"score","score":2}},"usage":{"input_tokens":300,"output_tokens":20,"cost":0.000013}});
                if missing_cost {raw["usage"].as_object_mut().unwrap().remove("cost");}
                ResponseTemplate::new(200).set_body_json(raw)
            }).expect(1).mount(&server).await;
            let report = execute(
                &spec,
                &output,
                "fixture-key",
                &format!("{}/decisions", server.uri()),
            )
            .await
            .unwrap();
            if missing_cost {
                assert_eq!(report["status"], "failed");
                assert!(
                    report["ledger"]["unresolved"]
                        .as_str()
                        .unwrap()
                        .parse::<u64>()
                        .unwrap()
                        > 0
                );
            } else {
                assert_eq!(report["status"], "observed");
                assert_eq!(report["actual_cost_micros"], "13");
                assert_eq!(report["qualified"], false);
            }
            assert!(execute(
                &spec,
                &output,
                "fixture-key",
                &format!("{}/decisions", server.uri())
            )
            .await
            .is_err());
            server.verify().await;
        }
    }
}
