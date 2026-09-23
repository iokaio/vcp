// SPDX-License-Identifier: Apache-2.0
use super::inspect;
use std::collections::BTreeMap;
use vcp_domain::{
    artifact::{ArtifactDescriptor, ArtifactSpec, Channel},
    ids::*,
    memory::*,
    revision::*,
    task::Objective,
    verification::Fingerprint,
    workspace::{Binding, Scope},
};
use vcp_engine::{Engine, HostFacts};
use vcp_memory::{access::Access, repository::propose};
use vcp_protocol::{
    command::{Command, CommandEnvelope},
    event::EventKind,
    memory as wire, methods,
};
use vcp_store::{
    artifact::ArtifactWriter,
    contract::{CanonicalStore, Collection, Mutation, Record, Transaction},
    BackendKind, Store,
};
fn fingerprint() -> Fingerprint {
    Fingerprint {
        repository: "a".repeat(64),
        buffers: "b".repeat(64),
        environment: "c".repeat(64),
    }
}
fn engine_access(workspace: &str) -> vcp_engine::Access {
    vcp_engine::Access {
        actor: ActorId::parse("owner").unwrap(),
        workspace: WorkspaceId::parse(workspace).unwrap(),
        session: SessionId::parse(format!("session-{workspace}")).unwrap(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: true,
    }
}
fn command(
    engine: &Engine<Store>,
    access: &vcp_engine::Access,
    task: Option<TaskId>,
    expected: Revision,
    payload: Command,
) -> CommandEnvelope {
    CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: access.workspace.clone(),
        session: access.session.clone(),
        task,
        caller: access.actor.clone(),
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected,
        steering: SteeringRevision::ZERO,
        payload,
    }
}
fn binding() -> Binding {
    Binding {
        host: HostId::new(),
        root: "C:/synthetic-memory-fixture".into(),
        repository: "repository".into(),
        worktree: "main".into(),
        revision: Revision::ZERO,
    }
}
async fn seed(engine: &mut Engine<Store>, workspace: &str) -> (Scope, EventId, ArtifactDescriptor) {
    let access = engine_access(workspace);
    let facts = HostFacts::inspect(Timestamp::new(100));
    let initialize = command(
        engine,
        &access,
        None,
        Revision::ZERO,
        Command::Initialize { binding: binding() },
    );
    engine.handle(initialize, &access, &facts).await.unwrap();
    let scope = Scope {
        workspace: access.workspace.clone(),
        session: access.session.clone(),
        task: TaskId::new(),
    };
    let create = command(
        engine,
        &access,
        Some(scope.task.clone()),
        Revision::ZERO,
        Command::CreateTask {
            root: scope.task.clone(),
            parent: None,
            fork_origin: None,
            objective: Objective {
                text: r#"{"memory_preference":{"key":"test-output","value":"retain evidence"}}"#
                    .into(),
                constraints: vec![],
                acceptance: vec!["scope preserved".into()],
                source: EventId::new(),
                steering: SteeringRevision::ZERO,
            },
            fingerprint: fingerprint(),
            editing: false,
            required_checks: vec!["cargo-test".into()],
        },
    );
    engine.handle(create, &access, &facts).await.unwrap();
    let origin = engine
        .store()
        .state()
        .events
        .iter()
        .find(|e| {
            e.event.workspace == scope.workspace
                && e.event.task.as_ref() == Some(&scope.task)
                && e.event.kind == EventKind::TaskCreated
        })
        .unwrap()
        .event
        .id
        .clone();
    let mut writer = engine
        .store()
        .spool()
        .create(ArtifactSpec {
            id: ArtifactId::new(),
            scope: scope.clone(),
            media_type: "text/plain".into(),
            schema: "memory-source/1".into(),
            source: "synthetic-governance-fixture".into(),
            channel: Channel::Evidence,
            retention: "history".into(),
            omissions: vec![],
        })
        .unwrap();
    writer
        .write_chunk(b"mod parser; // observed source revision A\n")
        .unwrap();
    let artifact = writer.finalize().unwrap();
    let attach = command(
        engine,
        &access,
        Some(scope.task.clone()),
        Revision::ZERO,
        Command::AttachArtifact {
            descriptor: artifact.clone(),
        },
    );
    engine.handle(attach, &access, &facts).await.unwrap();
    (scope, origin, artifact)
}
struct Fixture {
    store: Store,
    access: Access,
    proposal: Proposal,
}
async fn fixture(path: &std::path::Path, backend: BackendKind) -> Fixture {
    let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
    let (scope, origin, artifact) = seed(&mut engine, "workspace").await;

    let access = Access {
        workspace: scope.workspace.clone(),
        actor: ActorId::parse("owner").unwrap(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        tasks: None,
    };
    let proposal = Proposal {
        id: ProposalId::new(),
        command: CommandId::new(),
        claim: ClaimId::new(),
        scope,
        actor: access.actor.clone(),
        epochs: Epochs {
            authority: AuthorityRevision::ZERO,
            deletion: DeletionEpoch::ZERO,
            policy: PolicyRevision::ZERO,
        },
        registry_version: REGISTRY_VERSION,
        extractor: "deterministic-fixture/1".into(),
        output_key: "architecture-a".into(),
        origins: vec![origin],
        subject: "parser".into(),
        predicate: "architecture".into(),
        statement: "Parser isolates syntax".into(),
        value: ClaimValue::Architecture {
            decision: "Parser isolates syntax".into(),
            rationale: "Observed module source".into(),
            inference: true,
        },
        applicability: Applicability {
            repository: "repository".into(),
            worktree: "main".into(),
            roots: vec![],
            paths: vec!["src/parser.rs".into()],
            symbols: vec![],
            branch: None,
            fingerprint: Some(fingerprint()),
            conditions: BTreeMap::new(),
            valid_from: None,
            valid_until: None,
        },
        evidence: vec![EvidenceRef {
            artifact: artifact.spec.id,
            sha256: artifact.sha256,
            range: Some(vcp_domain::artifact::Range {
                start: ByteCount::new(2),
                end: ByteCount::new(8),
            }),
            source: Some(fingerprint()),
            verification: None,
            kind: EvidenceKind::Source,
        }],
        predecessor: None,
        correction_reason: None,
        retention: "workspace".into(),
    };
    Fixture {
        store: engine.into_store(),
        access,
        proposal,
    }
}
fn request(f: &Fixture) -> methods::MemoryInspect {
    methods::MemoryInspect {
        scope: methods::Scope {
            workspace: f.access.workspace.to_string().try_into().unwrap(),
            session: f.proposal.scope.session.to_string().try_into().unwrap(),
        },
        task: f.proposal.scope.task.to_string().try_into().unwrap(),
        claim: f.proposal.claim.to_string().try_into().unwrap(),
        version: None,
    }
}
#[tokio::test]
async fn governed_public_memory_preserves_resolution_source_status_and_read_scope() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let accepted = propose(
            &mut f.store,
            &f.access,
            f.proposal.clone(),
            Timestamp::new(200),
        )
        .await
        .unwrap();
        let query = request(&f);
        // Governed versions cannot cite one artifact twice, even with distinct
        // ranges. Availability observations therefore identify exactly one ref.
        let mut duplicate = f.proposal.clone();
        let mut second_range = duplicate.evidence[0].clone();
        second_range.range = Some(vcp_domain::artifact::Range {
            start: ByteCount::new(8),
            end: ByteCount::new(12),
        });
        duplicate.evidence.push(second_range);
        assert!(duplicate.validate().is_err());
        let mut access = engine_access("workspace");
        access.write = false;
        let before = f.store.state().watermark;
        let page = inspect(&f.store, &access, &query, &|| Ok(())).unwrap();
        assert!(page.complete);
        assert_eq!(page.findings.len(), 1);
        assert_eq!(page.findings[0].content, f.proposal.statement);
        let state = page.findings[0].state.as_ref().unwrap();
        assert_eq!(state.visibility, wire::Visibility::Retained);
        assert!(state.current && state.applicable);
        assert_eq!(
            state.resolution.as_ref().unwrap().outcome,
            wire::Outcome::Accepted
        );
        assert_eq!(
            state.evidence[0].availability,
            wire::EvidenceAvailability::Available
        );
        assert_eq!(
            page.findings[0].evidence[0].artifact.as_str(),
            f.proposal.evidence[0].artifact.as_str()
        );
        assert_eq!(page.findings[0].evidence[0].offset.as_str(), "2");
        assert_eq!(page.findings[0].evidence[0].length.as_str(), "6");
        assert_eq!(f.store.state().watermark, before);
        let mut explicit = query.clone();
        explicit.version = Some(
            accepted
                .result
                .version
                .unwrap()
                .to_string()
                .try_into()
                .unwrap(),
        );
        assert_eq!(
            inspect(&f.store, &access, &explicit, &|| Ok(())).unwrap(),
            page
        );
        explicit.version = Some("unknown-version".to_owned().try_into().unwrap());
        assert!(inspect(&f.store, &access, &explicit, &|| Ok(())).is_err());
        let mut foreign = query.clone();
        foreign.scope.session = "other-session".to_owned().try_into().unwrap();
        assert!(inspect(&f.store, &access, &foreign, &|| Ok(())).is_err());
        access.authority = AuthorityRevision::new(1);
        assert!(inspect(&f.store, &access, &query, &|| Ok(())).is_err());
        access.authority = AuthorityRevision::ZERO;
        assert!(inspect(&f.store, &access, &query, &|| Err(
            vcp_memory::Error::Conflict("cancelled")
        ))
        .is_err());
        assert_eq!(f.store.state().watermark, before);
        let mut contrary = f.proposal.clone();
        contrary.id = ProposalId::new();
        contrary.command = CommandId::new();
        contrary.claim = ClaimId::new();
        contrary.output_key = "contrary".into();
        contrary.statement = "Parser also owns filesystem operations".into();
        if let ClaimValue::Architecture { decision, .. } = &mut contrary.value {
            *decision = contrary.statement.clone();
        }
        let committed = propose(
            &mut f.store,
            &f.access,
            contrary.clone(),
            Timestamp::new(201),
        )
        .await
        .unwrap();
        assert_eq!(committed.result.resolution.outcome, Outcome::Disputed);
        let mut disputed = query.clone();
        disputed.claim = contrary.claim.to_string().try_into().unwrap();
        let result = inspect(&f.store, &access, &disputed, &|| Ok(())).unwrap();
        let state = result.findings[0].state.as_ref().unwrap();
        assert!(!state.current);
        assert_eq!(
            state.resolution.as_ref().unwrap().outcome,
            wire::Outcome::Disputed
        );
        assert_eq!(state.resolution.as_ref().unwrap().conflicts.len(), 1);
        f.store.close().await.unwrap();
    }
}
#[tokio::test]
async fn logical_retention_never_projects_pruned_content_as_an_empty_retained_claim() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        propose(
            &mut f.store,
            &f.access,
            f.proposal.clone(),
            Timestamp::new(200),
        )
        .await
        .unwrap();
        let query = request(&f);
        let artifact = f.proposal.evidence[0].artifact.clone();
        let attached = f
            .store
            .state()
            .events
            .iter()
            .find(|e| e.event.artifacts.contains(&artifact))
            .unwrap();
        let mut workspace: vcp_domain::workspace::Workspace = f
            .store
            .state()
            .record(
                Collection::Workspace,
                f.access.workspace.as_str(),
                &f.access.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let previous = workspace.revision;
        workspace.revision = previous.next().unwrap();
        workspace.deletion = workspace.deletion.next().unwrap();
        let mask = vcp_domain::retention::RetentionMask {
            schema_version: 1,
            workspace: workspace.id.clone(),
            session: attached.event.session.clone(),
            first: attached.sequence,
            last: attached.sequence,
            artifacts: vec![artifact],
            deletion: workspace.deletion,
            reason: "public inspection retention".into(),
        };
        f.store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: f.store.state().watermark,
                mutations: vec![
                    Mutation::Put {
                        expected: Some(previous),
                        record: Record::typed(
                            Collection::Workspace,
                            workspace.id.as_str(),
                            workspace.id.clone(),
                            workspace.revision,
                            &workspace,
                        )
                        .unwrap(),
                    },
                    Mutation::Put {
                        expected: None,
                        record: Record::typed(
                            Collection::Tombstone,
                            "memory-mask",
                            workspace.id.clone(),
                            Revision::ZERO,
                            &mask,
                        )
                        .unwrap(),
                    },
                ],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        let page = inspect(&f.store, &engine_access("workspace"), &query, &|| Ok(())).unwrap();
        assert!(page.complete);
        let finding = &page.findings[0];
        assert!(finding.content.is_empty() && finding.evidence.is_empty());
        let state = finding.state.as_ref().unwrap();
        assert_eq!(state.visibility, wire::Visibility::Pruned);
        assert!(!state.applicable);
        assert!(state.current);
        assert!(state.resolution.is_none() && state.evidence.is_empty());
        assert!(!serde_json::to_string(&page)
            .unwrap()
            .contains(&f.proposal.statement));
        f.store.close().await.unwrap();
        let reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(
            inspect(&reopened, &engine_access("workspace"), &query, &|| Ok(())).unwrap(),
            page
        );
        reopened.close().await.unwrap();
    }
}

#[tokio::test]
async fn physical_purge_preserves_only_governed_lineage() {
    use vcp_domain::{
        task::{Task, TaskState},
        workspace::Workspace,
    };
    use vcp_store::contract::{CanonicalStore, Mutation, Record, Transaction};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let marker = "unique-purged-memory-marker-918";
        f.proposal.statement = marker.into();
        f.proposal.value = ClaimValue::Architecture {
            decision: marker.into(),
            rationale: marker.into(),
            inference: true,
        };
        let accepted = propose(
            &mut f.store,
            &f.access,
            f.proposal.clone(),
            Timestamp::new(200),
        )
        .await
        .unwrap();
        assert_eq!(accepted.result.resolution.outcome, Outcome::Accepted);
        let mut task: Task = f
            .store
            .state()
            .record(
                Collection::Task,
                f.proposal.scope.task.as_str(),
                &f.access.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let mut workspace: Workspace = f
            .store
            .state()
            .record(
                Collection::Workspace,
                f.access.workspace.as_str(),
                &f.access.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let task_prior = task.revision;
        task.revision = task.revision.next().unwrap();
        task.state = TaskState::Cancelled;
        let workspace_prior = workspace.revision;
        workspace.revision = workspace.revision.next().unwrap();
        workspace.deletion = workspace.deletion.next().unwrap();
        f.store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: f.store.state().watermark,
                events: vec![],
                command: None,
                mutations: vec![
                    Mutation::Put {
                        expected: Some(task_prior),
                        record: Record::typed(
                            Collection::Task,
                            task.scope.task.as_str(),
                            f.access.workspace.clone(),
                            task.revision,
                            &task,
                        )
                        .unwrap(),
                    },
                    Mutation::Put {
                        expected: Some(workspace_prior),
                        record: Record::typed(
                            Collection::Workspace,
                            workspace.id.as_str(),
                            workspace.id.clone(),
                            workspace.revision,
                            &workspace,
                        )
                        .unwrap(),
                    },
                ],
            })
            .await
            .unwrap();
        let mut state = f.store.state().clone();
        for row in state
            .records
            .values_mut()
            .filter(|r| r.workspace == f.access.workspace)
        {
            row.value = match row.value["document_type"].as_str() {
                Some("vcp_memory_proposal_v1") => serde_json::to_value(
                    vcp_protocol::redaction::proposal(
                        &row.decode::<ProposalRecord>().unwrap(),
                        workspace.deletion,
                    )
                    .unwrap(),
                )
                .unwrap(),
                Some("vcp_memory_version_v1") => serde_json::to_value(
                    vcp_protocol::redaction::version(
                        &row.decode::<Version>().unwrap(),
                        workspace.deletion,
                    )
                    .unwrap(),
                )
                .unwrap(),
                Some("vcp_memory_result_v1") => serde_json::to_value(
                    vcp_protocol::redaction::result(
                        &row.decode::<ProposalResult>().unwrap(),
                        workspace.deletion,
                    )
                    .unwrap(),
                )
                .unwrap(),
                _ => row.value.clone(),
            };
        }
        for event in state
            .events
            .iter_mut()
            .filter(|e| e.event.workspace == f.access.workspace)
        {
            *event = vcp_protocol::redaction::event(event, workspace.deletion).unwrap();
        }
        f.store.rewrite_base(state, &[]).await.unwrap();
        let query = request(&f);
        let page = inspect(&f.store, &engine_access("workspace"), &query, &|| Ok(())).unwrap();
        let finding = &page.findings[0];
        assert!(finding.content.is_empty() && finding.evidence.is_empty());
        let state = finding.state.as_ref().unwrap();
        assert_eq!(state.visibility, wire::Visibility::Purged);
        assert!(state.current && !state.applicable);
        assert!(state.resolution.is_none() && state.evidence.is_empty());
        assert!(!serde_json::to_string(&page).unwrap().contains(marker));
        f.store.close().await.unwrap();
        let reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(
            inspect(&reopened, &engine_access("workspace"), &query, &|| Ok(())).unwrap(),
            page
        );
        reopened.close().await.unwrap();
    }
}
