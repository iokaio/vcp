// SPDX-License-Identifier: Apache-2.0
use super::*;
#[path = "../tests/common/mod.rs"]
mod common;

// Explicit archival/reference adapter, never a live owner's representation.
fn finish_reference(
    source: &State,
    transaction: &Transaction,
    outcome: Outcome,
) -> Result<(State, Commit)> {
    let proposed = match outcome {
        Outcome::Proposed(proposed) => proposed,
        Outcome::Duplicate(receipt) => {
            return Ok((
                source.clone(),
                Commit {
                    version: FORMAT_VERSION,
                    transaction: transaction.clone(),
                    receipt,
                },
            ))
        }
    };
    assert_eq!(
        proposed.touched,
        transaction
            .mutations
            .iter()
            .map(|mutation| match mutation {
                Mutation::Put { record, .. } => record.key(),
                Mutation::DropProjection { id, .. } => key(Collection::Projection, id),
            })
            .collect()
    );
    let accounted = StateSize::measure(source)?.next_proposal(source, &proposed)?;
    let mut result = State {
        watermark: proposed.current.watermark,
        records: proposed.current.records,
        sequences: proposed.current.sequences,
        events: source.events.clone(),
        commands: source.commands.clone(),
        transactions: source.transactions.clone(),
    };
    result.events.extend(proposed.events);
    if let Some(receipt) = &proposed.receipt.command {
        result.commands.insert(
            command_key(&receipt.workspace, &receipt.command),
            receipt.clone(),
        );
    }
    result
        .transactions
        .insert(transaction.id.clone(), proposed.receipt.clone());
    assert_eq!(accounted.bytes(), StateSize::measure(&result)?.bytes());
    result.validate()?;
    let before = source.record_view();
    crate::accounting_contract::admission(before, &result, transaction)?;
    search_contract::publication(before, &result, transaction)?;
    agents_contract::publication(before, &result)?;
    Ok((
        result,
        Commit {
            version: FORMAT_VERSION,
            transaction: transaction.clone(),
            receipt: proposed.receipt,
        },
    ))
}
fn compare(source: &State, transaction: &Transaction) {
    let current = CurrentState::from_state(source);
    let before = current.clone();
    let candidate = propose(&current, transaction, &mut StateHistory(source))
        .and_then(|outcome| finish_reference(source, transaction, outcome));
    let expected = source
        .prepare_reference(transaction)
        .map_err(|error| error.to_string());
    assert_eq!(candidate.map_err(|error| error.to_string()), expected);
    assert_eq!(
        source
            .prepare(transaction)
            .map_err(|error| error.to_string()),
        expected
    );
    assert_eq!(current, before);
}

#[test]
fn current_proposal_and_explicit_archival_adapter_match_canonical_preparation() {
    let mut source = State::default();
    let initial = common::initial();
    compare(&source, &initial);
    source = source.prepare(&initial).unwrap().0;
    compare(&source, &initial);
    for index in 0..40 {
        let mut transaction = common::initial();
        transaction.id = TransactionId::parse(format!("proposed-{index}")).unwrap();
        transaction.expected_watermark = source.watermark;
        let mut task: Task = source
            .record(Collection::Task, "task", &common::workspace().id)
            .unwrap()
            .decode()
            .unwrap();
        let prior = task.revision;
        task.revision = prior.next().unwrap();
        task.reason = format!("revision {index}");
        transaction.mutations = vec![Mutation::Put {
            expected: Some(prior),
            record: Record::typed(
                Collection::Task,
                "task",
                task.scope.workspace.clone(),
                task.revision,
                &task,
            )
            .unwrap(),
        }];
        transaction.events[0].id = EventId::parse(format!("proposed-event-{index}")).unwrap();
        transaction.events[0].correlation =
            CommandId::parse(format!("proposed-command-{index}")).unwrap();
        transaction.command.as_mut().unwrap().command = transaction.events[0].correlation.clone();
        transaction.command.as_mut().unwrap().digest = format!("{index:064x}");
        for invalid in 0..8 {
            let mut altered = transaction.clone();
            match invalid {
                0 => altered.expected_watermark = Watermark::ZERO,
                1 => altered.events[0].id = EventId::parse("created").unwrap(),
                2 => {
                    altered.command.as_mut().unwrap().command = CommandId::parse("create").unwrap();
                    altered.events[0].correlation = CommandId::parse("create").unwrap();
                }
                3 => altered.command.as_mut().unwrap().digest = "g".repeat(64),
                4 => altered.events[0].session = SessionId::parse("other").unwrap(),
                5 => altered.mutations.push(altered.mutations[0].clone()),
                6 => {
                    if let Mutation::Put { record, .. } = &mut altered.mutations[0] {
                        record.revision = Revision::ZERO;
                    }
                }
                7 => altered.id = initial.id.clone(),
                _ => unreachable!(),
            }
            compare(&source, &altered);
        }
        compare(&source, &transaction);
        source = source.prepare(&transaction).unwrap().0;
        compare(&source, &transaction);
    }
}

#[test]
fn unavailable_historical_obligations_never_become_absence_or_a_publishable_result() {
    struct Failed<'a> {
        source: StateHistory<'a>,
        command_only: bool,
    }
    impl ForkHistory for Failed<'_> {
        fn has_session(&mut self, id: &SessionId) -> Result<bool> {
            self.source.has_session(id)
        }
        fn any_event(
            &mut self,
            id: &EventId,
            predicate: &dyn Fn(&EventEnvelope) -> bool,
        ) -> Result<bool> {
            self.source.any_event(id, predicate)
        }
    }
    impl PreparationHistory for Failed<'_> {
        fn transaction_receipt(&mut self, _: &TransactionId) -> Result<Option<Receipt>> {
            if self.command_only {
                Ok(None)
            } else {
                Err(Error::Unavailable("transaction read failed"))
            }
        }
        fn command_present(&mut self, _: &WorkspaceId, _: &CommandId) -> Result<bool> {
            Err(Error::Unavailable("command read failed"))
        }
        fn source_digest(&mut self) -> Result<String> {
            Err(Error::Unavailable("snapshot digest failed"))
        }
    }
    let source = State::default();
    let current = CurrentState::from_state(&source);
    for command_only in [false, true] {
        let error = propose(
            &current,
            &common::initial(),
            &mut Failed {
                source: StateHistory(&source),
                command_only,
            },
        );
        assert!(matches!(error, Err(Error::Unavailable(_))));
        assert!(current.records.is_empty());
    }
}
