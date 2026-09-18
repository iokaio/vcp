// SPDX-License-Identifier: Apache-2.0
//! Native qualification entry point; uses only public synthetic fixture inputs.
use serde::Deserialize;
use std::{path::Path, time::Instant};
use vcp_embedding::{Embedding, Error, MiniLm, DIMENSIONS, MAX_TOKENS};

#[derive(Deserialize)]
struct Case {
    text: String,
    vector: Vec<f32>,
}
#[derive(Deserialize)]
struct Golden {
    model_revision: String,
    cases: Vec<Case>,
}
fn compare(actual: &[Embedding], expected: &[Vec<f32>]) -> Result<f32, String> {
    if actual.len() != expected.len() {
        return Err("missing embedding rows".into());
    }
    let mut maximum = 0f32;
    for (row, truth) in actual.iter().zip(expected) {
        if row.vector.len() != DIMENSIONS || truth.len() != DIMENSIONS {
            return Err("dimension mismatch".into());
        }
        for (a, b) in row.vector.iter().zip(truth) {
            let delta = (a - b).abs();
            if !delta.is_finite() || delta > 1e-5 {
                return Err("reference vector mismatch".into());
            }
            maximum = maximum.max(delta);
        }
        let norm: f64 = row.vector.iter().map(|v| (*v as f64).powi(2)).sum();
        if (norm - 1.).abs() > 1e-5 {
            return Err("non-unit embedding".into());
        }
    }
    Ok(maximum)
}
fn qualify(root: &Path) -> Result<serde_json::Value, Error> {
    let failure = |reason: String| Error::Runtime { reason };
    let golden: Golden = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/local-embeddings/minilm-golden.json"
    ))
    .map_err(|e| failure(e.to_string()))?;
    if golden.model_revision != "1110a243fdf4706b3f48f1d95db1a4f5529b4d41"
        || golden.cases.len() != 4
    {
        return Err(failure("unexpected golden fixture identity".into()));
    }
    let texts: Vec<_> = golden.cases.iter().map(|c| c.text.as_str()).collect();
    let expected: Vec<_> = golden.cases.iter().map(|c| c.vector.clone()).collect();
    let started = Instant::now();
    let model = MiniLm::load(root)?;
    let load_ms = started.elapsed().as_millis();
    let inference = Instant::now();
    let batch = model.embed(&texts)?;
    let batch_ms = inference.elapsed().as_millis();
    let reference_delta = compare(&batch, &expected).map_err(failure)?;
    let single = model.embed(&texts[..1])?;
    let batch_delta = compare(&single, &[batch[0].vector.clone()]).map_err(failure)?;
    let long = "task ".repeat(1000);
    let truncated = model.embed(&[&long])?;
    if !truncated[0].truncated
        || truncated[0].tokens_used != MAX_TOKENS
        || batch.iter().any(|e| e.truncated)
    {
        return Err(failure("truncation reporting mismatch".into()));
    }
    drop(model);
    let reopened = MiniLm::load(root)?.embed(&texts)?;
    let reopen_delta = compare(&reopened, &expected).map_err(failure)?;
    Ok(
        serde_json::json!({"status":"pass","device":"cpu","dimensions":DIMENSIONS,
        "asset_spec_sha256":MiniLm::specification_sha256(),"cases":4,"checks":4,
        "checks_passed":["independent_reference","batch_padding","truncation_reporting","model_reopen"],
        "reference_max_delta":reference_delta,"batch_max_delta":batch_delta,"reopen_max_delta":reopen_delta,
        "load_ms":load_ms,"batch_ms":batch_ms,
        "limitations":["Synthetic embedding qualification; no VCP index, OS network-denial or resource-envelope qualification."]}),
    )
}
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let result = match args.as_slice() {
        [assets] => qualify(Path::new(assets)),
        [mode, address, port, nonce] if mode == "--network-control" =>
            network::Canary::new(&address.to_string_lossy(), &port.to_string_lossy(), &nonce.to_string_lossy())
                .and_then(|canary| canary.observe(false))
                .map(|observation| serde_json::json!({"status":"pass","phase":"network-control","observation":observation})),
        [mode, assets, address, port, nonce] if mode == "--offline" => (|| {
            let canary = network::Canary::new(&address.to_string_lossy(), &port.to_string_lossy(), &nonce.to_string_lossy())?;
            let before = canary.observe(true)?;
            let mut result = match qualify(Path::new(assets)) {
                Ok(value) => value,
                Err(error) => {
                    println!("{}", serde_json::json!({"status":"error","network_before":before,"failure":error}));
                    return Err(error);
                }
            };
            let after = canary.observe(true)?;
            result["network"] = serde_json::json!({"before":before,"after":after});
            result["limitations"] = serde_json::json!([
                "Parent token, traffic and control verification is required; binary output alone is not OS denial proof.",
                "CPU fixture only; no VCP index, durable store or full resource-envelope qualification."
            ]);
            Ok(result)
        })(),
        _ => {
            eprintln!("Required: qualify <assets> | --network-control <private-ipv4> <port> <nonce> | --offline <assets> <private-ipv4> <port> <nonce>");
            std::process::exit(2);
        }
    };
    match result {
        Ok(result) => println!("{result}"),
        Err(error) => {
            let code = if matches!(error, Error::MissingAsset { .. }) {
                3
            } else {
                1
            };
            eprintln!("{error}");
            std::process::exit(code);
        }
    }
}

#[path = "qualify/network.rs"]
mod network;
