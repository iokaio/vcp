// SPDX-License-Identifier: Apache-2.0
//! Frozen complete-State preparation before the pure proposal adapter.
use super::*;
impl State {
    pub(crate) fn prepare_reference(&self, transaction: &Transaction) -> Result<(Self, Commit)> {
        Self::prepare_from_reference(Preparation::Borrowed(self), transaction, None, None)
    }
    fn prepare_from_reference(
        mut source: Preparation<'_>,
        transaction: &Transaction,
        diagnostics: Option<&mut crate::StoreDiagnostics>,
        mut size: Option<&mut StateSize>,
    ) -> Result<(Self, Commit)> {
        let bytes = canonical_bytes(transaction)?;
        if bytes.len() > MAX_TRANSACTION_BYTES {
            return Err(Error::Limit("transaction bytes"));
        }
        let digest = digest_bytes(&bytes);
        if let Some(receipt) = source.transactions.get(&transaction.id) {
            if receipt.digest != digest {
                return Err(Error::Conflict("transaction ID reused"));
            }
            return Ok((
                source.clone(),
                Commit {
                    version: FORMAT_VERSION,
                    transaction: transaction.clone(),
                    receipt: receipt.clone(),
                },
            ));
        }
        if transaction.expected_watermark != source.watermark {
            return Err(Error::Conflict("stale canonical watermark"));
        }
        crate::memory_review_contract::transaction(&*source, transaction)?;
        let watermark = source.watermark.next()?;
        let mut result = State {
            watermark: source.watermark,
            records: source.records.clone(),
            events: SharedStateValue::default(),
            commands: SharedStateValue::default(),
            transactions: SharedStateValue::default(),
            sequences: source.sequences.clone(),
        };
        result.watermark = watermark;
        let touched = mutation_preparation::prepare(
            crate::CurrentStateView::from(&*source),
            transaction,
            &mut result.records,
            &mut || crate::legacy_state_stream::digest(&source),
        )?;
        let mut first = SessionSeq::ZERO;
        let mut last = SessionSeq::ZERO;
        for event in &transaction.events {
            let sequence = result
                .sequences
                .get(&event.session)
                .copied()
                .unwrap_or_default()
                .next()?;
            result.sequences.insert(event.session.clone(), sequence);
            if transaction
                .command
                .as_ref()
                .is_some_and(|c| c.session == event.session)
            {
                if first == SessionSeq::ZERO {
                    first = sequence;
                }
                last = sequence;
            }
            result.events.push(EventEnvelope {
                redaction: None,
                version: 1,
                sequence,
                watermark,
                event: event.clone(),
            });
        }
        let command = if let Some(input) = &transaction.command {
            result.record(
                Collection::Session,
                input.session.as_str(),
                &input.workspace,
            )?;
            if input.digest.len() != 64
                || !input
                    .digest
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            {
                return Err(Error::Corruption("command digest"));
            }
            if crate::fork_contract::marked(transaction)
                || transaction.events.iter().any(|e| {
                    e.workspace != input.workspace
                        || e.session != input.session
                        || e.correlation != input.command
                })
            {
                crate::fork_contract::validate(&source, transaction)?;
            }
            let key = command_key(&input.workspace, &input.command);
            if source.commands.contains_key(&key) {
                return Err(Error::Conflict("command already committed"));
            }
            let receipt = CommandReceipt {
                version: 1,
                command: input.command.clone(),
                workspace: input.workspace.clone(),
                digest: input.digest.clone(),
                transaction: transaction.id.clone(),
                watermark,
                first_event: first,
                last_event: last,
                result: input.result.clone(),
            };
            result.commands.insert(key, receipt.clone());
            Some(receipt)
        } else {
            None
        };
        let receipt = Receipt {
            transaction: transaction.id.clone(),
            digest,
            watermark,
            command,
        };
        result
            .transactions
            .insert(transaction.id.clone(), receipt.clone());
        let next_size = size
            .as_deref()
            .map(|size| size.next(&source, &result, &touched))
            .transpose()?;
        // All checks requiring the complete prior State have finished. Keep
        // candidate validation complete while transferring privately owned
        // history. Borrowed preparation shares untouched collections and
        // detaches only components receiving a new event or receipt.
        let mut history = source.history();
        if !result.events.is_empty() {
            history.append(&mut result.events);
        }
        result.events = history;
        let mut commands = source.commands();
        for (key, receipt) in std::mem::take(&mut result.commands) {
            commands.insert(key, receipt);
        }
        result.commands = commands;
        let mut transactions = source.transactions();
        for (id, receipt) in std::mem::take(&mut result.transactions) {
            transactions.insert(id, receipt);
        }
        result.transactions = transactions;
        if let Some(diagnostics) = diagnostics {
            if next_size.is_some() {
                diagnostics.state_size_delta_updates =
                    diagnostics.state_size_delta_updates.saturating_add(1);
            }
            let started = std::time::Instant::now();
            let validation =
                result.validate_observed(next_size, Some(&mut diagnostics.validation_phases));
            diagnostics.validation.record(started, validation.is_ok());
            diagnostics.validation_input_records = diagnostics
                .validation_input_records
                .saturating_add(result.records.len() as u64);
            diagnostics.validation_input_events = diagnostics
                .validation_input_events
                .saturating_add(result.events.len() as u64);
            validation?;
        } else {
            result.validate_sized(next_size)?;
        }
        // The only receipt added above is the current transaction, whose prior
        // absence was checked before preparation. Excluding it recreates the
        // exact prior receipt lookup without cloning the historical map.
        let before = RecordView {
            watermark: source.watermark,
            records: &source.records,
            transactions: &result.transactions,
            excluded_transaction: Some(&transaction.id),
        };
        crate::accounting_contract::admission(before, &result, transaction)?;
        search_contract::publication(before, &result, transaction)?;
        agents_contract::publication(before, &result)?;
        if let (Some(size), Some(next_size)) = (size.as_deref_mut(), next_size) {
            *size = next_size;
        }
        Ok((
            result,
            Commit {
                version: FORMAT_VERSION,
                transaction: transaction.clone(),
                receipt,
            },
        ))
    }
}
