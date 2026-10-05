// SPDX-License-Identifier: Apache-2.0
use crate::{contract::*, CurrentStateView, Error, Result};
use std::collections::BTreeMap;
use vcp_domain::{
    accounting::*,
    artifact::{ArtifactDescriptor, CaptureState},
    ids::*,
    task::Task,
};
#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod history_test_common;
#[cfg(test)]
#[path = "accounting_resumption_tests.rs"]
mod resumption_tests;
fn sum(left: u64, right: u64) -> Result<u64> {
    left.checked_add(right)
        .ok_or(Error::Corruption("accounting overflow"))
}
fn lineage(records: &BTreeMap<String, Record>, task: &Task) -> Result<Vec<Task>> {
    let mut result = vec![task.clone()];
    let mut parent = task.parent.clone();
    while let Some(id) = parent {
        if result.iter().any(|row| row.scope.task == id) {
            return Err(Error::Corruption("task ancestry cycle"));
        }
        let record = records
            .get(&key(Collection::Task, id.as_str()))
            .ok_or(Error::Conflict("record not found"))?;
        if record.workspace != task.scope.workspace {
            return Err(Error::Access);
        }
        let ancestor: Task = record.decode()?;
        parent = ancestor.parent.clone();
        result.push(ancestor);
    }
    Ok(result)
}
pub(crate) fn validate(state: &State) -> Result<()> {
    validate_with_history(
        state.into(),
        &mut crate::historical_facts::StateEventFacts::new(state),
    )
}

pub(crate) fn validate_with_history(
    state: CurrentStateView<'_>,
    history: &mut impl crate::historical_facts::EventFacts,
) -> Result<()> {
    let validation = Validation::new(state)?;
    for attempt in validation.attempts() {
        validation.attempt(attempt, history)?;
    }
    validation.finish()
}

/// One immutable accounting pass. Resolving a missing send-intent may retry
/// its current attempt without decoding every ledger or rechecking earlier
/// attempts. No fact or successful predicate survives the current state cut.
pub(crate) struct Validation<'a> {
    state: CurrentStateView<'a>,
    ledgers: BTreeMap<TaskId, Ledger>,
    reservations: BTreeMap<ReservationId, Reservation>,
    attempts: BTreeMap<AttemptId, Attempt>,
    charged: BTreeMap<AttemptId, (u64, u64)>,
}
impl<'a> Validation<'a> {
    pub(crate) fn new(state: CurrentStateView<'a>) -> Result<Self> {
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
            if raw.state != CaptureState::Complete && raw.state != CaptureState::Purged {
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
        Ok(Self {
            state,
            ledgers,
            reservations,
            attempts,
            charged,
        })
    }
    pub(crate) fn attempts(&self) -> impl Iterator<Item = &Attempt> {
        self.attempts.values()
    }
    pub(crate) fn attempt(
        &self,
        attempt: &Attempt,
        history: &mut impl crate::historical_facts::EventFacts,
    ) -> Result<()> {
        let state = self.state;
        let ledgers = &self.ledgers;
        let reservations = &self.reservations;
        let attempts = &self.attempts;
        let charged = &self.charged;
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
        let mut missing = 0u64;
        for (kind, units) in attempt.quote.bounds.disjoint()? {
            let Some(rate) = attempt.quote.price.rates.get(&kind) else {
                missing += 1;
                continue;
            };
            let denominator = rate.per_units.get() as u128;
            if denominator == 0 {
                return Err(Error::Corruption("zero price unit"));
            }
            let numerator = rate.micros.get() as u128 * units.get() as u128;
            quote_total = quote_total
                .checked_add(numerator / denominator + u128::from(numerator % denominator != 0))
                .ok_or(Error::Corruption("quote overflow"))?;
        }
        if quote_total != attempt.quote.amount.micros.known_component().get() as u128
            || missing != attempt.quote.amount.micros.unknown_components().get()
            || (missing > 0 && attempt.quote.method != "ceil_disjoint_bounds_unknown_v2")
        {
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
        if (request.state != CaptureState::Complete && request.state != CaptureState::Purged)
            || request.sha256 != attempt.request_digest
        {
            return Err(Error::Corruption(
                "request must be fully captured before admission",
            ));
        }
        if let Some(send) = &attempt.send_intent {
            send_intent(history, send, &attempt.scope)?;
        }
        Ok(())
    }
    pub(crate) fn finish(&self) -> Result<()> {
        let state = self.state;
        let ledgers = &self.ledgers;
        let reservations = &self.reservations;
        let attempts = &self.attempts;
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
            if root.parent.is_some() || root.root != ledger.scope.task || root.scope != ledger.scope
            {
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
            let mut active = EstimatedMicros::ZERO;
            let mut unresolved = EstimatedMicros::ZERO;
            for reservation in reservations.values().filter(|r| r.root == root.root) {
                if reservation.amount.currency != ledger.currency {
                    return Err(Error::Corruption("mixed currency root"));
                }
                settled = sum(settled, reservation.charged.get())?;
                match reservation.phase {
                    ReservationState::Created | ReservationState::Submitted => {
                        active = active.checked_add(reservation.liability)?
                    }
                    ReservationState::ReconciliationPending => {
                        unresolved = unresolved.checked_add(reservation.liability)?
                    }
                    _ => {}
                }
            }
            let total = active
                .checked_add(unresolved)?
                .checked_add(vcp_domain::Micros::new(settled).into())?
                .checked_add(ledger.protected.into())?;
            let overrun = if let Some(cap) = ledger.cap.finite() {
                total
                    .known()
                    .ok_or(Error::Corruption("unpriced liability under finite cap"))?
                    > *cap
            } else {
                false
            };
            if ledger.settled.get() != settled
                || ledger.active != active
                || ledger.unresolved != unresolved
                || ledger.overrun != overrun
            {
                return Err(Error::Corruption(
                    "root aggregate differs from canonical reservations",
                ));
            }
        }
        Ok(())
    }
}

fn send_intent(
    history: &mut impl crate::historical_facts::EventFacts,
    send: &EventId,
    scope: &vcp_domain::workspace::Scope,
) -> Result<()> {
    if !history.any(send, &|event| {
        event.id == *send
            && event.workspace == scope.workspace
            && event.session == scope.session
            && event.task.as_ref() == Some(&scope.task)
            && event.kind == vcp_protocol::event::EventKind::AttemptSubmitted
    })? {
        return Err(Error::Corruption("durable send intent missing"));
    }
    Ok(())
}

pub(crate) fn transition(previous: &Record, next: &Record) -> Result<()> {
    if previous.collection == Collection::Attempt {
        let before: Attempt = previous.decode()?;
        let after: Attempt = next.decode()?;
        if before.redaction != after.redaction
            || before.redacted_at_revision != after.redacted_at_revision
            || before.scope != after.scope
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
pub(crate) fn admission<'a>(
    before: RecordView<'_>,
    after: impl Into<CurrentStateView<'a>>,
    transaction: &Transaction,
) -> Result<()> {
    admission_records(
        PriorRecords {
            records: before.records,
        },
        after.into(),
        transaction,
    )
}
pub(crate) fn admission_current(
    before: CurrentStateView<'_>,
    after: CurrentStateView<'_>,
    transaction: &Transaction,
) -> Result<()> {
    admission_records(
        PriorRecords {
            records: before.records,
        },
        after,
        transaction,
    )
}
// Admission needs prior records only. Do not manufacture historical fields or
// sequence metadata to adapt the existing archival publication view.
struct PriorRecords<'a> {
    records: &'a BTreeMap<String, Record>,
}
impl<'a> PriorRecords<'a> {
    fn record(
        &self,
        collection: Collection,
        id: &str,
        workspace: &WorkspaceId,
    ) -> Result<&'a Record> {
        let record = self
            .records
            .get(&key(collection, id))
            .ok_or(Error::Conflict("record not found"))?;
        if &record.workspace != workspace {
            return Err(Error::Access);
        }
        Ok(record)
    }
}
fn admission_records(
    before: PriorRecords<'_>,
    after: CurrentStateView<'_>,
    transaction: &Transaction,
) -> Result<()> {
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
                    || (!ledger.cap.is_unbounded() && attempt.quote.amount.micros.known().is_none())
                {
                    return Err(Error::Conflict("invalid initial accounting admission"));
                }
                let ancestors = lineage(before.records, &task)?;
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
                    if !ledger.cap.is_unbounded() {
                        let liability = row
                            .liability
                            .known()
                            .ok_or(Error::Conflict("unpriced finite admission"))?
                            .get();
                        daily_total = sum(
                            daily_total,
                            if row.day == reservation.day {
                                sum(row.charged.get(), liability)?
                            } else {
                                liability
                            },
                        )?;
                    }
                    let row_task: Task = after
                        .record(
                            Collection::Task,
                            row.scope.task.as_str(),
                            &row.scope.workspace,
                        )?
                        .decode()?;
                    for ancestor in lineage(after.records, &row_task)? {
                        if !ledger.cap.is_unbounded()
                            && ledger.allocations.contains_key(&ancestor.scope.task)
                        {
                            let total = allocated.entry(ancestor.scope.task).or_default();
                            *total = sum(
                                *total,
                                sum(
                                    row.charged.get(),
                                    row.liability
                                        .known()
                                        .ok_or(Error::Conflict("unpriced finite admission"))?
                                        .get(),
                                )?,
                            )?;
                        }
                    }
                }
                if ledger.daily.as_ref().is_some_and(|daily| {
                    !ledger.cap.is_unbounded() && daily_total > daily.cap.get()
                }) {
                    return Err(Error::Conflict("daily admission cap"));
                }
                for ancestor in ancestors {
                    if ledger
                        .allocations
                        .get(&ancestor.scope.task)
                        .is_some_and(|cap| {
                            !ledger.cap.is_unbounded()
                                && allocated.get(&ancestor.scope.task).copied().unwrap_or(0)
                                    > cap.get()
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

/// A historical erasure marker cannot be minted or removed by an ordinary Put.
/// A later observation may update its retained accounting facts only when that
/// transaction also admits exactly one fresh immutable usage observation.
pub(crate) fn redacted_attempt_update<'a>(
    before: impl Into<CurrentStateView<'a>>,
    transaction: &Transaction,
    record: &Record,
) -> Result<bool> {
    let before = before.into();
    if record.collection != Collection::Attempt {
        return Ok(false);
    }
    let next: Attempt = record.decode()?;
    if next.redaction.is_none() {
        return Ok(false);
    }
    let previous: Attempt = before
        .record(Collection::Attempt, &record.id, &record.workspace)?
        .decode()?;
    if previous.redaction.is_none()
        || previous.redaction != next.redaction
        || previous.redacted_at_revision != next.redacted_at_revision
    {
        return Err(Error::Conflict("accounting erasure marker changed"));
    }
    let mut observations = Vec::new();
    for mutation in &transaction.mutations {
        if let Mutation::Put {
            expected: None,
            record: candidate,
        } = mutation
        {
            if candidate.collection == Collection::Settlement {
                let settlement: Settlement = candidate.decode()?;
                if settlement.attempt == next.id {
                    if settlement.scope != next.scope
                        || settlement.redaction.is_some()
                        || settlement.observation_digest.is_some()
                        || before.records.contains_key(&candidate.key())
                    {
                        return Err(Error::Conflict("late accounting evidence identity"));
                    }
                    observations.push(settlement);
                }
            }
        }
    }
    if observations.len() != 1 {
        return Err(Error::Conflict(
            "redacted accounting requires fresh observation",
        ));
    }
    let settlement = &observations[0];
    let mut uncertainty = previous.uncertain.clone();
    let mut phase = previous.phase;
    if settlement.applied && settlement.observation.final_usage {
        uncertainty = settlement
            .observation
            .correction
            .as_ref()
            .map(|value| value.remaining_uncertainty.clone())
            .filter(|value| !value.trim().is_empty());
        phase = if uncertainty.is_some() {
            ReservationState::ExplicitlyResolved
        } else {
            ReservationState::Settled
        };
    } else if settlement.applied
        && matches!(
            previous.phase,
            ReservationState::Settled | ReservationState::ExplicitlyResolved
        )
    {
        uncertainty = Some("new usage observation is not final".into());
        phase = ReservationState::ReconciliationPending;
    }
    if next.phase != phase
        || next.uncertain != uncertainty
        || next.provider_request.as_deref()
            != Some(settlement.observation.provider_request.as_str())
    {
        return Err(Error::Conflict(
            "late accounting narrative lacks fresh provenance",
        ));
    }
    Ok(true)
}

#[cfg(test)]
mod history_tests {
    use super::history_test_common as common;
    use super::*;
    use crate::historical_facts::{EventFact, EventFacts, StateEventFacts};

    struct Facts {
        rows: Vec<EventFact>,
        fail: bool,
    }
    impl EventFacts for Facts {
        fn last(&mut self, _: &EventId) -> Result<Option<EventFact>> {
            panic!("send intent requires any-match semantics")
        }
        fn any(&mut self, id: &EventId, predicate: &dyn Fn(&EventFact) -> bool) -> Result<bool> {
            if self.fail {
                return Err(Error::Unavailable("injected history failure"));
            }
            Ok(self.rows.iter().filter(|row| &row.id == id).any(predicate))
        }
    }
    #[test]
    fn send_intent_facts_preserve_exact_scope_kind_and_read_errors() {
        let (base, _) = State::default().prepare(&common::initial()).unwrap();
        let scope = common::task().scope;
        let mut submitted = base.events[0].clone();
        submitted.event.kind = vcp_protocol::event::EventKind::AttemptSubmitted;
        let id = submitted.event.id.clone();
        for variant in 0..8 {
            let mut state = base.clone();
            state.events.clear();
            let mut row = submitted.clone();
            match variant {
                1 => row.event.workspace = WorkspaceId::new(),
                2 => row.event.session = SessionId::new(),
                3 => row.event.task = Some(TaskId::new()),
                4 => row.event.kind = vcp_protocol::event::EventKind::TaskCreated,
                5 => row.event.id = EventId::new(),
                6 => row.event.task = None,
                _ => {}
            }
            state.events.push(row);
            if variant == 7 {
                // Standalone corrupt duplicates keep the original any-match
                // result; the global event validator separately rejects them.
                state.events.push(submitted.clone());
                state.events[0].event.workspace = WorkspaceId::new();
            }
            let expected = state.events.iter().any(|event| {
                event.event.id == id
                    && event.event.workspace == scope.workspace
                    && event.event.session == scope.session
                    && event.event.task.as_ref() == Some(&scope.task)
                    && event.event.kind == vcp_protocol::event::EventKind::AttemptSubmitted
            });
            let mut facts = Facts {
                rows: state.events.iter().map(EventFact::from).collect(),
                fail: false,
            };
            let actual = send_intent(&mut facts, &id, &scope);
            assert_eq!(actual.is_ok(), expected);
            assert_eq!(
                actual.map_err(|error| error.to_string()),
                send_intent(&mut StateEventFacts::new(&state), &id, &scope)
                    .map_err(|error| error.to_string())
            );
            facts.fail = true;
            assert!(matches!(
                send_intent(&mut facts, &id, &scope),
                Err(Error::Unavailable("injected history failure"))
            ));
        }
    }
}
