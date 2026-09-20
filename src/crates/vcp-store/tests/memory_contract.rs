// SPDX-License-Identifier: Apache-2.0
mod common;
use common::*;
use vcp_domain::{memory::*, *};
use vcp_store::contract::*;

fn proposal(outcome: Outcome) -> ProposalRecord {
    let proposal = Proposal {
        id: ProposalId::new(),
        command: CommandId::new(),
        claim: ClaimId::new(),
        scope: task().scope,
        actor: ActorId::new(),
        epochs: Epochs {
            authority: AuthorityRevision::ZERO,
            deletion: DeletionEpoch::ZERO,
            policy: PolicyRevision::ZERO,
        },
        registry_version: REGISTRY_VERSION,
        extractor: "fixture".into(),
        output_key: "one".into(),
        origins: vec![EventId::parse("created").unwrap()],
        subject: "module".into(),
        predicate: "design".into(),
        statement: "An inferred design".into(),
        value: ClaimValue::Architecture {
            decision: "isolate tools".into(),
            rationale: "observed boundary".into(),
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
    ProposalRecord {
        document_type: DocumentType::Proposal,
        schema_version: 1,
        id: proposal.id.clone(),
        scope: proposal.scope.clone(),
        revision: Revision::ZERO,
        proposal,
        resolution: Resolution {
            outcome,
            evidence_status: EvidenceStatus::Inferred,
            findings: vec![],
            conflicts: vec![],
            validated_evidence: vec![],
        },
        payload_digest: "a".repeat(64),
        recorded_at: Timestamp::new(101),
    }
}
fn proposal_record(proposal: &ProposalRecord) -> Record {
    Record::typed(
        Collection::Claim,
        proposal.id.to_string(),
        proposal.scope.workspace.clone(),
        proposal.revision,
        proposal,
    )
    .unwrap()
}
fn version(proposal: &ProposalRecord, watermark: Watermark) -> Version {
    Version {
        document_type: DocumentType::Version,
        schema_version: 1,
        id: ClaimVersionId::new(),
        scope: proposal.scope.clone(),
        revision: Revision::ZERO,
        proposal: proposal.proposal.clone(),
        memory_seq: MemorySeq::new(1),
        canonical_watermark: watermark,
        recorded_at: Timestamp::new(101),
        resolution: proposal.resolution.clone(),
    }
}
fn version_record(value: &Version) -> Record {
    Record::typed(
        Collection::Claim,
        value.id.to_string(),
        value.scope.workspace.clone(),
        value.revision,
        value,
    )
    .unwrap()
}
fn transaction(state: &State, records: Vec<Record>) -> Transaction {
    Transaction {
        id: TransactionId::new(),
        expected_watermark: state.watermark,
        mutations: records
            .into_iter()
            .map(|record| Mutation::Put {
                expected: None,
                record,
            })
            .collect(),
        events: vec![],
        command: None,
    }
}
fn initial_state() -> State {
    State::default().prepare(&initial()).unwrap().0
}

#[test]
fn typed_memory_rejects_identity_collection_revision_and_reference_spoofing() {
    let state = initial_state();
    let proposal = proposal(Outcome::Accepted);
    let record = proposal_record(&proposal);
    for field in ["id", "workspace", "revision", "collection"] {
        let mut malformed = record.clone();
        match field {
            "id" => malformed.id = "different".into(),
            "workspace" => malformed.workspace = WorkspaceId::new(),
            "revision" => malformed.revision = Revision::new(1),
            _ => malformed.collection = Collection::Projection,
        }
        assert!(malformed.validate_shape().is_err(), "{field}");
    }
    let mut value = version(&proposal, state.watermark.next().unwrap());
    let mut fake = record.clone();
    fake.value = serde_json::json!({"schema_version":1});
    assert!(state
        .prepare(&transaction(&state, vec![fake, version_record(&value)]))
        .is_err());
    value.proposal.statement = "different from immutable proposal".into();
    assert!(state
        .prepare(&transaction(&state, vec![record, version_record(&value)]))
        .is_err());
}

#[test]
fn rejected_allegations_do_not_become_authoritative_evidence_or_predecessor_links() {
    let state = initial_state();
    let mut rejected = proposal(Outcome::Rejected);
    rejected.proposal.evidence.push(EvidenceRef {
        artifact: ArtifactId::new(),
        sha256: "b".repeat(64),
        range: None,
        source: None,
        verification: None,
        kind: EvidenceKind::Source,
    });
    rejected.proposal.predecessor = Some(ClaimVersionId::new());
    rejected.proposal.correction_reason = Some("alleged correction of missing version".into());
    let record = proposal_record(&rejected);
    let refs = record.required_references().unwrap();
    assert!(!refs.contains(&key(
        Collection::Artifact,
        rejected.proposal.evidence[0].artifact.as_str()
    )));
    assert!(!refs.contains(&key(
        Collection::Claim,
        rejected.proposal.predecessor.as_ref().unwrap().as_str()
    )));
    state.prepare(&transaction(&state, vec![record])).unwrap();
    rejected
        .resolution
        .validated_evidence
        .push(rejected.proposal.evidence[0].artifact.clone());
    assert!(state
        .prepare(&transaction(&state, vec![proposal_record(&rejected)]))
        .is_err());
}

#[test]
fn immutable_versions_and_retry_results_survive_mutation_and_projection_drop_attempts() {
    let state = initial_state();
    let rejected = proposal(Outcome::Rejected);
    let result = ProposalResult {
        document_type: DocumentType::Result,
        schema_version: 1,
        id: rejected.proposal.command.clone(),
        scope: rejected.scope.clone(),
        revision: Revision::ZERO,
        proposal: rejected.id.clone(),
        payload_digest: rejected.payload_digest.clone(),
        transaction: TransactionId::new(),
        version: None,
        intent: None,
        resolution: rejected.resolution.clone(),
        memory_seq: MemorySeq::ZERO,
    };
    let result_record = Record::typed(
        Collection::Projection,
        result.id.to_string(),
        result.scope.workspace.clone(),
        Revision::ZERO,
        &result,
    )
    .unwrap();
    let accepted = proposal(Outcome::Accepted);
    let value = version(&accepted, state.watermark.next().unwrap());
    let immutable = vec![
        proposal_record(&rejected),
        result_record.clone(),
        proposal_record(&accepted),
        version_record(&value),
    ];
    let state = state
        .prepare(&transaction(&state, immutable.clone()))
        .unwrap()
        .0;
    for original in immutable {
        // Even a downgrade to legacy schema must not bypass immutability.
        let mut changed = original.clone();
        changed.revision = Revision::new(1);
        changed.value = serde_json::json!({"schema_version":1});
        let mut tx = transaction(&state, vec![]);
        tx.mutations.push(Mutation::Put {
            expected: Some(Revision::ZERO),
            record: changed,
        });
        assert!(state.prepare(&tx).is_err());
    }
    let mut tx = transaction(&state, vec![]);
    tx.mutations.push(Mutation::DropProjection {
        id: result_record.id,
        expected: Revision::ZERO,
    });
    assert!(state.prepare(&tx).is_err());
}

#[test]
fn heads_are_mutable_rebuildable_and_must_reference_versions_of_their_claim() {
    let state = initial_state();
    let proposal = proposal(Outcome::Accepted);
    let value = version(&proposal, state.watermark.next().unwrap());
    let mut head = Head {
        document_type: DocumentType::Head,
        schema_version: 1,
        id: proposal.proposal.claim.clone(),
        scope: proposal.scope.clone(),
        revision: Revision::ZERO,
        current: Some(value.id.clone()),
        disputed: vec![],
    };
    let head_record = |head: &Head| {
        Record::typed(
            Collection::Projection,
            head.id.to_string(),
            head.scope.workspace.clone(),
            head.revision,
            head,
        )
        .unwrap()
    };
    let state = state
        .prepare(&transaction(
            &state,
            vec![
                proposal_record(&proposal),
                version_record(&value),
                head_record(&head),
            ],
        ))
        .unwrap()
        .0;
    let mut other = head.clone();
    other.id = ClaimId::new();
    assert!(state
        .prepare(&transaction(&state, vec![head_record(&other)]))
        .is_err());
    head.revision = Revision::new(1);
    head.current = None;
    let mut update = transaction(&state, vec![]);
    update.mutations.push(Mutation::Put {
        expected: Some(Revision::ZERO),
        record: head_record(&head),
    });
    let state = state.prepare(&update).unwrap().0;
    let mut drop = transaction(&state, vec![]);
    drop.mutations.push(Mutation::DropProjection {
        id: head.id.to_string(),
        expected: head.revision,
    });
    state.prepare(&drop).unwrap();
    // Preexisting generic documents are not reinterpreted as typed memory.
    let legacy = Record::typed(
        Collection::Claim,
        "legacy-claim",
        workspace().id,
        Revision::ZERO,
        &serde_json::json!({"schema_version":1,"data":{"memory":"old"}}),
    )
    .unwrap();
    state.prepare(&transaction(&state, vec![legacy])).unwrap();
}

#[test]
fn sequence_advances_and_index_status_cannot_rewrite_committed_work() {
    let state = initial_state();
    let proposal = proposal(Outcome::Accepted);
    let version = version(&proposal, state.watermark.next().unwrap());
    let mut sequence = MemoryHead {
        document_type: DocumentType::Sequence,
        schema_version: 1,
        id: workspace().id,
        workspace: workspace().id,
        revision: Revision::ZERO,
        sequence: MemorySeq::new(1),
    };
    let mut intent = IndexIntent {
        deletion: None,
        document_type: DocumentType::IndexIntent,
        schema_version: 1,
        id: IndexIntentId::new(),
        scope: proposal.scope.clone(),
        revision: Revision::ZERO,
        transaction: TransactionId::new(),
        versions: vec![version.id.clone()],
        supersedes: vec![],
        memory_seq: version.memory_seq,
        canonical_watermark: version.canonical_watermark,
        status: IndexStatus::Pending,
    };
    let sequence_record = |value: &MemoryHead| {
        Record::typed(
            Collection::Projection,
            value.id.to_string(),
            value.workspace.clone(),
            value.revision,
            value,
        )
        .unwrap()
    };
    let intent_record = |value: &IndexIntent| {
        Record::typed(
            Collection::IndexIntent,
            value.id.to_string(),
            value.scope.workspace.clone(),
            value.revision,
            value,
        )
        .unwrap()
    };
    let state = state
        .prepare(&transaction(
            &state,
            vec![
                proposal_record(&proposal),
                version_record(&version),
                sequence_record(&sequence),
                intent_record(&intent),
            ],
        ))
        .unwrap()
        .0;
    sequence.revision = Revision::new(1);
    let mut tx = transaction(&state, vec![]);
    tx.mutations.push(Mutation::Put {
        expected: Some(Revision::ZERO),
        record: sequence_record(&sequence),
    });
    assert!(state.prepare(&tx).is_err());
    sequence.sequence = MemorySeq::new(2);
    tx.mutations[0] = Mutation::Put {
        expected: Some(Revision::ZERO),
        record: sequence_record(&sequence),
    };
    state.prepare(&tx).unwrap();
    intent.revision = Revision::new(1);
    intent.status = IndexStatus::Ready;
    tx.mutations[0] = Mutation::Put {
        expected: Some(Revision::ZERO),
        record: intent_record(&intent),
    };
    // P5-05 tightens Ready: acknowledgement must accompany its immutable
    // generation manifest and active pointer in the publication transaction.
    assert!(state.prepare(&tx).is_err());
    intent.status = IndexStatus::Failed;
    tx.mutations[0] = Mutation::Put {
        expected: Some(Revision::ZERO),
        record: intent_record(&intent),
    };
    state.prepare(&tx).unwrap();
    intent.transaction = TransactionId::new();
    tx.mutations[0] = Mutation::Put {
        expected: Some(Revision::ZERO),
        record: intent_record(&intent),
    };
    assert!(state.prepare(&tx).is_err());
}

#[test]
fn accepted_evidence_can_reference_prior_tasks_but_not_other_workspaces() {
    use vcp_domain::artifact::*;
    let mut initial = initial();
    let mut other = task();
    other.scope.task = TaskId::new();
    other.root = other.scope.task.clone();
    initial.mutations.push(Mutation::Put {
        expected: None,
        record: Record::typed(
            Collection::Task,
            other.scope.task.to_string(),
            other.scope.workspace.clone(),
            Revision::ZERO,
            &other,
        )
        .unwrap(),
    });
    let mut spec = spec();
    spec.scope = other.scope;
    let artifact = ArtifactDescriptor {
        spec,
        state: CaptureState::Complete,
        length: ByteCount::ZERO,
        sha256: "b".repeat(64),
        retained: vec![Range {
            start: ByteCount::ZERO,
            end: ByteCount::ZERO,
        }],
    };
    initial.mutations.push(Mutation::Put {
        expected: None,
        record: Record::typed(
            Collection::Artifact,
            artifact.spec.id.to_string(),
            artifact.spec.scope.workspace.clone(),
            Revision::ZERO,
            &artifact,
        )
        .unwrap(),
    });
    let state = State::default().prepare(&initial).unwrap().0;
    let mut proposal = proposal(Outcome::Accepted);
    proposal.proposal.evidence.push(EvidenceRef {
        artifact: artifact.spec.id.clone(),
        sha256: artifact.sha256.clone(),
        range: None,
        source: None,
        verification: None,
        kind: EvidenceKind::Source,
    });
    proposal
        .resolution
        .validated_evidence
        .push(artifact.spec.id.clone());
    let value = version(&proposal, state.watermark.next().unwrap());
    let tx = transaction(
        &state,
        vec![proposal_record(&proposal), version_record(&value)],
    );
    state.prepare(&tx).unwrap();
    let mut foreign = state;
    let mut w = workspace();
    w.id = WorkspaceId::new();
    let mut s = session();
    s.id = SessionId::new();
    s.workspace = w.id.clone();
    let mut t = task();
    t.scope.workspace = w.id.clone();
    t.scope.session = s.id.clone();
    t.scope.task = TaskId::new();
    t.root = t.scope.task.clone();
    let mut artifact = artifact;
    artifact.spec.scope = t.scope.clone();
    for record in [
        Record::typed(
            Collection::Workspace,
            w.id.to_string(),
            w.id.clone(),
            Revision::ZERO,
            &w,
        )
        .unwrap(),
        Record::typed(
            Collection::Session,
            s.id.to_string(),
            w.id.clone(),
            Revision::ZERO,
            &s,
        )
        .unwrap(),
        Record::typed(
            Collection::Task,
            t.scope.task.to_string(),
            w.id.clone(),
            Revision::ZERO,
            &t,
        )
        .unwrap(),
        Record::typed(
            Collection::Artifact,
            artifact.spec.id.to_string(),
            w.id.clone(),
            Revision::ZERO,
            &artifact,
        )
        .unwrap(),
    ] {
        foreign.records.insert(record.key(), record);
    }
    foreign.validate().unwrap();
    assert!(foreign.prepare(&tx).is_err());
    proposal.resolution.outcome = Outcome::Rejected;
    proposal.resolution.validated_evidence.clear();
    foreign
        .prepare(&transaction(&foreign, vec![proposal_record(&proposal)]))
        .unwrap();
}
