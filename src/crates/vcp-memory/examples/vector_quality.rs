// SPDX-License-Identifier: Apache-2.0
//! Actual CPU vector/index qualification under the native network-denial broker.
#[path = "../../vcp-embedding/src/bin/qualify/network.rs"]
mod network;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeSet, path::Path, time::Instant};
use vcp_domain::{workspace::Scope, *};
use vcp_memory::{
    embedding::{self, Encoded, LocalEmbedding, Specification},
    vector::{Component, Mode},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const CORPUS: &[u8] = include_bytes!("../../../tests/fixtures/local-memory/corpus.json");
const K: usize = 3;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    id: String,
    workspace: String,
    current: bool,
    supersedes: Option<String>,
    text: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    workspace: String,
    text: String,
    lexical_required: Vec<String>,
    semantic_required: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Corpus {
    version: u32,
    documents: Vec<Document>,
    queries: Vec<Case>,
}

fn oracle(
    rows: &[Encoded],
    vector: &[f32],
    authorized: &BTreeSet<String>,
    limit: usize,
) -> Vec<String> {
    let norm = |values: &[f32]| {
        values
            .iter()
            .map(|x| f64::from(*x).powi(2))
            .sum::<f64>()
            .sqrt()
    };
    let mut distances: Vec<_> = rows
        .iter()
        .filter(|row| authorized.contains(&row.identity.id))
        .map(|row| {
            let dot = row
                .vector
                .iter()
                .zip(vector)
                .map(|(a, b)| f64::from(*a) * f64::from(*b))
                .sum::<f64>();
            (
                1.0 - dot / (norm(&row.vector) * norm(vector)),
                row.identity.id.clone(),
            )
        })
        .collect();
    distances.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    distances
        .into_iter()
        .take(limit)
        .map(|(_, id)| id)
        .collect()
}
fn mode(value: Mode) -> &'static str {
    match value {
        Mode::Ann => "ann",
        Mode::ExactAuthorizedSubset => "exact_authorized_subset",
        Mode::ReducedRecall => "reduced_recall",
    }
}

fn qualify(mut model: LocalEmbedding) -> Result<Value> {
    let corpus: Corpus = serde_json::from_slice(CORPUS)?;
    if corpus.version != 2 || corpus.documents.len() != 24 || corpus.queries.len() != 7 {
        return Err("unexpected fixed corpus shape".into());
    }
    let mut source_ids = BTreeSet::new();
    let mut query_ids = BTreeSet::new();
    for doc in &corpus.documents {
        if !source_ids.insert(&doc.id) || doc.text.is_empty() {
            return Err("invalid fixture source".into());
        }
        if let Some(old) = &doc.supersedes {
            if !corpus
                .documents
                .iter()
                .any(|r| r.id == *old && r.workspace == doc.workspace && !r.current)
            {
                return Err("invalid supersession fixture".into());
            }
        }
    }
    for case in &corpus.queries {
        if !query_ids.insert(&case.id) {
            return Err("duplicate fixture query".into());
        }
        for required in case.lexical_required.iter().chain(&case.semantic_required) {
            if !corpus
                .documents
                .iter()
                .any(|doc| doc.id == *required && doc.workspace == case.workspace && doc.current)
            {
                return Err("unavailable fixture truth".into());
            }
        }
    }
    let temp = tempfile::tempdir()?;
    let spec = Specification::qualified();
    let workspaces: BTreeSet<_> = corpus
        .documents
        .iter()
        .map(|doc| doc.workspace.as_str())
        .collect();
    let mut builds = Vec::new();
    let mut observations = Vec::new();
    let mut checks = Vec::new();
    for workspace in workspaces {
        let workspace_id = WorkspaceId::parse(workspace)?;
        let mut chunks = Vec::new();
        for doc in corpus
            .documents
            .iter()
            .filter(|doc| doc.workspace == workspace && doc.current)
        {
            let scope = Scope {
                workspace: workspace_id.clone(),
                session: SessionId::parse("vector-fixture")?,
                task: TaskId::parse(&doc.id)?,
            };
            chunks.extend(embedding::chunks(&doc.id, &scope, &doc.text, &spec)?);
        }
        let start = Instant::now();
        let rows = model.embed(&chunks, &|| false)?;
        let embed_us = start.elapsed().as_micros();
        let cached = model.embed(&chunks, &|| false)?;
        if rows != cached {
            return Err("embedding cache changed source vectors".into());
        }
        let start = Instant::now();
        let component =
            Component::build(workspace_id.clone(), spec.clone(), rows.clone(), &|| false)?;
        let build_us = start.elapsed().as_micros();
        let path = temp.path().join(format!("{workspace}.json"));
        let checksum = component.save_private(&path, &|| false)?;
        drop(component);
        let start = Instant::now();
        let component = Component::open(&path, &checksum, &workspace_id, &spec, &|| false)?;
        let reopen_us = start.elapsed().as_micros();
        if component.rows() != rows {
            return Err("reopen vector/source identity drift".into());
        }
        let authorized: BTreeSet<_> = rows.iter().map(|row| row.identity.id.clone()).collect();
        builds.push(json!({"workspace":workspace,"sources":corpus.documents.iter().filter(|doc| doc.workspace==workspace && doc.current).count(),"chunks":rows.len(),"embed_us":embed_us,"build_us":build_us,"reopen_us":reopen_us,"component_checksum":checksum}));
        for case in corpus
            .queries
            .iter()
            .filter(|case| case.workspace == workspace)
        {
            let start = Instant::now();
            let vector = model.query(&case.text, &|| false)?;
            let inference_us = start.elapsed().as_micros();
            let truth = oracle(&rows, &vector, &authorized, K);
            let mut prior = None;
            for repetition in 0..=2 {
                let start = Instant::now();
                let result = component.query(&workspace_id, &authorized, &vector, K, &|| false)?;
                let query_us = start.elapsed().as_micros();
                let ids: Vec<_> = result.rows.iter().map(|row| row.chunk.clone()).collect();
                if result.mode != Mode::Ann {
                    return Err("unfiltered fixture did not exercise ANN".into());
                }
                let hits = truth.iter().filter(|id| ids.contains(id)).count();
                let recall = hits as f64 / truth.len() as f64;
                if recall < 1.0 {
                    return Err(
                        format!("ANN oracle recall below declared1.0 for {}", case.id).into(),
                    );
                }
                if let Some(previous) = &prior {
                    if previous != &ids {
                        return Err("warm ANN identity drift".into());
                    }
                } else {
                    prior = Some(ids.clone());
                }
                let mut sources = Vec::new();
                for row in &result.rows {
                    if !authorized.contains(&row.chunk)
                        || !corpus.documents.iter().any(|doc| {
                            doc.id == row.source && doc.workspace == workspace && doc.current
                        })
                    {
                        return Err("unauthorized vector source returned".into());
                    }
                    if !sources.contains(&row.source) {
                        sources.push(row.source.clone());
                    }
                }
                let semantic_hits = case
                    .semantic_required
                    .iter()
                    .filter(|id| sources.contains(id))
                    .count();
                observations.push(json!({"query":case.id,"workspace":workspace,"repetition":repetition,"mode":mode(result.mode),"chunk_ids":ids,"source_ids":sources,"oracle_ids":truth,"ann_recall_at_3":recall,"inference_us":inference_us,"query_us":query_us,
                    "semantic_source_recall_diagnostic":if case.semantic_required.is_empty() {None} else {Some(semantic_hits as f64/case.semantic_required.len() as f64)}}));
            }
            let required = case
                .semantic_required
                .first()
                .or_else(|| case.lexical_required.first())
                .ok_or("fixture lacks relevant source")?;
            let restricted: BTreeSet<_> = rows
                .iter()
                .filter(|row| row.identity.source == *required)
                .map(|row| row.identity.id.clone())
                .collect();
            let filtered = component.query(&workspace_id, &restricted, &vector, K, &|| false)?;
            if filtered.rows.is_empty()
                || filtered
                    .rows
                    .iter()
                    .any(|row| row.source != *required || !restricted.contains(&row.chunk))
            {
                return Err("source-filter failure".into());
            }
            if !component
                .query(&workspace_id, &BTreeSet::new(), &vector, K, &|| false)?
                .rows
                .is_empty()
            {
                return Err("empty authorization scope leaked".into());
            }
            if component
                .query(
                    &WorkspaceId::parse("foreign")?,
                    &authorized,
                    &vector,
                    K,
                    &|| false,
                )
                .is_ok()
            {
                return Err("foreign workspace accepted".into());
            }
            checks.push(json!({"query":case.id,"source_filter":"pass","filtered_mode":mode(filtered.mode),"empty_scope":"pass","foreign_workspace":"rejected"}));
        }
        let mut incompatible = spec.clone();
        incompatible.dimensions += 1;
        if Component::open(&path, &checksum, &workspace_id, &incompatible, &|| false).is_ok() {
            return Err("incompatible dimension accepted".into());
        }
        let mut bytes = std::fs::read(&path)?;
        let last = bytes.last_mut().ok_or("empty component")?;
        *last ^= 1;
        std::fs::write(&path, bytes)?;
        if Component::open(&path, &checksum, &workspace_id, &spec, &|| false).is_ok() {
            return Err("corrupt component accepted".into());
        }
    }
    Ok(
        json!({"status":"pass","phase":"vector-quality","device":"cpu","dimensions":spec.dimensions,"asset_spec_sha256":spec.assets,"specification_digest":spec.digest()?,"corpus_sha256":vcp_protocol::digest_bytes(CORPUS),"vector_engine":"diskann","top_k":K,"ann_minimum_recall":1.0,"cases":corpus.queries.len(),"repetitions":3,"builds":builds,"queries":observations,"scope_checks":checks,
        "checks_passed":["real_cpu_embedding","embedding_cache","ann_vs_exhaustive","component_reopen","source_authorization","foreign_workspace","corrupt_component","incompatible_dimensions"],
        "limitations":["Public bounded synthetic corpus, not a production recall or resident-memory guarantee.","ANN-vs-exhaustive is chunk recall; semantic source labels are separate diagnostics, never fused results.","Caller fixture authorization is not the canonical host or P5-05 publication campaign."]}),
    )
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result: Result<Value> = (|| {
        match args.as_slice() {
            [mode,address,port,nonce] if mode=="--network-control" => Ok(json!({"status":"pass","phase":"network-control","observation":network::Canary::new(address,port,nonce)?.observe(false)?})),
            [mode,assets,address,port,nonce] if mode=="--offline" => {
                let canary=network::Canary::new(address,port,nonce)?;
                let before=canary.observe(true)?;
                let started=Instant::now();
                let model=match LocalEmbedding::load(Path::new(assets), &|| false) {
                    Ok(model)=>model,
                    Err(_)=> { println!("{}",json!({"status":"error","network_before":before,"failure":{"kind":"local_embedding_load_failed","stage":"load"}})); return Err("local_embedding_load_failed".into()); }
                };
                let load_us=started.elapsed().as_micros();
                let mut report=qualify(model)?;
                report["load_us"]=json!(load_us);
                report["network"]=json!({"before":before,"after":canary.observe(true)?});
                Ok(report)
            },
            _=>Err("required --network-control <private-ip> <port> <nonce> or --offline <assets> <private-ip> <port> <nonce>".into()),
        }
    })();
    match result {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("vector quality failed: {error}");
            std::process::exit(1);
        }
    }
}
