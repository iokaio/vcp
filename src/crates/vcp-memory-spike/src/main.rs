// SPDX-License-Identifier: Apache-2.0
//! P0-02 bounded local-corpus experiment; not a production memory service.
//! Calls the retained Munarium datastore API at 8da666067000ca1ee9c131bc67e70b978862faa3.
use munarium_datastore::{
    lexical::{self, LexicalPlan, PlanTerm},
    model::*,
    shard::{OpenShard, ShardWriter, BUILD_SPEC},
    store::{ArtifactStore, LocalFileStore},
    vector::Candidate,
    vector_diskann::{self, GraphParams},
    verify::{Limits, ReaderCapabilities},
    PreparedChunk,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::Path,
    time::Instant,
};
use vcp_embedding::{MiniLm, DIMENSIONS, MAX_BATCH};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const CORPUS: &str = include_str!("../../../tests/fixtures/local-memory/corpus.json");
const TOP_K: usize = 3;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    id: String,
    workspace: String,
    current: bool,
    text: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Query {
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
    queries: Vec<Query>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    workspace: String,
    artifact_id: String,
    logical_id: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    version: u32,
    corpus_sha256: String,
    model_spec_sha256: String,
    bindings: Vec<Binding>,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn corpus() -> Result<Corpus> {
    let corpus: Corpus = serde_json::from_str(CORPUS)?;
    if corpus.version != 1
        || corpus.documents.is_empty()
        || corpus.documents.len() > 256
        || corpus.queries.is_empty()
    {
        return Err("invalid bounded corpus".into());
    }
    let mut ids = BTreeSet::new();
    for document in &corpus.documents {
        if !token(&document.id)
            || !token(&document.workspace)
            || !ids.insert(&document.id)
            || document.text.is_empty()
            || document.text.len() > 4096
        {
            return Err("invalid corpus document".into());
        }
    }
    let mut queries = BTreeSet::new();
    for query in &corpus.queries {
        if !token(&query.id)
            || !queries.insert(&query.id)
            || query.text.is_empty()
            || query.text.len() > 4096
            || query.lexical_required.len() > TOP_K
            || query.semantic_required.len() > TOP_K
        {
            return Err("invalid corpus query".into());
        }
        for expected in query
            .lexical_required
            .iter()
            .chain(&query.semantic_required)
        {
            if !corpus
                .documents
                .iter()
                .any(|d| d.id == *expected && d.workspace == query.workspace && d.current)
            {
                return Err("query truth contains an unavailable document".into());
            }
        }
        if !corpus
            .documents
            .iter()
            .any(|d| d.workspace == query.workspace)
        {
            return Err("unknown query workspace".into());
        }
    }
    Ok(corpus)
}
fn embed(model: &MiniLm, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
    let mut vectors = Vec::with_capacity(texts.len());
    for batch in texts.chunks(MAX_BATCH) {
        for row in model.embed(batch)? {
            if row.truncated {
                return Err("qualification corpus was truncated".into());
            }
            vectors.push(row.vector);
        }
    }
    Ok(vectors)
}
fn specification(workspace: &str, documents: &[&Document]) -> BuildSpec {
    BuildSpec {
        spec_version: 1,
        scope: Scope {
            kind: ScopeKind::Collection,
            id: workspace.into(),
        },
        sources: documents
            .iter()
            .map(|d| SourceRef {
                source_id: d.id.clone(),
                logical_path: format!("{}.txt", d.id),
                media_type: "text/plain".into(),
                content_sha256: digest(d.text.as_bytes()),
                revision: Some(d.id.clone()),
            })
            .collect(),
        snapshot: Snapshot { watermark_seq: 1 },
        shape: ShapeRef {
            shape_ref: "vcp-synthetic-document".into(),
            version: 1,
        },
        chunker: Chunker {
            name: "whole-document".into(),
            version: "1".into(),
            params: BTreeMap::new(),
        },
        extractor: Extractor {
            name: "vcp-fixture".into(),
            version: "1".into(),
            config: BTreeMap::new(),
            per_source: documents
                .iter()
                .map(|d| ExtractionOutcome {
                    source_id: d.id.clone(),
                    outcome: ExtractionStatus::Extracted,
                    extracted_text_sha256: Some(digest(d.text.as_bytes())),
                    method: Some("literal-text".into()),
                })
                .collect(),
        },
        embedder: Some(Embedder {
            model: format!("minilm:{}", MiniLm::specification_sha256()),
            dimensions: DIMENSIONS as u32,
            normalization: Normalization::L2,
            metric: Metric::Cosine,
        }),
        lexical_analysis: LexicalAnalysis {
            contract_version: lexical::ANALYZER_CONTRACT_VERSION,
            tokenizer: lexical::TOKENIZER_ID.into(),
            stemmer: "snowball-english".into(),
            stop_terms_ref: StopTerms {
                list_ref: "pg16/english".into(),
                sha256: lexical::stop_terms_sha256(),
            },
            index_options: IndexOptions {
                positions: true,
                case_folding: Some("lowercase".into()),
                accent_folding: Some("none".into()),
            },
        },
        reconstructed: false,
    }
}
fn plan() -> ArtifactBuildPlan {
    ArtifactBuildPlan { plan_version: 1,
        envelope: Envelope { format_version: 1, feature_bits: vec!["records.v1".into(), vector_diskann::FEATURE_BIT.into()] },
        lexical: LexicalEngine { engine_id: "tantivy".into(), engine_revision: lexical::engine_revision().into(),
            positions: true, segments: Some(1), compression: None },
        vector: Some(VectorEngine { engine_id: vector_diskann::ENGINE_ID.into(), engine_revision: vector_diskann::ENGINE_REVISION.into(),
            kind: VectorKind::Approximate, quantization: None, graph: Some(GraphParams::default().to_plan_map()), rescore_depth: None }),
        records: RecordsFormat { format: "munarium-records@1".into(), compression: None }, range_map: None,
        shaper: Shaper { policy_version: 1, decisions: vec![ShaperDecision { setting: "vector.engine".into(),
            chosen: Param::Text("diskann".into()), because: "Explicit P0-02 approximate-index qualification, regardless of small corpus size".into(),
            threshold: None, observed: None }] },
    }
}
fn build(assets: &Path, root: &Path) -> Result<serde_json::Value> {
    let corpus = corpus()?;
    let started = Instant::now();
    let model = MiniLm::load(assets)?;
    let load_ms = started.elapsed().as_millis();
    // Exclusive root creation preserves any previous output, including failed runs.
    fs::create_dir(root)?;
    let workspaces: BTreeSet<_> = corpus
        .documents
        .iter()
        .map(|d| d.workspace.as_str())
        .collect();
    let mut bindings = Vec::new();
    let mut timings = Vec::new();
    for workspace in workspaces {
        let documents: Vec<_> = corpus
            .documents
            .iter()
            .filter(|d| d.workspace == workspace)
            .collect();
        let texts: Vec<_> = documents.iter().map(|d| d.text.as_str()).collect();
        let inference = Instant::now();
        let vectors = embed(&model, &texts)?;
        let inference_ms = inference.elapsed().as_millis();
        let index_start = Instant::now();
        let store = LocalFileStore::new(root.join(workspace))?;
        let mut writer = ShardWriter::new(Some(DIMENSIONS));
        for (ordinal, (document, vector)) in documents.iter().zip(vectors).enumerate() {
            writer.add(PreparedChunk {
                chunk_id: document.id.clone(),
                source_id: document.id.clone(),
                source_path: format!("{}.txt", document.id),
                node_id: None,
                ordinal: ordinal as u32,
                text: document.text.clone(),
                text_sha256: Sha256::digest(document.text.as_bytes()).into(),
                embedding: Some(vector),
                metadata: BTreeMap::from([("workspace".into(), workspace.into())]),
            })?;
        }
        let spec = specification(workspace, &documents);
        let sealed = writer.seal(&spec, &plan(), &store)?;
        sealed.publish_manifest(&store)?;
        bindings.push(Binding {
            workspace: workspace.into(),
            artifact_id: sealed.artifact_id,
            logical_id: spec.index_version_id()?,
        });
        timings.push(serde_json::json!({"workspace":workspace,"documents":documents.len(),"inference_ms":inference_ms,"index_ms":index_start.elapsed().as_millis()}));
    }
    let receipt = Receipt {
        version: 1,
        corpus_sha256: digest(CORPUS.as_bytes()),
        model_spec_sha256: MiniLm::specification_sha256(),
        bindings,
    };
    let bytes = serde_json::to_vec(&receipt)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("receipt.json"))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(
        serde_json::json!({"status":"pass","phase":"build","documents":corpus.documents.len(),"load_ms":load_ms,
        "receipt_sha256":digest(&bytes),"timings":timings,"model_spec_sha256":receipt.model_spec_sha256,
        "corpus_sha256":receipt.corpus_sha256,"dimensions":DIMENSIONS,"metric":"cosine","vector_engine":"diskann","lexical_engine":"tantivy"}),
    )
}
fn read_receipt(root: &Path, expected_sha: &str) -> Result<Receipt> {
    let mut bytes = Vec::new();
    fs::File::open(root.join("receipt.json"))?
        .take(65537)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 65536 || digest(&bytes) != expected_sha {
        return Err("receipt identity mismatch".into());
    }
    let receipt: Receipt = serde_json::from_slice(&bytes)?;
    if receipt.version != 1
        || receipt.corpus_sha256 != digest(CORPUS.as_bytes())
        || receipt.model_spec_sha256 != MiniLm::specification_sha256()
    {
        return Err("corpus or embedding identity changed".into());
    }
    let mut workspaces = BTreeSet::new();
    for binding in &receipt.bindings {
        if !token(&binding.workspace) || !workspaces.insert(&binding.workspace) {
            return Err("invalid workspace binding".into());
        }
    }
    Ok(receipt)
}
fn open(root: &Path, workspace: &str, binding: &Binding) -> Result<OpenShard> {
    if workspace != binding.workspace {
        return Err("workspace binding mismatch".into());
    }
    let store = LocalFileStore::new(root.join(workspace))?;
    let limits = Limits {
        max_components: 16,
        max_component_bytes: 64 * 1024 * 1024,
        max_total_bytes: 128 * 1024 * 1024,
        max_chunks: 256,
        max_dimensions: DIMENSIONS as u32,
    };
    let shard = OpenShard::open(
        &store,
        &binding.artifact_id,
        &ReaderCapabilities::v1(),
        &limits,
    )?;
    let spec: BuildSpec = serde_json::from_slice(&store.get_component(BUILD_SPEC, None)?)?;
    let fixture = corpus()?;
    let documents: Vec<_> = fixture
        .documents
        .iter()
        .filter(|d| d.workspace == workspace)
        .collect();
    if spec != specification(workspace, &documents)
        || spec.index_version_id()? != binding.logical_id
    {
        return Err("artifact scope or model shape mismatch".into());
    }
    for document in documents {
        let record = shard
            .record(&document.id)
            .ok_or("missing hydrated record")?;
        if record.text != document.text
            || record.text_sha256 != digest(document.text.as_bytes())
            || record.metadata.get("workspace").map(String::as_str) != Some(workspace)
        {
            return Err("hydrated content or provenance mismatch".into());
        }
    }
    Ok(shard)
}
// This fixed corpus is the experiment's canonical view. Production scope,
// revisions and tombstones must come from the canonical store, not index metadata.
fn visible(corpus: &Corpus, workspace: &str, candidates: &[Candidate]) -> Result<Vec<String>> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for candidate in candidates {
        let document = corpus
            .documents
            .iter()
            .find(|d| d.id == candidate.chunk_id)
            .ok_or("unknown indexed record")?;
        if !candidate.score.is_finite() {
            return Err("non-finite retrieval score".into());
        }
        if document.workspace == workspace && document.current && seen.insert(&document.id) {
            result.push(document.id.clone());
        }
    }
    result.truncate(TOP_K);
    Ok(result)
}
// Independent f64 exhaustive cosine oracle; does not call either index's scoring.
fn exact(query: &[f32], rows: &[(&Document, Vec<f32>)]) -> Vec<String> {
    let norm = |v: &[f32]| v.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    let mut scores: Vec<_> = rows
        .iter()
        .map(|(d, v)| {
            let dot: f64 = query
                .iter()
                .zip(v)
                .map(|(a, b)| *a as f64 * *b as f64)
                .sum();
            (d.id.clone(), 1. - dot / (norm(query) * norm(v)))
        })
        .collect();
    scores.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    scores.into_iter().take(TOP_K).map(|(id, _)| id).collect()
}
fn query(assets: &Path, root: &Path, receipt_sha: &str) -> Result<serde_json::Value> {
    let corpus = corpus()?;
    let receipt = read_receipt(root, receipt_sha)?;
    let started = Instant::now();
    let model = MiniLm::load(assets)?;
    let load_ms = started.elapsed().as_millis();
    let mut results = Vec::new();
    for binding in &receipt.bindings {
        let start = Instant::now();
        let shard = open(root, &binding.workspace, binding)?;
        let reopen_ms = start.elapsed().as_millis();
        if shard.vector_candidates(&[0.; 3], 1).is_ok() {
            return Err("wrong vector dimensions were accepted".into());
        }
        if open(root, "wrong-workspace", binding).is_ok() {
            return Err("cross-workspace binding was accepted".into());
        }
        let documents: Vec<_> = corpus
            .documents
            .iter()
            .filter(|d| d.workspace == binding.workspace)
            .collect();
        if shard.records().len() != documents.len() {
            return Err("reopened document count mismatch".into());
        }
        let vectors = embed(
            &model,
            &documents
                .iter()
                .map(|d| d.text.as_str())
                .collect::<Vec<_>>(),
        )?;
        let rows: Vec<_> = documents.iter().copied().zip(vectors).collect();
        for case in corpus
            .queries
            .iter()
            .filter(|q| q.workspace == binding.workspace)
        {
            let inference = Instant::now();
            let vector = embed(&model, &[&case.text])?.remove(0);
            let inference_us = inference.elapsed().as_micros();
            let lookup = Instant::now();
            let plan = LexicalPlan {
                terms: shard
                    .analyze(&case.text)?
                    .into_iter()
                    .map(PlanTerm::user)
                    .collect(),
                minimum_should_match: 1,
                ..Default::default()
            };
            // Bounded experiment: scan the candidate set before canonical filtering
            // so an old version cannot consume the final result budget.
            let lexical = visible(
                &corpus,
                &binding.workspace,
                &shard.lexical_candidates(&plan, documents.len())?,
            )?;
            let semantic = visible(
                &corpus,
                &binding.workspace,
                &shard.vector_candidates(&vector, documents.len())?,
            )?;
            let raw_ann: Vec<_> = shard
                .vector_candidates(&vector, TOP_K)?
                .into_iter()
                .map(|c| c.chunk_id)
                .collect();
            let query_us = lookup.elapsed().as_micros();
            let truth = exact(&vector, &rows);
            let recall =
                truth.iter().filter(|id| raw_ann.contains(id)).count() as f64 / truth.len() as f64;
            if recall < 1. {
                return Err(
                    format!("exact-oracle recall below 1 for {}: {recall}", case.id).into(),
                );
            }
            if !case.lexical_required.iter().all(|id| lexical.contains(id))
                || !case
                    .semantic_required
                    .iter()
                    .all(|id| semantic.contains(id))
            {
                return Err(format!("required relevance missing for {}", case.id).into());
            }
            results.push(serde_json::json!({"case":case.id,"workspace":binding.workspace,"lexical":lexical,"semantic":semantic,
                "ann_recall_at_3":recall,"reopen_ms":reopen_ms,"inference_us":inference_us,"query_us":query_us}));
        }
    }
    if results.len() != corpus.queries.len() {
        return Err("incomplete query result set".into());
    }
    Ok(
        serde_json::json!({"status":"pass","phase":"query","queries":results,"load_ms":load_ms,
        "checks":["lexical_identifiers","semantic_relevance","exact_vector_oracle","workspace_binding","current_version_filter","dimension_rejection","process_reopen"],
        "limitations":["Bounded synthetic fixture; not production recall or resource qualification.","No OS network-denial claim.","Fixture canonical view; no durable VCP governance store."]}),
    )
}
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let result = match args.as_slice() {
        [mode, assets, root] if mode == "build" => build(Path::new(assets), Path::new(root)),
        [mode, assets, root, sha] if mode == "query" => {
            query(Path::new(assets), Path::new(root), &sha.to_string_lossy())
        }
        _ => {
            eprintln!("Required: vcp-memory-spike build <assets> <new-root> | query <assets> <root> <receipt-sha256>");
            std::process::exit(2);
        }
    };
    match result {
        Ok(result) => println!("{result}"),
        Err(error) => {
            let code = if matches!(
                error.downcast_ref::<vcp_embedding::Error>(),
                Some(vcp_embedding::Error::MissingAsset { .. })
            ) {
                3
            } else {
                1
            };
            eprintln!("{error}");
            std::process::exit(code);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_filter_excludes_other_workspace_and_superseded_content_before_limit() {
        let corpus = corpus().unwrap();
        let candidates: Vec<_> = [
            "boreal-pause",
            "atlas-pause-v1",
            "atlas-pause-v2",
            "atlas-budget",
            "atlas-vault",
            "atlas-patch",
        ]
        .into_iter()
        .map(|id| Candidate {
            chunk_id: id.into(),
            score: 0.5,
        })
        .collect();
        assert_eq!(
            visible(&corpus, "atlas", &candidates).unwrap(),
            ["atlas-pause-v2", "atlas-budget", "atlas-vault"]
        );
        assert_eq!(
            visible(&corpus, "boreal", &candidates).unwrap(),
            ["boreal-pause"]
        );
        assert!(visible(
            &corpus,
            "atlas",
            &[Candidate {
                chunk_id: "unknown".into(),
                score: 0.5
            }]
        )
        .is_err());
        assert!(visible(
            &corpus,
            "atlas",
            &[Candidate {
                chunk_id: "atlas-budget".into(),
                score: f32::NAN
            }]
        )
        .is_err());
    }

    #[test]
    fn exact_oracle_uses_cosine_and_stable_id_order() {
        let document = |id: &str| Document {
            id: id.into(),
            workspace: "test".into(),
            current: true,
            text: "synthetic".into(),
        };
        let a = document("a");
        let b = document("b");
        let c = document("c");
        let d = document("d");
        let rows = vec![
            (&d, vec![-1., 0.]),
            (&c, vec![0., 1.]),
            (&b, vec![2., 0.]),
            (&a, vec![1., 0.]),
        ];
        assert_eq!(exact(&[1., 0.], &rows), ["a", "b", "c"]);
    }

    #[test]
    fn receipt_and_workspace_rejection_precede_index_access() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("receipt.json"), b"{}").unwrap();
        assert!(read_receipt(root.path(), &"0".repeat(64)).is_err());
        let binding = Binding {
            workspace: "atlas".into(),
            artifact_id: "0".repeat(64),
            logical_id: "synthetic".into(),
        };
        assert!(open(root.path(), "boreal", &binding).is_err());
        assert!(!root.path().join("boreal").exists());
    }
}
