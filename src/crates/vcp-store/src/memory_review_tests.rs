// SPDX-License-Identifier: Apache-2.0
use super::*;
#[path = "../tests/common/mod.rs"]
mod common;
use crate::{BackendKind, Store};
use vcp_protocol::{command::CommandResult, event::EventInput};
fn submission() -> Submission {
    let candidate = Proposal {
        id: ProposalId::new(),
        command: CommandId::new(),
        claim: ClaimId::new(),
        scope: common::task().scope,
        actor: ActorId::parse("human").unwrap(),
        epochs: Epochs {
            authority: AuthorityRevision::ZERO,
            policy: PolicyRevision::ZERO,
            deletion: DeletionEpoch::ZERO,
        },
        registry_version: REGISTRY_VERSION,
        extractor: MANUAL_EXTRACTOR.into(),
        output_key: "candidate".into(),
        origins: vec![EventId::parse("created").unwrap()],
        subject: "module".into(),
        predicate: "architecture".into(),
        statement: "private submission marker".into(),
        value: ClaimValue::Architecture {
            decision: "isolate tools".into(),
            rationale: "retained source".into(),
            inference: true,
        },
        applicability: Applicability {
            repository: "fixture".into(),
            worktree: "main".into(),
            roots: vec![],
            paths: vec![],
            symbols: vec![],
            branch: None,
            fingerprint: None,
            conditions: Default::default(),
            valid_from: None,
            valid_until: None,
        },
        evidence: vec![],
        predecessor: None,
        correction_reason: None,
        retention: "history".into(),
    };
    Submission {
        document_type: SUBMISSION.into(),
        schema_version: 1,
        id: candidate.id.clone(),
        scope: candidate.scope.clone(),
        revision: Revision::ZERO,
        candidate_digest: digest_bytes(&canonical_bytes(&candidate).unwrap()),
        candidate,
        command_digest: "a".repeat(64),
        task_revision: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        expected_head: None,
        recorded_at: Timestamp::new(101),
    }
}
fn row<T: serde::Serialize>(collection: Collection, id: &str, value: &T) -> Record {
    Record::typed(
        collection,
        id,
        common::workspace().id,
        Revision::ZERO,
        value,
    )
    .unwrap()
}
fn review_tx(state: &State, record: Record) -> Transaction {
    let submit = record.value["document_type"] == SUBMISSION;
    let (command, digest, actor, time) = if submit {
        let s: Submission = record.decode().unwrap();
        (
            s.candidate.command,
            s.command_digest,
            s.candidate.actor,
            s.recorded_at,
        )
    } else {
        let d: Decision = record.decode().unwrap();
        (d.command, d.command_digest, d.actor, d.recorded_at)
    };
    let event = EventInput {
        id: EventId::new(),
        workspace: record.workspace.clone(),
        session: common::session().id,
        task: Some(common::task().scope.task),
        actor,
        correlation: command.clone(),
        causation: Some(EventId::parse("created").unwrap()),
        timestamp: time,
        kind: EventKind::MemoryResolved,
        artifacts: vec![],
        metadata: None,
        data: serde_json::json!({"schema_version":1,"memory_review":{"schema_version":1,"action":if submit{"submission"}else{"decision"},"id":record.id,"record_digest":digest_bytes(&canonical_bytes(&record.value).unwrap())}}),
    };
    Transaction {
        id: TransactionId::new(),
        expected_watermark: state.watermark,
        mutations: vec![Mutation::Put {
            expected: None,
            record,
        }],
        events: vec![event],
        command: Some(ReceiptInput {
            command,
            workspace: common::workspace().id,
            session: common::session().id,
            digest,
            result: CommandResult::Accepted {
                revision: Revision::ZERO,
            },
        }),
    }
}
fn decision(s: &Submission, choice: Choice, outcome: Outcome) -> Decision {
    Decision {
        document_type: DECISION.into(),
        schema_version: 1,
        id: decision_id(&s.scope, &s.id).unwrap(),
        scope: s.scope.clone(),
        revision: Revision::ZERO,
        submission: s.id.clone(),
        submission_digest: s.candidate_digest.clone(),
        command: CommandId::new(),
        command_digest: "b".repeat(64),
        actor: ActorId::parse("reviewer").unwrap(),
        epochs: s.candidate.epochs.clone(),
        task_revision: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        expected_head: s.expected_head.clone(),
        choice,
        reason: "private review reason".into(),
        governed_proposal: (choice == Choice::Accept).then(ProposalId::new),
        resolution: Resolution {
            outcome,
            evidence_status: EvidenceStatus::Inferred,
            findings: vec![],
            conflicts: vec![],
            validated_evidence: vec![],
        },
        recorded_at: Timestamp::new(102),
    }
}
fn accepted_tx(state: &State, s: &Submission, d: &Decision) -> Transaction {
    let mut tx = review_tx(state, row(Collection::Projection, &d.id, d));
    let mut proposal = s.candidate.clone();
    proposal.id = d.governed_proposal.clone().unwrap();
    proposal.command = d.command.clone();
    proposal.actor = d.actor.clone();
    proposal.epochs = d.epochs.clone();
    let payload_digest = digest_bytes(&canonical_bytes(&proposal).unwrap());
    let recorded = ProposalRecord {
        document_type: DocumentType::Proposal,
        schema_version: 1,
        id: proposal.id.clone(),
        scope: proposal.scope.clone(),
        revision: Revision::ZERO,
        proposal: proposal.clone(),
        resolution: d.resolution.clone(),
        payload_digest: payload_digest.clone(),
        recorded_at: d.recorded_at,
    };
    let accepted = matches!(d.resolution.outcome, Outcome::Accepted | Outcome::Disputed);
    let version = accepted.then(ClaimVersionId::new);
    let intent = accepted.then(IndexIntentId::new);
    let mut records = vec![row(Collection::Claim, recorded.id.as_str(), &recorded)];
    if let (Some(id), Some(intent)) = (&version, &intent) {
        records.push(row(
            Collection::Claim,
            id.as_str(),
            &Version {
                document_type: DocumentType::Version,
                schema_version: 1,
                id: id.clone(),
                scope: s.scope.clone(),
                revision: Revision::ZERO,
                proposal: proposal.clone(),
                memory_seq: MemorySeq::new(1),
                canonical_watermark: state.watermark.next().unwrap(),
                recorded_at: d.recorded_at,
                resolution: d.resolution.clone(),
            },
        ));
        records.push(row(
            Collection::Projection,
            proposal.claim.as_str(),
            &Head {
                document_type: DocumentType::Head,
                schema_version: 1,
                id: proposal.claim.clone(),
                scope: s.scope.clone(),
                revision: Revision::ZERO,
                current: (d.resolution.outcome == Outcome::Accepted).then(|| id.clone()),
                disputed: if d.resolution.outcome == Outcome::Disputed {
                    vec![id.clone()]
                } else {
                    vec![]
                },
            },
        ));
        records.push(row(
            Collection::IndexIntent,
            intent.as_str(),
            &IndexIntent {
                document_type: DocumentType::IndexIntent,
                schema_version: 1,
                id: intent.clone(),
                scope: s.scope.clone(),
                revision: Revision::ZERO,
                transaction: tx.id.clone(),
                versions: vec![id.clone()],
                supersedes: vec![],
                memory_seq: MemorySeq::new(1),
                canonical_watermark: state.watermark.next().unwrap(),
                status: IndexStatus::Pending,
                deletion: None,
            },
        ));
    }
    records.push(row(
        Collection::Projection,
        s.scope.workspace.as_str(),
        &MemoryHead {
            document_type: DocumentType::Sequence,
            schema_version: 1,
            id: s.scope.workspace.clone(),
            workspace: s.scope.workspace.clone(),
            revision: Revision::ZERO,
            sequence: MemorySeq::new(1),
        },
    ));
    records.push(row(
        Collection::Projection,
        d.command.as_str(),
        &ProposalResult {
            document_type: DocumentType::Result,
            schema_version: 1,
            id: d.command.clone(),
            scope: s.scope.clone(),
            revision: Revision::ZERO,
            proposal: proposal.id,
            payload_digest,
            transaction: tx.id.clone(),
            version,
            intent,
            resolution: d.resolution.clone(),
            memory_seq: MemorySeq::new(1),
        },
    ));
    tx.mutations
        .extend(records.into_iter().map(|record| Mutation::Put {
            expected: None,
            record,
        }));
    tx
}
#[tokio::test]
async fn immutable_submission_and_each_governed_outcome_reopen_on_both_stores() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for outcome in [
            None,
            Some(Outcome::Accepted),
            Some(Outcome::Disputed),
            Some(Outcome::Rejected),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
            store.transact(common::initial()).await.unwrap();
            let s = submission();
            let tx = review_tx(store.state(), row(Collection::Claim, s.id.as_str(), &s));
            let receipt = store.transact(tx.clone()).await.unwrap();
            assert_eq!(store.transact(tx).await.unwrap(), receipt);
            assert!(!store
                .state()
                .records
                .values()
                .any(|r| r.value["document_type"] == "vcp_memory_version_v1"));
            let d = decision(
                &s,
                if outcome.is_some() {
                    Choice::Accept
                } else {
                    Choice::Reject
                },
                outcome.unwrap_or(Outcome::Rejected),
            );
            let tx = if outcome.is_some() {
                accepted_tx(store.state(), &s, &d)
            } else {
                review_tx(store.state(), row(Collection::Projection, &d.id, &d))
            };
            let receipt = store.transact(tx.clone()).await.unwrap();
            assert_eq!(store.transact(tx).await.unwrap(), receipt);
            let snapshot = store.state().clone();
            let other = decision(&s, Choice::Reject, Outcome::Rejected);
            assert_eq!(other.id, d.id);
            assert!(store
                .transact(review_tx(
                    store.state(),
                    row(Collection::Projection, &other.id, &other)
                ))
                .await
                .is_err());
            assert_eq!(store.state(), &snapshot);
            store.close().await.unwrap();
            let store = Store::open(temp.path(), backend, &[]).await.unwrap();
            assert_eq!(store.state(), &snapshot);
            store.close().await.unwrap();
        }
    }
}
#[test]
fn forged_review_atomic_linkage_never_changes_canonical_state() {
    let state = State::default().prepare(&common::initial()).unwrap().0;
    let s = submission();
    let tx = review_tx(&state, row(Collection::Claim, s.id.as_str(), &s));
    assert_receipt_result_bound(&state, &tx);
    for variant in 0..9 {
        let mut bad = tx.clone();
        match variant {
            0 => bad.command = None,
            1 => bad.command.as_mut().unwrap().digest = "f".repeat(64),
            2 => bad.events[0].actor = ActorId::new(),
            3 => bad.events[0].task = Some(TaskId::new()),
            4 => bad.events[0].data["memory_review"]["record_digest"] = "f".repeat(64).into(),
            5 => {
                if let Mutation::Put { record, .. } = &mut bad.mutations[0] {
                    record.value["candidate_digest"] = "f".repeat(64).into()
                }
            }
            6 => {
                if let Mutation::Put { record, .. } = &mut bad.mutations[0] {
                    record.value["task_revision"] = "1".into()
                }
            }
            7 => bad.events[0].data["memory_review"]["unknown"] = true.into(),
            _ => bad.mutations.push(Mutation::DropProjection {
                id: common::task().scope.task.to_string(),
                expected: Revision::ZERO,
            }),
        }
        assert!(state.prepare(&bad).is_err(), "variant {variant}");
    }
    let state = state.prepare(&tx).unwrap().0;
    let d = decision(&s, Choice::Accept, Outcome::Disputed);
    let good = accepted_tx(&state, &s, &d);
    state.prepare(&good).unwrap();
    assert_receipt_result_bound(&state, &good);
    let rejected = decision(&s, Choice::Reject, Outcome::Rejected);
    let rejected = review_tx(&state, row(Collection::Projection, &rejected.id, &rejected));
    state.prepare(&rejected).unwrap();
    assert_receipt_result_bound(&state, &rejected);
    for variant in 0..10 {
        let mut bad = good.clone();
        match variant {
            0 => {
                bad.mutations.pop();
            }
            1 => {
                if let Mutation::Put { record, .. } = &mut bad.mutations[1] {
                    record.value["proposal"]["statement"] = "different candidate".into()
                }
            }
            2 => {
                if let Mutation::Put { record, .. } = &mut bad.mutations[1] {
                    record.value["resolution"]["outcome"] = "accepted".into()
                }
            }
            3 => {
                if let Mutation::Put { record, .. } = &mut bad.mutations[0] {
                    record.id = format!("{DECISION_PREFIX}{}", "f".repeat(64));
                    record.value["id"] = record.id.clone().into()
                }
            }
            4 => {
                bad.mutations.remove(0);
                bad.events[0]
                    .data
                    .as_object_mut()
                    .unwrap()
                    .remove("memory_review");
            }
            5 => {
                if let Mutation::Put { record, .. } = &mut bad.mutations[0] {
                    record.value["choice"] = "reject".into();
                }
            }
            6 => {
                for mutation in &mut bad.mutations {
                    if let Mutation::Put { record, .. } = mutation {
                        if record.value["document_type"] == "vcp_memory_head_v1" {
                            record.value["disputed"] = serde_json::json!([]);
                        }
                    }
                }
            }
            7 => {
                for mutation in &mut bad.mutations {
                    if let Mutation::Put { record, .. } = mutation {
                        if record.collection == Collection::IndexIntent {
                            record.value["transaction"] = TransactionId::new().to_string().into();
                        }
                    }
                }
            }
            8 => {
                for mutation in &mut bad.mutations {
                    if let Mutation::Put { record, .. } = mutation {
                        if record.value["document_type"] == "vcp_memory_result_v1" {
                            record.value["memory_seq"] = "2".into();
                        }
                    }
                }
            }
            _ => {
                for mutation in &mut bad.mutations {
                    if let Mutation::Put { record, .. } = mutation {
                        if record.collection == Collection::IndexIntent {
                            record.value["status"] = "ready".into();
                        }
                    }
                }
            }
        }
        assert!(state.prepare(&bad).is_err(), "accept variant {variant}");
    }
}
fn assert_receipt_result_bound(state: &State, tx: &Transaction) {
    for result in [
        CommandResult::Inspection { task: None },
        CommandResult::Accepted {
            revision: Revision::new(99),
        },
    ] {
        let mut forged = tx.clone();
        forged.command.as_mut().unwrap().result = result;
        assert!(state.prepare(&forged).is_err());
    }
}
#[test]
fn redaction_retains_only_review_identity_and_cannot_be_inserted_normally() {
    let state = State::default().prepare(&common::initial()).unwrap().0;
    let s = submission();
    let state = state
        .prepare(&review_tx(
            &state,
            row(Collection::Claim, s.id.as_str(), &s),
        ))
        .unwrap()
        .0;
    let d = decision(&s, Choice::Reject, Outcome::Rejected);
    let state = state
        .prepare(&review_tx(&state, row(Collection::Projection, &d.id, &d)))
        .unwrap()
        .0;
    for (collection, id) in [
        (Collection::Claim, s.id.as_str()),
        (Collection::Projection, d.id.as_str()),
    ] {
        let source = state.record(collection, id, &s.scope.workspace).unwrap();
        let redacted =
            crate::redaction_contract::redact_record(&state, source, DeletionEpoch::new(1))
                .unwrap();
        redacted.validate_shape().unwrap();
        let text = serde_json::to_string(&redacted).unwrap();
        assert!(!text.contains("private submission marker"));
        assert!(!text.contains("private review reason"));
        assert!(!text.contains("findings"));
        assert!(!text.contains("candidate\":"));
        let tx = Transaction {
            id: TransactionId::new(),
            expected_watermark: state.watermark,
            mutations: vec![Mutation::Put {
                expected: None,
                record: redacted,
            }],
            events: vec![],
            command: None,
        };
        assert!(state.prepare(&tx).is_err());
    }
}

#[tokio::test]
async fn qualified_review_redaction_rewrites_and_reopens_both_stores() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        store.transact(common::initial()).await.unwrap();
        let s = submission();
        store
            .transact(review_tx(
                store.state(),
                row(Collection::Claim, s.id.as_str(), &s),
            ))
            .await
            .unwrap();
        let d = decision(&s, Choice::Reject, Outcome::Rejected);
        store
            .transact(review_tx(
                store.state(),
                row(Collection::Projection, &d.id, &d),
            ))
            .await
            .unwrap();
        let mut workspace = common::workspace();
        workspace.revision = Revision::new(1);
        workspace.deletion = DeletionEpoch::new(1);
        let mut task = common::task();
        task.revision = Revision::new(1);
        task.state = vcp_domain::task::TaskState::Cancelled;
        let tx = Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![
                Mutation::Put {
                    expected: Some(Revision::ZERO),
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
                    expected: Some(Revision::ZERO),
                    record: Record::typed(
                        Collection::Task,
                        task.scope.task.as_str(),
                        workspace.id.clone(),
                        task.revision,
                        &task,
                    )
                    .unwrap(),
                },
            ],
            events: vec![],
            command: None,
        };
        store.transact(tx).await.unwrap();
        let source = store.state().clone();
        let mut erased = source.clone();
        for (collection, id) in [
            (Collection::Claim, s.id.as_str()),
            (Collection::Projection, d.id.as_str()),
        ] {
            let before = source.record(collection, id, &s.scope.workspace).unwrap();
            erased.records.insert(
                before.key(),
                crate::redaction_contract::redact_record(&source, before, workspace.deletion)
                    .unwrap(),
            );
        }
        erased.validate().unwrap();
        crate::redaction_contract::validate_rewrite(&source, &erased).unwrap();
        let mut forged = erased.clone();
        forged
            .records
            .get_mut(&key(Collection::Projection, &d.id))
            .unwrap()
            .value["original_digest"] = "f".repeat(64).into();
        assert!(crate::redaction_contract::validate_rewrite(&source, &forged).is_err());
        store.rewrite_base(erased.clone(), &[]).await.unwrap();
        store.close().await.unwrap();
        let store = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(store.state(), &erased);
        let text = serde_json::to_string(store.state()).unwrap();
        assert!(!text.contains("private submission marker"));
        assert!(!text.contains("private review reason"));
        store.close().await.unwrap();
    }
}
