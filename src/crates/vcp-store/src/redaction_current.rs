// SPDX-License-Identifier: Apache-2.0
//! Current redaction invariants with explicit fallible receipt evidence.
use super::*;
use crate::historical_facts::TransactionFacts;

pub(super) fn validate(
    state: crate::CurrentStateView<'_>,
    transactions: &mut impl TransactionFacts,
) -> Result<()> {
    for row in state.records.values() {
        if row.collection == Collection::Task {
            let task: Task = row.decode()?;
            if let Some(redaction) = task.redaction {
                if redaction.deletion > record_epoch(state.records, &row.workspace)? {
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
            if erased.deletion > record_epoch(state.records, &row.workspace)? {
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
                if transactions.watermark(&value.transaction)?.is_none() {
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

#[cfg(test)]
#[path = "redaction_current_tests.rs"]
mod tests;
