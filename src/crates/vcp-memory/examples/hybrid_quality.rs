// SPDX-License-Identifier: Apache-2.0
//! Composite native qualification: denied-network inference, trusted canonical owner.
//! Corpus/truth are the exact shared lexical/vector fixture, never retuned here.
#[path = "../../vcp-embedding/src/bin/qualify/network.rs"]
mod network;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
    sync::atomic::AtomicBool,
    time::Instant,
};
use vcp_domain::{artifact::*, task::Objective, verification::Fingerprint, workspace::*, *};
use vcp_engine::{Engine, HostFacts};
use vcp_memory::{
    access::Access,
    embedding::{self, Encoded, LocalEmbedding, Specification},
    lexical,
    local_resources::{Admission, Limits as ResourceLimits, Workload},
    publication::{self, Publisher},
    retrieval::{self, QueryVector, Request},
    search_record::{self, ChunkerSpec, SourceBinding},
    vector,
};
use vcp_protocol::{
    canonical_bytes,
    command::{Command, CommandEnvelope},
    digest_bytes,
};
use vcp_store::{artifact::ArtifactWriter, BackendKind, Store};
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
fn owner(scope: &Scope) -> vcp_engine::Access {
    vcp_engine::Access {
        workspace: scope.workspace.clone(),
        session: scope.session.clone(),
        actor: ActorId::parse("owner").unwrap(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: true,
    }
}
async fn issue(engine: &mut Engine<Store>, scope: &Scope, payload: Command) -> Result<()> {
    let envelope = CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: scope.workspace.clone(),
        session: scope.session.clone(),
        task: if matches!(&payload, Command::Initialize { .. }) {
            None
        } else {
            Some(scope.task.clone())
        },
        caller: owner(scope).actor,
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        payload,
    };
    engine
        .handle(
            envelope,
            &owner(scope),
            &HostFacts::inspect(Timestamp::new(100)),
        )
        .await?;
    Ok(())
}
async fn artifact(
    engine: &mut Engine<Store>,
    scope: &Scope,
    schema: &str,
    bytes: &[u8],
) -> Result<ArtifactDescriptor> {
    let mut writer = engine.store().spool().create(ArtifactSpec {
        id: ArtifactId::new(),
        scope: scope.clone(),
        media_type: "application/octet-stream".into(),
        schema: schema.into(),
        source: "hybrid-quality-fixture".into(),
        channel: Channel::Evidence,
        retention: "history".into(),
        omissions: vec![Omission::AuthenticationHeaders, Omission::RecoveryMaterial],
    })?;
    writer.write_chunk(bytes)?;
    let descriptor = writer.finalize()?;
    issue(
        engine,
        scope,
        Command::AttachArtifact {
            descriptor: descriptor.clone(),
        },
    )
    .await?;
    Ok(descriptor)
}
async fn fixture(
    path: &Path,
    backend: BackendKind,
    corpus: &Corpus,
    workspace: &str,
) -> Result<(Store, Scope, Access, Vec<SourceBinding>)> {
    let mut engine = Engine::new(Store::open(path, backend, &[]).await?)?;
    let mut scope = Scope {
        workspace: WorkspaceId::parse(workspace)?,
        session: SessionId::parse("hybrid-fixture")?,
        task: TaskId::parse("fixture-placeholder")?,
    };
    issue(
        &mut engine,
        &scope,
        Command::Initialize {
            binding: Binding {
                host: HostId::new(),
                root: "C:/hybrid-quality-fixture".into(),
                repository: "fixture".into(),
                worktree: "main".into(),
                revision: Revision::ZERO,
            },
        },
    )
    .await?;
    let root = RootId::parse("workspace")?;
    let mut sources = Vec::new();
    let mut publish_scope = None;
    for doc in corpus.documents.iter().filter(|d| d.workspace == workspace) {
        scope.task = TaskId::parse(&doc.id)?;
        let bytes = doc.text.as_bytes();
        // Neutral identical path avoids adding query-relevant identifiers absent
        // from the shared corpus; source identities remain separate task IDs.
        let manifest = json!({"identity":{"workspace":scope.workspace,"root":root,"repository":"fixture","worktree":"main","binding":Revision::ZERO},"bounded_scan_complete":true,"files":[{"root":root,"path":"fixture.txt","sha256":digest_bytes(bytes),"bytes":ByteCount::new(bytes.len() as u64)}]});
        let fingerprint = Fingerprint {
            repository: digest_bytes(&canonical_bytes(&manifest)?),
            buffers: "b".repeat(64),
            environment: "c".repeat(64),
        };
        issue(
            &mut engine,
            &scope,
            Command::CreateTask {
                root: scope.task.clone(),
                parent: None,
                fork_origin: None,
                objective: Objective {
                    text: "Retain unchanged labelled source fixture".into(),
                    constraints: vec![],
                    acceptance: vec!["exact source bytes".into()],
                    source: EventId::new(),
                    steering: SteeringRevision::ZERO,
                },
                fingerprint: fingerprint.clone(),
                editing: false,
                required_checks: vec![],
            },
        )
        .await?;
        let source = artifact(&mut engine, &scope, "verification-source/1", bytes).await?;
        let plan = artifact(
            &mut engine,
            &scope,
            "verification-plan/1",
            &canonical_bytes(&json!({"before":manifest,"source_artifacts":[source.spec.id]}))?,
        )
        .await?;
        // Same current-doc selection as vector_quality; historical bytes remain
        // canonical but are explicitly not nominated as current source bindings.
        if doc.current {
            publish_scope.get_or_insert_with(|| scope.clone());
            sources.push(SourceBinding {
                manifest: plan.spec.id,
                artifact: source.spec.id,
                root: root.clone(),
                path: "fixture.txt".into(),
                symbols: vec![],
                fingerprint,
            });
        }
    }
    let access = Access {
        workspace: scope.workspace.clone(),
        actor: owner(&scope).actor,
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        tasks: None,
    };
    Ok((
        engine.into_store(),
        publish_scope.ok_or("empty workspace corpus")?,
        access,
        sources,
    ))
}
fn oracle(rows: &[Encoded], query: &[f32], allowed: &BTreeSet<String>) -> Vec<String> {
    let norm = |v: &[f32]| v.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>().sqrt();
    let mut scored: Vec<_> = rows
        .iter()
        .filter(|r| allowed.contains(&r.identity.id))
        .map(|r| {
            let dot = r
                .vector
                .iter()
                .zip(query)
                .map(|(a, b)| f64::from(*a) * f64::from(*b))
                .sum::<f64>();
            (
                1.0 - dot / (norm(&r.vector) * norm(query)),
                r.identity.id.clone(),
            )
        })
        .collect();
    scored.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    scored.into_iter().take(K).map(|(_, id)| id).collect()
}
fn recall(found: &[String], required: &[String]) -> Option<f64> {
    (!required.is_empty()).then(|| {
        required.iter().filter(|id| found.contains(id)).count() as f64 / required.len() as f64
    })
}
fn labels(found: &[String], case: &Case) -> Value {
    let union: Vec<_> = case
        .lexical_required
        .iter()
        .chain(&case.semantic_required)
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    json!({"source_ids":found,"lexical_label_recall_at_3":recall(found,&case.lexical_required),"semantic_label_recall_at_3":recall(found,&case.semantic_required),"union_label_recall_at_3":recall(found,&union)})
}
fn source_ids(
    ids: impl IntoIterator<Item = String>,
    mapping: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut out = Vec::new();
    for id in ids {
        if let Some(source) = mapping.get(&id) {
            if !out.contains(source) {
                out.push(source.clone());
            }
        }
        if out.len() == K {
            break;
        }
    }
    out
}
fn request(workspace: &WorkspaceId, text: &str) -> Request {
    Request {
        workspace: workspace.clone(),
        tasks: None,
        roots: None,
        paths: None,
        symbols: None,
        text: text.into(),
        historical: None,
        minimum_sequence: None,
        timeout_ms: 30_000,
        results: K,
        tokens: 16_384,
        bytes: 65_536,
    }
}
fn distribution(samples: &[u64]) -> Value {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    if sorted.is_empty() {
        return json!({"samples":0});
    }
    json!({"samples":sorted.len(),"min_us":sorted[0],"p50_us":sorted[(sorted.len()-1)/2],"p95_us":sorted[(sorted.len()-1)*95/100],"max_us":sorted[sorted.len()-1],"all_us":samples})
}
async fn qualify(bundle: Bundle, report: &mut Value) -> Result<()> {
    let corpus: Corpus = serde_json::from_slice(CORPUS)?;
    if corpus.version != 2 || corpus.documents.len() != 24 || corpus.queries.len() != 7 {
        return Err("shared corpus shape changed".into());
    }
    for doc in &corpus.documents {
        if let Some(old) = &doc.supersedes {
            if !corpus
                .documents
                .iter()
                .any(|d| &d.id == old && d.workspace == doc.workspace && !d.current)
            {
                return Err("invalid corpus supersession".into());
            }
        }
    }
    for case in &corpus.queries {
        for id in case.lexical_required.iter().chain(&case.semantic_required) {
            if !corpus
                .documents
                .iter()
                .any(|d| &d.id == id && d.workspace == case.workspace && d.current)
            {
                return Err("invalid shared truth".into());
            }
        }
    }
    let admission = Admission::new(ResourceLimits::default())?;
    let bytes: usize = corpus.documents.iter().map(|d| d.text.len()).sum();
    // Exact corpus is below this declared reservation; one permit remains live
    // through all model, vector readers, query embedding, and private builds.
    let permit = admission.acquire(Workload {
        rows: 1024,
        source_bytes: bytes,
        batch: 16,
        load_model: false,
    })?;
    report["declared_resources"] = json!({"estimate_version":vcp_memory::local_resources::ESTIMATE_VERSION,"ram_bytes":permit.estimate().ram_bytes,"temporary_disk_bytes":permit.estimate().temporary_disk_bytes});
    validate_bundle(&bundle, &corpus)?;
    report["boundary"] = json!({"canonical_owner":"trusted process; no model loading or inference", "embedding":"zero-capability AppContainer; separately observed network canaries", "handoff":"validated specification, exact source/content digests, scope and UTF-8 spans"});
    let temp = tempfile::tempdir()?;
    let chunker = ChunkerSpec::default();
    let spec = Specification::qualified();
    let spec_digest = spec.digest()?;
    report["specification"] = json!({"embedding":spec_digest,"assets":spec.assets,"chunker":chunker.digest()?,"fusion":retrieval::FUSION_VERSION});
    let workspaces: BTreeSet<_> = corpus
        .documents
        .iter()
        .map(|d| d.workspace.as_str())
        .collect();
    for (backend_name, backend) in [
        ("files", BackendKind::Files),
        ("sqlite", BackendKind::Sqlite),
    ] {
        for workspace in &workspaces {
            report["stage"] = json!({"backend":backend_name,"workspace":workspace,"operation":"canonical_fixture_and_publication"});
            let base = temp.path().join(format!("{backend_name}-{workspace}"));
            let (mut store, scope, access, bindings) =
                fixture(&base.join("canonical"), backend, &corpus, workspace).await?;
            let inventory = search_record::inventory(
                &store,
                &access,
                &bindings,
                &chunker,
                search_record::Limits::default(),
            )?;
            if !inventory.exclusions.is_empty() {
                return Err("fixture unexpectedly excluded source bytes".into());
            }
            let mapping: BTreeMap<_, _> = inventory
                .records
                .iter()
                .map(|r| (r.id.clone(), r.scope.task.to_string()))
                .collect();
            let publisher = Publisher::new(&base.join("generations"))?;
            let start = Instant::now();
            let chunks = embedding::inventory_chunks(&inventory)?;
            let mut rows = Vec::new();
            for chunk in chunks {
                let source = bundle
                    .rows
                    .iter()
                    .find(|row| {
                        row.identity.source == chunk.identity.scope.task.as_str()
                            && row.identity.scope == chunk.identity.scope
                            && row.identity.source_digest == chunk.identity.source_digest
                            && row.identity.start == chunk.identity.start
                            && row.identity.end == chunk.identity.end
                            && row.identity.content_digest == chunk.identity.content_digest
                            && row.identity.specification == chunk.identity.specification
                    })
                    .ok_or("export does not cover exact canonical source chunk")?;
                rows.push(Encoded {
                    identity: chunk.identity,
                    vector: source.vector.clone(),
                });
            }
            let component =
                vector::Component::build(access.workspace.clone(), spec.clone(), rows, &|| false)?;
            let vector_path = base.join("admitted-vectors.json");
            let checksum = component.save_private(&vector_path, &|| false)?;
            drop(component);
            let prepared = publisher.prepare_with_vectors(
                publication::capture(&store, &access, &scope, inventory)?,
                &vector_path,
                &checksum,
                &AtomicBool::new(false),
                &|_| {},
            )?;
            let receipt = publisher
                .publish(&mut store, &access, &prepared, Timestamp::new(200), &|_| {})
                .await?;
            let generation_disk = vcp_memory::local_resources::temporary_disk_bytes(temp.path())?;
            permit.check_temporary_disk(generation_disk)?;
            report["builds"].as_array_mut().unwrap().push(json!({"backend":backend_name,"workspace":workspace,"elapsed_us":start.elapsed().as_micros(),"manifest":prepared.manifest(),"receipt":receipt,"owned_temporary_disk_bytes":generation_disk}));
            let mut cold = Vec::new();
            let mut warm = Vec::new();
            for case in corpus.queries.iter().filter(|q| q.workspace == *workspace) {
                report["stage"] = json!({"backend":backend_name,"workspace":workspace,"query":case.id,"operation":"query"});
                let exported = bundle
                    .queries
                    .iter()
                    .find(|row| row.id == case.id)
                    .ok_or("query embedding absent")?;
                let vector = &exported.vector;
                let inference_us = exported.inference_us;
                for repetition in 0..3 {
                    // Cold reader means close/reopen with OS caches uncontrolled;
                    // it is deliberately not reported as cold filesystem latency.
                    let start = Instant::now();
                    let recovered = publisher.recover(&store, &access)?;
                    let view = recovered.view.ok_or("published generation unavailable")?;
                    cold.push(start.elapsed().as_micros() as u64);
                    let component = view.vector.as_ref().ok_or("full fixture lost vectors")?;
                    let allowed = component
                        .rows()
                        .iter()
                        .map(|r| r.identity.id.clone())
                        .collect();
                    let oracle_ids = oracle(component.rows(), &vector, &allowed);
                    let vector_start = Instant::now();
                    let ann =
                        component.query(&access.workspace, &allowed, &vector, K, &|| false)?;
                    let vector_us = vector_start.elapsed().as_micros();
                    let ann_ids: Vec<_> = ann.rows.iter().map(|r| r.chunk.clone()).collect();
                    let ann_recall = recall(&ann_ids, &oracle_ids).ok_or("empty oracle")?;
                    let lexical_start = Instant::now();
                    let lexical = view.lexical.search(&lexical::Query {
                        workspace: access.workspace.clone(),
                        tasks: None,
                        roots: None,
                        paths: None,
                        symbols: None,
                        kind: None,
                        claim_kind: None,
                        status: None,
                        text: case.text.clone(),
                        phrase: false,
                        limit: 100,
                    })?;
                    let lexical_us = lexical_start.elapsed().as_micros();
                    let lexical_ids = source_ids(lexical.into_iter().map(|r| r.id), &mapping);
                    let vector_ids =
                        source_ids(ann.rows.iter().map(|r| r.source.clone()), &mapping);
                    for warm_repetition in 0..3 {
                        let start = Instant::now();
                        let response = retrieval::query(
                            &store,
                            &access,
                            Some(&view),
                            &request(&access.workspace, &case.text),
                            &bindings,
                            &chunker,
                            Some(QueryVector {
                                specification: &spec_digest,
                                values: &vector,
                            }),
                            &|| false,
                        )?;
                        let elapsed = start.elapsed().as_micros() as u64;
                        warm.push(elapsed);
                        retrieval::revalidate_fence(
                            &store,
                            &access,
                            response.fence.as_ref().ok_or("missing source fence")?,
                        )?;
                        let fused: Vec<_> = response
                            .passages
                            .iter()
                            .map(|p| p.scope.task.to_string())
                            .collect();
                        let authorized = response.passages.iter().all(|p| {
                            p.scope.workspace == access.workspace
                                && corpus.documents.iter().any(|d| {
                                    d.id == p.scope.task.as_str()
                                        && d.current
                                        && d.workspace == case.workspace
                                })
                        });
                        report["queries"].as_array_mut().unwrap().push(json!({"backend":backend_name,"query":case.id,"reader_repetition":repetition,"warm_repetition":warm_repetition,"inference_us":inference_us,"hybrid_us":elapsed,"lexical_us":lexical_us,"vector_us":vector_us,"lexical":labels(&lexical_ids,case),"vector":labels(&vector_ids,case),"fused":labels(&fused,case),"ann_mode":format!("{:?}",ann.mode),"ann_oracle_recall_at_3":ann_recall,"ann_chunk_ids":ann_ids,"oracle_chunk_ids":oracle_ids,"authorized":authorized,"degraded":response.degraded}));
                        if !authorized || ann_recall < 1.0 || ann.mode != vector::Mode::Ann {
                            return Err(
                                "authorization or declared ANN oracle criterion failed".into()
                            );
                        }
                    }
                    let wanted = case
                        .semantic_required
                        .first()
                        .or(case.lexical_required.first())
                        .ok_or("missing narrow label")?;
                    let narrow = Access {
                        workspace: access.workspace.clone(),
                        actor: access.actor.clone(),
                        authority: access.authority,
                        read: true,
                        write: false,
                        tasks: Some(BTreeSet::from([TaskId::parse(wanted)?])),
                    };
                    let response = retrieval::query(
                        &store,
                        &narrow,
                        Some(&view),
                        &request(&access.workspace, &case.text),
                        &bindings,
                        &chunker,
                        Some(QueryVector {
                            specification: &spec_digest,
                            values: &vector,
                        }),
                        &|| false,
                    )?;
                    let narrow_ok = !response.passages.is_empty()
                        && response
                            .passages
                            .iter()
                            .all(|p| p.scope.task.as_str() == wanted);
                    let mut foreign = request(&WorkspaceId::parse("foreign")?, &case.text);
                    let foreign_denied = matches!(
                        retrieval::query(
                            &store,
                            &access,
                            Some(&view),
                            &foreign,
                            &bindings,
                            &chunker,
                            None,
                            &|| false
                        ),
                        Err(vcp_memory::Error::Access)
                    );
                    foreign.workspace = access.workspace.clone();
                    foreign.tasks = Some(vec![]);
                    let empty = retrieval::query(
                        &store,
                        &access,
                        Some(&view),
                        &foreign,
                        &bindings,
                        &chunker,
                        Some(QueryVector {
                            specification: &spec_digest,
                            values: &vector,
                        }),
                        &|| false,
                    )?
                    .passages
                    .is_empty();
                    report["scope_checks"].as_array_mut().unwrap().push(json!({"backend":backend_name,"query":case.id,"reader_repetition":repetition,"narrow_nonempty_exact_task":narrow_ok,"foreign_workspace_denied":foreign_denied,"empty_scope_no_passages":empty}));
                    if !narrow_ok || !foreign_denied || !empty {
                        return Err("hybrid scope isolation failed".into());
                    }
                }
            }
            report["timings"].as_array_mut().unwrap().push(json!({"backend":backend_name,"workspace":workspace,"cold_reader_reopen":distribution(&cold),"warm_hybrid_query":distribution(&warm)}));
            store.close().await?;
        }
    }
    drop(permit);
    Ok(())
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mut report = json!({"status":"running","phase":"hybrid-quality","corpus_sha256":digest_bytes(CORPUS),"top_k":K,"builds":[],"queries":[],"scope_checks":[],"timings":[],"failures":[],"limitations":["Fixed public synthetic fixture; labelled recall is separate from ANN agreement, with no retuned labels or asserted production quality threshold.","Cold-reader timing reopens components; filesystem caches are uncontrolled.","Current fixture eligibility is the original corpus current flag; this run does not qualify retention deletion.","Explicit admission estimates are not measured process RAM or a hard OS allocation quota."]});
    let result: Result<()> = (|| {
        match args.as_slice() {
            [mode,address,port,nonce] if mode=="--network-control" => {report["network"]=json!(network::Canary::new(address,port,nonce)?.observe(false)?); Ok(())},
            [mode,assets,address,port,nonce] if mode=="--offline" => {
                let canary=network::Canary::new(address,port,nonce)?;
                report["network_before"]=json!(canary.observe(true)?);
                let result=export_embeddings(Path::new(assets), &mut report);
                report["network_after"]=json!(canary.observe(true)?);
                result
            },
            [mode,file,sha] if mode=="--hybrid" => {
                let mut bytes=Vec::new();
                std::fs::File::open(file)?.take(2*1024*1024+1).read_to_end(&mut bytes)?;
                if bytes.len()>2*1024*1024 || digest_bytes(&bytes)!=*sha {return Err("embedding handoff byte bound/digest".into());}
                let bundle:Bundle=serde_json::from_slice(&bytes)?;
                report["embedding_bundle_sha256"]=json!(sha);
                let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build()?;
                runtime.block_on(qualify(bundle,&mut report))
            },
            _=>Err("required --network-control <private-ip> <port> <nonce> or --offline <assets> <private-ip> <port> <nonce>".into())
        }
    })();
    match result {
        Ok(()) => report["status"] = json!("pass"),
        Err(error) => {
            report["status"] = json!("fail");
            let stage = report["stage"].clone();
            report["failures"]
                .as_array_mut()
                .unwrap()
                .push(json!({"stage":stage,"error":error.to_string()}));
        }
    }
    println!("{report}");
    if report["status"] != "pass" {
        std::process::exit(1);
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryEmbedding {
    id: String,
    workspace: String,
    text_sha256: String,
    vector: Vec<f32>,
    inference_us: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    schema_version: u32,
    corpus_sha256: String,
    specification: String,
    rows: Vec<Encoded>,
    queries: Vec<QueryEmbedding>,
}
fn document_chunks(corpus: &Corpus) -> Result<Vec<embedding::Chunk>> {
    let mut chunks = Vec::new();
    for doc in corpus.documents.iter().filter(|doc| doc.current) {
        let scope = Scope {
            workspace: WorkspaceId::parse(&doc.workspace)?,
            session: SessionId::parse("hybrid-fixture")?,
            task: TaskId::parse(&doc.id)?,
        };
        chunks.extend(embedding::chunks(
            &doc.id,
            &scope,
            &doc.text,
            &Specification::qualified(),
        )?);
    }
    Ok(chunks)
}
fn normalized(vector: &[f32]) -> bool {
    let norm: f64 = vector.iter().map(|x| f64::from(*x).powi(2)).sum();
    vector.len() == 384 && norm.is_finite() && (norm - 1.0).abs() <= 0.001
}
fn validate_bundle(bundle: &Bundle, corpus: &Corpus) -> Result<()> {
    if bundle.schema_version != 1
        || bundle.corpus_sha256 != digest_bytes(CORPUS)
        || bundle.specification != Specification::qualified().digest()?
        || bundle.rows.len() > 1024
        || bundle.queries.len() != corpus.queries.len()
    {
        return Err("embedding bundle schema/specification/corpus mismatch".into());
    }
    let expected = document_chunks(corpus)?;
    if expected.len() != bundle.rows.len() {
        return Err("embedding source coverage differs from shared corpus".into());
    }
    for (chunk, row) in expected.iter().zip(&bundle.rows) {
        if chunk.identity != row.identity || !normalized(&row.vector) {
            return Err("embedding source identity or normalized vector invalid".into());
        }
    }
    for (case, row) in corpus.queries.iter().zip(&bundle.queries) {
        if row.id != case.id
            || row.workspace != case.workspace
            || row.text_sha256 != digest_bytes(case.text.as_bytes())
            || !normalized(&row.vector)
        {
            return Err("query embedding identity or normalized vector invalid".into());
        }
    }
    Ok(())
}
/// The only mode allowed to load a model. This mode performs no canonical store
/// operations and is always launched by the existing zero-capability broker.
fn export_embeddings(assets: &Path, report: &mut Value) -> Result<()> {
    report["phase"] = json!("hybrid-embedding-export");
    let corpus: Corpus = serde_json::from_slice(CORPUS)?;
    let chunks = document_chunks(&corpus)?;
    let admission = Admission::new(ResourceLimits::default())?;
    let permit = admission.acquire(Workload {
        rows: chunks.len(),
        source_bytes: chunks.iter().map(|c| c.text.len()).sum(),
        batch: 16,
        load_model: true,
    })?;
    report["declared_resources"] = json!({"estimate_version":vcp_memory::local_resources::ESTIMATE_VERSION,"ram_bytes":permit.estimate().ram_bytes,"temporary_disk_bytes":permit.estimate().temporary_disk_bytes});
    report["stage"] = json!("load_local_embedding");
    let start = Instant::now();
    let mut model = LocalEmbedding::load(assets, &|| false)?;
    report["load_us"] = json!(start.elapsed().as_micros());
    report["stage"] = json!("export_exact_source_and_query_vectors");
    let rows = model.embed(&chunks, &|| false)?;
    let mut queries = Vec::new();
    for case in &corpus.queries {
        let start = Instant::now();
        let vector = model.query(&case.text, &|| false)?;
        queries.push(QueryEmbedding {
            id: case.id.clone(),
            workspace: case.workspace.clone(),
            text_sha256: digest_bytes(case.text.as_bytes()),
            vector,
            inference_us: start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64,
        });
    }
    let bundle = Bundle {
        schema_version: 1,
        corpus_sha256: digest_bytes(CORPUS),
        specification: Specification::qualified().digest()?,
        rows,
        queries,
    };
    validate_bundle(&bundle, &corpus)?;
    let encoded = canonical_bytes(&bundle)?;
    if encoded.len() > 2 * 1024 * 1024 {
        return Err("embedding export exceeds handoff limit".into());
    }
    report["bundle"] = serde_json::to_value(bundle)?;
    report["boundary"]=json!("all model loading, source embedding, and query embedding inside zero-capability AppContainer; no canonical store in this child");
    drop(model);
    drop(permit);
    Ok(())
}
