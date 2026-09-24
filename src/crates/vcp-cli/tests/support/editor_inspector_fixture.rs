// SPDX-License-Identifier: Apache-2.0
//! Synthetic canonical source for actual editor inspectors, seeded through the
//! same governed services as native CLI/SDK qualification. No provider runs.
use super::local_fixture::Fixture;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
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
};
pub(super) async fn seed(fixture: &Fixture) -> serde_json::Value {
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
            media_type: "application/json".into(),
            schema: "context-manifest/1".into(),
            source: "offline compiled memory fixture".into(),
            channel: Channel::Evidence,
            retention: "history".into(),
            omissions: vec![],
        })
        .unwrap();
    let content = "<script>hostile()</script> command:workbench.action.quit file:///C:/private javascript:alert(1) inspector-native-sentinel\n".repeat(700);
    for chunk in context_manifest(&task, &content).chunks(16384) {
        writer.write_chunk(chunk).unwrap();
    }
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
    // A separate retained artifact has no claim provenance or active source pin.
    // Its exact event path can be purged without weakening protected-source rules.
    let mut retained = store
        .spool()
        .create(ArtifactSpec {
            id: ArtifactId::new(),
            scope: task.scope.clone(),
            media_type: "application/json".into(),
            schema: "context-manifest/1".into(),
            source: "standalone inspector retention fixture".into(),
            channel: Channel::Evidence,
            retention: "history".into(),
            omissions: vec![],
        })
        .unwrap();
    let content = "retention-inspector-sentinel command:workbench.action.quit\n".repeat(400);
    for chunk in context_manifest(&task, &content).chunks(16384) {
        retained.write_chunk(chunk).unwrap();
    }
    let retained_artifact = retained.finalize().unwrap();
    drop(retained);
    let retained = retained_artifact;
    let retention_artifact = retained.spec.id.clone();
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put {
                expected: None,
                record: Record::typed(
                    Collection::Artifact,
                    retained.spec.id.as_str(),
                    task.scope.workspace.clone(),
                    Revision::ZERO,
                    &retained,
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
                timestamp: Timestamp::new(100),
                kind: vcp_protocol::event::EventKind::Diagnostic,
                artifacts: vec![retention_artifact.clone()],
                data: json!({"schema_version":1,"diagnostic":"standalone-retention"}),
                metadata: Some(vcp_protocol::event::EventMetadata {
                    agent: None,
                    provider: None,
                    model: None,
                    paths: vec!["src/retention-only".into()],
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
        next.statement = format!(
            "Parser revision {index}; command:workbench.action.quit inspector-native-sentinel"
        );
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
    // Build the actual derived lexical generation consumed by memory/query.
    // History itself does not require an index; finding-to-version navigation does.
    let inventory = vcp_memory::search_record::inventory(
        &store,
        &access,
        &[],
        &vcp_memory::search_record::ChunkerSpec::default(),
        vcp_memory::search_record::Limits::default(),
    )
    .unwrap();
    let publisher = vcp_memory::publication::Publisher::new(
        &fixture.config.canonical_root.join("search-generations"),
    )
    .unwrap();
    let prepared = publisher
        .prepare(
            vcp_memory::publication::capture(&store, &access, &task.scope, inventory).unwrap(),
            None,
            &std::sync::atomic::AtomicBool::new(false),
            &|_| {},
        )
        .unwrap();
    publisher
        .publish(
            &mut store,
            &access,
            &prepared,
            Timestamp::new(1003),
            &|_| {},
        )
        .await
        .unwrap();
    assert!(
        !publisher
            .recover(&store, &access)
            .unwrap()
            .view
            .unwrap()
            .lexical
            .search(&vcp_memory::lexical::Query {
                workspace: access.workspace.clone(),
                tasks: Some(vec![task.scope.task.clone()]),
                roots: None,
                paths: None,
                symbols: None,
                kind: None,
                claim_kind: None,
                status: None,
                text: "Parser".into(),
                phrase: false,
                limit: 16
            })
            .unwrap()
            .is_empty(),
        "real published generation must return the fixture claim"
    );
    let scoped = vcp_memory::access::Access {
        workspace: access.workspace.clone(),
        actor: access.actor.clone(),
        authority: access.authority,
        read: access.read,
        write: access.write,
        tasks: Some(BTreeSet::from([task.scope.task.clone()])),
    };
    let probe = vcp_memory::retention_public::preview(&store,&scoped,task.scope.clone(),
        Selector { schema_version:1, tree:Tree::Match(Criterion::Path("src/retention-only".into())) },
        vcp_memory::retention::Action::Purge, Timestamp::new(1004),
    ).expect("valid synthetic manifests and exact retained path must permit a real purge preview before GUI launch");
    assert!(!probe.selection().selected.is_empty());
    assert!(
        !probe.selection().protected.is_empty(),
        "paused recovery must remain protected"
    );
    let mut engine = vcp_engine::Engine::new(store).unwrap();
    let caller = vcp_engine::Access {
        actor: access.actor.clone(),
        workspace: access.workspace.clone(),
        session: task.scope.session.clone(),
        authority: access.authority,
        read: true,
        write: true,
        bootstrap: false,
    };
    engine
        .handle(
            vcp_protocol::command::CommandEnvelope {
                version: 1,
                id: CommandId::new(),
                workspace: caller.workspace.clone(),
                session: caller.session.clone(),
                task: Some(task.scope.task.clone()),
                caller: caller.actor.clone(),
                controller: engine.controller().clone(),
                owner_epoch: engine.owner_epoch(),
                expected: task.revision,
                steering: task.steering,
                payload: vcp_protocol::command::Command::Transition {
                    next: vcp_domain::task::TaskState::Cancelled,
                    reason:
                        "finish offline fixture recovery before eligible retention qualification"
                            .into(),
                    verification: None,
                },
            },
            &caller,
            &vcp_engine::HostFacts::inspect(Timestamp::new(1005)),
        )
        .await
        .unwrap();
    store = engine.into_store();
    let eligible = vcp_memory::retention_public::preview(
        &store,
        &scoped,
        task.scope.clone(),
        Selector {
            schema_version: 1,
            tree: Tree::Match(Criterion::Path("src/retention-only".into())),
        },
        vcp_memory::retention::Action::Purge,
        Timestamp::new(1006),
    )
    .expect("terminal independent source permits governed purge preview");
    assert!(!eligible.selection().selected.is_empty());
    assert!(eligible.selection().protected.is_empty());
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
    let input = json!({"executable":env!("CARGO_BIN_EXE_vcp"),"workspace":fixture.workspace,"data":fixture.data,"scope":fixture.scope(),"task":fixture.config.root_task,"claim":proposal.claim,"artifact":artifact.spec.id,"retention_artifact":retention_artifact,"origin":origin,"versions":versions,"history":history,"watermark":store.state().watermark.get().to_string(),"inspection":inspection});
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
    let mut trust_command = envelope(
        &engine,
        &caller,
        EngineCommand::SetWorkspaceTrust {
            trust: vcp_domain::workspace::Trust::Trusted,
        },
    );
    trust_command.expected = workspace.revision;
    engine
        .handle(
            trust_command,
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

/// A complete synthetic context-manifest shape. Hostile text is excluded content,
/// never an instruction or fabricated engine reply. Retention scans its real
/// included-source and request commitment fields, even though this fixture does
/// not dispatch a provider request.
fn context_manifest(task: &Task, text: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "version":1,
        "revisions":{"scope":task.scope,"steering":task.steering,"policy":0,"authority":0,"deletion":0,"binding":0,"instructions":0,"tools":0,"skills":0,"memory":0,"task_state":task.revision},
        "envelope":{"model":"offline-fixture","catalog":"offline-fixture","compatibility":"offline-fixture","context":1000000,"output":1,"overhead":0,"margin":0,"supports_tools":false,"preserves_trust":true},
        "included":[],"excluded":[{"id":"synthetic-untrusted-text","reason":text,"start":0,"end":text.len()}],
        "instruction_probes":[],"schemas_sha256":vcp_protocol::digest_bytes(b"[]"),"request_sha256":vcp_protocol::digest_bytes(b"{}"),
        "input_estimate":0,"estimate_method":"offline fixture, no provider dispatch","estimated":true
    })).unwrap()
}
