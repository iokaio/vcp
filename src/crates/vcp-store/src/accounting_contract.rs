// SPDX-License-Identifier: Apache-2.0
use crate::{contract::*, Error, Result};
use std::collections::BTreeMap;
use vcp_domain::{
    accounting::*,
    artifact::{ArtifactDescriptor, CaptureState},
    ids::*,
    task::Task,
};
fn sum(left: u64, right: u64) -> Result<u64> {
    left.checked_add(right)
        .ok_or(Error::Corruption("accounting overflow"))
}
fn lineage(state: &State, task: &Task) -> Result<Vec<Task>> {
    let mut result = vec![task.clone()];
    let mut parent = task.parent.clone();
    while let Some(id) = parent {
        if result.iter().any(|row| row.scope.task == id) {
            return Err(Error::Corruption("task ancestry cycle"));
        }
        let ancestor: Task = state
            .record(Collection::Task, id.as_str(), &task.scope.workspace)?
            .decode()?;
        parent = ancestor.parent.clone();
        result.push(ancestor);
    }
    Ok(result)
}
pub(crate) fn validate(state: &State) -> Result<()> {
    let mut ledgers = BTreeMap::<TaskId, Ledger>::new();
    let mut reservations = BTreeMap::<ReservationId, Reservation>::new();
    let mut attempts = BTreeMap::<AttemptId, Attempt>::new();
    let mut settlements = Vec::<Settlement>::new();
    for record in state.records.values() {
        match record.collection {
            Collection::Ledger => {
                let value: Ledger = record.decode()?;
                ledgers.insert(value.scope.task.clone(), value);
            }
            Collection::Reservation => {
                let value: Reservation = record.decode()?;
                reservations.insert(value.id.clone(), value);
            }
            Collection::Attempt => {
                let value: Attempt = record.decode()?;
                attempts.insert(value.id.clone(), value);
            }
            Collection::Settlement => settlements.push(record.decode()?),
            _ => {}
        }
    }
    let mut charged = BTreeMap::<AttemptId, (u64, u64)>::new();
    for settlement in &settlements {
        let attempt = attempts
            .get(&settlement.attempt)
            .ok_or(Error::Corruption("settlement attempt"))?;
        if settlement.scope != attempt.scope
            || settlement.observation.amount.currency != attempt.quote.amount.currency
        {
            return Err(Error::Access);
        }
        let raw: ArtifactDescriptor = state
            .record(
                Collection::Artifact,
                settlement.observation.raw.as_str(),
                &attempt.scope.workspace,
            )?
            .decode()?;
        if raw.state != CaptureState::Complete {
            return Err(Error::Corruption("usage evidence is incomplete"));
        }
        let entry = charged.entry(attempt.id.clone()).or_default();
        match settlement.direction {
            AdjustmentDirection::Debit => entry.0 = sum(entry.0, settlement.adjustment.get())?,
            AdjustmentDirection::Credit => entry.1 = sum(entry.1, settlement.adjustment.get())?,
            AdjustmentDirection::None => {
                if settlement.adjustment.get() != 0 {
                    return Err(Error::Corruption("zero adjustment"));
                }
            }
        }
        if !settlement.applied
            && (settlement.adjustment.get() != 0
                || settlement.direction != AdjustmentDirection::None)
        {
            return Err(Error::Corruption("unapplied usage changed spend"));
        }
    }
    for attempt in attempts.values() {
        let reservation = reservations
            .get(&attempt.reservation)
            .ok_or(Error::Corruption("attempt without reservation"))?;
        let ledger = ledgers
            .get(&attempt.root)
            .ok_or(Error::Corruption("attempt root ledger"))?;
        let task: Task = state
            .record(
                Collection::Task,
                attempt.scope.task.as_str(),
                &attempt.scope.workspace,
            )?
            .decode()?;
        if task.root != attempt.root
            || task.scope != attempt.scope
            || reservation.attempt != attempt.id
            || reservation.scope != attempt.scope
            || reservation.root != attempt.root
            || reservation.phase != attempt.phase
            || reservation.charged != attempt.charged
            || reservation.role != attempt.role
            || reservation.amount != attempt.quote.amount
            || ledger.currency != attempt.quote.amount.currency
        {
            return Err(Error::Corruption("attempt/reservation/root mismatch"));
        }
        if attempt.quote.price.currency != attempt.quote.amount.currency
            || !valid_hash(&attempt.quote.price.id)
            || !valid_hash(&attempt.quote.price.capability)
        {
            return Err(Error::Corruption("quote identity or currency"));
        }
        let mut quote_total = 0u128;
        for (kind, units) in attempt.quote.bounds.disjoint()? {
            let rate = attempt
                .quote
                .price
                .rates
                .get(&kind)
                .ok_or(Error::Corruption("unknown quote category"))?;
            let denominator = rate.per_units.get() as u128;
            if denominator == 0 {
                return Err(Error::Corruption("zero price unit"));
            }
            let numerator = rate.micros.get() as u128 * units.get() as u128;
            quote_total = quote_total
                .checked_add(numerator / denominator + u128::from(numerator % denominator != 0))
                .ok_or(Error::Corruption("quote overflow"))?;
        }
        if quote_total != attempt.quote.amount.micros.get() as u128 {
            return Err(Error::Corruption(
                "reservation differs from checked price bounds",
            ));
        }
        if let Some(previous) = &attempt.previous {
            let previous = attempts
                .get(previous)
                .ok_or(Error::Corruption("retry predecessor"))?;
            if previous.scope != attempt.scope || previous.root != attempt.root {
                return Err(Error::Access);
            }
        }
        let totals = charged.get(&attempt.id).copied().unwrap_or_default();
        if totals.0.checked_sub(totals.1) != Some(attempt.charged.get()) {
            return Err(Error::Corruption(
                "attempt charges differ from immutable adjustments",
            ));
        }
        let request: ArtifactDescriptor = state
            .record(
                Collection::Artifact,
                attempt.request.as_str(),
                &attempt.scope.workspace,
            )?
            .decode()?;
        if request.state != CaptureState::Complete || request.sha256 != attempt.request_digest {
            return Err(Error::Corruption(
                "request must be fully captured before admission",
            ));
        }
        if let Some(send) = &attempt.send_intent {
            if !state.events.iter().any(|event| {
                event.event.id == *send
                    && event.event.workspace == attempt.scope.workspace
                    && event.event.session == attempt.scope.session
                    && event.event.task.as_ref() == Some(&attempt.scope.task)
                    && event.event.kind == vcp_protocol::event::EventKind::AttemptSubmitted
            }) {
                return Err(Error::Corruption("durable send intent missing"));
            }
        }
    }
    for reservation in reservations.values() {
        if !attempts
            .get(&reservation.attempt)
            .is_some_and(|a| a.reservation == reservation.id)
        {
            return Err(Error::Corruption("orphan reservation"));
        }
    }
    for ledger in ledgers.values() {
        let root: Task = state
            .record(
                Collection::Task,
                ledger.scope.task.as_str(),
                &ledger.scope.workspace,
            )?
            .decode()?;
        if root.parent.is_some() || root.root != ledger.scope.task || root.scope != ledger.scope {
            return Err(Error::Corruption("child cannot own a second ledger"));
        }
        for id in ledger.allocations.keys() {
            let child: Task = state
                .record(Collection::Task, id.as_str(), &ledger.scope.workspace)?
                .decode()?;
            if child.root != root.root || child.scope == root.scope {
                return Err(Error::Corruption("child allocation root"));
            }
        }
        let mut settled = 0;
        let mut active = 0;
        let mut unresolved = 0;
        for reservation in reservations.values().filter(|r| r.root == root.root) {
            if reservation.amount.currency != ledger.currency {
                return Err(Error::Corruption("mixed currency root"));
            }
            settled = sum(settled, reservation.charged.get())?;
            match reservation.phase {
                ReservationState::Created | ReservationState::Submitted => {
                    active = sum(active, reservation.liability.get())?
                }
                ReservationState::ReconciliationPending => {
                    unresolved = sum(unresolved, reservation.liability.get())?
                }
                _ => {}
            }
        }
        let total = sum(
            sum(sum(settled, active)?, unresolved)?,
            ledger.protected.get(),
        )?;
        if ledger.settled.get() != settled
            || ledger.active.get() != active
            || ledger.unresolved.get() != unresolved
            || ledger.overrun != (total > ledger.cap.get())
        {
            return Err(Error::Corruption(
                "root aggregate differs from canonical reservations",
            ));
        }
    }
    Ok(())
}

pub(crate) fn transition(previous: &Record, next: &Record) -> Result<()> {
    if previous.collection == Collection::Attempt {
        let before: Attempt = previous.decode()?;
        let after: Attempt = next.decode()?;
        if before.scope != after.scope
            || before.root != after.root
            || before.reservation != after.reservation
            || before.role != after.role
            || before.agent != after.agent
            || before.previous != after.previous
            || before.request != after.request
            || before.request_digest != after.request_digest
            || before.admission_digest != after.admission_digest
            || before.steering != after.steering
            || before.quote != after.quote
            || before.admitted_policy != after.admitted_policy
            || before
                .send_intent
                .as_ref()
                .is_some_and(|id| after.send_intent.as_ref() != Some(id))
        {
            return Err(Error::Conflict("immutable attempt admission"));
        }
        use ReservationState::*;
        if !matches!(
            (before.phase, after.phase),
            (Created, Submitted | Released)
                | (
                    Submitted,
                    Submitted | Settled | ReconciliationPending | ExplicitlyResolved
                )
                | (
                    ReconciliationPending,
                    ReconciliationPending | Settled | ExplicitlyResolved
                )
                | (
                    Settled,
                    Settled | ReconciliationPending | ExplicitlyResolved
                )
                | (
                    ExplicitlyResolved,
                    ExplicitlyResolved | Settled | ReconciliationPending
                )
        ) {
            return Err(Error::Conflict("accounting lifecycle transition"));
        }
    }
    if previous.collection == Collection::Reservation {
        let before: Reservation = previous.decode()?;
        let after: Reservation = next.decode()?;
        if before.scope != after.scope
            || before.root != after.root
            || before.attempt != after.attempt
            || before.amount != after.amount
            || before.role != after.role
            || before.day != after.day
            || before.protected_draw != after.protected_draw
            || after.protected_returned < before.protected_returned
        {
            return Err(Error::Conflict("immutable reservation admission"));
        }
    }
    if previous.collection == Collection::Ledger {
        let before: Ledger = previous.decode()?;
        let after: Ledger = next.decode()?;
        if before.scope != after.scope
            || before.currency != after.currency
            || before.daily != after.daily
        {
            return Err(Error::Conflict("immutable ledger currency or daily scope"));
        }
        if (before.cap != after.cap || before.allocations != after.allocations)
            && after.policy != before.policy.next()?
        {
            return Err(Error::Conflict("budget change needs new policy revision"));
        }
    }
    Ok(())
}
pub(crate) fn admission(before: &State, after: &State, transaction: &Transaction) -> Result<()> {
    for mutation in &transaction.mutations {
        if let Mutation::Put {
            expected: None,
            record,
        } = mutation
        {
            if record.collection == Collection::Attempt {
                let attempt: Attempt = record.decode()?;
                let ledger: Ledger = after
                    .record(
                        Collection::Ledger,
                        attempt.root.as_str(),
                        &attempt.scope.workspace,
                    )?
                    .decode()?;
                let task: Task = before
                    .record(
                        Collection::Task,
                        attempt.scope.task.as_str(),
                        &attempt.scope.workspace,
                    )?
                    .decode()?;
                if attempt.phase != ReservationState::Created
                    || attempt.charged.get() != 0
                    || attempt.admitted_policy != ledger.policy
                    || attempt.steering != task.steering
                    || task.state != vcp_domain::task::TaskState::Running
                    || ledger.overrun
                {
                    return Err(Error::Conflict("invalid initial accounting admission"));
                }
                let ancestors = lineage(before, &task)?;
                if ancestors
                    .iter()
                    .any(|row| row.state != vcp_domain::task::TaskState::Running)
                {
                    return Err(Error::Conflict("held ancestor cannot admit work"));
                }
                if let Some(previous) = &attempt.previous {
                    let prior: Attempt = before
                        .record(
                            Collection::Attempt,
                            previous.as_str(),
                            &attempt.scope.workspace,
                        )?
                        .decode()?;
                    if matches!(
                        prior.phase,
                        ReservationState::Created | ReservationState::Submitted
                    ) {
                        return Err(Error::Conflict("retry predecessor still live"));
                    }
                }
                let reservation: Reservation = after
                    .record(
                        Collection::Reservation,
                        attempt.reservation.as_str(),
                        &attempt.scope.workspace,
                    )?
                    .decode()?;
                if reservation.protected_draw.get() != 0
                    && attempt.role != RequestRole::Verification
                {
                    return Err(Error::Conflict("protected funds are for verification"));
                }
                let mut daily_total = ledger.protected.get();
                let mut allocated = BTreeMap::<TaskId, u64>::new();
                for record in after.records.values().filter(|row| {
                    row.collection == Collection::Reservation
                        && row.workspace == attempt.scope.workspace
                }) {
                    let row: Reservation = record.decode()?;
                    if row.root != attempt.root {
                        continue;
                    }
                    daily_total = sum(
                        daily_total,
                        if row.day == reservation.day {
                            sum(row.charged.get(), row.liability.get())?
                        } else {
                            row.liability.get()
                        },
                    )?;
                    let row_task: Task = after
                        .record(
                            Collection::Task,
                            row.scope.task.as_str(),
                            &row.scope.workspace,
                        )?
                        .decode()?;
                    for ancestor in lineage(after, &row_task)? {
                        if ledger.allocations.contains_key(&ancestor.scope.task) {
                            let total = allocated.entry(ancestor.scope.task).or_default();
                            *total = sum(*total, sum(row.charged.get(), row.liability.get())?)?;
                        }
                    }
                }
                if ledger
                    .daily
                    .as_ref()
                    .is_some_and(|daily| daily_total > daily.cap.get())
                {
                    return Err(Error::Conflict("daily admission cap"));
                }
                for ancestor in ancestors {
                    if ledger
                        .allocations
                        .get(&ancestor.scope.task)
                        .is_some_and(|cap| {
                            allocated.get(&ancestor.scope.task).copied().unwrap_or(0) > cap.get()
                        })
                    {
                        return Err(Error::Conflict("child admission cap"));
                    }
                }
            }
        }
    }
    // Reducing a protected reserve requires either a new owner policy or the
    // matching verification draw. Returning an unsent draw restores it once.
    for mutation in &transaction.mutations {
        if let Mutation::Put {
            expected: Some(_),
            record,
        } = mutation
        {
            if record.collection == Collection::Ledger {
                let next: Ledger = record.decode()?;
                let prior: Ledger = before
                    .record(Collection::Ledger, &record.id, &record.workspace)?
                    .decode()?;
                if next.policy == prior.policy {
                    let mut draw = 0;
                    let mut returned = 0;
                    for change in &transaction.mutations {
                        if let Mutation::Put { expected, record } = change {
                            if record.collection == Collection::Reservation {
                                let row: Reservation = record.decode()?;
                                if row.root != next.scope.task {
                                    continue;
                                }
                                if expected.is_none() {
                                    draw = sum(draw, row.protected_draw.get())?;
                                } else {
                                    let old: Reservation = before
                                        .record(
                                            Collection::Reservation,
                                            &record.id,
                                            &record.workspace,
                                        )?
                                        .decode()?;
                                    returned = sum(
                                        returned,
                                        row.protected_returned
                                            .get()
                                            .checked_sub(old.protected_returned.get())
                                            .ok_or(Error::Corruption(
                                                "protected return decreased",
                                            ))?,
                                    )?;
                                }
                            }
                        }
                    }
                    if prior
                        .protected
                        .get()
                        .checked_sub(draw)
                        .and_then(|value| value.checked_add(returned))
                        != Some(next.protected.get())
                    {
                        return Err(Error::Conflict(
                            "protected reserve changed without authority",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}
