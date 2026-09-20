// SPDX-License-Identifier: Apache-2.0
//! Deterministic held-out evidence comparison; no model or embedding inference.
#[path = "../../../evals/memory/grader.rs"]
mod grader;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path, sync::atomic::AtomicBool, time::Instant};
use vcp_domain::{memory::Outcome, task::Objective, verification::Fingerprint, workspace::*, *};
use vcp_engine::{Engine, HostFacts};
use vcp_memory::{
    access::Access,
    publication::{self, Publisher},
    retrieval,
    search_record::{self, ChunkerSpec, TextSource},
};
use vcp_protocol::{
    command::{Command, CommandEnvelope},
    digest_bytes,
};
use vcp_store::{BackendKind, Store};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const HISTORY: &[u8] = include_bytes!("../../../evals/memory/history.json");
const QUESTIONS: &[u8] = include_bytes!("../../../evals/memory/questions.json");
const CAP: usize = 16384;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    id: String,
    key: String,
    value: String,
    supersedes: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct History {
    revision: String,
    observations: Vec<Observation>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Questions {
    revision: String,
    cases: Vec<grader::Case>,
}
struct MarkdownSection {
    id: String,
    text: String,
    supersedes: String,
}
fn markdown_sections(text: &str) -> Result<Vec<MarkdownSection>> {
    text.split("## ")
        .skip(1)
        .map(|section| {
            let mut lines = section.lines();
            let id = lines
                .next()
                .ok_or("missing Markdown section identity")?
                .to_string();
            let text = lines
                .next()
                .ok_or("missing Markdown source text")?
                .to_string();
            let supersedes = lines
                .next()
                .and_then(|line| line.strip_prefix("Supersedes: "))
                .ok_or("missing Markdown predecessor")?
                .to_string();
            if lines.any(|line| !line.is_empty()) {
                return Err("unexpected Markdown section content".into());
            }
            Ok(MarkdownSection {
                id,
                text,
                supersedes,
            })
        })
        .collect()
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
async fn publish(
    engine: &mut Engine<Store>,
    access: &Access,
    scope: &Scope,
    publisher: &Publisher,
) -> Result<()> {
    let inventory = search_record::inventory(
        engine.store(),
        access,
        &[],
        &ChunkerSpec::default(),
        search_record::Limits::default(),
    )?;
    let prepared = publisher.prepare(
        publication::capture(engine.store(), access, scope, inventory)?,
        None,
        &AtomicBool::new(false),
        &|_| {},
    )?;
    publisher
        .publish(
            engine.store_mut(),
            access,
            &prepared,
            Timestamp::new(300),
            &|_| {},
        )
        .await?;
    Ok(())
}
fn retained_bytes(path: &Path) -> Result<u64> {
    let mut total = 0;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        total += if entry.file_type()?.is_dir() {
            retained_bytes(&entry.path())?
        } else {
            entry.metadata()?.len()
        };
    }
    Ok(total)
}
async fn run() -> Result<Value> {
    let history: History = serde_json::from_slice(HISTORY)?;
    // Ingestion sees history only. Questions and grader labels are opened later.
    let temporary = tempfile::tempdir()?;
    let mut attempts = Vec::new();
    let mut resources = Vec::new();
    for (backend_name, backend) in [
        ("files", BackendKind::Files),
        ("sqlite", BackendKind::Sqlite),
    ] {
        let path = temporary.path().join(backend_name);
        let mut engine = Engine::new(Store::open(&path.join("canonical"), backend, &[]).await?)?;
        let mut scope = Scope {
            workspace: WorkspaceId::parse("comparison")?,
            session: SessionId::parse("replay")?,
            task: TaskId::parse("bootstrap")?,
        };
        issue(
            &mut engine,
            &scope,
            Command::Initialize {
                binding: Binding {
                    host: HostId::new(),
                    root: "C:/synthetic-memory-comparison".into(),
                    repository: "comparison".into(),
                    worktree: "main".into(),
                    revision: Revision::ZERO,
                },
            },
        )
        .await?;
        let access = Access {
            workspace: scope.workspace.clone(),
            actor: owner(&scope).actor,
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            tasks: None,
        };
        let publisher = Publisher::new(&path.join("derived"))?;
        let mut versions = BTreeMap::<String, (ClaimId, ClaimVersionId)>::new();
        let mut version_sources = BTreeMap::<String, String>::new();
        let mut ingest_us = 0;
        let mut build_us = 0;
        let mut markdown = String::new();
        for (position, observation) in history.observations.iter().enumerate() {
            let start = Instant::now();
            scope.task = TaskId::parse(&observation.id)?;
            issue(&mut engine,&scope,Command::CreateTask {root:scope.task.clone(),parent:None,fork_origin:None,objective:Objective {text:json!({"memory_preference":{"key":observation.key,"value":observation.value}}).to_string(),constraints:vec![],acceptance:vec!["retain explicit input".into()],source:EventId::new(),steering:SteeringRevision::ZERO},fingerprint:Fingerprint {repository:"a".repeat(64),buffers:"b".repeat(64),environment:"c".repeat(64)},editing:false,required_checks:vec![]}).await?;
            let event = engine
                .store()
                .state()
                .events
                .last()
                .ok_or("missing canonical event")?
                .clone();
            let mut proposal =
                vcp_memory::preferences::materialize(engine.store_mut(), &access, &event)
                    .await?
                    .ok_or("preference was not materialized")?;
            if let Some(prior) = &observation.supersedes {
                let (claim, version) = versions.get(prior).ok_or("invalid history predecessor")?;
                proposal.claim = claim.clone();
                proposal.predecessor = Some(version.clone());
                proposal.correction_reason =
                    Some("Explicit synthetic owner correction in frozen history".into());
            }
            let claim = proposal.claim.clone();
            let committed = vcp_memory::repository::propose(
                engine.store_mut(),
                &access,
                proposal,
                Timestamp::new(200 + position as u64),
            )
            .await?;
            if committed.result.resolution.outcome != Outcome::Accepted {
                return Err(format!(
                    "observation {} rejected: {:?}",
                    observation.id, committed.result.resolution
                )
                .into());
            }
            let version = committed
                .result
                .version
                .ok_or("accepted proposal has no version")?;
            version_sources.insert(version.to_string(), observation.id.clone());
            versions.insert(observation.id.clone(), (claim, version));
            markdown.push_str(&format!(
                "## {}\n{} = {}\nSupersedes: {}\n",
                observation.id,
                observation.key,
                observation.value,
                observation.supersedes.as_deref().unwrap_or("")
            ));
            ingest_us += start.elapsed().as_micros();
            // Freeze a genuinely lagging generation before the final observation.
            if position + 2 == history.observations.len() {
                let start = Instant::now();
                publish(&mut engine, &access, &scope, &publisher).await?;
                build_us += start.elapsed().as_micros();
            }
        }
        std::fs::write(path.join("versioned.md"), &markdown)?;
        let questions: Questions = serde_json::from_slice(QUESTIONS)?;
        for mode in ["lagging", "current"] {
            if mode == "current" {
                let start = Instant::now();
                publish(&mut engine, &access, &scope, &publisher).await?;
                build_us += start.elapsed().as_micros();
            }
            let recovery = publisher.recover(engine.store(), &access)?;
            let view = recovery.view.as_ref().ok_or("published view missing")?;
            for case in &questions.cases {
                for strategy in [
                    "no_semantic_memory",
                    "versioned_markdown",
                    "governed_memory",
                ] {
                    let start = Instant::now();
                    let mut ids = Vec::new();
                    let mut texts = Vec::new();
                    let mut evidence = Vec::new();
                    let mut error = None;
                    let mut supported = true;
                    let mut detail = json!({"route":"none"});
                    if strategy == "versioned_markdown" {
                        // Deterministic reference: read the retained Markdown bytes,
                        // select current authorized source sections with exact lexical matching.
                        let retained = std::fs::read_to_string(path.join("versioned.md"))?;
                        let sections = markdown_sections(&retained)?;
                        for doc in &sections {
                            if case.allowed.contains(&doc.id)
                                && !sections.iter().any(|new| new.supersedes == doc.id)
                                && doc.text.to_lowercase().contains(&case.query.to_lowercase())
                            {
                                let text = doc.text.clone();
                                supported &= history.observations.iter().any(|source| {
                                    source.id == doc.id
                                        && text == format!("{} = {}", source.key, source.value)
                                });
                                if texts.iter().map(String::len).sum::<usize>() + text.len() <= CAP
                                    && ids.len() < 8
                                {
                                    ids.push(doc.id.clone());
                                    texts.push(text);
                                }
                            }
                        }
                        detail = json!({"route":"markdown_lexical_current_head","retained_sha256":digest_bytes(retained.as_bytes())});
                    } else if strategy == "governed_memory" {
                        let mut scoped = Access {
                            workspace: access.workspace.clone(),
                            actor: access.actor.clone(),
                            authority: access.authority,
                            read: access.read,
                            write: access.write,
                            tasks: None,
                        };
                        scoped.tasks = Some(
                            case.allowed
                                .iter()
                                .map(TaskId::parse)
                                .collect::<std::result::Result<_, _>>()?,
                        );
                        let request = retrieval::Request {
                            workspace: scope.workspace.clone(),
                            tasks: None,
                            roots: None,
                            paths: None,
                            symbols: None,
                            text: case.query.clone(),
                            historical: None,
                            minimum_sequence: None,
                            timeout_ms: 30_000,
                            results: 8,
                            tokens: CAP,
                            bytes: CAP,
                        };
                        match retrieval::query(
                            engine.store(),
                            &scoped,
                            Some(view),
                            &request,
                            &[],
                            &ChunkerSpec::default(),
                            None,
                            &|| false,
                        ) {
                            Ok(response) => {
                                detail = json!({"route":"production_lexical_retrieval","degraded":response.degraded,"indexed_sequence":response.indexed_sequence,"canonical_watermark":response.canonical_watermark,"generation_watermark":response.generation_watermark,"token_upper_bound":response.token_upper_bound});
                                for passage in &response.passages {
                                    let id = match &passage.source {
                                        TextSource::Claim { version, .. } => {
                                            version_sources.get(version.as_str()).cloned()
                                        }
                                        _ => None,
                                    };
                                    let doc = id.as_ref().and_then(|id| {
                                        history.observations.iter().find(|doc| &doc.id == id)
                                    });
                                    supported &= doc.is_some_and(|doc| {
                                        passage.text == format!("{} = {}", doc.key, doc.value)
                                    }) && !passage.evidence.is_empty();
                                    ids.push(id.unwrap_or_else(|| "UNKNOWN-SOURCE".into()));
                                    texts.push(passage.text.clone());
                                    evidence.push(json!({"source":passage.source,"digest":passage.source_digest,"evidence":passage.evidence,"rank":passage.rank,"routes":{"lexical":passage.rank.lexical_rank.is_some(),"vector":passage.rank.vector_rank.is_some(),"recent_overlay":passage.rank.overlay_rank.is_some(),"canonical_fallback":false}}));
                                }
                                if let Some(fence) = response.fence {
                                    if let Err(problem) =
                                        retrieval::revalidate_fence(engine.store(), &scoped, &fence)
                                    {
                                        error = Some(problem.to_string());
                                    }
                                }
                            }
                            Err(problem) => error = Some(problem.to_string()),
                        }
                    }
                    let query_us = start.elapsed().as_micros();
                    let grades = grader::grade(case, &ids, supported, error.is_none());
                    attempts.push(json!({"backend":backend_name,"mode":mode,"strategy":strategy,"case":case.id,"question":case.question,"query":case.query,"authorized_tasks":case.allowed,"relevant":case.relevant,"source_ids":ids,"passages":texts,"evidence":evidence,"detail":detail,"error":error,"abandoned":false,"query_us":query_us,"prompt_utf8_bytes":serde_json::to_vec(&texts)?.len(),"grade":grades}));
                }
            }
        }
        resources.push(json!({"backend":backend_name,"ingest_us":ingest_us,"lexical_build_us":build_us,"canonical_retained_bytes":retained_bytes(&path.join("canonical"))?,"derived_retained_bytes":retained_bytes(&path.join("derived"))?,"markdown_retained_bytes":markdown.len(),"embedding_us":null,"restore_readiness_us":null,"peak_rss_bytes":null}));
    }
    let questions: Questions = serde_json::from_slice(QUESTIONS)?;
    let mut summaries = Vec::new();
    for strategy in [
        "no_semantic_memory",
        "versioned_markdown",
        "governed_memory",
    ] {
        let rows: Vec<_> = attempts
            .iter()
            .filter(|row| row["strategy"] == strategy)
            .collect();
        let sum = |key: &str| {
            rows.iter()
                .map(|row| row["grade"][key].as_u64().unwrap_or(0))
                .sum::<u64>()
        };
        summaries.push(json!({"strategy":strategy,"attempted":rows.len(),"errors":rows.iter().filter(|r|!r["error"].is_null()).count(),"task_passes":rows.iter().filter(|r|r["grade"]["task_pass"]==true).count(),"sourced_hits":sum("sourced_hits"),"relevant_denominator":sum("relevant_denominator"),"forbidden_passages":sum("forbidden_passages"),"stale_passages":sum("stale_passages"),"unsupported_passages":sum("unsupported_passages")}));
    }
    let pass = attempts
        .iter()
        .filter(|row| row["strategy"] == "governed_memory")
        .all(|row| row["grade"]["task_pass"] == true);
    Ok(
        json!({"schema":"p5-08-memory-comparison/1","pass":pass,"history_revision":history.revision,"history_sha256":digest_bytes(HISTORY),"questions_revision":questions.revision,"questions_sha256":digest_bytes(QUESTIONS),"grader_sha256":digest_bytes(include_bytes!("../../../evals/memory/grader.rs")),"harness_sha256":digest_bytes(include_bytes!("memory_comparison.rs")),"hardware":{"os":std::env::consts::OS,"architecture":std::env::consts::ARCH,"logical_parallelism":std::thread::available_parallelism()?.get()},"configuration":{"model":"none-deterministic-source-evidence","provider":null,"model_spend_usd":0,"maintenance_spend_usd":0,"embedding":"not_run","result_cap":8,"prompt_byte_cap":CAP,"initial_state":"empty isolated store per backend; same history replayed","repetitions":1},"limitations":["Synthetic explicit preferences only; not model answer quality or broad semantic recall","Markdown baseline uses frozen history metadata for scope and supersession, without a model or Git process","No inference, embeddings, paid calls, restore timing or peak RSS measured here","Timing includes wall clock query execution; hardware CPU model/RAM supplied by orchestration if available","Overall P5-08 acceptance requires separate fault, vault and resource campaign evidence"],"resources":resources,"summary":summaries,"attempts":attempts}),
    )
}
#[tokio::main]
async fn main() -> Result<()> {
    let report = run().await?;
    let bytes = serde_json::to_vec_pretty(&report)?;
    if let Some(path) = std::env::args_os().nth(1) {
        std::fs::write(path, &bytes)?;
    } else {
        println!("{}", String::from_utf8(bytes)?);
    }
    if report["pass"] != true {
        return Err(
            "governed evidence comparison failed; all attempted queries retained in report".into(),
        );
    }
    Ok(())
}
