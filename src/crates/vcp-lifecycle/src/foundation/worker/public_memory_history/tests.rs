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
fn request(f: &Fixture) -> vcp_protocol::memory_history::Request {
    vcp_protocol::memory_history::Request {
        scope: methods::Scope {
            workspace: f.access.workspace.to_string().try_into().unwrap(),
            session: f.proposal.scope.session.to_string().try_into().unwrap(),
        },
        task: f.proposal.scope.task.to_string().try_into().unwrap(),
        claim: f.proposal.claim.to_string().try_into().unwrap(),
        limit: 32,
        cursor: None,
    }
}

async fn append(f: &mut Fixture, previous: Option<ClaimVersionId>, index: u64) -> ClaimVersionId {
    let mut proposal = f.proposal.clone();
    proposal.id = ProposalId::new();
    proposal.command = CommandId::new();
    proposal.output_key = format!("history-{index}");
    proposal.predecessor = previous;
    proposal.correction_reason = proposal
        .predecessor
        .as_ref()
        .map(|_| "explicit correction".into());
    proposal.statement = format!("Parser version {index} {}", "é".repeat(2500));
    if let ClaimValue::Architecture { decision, .. } = &mut proposal.value {
        *decision = proposal.statement.clone();
    }
    propose(
        &mut f.store,
        &f.access,
        proposal,
        Timestamp::new(200 + index),
    )
    .await
    .unwrap()
    .result
    .version
    .unwrap()
}
#[tokio::test]
async fn large_history_pages_match_cli_window_and_freeze_boundary_without_writes() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let mut ids = Vec::new();
        let mut previous = None;
        for index in 0..130 {
            let id = append(&mut f, previous, index).await;
            previous = Some(id.clone());
            ids.push(id);
        }
        let access = engine_access("workspace");
        let mut query = request(&f);
        let first = inspect(&f.store, &access, &query, &|| Ok(())).unwrap();
        assert!(!first.versions.is_empty() && first.versions.len() < 32);
        assert!(!first.complete);
        let (cli, at, _) = vcp_memory::history::window(
            &f.store,
            &f.access,
            &f.proposal.claim,
            None,
            MemorySeq::ZERO,
            32,
        )
        .unwrap();
        for (public, cli) in first.versions.iter().zip(&cli.versions) {
            assert_eq!(public.finding.version.as_str(), cli.id.as_str());
            assert!(public.content_truncated);
            assert!(public.finding.content.len() <= 4096);
            assert!(cli
                .version
                .as_ref()
                .unwrap()
                .proposal
                .statement
                .starts_with(&public.finding.content));
            assert_eq!(public.finding.state.as_ref().unwrap().current, cli.current);
            assert_eq!(public.finding.evidence[0].offset.as_str(), "2");
            assert_eq!(public.origins[0].as_str(), f.proposal.origins[0].as_str());
        }
        append(&mut f, previous, 130).await;
        let before = f.store.state().watermark;
        let mut seen = first
            .versions
            .iter()
            .map(|v| v.finding.version.as_str().to_owned())
            .collect::<Vec<_>>();
        query.cursor = first.next_cursor;
        while query.cursor.is_some() {
            let page = inspect(&f.store, &access, &query, &|| Ok(())).unwrap();
            assert_eq!(page.at.as_str(), at.get().to_string());
            assert!(!page.versions.is_empty());
            assert!(serde_json::to_vec(&page).unwrap().len() < 65536);
            seen.extend(
                page.versions
                    .iter()
                    .map(|v| v.finding.version.as_str().to_owned()),
            );
            query.cursor = page.next_cursor;
        }
        assert_eq!(
            seen,
            ids.iter().map(ToString::to_string).collect::<Vec<_>>()
        );
        assert_eq!(before, f.store.state().watermark);
        f.store.close().await.unwrap();
    }
}
#[tokio::test]
async fn continuation_rechecks_actor_scope_authority_retention_and_interruption() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let first = append(&mut f, None, 0).await;
        append(&mut f, Some(first), 1).await;
        let access = engine_access("workspace");
        let mut query = request(&f);
        query.limit = 1;
        // Also hide taskless origins from other sessions, beyond governed task access.
        let mut source_state = f.store.state().clone();
        let origin = &f.proposal.origins[0];
        assert_eq!(
            super::visible_origins(&source_state, &access, std::slice::from_ref(origin))
                .unwrap()
                .len(),
            1
        );
        let event = source_state
            .events
            .iter_mut()
            .find(|event| &event.event.id == origin)
            .unwrap();
        event.event.session = SessionId::parse("foreign-session").unwrap();
        event.event.task = None;
        assert!(
            super::visible_origins(&source_state, &access, std::slice::from_ref(origin))
                .unwrap()
                .is_empty()
        );
        query.cursor = inspect(&f.store, &access, &query, &|| Ok(()))
            .unwrap()
            .next_cursor;
        assert!(query.cursor.is_some());
        let mut foreign = access.clone();
        foreign.actor = ActorId::parse("other").unwrap();
        assert!(inspect(&f.store, &foreign, &query, &|| Ok(())).is_err());
        foreign = access.clone();
        foreign.read = false;
        assert!(inspect(&f.store, &foreign, &query, &|| Ok(())).is_err());
        foreign = access.clone();
        foreign.authority = AuthorityRevision::new(1);
        assert!(inspect(&f.store, &foreign, &query, &|| Ok(())).is_err());
        let mut other = query.clone();
        other.scope.session = "other".to_owned().try_into().unwrap();
        assert!(inspect(&f.store, &access, &other, &|| Ok(())).is_err());
        other = query.clone();
        other.claim = "other".to_owned().try_into().unwrap();
        assert!(inspect(&f.store, &access, &other, &|| Ok(())).is_err());
        other.cursor = None;
        assert!(inspect(&f.store, &access, &other, &|| Ok(())).is_err());
        assert!(inspect(&f.store, &access, &query, &|| Err(
            vcp_memory::Error::Conflict("cancelled")
        ))
        .is_err());
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
            reason: "paged retention".into(),
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
                            "paged-mask",
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
        assert!(inspect(&f.store, &access, &query, &|| Ok(())).is_err());
        query.cursor = None;
        let page = inspect(&f.store, &access, &query, &|| Ok(())).unwrap();
        assert_eq!(
            page.versions[0].finding.state.as_ref().unwrap().visibility,
            wire::Visibility::Pruned
        );
        assert!(page.versions[0].finding.content.is_empty());
        assert!(page.versions[0].finding.evidence.is_empty());
        assert!(page.versions[0].origins.is_empty());
        f.store.close().await.unwrap();
        let reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(
            inspect(&reopened, &access, &query, &|| Ok(())).unwrap(),
            page
        );
        reopened.close().await.unwrap();
    }
}
