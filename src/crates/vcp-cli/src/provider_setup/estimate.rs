// SPDX-License-Identifier: Apache-2.0
//! Price estimates before any provider call: one public catalog read per
//! distinct model, no credential, no files and no model call. Amounts use the
//! same bounds and quote arithmetic as admission.
use super::*;
use crate::model_sets::{self, Role};
use serde_json::Value;
use std::collections::BTreeMap;

/// Output tokens of each fixed setup probe.
pub const PROBE_OUTPUT: u64 = 512;
/// Highest cap a single `setup provider` invocation may authorize.
pub const SETUP_CEILING_MICROS: u64 = 25_000_000;

#[derive(Debug, clap::Args)]
pub struct Estimate {
    /// Built-in model set to price; see the list in `vcp setup estimate --help`.
    #[arg(long, conflicts_with_all = ["model", "endpoint"])]
    pub set: Option<String>,
    /// Exact OpenRouter model ID to price instead of a set.
    #[arg(long, required_unless_present = "set", requires = "endpoint")]
    pub model: Option<String>,
    /// Exact endpoint tag for --model.
    #[arg(long, required_unless_present = "set", requires = "model")]
    pub endpoint: Option<String>,
    /// Maximum USD per provider request; defaults to 0.001.
    #[arg(long)]
    pub request_price_limit: Option<String>,
    /// Output tokens per task request; defaults to the model set limit.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=262_144))]
    pub output_tokens: Option<u32>,
}

fn per_million(rate: &Rate) -> Value {
    if rate.per_units.get() == 0 {
        return Value::Null;
    }
    let product = u128::from(rate.micros.get()) * 1_000_000;
    let divisor = u128::from(rate.per_units.get());
    let micros = product / divisor + u128::from(product % divisor != 0);
    json!(micros.to_string())
}

/// Price one endpoint from exact catalog bytes. The probe and task amounts are
/// what admission would reserve for one request with these bounds.
pub fn price(
    raw: &[u8],
    model: &str,
    endpoint: &str,
    request_price_limit: &str,
    task_output: u64,
    now: Timestamp,
) -> std::result::Result<Value, String> {
    let candidate = CandidateMetadata::from_endpoints(
        raw,
        now,
        Timestamp::new(now.get().saturating_add(12 * 60 * 60 * 1000)),
        model.into(),
        endpoint.into(),
        request_price_limit.into(),
        BTreeSet::from(["tools".into(), "tool_choice".into(), "max_tokens".into()]),
    )
    .map_err(|error| format!("{error}; this endpoint cannot be set up as listed"))?;
    let reserve = |output: u64| {
        conformance::request_reservation(
            &candidate.price,
            candidate.max_input,
            Units::new(output.min(candidate.max_output.get())),
            now,
        )
    };
    let probe = reserve(PROBE_OUTPUT)?;
    let task = reserve(task_output)?;
    let rate = |category| {
        candidate
            .price
            .rates
            .get(&category)
            .map(per_million)
            .unwrap_or(Value::Null)
    };
    Ok(json!({
        "available": true,
        "context": candidate.context,
        "max_input": candidate.max_input,
        "max_output": candidate.max_output,
        "per_million_micros": {
            "input": rate(ChargeCategory::Input),
            "output": rate(ChargeCategory::Output),
            "cache_read": rate(ChargeCategory::CacheRead),
            "cache_write": rate(ChargeCategory::CacheWrite),
        },
        "request_price_limit": request_price_limit,
        "probe_reservation_micros": probe,
        "probe_pair_micros": Micros::new(probe.get().saturating_mul(2)),
        "within_setup_ceiling": probe.get() <= SETUP_CEILING_MICROS,
        "task_request_reservation_micros": task,
        "task_output_tokens": task_output.min(candidate.max_output.get()),
    }))
}

pub async fn run(request: &Estimate) -> std::result::Result<Value, String> {
    estimate_with(request, production::OPENROUTER_API, now()).await
}

async fn estimate_with(
    request: &Estimate,
    api: &str,
    now: Timestamp,
) -> std::result::Result<Value, String> {
    let limit = request
        .request_price_limit
        .clone()
        .unwrap_or_else(|| model_sets::REQUEST_PRICE_LIMIT.into());
    let output = u64::from(
        request
            .output_tokens
            .unwrap_or(model_sets::LIMITS.output_tokens),
    );
    let set = request.set.as_deref().map(model_sets::find).transpose()?;
    let rows: Vec<(Vec<Role>, String, String, String)> =
        match (set, &request.model, &request.endpoint) {
            (Some(set), _, _) => set
                .distinct()
                .into_iter()
                .map(|(roles, member)| {
                    (
                        roles,
                        member.model.clone(),
                        member.endpoint.clone(),
                        member.level.clone(),
                    )
                })
                .collect(),
            (None, Some(model), Some(endpoint)) => {
                vec![(
                    vec![Role::Main],
                    model.clone(),
                    endpoint.clone(),
                    "unrated".into(),
                )]
            }
            _ => return Err("pass --set, or --model and --endpoint".into()),
        };
    for (_, model, endpoint, _) in &rows {
        production::valid_selection(model, endpoint, &limit, "25")?;
    }
    let mut catalogs: BTreeMap<String, std::result::Result<Vec<u8>, String>> = BTreeMap::new();
    let mut members = Vec::new();
    let (mut setup, mut task) = (Some(0u64), Some(0u64));
    for (roles, model, endpoint, level) in rows {
        if !catalogs.contains_key(&model) {
            let fetched = production::fetch_catalog(&model, None, api)
                .await
                .map_err(|_| "provider catalog unavailable; no automatic retry".to_owned());
            catalogs.insert(model.clone(), fetched);
        }
        let priced = match &catalogs[&model] {
            Ok(raw) => price(raw, &model, &endpoint, &limit, output, now),
            Err(error) => Err(error.clone()),
        };
        let mut row = match priced {
            Ok(row) => row,
            Err(error) => json!({"available": false, "error": error}),
        };
        let probe = row["probe_reservation_micros"]
            .as_str()
            .and_then(|v| v.parse::<u64>().ok());
        let request = row["task_request_reservation_micros"]
            .as_str()
            .and_then(|v| v.parse::<u64>().ok());
        setup = setup
            .zip(probe)
            .map(|(sum, probe)| sum.saturating_add(probe));
        task = task.zip(request).map(|(max, request)| max.max(request));
        row["roles"] = json!(roles);
        row["model"] = json!(model);
        row["endpoint"] = json!(endpoint);
        row["level"] = json!(level);
        members.push(row);
    }
    let available = members.iter().all(|member| member["available"] == true);
    Ok(json!({
        "status": if available { "estimated" } else { "unavailable" },
        "set": set.map(|set| &set.id),
        "title": set.map(|set| &set.title),
        "notes": set.map(|set| set.notes.clone()).unwrap_or_default(),
        "members": members,
        "setup_minimum_micros": setup.map(Micros::new),
        "task_budget_minimum_micros": task.map(Micros::new),
        "observed_at": now,
        "model_calls": 0,
        "files_written": 0,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qwen_catalog() -> serde_json::Value {
        json!({"data":{"id":"qwen/qwen3.8-max-0902","endpoints":[{
            "tag":"alibaba","status":0,"context_length":1000000,"max_prompt_tokens":983616,
            "max_completion_tokens":131072,"supported_parameters":["tools","tool_choice","max_tokens"],
            "pricing":{"prompt":"0.000002","completion":"0.000006","input_cache_read":"0.00000025","input_cache_write":"0.0000025"}}]}})
    }

    #[test]
    fn pricing_matches_admission_and_rejects_unusable_endpoints() {
        let now = Timestamp::new(1_759_400_000_000);
        let raw = serde_json::to_vec(&qwen_catalog()).unwrap();
        let row = price(
            &raw,
            "qwen/qwen3.8-max-0902",
            "alibaba",
            "0.001",
            16_384,
            now,
        )
        .unwrap();
        assert_eq!(row["probe_reservation_micros"], "6397576");
        assert_eq!(row["probe_pair_micros"], "12795152");
        assert_eq!(row["task_request_reservation_micros"], "6492808");
        assert_eq!(row["within_setup_ceiling"], true);
        assert_eq!(row["per_million_micros"]["input"], "2000000");
        assert_eq!(row["per_million_micros"]["cache_read"], "2000000");
        assert_eq!(row["per_million_micros"]["cache_write"], "2500000");
        assert!(price(&raw, "qwen/qwen3.8-max-0902", "other", "0.001", 512, now).is_err());
        let mut unavailable = qwen_catalog();
        unavailable["data"]["endpoints"][0]["status"] = json!(1);
        let raw = serde_json::to_vec(&unavailable).unwrap();
        assert!(price(&raw, "qwen/qwen3.8-max-0902", "alibaba", "0.001", 512, now).is_err());
    }

    #[tokio::test]
    async fn set_estimates_read_each_catalog_once_without_a_credential() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models/qwen/qwen3.8-max-0902/endpoints"))
            .respond_with(|request: &wiremock::Request| {
                assert!(!request.headers.contains_key("authorization"));
                ResponseTemplate::new(200).set_body_json(qwen_catalog())
            })
            .expect(1)
            .mount(&server)
            .await;
        let now = Timestamp::new(1_759_400_000_000);
        let request = Estimate {
            set: Some("quick".into()),
            model: None,
            endpoint: None,
            request_price_limit: None,
            output_tokens: None,
        };
        let value = estimate_with(&request, &server.uri(), now).await.unwrap();
        assert_eq!(value["status"], "estimated");
        assert_eq!(value["setup_minimum_micros"], "6397576");
        assert_eq!(value["task_budget_minimum_micros"], "6492808");
        assert_eq!(value["members"][0]["roles"], json!(["main"]));
        assert_eq!(value["model_calls"], 0);
        // A missing endpoint is reported, never substituted.
        Mock::given(method("GET"))
            .and(path("/models/z-ai/glm-5.3-flash/endpoints"))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/models/qwen/qwen3.8-flash/endpoints"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"data":{"endpoints":[]}})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let request = Estimate {
            set: Some("medium".into()),
            ..request
        };
        let value = estimate_with(&request, &server.uri(), now).await.unwrap();
        assert_eq!(value["status"], "unavailable");
        assert!(value["setup_minimum_micros"].is_null());
        assert_eq!(value["members"].as_array().unwrap().len(), 2);
        assert!(value["members"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["available"] == false));
        assert_eq!(
            server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|r| r.method == wiremock::http::Method::POST)
                .count(),
            0
        );
    }
}
