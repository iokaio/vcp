// SPDX-License-Identifier: Apache-2.0
//! Explicit offline qualification against the retained B T2 spool. No owner,
//! workspace mutation, provider call, or fixture content is required in CI.
use serde_json::{json, Value};
use std::{fs, path::PathBuf};
use vcp_context::{
    compaction::{compact, Config},
    manifest::{Content, Kind, Manifest, Part, Trust},
};
use vcp_domain::{artifact::ArtifactDescriptor, ByteCount};
use vcp_protocol::{canonical_bytes, digest_bytes};

fn read(spool: &PathBuf, id: &str) -> (ArtifactDescriptor, Vec<u8>) {
    let seal: Value =
        serde_json::from_slice(&fs::read(spool.join(id).join("seal.json")).unwrap()).unwrap();
    let descriptor: ArtifactDescriptor =
        serde_json::from_value(seal["descriptor"].clone()).unwrap();
    let mut files: Vec<_> = fs::read_dir(spool.join(id))
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "chunk"))
        .collect();
    files.sort();
    let bytes: Vec<_> = files
        .into_iter()
        .flat_map(|p| fs::read(p).unwrap())
        .collect();
    assert_eq!(descriptor.spec.id.as_str(), id);
    assert_eq!(bytes.len() as u64, descriptor.length.get());
    assert_eq!(digest_bytes(&bytes), descriptor.sha256);
    (descriptor, bytes)
}

// Exact request::encode input framing. The retained manifest proves the old
// whole-request byte count; provider settings, envelope and schemas do not
// change. Subtract/add the serialized input arrays, including their commas.
fn input(parts: &[Part]) -> Vec<u8> {
    let values: Vec<_> = parts.iter().map(|p| match &p.content {
        Content::ToolCall { id, name, arguments } => json!({"type":"function_call","call_id":id,"name":name,
            "arguments":String::from_utf8(canonical_bytes(arguments).unwrap()).unwrap()}),
        Content::ToolResult { id, output } => json!({"type":"function_call_output","call_id":id,"output":output}),
        Content::Text { text } => {
            let role = match p.kind { Kind::Operating => "system", Kind::ProjectInstruction => "developer", _ => "user" };
            let quoted = serde_json::to_string(&json!({"kind":p.kind,"trust":p.trust,"artifact":p.artifact,
                "source_sha256":p.source_hash,"range":[p.start,p.end],"applicable_paths":p.applicable_paths,"text":text})).unwrap();
            json!({"type":"message","role":role,"content":[{"type":"input_text","text":quoted}]})
        }
    }).collect();
    canonical_bytes(&values).unwrap()
}

#[test]
#[ignore = "requires VCP_B_T2_REPLAY_SPOOL pointing to retained 20261004-105658 B spool"]
fn retained_b_seventh_context_fits_without_losing_recent_pairs_or_sources() {
    let spool = PathBuf::from(std::env::var("VCP_B_T2_REPLAY_SPOOL").unwrap());
    let (_, raw) = read(&spool, "782d0857-30c9-4919-8038-a5d717409f71");
    let manifest: Manifest = serde_json::from_slice(&raw).unwrap();
    assert_eq!(manifest.input_estimate.get(), 93_801);
    assert_eq!(manifest.envelope.context.get(), 200_000);
    let capacity = 200_000 - 8192 - 512;
    let old_input = input(&manifest.included).len();
    let fixed_framing = 93_801usize.checked_sub(old_input).unwrap();
    let mut parts = manifest.included.clone();
    for (index, id) in [
        (1, "b90062cb-0f72-44a2-8300-6a5693f9ee68"),
        (2, "58071def-e7d0-45a6-b637-98094f87d40f"),
        (3, "ee95df69-ac59-4fbc-9c7d-909ba6a86aea"),
        (5, "472e1143-8f6e-4c29-a0b5-f878d2db76a3"),
    ] {
        let (descriptor, bytes) = read(&spool, id);
        let old = &parts[index];
        parts[index] = Part::captured_text(
            id.into(),
            old.kind,
            old.trust,
            &descriptor,
            &bytes,
            old.mandatory,
            old.rank,
            old.reason.clone(),
        )
        .unwrap();
    }
    let (_, pair_raw) = read(&spool, "fcc3e6f7-9712-4178-a56f-1d0f49a2f070");
    let pair: Value = serde_json::from_slice(&pair_raw).unwrap();
    let newest: Vec<Part> = serde_json::from_value(pair["parts"].clone()).unwrap();
    assert_eq!(newest.len(), 2);
    assert_eq!(newest[1].content.bytes().unwrap().len(), 137_538);
    parts.extend(newest);
    let before = fixed_framing + input(&parts).len();
    assert!(before > capacity);
    let history: Vec<_> = parts
        .iter()
        .filter(|p| matches!(p.kind, Kind::ToolCall | Kind::ToolResult))
        .cloned()
        .collect();
    assert_eq!(history.len(), 14);
    let (_, config_raw) = read(&spool, "1d55298f-3932-4c3b-90a8-a2410ebebd88");
    let config: Config = serde_json::from_slice(&config_raw).unwrap();
    let projection = compact(&history, &manifest.revisions, &config)
        .unwrap()
        .unwrap();
    assert_eq!(projection.compacted_pairs, 2);
    assert_eq!(projection.retained, history[2..12]);
    assert_eq!(projection.sources.len(), 14);
    let verified = projection
        .revalidate(&manifest.revisions, &history, |id| {
            Ok(read(&spool, id.as_str()).1)
        })
        .unwrap();
    let mut descriptor = read(&spool, "3021d0d5-8761-431f-bd18-f26681636070").0;
    descriptor.spec.id = vcp_domain::ArtifactId::new();
    descriptor.length = ByteCount::new(projection.summary.len() as u64);
    descriptor.sha256 = digest_bytes(projection.summary.as_bytes());
    descriptor.retained[0].end = descriptor.length;
    let summary = verified.captured_part(&descriptor).unwrap();
    assert_eq!(summary.trust, Trust::Untrusted);
    let mut projected: Vec<_> = parts
        .iter()
        .filter(|p| !matches!(p.kind, Kind::ToolCall | Kind::ToolResult))
        .cloned()
        .collect();
    projected.push(summary);
    projected.extend(projection.retained.clone());
    let after = fixed_framing + input(&projected).len();
    assert!(after <= capacity);
    let expanded_config = Config {
        keep_recent_pairs: 12,
        ..config.clone()
    };
    let expanded = compact(&history, &manifest.revisions, &expanded_config)
        .unwrap()
        .unwrap();
    assert_eq!(expanded.retained, history[..12]);
    assert_eq!(
        expanded.compacted_pairs, 1,
        "oversized newest pair stays summarized"
    );
    let expanded_verified = expanded
        .revalidate(&manifest.revisions, &history, |id| {
            Ok(read(&spool, id.as_str()).1)
        })
        .unwrap();
    let mut expanded_descriptor = descriptor.clone();
    expanded_descriptor.length = ByteCount::new(expanded.summary.len() as u64);
    expanded_descriptor.sha256 = digest_bytes(expanded.summary.as_bytes());
    expanded_descriptor.retained[0].end = expanded_descriptor.length;
    let mut expanded_parts: Vec<_> = parts
        .iter()
        .filter(|p| !matches!(p.kind, Kind::ToolCall | Kind::ToolResult))
        .cloned()
        .collect();
    expanded_parts.push(
        expanded_verified
            .captured_part(&expanded_descriptor)
            .unwrap(),
    );
    expanded_parts.extend(expanded.retained.clone());
    let expanded_after = fixed_framing + input(&expanded_parts).len();
    assert_eq!(expanded_after, 97819);
    for id in [&history[1].artifact, &history[13].artifact] {
        assert!(projection
            .revalidate(&manifest.revisions, &history, |source| {
                if source == id {
                    Ok(b"tampered omitted source".to_vec())
                } else {
                    Ok(read(&spool, source.as_str()).1)
                }
            })
            .is_err());
    }
    println!(
        "{}",
        json!({"algorithm":"bounded-tool-pair-previews/3","pairs":7,
        "sources_verified":14,"history_sha256":projection.input_digest,"capacity":capacity,
        "before_request_bytes":before,"after_request_bytes":after,"headroom":capacity-after,
        "full_recent_pairs":projection.retained.len()/2,"expanded_after":expanded_after,"expanded_full_pairs":expanded.retained.len()/2,"originals_modified":false})
    );
}

#[test]
#[ignore = "requires VCP_CAPACITY_A_SPOOL and VCP_CAPACITY_B_SPOOL retained campaign evidence"]
fn retained_a_loop_and_older_b_overflow_fit_expanded_recent_window() {
    for (variable, receipt_id, expected_base, expected_expanded) in [
        (
            "VCP_CAPACITY_A_SPOOL",
            "c2190ebc-031c-4398-91b1-d9d8479a7d5c",
            116044,
            155611,
        ),
        (
            "VCP_CAPACITY_B_SPOOL",
            "81a58ce5-db7d-40fe-b35c-f2d968aca84d",
            147895,
            174366,
        ),
    ] {
        let spool = PathBuf::from(std::env::var(variable).unwrap());
        let receipt: Value = serde_json::from_slice(&read(&spool, receipt_id).1).unwrap();
        let old: vcp_context::compaction::Projection =
            serde_json::from_value(receipt["projection"].clone()).unwrap();
        let summary: Value = serde_json::from_str(&old.summary).unwrap();
        let mut pairs = std::collections::BTreeMap::new();
        for (index, pair) in summary["pairs"].as_array().unwrap().iter().enumerate() {
            let mut parts = Vec::new();
            for (field, kind) in [("arguments", Kind::ToolCall), ("result", Kind::ToolResult)] {
                let id = pair[field]["artifact"].as_str().unwrap();
                let (descriptor, bytes) = read(&spool, id);
                let mut part = Part::captured_text(
                    id.into(),
                    kind,
                    if kind == Kind::ToolCall {
                        Trust::Observed
                    } else {
                        Trust::Untrusted
                    },
                    &descriptor,
                    &bytes,
                    true,
                    0,
                    "canonical coding source".into(),
                )
                .unwrap();
                part.content = serde_json::from_slice(&bytes).unwrap();
                parts.push(part);
            }
            pairs.insert(
                pair["pair_index"]
                    .as_u64()
                    .map(|n| n as usize)
                    .unwrap_or(index),
                parts,
            );
        }
        let prefix = pairs.len();
        for (index, pair) in old.retained.chunks_exact(2).enumerate() {
            let position = summary["retained_pairs"]
                .get(index)
                .and_then(|p| p["pair_index"].as_u64())
                .map(|n| n as usize)
                .unwrap_or(prefix + index);
            pairs.insert(position, pair.to_vec());
        }
        let history: Vec<Part> = pairs.into_values().flatten().collect();
        assert_eq!(
            digest_bytes(&canonical_bytes(&history).unwrap()),
            old.input_digest
        );
        let summary_id = receipt["summary_artifact"].as_str().unwrap();
        let old_descriptor = read(&spool, summary_id).0;
        let old_part = Part::captured_text(
            summary_id.into(),
            Kind::History,
            Trust::Untrusted,
            &old_descriptor,
            old.summary.as_bytes(),
            true,
            0,
            "deterministic tool history preview; original artifacts retained".into(),
        )
        .unwrap();
        let mut old_parts = vec![old_part];
        old_parts.extend(old.retained.clone());
        let fixed =
            receipt["input_estimate_after"].as_u64().unwrap() as usize - input(&old_parts).len();
        for (keep_recent_pairs, expected) in [(6, expected_base), (12, expected_expanded)] {
            let config = Config {
                keep_recent_pairs,
                ..old.config.clone()
            };
            let projection = compact(&history, &old.revisions, &config).unwrap().unwrap();
            assert_eq!(projection.sources, old.sources);
            let verified = projection
                .revalidate(&old.revisions, &history, |id| {
                    Ok(read(&spool, id.as_str()).1)
                })
                .unwrap();
            let mut descriptor = old_descriptor.clone();
            descriptor.length = ByteCount::new(projection.summary.len() as u64);
            descriptor.sha256 = digest_bytes(projection.summary.as_bytes());
            descriptor.retained[0].end = descriptor.length;
            let mut selected = vec![verified.captured_part(&descriptor).unwrap()];
            selected.extend(projection.retained.clone());
            let measured = fixed + input(&selected).len();
            assert_eq!(measured, expected);
            assert!(measured <= 191296);
            if keep_recent_pairs == 12 {
                assert_eq!(projection.retained, history[history.len() - 24..]);
            }
            println!(
                "{}",
                json!({"case":variable,"recent_pairs":keep_recent_pairs,"request_bytes":measured,"capacity":191296,"verified_sources":projection.sources.len(),"originals_modified":false})
            );
        }
    }
}
