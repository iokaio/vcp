// SPDX-License-Identifier: Apache-2.0
use super::*;
#[path = "../tests/common/mod.rs"]
mod common;

// Frozen complete-State record loop before the current-view extraction.
fn reference(state: &State) -> Result<()> {
    for row in state.records.values() {
        if row.collection == Collection::Task {
            let task: Task = row.decode()?;
            if let Some(redaction) = task.redaction {
                if redaction.deletion > epoch(state, &row.workspace)? {
                    return Err(Error::Corruption("task redaction exceeds deletion epoch"));
                }
            }
        }
        if row.collection == Collection::Settlement {
            let settlement: Settlement = row.decode()?;
            if settlement.redaction.is_none() && settlement.observation_digest.is_some() {
                return Err(Error::Corruption(
                    "settlement digest requires explicit redaction",
                ));
            }
        }
        let erased = match row.collection {
            Collection::Verification => {
                row.decode::<vcp_domain::verification::Verification>()?
                    .redaction
            }
            Collection::Effect => row.decode::<vcp_domain::effect::Effect>()?.redaction,
            Collection::Turn => row.decode::<vcp_domain::task::Turn>()?.redaction,
            Collection::Attempt => row.decode::<Attempt>()?.redaction,
            Collection::Settlement => row.decode::<Settlement>()?.redaction,
            _ => None,
        };
        if let Some(erased) = erased {
            erased.validate()?;
            if erased.deletion > epoch(state, &row.workspace)? {
                return Err(Error::Corruption("redaction epoch"));
            }
            // Protection is checked against the exact source at rewrite admission.
            // Later newly captured liability may coexist with erased old evidence.
            if row.collection == Collection::Verification {
                let value: vcp_domain::verification::Verification = row.decode()?;
                let retained_check_text = value.checks.iter().any(|check| {
                    !check.specification.is_empty()
                        || matches!(
                            &check.outcome,
                            vcp_domain::verification::CheckOutcome::Failed { reason }
                                | vcp_domain::verification::CheckOutcome::NotRun { reason }
                                if !reason.is_empty()
                        )
                });
                let retained_cost_text = matches!(
                    &value.cost,
                    vcp_domain::verification::CostCertainty::Uncertain { reason, .. }
                        if !reason.is_empty()
                );
                if !value.outstanding_issues.is_empty() || retained_check_text || retained_cost_text
                {
                    return Err(Error::Corruption("redacted verification payload"));
                }
            } else if row.collection == Collection::Attempt {
                row.decode::<Attempt>()?.validate()?;
            } else if row.collection == Collection::Settlement {
                let value: Settlement = row.decode()?;
                if value
                    .observation_digest
                    .as_deref()
                    .is_none_or(|digest| !valid_hash(digest))
                    || value
                        .observation
                        .correction
                        .as_ref()
                        .is_some_and(|correction| {
                            !correction.reason.is_empty()
                                || !correction.remaining_uncertainty.is_empty()
                        })
                {
                    return Err(Error::Corruption("redacted settlement narrative"));
                }
            } else if row.value["reason"] != "" {
                return Err(Error::Corruption("redacted lifecycle payload"));
            }
        }
        if kind(row)?.is_none() {
            continue;
        }
        shape(row)?;
        let workspace: Workspace = state
            .record(
                Collection::Workspace,
                row.workspace.as_str(),
                &row.workspace,
            )?
            .decode()?;
        let epoch = match kind(row)? {
            Some(vcp_domain::memory_review::REDACTED_SUBMISSION) => {
                row.decode::<vcp_domain::memory_review::RedactedSubmission>()?
                    .deletion
            }
            Some(vcp_domain::memory_review::REDACTED_DECISION) => {
                row.decode::<vcp_domain::memory_review::RedactedDecision>()?
                    .deletion
            }
            Some(vcp_domain::forecast::REDACTED) => {
                row.decode::<vcp_domain::forecast::RedactedSources>()?
                    .deletion
            }
            Some(redaction::ADVISORY) => row.decode::<RedactedAdvisory>()?.deletion,
            Some(redaction::OBSERVER) => row.decode::<RedactedObserver>()?.deletion,
            Some(redaction::PROPOSAL) => row.decode::<RedactedProposal>()?.deletion,
            Some(redaction::VERSION) => {
                let value: RedactedVersion = row.decode()?;
                let proposal: RedactedProposal = state
                    .record(Collection::Claim, value.proposal.as_str(), &row.workspace)?
                    .decode()?;
                if proposal.claim != value.claim
                    || proposal.scope != value.scope
                    || proposal.outcome != value.outcome
                    || proposal.sources != value.sources
                {
                    return Err(Error::Corruption("redacted version proposal lineage"));
                }
                if let Some(previous) = &value.predecessor {
                    let (claim, _, sequence) = version_identity(state, previous, &row.workspace)?;
                    if claim != value.claim || sequence >= value.memory_seq {
                        return Err(Error::Corruption("redacted predecessor lineage"));
                    }
                }
                value.deletion
            }
            Some(redaction::RESULT) => {
                let value: RedactedResult = row.decode()?;
                let proposal: RedactedProposal = state
                    .record(Collection::Claim, value.proposal.as_str(), &row.workspace)?
                    .decode()?;
                if proposal.command != value.id
                    || proposal.scope != value.scope
                    || proposal.payload_digest != value.payload_digest
                    || proposal.outcome != value.outcome
                {
                    return Err(Error::Corruption("redacted result proposal identity"));
                }
                if let Some(id) = &value.version {
                    let (_, proposal, sequence) = version_identity(state, id, &row.workspace)?;
                    if proposal != value.proposal || sequence != value.memory_seq {
                        return Err(Error::Corruption("redacted result version identity"));
                    }
                }
                if !state.transactions.contains_key(&value.transaction) {
                    return Err(Error::Corruption("redacted result receipt missing"));
                }
                value.deletion
            }
            _ => return Err(Error::Corruption("redacted entity expected")),
        };
        if epoch > workspace.deletion {
            return Err(Error::Corruption("redaction exceeds deletion epoch"));
        }
    }
    Ok(())
}

fn fixture() -> State {
    let (mut state, _) = State::default().prepare(&common::initial()).unwrap();
    let mut workspace = common::workspace();
    workspace.deletion = DeletionEpoch::new(1);
    state
        .records
        .get_mut(&key(Collection::Workspace, workspace.id.as_str()))
        .unwrap()
        .value = serde_json::to_value(&workspace).unwrap();
    let proposal = RedactedProposal {
        document_type: redaction::PROPOSAL.into(),
        schema_version: 1,
        id: ProposalId::parse("proposal").unwrap(),
        scope: common::task().scope,
        revision: Revision::ZERO,
        deletion: workspace.deletion,
        original_digest: "a".repeat(64),
        payload_digest: "b".repeat(64),
        command: CommandId::parse("create").unwrap(),
        claim: ClaimId::parse("claim").unwrap(),
        actor: ActorId::parse("human").unwrap(),
        sources: Sources::default(),
        predecessor: None,
        outcome: vcp_domain::memory::Outcome::Rejected,
        recorded_at: Timestamp::new(100),
        origin_output_keys: Vec::new(),
        extractor_digest: "c".repeat(64),
    };
    let result = RedactedResult {
        document_type: redaction::RESULT.into(),
        schema_version: 1,
        id: proposal.command.clone(),
        scope: proposal.scope.clone(),
        revision: Revision::ZERO,
        deletion: workspace.deletion,
        original_digest: "d".repeat(64),
        proposal: proposal.id.clone(),
        payload_digest: proposal.payload_digest.clone(),
        transaction: TransactionId::parse("initial").unwrap(),
        version: None,
        intent: None,
        outcome: proposal.outcome,
        memory_seq: MemorySeq::ZERO,
    };
    for record in [
        Record::typed(
            Collection::Claim,
            proposal.id.to_string(),
            workspace.id.clone(),
            Revision::ZERO,
            &proposal,
        )
        .unwrap(),
        Record::typed(
            Collection::Projection,
            result.id.to_string(),
            workspace.id,
            Revision::ZERO,
            &result,
        )
        .unwrap(),
    ] {
        state.records.insert(record.key(), record);
    }
    reference(&state).unwrap();
    state
}
fn equivalent(state: &State) {
    let mut facts = crate::historical_facts::StateTransactionFacts::new(state);
    assert_eq!(
        validate(state.into(), &mut facts).map_err(|error| error.to_string()),
        reference(state).map_err(|error| error.to_string())
    );
}

#[test]
fn current_redaction_matches_frozen_record_checks_and_receipt_presence() {
    equivalent(&State::default());
    let source = fixture();
    equivalent(&source);
    for field in [
        "transaction",
        "proposal",
        "payload_digest",
        "outcome",
        "deletion",
        "original_digest",
        "schema_version",
        "id",
    ] {
        for value in [
            serde_json::Value::Null,
            serde_json::json!("missing"),
            serde_json::json!(2),
        ] {
            let mut state = source.clone();
            state
                .records
                .get_mut(&key(Collection::Projection, "create"))
                .unwrap()
                .value[field] = value;
            equivalent(&state);
        }
    }
    for epoch in [0, 1, 2] {
        let mut state = source.clone();
        let mut workspace = common::workspace();
        workspace.deletion = DeletionEpoch::new(epoch);
        state
            .records
            .get_mut(&key(Collection::Workspace, workspace.id.as_str()))
            .unwrap()
            .value = serde_json::to_value(&workspace).unwrap();
        equivalent(&state);
    }
    let mut state = source.clone();
    state.transactions.clear();
    equivalent(&state);
    assert!(reference(&state).is_err());
    for epoch in [0, 1, 2] {
        let mut state = source.clone();
        let mut task = common::task();
        task.redaction = Some(vcp_domain::redaction::ContentRedaction {
            deletion: DeletionEpoch::new(epoch),
            original_digest: "e".repeat(64),
        });
        state
            .records
            .get_mut(&key(Collection::Task, task.scope.task.as_str()))
            .unwrap()
            .value = serde_json::to_value(&task).unwrap();
        equivalent(&state);
    }
}

#[test]
fn missing_unavailable_and_unrequested_receipt_evidence_are_distinct() {
    struct Unavailable;
    impl TransactionFacts for Unavailable {
        fn watermark(&mut self, _: &TransactionId) -> Result<Option<Watermark>> {
            Err(Error::Unavailable("receipt read failed"))
        }
    }
    let source = fixture();
    assert!(matches!(
        validate((&source).into(), &mut Unavailable),
        Err(Error::Unavailable("receipt read failed"))
    ));
    let (plain, _) = State::default().prepare(&common::initial()).unwrap();
    validate((&plain).into(), &mut Unavailable).unwrap();
}
