// SPDX-License-Identifier: Apache-2.0
//! Offline profile preparation using actual `setup provider-metadata` captures.
use serde_json::json;
use std::{collections::BTreeMap, fs, path::PathBuf};
use vcp_cli::model_preferences::{routing_configuration, ChoicePreferences, ModelSet, Preferences};
use vcp_models::catalog::Snapshot;

fn prepare() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if !(args.len() == 4 || args.len() == 5) {
        return Err("usage: model_rotation_profiles NEW_OUTPUT QWEN_GOOGLE_METADATA DEEPSEEK_DEEPINFRA_METADATA GLM_DEEPINFRA_METADATA [QWEN_VENICE_METADATA]".into());
    }
    let expected = [
        ("qwen/qwen3-coder", "google-vertex/us-south1"),
        ("deepseek/deepseek-v3.2", "deepinfra/fp4"),
        ("z-ai/glm-4.7", "deepinfra/fp4"),
        ("qwen/qwen3-coder", "venice/fp8"),
    ];
    let mut prepared = Vec::new();
    for (directory, (model, endpoint)) in args.iter().skip(1).zip(expected) {
        let raw =
            vcp_cli::settings::read_bounded(&directory.join("endpoints.json"), 4 * 1024 * 1024)?;
        let snapshot: Snapshot = serde_json::from_slice(&vcp_cli::settings::read_bounded(
            &directory.join("snapshot.json"),
            1024 * 1024,
        )?)
        .map_err(|_| "invalid captured snapshot")?;
        if snapshot.compatibility.model != model || snapshot.compatibility.endpoint != endpoint {
            return Err("metadata directory has a different exact model/endpoint".into());
        }
        snapshot
            .current(vcp_cli::settings::now())
            .map_err(|e| e.to_string())?;
        let validated = Snapshot::from_endpoints(
            &raw,
            snapshot.observed_at,
            snapshot.valid_until,
            snapshot.compatibility.clone(),
        )
        .map_err(|e| e.to_string())?;
        if validated != snapshot {
            return Err("snapshot/catalog provenance mismatch".into());
        }
        prepared.push((snapshot, raw));
    }
    let qwen_endpoints = prepared
        .iter()
        .filter(|(s, _)| s.compatibility.model == expected[0].0)
        .map(|(s, _)| s.compatibility.endpoint.clone())
        .collect();
    let first = ChoicePreferences {
        models: vec![expected[0].0.into(), expected[1].0.into()],
        endpoints: BTreeMap::from([
            (expected[0].0.into(), qwen_endpoints),
            (expected[1].0.into(), vec![expected[1].1.into()]),
        ]),
        max_reference_request_cost_usd: None,
    };
    let second = ChoicePreferences {
        models: vec![expected[2].0.into()],
        endpoints: BTreeMap::from([(expected[2].0.into(), vec![expected[2].1.into()])]),
        max_reference_request_cost_usd: None,
    };
    let roles = [
        "main",
        "child",
        "helper",
        "compaction",
        "reviewer",
        "verification",
        "optimizer",
        "memory",
    ];
    let preferences = Preferences {
        version: 1,
        set: ModelSet {
            id: "model-rotation-qualification".into(),
            label: "Synthetic model rotation qualification".into(),
            maker: "mixed".into(),
            project_type: "synthetic-source-edit".into(),
            rationale: "Owner-selected exact endpoints; no empirical quality claim".into(),
            roles: roles
                .iter()
                .map(|r| {
                    (
                        r.to_string(),
                        expected[..3].iter().map(|(m, _)| m.to_string()).collect(),
                    )
                })
                .collect(),
        },
        budget_usd: "3.000000".into(),
        choice_sets: roles
            .iter()
            .map(|r| (r.to_string(), vec![first.clone(), second.clone()]))
            .collect(),
    };
    let routing = routing_configuration(&preferences, &prepared)?;
    // Keep the ordinary absolute spelling. Windows canonicalize adds the device
    // prefix, which the campaign's component-by-component link checks reject.
    // The runner validates the path and freezes the exact bytes before dispatch.
    let catalog = std::path::absolute(args[1].join("endpoints.json"))
        .map_err(|_| "catalog path unavailable")?;
    let base = json!({"version":1,"workspace":"WORKSPACE_REPLACED_BY_RUNNER","trust_workspace":true,"maximum_autonomy":"autonomous","automatic_effects":["read","write","execute","opaque"],"canonical_tools":["vcp_read","vcp_patch","vcp_verify"],"budget_usd":"3.000000","provider":prepared[0].0,"routing":null,"catalog":catalog,"affected_paths":["value.cjs"],"max_requests":12,"output_tokens":"1024","provider_timeout_seconds":45,"max_transport_retries":2,"deadline_seconds":180,"processes":[],"checks":[]});
    // Deserialize through the real profile contract before writing either template.
    let _: vcp_cli::settings::Profile =
        serde_json::from_value(base.clone()).map_err(|e| e.to_string())?;
    let mut rotated = base.clone();
    rotated["routing"] = serde_json::to_value(routing).map_err(|e| e.to_string())?;
    let _: vcp_cli::settings::Profile =
        serde_json::from_value(rotated.clone()).map_err(|e| e.to_string())?;
    fs::create_dir(&args[0]).map_err(|_| "new output directory required")?;
    for (name, value) in [
        ("baseline.json", base),
        ("rotation.json", rotated),
        ("preferences.json", json!(preferences)),
    ] {
        fs::write(
            args[0].join(name),
            serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
        )
        .map_err(|_| "profile write failed")?;
    }
    Ok(())
}
fn main() {
    if let Err(error) = prepare() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
