// SPDX-License-Identifier: Apache-2.0
//! Retained retry disposition and completion proof. This never settles cost or
//! grants authority; normal request, retry, turn and effect fences still apply.
use super::*;
use serde::{Deserialize, Serialize};

const SCHEMA: &str = "provider-completed-execution/1";
const MARKER_LIMIT: u64 = 8192;
const EMPTY_RETRY_SCHEMA: &str = "provider-empty-response-retry/1";
const RETRY_DISPOSITION_SCHEMA: &str = "provider-retry-disposition/1";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetryDispositionProof {
    attempt: AttemptId,
    scope: Scope,
    turn: TurnId,
    send_intent: EventId,
    request_digest: String,
    admission_digest: String,
    raw: ArtifactDescriptor,
    // None means an exactly empty aborted response, never an unknown body.
    rejection: Option<vcp_models::retry::HttpRejection>,
}

fn same_retry_lineage(current: &Attempt, next: &Attempt) -> bool {
    next.previous.as_ref() == Some(&current.id)
        && next.id != current.id
        && next.scope == current.scope
        && next.root == current.root
        && next.agent == current.agent
        && next.role == current.role
        && next.steering == current.steering
        && next.reservation != current.reservation
        && next.request != current.request
        && next.send_intent.is_some()
        && next.send_intent != current.send_intent
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyRetryProof {
    attempt: AttemptId,
    scope: Scope,
    send_intent: EventId,
    request_digest: String,
    admission_digest: String,
    raw: ArtifactDescriptor,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proof {
    attempt: AttemptId,
    scope: Scope,
    send_intent: EventId,
    request_digest: String,
    admission_digest: String,
    response_id: String,
    raw: ArtifactDescriptor,
    normalized: ArtifactDescriptor,
}

impl Context {
    fn attempt_response(&self, attempt: &Attempt) -> Result<Option<ArtifactDescriptor>> {
        let mut selected = None;
        for row in self
            .engine
            .store()
            .current()
            .records
            .values()
            .filter(|row| {
                row.collection == Collection::Artifact && row.workspace == attempt.scope.workspace
            })
        {
            let raw: ArtifactDescriptor = row.decode()?;
            if raw.spec.scope == attempt.scope
                && raw.spec.channel == Channel::Response
                && raw.spec.schema == "responses-sse-observed-through-terminal/1"
                && raw.spec.source == capture_recovery::source(&attempt.id)
                && selected.replace(raw).is_some()
            {
                return Ok(None);
            }
        }
        Ok(selected)
    }

    // Called only after a real failure has been retained and bounded retry
    // scheduled. The normalized provider boundary excludes model/partial bodies.
    pub(super) fn record_retry_disposition(
        &mut self,
        id: &AttemptId,
        status: Option<u16>,
    ) -> Result<()> {
        let attempt =
            vcp_budget::attempt(self.engine.store().current(), id, &self.config.workspace)?;
        let Some(raw) = self.attempt_response(&attempt)? else {
            return Ok(());
        };
        if raw.state != CaptureState::Aborted
            || raw.length.get() > vcp_models::retry::HTTP_REJECTION_LIMIT
        {
            return Ok(());
        }
        let bytes = self.financial_capture_bytes(
            &raw,
            vcp_models::retry::HTTP_REJECTION_LIMIT,
            CaptureState::Aborted,
        )?;
        let rejection = match status {
            Some(status) => match vcp_models::retry::normalize_http_rejection(&bytes, status) {
                Some(rejection) => Some(rejection),
                None => return Ok(()),
            },
            None if bytes.is_empty() => None,
            None => return Ok(()),
        };
        let Some(turn) = self
            .runtime
            .block_on(vcp_engine::public::current_public_turn_store(
                self.engine.store(),
                &attempt.scope,
            ))?
        else {
            return Ok(());
        };
        if turn.state != TurnState::RequestingModel || turn.steering != attempt.steering {
            return Ok(());
        }
        let proof = RetryDispositionProof {
            attempt: attempt.id,
            scope: attempt.scope.clone(),
            turn: turn.id,
            send_intent: attempt.send_intent.ok_or("HTTP retry lacks send intent")?,
            request_digest: attempt.request_digest,
            admission_digest: attempt.admission_digest,
            raw,
            rejection,
        };
        let bytes = canonical_bytes(&proof)?;
        if bytes.len() as u64 > MARKER_LIMIT {
            return Err("HTTP retry proof exceeds bound".into());
        }
        self.capture(
            &attempt.scope,
            Channel::Evidence,
            &bytes,
            RETRY_DISPOSITION_SCHEMA,
        )?;
        Ok(())
    }

    fn retry_disposition_proven(
        &self,
        current: &Attempt,
        turn: &TurnId,
        allow_empty: bool,
    ) -> Result<bool> {
        let mut selected = None;
        for row in self
            .engine
            .store()
            .current()
            .records
            .values()
            .filter(|row| {
                row.collection == Collection::Artifact && row.workspace == current.scope.workspace
            })
        {
            let descriptor: ArtifactDescriptor = row.decode()?;
            if descriptor.spec.schema != RETRY_DISPOSITION_SCHEMA
                || descriptor.spec.scope != current.scope
            {
                continue;
            }
            let bytes = self.financial_proof_bytes(&descriptor, MARKER_LIMIT)?;
            let proof: RetryDispositionProof = serde_json::from_slice(&bytes)?;
            if canonical_bytes(&proof)? != bytes {
                return Err("HTTP retry proof is not canonical".into());
            }
            if proof.attempt == current.id && selected.replace(proof).is_some() {
                return Ok(false);
            }
        }
        let Some(proof) = selected else {
            return Ok(false);
        };
        if proof.scope != current.scope
            || &proof.turn != turn
            || Some(&proof.send_intent) != current.send_intent.as_ref()
            || proof.request_digest != current.request_digest
            || proof.admission_digest != current.admission_digest
            || self.attempt_response(current)?.as_ref() != Some(&proof.raw)
        {
            return Ok(false);
        }
        let bytes = self.financial_capture_bytes(
            &proof.raw,
            vcp_models::retry::HTTP_REJECTION_LIMIT,
            CaptureState::Aborted,
        )?;
        Ok(match proof.rejection {
            Some(rejection) => {
                vcp_models::retry::normalize_http_rejection(&bytes, rejection.status).as_ref()
                    == Some(&rejection)
            }
            None => allow_empty && bytes.is_empty() && self.legacy_empty_retry_proven(current)?,
        })
    }

    fn retried_provider_disposition(&self, current: &Attempt) -> Result<bool> {
        let task: Task = self
            .engine
            .store()
            .current()
            .record(
                Collection::Task,
                current.scope.task.as_str(),
                &current.scope.workspace,
            )?
            .decode()?;
        let Some(turn) = self
            .runtime
            .block_on(vcp_engine::public::current_public_turn_store(
                self.engine.store(),
                &current.scope,
            ))?
        else {
            return Ok(false);
        };
        if task.scope != current.scope
            || task.state != TaskState::Completed
            || task.steering != current.steering
            || turn.state != TurnState::Completed
            || turn.steering != current.steering
        {
            return Ok(false);
        }
        // Empty-response completion retains its original, independently checked
        // marker path; a second marker must not bypass its ambiguity checks.
        self.recovered_retry_chain(current, &turn.id, false)
    }

    // Only fully recovered execution may stop counting toward the availability
    // uncertainty guard. Outstanding charges remain untouched and finite
    // ledgers retain their existing containment policy.
    pub(super) fn recovered_retry_while_running(&self, current: &Attempt) -> Result<bool> {
        if current.root != self.config.root_task
            || current.phase != ReservationState::ReconciliationPending
            || current.send_intent.is_none()
            || !vcp_budget::ledger(self.engine.store().current(), &current.scope)?
                .cap
                .is_unbounded()
        {
            return Ok(false);
        }
        let task: Task = self
            .engine
            .store()
            .current()
            .record(
                Collection::Task,
                current.scope.task.as_str(),
                &current.scope.workspace,
            )?
            .decode()?;
        let Some(turn) = self
            .runtime
            .block_on(vcp_engine::public::current_public_turn_store(
                self.engine.store(),
                &current.scope,
            ))?
        else {
            return Ok(false);
        };
        if task.scope != current.scope
            || task.state != TaskState::Running
            || task.steering != current.steering
            || turn.steering != current.steering
            || !matches!(
                turn.state,
                TurnState::AssemblingContext
                    | TurnState::ReservingBudget
                    | TurnState::RequestingModel
                    | TurnState::ProcessingResponse
                    | TurnState::ExecutingTools
                    | TurnState::Verifying
            )
        {
            return Ok(false);
        }
        self.recovered_retry_chain(current, &turn.id, true)
    }

    fn recovered_retry_chain(
        &self,
        current: &Attempt,
        turn: &TurnId,
        allow_initial_empty: bool,
    ) -> Result<bool> {
        let attempts: Vec<Attempt> = self
            .engine
            .store()
            .current()
            .records
            .values()
            .filter(|row| {
                row.collection == Collection::Attempt && row.workspace == current.scope.workspace
            })
            .map(Record::decode)
            .collect::<std::result::Result<_, _>>()?;
        let mut visited = std::collections::BTreeSet::new();
        let mut prior = current;
        loop {
            if !visited.insert(prior.id.clone())
                || !self.retry_disposition_proven(
                    prior,
                    turn,
                    allow_initial_empty || prior.id != current.id,
                )?
            {
                return Ok(false);
            }
            // Count every successor, including conflicting scope/agent links.
            let successors: Vec<_> = attempts
                .iter()
                .filter(|next| next.previous.as_ref() == Some(&prior.id))
                .collect();
            let [next] = successors.as_slice() else {
                return Ok(false);
            };
            if !same_retry_lineage(prior, next) {
                return Ok(false);
            }
            let Some(raw) = self.attempt_response(next)? else {
                return Ok(false);
            };
            if raw.state == CaptureState::Complete {
                // A settled rejection alone is insufficient: the successor must
                // have actually returned a validated completed model response.
                if !matches!(
                    next.phase,
                    ReservationState::Settled | ReservationState::ReconciliationPending
                ) {
                    return Ok(false);
                }
                let bytes = self.financial_proof_bytes(&raw, self.config.artifact_limit.get())?;
                let Some(identity) = vcp_models::stream::retained_completed_terminal(&bytes)?
                else {
                    return Ok(false);
                };
                return if next.phase == ReservationState::Settled {
                    Ok(next.provider_request.as_ref() == Some(&identity.request_id))
                } else {
                    self.completed_financial_uncertainty(next)
                };
            }
            if next.phase != ReservationState::ReconciliationPending {
                return Ok(false);
            }
            prior = next;
        }
    }

    // A failed response with no bytes cannot have supplied an executable tool
    // call. Retain that evidence separately from its still-unknown charge. This
    // marker is written only by bounded retry scheduling, never by permit drop.
    pub(super) fn record_empty_response_retry(&mut self, id: &AttemptId) -> Result<()> {
        let attempt =
            vcp_budget::attempt(self.engine.store().current(), id, &self.config.workspace)?;
        let captures: Vec<ArtifactDescriptor> = self
            .engine
            .store()
            .current()
            .records
            .values()
            .filter(|row| {
                row.collection == Collection::Artifact && row.workspace == attempt.scope.workspace
            })
            .map(Record::decode)
            .collect::<std::result::Result<_, _>>()?;
        let captures: Vec<_> = captures
            .into_iter()
            .filter(|raw| {
                raw.spec.scope == attempt.scope
                    && raw.spec.channel == Channel::Response
                    && raw.spec.schema == "responses-sse-observed-through-terminal/1"
                    && raw.spec.source == capture_recovery::source(id)
            })
            .collect();
        let [raw] = captures.as_slice() else {
            return Ok(());
        };
        if raw.state != CaptureState::Aborted
            || raw.length.get() != 0
            || raw.sha256 != vcp_protocol::digest_bytes(&[])
        {
            return Ok(());
        }
        let proof = EmptyRetryProof {
            attempt: attempt.id,
            scope: attempt.scope.clone(),
            send_intent: attempt.send_intent.ok_or("empty retry lacks send intent")?,
            request_digest: attempt.request_digest,
            admission_digest: attempt.admission_digest,
            raw: raw.clone(),
        };
        let bytes = canonical_bytes(&proof)?;
        if bytes.len() as u64 > MARKER_LIMIT {
            return Err("empty retry proof exceeds bound".into());
        }
        self.capture(
            &attempt.scope,
            Channel::Evidence,
            &bytes,
            EMPTY_RETRY_SCHEMA,
        )?;
        self.record_retry_disposition(id, None)
    }

    fn retried_empty_response(&self, current: &Attempt) -> Result<bool> {
        // Do not present an exhausted, cancelled or merely scheduled retry as
        // recovered execution. Completion still requires ordinary verification.
        let task: Task = self
            .engine
            .store()
            .current()
            .record(
                Collection::Task,
                current.scope.task.as_str(),
                &current.scope.workspace,
            )?
            .decode()?;
        if task.scope != current.scope || task.state != TaskState::Completed {
            return Ok(false);
        }
        self.legacy_empty_retry_proven(current)
    }

    fn legacy_empty_retry_proven(&self, current: &Attempt) -> Result<bool> {
        let mut selected = None;
        let mut successors = 0;
        for row in self
            .engine
            .store()
            .current()
            .records
            .values()
            .filter(|row| row.workspace == current.scope.workspace)
        {
            if row.collection == Collection::Attempt {
                let next: Attempt = row.decode()?;
                if next.previous.as_ref() == Some(&current.id)
                    && next.scope == current.scope
                    && next.root == current.root
                    && next.send_intent.is_some()
                {
                    successors += 1;
                }
            } else if row.collection == Collection::Artifact {
                let descriptor: ArtifactDescriptor = row.decode()?;
                if descriptor.spec.schema != EMPTY_RETRY_SCHEMA
                    || descriptor.spec.scope != current.scope
                {
                    continue;
                }
                let bytes = self.financial_proof_bytes(&descriptor, MARKER_LIMIT)?;
                let proof: EmptyRetryProof = serde_json::from_slice(&bytes)?;
                if canonical_bytes(&proof)? != bytes {
                    return Err("empty retry proof is not canonical".into());
                }
                if proof.attempt == current.id && selected.replace(proof).is_some() {
                    return Ok(false);
                }
            }
        }
        let Some(proof) = selected else {
            return Ok(false);
        };
        if successors != 1
            || proof.scope != current.scope
            || Some(&proof.send_intent) != current.send_intent.as_ref()
            || proof.request_digest != current.request_digest
            || proof.admission_digest != current.admission_digest
            || proof.raw.spec.scope != current.scope
            || proof.raw.spec.source != capture_recovery::source(&current.id)
            || proof.raw.spec.schema != "responses-sse-observed-through-terminal/1"
            || proof.raw.spec.channel != Channel::Response
        {
            return Ok(false);
        }
        let raw: ArtifactDescriptor = self
            .engine
            .store()
            .current()
            .record(
                Collection::Artifact,
                proof.raw.spec.id.as_str(),
                &current.scope.workspace,
            )?
            .decode()?;
        if raw != proof.raw
            || raw.state != CaptureState::Aborted
            || raw.length.get() != 0
            || raw.sha256 != vcp_protocol::digest_bytes(&[])
        {
            return Ok(false);
        }
        let mut bytes = Vec::new();
        self.runtime
            .block_on(vcp_audit::history::History::read_artifact(
                self.engine.store(),
                &self.history_access(),
                &raw.spec.id,
                &mut bytes,
            ))?;
        Ok(bytes.is_empty())
    }
    pub(super) fn record_completed_financial_uncertainty(
        &mut self,
        attempt: &Attempt,
        response_id: &str,
        raw: ArtifactDescriptor,
        normalized: ArtifactDescriptor,
    ) -> Result<()> {
        let proof = Proof {
            attempt: attempt.id.clone(),
            scope: attempt.scope.clone(),
            send_intent: attempt
                .send_intent
                .clone()
                .ok_or("completed response lacks send intent")?,
            request_digest: attempt.request_digest.clone(),
            admission_digest: attempt.admission_digest.clone(),
            response_id: response_id.into(),
            raw,
            normalized,
        };
        let bytes = canonical_bytes(&proof)?;
        if bytes.len() as u64 > MARKER_LIMIT {
            return Err("completed response proof exceeds bound".into());
        }
        self.capture(&attempt.scope, Channel::Evidence, &bytes, SCHEMA)?;
        Ok(())
    }

    pub(in crate::foundation) fn completed_financial_uncertainty(
        &self,
        expected: &Attempt,
    ) -> Result<bool> {
        let current = vcp_budget::attempt(
            self.engine.store().current(),
            &expected.id,
            &self.config.workspace,
        )?;
        if current != *expected
            || current.root != self.config.root_task
            || current.phase != ReservationState::ReconciliationPending
            || current.send_intent.is_none()
            || !vcp_budget::ledger(self.engine.store().current(), &current.scope)?
                .cap
                .is_unbounded()
        {
            return Ok(false);
        }
        let mut selected = None;
        for row in self
            .engine
            .store()
            .current()
            .records
            .values()
            .filter(|row| {
                row.collection == Collection::Artifact && row.workspace == current.scope.workspace
            })
        {
            let descriptor: ArtifactDescriptor = row.decode()?;
            if descriptor.spec.schema != SCHEMA || descriptor.spec.scope != current.scope {
                continue;
            }
            let bytes = self.financial_proof_bytes(&descriptor, MARKER_LIMIT)?;
            let proof: Proof = serde_json::from_slice(&bytes)?;
            if canonical_bytes(&proof)? != bytes {
                return Err("completed response proof is not canonical".into());
            }
            if proof.attempt != current.id {
                continue;
            }
            if selected.replace(proof).is_some() {
                return Ok(false);
            }
        }
        let Some(proof) = selected else {
            return Ok(self.retried_empty_response(&current)?
                || self.retried_provider_disposition(&current)?);
        };
        if proof.scope != current.scope
            || Some(&proof.send_intent) != current.send_intent.as_ref()
            || proof.request_digest != current.request_digest
            || proof.admission_digest != current.admission_digest
            || proof.raw.spec.scope != current.scope
            || proof.normalized.spec.scope != current.scope
            || proof.raw.spec.schema != "responses-sse-observed-through-terminal/1"
            || proof.raw.spec.channel != Channel::Response
            || proof.raw.spec.source != capture_recovery::source(&current.id)
            || proof.normalized.spec.schema != "openrouter-normalized-response/1"
            || proof.normalized.spec.channel != Channel::Evidence
        {
            return Err("completed response proof identity differs".into());
        }
        let raw = self.financial_proof_bytes(&proof.raw, self.config.artifact_limit.get())?;
        let bytes =
            self.financial_proof_bytes(&proof.normalized, self.config.artifact_limit.get())?;
        let normalized: vcp_models::stream::ResultBody = serde_json::from_slice(&bytes)?;
        if canonical_bytes(&normalized)? != bytes
            || normalized.response_id != proof.response_id
            || normalized.status != vcp_models::stream::Status::Completed
            || normalized.terminal_diagnostic.is_some()
            || normalized
                .usage
                .as_ref()
                .and_then(|usage| usage.cost.as_ref())
                .is_some()
            || normalized
                .served_model
                .as_ref()
                .is_some_and(|model| model != &current.quote.price.model)
        {
            return Err("completed response proof normalized result differs".into());
        }
        Ok(
            vcp_models::stream::retained_completed_terminal(&raw)?.is_some_and(|identity| {
                identity.request_id == proof.response_id
                    && identity.frame_sha256 == normalized.raw_terminal_sha256
            }),
        )
    }

    fn financial_proof_bytes(&self, expected: &ArtifactDescriptor, limit: u64) -> Result<Vec<u8>> {
        self.financial_capture_bytes(expected, limit, CaptureState::Complete)
    }

    fn financial_capture_bytes(
        &self,
        expected: &ArtifactDescriptor,
        limit: u64,
        state: CaptureState,
    ) -> Result<Vec<u8>> {
        let current: ArtifactDescriptor = self
            .engine
            .store()
            .current()
            .record(
                Collection::Artifact,
                expected.spec.id.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        if current != *expected || current.state != state || current.length.get() > limit {
            return Err("completed response proof descriptor differs or is incomplete".into());
        }
        let mut bytes = Vec::new();
        self.runtime
            .block_on(vcp_audit::history::History::read_artifact(
                self.engine.store(),
                &self.history_access(),
                &current.spec.id,
                &mut bytes,
            ))?;
        if bytes.len() as u64 != current.length.get()
            || vcp_protocol::digest_bytes(&bytes) != current.sha256
        {
            return Err("completed response proof content differs".into());
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod http_retry_lineage_tests {
    use super::*;

    fn attempt() -> Attempt {
        let scope = Scope {
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
            task: TaskId::new(),
        };
        let currency: Currency = "USD".to_owned().try_into().unwrap();
        Attempt {
            redaction: None,
            redacted_at_revision: None,
            schema_version: 1,
            id: AttemptId::new(),
            scope: scope.clone(),
            root: scope.task,
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
                    rates: std::collections::BTreeMap::new(),
                },
                bounds: Usage::default(),
                amount: Money {
                    currency,
                    micros: Micros::new(100),
                }
                .into(),
                method: "fixture".into(),
            },
            admitted_policy: PolicyRevision::ZERO,
            send_intent: Some(EventId::new()),
            observation_mode: None,
            usage_watermark: Units::ZERO,
            charged: Micros::ZERO,
            uncertain: Some("unknown billing".into()),
            provider_request: None,
        }
    }

    #[test]
    fn http_retry_requires_fresh_send_and_exact_scope_owner_and_steering_lineage() {
        let prior = attempt();
        let mut successor = prior.clone();
        successor.id = AttemptId::new();
        successor.previous = Some(prior.id.clone());
        successor.reservation = ReservationId::new();
        successor.request = ArtifactId::new();
        successor.send_intent = Some(EventId::new());
        assert!(same_retry_lineage(&prior, &successor));
        let changes: Vec<Box<dyn Fn(&mut Attempt)>> = vec![
            Box::new(|next| next.previous = None),
            Box::new(|next| next.previous = Some(AttemptId::new())),
            Box::new(|next| next.scope.workspace = WorkspaceId::new()),
            Box::new(|next| next.scope.session = SessionId::new()),
            Box::new(|next| next.scope.task = TaskId::new()),
            Box::new(|next| next.root = TaskId::new()),
            Box::new(|next| next.agent = AgentId::new()),
            Box::new(|next| next.role = RequestRole::Helper),
            Box::new(|next| next.steering = next.steering.next().unwrap()),
            Box::new(|next| next.send_intent = None),
        ];
        for change in changes {
            let mut invalid = successor.clone();
            change(&mut invalid);
            assert!(!same_retry_lineage(&prior, &invalid));
        }
        let mut invalid = successor.clone();
        invalid.id = prior.id.clone();
        assert!(!same_retry_lineage(&prior, &invalid));
        invalid = successor.clone();
        invalid.reservation = prior.reservation.clone();
        assert!(!same_retry_lineage(&prior, &invalid));
        invalid = successor.clone();
        invalid.request = prior.request.clone();
        assert!(!same_retry_lineage(&prior, &invalid));
        invalid = successor;
        invalid.send_intent = prior.send_intent.clone();
        assert!(!same_retry_lineage(&prior, &invalid));
    }
}
