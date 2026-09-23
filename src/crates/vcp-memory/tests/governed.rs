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
#[tokio::test]
async fn date_selector_uses_record_provenance_not_its_old_task_creation() {
    use vcp_domain::{
        retention_selector::*,
        task::{Turn, TurnState},
    };
    use vcp_memory::retention::{self, Action, Target};
    use vcp_protocol::event::EventInput;
    use vcp_store::contract::{key, CanonicalStore, Mutation, Record, Transaction};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let turn = Turn {
            redaction: None,
            id: TurnId::new(),
            scope: f.proposal.scope.clone(),
            revision: Revision::ZERO,
            steering: SteeringRevision::ZERO,
            state: TurnState::Queued,
            trigger: f.proposal.evidence[0].artifact.clone(),
            cause: f.proposal.origins[0].clone(),
            reason: "fresh turn on an old task".into(),
        };
        let record = Record::typed(
            Collection::Turn,
            turn.id.to_string(),
            f.access.workspace.clone(),
            Revision::ZERO,
            &turn,
        )
        .unwrap();
        f.store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: f.store.state().watermark,
                mutations: vec![Mutation::Put {
                    record: record.clone(),
                    expected: None,
                }],
                command: None,
                events: vec![EventInput {
                    id: EventId::new(),
                    workspace: f.access.workspace.clone(),
                    session: turn.scope.session.clone(),
                    task: Some(turn.scope.task.clone()),
                    actor: f.access.actor.clone(),
                    correlation: CommandId::new(),
                    causation: None,
                    timestamp: Timestamp::new(2000),
                    kind: EventKind::Commentary,
                    artifacts: vec![],
                    metadata: None,
                    data: serde_json::json!({"facts":[record]}),
                }],
            })
            .await
            .unwrap();
        let preview = retention::preview(
            &f.store,
            &f.access,
            Selector {
                schema_version: 1,
                tree: Tree::Match(Criterion::Date(TimeWindow {
                    lower: None,
                    upper: Some(Bound {
                        instant: InstantSpec::parse("1970-01-01T00:00:01Z", None).unwrap(),
                        inclusive: false,
                    }),
                })),
            },
            Action::Exclude,
            Timestamp::new(3000),
        )
        .unwrap();
        assert!(preview.selected.contains(&Target::Record(key(
            Collection::Task,
            turn.scope.task.as_str()
        ))));
        assert!(!preview
            .selected
            .contains(&Target::Record(key(Collection::Turn, turn.id.as_str()))));
        f.store.close().await.unwrap();
    }
}

#[tokio::test]
async fn bounded_claim_windows_keep_head_and_frozen_sequence_while_history_appends() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let mut previous = None;
        let mut ids = Vec::new();
        for index in 0..5 {
            let mut proposal = different_output(&f.proposal, &format!("window-{index}"));
            proposal.predecessor = previous.clone();
            proposal.correction_reason = previous.as_ref().map(|_| "explicit correction".into());
            proposal.statement = format!("Parser responsibility revision {index}");
            if let ClaimValue::Architecture { decision, .. } = &mut proposal.value {
                *decision = proposal.statement.clone();
            }
            let result = propose(
                &mut f.store,
                &f.access,
                proposal,
                Timestamp::new(200 + index),
            )
            .await
            .unwrap();
            assert_eq!(result.result.resolution.outcome, Outcome::Accepted);
            previous = result.result.version;
            ids.push(previous.clone().unwrap());
        }
        let (first, at, more) = history::window(
            &f.store,
            &f.access,
            &f.proposal.claim,
            None,
            MemorySeq::ZERO,
            2,
        )
        .unwrap();
        assert!(more);
        assert!(first.versions.iter().all(|row| !row.current));
        let mut appended = different_output(&f.proposal, "window-newer");
        appended.predecessor = previous;
        appended.correction_reason = Some("later explicit correction".into());
        appended.statement = "Newer parser responsibility outside frozen history".into();
        if let ClaimValue::Architecture { decision, .. } = &mut appended.value {
            *decision = appended.statement.clone();
        }
        propose(&mut f.store, &f.access, appended, Timestamp::new(300))
            .await
            .unwrap();
        let (second, same_at, more) = history::window(
            &f.store,
            &f.access,
            &f.proposal.claim,
            Some(at),
            first.versions[1].memory_seq,
            2,
        )
        .unwrap();
        assert_eq!(same_at, at);
        assert!(more);
        let (last, _, more) = history::window(
            &f.store,
            &f.access,
            &f.proposal.claim,
            Some(at),
            second.versions[1].memory_seq,
            2,
        )
        .unwrap();
        assert!(!more);
        assert!(last.versions[0].current);
        let actual: Vec<_> = first
            .versions
            .into_iter()
            .chain(second.versions)
            .chain(last.versions)
            .map(|row| row.id)
            .collect();
        assert_eq!(actual, ids);
        let origins = f.proposal.origins.iter().cloned().collect();
        let (links, truncated) = history::origin_links(&f.store, &f.access, &origins).unwrap();
        assert_eq!(links.len(), 6);
        assert!(!truncated);
        let denied = Access {
            workspace: f.access.workspace.clone(),
            actor: f.access.actor.clone(),
            authority: f.access.authority,
            read: true,
            write: false,
            tasks: Some(Default::default()),
        };
        assert!(history::origin_links(&f.store, &denied, &origins)
            .unwrap()
            .0
            .is_empty());
        assert!(history::window(
            &f.store,
            &denied,
            &f.proposal.claim,
            Some(at),
            MemorySeq::ZERO,
            2
        )
        .is_err());
        f.store.close().await.unwrap();
    }
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
            redaction: None,
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

#[tokio::test]
async fn purged_memory_preserves_authorized_lineage_and_cannot_resurrect_on_retry() {
    use vcp_domain::{
        redaction,
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
        let old_head = head(&f.store, &f.proposal);
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
                events: vec![],
                command: None,
                mutations: vec![
                    Mutation::DropProjection {
                        id: old_head.id.to_string(),
                        expected: old_head.revision,
                    },
                    Mutation::DropProjection {
                        id: sequence.id.to_string(),
                        expected: sequence.revision,
                    },
                ],
            })
            .await
            .unwrap();
        assert!(
            vcp_memory::projections::rebuild(&mut f.store, &f.access, Timestamp::new(205))
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(head(&f.store, &f.proposal).current, accepted.result.version);
        let bytes = vcp_protocol::canonical_bytes(f.store.state()).unwrap();
        assert!(!bytes.windows(marker.len()).any(|v| v == marker.as_bytes()));
        let view = history::query(&f.store, &f.access, &f.proposal.claim, None, None).unwrap();
        assert_eq!(view.versions.len(), 1);
        assert_eq!(view.versions[0].visibility, "purged");
        assert!(
            view.versions[0].current
                && !view.versions[0].applicable
                && view.versions[0].version.is_none()
        );
        assert_eq!(view.versions[0].memory_seq, accepted.result.memory_seq);
        assert!(f
            .store
            .state()
            .records
            .values()
            .any(|r| r.value["document_type"] == redaction::VERSION));
        f.proposal.epochs.deletion = workspace.deletion;
        let before = f.store.state().watermark;
        assert!(propose(
            &mut f.store,
            &f.access,
            f.proposal.clone(),
            Timestamp::new(210)
        )
        .await
        .is_err());
        assert_eq!(f.store.state().watermark, before);
        f.access.tasks = Some(Default::default());
        assert!(matches!(
            history::query(&f.store, &f.access, &f.proposal.claim, None, None),
            Err(vcp_memory::Error::Access)
        ));
        f.store.close().await.unwrap();
        let store = Store::open(temp.path(), backend, &[]).await.unwrap();
        f.access.tasks = None;
        assert_eq!(
            history::query(&store, &f.access, &f.proposal.claim, None, None)
                .unwrap()
                .versions[0]
                .visibility,
            "purged"
        );
        store.close().await.unwrap();
    }
}

#[tokio::test]
async fn retention_preview_tombstone_restart_and_physical_cleanup_are_distinct() {
    use vcp_domain::{
        retention_selector::{Criterion, Selector, Tree},
        task::{Task, TaskState},
    };
    use vcp_memory::retention::{self, Action};
    use vcp_protocol::canonical_bytes;
    use vcp_store::contract::{CanonicalStore, Mutation, Record, Transaction};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let marker = "prune-physical-marker-314159";
        f.proposal.statement = marker.into();
        f.proposal.value = ClaimValue::Architecture {
            decision: marker.into(),
            rationale: marker.into(),
            inference: true,
        };
        propose(
            &mut f.store,
            &f.access,
            f.proposal.clone(),
            Timestamp::new(200),
        )
        .await
        .unwrap();
        let selector = Selector {
            schema_version: 1,
            tree: Tree::Match(Criterion::Task(f.proposal.scope.task.clone())),
        };
        let protected = retention::preview(
            &f.store,
            &f.access,
            selector.clone(),
            Action::Purge,
            Timestamp::new(300),
        )
        .unwrap();
        assert!(!protected.protected.is_empty());
        assert!(
            retention::apply(&mut f.store, &f.access, &protected, Timestamp::new(301))
                .await
                .is_err()
        );
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
        let previous = task.revision;
        task.revision = task.revision.next().unwrap();
        task.state = TaskState::Cancelled;
        f.store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: f.store.state().watermark,
                mutations: vec![Mutation::Put {
                    record: Record::typed(
                        Collection::Task,
                        task.scope.task.as_str(),
                        f.access.workspace.clone(),
                        task.revision,
                        &task,
                    )
                    .unwrap(),
                    expected: Some(previous),
                }],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        assert!(
            retention::apply(&mut f.store, &f.access, &protected, Timestamp::new(302))
                .await
                .is_err()
        );
        let preview = retention::preview(
            &f.store,
            &f.access,
            selector,
            Action::Purge,
            Timestamp::new(303),
        )
        .unwrap();
        assert!(preview.protected.is_empty());
        let held = f.store.snapshot().unwrap();
        let job = retention::apply(&mut f.store, &f.access, &preview, Timestamp::new(304))
            .await
            .unwrap();
        assert!(job.logical_unavailable);
        assert!(!job.local_cleanup_complete);
        let denied = history::query(&f.store, &f.access, &f.proposal.claim, None, None).unwrap();
        assert!(denied.versions.iter().all(|v| v.version.is_none()));
        let job = retention::cleanup(&mut f.store, &f.access, &job.id, Timestamp::new(305))
            .await
            .unwrap();
        assert!(job.rewrite_complete);
        assert!(!job.local_cleanup_complete);
        assert!(
            !String::from_utf8(canonical_bytes(f.store.state()).unwrap())
                .unwrap()
                .contains(marker)
        );
        assert!(String::from_utf8(canonical_bytes(held.state()).unwrap())
            .unwrap()
            .contains(marker));
        drop(held);
        f.store.close().await.unwrap();
        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        let complete = retention::cleanup(&mut store, &f.access, &job.id, Timestamp::new(306))
            .await
            .unwrap();
        assert!(complete.local_cleanup_complete, "{:?}", complete);
        let again = retention::cleanup(&mut store, &f.access, &job.id, Timestamp::new(307))
            .await
            .unwrap();
        assert!(again.local_cleanup_complete);
        // A rebuilt empty generation acknowledges purged metadata and the exact
        // retention intent without allocating a new memory sequence or model.
        let inventory = vcp_memory::search_record::inventory(
            &store,
            &f.access,
            &[],
            &vcp_memory::search_record::ChunkerSpec::default(),
            vcp_memory::search_record::Limits::default(),
        )
        .unwrap();
        assert!(inventory.records.is_empty());
        assert!(inventory.exclusions.iter().any(|e| e.reason == "purged"));
        let publisher =
            vcp_memory::publication::Publisher::new(&temp.path().join("search-generations"))
                .unwrap();
        let captured =
            vcp_memory::publication::capture(&store, &f.access, &f.proposal.scope, inventory)
                .unwrap();
        let ready = publisher
            .prepare(
                captured,
                None,
                &std::sync::atomic::AtomicBool::new(false),
                &|_| {},
            )
            .unwrap();
        publisher
            .publish(&mut store, &f.access, &ready, Timestamp::new(308), &|_| {})
            .await
            .unwrap();
        assert!(
            publisher
                .recover(&store, &f.access)
                .unwrap()
                .view
                .unwrap()
                .manifest
                .empty_complete
        );
        drop(ready);
        drop(publisher);
        store.close().await.unwrap();
        let mut paths = vec![temp.path().to_path_buf()];
        while let Some(path) = paths.pop() {
            for entry in std::fs::read_dir(path).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    paths.push(entry.path());
                } else if entry.file_type().unwrap().is_file() {
                    let bytes = std::fs::read(entry.path()).unwrap();
                    assert!(
                        !bytes.windows(marker.len()).any(|b| b == marker.as_bytes()),
                        "{}",
                        entry.path().display()
                    );
                }
            }
        }
        Store::open(temp.path(), backend, &[])
            .await
            .unwrap()
            .close()
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn saved_previews_and_independent_reversible_actions_preserve_raw_history() {
    use vcp_domain::retention_selector::{Criterion, Selector, Tree};
    use vcp_memory::retention::{self, Action, Target};
    use vcp_store::contract::key;
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        f.proposal.value = ClaimValue::Architecture {
            decision: "retain raw evidence".into(),
            rationale: "fixture".into(),
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
        let version = accepted.result.version.unwrap();
        let target = Target::Record(key(Collection::Claim, version.as_str()));
        let selector = Selector {
            schema_version: 1,
            tree: Tree::Match(Criterion::Claim(ClaimKind::Architecture)),
        };
        let original = f.store.state().records[&key(Collection::Claim, version.as_str())].clone();
        for (n, action) in [Action::Exclude, Action::Compact, Action::RestoreRecall]
            .into_iter()
            .enumerate()
        {
            let preview = retention::preview(
                &f.store,
                &f.access,
                selector.clone(),
                action,
                Timestamp::new(300 + n as u64),
            )
            .unwrap();
            retention::save_preview(
                &mut f.store,
                &f.access,
                &preview,
                Timestamp::new(400 + n as u64),
            )
            .await
            .unwrap();
            let loaded = retention::load_preview(&f.store, &f.access, &preview.id).unwrap();
            assert_eq!(loaded, preview);
            let receipt = retention::apply(
                &mut f.store,
                &f.access,
                &loaded,
                Timestamp::new(500 + n as u64),
            )
            .await
            .unwrap();
            assert!(receipt.local_cleanup_complete);
            assert!(!receipt.logical_unavailable);
            let decision = retention::decision(f.store.state(), &f.access.workspace, &target)
                .unwrap()
                .unwrap();
            assert_eq!(decision.recall_excluded, action != Action::RestoreRecall);
            assert_eq!(decision.compacted, n >= 1);
            assert!(!decision.purged);
            assert_eq!(
                f.store.state().records[&key(Collection::Claim, version.as_str())],
                original
            );
            let raw = history::query(&f.store, &f.access, &f.proposal.claim, None, None).unwrap();
            assert!(raw.versions.iter().all(|v| v.version.is_some()));
            assert!(raw
                .versions
                .iter()
                .all(|v| v.applicable == (action == Action::RestoreRecall)));
        }
    }
}

#[tokio::test]
async fn copied_context_lineage_follows_source_ids_and_request_commitments_only() {
    use vcp_domain::retention_selector::{Criterion, Selector, Tree};
    use vcp_memory::retention::{self, Action, Target};
    use vcp_store::contract::{key, CanonicalStore, Mutation, Record, Transaction};
    async fn retain(
        store: &mut Store,
        scope: &Scope,
        schema: &str,
        bytes: &[u8],
    ) -> ArtifactDescriptor {
        let mut writer = store
            .spool()
            .create(ArtifactSpec {
                id: ArtifactId::new(),
                scope: scope.clone(),
                media_type: "application/json".into(),
                schema: schema.into(),
                source: "synthetic".into(),
                channel: Channel::Evidence,
                retention: "history".into(),
                omissions: vec![],
            })
            .unwrap();
        writer.write_chunk(bytes).unwrap();
        let artifact = writer.finalize().unwrap();
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![Mutation::Put {
                    record: Record::typed(
                        Collection::Artifact,
                        artifact.spec.id.as_str(),
                        scope.workspace.clone(),
                        Revision::ZERO,
                        &artifact,
                    )
                    .unwrap(),
                    expected: None,
                }],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        artifact
    }
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        f.proposal.value = ClaimValue::Architecture {
            decision: "copied claim".into(),
            rationale: "fixture".into(),
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
        let version = accepted.result.version.unwrap();
        let scope = &f.proposal.scope;
        let captured=retain(&mut f.store,scope,"memory-context/1",&serde_json::to_vec(&serde_json::json!([{"source":{"kind":"claim","version":version,"claim":f.proposal.claim},"evidence":[],"text":"copied claim"}])).unwrap()).await;
        let unrelated=retain(&mut f.store,scope,"memory-context/1",&serde_json::to_vec(&serde_json::json!([{"source":{"kind":"artifact","id":f.proposal.evidence[0].artifact},"evidence":[],"text":"not a copy of the selected claim"}])).unwrap()).await;
        let request = retain(
            &mut f.store,
            scope,
            "responses-request/1",
            b"copied claim provider request",
        )
        .await;
        let manifest=retain(&mut f.store,scope,"context-manifest/1",&serde_json::to_vec(&serde_json::json!({"included":[{"artifact":captured.spec.id}],"request_sha256":request.sha256})).unwrap()).await;
        let preview = retention::preview(
            &f.store,
            &f.access,
            Selector {
                schema_version: 1,
                tree: Tree::Match(Criterion::Claim(ClaimKind::Architecture)),
            },
            Action::Exclude,
            Timestamp::new(300),
        )
        .unwrap();
        for artifact in [&captured, &request, &manifest] {
            assert!(preview.dependent.contains(&Target::Record(key(
                Collection::Artifact,
                artifact.spec.id.as_str()
            ))));
        }
        assert!(!preview.dependent.contains(&Target::Record(key(
            Collection::Artifact,
            unrelated.spec.id.as_str()
        ))));
    }
}

#[cfg(feature = "qualification")]
#[tokio::test]
async fn retention_process_child() {
    use vcp_domain::{
        retention_selector::{Criterion, Selector, Tree},
        task::{Task, TaskState},
    };
    use vcp_memory::retention::{self, Action};
    use vcp_store::{
        contract::{CanonicalStore, Mutation, Record, Transaction},
        Barrier,
    };
    let Some(path) = std::env::var_os("VCP_PRUNE_CHILD_ROOT") else {
        return;
    };
    let phase = std::env::var("VCP_PRUNE_CHILD_PHASE").unwrap();
    let backend = std::env::var("VCP_PRUNE_CHILD_BACKEND")
        .unwrap()
        .parse()
        .unwrap();
    let mut f = fixture(std::path::Path::new(&path), backend).await;
    f.proposal.statement = "kill-prune-sensitive-marker-2718".into();
    propose(
        &mut f.store,
        &f.access,
        f.proposal.clone(),
        Timestamp::new(200),
    )
    .await
    .unwrap();
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
    let previous = task.revision;
    task.revision = task.revision.next().unwrap();
    task.state = TaskState::Cancelled;
    f.store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: f.store.state().watermark,
            mutations: vec![Mutation::Put {
                record: Record::typed(
                    Collection::Task,
                    task.scope.task.as_str(),
                    f.access.workspace.clone(),
                    task.revision,
                    &task,
                )
                .unwrap(),
                expected: Some(previous),
            }],
            events: vec![],
            command: None,
        })
        .await
        .unwrap();
    let preview = retention::preview(
        &f.store,
        &f.access,
        Selector {
            schema_version: 1,
            tree: Tree::Match(Criterion::Task(task.scope.task)),
        },
        Action::Purge,
        Timestamp::new(300),
    )
    .unwrap();
    if let Some(marker) = std::env::var_os("VCP_PRUNE_SUPERVISOR_MARKER") {
        let evidence = std::path::PathBuf::from(marker)
            .parent()
            .unwrap()
            .to_owned();
        std::fs::write(
            evidence.join("acknowledged.json"),
            vcp_protocol::canonical_bytes(f.store.state()).unwrap(),
        )
        .unwrap();
        std::fs::write(
            evidence.join("claim.json"),
            serde_json::to_vec(&f.proposal.claim).unwrap(),
        )
        .unwrap();
    }
    f.store.observe(std::sync::Arc::new(move |actual| {
        let selected = match phase.as_str() {
            "tombstone" => Barrier::AfterCommit,
            "before_activation" => Barrier::BeforeActivation,
            "activation" => Barrier::AfterActivation,
            "cleanup" => Barrier::AfterCleanupFile,
            _ => panic!("unknown phase"),
        };
        if actual == selected {
            if let Some(marker) = std::env::var_os("VCP_PRUNE_SUPERVISOR_MARKER") {
                use std::io::Write;
                let mut file = std::fs::File::create(marker).unwrap();
                file.write_all(phase.as_bytes()).unwrap();
                file.sync_all().unwrap();
                std::thread::sleep(std::time::Duration::from_secs(40));
                panic!("independent prune supervisor did not terminate child");
            }
            std::process::exit(73)
        }
    }));
    let job = retention::apply(&mut f.store, &f.access, &preview, Timestamp::new(400))
        .await
        .unwrap();
    retention::cleanup(&mut f.store, &f.access, &job.id, Timestamp::new(500))
        .await
        .unwrap();
    panic!("kill barrier not reached");
}
#[cfg(feature = "qualification")]
#[tokio::test]
async fn independently_observed_prune_kills_preserve_exclusion_and_exact_cleanup() {
    use std::{
        io::Write,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    use vcp_memory::retention::{self, PruneReceipt};
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
                let deadline = Instant::now() + Duration::from_secs(10);
                while Instant::now() < deadline && matches!(self.0.try_wait(), Ok(None)) {
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
    }
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for phase in ["tombstone", "before_activation", "activation", "cleanup"] {
            let evidence = tempfile::tempdir().unwrap().keep();
            println!("P8-02 prune {backend:?}/{phase}: {}", evidence.display());
            let root = evidence.join("canonical");
            let marker = evidence.join("ready");
            let mut child = Child(
                Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "retention_process_child", "--nocapture"])
                    .env("VCP_PRUNE_CHILD_ROOT", &root)
                    .env(
                        "VCP_PRUNE_CHILD_BACKEND",
                        if backend == BackendKind::Files {
                            "files"
                        } else {
                            "sqlite"
                        },
                    )
                    .env("VCP_PRUNE_CHILD_PHASE", phase)
                    .env("VCP_PRUNE_SUPERVISOR_MARKER", &marker)
                    .stdin(Stdio::null())
                    .stdout(std::fs::File::create(evidence.join("child.log")).unwrap())
                    .stderr(std::fs::File::create(evidence.join("child.err")).unwrap())
                    .spawn()
                    .unwrap(),
            );
            let deadline = Instant::now() + Duration::from_secs(30);
            while std::fs::read_to_string(&marker).ok().as_deref() != Some(phase) {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "child exited before barrier"
                );
                assert!(Instant::now() < deadline, "prune barrier deadline");
                std::thread::sleep(Duration::from_millis(10));
            }
            let mut receipt = std::fs::File::create(evidence.join("supervisor.json")).unwrap();
            receipt
                .write_all(
                    &serde_json::to_vec(&serde_json::json!({
                        "backend": backend, "barrier": phase, "observed": true,
                        "prune_acknowledged": false, "child_pid": child.0.id()
                    }))
                    .unwrap(),
                )
                .unwrap();
            receipt.sync_all().unwrap();
            child.0.kill().unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if let Some(status) = child.0.try_wait().unwrap() {
                    assert!(!status.success());
                    break;
                }
                assert!(Instant::now() < deadline, "prune child did not exit");
                std::thread::sleep(Duration::from_millis(10));
            }
            let mut store = Store::open(&root, backend, &[]).await.unwrap();
            let access = Access {
                workspace: WorkspaceId::parse("workspace").unwrap(),
                actor: ActorId::parse("owner").unwrap(),
                authority: AuthorityRevision::ZERO,
                read: true,
                write: true,
                tasks: None,
            };
            let acknowledged: vcp_store::contract::State =
                serde_json::from_slice(&std::fs::read(evidence.join("acknowledged.json")).unwrap())
                    .unwrap();
            for (key, record) in &acknowledged.records {
                if record.workspace != access.workspace {
                    assert!(
                        store.state().records.get(key) == Some(record),
                        "unselected workspace record changed"
                    );
                }
            }
            for (id, receipt) in &acknowledged.transactions {
                assert!(
                    store.state().transactions.get(id) == Some(receipt),
                    "acknowledged transaction receipt lost"
                );
            }
            let claim =
                serde_json::from_slice(&std::fs::read(evidence.join("claim.json")).unwrap())
                    .unwrap();
            let hidden = history::query(&store, &access, &claim, None, None).unwrap();
            assert!(!hidden.versions.is_empty());
            assert!(hidden
                .versions
                .iter()
                .all(|v| matches!(v.visibility, "pruned" | "purged")
                    && v.version.is_none()
                    && v.evidence.is_empty()
                    && !v.applicable));
            let job: PruneReceipt = store
                .state()
                .records
                .values()
                .find(|r| r.value["document_type"] == "vcp_retention_job_v1")
                .unwrap()
                .decode()
                .unwrap();
            assert!(job.logical_unavailable);
            let done = retention::cleanup(&mut store, &access, &job.id, Timestamp::new(600))
                .await
                .unwrap();
            assert!(done.local_cleanup_complete);
            let before_retry = store.state().clone();
            let retry = retention::cleanup(&mut store, &access, &job.id, Timestamp::new(601))
                .await
                .unwrap();
            assert!(retry.local_cleanup_complete && retry.rewrite_complete);
            assert_eq!(retry.revision, done.revision.next().unwrap());
            // Cleanup records each attempt. A retry may append its own receipt,
            // but cannot change another record or repeat a redaction/rewrite.
            for (key, record) in &before_retry.records {
                if record.id != job.id {
                    assert!(store.state().records.get(key) == Some(record));
                }
            }
            assert!(store.state().events.starts_with(&before_retry.events));
            for (id, receipt) in &before_retry.transactions {
                assert!(store.state().transactions.get(id) == Some(receipt));
            }
            let state = store.state().clone();
            store.close().await.unwrap();
            let reopened = Store::open(&root, backend, &[]).await.unwrap();
            assert_eq!(reopened.state(), &state);
            let after_cleanup = history::query(&reopened, &access, &claim, None, None).unwrap();
            assert_eq!(after_cleanup.versions.len(), hidden.versions.len());
            assert!(after_cleanup
                .versions
                .iter()
                .all(|v| v.visibility == "purged"
                    && v.version.is_none()
                    && v.evidence.is_empty()
                    && !v.applicable));
            for (key, record) in &acknowledged.records {
                if record.workspace != access.workspace {
                    assert!(
                        reopened.state().records.get(key) == Some(record),
                        "unselected workspace changed during cleanup"
                    );
                }
            }
            for (id, receipt) in &acknowledged.transactions {
                assert!(
                    reopened.state().transactions.get(id) == Some(receipt),
                    "cleanup lost acknowledged receipt"
                );
            }
            reopened.close().await.unwrap();
            let mut paths = vec![root.clone()];
            while let Some(path) = paths.pop() {
                for entry in std::fs::read_dir(path).unwrap() {
                    let entry = entry.unwrap();
                    if entry.file_type().unwrap().is_dir() {
                        paths.push(entry.path());
                    } else {
                        let bytes = std::fs::read(entry.path()).unwrap();
                        assert!(
                            !bytes
                                .windows(b"kill-prune-sensitive-marker-2718".len())
                                .any(|part| part == b"kill-prune-sensitive-marker-2718"),
                            "purged bytes remain in canonical root"
                        );
                    }
                }
            }
            std::fs::write(
                evidence.join("result.json"),
                serde_json::to_vec(&serde_json::json!({
                    "pass": true, "backend": backend, "barrier": phase,
                    "termination_observed": true, "exclusion_survives": true,
                    "cleanup_retry_only_appends_its_own_receipt": true
                }))
                .unwrap(),
            )
            .unwrap();
        }
    }
}
#[cfg(feature = "qualification")]
#[tokio::test]
async fn process_death_after_tombstone_activation_and_partial_cleanup_resumes_exactly() {
    use vcp_memory::retention::{self, PruneReceipt};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for phase in ["tombstone", "before_activation", "activation", "cleanup"] {
            let temp = tempfile::tempdir().unwrap();
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "retention_process_child", "--nocapture"])
                .env("VCP_PRUNE_CHILD_ROOT", temp.path())
                .env(
                    "VCP_PRUNE_CHILD_BACKEND",
                    if backend == BackendKind::Files {
                        "files"
                    } else {
                        "sqlite"
                    },
                )
                .env("VCP_PRUNE_CHILD_PHASE", phase)
                .status()
                .unwrap();
            assert_eq!(result.code(), Some(73));
            let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
            let access = Access {
                workspace: WorkspaceId::parse("workspace").unwrap(),
                actor: ActorId::parse("owner").unwrap(),
                authority: AuthorityRevision::ZERO,
                read: true,
                write: true,
                tasks: None,
            };
            let job: PruneReceipt = store
                .state()
                .records
                .values()
                .find(|r| r.value["document_type"] == "vcp_retention_job_v1")
                .unwrap()
                .decode()
                .unwrap();
            assert!(job.logical_unavailable);
            let done = retention::cleanup(&mut store, &access, &job.id, Timestamp::new(600))
                .await
                .unwrap();
            assert!(done.local_cleanup_complete, "{phase}: {done:?}");
            store.close().await.unwrap();
            let mut paths = vec![temp.path().to_path_buf()];
            while let Some(path) = paths.pop() {
                for entry in std::fs::read_dir(path).unwrap() {
                    let entry = entry.unwrap();
                    if entry.file_type().unwrap().is_dir() {
                        paths.push(entry.path());
                    } else {
                        let bytes = std::fs::read(entry.path()).unwrap();
                        assert!(
                            !bytes
                                .windows(b"kill-prune-sensitive-marker-2718".len())
                                .any(|b| b == b"kill-prune-sensitive-marker-2718"),
                            "{phase}: {}",
                            entry.path().display()
                        );
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn newly_matching_history_cannot_expand_a_saved_preview() {
    use vcp_domain::retention_selector::{Criterion, Selector, Tree};
    use vcp_memory::retention::{self, Action, Target};
    use vcp_protocol::event::EventInput;
    use vcp_store::contract::{CanonicalStore, Transaction};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let selector = Selector {
            schema_version: 1,
            tree: Tree::Match(Criterion::Task(f.proposal.scope.task.clone())),
        };
        let preview = retention::preview(
            &f.store,
            &f.access,
            selector.clone(),
            Action::Exclude,
            Timestamp::new(200),
        )
        .unwrap();
        retention::save_preview(&mut f.store, &f.access, &preview, Timestamp::new(201))
            .await
            .unwrap();
        let newer = EventId::new();
        f.store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: f.store.state().watermark,
                mutations: vec![],
                command: None,
                events: vec![EventInput {
                    id: newer.clone(),
                    workspace: f.access.workspace.clone(),
                    session: f.proposal.scope.session.clone(),
                    task: Some(f.proposal.scope.task.clone()),
                    actor: f.access.actor.clone(),
                    correlation: CommandId::new(),
                    causation: None,
                    timestamp: Timestamp::new(202),
                    kind: EventKind::Commentary,
                    artifacts: vec![],
                    data: serde_json::json!({"text":"new matching history"}),
                    metadata: None,
                }],
            })
            .await
            .unwrap();
        let before = f.store.state().clone();
        assert!(
            retention::apply(&mut f.store, &f.access, &preview, Timestamp::new(203))
                .await
                .is_err()
        );
        assert_eq!(f.store.state(), &before);
        let fresh = retention::preview(
            &f.store,
            &f.access,
            selector,
            Action::Exclude,
            Timestamp::new(204),
        )
        .unwrap();
        assert!(!preview.selected.contains(&Target::Event(newer.clone())));
        assert!(fresh.selected.contains(&Target::Event(newer)));
    }
}

#[tokio::test]
async fn retention_selects_historical_source_roots_and_paths_without_current_binding_inference() {
    use vcp_domain::retention_selector::{Criterion, Selector, Tree};
    use vcp_memory::retention::{self, Action, Target};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let dir = tempfile::tempdir().unwrap();
        let f = fixture(dir.path(), backend).await;
        let descriptor: ArtifactDescriptor = f
            .store
            .state()
            .record(
                Collection::Artifact,
                f.proposal.evidence[0].artifact.as_str(),
                &f.access.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let root = RootId::parse("historical-source-root").unwrap();
        let manifest = serde_json::json!({
            "manifest": {
                "identity": {"workspace":f.access.workspace,"root":root,
                    "repository":"old-repository","worktree":"old-worktree","binding":"93"},
                "files":[{"root":root,"path":"old/module.rs","sha256":descriptor.sha256,"bytes":descriptor.length}]
            },
            "sources":[descriptor.spec.id]
        });
        let mut engine = Engine::new(f.store).unwrap();
        capture(
            &mut engine,
            &f.proposal.scope,
            "verification-baseline/1",
            &serde_json::to_vec(&manifest).unwrap(),
        )
        .await;
        // A foreign capture cannot supply additional metadata about this artifact.
        let mut forged = manifest.clone();
        forged["manifest"]["files"][0]["path"] = serde_json::json!("foreign.rs");
        capture(
            &mut engine,
            &f.foreign.spec.scope,
            "verification-baseline/1",
            &serde_json::to_vec(&forged).unwrap(),
        )
        .await;
        let store = engine.into_store();
        let target = Target::Record(vcp_store::contract::key(
            Collection::Artifact,
            descriptor.spec.id.as_str(),
        ));
        let preview = retention::preview(
            &store,
            &f.access,
            Selector {
                schema_version: 1,
                tree: Tree::All(vec![
                    Tree::Match(Criterion::Root(root.clone())),
                    Tree::Match(Criterion::Path("old/module.rs".into())),
                ]),
            },
            Action::Exclude,
            Timestamp::new(500),
        )
        .unwrap();
        assert!(preview.selected.contains(&target));
        let foreign = retention::preview(
            &store,
            &f.access,
            Selector {
                schema_version: 1,
                tree: Tree::Match(Criterion::Path("foreign.rs".into())),
            },
            Action::Exclude,
            Timestamp::new(500),
        )
        .unwrap();
        assert!(!foreign.selected.contains(&target));
        let wrong = retention::preview(
            &store,
            &f.access,
            Selector {
                schema_version: 1,
                tree: Tree::Match(Criterion::Root(RootId::parse("unrelated-root").unwrap())),
            },
            Action::Exclude,
            Timestamp::new(500),
        )
        .unwrap();
        assert!(!wrong.selected.contains(&target));
    }
}
