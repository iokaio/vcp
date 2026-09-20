// SPDX-License-Identifier: Apache-2.0
//! Both canonical adapters must retain the same governed-write invariants.
use std::collections::BTreeMap;
use vcp_domain::{
    artifact::{ArtifactDescriptor, ArtifactSpec, Channel},
    ids::*,
    memory::*,
    revision::*,
    task::Objective,
    verification::{Check, CheckOutcome, CostCertainty, Fingerprint, Verification},
    workspace::{Binding, Scope},
};
use vcp_engine::{Engine, HostFacts};
use vcp_memory::{access::Access, history, repository::propose};
use vcp_protocol::{
    command::{Command, CommandEnvelope},
    event::EventKind,
};
use vcp_store::{
    artifact::ArtifactWriter,
    contract::{Collection, State},
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
async fn capture(
    engine: &mut Engine<Store>,
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
            media_type: "application/json".into(),
            schema: schema.into(),
            source: "synthetic-governance-fixture".into(),
            channel: Channel::Evidence,
            retention: "history".into(),
            omissions: vec![],
        })
        .unwrap();
    writer.write_chunk(bytes).unwrap();
    let descriptor = writer.finalize().unwrap();
    let access = engine_access(scope.workspace.as_str());
    let attach = command(
        engine,
        &access,
        Some(scope.task.clone()),
        Revision::ZERO,
        Command::AttachArtifact {
            descriptor: descriptor.clone(),
        },
    );
    engine
        .handle(attach, &access, &HostFacts::inspect(Timestamp::new(149)))
        .await
        .unwrap();
    descriptor
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
    foreign: ArtifactDescriptor,
}
async fn fixture(path: &std::path::Path, backend: BackendKind) -> Fixture {
    let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
    let (scope, origin, artifact) = seed(&mut engine, "workspace").await;
    let (_, _, foreign) = seed(&mut engine, "foreign").await;
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
            range: None,
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
        foreign,
    }
}
fn different_output(proposal: &Proposal, key: &str) -> Proposal {
    let mut result = proposal.clone();
    result.id = ProposalId::new();
    result.command = CommandId::new();
    result.output_key = key.into();
    result
}
fn versions(state: &State, workspace: &WorkspaceId) -> Vec<Version> {
    state
        .records
        .values()
        .filter(|r| {
            &r.workspace == workspace
                && r.collection == Collection::Claim
                && r.value["document_type"] == "vcp_memory_version_v1"
        })
        .map(|r| r.decode().unwrap())
        .collect()
}
fn head(store: &Store, proposal: &Proposal) -> Head {
    store
        .state()
        .record(
            Collection::Projection,
            proposal.claim.as_str(),
            &proposal.scope.workspace,
        )
        .unwrap()
        .decode()
        .unwrap()
}

#[tokio::test]
async fn lost_ack_reopen_returns_identical_receipt_and_pending_intent_without_duplicate_history() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let before_events = f.store.state().events.len();
        let first = propose(
            &mut f.store,
            &f.access,
            f.proposal.clone(),
            Timestamp::new(200),
        )
        .await
        .unwrap();
        assert_eq!(first.result.resolution.outcome, Outcome::Accepted);
        assert_eq!(
            first.result.resolution.evidence_status,
            EvidenceStatus::Inferred
        );
        assert_eq!(first.indexing, Some(IndexStatus::Pending));
        assert_eq!(f.store.state().events.len(), before_events + 1);
        let watermark = f.store.state().watermark;
        let original_state = f.store.state().clone();
        drop(f.store);
        let mut reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        let retry = propose(
            &mut reopened,
            &f.access,
            f.proposal.clone(),
            Timestamp::new(999),
        )
        .await
        .unwrap();
        assert_eq!(retry.receipt, first.receipt);
        assert_eq!(retry.result, first.result);
        assert_eq!(reopened.state().watermark, watermark);
        assert_eq!(reopened.state(), &original_state);
        assert_eq!(versions(reopened.state(), &f.access.workspace).len(), 1);
        let intent: IndexIntent = reopened
            .state()
            .record(
                Collection::IndexIntent,
                first.result.intent.as_ref().unwrap().as_str(),
                &f.access.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(intent.versions, vec![first.result.version.unwrap()]);
        assert_eq!(intent.memory_seq, first.result.memory_seq);
        assert_eq!(intent.transaction, first.receipt.transaction);
        assert_eq!(intent.status, IndexStatus::Pending);
    }
}

#[tokio::test]
async fn payload_and_origin_output_identity_collisions_cannot_overwrite_a_receipt() {
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
        let before = f.store.state().clone();
        let mut changed = f.proposal.clone();
        changed.statement = "Different payload under the same identity".into();
        assert!(
            propose(&mut f.store, &f.access, changed, Timestamp::new(201))
                .await
                .is_err()
        );
        let mut duplicate_origin = f.proposal.clone();
        duplicate_origin.id = ProposalId::new();
        duplicate_origin.command = CommandId::new();
        assert!(propose(
            &mut f.store,
            &f.access,
            duplicate_origin,
            Timestamp::new(202)
        )
        .await
        .is_err());
        assert_eq!(f.store.state(), &before);
    }
}

#[tokio::test]
async fn missing_and_foreign_evidence_are_durable_rejections_without_foreign_references() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let raw_event_ids: Vec<_> = f
            .store
            .state()
            .events
            .iter()
            .map(|e| e.event.id.clone())
            .collect();
        let mut rejected_ids = Vec::new();
        for foreign in [false, true] {
            let mut proposal =
                different_output(&f.proposal, if foreign { "foreign" } else { "missing" });
            proposal.evidence[0].artifact = if foreign {
                f.foreign.spec.id.clone()
            } else {
                ArtifactId::new()
            };
            if foreign {
                proposal.evidence[0].sha256 = f.foreign.sha256.clone();
            }
            let result = propose(
                &mut f.store,
                &f.access,
                proposal.clone(),
                Timestamp::new(200),
            )
            .await
            .unwrap();
            assert_eq!(result.result.resolution.outcome, Outcome::Rejected);
            assert!(result.result.resolution.validated_evidence.is_empty());
            assert!(result.result.version.is_none());
            assert!(result.result.intent.is_none());
            let row = f
                .store
                .state()
                .record(Collection::Claim, proposal.id.as_str(), &f.access.workspace)
                .unwrap();
            let persisted: ProposalRecord = row.decode().unwrap();
            assert_eq!(persisted.proposal.evidence, proposal.evidence);
            assert!(!row
                .required_references()
                .unwrap()
                .contains(&vcp_store::contract::key(
                    Collection::Artifact,
                    proposal.evidence[0].artifact.as_str()
                )));
            rejected_ids.push(proposal.id);
        }
        assert!(versions(f.store.state(), &f.access.workspace).is_empty());
        assert!(raw_event_ids.iter().all(|id| f
            .store
            .state()
            .events
            .iter()
            .any(|e| &e.event.id == id)));
        drop(f.store);
        let reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        for id in rejected_ids {
            let record: ProposalRecord = reopened
                .state()
                .record(Collection::Claim, id.as_str(), &f.access.workspace)
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(record.resolution.outcome, Outcome::Rejected);
        }
    }
}

#[tokio::test]
async fn contradictory_inference_is_disputed_without_replacing_accepted_head() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let first = propose(
            &mut f.store,
            &f.access,
            f.proposal.clone(),
            Timestamp::new(200),
        )
        .await
        .unwrap();
        let accepted_head = head(&f.store, &f.proposal);
        let mut contrary = different_output(&f.proposal, "architecture-contradiction");
        contrary.claim = ClaimId::new();
        contrary.statement = "Parser also owns filesystem operations".into();
        if let ClaimValue::Architecture { decision, .. } = &mut contrary.value {
            *decision = contrary.statement.clone();
        }
        let second = propose(
            &mut f.store,
            &f.access,
            contrary.clone(),
            Timestamp::new(201),
        )
        .await
        .unwrap();
        assert_eq!(second.result.resolution.outcome, Outcome::Disputed);
        assert_eq!(
            second.result.resolution.evidence_status,
            EvidenceStatus::Inferred
        );
        assert!(second
            .result
            .resolution
            .conflicts
            .contains(first.result.version.as_ref().unwrap()));
        assert_eq!(head(&f.store, &f.proposal), accepted_head);
        let disputed_head = head(&f.store, &contrary);
        assert!(disputed_head.current.is_none());
        assert_eq!(disputed_head.disputed, vec![second.result.version.unwrap()]);
        assert_eq!(versions(f.store.state(), &f.access.workspace).len(), 2);
    }
}

#[tokio::test]
async fn correction_requires_current_predecessor_and_preserves_immutable_historical_version() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let first = propose(
            &mut f.store,
            &f.access,
            f.proposal.clone(),
            Timestamp::new(200),
        )
        .await
        .unwrap();
        let old_id = first.result.version.clone().unwrap();
        let old_record = f
            .store
            .state()
            .record(Collection::Claim, old_id.as_str(), &f.access.workspace)
            .unwrap()
            .clone();
        let mut correction = different_output(&f.proposal, "bad-correction");
        correction.predecessor = Some(ClaimVersionId::new());
        correction.correction_reason = Some("User clarified parser responsibility".into());
        correction.statement = "Parser owns token normalization".into();
        if let ClaimValue::Architecture { decision, .. } = &mut correction.value {
            *decision = correction.statement.clone();
        }
        let rejected = propose(
            &mut f.store,
            &f.access,
            correction.clone(),
            Timestamp::new(201),
        )
        .await
        .unwrap();
        assert_eq!(rejected.result.resolution.outcome, Outcome::Rejected);
        assert_eq!(head(&f.store, &f.proposal).current, Some(old_id.clone()));
        correction = different_output(&correction, "good-correction");
        correction.predecessor = Some(old_id.clone());
        let accepted = propose(&mut f.store, &f.access, correction, Timestamp::new(202))
            .await
            .unwrap();
        assert_eq!(accepted.result.resolution.outcome, Outcome::Accepted);
        assert_eq!(head(&f.store, &f.proposal).current, accepted.result.version);
        assert_eq!(
            f.store
                .state()
                .record(Collection::Claim, old_id.as_str(), &f.access.workspace)
                .unwrap(),
            &old_record
        );
        let historical = history::query(
            &f.store,
            &f.access,
            &f.proposal.claim,
            Some(first.result.memory_seq),
            None,
        )
        .unwrap();
        assert_eq!(historical.versions.len(), 1);
        assert_eq!(historical.versions[0].id, old_id);
        assert_eq!(
            historical.versions[0]
                .version
                .as_ref()
                .unwrap()
                .proposal
                .statement,
            f.proposal.statement
        );
    }
}

#[tokio::test]
async fn historical_reads_recheck_current_authority_and_source_applicability() {
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
        let current = fingerprint();
        let same = history::query(
            &f.store,
            &f.access,
            &f.proposal.claim,
            Some(accepted.result.memory_seq),
            Some(&current),
        )
        .unwrap();
        assert!(same.versions[0].applicable);
        let changed = Fingerprint {
            repository: "d".repeat(64),
            ..current
        };
        let stale = history::query(
            &f.store,
            &f.access,
            &f.proposal.claim,
            Some(accepted.result.memory_seq),
            Some(&changed),
        )
        .unwrap();
        assert!(!stale.versions[0].applicable);
        assert!(stale.versions[0].version.is_some());
        let mut engine = Engine::new(f.store).unwrap();
        let access = engine_access("workspace");
        let rebind = command(
            &engine,
            &access,
            None,
            Revision::ZERO,
            Command::Rebind { binding: binding() },
        );
        engine
            .handle(rebind, &access, &HostFacts::inspect(Timestamp::new(300)))
            .await
            .unwrap();
        let store = engine.into_store();
        assert!(history::query(
            &store,
            &f.access,
            &f.proposal.claim,
            Some(accepted.result.memory_seq),
            None
        )
        .is_err());
        let refreshed = Access {
            authority: AuthorityRevision::new(1),
            ..f.access
        };
        let history = history::query(
            &store,
            &refreshed,
            &f.proposal.claim,
            Some(accepted.result.memory_seq),
            None,
        )
        .unwrap();
        assert_eq!(history.versions.len(), 1);
    }
}

#[tokio::test]
async fn derived_heads_rebuild_from_immutable_records_without_reusing_rejected_sequence() {
    use vcp_store::contract::{CanonicalStore, Mutation, Transaction};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let first = propose(
            &mut f.store,
            &f.access,
            f.proposal.clone(),
            Timestamp::new(200),
        )
        .await
        .unwrap();
        let original_head = head(&f.store, &f.proposal);
        let mut invalid = different_output(&f.proposal, "rejected-before-rebuild");
        invalid.evidence[0].artifact = ArtifactId::new();
        let rejected = propose(&mut f.store, &f.access, invalid, Timestamp::new(201))
            .await
            .unwrap();
        assert_eq!(rejected.result.resolution.outcome, Outcome::Rejected);
        assert!(rejected.result.memory_seq > first.result.memory_seq);
        let sequence: MemoryHead = f
            .store
            .state()
            .record(
                Collection::Projection,
                f.access.workspace.as_str(),
                &f.access.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        f.store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: f.store.state().watermark,
                mutations: vec![
                    Mutation::DropProjection {
                        id: original_head.id.to_string(),
                        expected: original_head.revision,
                    },
                    Mutation::DropProjection {
                        id: sequence.id.to_string(),
                        expected: sequence.revision,
                    },
                ],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        let before_rebuild = f.store.state().watermark;
        assert!(
            vcp_memory::projections::rebuild(&mut f.store, &f.access, Timestamp::new(202))
                .await
                .unwrap()
                .is_some()
        );
        assert!(f.store.state().watermark > before_rebuild);
        assert_eq!(head(&f.store, &f.proposal).current, original_head.current);
        let rebuilt: MemoryHead = f
            .store
            .state()
            .record(
                Collection::Projection,
                f.access.workspace.as_str(),
                &f.access.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(rebuilt.sequence, rejected.result.memory_seq);
        let watermark = f.store.state().watermark;
        assert!(
            vcp_memory::projections::rebuild(&mut f.store, &f.access, Timestamp::new(203))
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(f.store.state().watermark, watermark);
        let retried = propose(
            &mut f.store,
            &f.access,
            f.proposal.clone(),
            Timestamp::new(204),
        )
        .await
        .unwrap();
        assert_eq!(retried.receipt, first.receipt);
        assert_eq!(retried.result, first.result);
        let historical = history::query(
            &f.store,
            &f.access,
            &f.proposal.claim,
            Some(first.result.memory_seq),
            None,
        )
        .unwrap();
        assert_eq!(historical.versions.len(), 1);
        let mut correction = different_output(&f.proposal, "after-rebuild");
        correction.predecessor = first.result.version;
        correction.correction_reason = Some("Reaffirm current source after rebuild".into());
        let next = propose(&mut f.store, &f.access, correction, Timestamp::new(205))
            .await
            .unwrap();
        assert_eq!(next.result.resolution.outcome, Outcome::Accepted);
        assert_eq!(
            next.result.memory_seq,
            rejected.result.memory_seq.next().unwrap()
        );
    }
}

#[tokio::test]
async fn retained_historical_sequence_cannot_restore_pruned_claim_payload() {
    use vcp_audit::history::RetentionMask;
    use vcp_domain::workspace::Workspace;
    use vcp_store::contract::{CanonicalStore, Mutation, Record, Transaction};
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
        let artifact = f.proposal.evidence[0].artifact.clone();
        let attached = f
            .store
            .state()
            .events
            .iter()
            .find(|e| e.event.artifacts.contains(&artifact))
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
        let previous = workspace.revision;
        workspace.revision = workspace.revision.next().unwrap();
        workspace.deletion = workspace.deletion.next().unwrap();
        let mask = RetentionMask {
            schema_version: 1,
            workspace: workspace.id.clone(),
            session: attached.event.session.clone(),
            first: attached.sequence,
            last: attached.sequence,
            artifacts: vec![artifact],
            deletion: workspace.deletion,
            reason: "Explicit fixture retention restriction".into(),
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
                            "memory-evidence-mask",
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
        let hidden = history::query(
            &f.store,
            &f.access,
            &f.proposal.claim,
            Some(accepted.result.memory_seq),
            None,
        )
        .unwrap();
        assert_eq!(hidden.versions.len(), 1);
        assert_eq!(
            hidden.versions[0].id,
            accepted.result.version.clone().unwrap()
        );
        assert_eq!(hidden.versions[0].visibility, "pruned");
        assert!(hidden.versions[0].version.is_none());
        assert!(hidden.versions[0].evidence.is_empty());
        assert!(!hidden.versions[0].applicable);
        let serialized = serde_json::to_string(&hidden).unwrap();
        assert!(!serialized.contains(&f.proposal.statement));
        drop(f.store);
        let reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        let hidden = history::query(
            &reopened,
            &f.access,
            &f.proposal.claim,
            Some(accepted.result.memory_seq),
            None,
        )
        .unwrap();
        assert!(hidden.versions[0].version.is_none());
        assert_eq!(hidden.versions[0].visibility, "pruned");
    }
}

#[tokio::test]
async fn all_six_classes_accept_scoped_evidence_and_verified_results_expire_with_source_revision() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let mut source = f.proposal.evidence[0].clone();
        let scope = f.proposal.scope.clone();
        let access = engine_access("workspace");
        let mut engine = Engine::new(f.store).unwrap();
        let base_manifest = serde_json::json!({"identity":{"workspace":scope.workspace},"bounded_scan_complete":true,"files":[{"path":"src/lib.rs","sha256":"d".repeat(64)}]});
        let current_manifest = serde_json::json!({"identity":{"workspace":scope.workspace},"bounded_scan_complete":true,"files":[{"path":"src/lib.rs","sha256":source.sha256}]});
        let before = Fingerprint {
            repository: vcp_protocol::digest_bytes(
                &vcp_protocol::canonical_bytes(&base_manifest).unwrap(),
            ),
            ..fingerprint()
        };
        let after = Fingerprint {
            repository: vcp_protocol::digest_bytes(
                &vcp_protocol::canonical_bytes(&current_manifest).unwrap(),
            ),
            ..fingerprint()
        };
        source.source = Some(after.clone());
        f.proposal.evidence[0] = source.clone();
        f.proposal.applicability.fingerprint = Some(after.clone());
        let observe = command(
            &engine,
            &access,
            Some(scope.task.clone()),
            Revision::ZERO,
            Command::ObserveFingerprint {
                fingerprint: after.clone(),
            },
        );
        engine
            .handle(observe, &access, &HostFacts::inspect(Timestamp::new(148)))
            .await
            .unwrap();
        let check = capture(&mut engine, &scope, "verification-check/1", &serde_json::to_vec(&serde_json::json!({
            "plan": {"runner":"cargo", "origin":{"sha256":source.sha256}, "directory":"", "request":{"arguments":["test"],"directory":""}, "not_run":null},
            "applicability":"current", "outcome":{"status":"passed"}, "exit_code":0
        })).unwrap()).await;
        let verification = Verification {
            id: VerificationId::new(),
            scope: scope.clone(),
            steering: SteeringRevision::ZERO,
            fingerprint: after.clone(),
            outputs: vec![source.artifact.clone()],
            checks: vec![Check {
                specification: "cargo-test".into(),
                outcome: CheckOutcome::Passed,
                output: check.spec.id.clone(),
                exit_code: Some(0),
            }],
            unresolved_effects: vec![],
            outstanding_issues: vec![],
            cost: CostCertainty::Known,
        };
        let patch = capture(
            &mut engine,
            &scope,
            "vcp-memory-change/1",
            &serde_json::to_vec(
                &serde_json::json!({"base":base_manifest,"current":current_manifest}),
            )
            .unwrap(),
        )
        .await;
        let record = command(
            &engine,
            &access,
            Some(scope.task.clone()),
            Revision::new(1),
            Command::RecordVerification {
                verification: verification.clone(),
            },
        );
        engine
            .handle(record, &access, &HostFacts::inspect(Timestamp::new(152)))
            .await
            .unwrap();
        f.store = engine.into_store();
        let mut verifying = source.clone();
        verifying.artifact = check.spec.id;
        verifying.sha256 = check.sha256;
        verifying.kind = EvidenceKind::Verification;
        verifying.verification = Some(verification.id.clone());
        let root = RootId::parse(f.access.workspace.as_str()).unwrap();
        let endpoint = SourceEndpoint {
            root: root.clone(),
            path: "src/parser.rs".into(),
            symbol: Some("Parser".into()),
            artifact: source.artifact.clone(),
            sha256: source.sha256.clone(),
        };
        let mut values = vec![
            (
                "command",
                ClaimValue::Command {
                    purpose: CommandPurpose::Test,
                    argv: vec!["cargo".into(), "test".into()],
                    cwd: ".".into(),
                    configuration: source.artifact.clone(),
                    outcome: Some(CheckOutcome::Passed),
                    verification: Some(verification.id.clone()),
                },
            ),
            (
                "module",
                ClaimValue::ModuleRelationship {
                    from: endpoint.clone(),
                    relation: "imports".into(),
                    to: SourceEndpoint {
                        path: "src/token.rs".into(),
                        ..endpoint
                    },
                },
            ),
            ("architecture", f.proposal.value.clone()),
            (
                "environment",
                ClaimValue::EnvironmentConstraint {
                    component: "rust".into(),
                    requirement: "stable".into(),
                    observed_value: Some("stable".into()),
                },
            ),
            (
                "fix",
                ClaimValue::VerifiedFix {
                    issue: "parser panic".into(),
                    patch: patch.spec.id.clone(),
                    verification: verification.id.clone(),
                    before: before.clone(),
                    after: after.clone(),
                },
            ),
            (
                "preference",
                ClaimValue::UserPreference {
                    key: "test-output".into(),
                    value: "retain evidence".into(),
                    explicit_origin: f.proposal.origins[0].clone(),
                },
            ),
        ];
        let mut verified_proposals = Vec::new();
        for (name, value) in values.drain(..) {
            let mut proposal = different_output(&f.proposal, name);
            proposal.claim = ClaimId::new();
            proposal.subject = name.into();
            proposal.value = value;
            proposal.applicability.roots = vec![root.clone()];
            if name == "command" || name == "fix" {
                proposal.evidence = vec![verifying.clone()];
            }
            if name == "command" {
                let mut configuration = source.clone();
                configuration.kind = EvidenceKind::Configuration;
                proposal.evidence.push(configuration);
            }
            if name == "fix" {
                proposal.evidence.push(EvidenceRef {
                    artifact: patch.spec.id.clone(),
                    sha256: patch.sha256.clone(),
                    range: None,
                    source: Some(after.clone()),
                    verification: None,
                    kind: EvidenceKind::Patch,
                });
            }
            if name == "preference" {
                proposal.evidence[0].kind = EvidenceKind::UserStatement;
            }
            let result = propose(
                &mut f.store,
                &f.access,
                proposal.clone(),
                Timestamp::new(200),
            )
            .await
            .unwrap();
            assert_eq!(
                result.result.resolution.outcome,
                Outcome::Accepted,
                "{name}: {:?}",
                result.result.resolution.findings
            );
            if name == "command" || name == "fix" {
                assert_eq!(
                    result.result.resolution.evidence_status,
                    EvidenceStatus::Verified
                );
                verified_proposals.push(proposal);
            }
        }
        assert_eq!(versions(f.store.state(), &f.access.workspace).len(), 6);
        for verified in &verified_proposals {
            let mut unrelated = different_output(
                verified,
                &format!("{}-unrelated-proof", verified.output_key),
            );
            unrelated.claim = ClaimId::new();
            match &mut unrelated.value {
                ClaimValue::Command { argv, .. } => *argv = vec!["cargo".into(), "publish".into()],
                ClaimValue::VerifiedFix { patch, .. } => {
                    let old_patch = patch.clone();
                    *patch = source.artifact.clone();
                    let reference = unrelated
                        .evidence
                        .iter_mut()
                        .find(|e| e.artifact == old_patch)
                        .unwrap();
                    reference.artifact = source.artifact.clone();
                    reference.sha256 = source.sha256.clone();
                }
                _ => unreachable!(),
            }
            let rejected = propose(&mut f.store, &f.access, unrelated, Timestamp::new(250))
                .await
                .unwrap();
            assert_eq!(rejected.result.resolution.outcome, Outcome::Rejected);
            assert!(rejected.result.version.is_none());
        }
        let mut engine = Engine::new(f.store).unwrap();
        let changed = Fingerprint {
            repository: "e".repeat(64),
            ..fingerprint()
        };
        let observe = command(
            &engine,
            &access,
            Some(scope.task),
            Revision::new(1),
            Command::ObserveFingerprint {
                fingerprint: changed,
            },
        );
        engine
            .handle(observe, &access, &HostFacts::inspect(Timestamp::new(300)))
            .await
            .unwrap();
        f.store = engine.into_store();
        for prior in verified_proposals {
            let historical = history::query(&f.store, &f.access, &prior.claim, None, None).unwrap();
            assert_eq!(historical.versions.len(), 1);
            assert!(!historical.versions[0].applicable);
            assert!(historical.versions[0].version.is_some());
            assert!(historical.versions[0]
                .evidence
                .iter()
                .all(|e| !e.verification_current));
            let mut retry_stale = different_output(&prior, &format!("{}-stale", prior.output_key));
            retry_stale.claim = ClaimId::new();
            let rejected = propose(&mut f.store, &f.access, retry_stale, Timestamp::new(301))
                .await
                .unwrap();
            assert_eq!(rejected.result.resolution.outcome, Outcome::Rejected);
            assert!(rejected.result.version.is_none());
        }
        assert_eq!(versions(f.store.state(), &f.access.workspace).len(), 6);
    }
}

#[tokio::test]
async fn historical_reads_and_exact_retry_recheck_origin_task_access() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let mut engine = Engine::new(f.store).unwrap();
        let engine_access = engine_access("workspace");
        let other_task = TaskId::new();
        let create = command(
            &engine,
            &engine_access,
            Some(other_task.clone()),
            Revision::ZERO,
            Command::CreateTask {
                root: other_task.clone(),
                parent: None,
                fork_origin: None,
                objective: Objective {
                    text: "Other task with separately authorized origin".into(),
                    constraints: vec![],
                    acceptance: vec!["capture origin".into()],
                    source: EventId::new(),
                    steering: SteeringRevision::ZERO,
                },
                fingerprint: fingerprint(),
                editing: false,
                required_checks: vec![],
            },
        );
        engine
            .handle(
                create,
                &engine_access,
                &HostFacts::inspect(Timestamp::new(150)),
            )
            .await
            .unwrap();
        let other_origin = engine
            .store()
            .state()
            .events
            .iter()
            .find(|e| {
                e.event.task.as_ref() == Some(&other_task) && e.event.kind == EventKind::TaskCreated
            })
            .unwrap()
            .event
            .id
            .clone();
        f.store = engine.into_store();
        f.proposal.origins = vec![other_origin];
        let accepted = propose(
            &mut f.store,
            &f.access,
            f.proposal.clone(),
            Timestamp::new(200),
        )
        .await
        .unwrap();
        assert_eq!(accepted.result.resolution.outcome, Outcome::Accepted);
        let narrow = Access {
            tasks: Some(std::collections::BTreeSet::from([f
                .proposal
                .scope
                .task
                .clone()])),
            ..f.access
        };
        let before = f.store.state().clone();
        assert!(matches!(
            history::query(
                &f.store,
                &narrow,
                &f.proposal.claim,
                Some(accepted.result.memory_seq),
                None
            ),
            Err(vcp_memory::Error::Access)
        ));
        assert!(matches!(
            propose(
                &mut f.store,
                &narrow,
                f.proposal.clone(),
                Timestamp::new(201)
            )
            .await,
            Err(vcp_memory::Error::Access)
        ));
        assert_eq!(f.store.state(), &before);
    }
}
