// SPDX-License-Identifier: Apache-2.0
//! Reporting proof only. This never admits a send, resumes work or settles cost.
use super::*;
use serde::{Deserialize, Serialize};

const SCHEMA: &str = "provider-completed-execution/1";
const MARKER_LIMIT: u64 = 8192;

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
            return Ok(false);
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
        if current != *expected
            || current.state != CaptureState::Complete
            || current.length.get() > limit
        {
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
