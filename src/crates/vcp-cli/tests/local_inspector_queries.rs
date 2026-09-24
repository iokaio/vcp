// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! Actual SDK history, memory, policy and routing reads; no provider or editor is launched.
#[path = "support/local_fixture.rs"]
mod local_fixture;
use local_fixture::*;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use vcp_domain::{
    artifact::{ArtifactSpec, Channel, Range},
    ids::*,
    memory::*,
    retention_selector::{Criterion, Selector, Tree},
    revision::*,
    task::Task,
};
use vcp_memory::repository::propose;
use vcp_store::{
    artifact::ArtifactWriter,
    contract::{CanonicalStore, Collection, Mutation, Record, Transaction},
    BackendKind,
};
async fn driver(input: Value) {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/sdk-ts/tests/native-inspector-queries.mjs");
    let bytes = serde_json::to_vec(&input).unwrap();
    assert!(bytes.len() < 256 * 1024);
    let output = tokio::task::spawn_blocking(move || {
        let mut child =
            Command::new(std::env::var_os("VCP_TEST_NODE").expect("qualified native Node path"))
                .arg(script)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
        child.stdin.take().unwrap().write_all(&bytes).unwrap();
        let deadline = Instant::now() + Duration::from_secs(120);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() > deadline {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                panic!(
                    "bounded SDK driver timeout: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        child.wait_with_output().unwrap()
    })
    .await
    .unwrap();
    assert!(
        output.status.success(),
        "SDK driver failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    println!(
        "SDK inspector evidence: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["ok"],
        true
    );
}
async fn seed(fixture: &Fixture) -> serde_json::Value {
    let mut store = fixture.reopen().await;
    let task: Task = store
        .state()
        .record(
            Collection::Task,
            fixture.config.root_task.as_str(),
            &fixture.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    let origin = store
        .state()
        .events
        .iter()
        .find(|event| {
            event.event.kind == vcp_protocol::event::EventKind::TaskCreated
                && event.event.task.as_ref() == Some(&fixture.config.root_task)
        })
        .unwrap()
        .event
        .id
        .clone();
    let mut writer = store
        .spool()
        .create(ArtifactSpec {
            id: ArtifactId::new(),
            scope: task.scope.clone(),
            media_type: "text/plain".into(),
            schema: "memory-source/1".into(),
            source: "offline compiled memory fixture".into(),
            channel: Channel::Evidence,
            retention: "history".into(),
            omissions: vec![],
        })
        .unwrap();
    writer
        .write_chunk(b"mod parser; // immutable source\n")
        .unwrap();
    let artifact = writer.finalize().unwrap();
    drop(writer);
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put {
                expected: None,
                record: Record::typed(
                    Collection::Artifact,
                    artifact.spec.id.as_str(),
                    task.scope.workspace.clone(),
                    Revision::ZERO,
                    &artifact,
                )
                .unwrap(),
            }],
            events: vec![vcp_protocol::event::EventInput {
                id: EventId::new(),
                workspace: task.scope.workspace.clone(),
                session: task.scope.session.clone(),
                task: Some(task.scope.task.clone()),
                actor: fixture.config.actor.clone(),
                correlation: CommandId::new(),
                causation: None,
                timestamp: Timestamp::new(9_007_199_254_740_993),
                kind: vcp_protocol::event::EventKind::Diagnostic,
                artifacts: vec![artifact.spec.id.clone()],
                data: json!({"schema_version":1,"diagnostic":"native-inspector-fixture"}),
                metadata: Some(vcp_protocol::event::EventMetadata {
                    agent: None,
                    provider: Some("offline-provider".into()),
                    model: Some("offline-model".into()),
                    paths: vec!["src/parser.rs".into()],
                }),
            }],
            command: None,
        })
        .await
        .unwrap();
    let mut access = vcp_memory::access::Access {
        workspace: task.scope.workspace.clone(),
        actor: fixture.config.actor.clone(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        tasks: None,
    };
    let proposal = Proposal {
        id: ProposalId::new(),
        command: CommandId::new(),
        claim: ClaimId::new(),
        scope: task.scope.clone(),
        actor: access.actor.clone(),
        epochs: Epochs {
            authority: AuthorityRevision::ZERO,
            deletion: DeletionEpoch::ZERO,
            policy: PolicyRevision::ZERO,
        },
        registry_version: REGISTRY_VERSION,
        extractor: "compiled-memory/1".into(),
        output_key: "architecture".into(),
        origins: vec![origin.clone()],
        subject: "parser".into(),
        predicate: "architecture".into(),
        statement: "Parser isolates syntax; retained-content-marker".into(),
        value: ClaimValue::Architecture {
            decision: "Parser isolates syntax".into(),
            rationale: "Retained module source".into(),
            inference: true,
        },
        applicability: Applicability {
            repository: fixture.config.binding.repository.clone(),
            worktree: fixture.config.binding.worktree.clone(),
            roots: vec![],
            paths: vec!["src/parser.rs".into()],
            symbols: vec![],
            branch: None,
            fingerprint: Some(task.fingerprint.clone()),
            conditions: BTreeMap::new(),
            valid_from: None,
            valid_until: None,
        },
        evidence: vec![EvidenceRef {
            artifact: artifact.spec.id.clone(),
            sha256: artifact.sha256.clone(),
            range: Some(Range {
                start: ByteCount::new(2),
                end: ByteCount::new(8),
            }),
            source: Some(task.fingerprint.clone()),
            verification: None,
            kind: EvidenceKind::Source,
        }],
        predecessor: None,
        correction_reason: None,
        retention: "workspace".into(),
    };
    let mut previous = None;
    // Keep native debug startup within the unchanged SDK bootstrap deadline.
    // The lifecycle fixture independently qualifies the >128-version boundary;
    // this actual SDK fixture exercises five pages of seven versions per store.
    for index in 0..33 {
        let mut next = proposal.clone();
        next.id = ProposalId::new();
        next.command = CommandId::new();
        next.output_key = format!("history-{index}");
        next.predecessor = previous;
        next.correction_reason = next
            .predecessor
            .as_ref()
            .map(|_| "explicit fixture correction".into());
        next.statement = format!("Parser revision {index}");
        if let ClaimValue::Architecture { decision, .. } = &mut next.value {
            *decision = next.statement.clone();
        }
        previous = propose(&mut store, &access, next, Timestamp::new(200 + index))
            .await
            .unwrap()
            .result
            .version;
        assert!(previous.is_some());
    }
    let mut versions = Vec::new();
    let mut after = MemorySeq::ZERO;
    let mut at = None;
    loop {
        let (page, upper, more) =
            vcp_memory::history::window(&store, &access, &proposal.claim, at, after, 32).unwrap();
        at = Some(upper);
        for row in page.versions {
            after = row.memory_seq;
            versions.push(row.id);
        }
        if !more {
            break;
        }
    }
    assert_eq!(versions.len(), 33);
    let (next, inspection) = seed_inspection(fixture, store, &mut access, &task).await;
    store = next;
    let history_access = vcp_audit::history::Access {
        workspace: access.workspace.clone(),
        authority: access.authority,
        read: true,
        tasks: None,
    };
    let mut query = vcp_audit::history_query::Query {
        selector: Selector {
            schema_version: 1,
            tree: Tree::Match(Criterion::Workspace(access.workspace.clone())),
        },
        text: None,
        limit: 7,
        cursor: None,
        artifact: None,
        expand_compacted: false,
    };
    let mut history = Vec::new();
    loop {
        let page = vcp_audit::history_query::query_session(
            store.state(),
            &history_access,
            &query,
            &task.scope.session,
        )
        .unwrap();
        for row in page.rows {
            history.push(json!({"id":row.event.event.id,"session":row.event.event.session,"task":row.event.event.task,"sequence":row.event.sequence.get().to_string(),"timestamp_ms":row.event.event.timestamp.get().to_string(),"visibility":row.visibility,"artifacts":row.artifact_links.iter().map(|a|json!({"id":a.id,"availability":a.availability,"original_bytes":a.original_bytes.map(|v|v.get().to_string())})).collect::<Vec<_>>()}));
        }
        query.cursor = page.next_cursor;
        if query.cursor.is_none() {
            break;
        }
    }
    let input = json!({"executable":env!("CARGO_BIN_EXE_vcp"),"workspace":fixture.workspace,"data":fixture.data,"scope":fixture.scope(),"task":fixture.config.root_task,"claim":proposal.claim,"artifact":artifact.spec.id,"origin":origin,"versions":versions,"history":history,"watermark":store.state().watermark.get().to_string(),"inspection":inspection});
    store.close().await.unwrap();
    input
}

async fn seed_inspection(
    fixture: &Fixture,
    store: vcp_store::Store,
    access: &mut vcp_memory::access::Access,
    task: &Task,
) -> (vcp_store::Store, Value) {
    use vcp_domain::policy::{
        Autonomy, Denial, EffectClass, Grant, GrantScope, GrantTarget, Policy, RuleOrigin,
    };
    use vcp_models::routing;
    use vcp_protocol::command::{Command as EngineCommand, CommandEnvelope};
    let mut engine = vcp_engine::Engine::new(store).unwrap();
    let mut caller = vcp_engine::Access {
        actor: access.actor.clone(),
        workspace: access.workspace.clone(),
        session: fixture.config.session.clone(),
        authority: access.authority,
        read: true,
        write: true,
        bootstrap: false,
    };
    let envelope = |engine: &vcp_engine::Engine<vcp_store::Store>,
                    caller: &vcp_engine::Access,
                    payload| CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: caller.workspace.clone(),
        session: caller.session.clone(),
        task: None,
        caller: caller.actor.clone(),
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        payload,
    };
    let policy = Policy {
        workspace: access.workspace.clone(),
        revision: PolicyRevision::ZERO,
        mode: Autonomy::Ask,
        denials: (0..2)
            .map(|i| Denial {
                id: format!("native-denial-{i}"),
                origin: RuleOrigin::User,
                reason: "private-denial-reason-marker".into(),
                effects: BTreeSet::from([EffectClass::Publish]),
                tool: Some(format!("private-tool-{i}")),
                roots: BTreeSet::from([RootId::parse(access.workspace.as_str()).unwrap()]),
                paths: vec![format!("private-path-{i}")],
            })
            .collect(),
        workspace_roots: BTreeSet::from([RootId::parse(access.workspace.as_str()).unwrap()]),
        automatic_effects: BTreeSet::new(),
        timeout_ceiling_ms: Units::new(12345),
        output_ceiling_bytes: ByteCount::new(54321),
    };
    engine
        .handle(
            envelope(
                &engine,
                &caller,
                EngineCommand::SetPolicy {
                    policy: policy.clone(),
                },
            ),
            &caller,
            &vcp_engine::HostFacts::inspect(Timestamp::new(1000)),
        )
        .await
        .unwrap();
    let workspace: vcp_domain::workspace::Workspace = engine
        .store()
        .state()
        .record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    caller.authority = workspace.authority;
    access.authority = workspace.authority;
    let mut grants = Vec::new();
    for index in 0..3 {
        let grant = Grant {
            id: GrantId::parse(format!("native-grant-{index}")).unwrap(),
            actor: caller.actor.clone(),
            scope: if index == 2 {
                GrantScope::Workspace {
                    workspace: access.workspace.clone(),
                }
            } else {
                GrantScope::Task {
                    scope: task.scope.clone(),
                }
            },
            host: workspace.binding.host.clone(),
            binding: workspace.binding.revision,
            authority: workspace.authority,
            policy: policy.revision,
            expires_at: Timestamp::new(u64::MAX),
            target: GrantTarget::Exact {
                digest: if index == 0 {
                    "a".repeat(64)
                } else {
                    "b".repeat(64)
                },
            },
            origin: RuleOrigin::User,
            reason: "private-grant-reason-marker".into(),
            revoked: index == 1,
            revision: Revision::ZERO,
            approval: None,
        };
        engine
            .handle(
                envelope(
                    &engine,
                    &caller,
                    EngineCommand::SetGrant {
                        grant: grant.clone(),
                    },
                ),
                &caller,
                &vcp_engine::HostFacts::inspect(Timestamp::new(1000)),
            )
            .await
            .unwrap();
        grants.push(grant);
    }
    let mut store = engine.into_store();
    let routing_policy = routing::Policy {
        schema_version: 1,
        id: String::new(),
        parent: None,
        profile: routing::Profile::Low,
        allowed_models: BTreeSet::from(["native-model-a".into(), "native-model-b".into()]),
        allowed_endpoints: BTreeSet::from(["native-endpoint".into()]),
        allowed_groups: BTreeSet::from([routing::Group::Low]),
        quality_floor_bps: 7000,
        minimum_samples: 10,
        maximum_evidence_age_ms: 9007199254740993,
        deny_data_collection: true,
        require_zdr: true,
        ordering: vec![
            routing::Preference::TotalCost,
            routing::Preference::Latency,
            routing::Preference::Quality,
            routing::Preference::Capability,
        ],
        pin: None,
        broader_task_class: None,
        output_tokens: Some(Units::new(321)),
        input_tokens: Some(Units::new(654)),
        escalation_limits: None,
        reasoning_effort: None,
        retrieval_limits: None,
    }
    .seal()
    .unwrap();
    let published = vcp_lifecycle::foundation::routing_state::initialize_policy(
        &mut store,
        access,
        routing_policy.clone(),
        Timestamp::new(1001),
    )
    .await
    .unwrap();
    let catalog = routing::CatalogRevision::create(
        None,
        Timestamp::new(1002),
        None,
        ["native-model-a", "native-model-b"]
            .into_iter()
            .map(|model| routing::Candidate {
                identity: routing::ModelEndpoint {
                    model: model.into(),
                    endpoint: "native-endpoint".into(),
                },
                availability: routing::State::Unknown,
                reasons: vec!["private-candidate-reason-marker".into()],
                provenance: vec![routing::Provenance {
                    source: "private-catalog-source-marker".into(),
                    sha256: "c".repeat(64),
                    observed_at: Timestamp::new(1002),
                    effective_at: None,
                    limitations: vec![],
                }],
                capabilities: BTreeMap::new(),
                snapshot: None,
                compatibility: vec![],
                memberships: vec![],
            })
            .collect(),
    )
    .unwrap();
    let mut writer = store
        .spool()
        .create(ArtifactSpec {
            id: ArtifactId::new(),
            scope: task.scope.clone(),
            media_type: "application/json".into(),
            schema: "routing-catalog/1".into(),
            source: "private-raw-catalog-marker".into(),
            channel: Channel::Evidence,
            retention: "history".into(),
            omissions: vec![],
        })
        .unwrap();
    writer
        .write_chunk(&serde_json::to_vec(&catalog).unwrap())
        .unwrap();
    let raw = writer.finalize().unwrap();
    drop(writer);
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put {
                expected: None,
                record: Record::typed(
                    Collection::Artifact,
                    raw.spec.id.as_str(),
                    access.workspace.clone(),
                    Revision::ZERO,
                    &raw,
                )
                .unwrap(),
            }],
            events: vec![],
            command: None,
        })
        .await
        .unwrap();
    let registry = vcp_lifecycle::foundation::routing_state::publish_registry(
        &mut store,
        access,
        None,
        catalog.clone(),
        raw.spec.id,
        Timestamp::new(1002),
    )
    .await
    .unwrap();
    let expected = json!({"authority":workspace.authority.get().to_string(),"actor":caller.actor,"policy":policy,"grants":grants,"routing":routing_policy,"routing_revision":published.revision.get().to_string(),"catalog":catalog,"catalog_revision":registry.revision.get().to_string()});
    (store, expected)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_sdk_inspector_queries_match_governed_cli_queries_without_mutation() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let fixture = Fixture::new(backend).await;
        let input = seed(&fixture).await;
        let before = fixture.reopen().await;
        let state = serde_json::to_value(before.state()).unwrap();
        before.close().await.unwrap();
        driver(input).await;
        let after = fixture.reopen_within(Duration::from_secs(45)).await;
        assert_eq!(
            serde_json::to_value(after.state()).unwrap(),
            state,
            "observer queries cannot mutate canonical state"
        );
        assert_offline_paused(&after, &fixture.config);
        after.close().await.unwrap();
    }
}
