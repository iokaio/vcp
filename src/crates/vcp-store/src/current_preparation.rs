// SPDX-License-Identifier: Apache-2.0
//! Pure proposal construction with explicit historical obligations. A proposal
//! is not validated or publishable; full semantic/admission checks follow it.
use super::*;
use crate::{
    fork_contract::{ForkHistory, StateForkHistory},
    CurrentState,
};

pub(crate) trait PreparationHistory: ForkHistory {
    fn transaction_receipt(&mut self, id: &TransactionId) -> Result<Option<Receipt>>;
    fn command_present(&mut self, workspace: &WorkspaceId, command: &CommandId) -> Result<bool>;
    fn source_digest(&mut self) -> Result<String>;
}
pub(crate) struct StateHistory<'a>(pub(crate) &'a State);
impl ForkHistory for StateHistory<'_> {
    fn has_session(&mut self, session: &SessionId) -> Result<bool> {
        StateForkHistory::new(self.0).has_session(session)
    }
    fn any_event(
        &mut self,
        id: &EventId,
        predicate: &dyn Fn(&EventEnvelope) -> bool,
    ) -> Result<bool> {
        StateForkHistory::new(self.0).any_event(id, predicate)
    }
}
impl PreparationHistory for StateHistory<'_> {
    fn transaction_receipt(&mut self, id: &TransactionId) -> Result<Option<Receipt>> {
        Ok(self.0.transactions.get(id).cloned())
    }
    fn command_present(&mut self, workspace: &WorkspaceId, command: &CommandId) -> Result<bool> {
        Ok(self
            .0
            .commands
            .contains_key(&command_key(workspace, command)))
    }
    fn source_digest(&mut self) -> Result<String> {
        crate::legacy_state_stream::digest(self.0)
    }
}

pub(crate) enum Outcome {
    Duplicate(Receipt),
    Proposed(ProposedTransition),
}
pub(crate) struct ProposedTransition {
    pub(crate) current: CurrentState,
    pub(crate) events: Vec<EventEnvelope>,
    pub(crate) receipt: Receipt,
    pub(crate) touched: BTreeSet<String>,
}

pub(crate) fn propose(
    source: &CurrentState,
    transaction: &Transaction,
    history: &mut impl PreparationHistory,
) -> Result<Outcome> {
    let bytes = canonical_bytes(transaction)?;
    if bytes.len() > MAX_TRANSACTION_BYTES {
        return Err(Error::Limit("transaction bytes"));
    }
    let digest = digest_bytes(&bytes);
    if let Some(receipt) = history.transaction_receipt(&transaction.id)? {
        if receipt.digest != digest {
            return Err(Error::Conflict("transaction ID reused"));
        }
        return Ok(Outcome::Duplicate(receipt));
    }
    if transaction.expected_watermark != source.watermark {
        return Err(Error::Conflict("stale canonical watermark"));
    }
    crate::memory_review_contract::transaction(source, transaction)?;
    let watermark = source.watermark.next()?;
    let mut current =
        CurrentState::from_parts(watermark, source.records.clone(), source.sequences.clone());
    let touched = mutation_preparation::prepare(
        source.into(),
        transaction,
        &mut current.records,
        &mut || history.source_digest(),
    )?;
    let mut first = SessionSeq::ZERO;
    let mut last = SessionSeq::ZERO;
    let mut events = Vec::new();
    for event in &transaction.events {
        let sequence = current
            .sequences
            .get(&event.session)
            .copied()
            .unwrap_or_default()
            .next()?;
        current.sequences.insert(event.session.clone(), sequence);
        if transaction
            .command
            .as_ref()
            .is_some_and(|command| command.session == event.session)
        {
            if first == SessionSeq::ZERO {
                first = sequence;
            }
            last = sequence;
        }
        events.push(EventEnvelope {
            redaction: None,
            version: 1,
            sequence,
            watermark,
            event: event.clone(),
        });
    }
    let command = if let Some(input) = &transaction.command {
        current.record(
            Collection::Session,
            input.session.as_str(),
            &input.workspace,
        )?;
        if input.digest.len() != 64
            || !input
                .digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(Error::Corruption("command digest"));
        }
        if crate::fork_contract::marked(transaction)
            || transaction.events.iter().any(|event| {
                event.workspace != input.workspace
                    || event.session != input.session
                    || event.correlation != input.command
            })
        {
            crate::fork_contract::validate_with_history(source.into(), transaction, history)?;
        }
        if history.command_present(&input.workspace, &input.command)? {
            return Err(Error::Conflict("command already committed"));
        }
        Some(CommandReceipt {
            version: 1,
            command: input.command.clone(),
            workspace: input.workspace.clone(),
            digest: input.digest.clone(),
            transaction: transaction.id.clone(),
            watermark,
            first_event: first,
            last_event: last,
            result: input.result.clone(),
        })
    } else {
        None
    };
    Ok(Outcome::Proposed(ProposedTransition {
        current,
        events,
        touched,
        receipt: Receipt {
            transaction: transaction.id.clone(),
            digest,
            watermark,
            command,
        },
    }))
}

#[cfg(test)]
#[path = "current_preparation_tests.rs"]
mod tests;
