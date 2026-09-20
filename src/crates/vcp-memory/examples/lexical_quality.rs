// SPDX-License-Identifier: Apache-2.0
//! Install at vcp-memory/examples/lexical_quality.rs after P5-03 registration.
//! Frozen public fixture; measured lexical output only, never hybrid/fused recall.
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::AtomicBool,
    time::Instant,
};
use vcp_domain::{
    artifact::Range,
    memory::{EvidenceStatus, Outcome},
    workspace::Scope,
    *,
};
use vcp_memory::{
    lexical::{self, Limits, Query},
    search_record::{ChunkerSpec, Exclusion, Inventory, SearchKind, SearchRecord, TextSource},
};

const CORPUS: &[u8] = include_bytes!("../../../tests/fixtures/local-memory/corpus.json");
const TOP_K: usize = 3;
const WARM_REPETITIONS: usize = 5;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

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

fn parse() -> Result<Corpus> {
    let corpus: Corpus = serde_json::from_slice(CORPUS)?;
    if corpus.version != 2
        || corpus.documents.is_empty()
        || corpus.documents.len() > 10_000
        || corpus.queries.is_empty()
        || corpus.queries.len() > 64
    {
        return Err("unsupported corpus shape".into());
    }
    let mut ids = BTreeSet::new();
    for doc in &corpus.documents {
        ArtifactId::parse(&doc.id)?;
        WorkspaceId::parse(&doc.workspace)?;
        if !ids.insert(&doc.id) || doc.text.is_empty() || doc.text.len() > 4096 {
            return Err("invalid bounded fixture document".into());
        }
        if let Some(old) = &doc.supersedes {
            if !corpus.documents.iter().any(|candidate| {
                candidate.id == *old && candidate.workspace == doc.workspace && !candidate.current
            }) {
                return Err("invalid fixture supersession".into());
            }
        }
    }
    let mut cases = BTreeSet::new();
    for case in &corpus.queries {
        if !cases.insert(&case.id)
            || case.text.is_empty()
            || case.text.len() > 4096
            || !corpus
                .documents
                .iter()
                .any(|doc| doc.workspace == case.workspace)
        {
            return Err("invalid fixture query".into());
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
    Ok(corpus)
}

fn inventory(corpus: &Corpus, workspace: &str) -> Result<Inventory> {
    let chunker = ChunkerSpec::default().digest()?;
    let mut records = Vec::new();
    let mut exclusions = Vec::new();
    for doc in corpus
        .documents
        .iter()
        .filter(|doc| doc.workspace == workspace)
    {
        let source = TextSource::Artifact {
            id: ArtifactId::parse(&doc.id)?,
        };
        if !doc.current {
            exclusions.push(Exclusion {
                source: Some(source),
                reason: "fixture marks this source superseded".into(),
            });
            continue;
        }
        let symbol = doc
            .text
            .split_whitespace()
            .nth(1)
            .ok_or("fixture function name missing")?;
        let mut record = SearchRecord {
            id: String::new(),
            scope: Scope {
                workspace: WorkspaceId::parse(workspace)?,
                session: SessionId::parse("quality-session")?,
                task: TaskId::parse(&doc.id)?,
            },
            root: RootId::parse(format!("{workspace}-root"))?,
            paths: vec![format!("src/{}.rs", doc.id)],
            symbols: vec![symbol.into()],
            kind: SearchKind::Source,
            claim_kind: None,
            source,
            source_digest: vcp_protocol::digest_bytes(doc.text.as_bytes()),
            span: Range {
                start: ByteCount::ZERO,
                end: ByteCount::new(doc.text.len() as u64),
            },
            applicability: None,
            status: Outcome::Accepted,
            evidence_status: EvidenceStatus::Observed,
            memory_seq: MemorySeq::ZERO,
            watermark: Watermark::new(1),
            text: doc.text.clone(),
        };
        record.id = record.calculate_id(&chunker)?;
        records.push(record);
    }
    records.sort_by(|a, b| a.id.cmp(&b.id));
    let mut result = Inventory {
        workspace: WorkspaceId::parse(workspace)?,
        watermark: Watermark::new(1),
        authority: AuthorityRevision::ZERO,
        deletion: DeletionEpoch::ZERO,
        chunker_digest: chunker,
        records,
        exclusions,
        digest: String::new(),
    };
    result.digest = result.calculate_digest()?;
    result.validate()?;
    Ok(result)
}
fn query(workspace: WorkspaceId, text: String) -> Query {
    Query {
        workspace,
        tasks: None,
        roots: None,
        paths: None,
        symbols: None,
        kind: Some(SearchKind::Source),
        claim_kind: None,
        status: Some(Outcome::Accepted),
        text,
        phrase: false,
        limit: TOP_K,
    }
}
fn source_ids(inventory: &Inventory, candidates: &[lexical::Candidate]) -> Result<Vec<String>> {
    candidates
        .iter()
        .map(|candidate| {
            let record = inventory
                .records
                .iter()
                .find(|record| record.id == candidate.id)
                .ok_or("candidate outside inventory")?;
            match &record.source {
                TextSource::Artifact { id } => Ok(id.to_string()),
                _ => Err("fixture source type changed".into()),
            }
        })
        .collect()
}
fn metrics(actual: &[String], expected: &[String]) -> Value {
    if expected.is_empty() {
        return Value::Null;
    }
    let expected: BTreeSet<_> = expected.iter().collect();
    let hits = actual.iter().filter(|id| expected.contains(id)).count();
    json!({"hits":hits,"expected":expected.len(),"returned":actual.len(),
        "precision_at_3":hits as f64 / TOP_K as f64,
        "precision_among_returned":hits as f64 / actual.len().max(1) as f64,
        "recall_at_3":hits as f64 / expected.len() as f64})
}

fn run() -> Result<Value> {
    let corpus = parse()?;
    let temp = tempfile::tempdir()?;
    let workspaces: BTreeSet<_> = corpus
        .documents
        .iter()
        .map(|doc| doc.workspace.as_str())
        .collect();
    let mut rows = Vec::new();
    let mut components = Vec::new();
    let mut checks = Vec::new();
    let mut quality_pass = true;
    for workspace in &workspaces {
        let inventory = inventory(&corpus, workspace)?;
        let directory = temp.path().join(workspace);
        std::fs::create_dir(&directory)?;
        let start = Instant::now();
        let reader = lexical::build(
            &directory,
            &inventory,
            Limits::default(),
            &AtomicBool::new(false),
        )?;
        let build_us = start.elapsed().as_micros();
        components.push(json!({"workspace":workspace,"documents":reader.len(),"excluded":inventory.exclusions.len(),"inventory_digest":inventory.digest,"build_us":build_us}));
        drop(reader);
        for case in corpus
            .queries
            .iter()
            .filter(|case| case.workspace == *workspace)
        {
            let open_start = Instant::now();
            let reader = lexical::open(&directory, &inventory, Limits::default())?;
            let open_us = open_start.elapsed().as_micros();
            let query = query(inventory.workspace.clone(), case.text.clone());
            let mut reference = None;
            for repetition in 0..=WARM_REPETITIONS {
                let start = Instant::now();
                let candidates = reader.search(&query)?;
                let query_us = start.elapsed().as_micros();
                let actual = source_ids(&inventory, &candidates)?;
                if actual.iter().any(|id| {
                    !corpus
                        .documents
                        .iter()
                        .any(|doc| doc.id == *id && doc.workspace == *workspace && doc.current)
                }) {
                    return Err("scope or supersession leak".into());
                }
                if let Some(reference) = &reference {
                    if reference != &actual {
                        return Err("warm query result drift".into());
                    }
                } else {
                    reference = Some(actual.clone());
                }
                if case.lexical_required.iter().any(|id| !actual.contains(id)) {
                    quality_pass = false;
                }
                rows.push(json!({"query":case.id,"workspace":workspace,"temperature":if repetition==0 {"fresh_reader"} else {"warm_reader"},"repetition":repetition,
                    "open_and_component_validation_us":if repetition==0 {Some(open_us)} else {None},"query_us":query_us,"source_ids":actual,"candidates":candidates,
                    "lexical_required":case.lexical_required,"lexical":metrics(&actual,&case.lexical_required),
                    "semantic_label_diagnostic_for_lexical_only":metrics(&actual,&case.semantic_required)}));
            }
            // Scope controls are measured separately, never used to improve quality scores.
            if let Some(expected) = case.lexical_required.first() {
                let record = inventory.records.iter().find(|r| matches!(&r.source, TextSource::Artifact { id } if id.as_str()==expected)).ok_or("missing scope target")?;
                let mut scoped = query.clone();
                scoped.tasks = Some(vec![record.scope.task.clone()]);
                scoped.roots = Some(vec![record.root.clone()]);
                scoped.paths = Some(record.paths.clone());
                scoped.symbols = Some(record.symbols.clone());
                scoped.limit = 1;
                if source_ids(&inventory, &reader.search(&scoped)?)? != vec![expected.clone()] {
                    return Err("exact scope filter failed".into());
                }
                scoped.tasks = Some(vec![]);
                if !reader.search(&scoped)?.is_empty() {
                    return Err("empty task permission expanded scope".into());
                }
                let foreign = workspaces
                    .iter()
                    .find(|other| **other != *workspace)
                    .ok_or("multiple workspace fixture required")?;
                scoped.workspace = WorkspaceId::parse(*foreign)?;
                if reader.search(&scoped).is_ok() {
                    return Err("foreign workspace accepted".into());
                }
                checks.push(json!({"query":case.id,"scope_filters_before_limit":"pass","empty_task_scope":"pass","foreign_workspace":"rejected"}));
            }
        }
    }
    let mut means = BTreeMap::new();
    for temperature in ["fresh_reader", "warm_reader"] {
        let scored: Vec<_> = rows
            .iter()
            .filter(|row| row["temperature"] == temperature && !row["lexical"].is_null())
            .collect();
        if scored.is_empty() {
            return Err("no lexical labels measured".into());
        }
        let average = |field: &str| {
            scored
                .iter()
                .map(|row| row["lexical"][field].as_f64().unwrap_or(0.0))
                .sum::<f64>()
                / scored.len() as f64
        };
        means.insert(temperature, json!({"scored_rows":scored.len(),"macro_precision_at_3":average("precision_at_3"),"macro_precision_among_returned":average("precision_among_returned"),"macro_recall_at_3":average("recall_at_3")}));
    }
    Ok(
        json!({"status":if quality_pass {"pass"} else {"quality_failed"},"schema_version":1,"fixture_sha256":vcp_protocol::digest_bytes(CORPUS),"retrieval":"lexical_only",
        "schema":lexical::SCHEMA_VERSION,"tokenizer":vcp_memory::tokenizer::TOKENIZER_VERSION,"exact_boost":lexical::EXACT_BOOST,"code_boost":lexical::CODE_BOOST,
        "top_k":TOP_K,"warm_repetitions":WARM_REPETITIONS,"components":components,"queries":rows,"lexical_summary":means,"scope_checks":checks,
        "limitations":["Synthetic previously published held-out query fixture; not a new blind benchmark or production-quality estimate.","Fresh-reader timing reopens and validates the component; OS page-cache coldness is neither enforced nor claimed.","Only two queries have lexical-required labels. Five semantic-required queries are diagnostic lexical observations, not ANN or fusion measurements.","Paths, symbols and task scopes are deterministic fixture bindings; no canonical authorization or full generation activation campaign is claimed.","Timings are this process/build/host only; source IDs are public fixture identifiers."]}),
    )
}
fn main() {
    match run() {
        Ok(report) => {
            let passed = report["status"] == "pass";
            println!("{report}");
            if !passed {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("lexical quality qualification failed: {error}");
            std::process::exit(1);
        }
    }
}
