// SPDX-License-Identifier: Apache-2.0
//! Explicit offline qualification of reviewed probe and generation receipts.
use super::*;
use vcp_models::catalog::{attribution, Compatibility, Snapshot};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    path: PathBuf,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    probe_spec: Source,
    report: Source,
    generations: [Source; 2],
    catalog: Source,
    observed_at: Timestamp,
    valid_until: Timestamp,
}
fn read(source: &Source) -> Result<Vec<u8>> {
    reject_links(&source.path)?;
    let bytes = vcp_cli::settings::read_bounded(&source.path, 4 * 1024 * 1024)?;
    if digest_bytes(&bytes) != source.sha256 {
        return Err("qualification source changed".into());
    }
    Ok(bytes)
}
pub(super) fn run(spec_path: &Path, output: &Path, authorized: &str) -> Result<()> {
    reject_links(spec_path)?;
    reject_links(output)?;
    let bytes = vcp_cli::settings::read_bounded(spec_path, 1024 * 1024)?;
    if digest_bytes(&bytes) != authorized {
        return Err("qualification authorization hash differs".into());
    }
    let input: Input = serde_json::from_slice(&bytes)?;
    let spec: Spec = serde_json::from_slice(&read(&input.probe_spec)?)?;
    let report: serde_json::Value = serde_json::from_slice(&read(&input.report)?)?;
    let raw = read(&input.catalog)?;
    if report["schema"] != "p6-provider-conformance/1"
        || report["status"] != "observed"
        || report["responses_text_tools"] != true
        || report["ledger"]["active"] != "0"
        || report["ledger"]["unresolved"] != "0"
        || report["ledger"]["overrun"] != false
        || report["candidate"]["raw_sha256"] != spec.catalog_sha256
        || report["candidate"]["price"]["model"] != spec.model
        || report["candidate"]["price"]["provider"] != spec.endpoint
        || input.observed_at < spec.observed_at
        || input.observed_at > now()
        || input.valid_until <= now()
        || input.valid_until.get() > spec.observed_at.get().saturating_add(86_400_000)
    {
        return Err("completed current probe, catalog and accounted cost required".into());
    }
    let original = vcp_cli::settings::read_bounded(&spec.catalog, 4 * 1024 * 1024)?;
    if digest_bytes(&original) != spec.catalog_sha256 {
        return Err("original probe catalog changed".into());
    }
    let candidate = |bytes: &[u8]| {
        CandidateMetadata::from_endpoints(
            bytes,
            spec.observed_at,
            spec.valid_until,
            spec.model.clone(),
            spec.endpoint.clone(),
            spec.request_price_limit.clone(),
            BTreeSet::from(["tools".into(), "tool_choice".into(), "max_tokens".into()]),
        )
    };
    let prior = candidate(&original)?;
    let current = candidate(&raw)?;
    if prior.context != current.context
        || prior.max_input != current.max_input
        || prior.max_output != current.max_output
        || prior.price.rates != current.price.rates
    {
        return Err("qualified endpoint capabilities or tariffs changed".into());
    }
    let responses: Vec<ResultBody> = serde_json::from_value(report["responses"].clone())?;
    if responses.len() != 2
        || responses[0].calls.len() != 1
        || responses[0].calls[0].name != "vcp_conformance_echo"
        || responses[0].calls[0].arguments != json!({"marker":conformance::MARKER})
        || !responses[1].calls.is_empty()
        || responses[1]
            .completed_messages
            .values()
            .cloned()
            .collect::<Vec<_>>()
            .join("")
            .trim()
            != conformance::FINAL
    {
        return Err("fixed tool and continuation proof required".into());
    }
    let mut attributions = Vec::new();
    let mut settled = 0u64;
    for (response, source) in responses.iter().zip(&input.generations) {
        attributions.push(attribution::from_generation(
            &raw,
            &read(source)?,
            response,
            &spec.model,
            &spec.endpoint,
        )?);
        settled = settled
            .checked_add(
                response
                    .usage
                    .as_ref()
                    .and_then(|u| u.cost.as_ref())
                    .ok_or("missing charge")?
                    .micros
                    .get(),
            )
            .ok_or("charge overflow")?;
    }
    if report["actual_cost_micros"] != settled.to_string()
        || report["ledger"]["settled"] != settled.to_string()
        || attributions[0].observed_model_revision != attributions[1].observed_model_revision
        || attributions[0].observed_endpoint_id != attributions[1].observed_endpoint_id
    {
        return Err("probe attribution or charge drift".into());
    }
    let compatibility = Compatibility {
        id: format!("p6-generation-qualified/{authorized}"),
        model: spec.model,
        endpoint: spec.endpoint,
        qualified_at: input.observed_at,
        valid_until: input.valid_until,
        responses_text_tools: true,
        byte_ceiling_qualified: false,
        provider_preferences_qualified: true,
        qualified_reasoning_efforts: BTreeSet::new(),
        deny_data_collection: true,
        require_zdr: false,
        request_price_limit: spec.request_price_limit,
        required_parameters: BTreeSet::from([
            "tools".into(),
            "tool_choice".into(),
            "max_tokens".into(),
        ]),
    };
    let snapshot =
        Snapshot::from_endpoints(&raw, input.observed_at, input.valid_until, compatibility)?;
    std::fs::create_dir(output)?;
    fresh_file(
        &output.join("claim.json"),
        &json!({"schema":"p6-provider-qualification/1","authorized_sources_sha256":authorized,"attribution":attributions,"limitations":["Catalog tag inferred only through unique complete-catalog provider-name mapping and exact catalog model-id/name revision binding; internal UUID retained separately.","Qualification applies only to observed alias/revision and dated catalog; no global tokenizer or quality proof."]}),
    )?;
    fresh_file(&output.join("snapshot.json"), &snapshot)?;
    println!(
        "{}",
        json!({"snapshot":output.join("snapshot.json"),"snapshot_id":snapshot.id,"byte_ceiling_qualified":false})
    );
    Ok(())
}
