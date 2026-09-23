// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
//! Compiled observer retrieval from real lexical-only retained publication.
#[path = "support/local_fixture.rs"]
mod wire;
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::atomic::AtomicBool};
use vcp_domain::{
    artifact::{ArtifactDescriptor, ArtifactSpec, Channel, Range},
    memory::*,
    task::{Objective, Task, TaskState},
    verification::Fingerprint,
    workspace::Scope,
    *,
};
use vcp_engine::{Access, Engine, HostFacts};
use vcp_memory::{
    publication::{self, Publisher},
    search_record::{self, ChunkerSpec, SourceBinding},
};
use vcp_protocol::command::{Command, CommandEnvelope};
use vcp_store::{
    artifact::ArtifactWriter,
    contract::{Collection, State},
    BackendKind, Store,
};
const CAPABILITY: &str = "memory/query-sources/1";
const TERM: &str = "retainedquerymarker";

fn initialize(client: &mut wire::Client, sources: bool) {
    let mut methods = vec!["memory/query", "task/read"];
    if sources {
        methods.push(CAPABILITY);
    }
    let result=client.rpc(1,"initialize",json!({"protocol_version":"1.0","client":{"name":"compiled-memory-query","version":"1"},"capabilities":methods,"required_capabilities":methods}));
    assert!(result.get("error").is_none(), "{result}");
}
fn params(fixture: &wire::Fixture, task: &TaskId, query: &str, limit: u32) -> Value {
    json!({"scope":fixture.scope(),"task":task,"query":query,"limit":limit})
}
#[track_caller]
fn page(client: &mut wire::Client, params: Value) -> Value {
    let reply = client.rpc(2, "memory/query", params.clone());
    assert!(reply.get("error").is_none(), "query {params}: {reply}");
    reply["result"]["value"].clone()
}
async fn issue(
    engine: &mut Engine<Store>,
    fixture: &wire::Fixture,
    task: &TaskId,
    payload: Command,
    expected: Revision,
) {
    let envelope = CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: fixture.config.workspace.clone(),
        session: fixture.config.session.clone(),
        task: Some(task.clone()),
        caller: fixture.config.actor.clone(),
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected,
        steering: SteeringRevision::ZERO,
        payload,
    };
    let access = Access {
        workspace: fixture.config.workspace.clone(),
        session: fixture.config.session.clone(),
        actor: fixture.config.actor.clone(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: false,
    };
    engine
        .handle(envelope, &access, &HostFacts::inspect(Timestamp::new(100)))
        .await
        .unwrap();
}
async fn capture(
    engine: &mut Engine<Store>,
    fixture: &wire::Fixture,
    scope: &Scope,
    schema: &str,
    bytes: &[u8],
) -> ArtifactDescriptor {
    let mut writer = engine
        .store()
        .spool()
        .create(ArtifactSpec {
            id: ArtifactId::new(),
            scope: scope.clone(),
            media_type: "text/plain".into(),
            schema: schema.into(),
            source: "compiled lexical query fixture".into(),
            channel: Channel::Evidence,
            retention: "history".into(),
            omissions: vec![],
        })
        .unwrap();
    for bytes in bytes.chunks(4096) {
        writer.write_chunk(bytes).unwrap();
    }
    let descriptor = writer.finalize().unwrap();
    drop(writer);
    issue(
        engine,
        fixture,
        &scope.task,
        Command::AttachArtifact {
            descriptor: descriptor.clone(),
        },
        Revision::ZERO,
    )
    .await;
    descriptor
}
struct Source {
    binding: SourceBinding,
    claim: ClaimId,
    version: ClaimVersionId,
    artifact: ArtifactId,
    task: TaskId,
    text: String,
}
async fn source(
    engine: &mut Engine<Store>,
    fixture: &wire::Fixture,
    task: TaskId,
    existing: bool,
    text: String,
) -> Source {
    let scope = Scope {
        workspace: fixture.config.workspace.clone(),
        session: fixture.config.session.clone(),
        task: task.clone(),
    };
    let root = RootId::parse(fixture.config.workspace.as_str()).unwrap();
    let snapshot = json!({"identity":{"workspace":scope.workspace,"root":root,"repository":fixture.config.binding.repository,"worktree":fixture.config.binding.worktree,"binding":fixture.config.binding.revision},"bounded_scan_complete":true,"files":[{"root":root,"path":"source.txt","sha256":vcp_protocol::digest_bytes(text.as_bytes()),"bytes":ByteCount::new(text.len() as u64)}]});
    let fingerprint = Fingerprint {
        repository: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&snapshot).unwrap()),
        buffers: "b".repeat(64),
        environment: "c".repeat(64),
    };
    if existing {
        let prior: Task = engine
            .store()
            .state()
            .record(Collection::Task, task.as_str(), &scope.workspace)
            .unwrap()
            .decode()
            .unwrap();
        issue(
            engine,
            fixture,
            &task,
            Command::ObserveFingerprint {
                fingerprint: fingerprint.clone(),
            },
            prior.revision,
        )
        .await;
    } else {
        issue(
            engine,
            fixture,
            &task,
            Command::CreateTask {
                root: task.clone(),
                parent: None,
                fork_origin: None,
                objective: Objective {
                    text: "retained lexical-only source".into(),
                    constraints: vec![],
                    acceptance: vec![],
                    source: EventId::new(),
                    steering: SteeringRevision::ZERO,
                },
                fingerprint: fingerprint.clone(),
                editing: false,
                required_checks: vec![],
            },
            Revision::ZERO,
        )
        .await;
        issue(
            engine,
            fixture,
            &task,
            Command::Transition {
                next: TaskState::Paused,
                reason: "observer retrieval only".into(),
                verification: None,
            },
            Revision::ZERO,
        )
        .await;
    }
    let origin = engine
        .store()
        .state()
        .events
        .iter()
        .find(|event| {
            event.event.kind == vcp_protocol::event::EventKind::TaskCreated
                && event.event.task.as_ref() == Some(&task)
        })
        .unwrap()
        .event
        .id
        .clone();
    let artifact = capture(
        engine,
        fixture,
        &scope,
        "verification-source/1",
        text.as_bytes(),
    )
    .await;
    let manifest = capture(
        engine,
        fixture,
        &scope,
        "verification-plan/1",
        &vcp_protocol::canonical_bytes(
            &json!({"before":snapshot,"source_artifacts":[artifact.spec.id]}),
        )
        .unwrap(),
    )
    .await;
    let access = memory_access(fixture);
    let proposal = Proposal {
        id: ProposalId::new(),
        command: CommandId::new(),
        claim: ClaimId::new(),
        scope: scope.clone(),
        actor: access.actor.clone(),
        epochs: Epochs {
            authority: AuthorityRevision::ZERO,
            deletion: DeletionEpoch::ZERO,
            policy: PolicyRevision::ZERO,
        },
        registry_version: REGISTRY_VERSION,
        extractor: "compiled-memory-query/1".into(),
        output_key: task.to_string(),
        origins: vec![origin],
        subject: task.to_string(),
        predicate: "architecture".into(),
        statement: format!("{TERM} governedclaimmarker retained source proves lexical retrieval"),
        value: ClaimValue::Architecture {
            decision: "Retain lexical source identity".into(),
            rationale: "Observed source bytes".into(),
            inference: true,
        },
        applicability: Applicability {
            repository: fixture.config.binding.repository.clone(),
            worktree: fixture.config.binding.worktree.clone(),
            roots: vec![root.clone()],
            paths: vec!["source.txt".into()],
            symbols: vec![],
            branch: None,
            fingerprint: Some(fingerprint.clone()),
            conditions: BTreeMap::new(),
            valid_from: None,
            valid_until: None,
        },
        evidence: vec![EvidenceRef {
            artifact: artifact.spec.id.clone(),
            sha256: artifact.sha256.clone(),
            range: Some(Range {
                start: ByteCount::ZERO,
                end: ByteCount::new(8),
            }),
            source: Some(fingerprint.clone()),
            verification: None,
            kind: EvidenceKind::Source,
        }],
        predecessor: None,
        correction_reason: None,
        retention: "workspace".into(),
    };
    let claim = proposal.claim.clone();
    let accepted =
        vcp_memory::repository::propose(engine.store_mut(), &access, proposal, Timestamp::new(200))
            .await
            .unwrap();
    assert_eq!(accepted.result.resolution.outcome, Outcome::Accepted);
    Source {
        binding: SourceBinding {
            manifest: manifest.spec.id,
            artifact: artifact.spec.id.clone(),
            root,
            path: "source.txt".into(),
            symbols: vec![],
            fingerprint,
        },
        claim,
        version: accepted.result.version.unwrap(),
        artifact: artifact.spec.id,
        task,
        text,
    }
}
fn memory_access(fixture: &wire::Fixture) -> vcp_memory::access::Access {
    vcp_memory::access::Access {
        workspace: fixture.config.workspace.clone(),
        actor: fixture.config.actor.clone(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        tasks: None,
    }
}
async fn seed(fixture: &wire::Fixture) -> (Source, Source, State) {
    let mut engine = Engine::new(fixture.reopen().await).unwrap();
    let selected = source(
        &mut engine,
        fixture,
        fixture.config.root_task.clone(),
        true,
        (0..96)
            .map(|index| {
                format!(
                    "{TERM} artifactmarker chunk {index:03} {}\n",
                    "retained source text ".repeat(100)
                )
            })
            .collect(),
    )
    .await;
    let other = source(
        &mut engine,
        fixture,
        TaskId::new(),
        false,
        format!("{TERM} foreigntaskmarker exact foreign source\n"),
    )
    .await;
    let mut store = engine.into_store();
    let access = memory_access(fixture);
    let inventory = search_record::inventory(
        &store,
        &access,
        &[selected.binding.clone(), other.binding.clone()],
        &ChunkerSpec::default(),
        search_record::Limits::default(),
    )
    .unwrap();
    assert!(!inventory.records.is_empty());
    let scope = Scope {
        workspace: fixture.config.workspace.clone(),
        session: fixture.config.session.clone(),
        task: fixture.config.root_task.clone(),
    };
    let publisher =
        Publisher::new(&fixture.config.canonical_root.join("search-generations")).unwrap();
    let prepared = publisher
        .prepare(
            publication::capture(&store, &access, &scope, inventory).unwrap(),
            None,
            &AtomicBool::new(false),
            &|_| {},
        )
        .unwrap();
    publisher
        .publish(&mut store, &access, &prepared, Timestamp::new(300), &|_| {})
        .await
        .unwrap();
    let state = store.state().clone();
    store.close().await.unwrap();
    (selected, other, state)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_observer_memory_query_preserves_real_sources_scope_and_read_only_bounds() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let fixture = wire::Fixture::new(backend).await;
        let store = fixture.reopen().await;
        let initial = store.state().clone();
        store.close().await.unwrap();
        let mut missing = wire::Client::connect(&fixture, "observer");
        initialize(&mut missing, true);
        let absent = page(
            &mut missing,
            params(&fixture, &fixture.config.root_task, TERM, 8),
        );
        assert_eq!(absent["generation"], Value::Null);
        assert_eq!(absent["findings"], json!([]));
        assert_eq!(absent["rebuild_required"], true);
        assert_eq!(absent["complete"], false);
        assert!(absent["degraded"]
            .as_array()
            .unwrap()
            .contains(&json!("generation_unavailable")));
        assert!(missing.finish().await.0.success());
        let store = fixture.reopen().await;
        assert_eq!(store.state(), &initial);
        store.close().await.unwrap();
        let (selected, other, before) = seed(&fixture).await;
        let mut legacy = wire::Client::connect(&fixture, "observer");
        initialize(&mut legacy, false);
        let unsupported = legacy.rpc(2, "memory/query", params(&fixture, &selected.task, TERM, 8));
        assert_eq!(
            unsupported["error"]["data"]["details"]["code"],
            "CAPABILITY_UNAVAILABLE"
        );
        assert!(legacy.finish().await.0.success());
        let mut client = wire::Client::connect(&fixture, "observer");
        initialize(&mut client, true);
        let claim = page(
            &mut client,
            params(&fixture, &selected.task, "governedclaimmarker", 8),
        );
        let finding = claim["findings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|finding| finding["source"]["kind"] == "claim")
            .unwrap();
        assert!(claim["generation"].is_string());
        assert_eq!(finding["source"]["claim"], selected.claim.as_str());
        assert_eq!(finding["source"]["version"], selected.version.as_str());
        assert!(finding["source"].get("artifact").is_none());
        assert_eq!(finding["evidence"], json!([selected.artifact]));
        let artifact = page(
            &mut client,
            params(&fixture, &selected.task, "artifactmarker", 64),
        );
        let findings = artifact["findings"].as_array().unwrap();
        assert!(!findings.is_empty());
        assert!(findings.len() <= 64);
        for finding in findings {
            assert_eq!(finding["source"]["kind"], "artifact");
            assert_eq!(finding["source"]["artifact"], selected.artifact.as_str());
            assert!(finding["source"].get("claim").is_none());
            assert_eq!(
                finding["source_sha256"],
                vcp_protocol::digest_bytes(selected.text.as_bytes())
            );
            let start: usize = finding["start"].as_str().unwrap().parse().unwrap();
            let end: usize = finding["end"].as_str().unwrap().parse().unwrap();
            assert_eq!(
                finding["content"].as_str().unwrap(),
                &selected.text[start..end]
            );
            assert!(!finding["content"]
                .as_str()
                .unwrap()
                .contains("foreigntaskmarker"));
        }
        assert_eq!(artifact["truncated"], true);
        assert_eq!(artifact["complete"], false);
        assert!(findings.iter().any(|finding| finding["trimmed"] == true));
        let one = page(
            &mut client,
            params(&fixture, &selected.task, "artifactmarker", 1),
        );
        assert_eq!(one["findings"].as_array().unwrap().len(), 1);
        assert_eq!(one["truncated"], true);
        assert_eq!(one["complete"], false);
        let foreign = page(
            &mut client,
            params(&fixture, &selected.task, "foreigntaskmarker", 8),
        );
        assert_eq!(foreign["findings"], json!([]));
        let own = page(
            &mut client,
            params(&fixture, &other.task, "foreigntaskmarker", 8),
        );
        assert!(own["findings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|finding| finding["source"]["artifact"] == other.artifact.as_str()));
        assert!(!own["findings"].as_array().unwrap().is_empty());
        let mut cross_session = params(&fixture, &selected.task, TERM, 8);
        cross_session["scope"]["session"] = json!("foreign-session");
        assert!(client
            .rpc(3, "memory/query", cross_session)
            .get("error")
            .is_some());
        let mut cross_workspace = params(&fixture, &selected.task, TERM, 8);
        cross_workspace["scope"]["workspace"] = json!("foreign-workspace");
        assert!(client
            .rpc(6, "memory/query", cross_workspace)
            .get("error")
            .is_some());
        assert!(client
            .rpc(
                4,
                "memory/query",
                params(&fixture, &selected.task, TERM, 65)
            )
            .get("error")
            .is_some());
        assert!(client
            .rpc(
                5,
                "memory/query",
                params(&fixture, &selected.task, &"x".repeat(4097), 8)
            )
            .get("error")
            .is_some());
        assert!(client.finish().await.0.success());
        let store = fixture.reopen().await;
        assert_eq!(store.state(), &before);
        store.close().await.unwrap();
    }
}
