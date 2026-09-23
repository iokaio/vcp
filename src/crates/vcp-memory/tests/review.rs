// SPDX-License-Identifier: Apache-2.0
//! Manual review has one atomic durable receipt and never bypasses governance.
use std::collections::BTreeMap;
use vcp_domain::memory_review::{self, Choice, Submission};
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
use vcp_memory::{
    access::Access,
    review::{self, Resolve},
};
use vcp_protocol::{
    command::{Command, CommandEnvelope},
    event::EventKind,
};
use vcp_store::contract::CanonicalStore;
use vcp_store::{artifact::ArtifactWriter, contract::Collection, BackendKind, Store};

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

fn submission(f: &Fixture) -> Submission {
    let task: vcp_domain::task::Task = f
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
    Submission {
        document_type: memory_review::SUBMISSION.into(),
        schema_version: 1,
        id: f.proposal.id.clone(),
        scope: f.proposal.scope.clone(),
        revision: Revision::ZERO,
        candidate: f.proposal.clone(),
        candidate_digest: vcp_protocol::digest_bytes(
            &vcp_protocol::canonical_bytes(&f.proposal).unwrap(),
        ),
        command_digest: "a".repeat(64),
        task_revision: task.revision,
        steering: task.steering,
        expected_head: None,
        recorded_at: Timestamp::new(200),
    }
}
fn resolution(submission: &Submission, choice: Choice) -> Resolve {
    Resolve {
        scope: submission.scope.clone(),
        submission: submission.id.clone(),
        submission_digest: submission.candidate_digest.clone(),
        command: CommandId::new(),
        command_digest: "b".repeat(64),
        task_revision: submission.task_revision,
        steering: submission.steering,
        epochs: submission.candidate.epochs.clone(),
        expected_head: submission.expected_head.clone(),
        choice,
        reason: "explicit fixture review".into(),
        now: Timestamp::new(201),
    }
}
fn count(store: &Store, kind: &str) -> usize {
    store
        .state()
        .records
        .values()
        .filter(|r| r.value["document_type"] == kind)
        .count()
}

#[tokio::test]
async fn source_purge_erases_pending_and_decided_review_content_on_both_stores() {
    use vcp_domain::{
        retention_selector::{Criterion, Selector, Tree},
        task::{Task, TaskState},
    };
    use vcp_memory::retention::{self, Action, Target};
    use vcp_store::contract::{key, Mutation, Record, Transaction};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for choice in [None, Some(Choice::Reject), Some(Choice::Accept)] {
            let temp = tempfile::tempdir().unwrap();
            let mut f = fixture(temp.path(), backend).await;
            let marker = "manual-review-purge-source-marker-6721";
            f.proposal.statement = marker.into();
            if let ClaimValue::Architecture {
                decision,
                rationale,
                ..
            } = &mut f.proposal.value
            {
                *decision = marker.into();
                *rationale = marker.into();
            }
            let input = submission(&f);
            review::submit(&mut f.store, &f.access, input.clone())
                .await
                .unwrap();
            let decision = if let Some(choice) = choice {
                let mut request = resolution(&input, choice);
                request.reason = marker.into();
                review::resolve(&mut f.store, &f.access, request)
                    .await
                    .unwrap()
                    .review
                    .decision
            } else {
                None
            };
            let mut task: Task = f
                .store
                .state()
                .record(
                    Collection::Task,
                    input.scope.task.as_str(),
                    &f.access.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            let prior = task.revision;
            task.revision = prior.next().unwrap();
            task.state = TaskState::Cancelled;
            f.store
                .transact(Transaction {
                    id: TransactionId::new(),
                    expected_watermark: f.store.state().watermark,
                    mutations: vec![Mutation::Put {
                        expected: Some(prior),
                        record: Record::typed(
                            Collection::Task,
                            task.scope.task.as_str(),
                            f.access.workspace.clone(),
                            task.revision,
                            &task,
                        )
                        .unwrap(),
                    }],
                    events: vec![],
                    command: None,
                })
                .await
                .unwrap();
            let maintenance = Access {
                workspace: f.access.workspace.clone(),
                actor: f.access.actor.clone(),
                authority: f.access.authority,
                read: true,
                write: true,
                tasks: None,
            };
            // A source selector must pull in pending candidate and decision copies.
            let preview = retention::preview(
                &f.store,
                &maintenance,
                Selector {
                    schema_version: 1,
                    tree: Tree::Match(Criterion::Event("artifact_attached".into())),
                },
                Action::Purge,
                Timestamp::new(300),
            )
            .unwrap();
            assert!(preview.protected.is_empty(), "{:?}", preview.protected);
            let targets: Vec<_> = preview
                .selected
                .iter()
                .chain(preview.dependent.iter())
                .collect();
            assert!(targets.contains(&&Target::Record(key(Collection::Claim, input.id.as_str()))));
            if let Some(decision) = &decision {
                assert!(
                    targets.contains(&&Target::Record(key(Collection::Projection, &decision.id)))
                );
            }
            let job = retention::apply(&mut f.store, &maintenance, &preview, Timestamp::new(301))
                .await
                .unwrap();
            assert!(review::read(&f.store, &f.access, &input.scope, &input.id).is_err());
            let job = retention::cleanup(&mut f.store, &maintenance, &job.id, Timestamp::new(302))
                .await
                .unwrap();
            assert!(job.rewrite_complete);
            assert!(
                !String::from_utf8(vcp_protocol::canonical_bytes(f.store.state()).unwrap())
                    .unwrap()
                    .contains(marker)
            );
            assert_eq!(count(&f.store, memory_review::REDACTED_SUBMISSION), 1);
            assert_eq!(
                count(&f.store, memory_review::REDACTED_DECISION),
                usize::from(choice.is_some())
            );
            f.store.close().await.unwrap();
            f.store = Store::open(temp.path(), backend, &[]).await.unwrap();
            assert!(review::read(&f.store, &f.access, &input.scope, &input.id).is_err());
            let before = f.store.state().clone();
            assert!(review::submit(&mut f.store, &f.access, input.clone())
                .await
                .is_err());
            assert!(
                review::resolve(&mut f.store, &f.access, resolution(&input, Choice::Accept))
                    .await
                    .is_err()
            );
            assert_eq!(f.store.state(), &before);
        }
    }
}

#[tokio::test]
async fn pending_acceptance_replay_and_governed_resolution_are_atomic_and_durable() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let input = submission(&f);
        let old = f.store.state().clone();
        let pending = review::submit(&mut f.store, &f.access, input.clone())
            .await
            .unwrap();
        assert_eq!(f.store.state().records.len(), old.records.len() + 1);
        assert_eq!(f.store.state().commands.len(), old.commands.len() + 1);
        assert_eq!(count(&f.store, "vcp_memory_version_v1"), 0);
        assert_eq!(count(&f.store, "vcp_memory_index_intent_v1"), 0);
        assert!(pending.review.decision.is_none());
        assert_eq!(
            pending.receipt.command.as_ref().unwrap().digest,
            input.command_digest
        );
        let before = f.store.state().clone();
        assert_eq!(
            review::submit(&mut f.store, &f.access, input.clone())
                .await
                .unwrap()
                .receipt,
            pending.receipt
        );
        assert_eq!(f.store.state(), &before);
        let mut changed = input.clone();
        changed.command_digest = "c".repeat(64);
        assert!(review::submit(&mut f.store, &f.access, changed)
            .await
            .is_err());
        f.access.write = false;
        assert!(review::read(&f.store, &f.access, &input.scope, &input.id)
            .unwrap()
            .decision
            .is_none());
        assert!(review::submit(&mut f.store, &f.access, input.clone())
            .await
            .is_err());
        f.access.write = true;
        let request = resolution(&input, Choice::Accept);
        let command = request.command.clone();
        let done = review::resolve(&mut f.store, &f.access, request)
            .await
            .unwrap();
        assert_eq!(
            done.review.decision.as_ref().unwrap().resolution.outcome,
            Outcome::Accepted
        );
        assert!(done.review.result.as_ref().unwrap().version.is_some());
        assert_eq!(
            done.review.result.as_ref().unwrap().transaction,
            done.receipt.transaction
        );
        assert_eq!(
            done.receipt.command.as_ref().unwrap().digest,
            "b".repeat(64)
        );
        assert_eq!(count(&f.store, "vcp_memory_version_v1"), 1);
        let before = f.store.state().clone();
        let mut retry = resolution(&input, Choice::Accept);
        retry.command = command;
        assert_eq!(
            review::resolve(&mut f.store, &f.access, retry)
                .await
                .unwrap()
                .receipt,
            done.receipt
        );
        assert_eq!(f.store.state(), &before);
        assert!(
            review::resolve(&mut f.store, &f.access, resolution(&input, Choice::Reject))
                .await
                .is_err()
        );
        assert_eq!(f.store.state(), &before);
        f.store.close().await.unwrap();
        f.store = Store::open(temp.path(), backend, &[]).await.unwrap();
        let read = review::read(&f.store, &f.access, &input.scope, &input.id).unwrap();
        assert_eq!(read.submission, input);
        assert_eq!(read.decision.unwrap(), done.review.decision.unwrap());
    }
}

#[tokio::test]
async fn stale_guards_and_explicit_rejection_never_create_versions_or_partial_receipts() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let input = submission(&f);
        review::submit(&mut f.store, &f.access, input.clone())
            .await
            .unwrap();
        let before = f.store.state().clone();
        for which in 0..5 {
            let mut request = resolution(&input, Choice::Accept);
            match which {
                0 => request.task_revision = request.task_revision.next().unwrap(),
                1 => request.steering = request.steering.next().unwrap(),
                2 => request.epochs.policy = request.epochs.policy.next().unwrap(),
                3 => request.epochs.deletion = request.epochs.deletion.next().unwrap(),
                _ => request.expected_head = Some(ClaimVersionId::new()),
            }
            assert!(review::resolve(&mut f.store, &f.access, request)
                .await
                .is_err());
            assert_eq!(f.store.state(), &before);
        }
        let rejected = review::resolve(&mut f.store, &f.access, resolution(&input, Choice::Reject))
            .await
            .unwrap();
        assert_eq!(
            rejected.review.decision.unwrap().resolution.outcome,
            Outcome::Rejected
        );
        assert!(rejected.review.result.is_none());
        assert_eq!(f.store.state().records.len(), before.records.len() + 1);
        assert_eq!(count(&f.store, "vcp_memory_version_v1"), 0);
        assert_eq!(count(&f.store, "vcp_memory_proposal_v1"), 0);
        assert_eq!(f.store.state().commands.len(), before.commands.len() + 1);
    }
}

#[tokio::test]
async fn accept_is_governance_evaluation_and_cannot_force_invalid_candidate_into_recall() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        f.proposal.applicability.repository = "wrong-registered-binding".into();
        let input = submission(&f);
        review::submit(&mut f.store, &f.access, input.clone())
            .await
            .unwrap();
        let result = review::resolve(&mut f.store, &f.access, resolution(&input, Choice::Accept))
            .await
            .unwrap();
        assert_eq!(result.review.decision.unwrap().choice, Choice::Accept);
        assert_eq!(
            result.review.result.as_ref().unwrap().resolution.outcome,
            Outcome::Rejected
        );
        assert!(result.review.result.unwrap().version.is_none());
        assert_eq!(count(&f.store, "vcp_memory_version_v1"), 0);
    }
}

#[tokio::test]
async fn logical_retention_hides_pending_content_and_denies_replay_before_physical_rewrite() {
    use vcp_store::contract::{Mutation, Record, Transaction};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let input = submission(&f);
        review::submit(&mut f.store, &f.access, input.clone())
            .await
            .unwrap();
        let origin = f
            .store
            .state()
            .events
            .iter()
            .find(|e| e.event.id == input.candidate.origins[0])
            .unwrap()
            .clone();
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
        workspace.revision = workspace.revision.next().unwrap();
        workspace.deletion = workspace.deletion.next().unwrap();
        let mask = vcp_domain::retention::RetentionMask {
            schema_version: 1,
            workspace: workspace.id.clone(),
            session: input.scope.session.clone(),
            first: origin.sequence,
            last: origin.sequence,
            artifacts: vec![],
            deletion: workspace.deletion,
            reason: "remove original pending source".into(),
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
                            workspace.id.to_string(),
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
                            "manual-source-mask",
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
        let before = f.store.state().clone();
        assert!(review::read(&f.store, &f.access, &input.scope, &input.id).is_err());
        assert!(review::submit(&mut f.store, &f.access, input.clone())
            .await
            .is_err());
        assert!(
            review::resolve(&mut f.store, &f.access, resolution(&input, Choice::Accept))
                .await
                .is_err()
        );
        assert_eq!(f.store.state(), &before);
        assert_eq!(
            count(&f.store, memory_review::SUBMISSION),
            1,
            "test must exercise logical mask before rewrite"
        );
    }
}

#[tokio::test]
async fn manual_accept_preserves_dispute_gates_and_hidden_claims_fail_closed() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let first = submission(&f);
        review::submit(&mut f.store, &f.access, first.clone())
            .await
            .unwrap();
        let accepted = review::resolve(&mut f.store, &f.access, resolution(&first, Choice::Accept))
            .await
            .unwrap();
        let accepted_version = accepted.review.result.unwrap().version.unwrap();
        f.proposal.id = ProposalId::new();
        f.proposal.command = CommandId::new();
        f.proposal.claim = ClaimId::new();
        f.proposal.output_key = "contradictory-manual-observation".into();
        f.proposal.statement = "Parser owns unrelated filesystem operations".into();
        if let ClaimValue::Architecture { decision, .. } = &mut f.proposal.value {
            *decision = f.proposal.statement.clone();
        }
        let contrary = submission(&f);
        review::submit(&mut f.store, &f.access, contrary.clone())
            .await
            .unwrap();
        let disputed = review::resolve(
            &mut f.store,
            &f.access,
            resolution(&contrary, Choice::Accept),
        )
        .await
        .unwrap();
        let result = disputed.review.result.unwrap();
        assert_eq!(result.resolution.outcome, Outcome::Disputed);
        assert!(result.resolution.conflicts.contains(&accepted_version));
        let original: Head = f
            .store
            .state()
            .record(
                Collection::Projection,
                first.candidate.claim.as_str(),
                &f.access.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(original.current, Some(accepted_version));

        let mut engine = Engine::new(f.store).unwrap();
        let authority = engine_access(f.access.workspace.as_str());
        let new_task = TaskId::new();
        let create = command(
            &engine,
            &authority,
            Some(new_task.clone()),
            Revision::ZERO,
            Command::CreateTask {
                root: new_task.clone(),
                parent: None,
                fork_origin: None,
                objective: Objective {
                    text: r#"{"memory_preference":{"key":"output","value":"concise"}}"#.into(),
                    constraints: vec![],
                    acceptance: vec!["retain exact statement".into()],
                    source: EventId::new(),
                    steering: SteeringRevision::ZERO,
                },
                fingerprint: fingerprint(),
                editing: false,
                required_checks: vec![],
            },
        );
        engine
            .handle(create, &authority, &HostFacts::inspect(Timestamp::new(210)))
            .await
            .unwrap();
        f.store = engine.into_store();
        let origin = f
            .store
            .state()
            .events
            .iter()
            .find(|e| {
                e.event.task.as_ref() == Some(&new_task) && e.event.kind == EventKind::TaskCreated
            })
            .unwrap()
            .event
            .id
            .clone();
        f.proposal.id = ProposalId::new();
        f.proposal.command = CommandId::new();
        f.proposal.claim = ClaimId::new();
        f.proposal.scope.task = new_task.clone();
        f.proposal.origins = vec![origin.clone()];
        f.proposal.evidence.clear();
        f.proposal.subject = "output".into();
        f.proposal.predicate = "preference".into();
        f.proposal.statement = "concise output".into();
        f.proposal.output_key = "another-task-manual".into();
        f.proposal.value = ClaimValue::UserPreference {
            key: "output".into(),
            value: "concise".into(),
            explicit_origin: origin,
        };
        f.access.tasks = Some(std::collections::BTreeSet::from([new_task]));
        let mut narrow = submission(&f);
        narrow.recorded_at = Timestamp::new(211);
        review::submit(&mut f.store, &f.access, narrow.clone())
            .await
            .unwrap();
        assert!(review::read(&f.store, &f.access, &narrow.scope, &narrow.id).is_ok());
        let before = f.store.state().clone();
        let mut request = resolution(&narrow, Choice::Accept);
        request.now = Timestamp::new(212);
        assert!(
            review::resolve(&mut f.store, &f.access, request)
                .await
                .is_err(),
            "hidden accepted versions cannot disappear from conflict checks"
        );
        assert_eq!(f.store.state(), &before);
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
        tasks: Some(std::collections::BTreeSet::from([scope.task.clone()])),
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
        extractor: memory_review::MANUAL_EXTRACTOR.into(),
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
    }
}
