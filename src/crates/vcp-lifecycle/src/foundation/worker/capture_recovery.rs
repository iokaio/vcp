// SPDX-License-Identifier: Apache-2.0
//! An acknowledged interrupted response can be inspected without authorizing
//! more execution. Its exact attempt still requires independent reconciliation.
use super::*;

const PREFIX: &str = "retained-codex-attempt:";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Recovery {
    Pending,
    Resolved,
}

pub(super) fn source(attempt: &AttemptId) -> String {
    format!("{PREFIX}{attempt}")
}

/// Unknown captures remain hard failures. Acknowledged pending liability permits
/// inspection only; independently settled/resolved accounting permits execution.
pub(super) fn response(state: &State, physical: &ArtifactDescriptor) -> Option<Recovery> {
    if physical.state != CaptureState::Aborted
        || physical.spec.schema != "responses-sse-observed-through-terminal/1"
        || physical.spec.channel != Channel::Response
        || !physical.spec.omissions.contains(&Omission::ExplicitAbort)
        || physical.spec.omissions.contains(&Omission::CaptureFailure)
    {
        return None;
    }
    let scope = &physical.spec.scope;
    let attached: ArtifactDescriptor = state
        .record(
            Collection::Artifact,
            physical.spec.id.as_str(),
            &scope.workspace,
        )
        .ok()?
        .decode()
        .ok()?;
    if attached != *physical {
        return None;
    }
    let id = AttemptId::parse(physical.spec.source.strip_prefix(PREFIX)?).ok()?;
    if source(&id) != physical.spec.source {
        return None;
    }
    let attempt: Attempt = state
        .record(Collection::Attempt, id.as_str(), &scope.workspace)
        .ok()?
        .decode()
        .ok()?;
    attempt.validate().ok()?;
    let reservation: Reservation = state
        .record(
            Collection::Reservation,
            attempt.reservation.as_str(),
            &scope.workspace,
        )
        .ok()?
        .decode()
        .ok()?;
    reservation.validate().ok()?;
    if attempt.id != id
        || attempt.scope != *scope
        || attempt.send_intent.is_none()
        || reservation.id != attempt.reservation
        || reservation.attempt != id
        || reservation.scope != *scope
        || reservation.root != attempt.root
        || reservation.phase != attempt.phase
        || reservation.charged != attempt.charged
        || reservation.amount != attempt.quote.amount
    {
        return None;
    }
    let ledger: Ledger = state
        .record(Collection::Ledger, attempt.root.as_str(), &scope.workspace)
        .ok()?
        .decode()
        .ok()?;
    ledger.validate().ok()?;
    if ledger.scope.workspace != scope.workspace
        || ledger.scope.session != scope.session
        || ledger.scope.task != attempt.root
        || ledger.currency != reservation.amount.currency
    {
        return None;
    }
    let (mut active, mut unresolved, mut settled) = (0u64, 0u64, 0u64);
    for record in state.records.values().filter(|record| {
        record.collection == Collection::Reservation && record.workspace == scope.workspace
    }) {
        let row: Reservation = record.decode().ok()?;
        row.validate().ok()?;
        if row.root != attempt.root {
            continue;
        }
        if row.scope.workspace != scope.workspace
            || row.scope.session != scope.session
            || row.amount.currency != ledger.currency
        {
            return None;
        }
        settled = settled.checked_add(row.charged.get())?;
        match row.phase {
            ReservationState::Created | ReservationState::Submitted => {
                active = active.checked_add(row.liability.get())?
            }
            ReservationState::ReconciliationPending => {
                unresolved = unresolved.checked_add(row.liability.get())?
            }
            _ => {}
        }
    }
    if (
        ledger.active.get(),
        ledger.unresolved.get(),
        ledger.settled.get(),
    ) != (active, unresolved, settled)
    {
        return None;
    }
    match attempt.phase {
        ReservationState::ReconciliationPending
            if attempt
                .uncertain
                .as_ref()
                .is_some_and(|reason| !reason.trim().is_empty()) =>
        {
            Some(Recovery::Pending)
        }
        ReservationState::Settled | ReservationState::ExplicitlyResolved => {
            // A terminal label alone is insufficient. Retain a concrete applied
            // final observation with its independently captured raw evidence.
            state
                .records
                .values()
                .filter(|record| {
                    record.collection == Collection::Settlement
                        && record.workspace == scope.workspace
                })
                .find_map(|record| {
                    let settlement: Settlement = record.decode().ok()?;
                    let observation = &settlement.observation;
                    if settlement.scope != *scope
                        || settlement.schema_version != 1
                        || settlement.normalization_version != 1
                        || settlement.attempt != id
                        || !settlement.applied
                        || settlement.total != attempt.charged
                        || observation.scope != *scope
                        || observation.attempt != id
                        || observation.id != settlement.id
                        || !observation.final_usage
                        || observation.amount.currency != ledger.currency
                        || attempt.provider_request.as_deref()
                            != Some(observation.provider_request.as_str())
                        || (attempt.phase == ReservationState::Settled
                            && attempt.uncertain.is_some())
                        || (attempt.phase == ReservationState::ExplicitlyResolved
                            && observation.correction.as_ref().is_none_or(|correction| {
                                correction.remaining_uncertainty.trim().is_empty()
                            }))
                    {
                        return None;
                    }
                    let raw: ArtifactDescriptor = state
                        .record(
                            Collection::Artifact,
                            observation.raw.as_str(),
                            &scope.workspace,
                        )
                        .ok()?
                        .decode()
                        .ok()?;
                    (raw.spec.scope == *scope && raw.state == CaptureState::Complete)
                        .then_some(Recovery::Resolved)
                })
        }
        _ => None,
    }
}

impl Context {
    pub(in crate::foundation) fn capture_admission_blocked(&self) -> bool {
        self.interrupted_capture
            || self.response_recovery.iter().any(|physical| {
                response(self.engine.store().state(), physical) != Some(Recovery::Resolved)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn put<T: serde::Serialize>(
        state: &mut State,
        collection: Collection,
        id: &str,
        scope: &Scope,
        value: &T,
    ) {
        let record = Record::typed(
            collection,
            id.to_owned(),
            scope.workspace.clone(),
            Revision::ZERO,
            value,
        )
        .unwrap();
        state.records.insert(record.key(), record);
    }

    fn fixture() -> (State, ArtifactDescriptor, Attempt, Reservation, Ledger) {
        let scope = Scope {
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
            task: TaskId::new(),
        };
        let currency: Currency = "USD".to_owned().try_into().unwrap();
        let money = Money {
            currency: currency.clone(),
            micros: Micros::new(100),
        };
        let attempt = Attempt {
            redaction: None,
            redacted_at_revision: None,
            schema_version: 1,
            id: AttemptId::new(),
            scope: scope.clone(),
            root: scope.task.clone(),
            reservation: ReservationId::new(),
            revision: Revision::ZERO,
            phase: ReservationState::ReconciliationPending,
            role: RequestRole::Main,
            agent: AgentId::new(),
            previous: None,
            request: ArtifactId::new(),
            request_digest: "a".repeat(64),
            admission_digest: "b".repeat(64),
            steering: SteeringRevision::ZERO,
            quote: CostQuote {
                normalization_version: 1,
                price: PriceSnapshot {
                    id: "fixture".into(),
                    provider: "fixture".into(),
                    model: "fixture".into(),
                    currency: currency.clone(),
                    capability: "fixture".into(),
                    valid_until: Timestamp::new(100),
                    rates: BTreeMap::new(),
                },
                bounds: Usage::default(),
                amount: money.clone(),
                method: "fixture".into(),
            },
            admitted_policy: PolicyRevision::ZERO,
            send_intent: Some(EventId::new()),
            observation_mode: None,
            usage_watermark: Units::ZERO,
            charged: Micros::ZERO,
            uncertain: Some("interrupted provider".into()),
            provider_request: None,
        };
        let reservation = Reservation {
            schema_version: 1,
            id: attempt.reservation.clone(),
            scope: scope.clone(),
            root: scope.task.clone(),
            attempt: attempt.id.clone(),
            revision: Revision::ZERO,
            phase: attempt.phase,
            amount: money,
            charged: Micros::ZERO,
            liability: Micros::new(100),
            protected_draw: Micros::ZERO,
            protected_returned: Micros::ZERO,
            day: 0,
            role: RequestRole::Main,
        };
        let ledger = Ledger {
            schema_version: 1,
            scope: scope.clone(),
            revision: Revision::ZERO,
            policy: PolicyRevision::ZERO,
            currency,
            cap: Micros::new(1000),
            protected: Micros::ZERO,
            settled: Micros::ZERO,
            active: Micros::ZERO,
            unresolved: Micros::new(100),
            allocations: BTreeMap::new(),
            daily: None,
            overrun: false,
        };
        let descriptor = ArtifactDescriptor {
            spec: ArtifactSpec {
                id: ArtifactId::new(),
                scope: scope.clone(),
                media_type: "application/octet-stream".into(),
                schema: "responses-sse-observed-through-terminal/1".into(),
                source: source(&attempt.id),
                channel: Channel::Response,
                retention: "full-work-history".into(),
                omissions: vec![Omission::ExplicitAbort],
            },
            state: CaptureState::Aborted,
            length: ByteCount::ZERO,
            sha256: "c".repeat(64),
            retained: vec![Range {
                start: ByteCount::ZERO,
                end: ByteCount::ZERO,
            }],
        };
        let mut state = State::default();
        put(
            &mut state,
            Collection::Attempt,
            attempt.id.as_str(),
            &scope,
            &attempt,
        );
        put(
            &mut state,
            Collection::Reservation,
            reservation.id.as_str(),
            &scope,
            &reservation,
        );
        put(
            &mut state,
            Collection::Ledger,
            scope.task.as_str(),
            &scope,
            &ledger,
        );
        put(
            &mut state,
            Collection::Artifact,
            descriptor.spec.id.as_str(),
            &scope,
            &descriptor,
        );
        (state, descriptor, attempt, reservation, ledger)
    }

    #[test]
    fn exact_acknowledged_response_retains_liability_and_rejects_unproved_links() {
        let (state, descriptor, attempt, reservation, ledger) = fixture();
        assert_eq!(response(&state, &descriptor), Some(Recovery::Pending));
        for case in [
            "orphan",
            "legacy",
            "failed",
            "pending",
            "scope",
            "attempt",
            "reservation",
            "ledger",
            "submitted",
            "uncertain",
        ] {
            let mut state = state.clone();
            let mut descriptor = descriptor.clone();
            let mut attempt = attempt.clone();
            let mut reservation = reservation.clone();
            let mut ledger = ledger.clone();
            match case {
                "orphan" => {
                    state
                        .records
                        .remove(&key(Collection::Artifact, descriptor.spec.id.as_str()));
                }
                "legacy" => descriptor.spec.source = "retained-codex".into(),
                "failed" => descriptor.spec.omissions.push(Omission::CaptureFailure),
                "pending" => descriptor.state = CaptureState::Pending,
                "scope" => attempt.scope.session = SessionId::new(),
                "attempt" => descriptor.spec.source = source(&AttemptId::new()),
                "reservation" => reservation.attempt = AttemptId::new(),
                "ledger" => ledger.unresolved = Micros::ZERO,
                "submitted" => {
                    attempt.phase = ReservationState::Submitted;
                    reservation.phase = attempt.phase;
                    ledger.active = ledger.unresolved;
                    ledger.unresolved = Micros::ZERO;
                }
                "uncertain" => attempt.uncertain = None,
                _ => unreachable!(),
            }
            let scope = descriptor.spec.scope.clone();
            if case != "orphan" {
                put(
                    &mut state,
                    Collection::Artifact,
                    descriptor.spec.id.as_str(),
                    &scope,
                    &descriptor,
                );
            }
            put(
                &mut state,
                Collection::Attempt,
                attempt.id.as_str(),
                &scope,
                &attempt,
            );
            put(
                &mut state,
                Collection::Reservation,
                reservation.id.as_str(),
                &scope,
                &reservation,
            );
            put(
                &mut state,
                Collection::Ledger,
                scope.task.as_str(),
                &scope,
                &ledger,
            );
            assert_eq!(response(&state, &descriptor), None, "{case}");
        }
    }

    #[test]
    fn later_resolution_requires_exact_applied_final_observation() {
        for phase in [
            ReservationState::Settled,
            ReservationState::ExplicitlyResolved,
        ] {
            let (mut state, descriptor, mut attempt, mut reservation, mut ledger) = fixture();
            let scope = descriptor.spec.scope.clone();
            attempt.phase = phase;
            attempt.provider_request = Some("independently-observed".into());
            attempt.charged = Micros::new(7);
            attempt.uncertain =
                (phase == ReservationState::ExplicitlyResolved).then(|| "explicit residual".into());
            reservation.phase = phase;
            reservation.charged = attempt.charged;
            reservation.liability = Micros::ZERO;
            ledger.unresolved = Micros::ZERO;
            ledger.settled = attempt.charged;
            put(
                &mut state,
                Collection::Attempt,
                attempt.id.as_str(),
                &scope,
                &attempt,
            );
            put(
                &mut state,
                Collection::Reservation,
                reservation.id.as_str(),
                &scope,
                &reservation,
            );
            put(
                &mut state,
                Collection::Ledger,
                scope.task.as_str(),
                &scope,
                &ledger,
            );
            assert_eq!(response(&state, &descriptor), None);
            let mut raw = descriptor.clone();
            raw.spec.id = ArtifactId::new();
            raw.state = CaptureState::Complete;
            put(
                &mut state,
                Collection::Artifact,
                raw.spec.id.as_str(),
                &scope,
                &raw,
            );
            let id = ObservationId::new();
            let observation = UsageObservation {
                id: id.clone(),
                scope: scope.clone(),
                attempt: attempt.id.clone(),
                provider_request: "independently-observed".into(),
                mode: UsageMode::Cumulative {
                    version: Units::new(1),
                },
                amount: Money {
                    currency: ledger.currency.clone(),
                    micros: attempt.charged,
                },
                final_usage: true,
                raw: raw.spec.id.clone(),
                correction: (phase == ReservationState::ExplicitlyResolved).then(|| Resolution {
                    actor: ActorId::new(),
                    policy: PolicyRevision::ZERO,
                    reason: "independent fixture proof".into(),
                    remaining_uncertainty: "explicit residual".into(),
                }),
            };
            let mut settlement = Settlement {
                schema_version: 1,
                redaction: None,
                observation_digest: None,
                id,
                scope: scope.clone(),
                attempt: attempt.id.clone(),
                observation,
                applied: true,
                direction: AdjustmentDirection::Debit,
                adjustment: attempt.charged,
                total: attempt.charged,
                normalization_version: 1,
            };
            put(
                &mut state,
                Collection::Settlement,
                settlement.id.as_str(),
                &scope,
                &settlement,
            );
            assert_eq!(response(&state, &descriptor), Some(Recovery::Resolved));
            settlement.applied = false;
            put(
                &mut state,
                Collection::Settlement,
                settlement.id.as_str(),
                &scope,
                &settlement,
            );
            assert_eq!(response(&state, &descriptor), None);
        }
    }
}
