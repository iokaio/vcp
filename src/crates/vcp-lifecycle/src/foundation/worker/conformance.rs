// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::foundation::conformance::{self, Probe};
use vcp_models::{catalog::CandidateMetadata, stream::ResultBody};
impl Context {
    pub(crate) fn admit_decision_cohort(
        &mut self,
        binding: &ThreadBinding,
        candidate: &conformance::cohort::Candidate,
    ) -> Result<(AttemptId, serde_json::Value)> {
        self.can_start(binding)?;
        let metadata = candidate.metadata()?;
        if self.provider_required
            || self.routing.is_some()
            || binding.role != RequestRole::Main
            || binding.scope.task != self.config.root_task
            || self.config.max_transport_retries != 0
            || self.config.price != metadata.price
            || self.config.input_ceiling != metadata.max_input
            || self.config.output_ceiling != candidate.output()
            || candidate.output() > metadata.max_output
            || self
                .engine
                .store()
                .state()
                .records
                .values()
                .any(|r| r.collection == Collection::Attempt)
        {
            return Err("decision cohort requires exact fresh isolated owner".into());
        }
        self.capture(
            &binding.scope,
            Channel::Evidence,
            &candidate.raw,
            "unqualified-decision-cohort-catalog/1",
        )?;
        self.capture(
            &binding.scope,
            Channel::Evidence,
            conformance::cohort::WORKLOAD,
            "frozen-decision-cohort/1",
        )?;
        let body = conformance::cohort::body(candidate, &metadata)?;
        let (attempt, body, _, _) = self.admit_inner(binding, body, true)?;
        Ok((attempt, body))
    }
    pub(crate) fn admit_native_conformance(
        &mut self,
        binding: &ThreadBinding,
        raw: &[u8],
        model: &str,
        provider: &str,
        request_cap: &str,
        expires: Timestamp,
    ) -> Result<(AttemptId, serde_json::Value)> {
        self.can_start(binding)?;
        let candidate = conformance::native::candidate(raw, model, provider, request_cap, expires)?;
        if self.provider_required
            || self.routing.is_some()
            || binding.role != RequestRole::Main
            || binding.scope.task != self.config.root_task
            || self.config.max_transport_retries != 0
            || self.config.price != candidate.price
            || self.config.input_ceiling != candidate.max_input
            || self.config.output_ceiling != Units::new(1)
            || self
                .engine
                .store()
                .state()
                .records
                .values()
                .any(|r| r.collection == Collection::Attempt)
        {
            return Err("native probe requires exact isolated fresh candidate owner".into());
        }
        self.capture(
            &binding.scope,
            Channel::Evidence,
            raw,
            "unqualified-native-catalog/1",
        )?;
        let body = conformance::native::body(&candidate)?;
        let (attempt, body, _, _) = self.admit_inner(binding, body, true)?;
        Ok((attempt, body))
    }
    pub(crate) fn complete_native_conformance(
        &mut self,
        binding: &ThreadBinding,
        attempt: &AttemptId,
        raw: &serde_json::Value,
        cost: Option<Micros>,
    ) -> Result<()> {
        let mut writer = self
            .streams
            .remove(attempt)
            .ok_or("native probe capture missing")?;
        let descriptor = writer.finalize()?;
        drop(writer);
        self.command(
            Command::AttachArtifact {
                descriptor: descriptor.clone(),
            },
            Some(binding.scope.task.clone()),
            Revision::ZERO,
        )?;
        let cost = cost.ok_or("native probe has no observed cost; liability retained")?;
        let request = raw["id"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 256)
            .ok_or("native response identity absent; liability retained")?;
        let actor = self.actor();
        self.runtime.block_on(vcp_budget::observe(
            self.engine.store_mut(),
            UsageObservation {
                id: ObservationId::new(),
                scope: binding.scope.clone(),
                attempt: attempt.clone(),
                provider_request: request.into(),
                mode: UsageMode::Cumulative {
                    version: Units::new(1),
                },
                amount: Money {
                    currency: "USD".to_owned().try_into()?,
                    micros: cost,
                },
                final_usage: true,
                raw: descriptor.spec.id,
                correction: None,
            },
            &actor,
        ))?;
        Ok(())
    }
    pub(crate) fn admit_conformance(
        &mut self,
        binding: &ThreadBinding,
        candidate: &CandidateMetadata,
        probe: &Probe,
    ) -> Result<(AttemptId, serde_json::Value)> {
        self.can_start(binding)?;
        if self.provider_required
            || self.routing.is_some()
            || binding.role != RequestRole::Main
            || binding.scope.task != self.config.root_task
            || self.config.max_transport_retries != 0
            || self.config.price != candidate.price
            || self.config.input_ceiling != candidate.max_input
            || self.config.output_ceiling > candidate.max_output
            || self.config.output_ceiling > Units::new(2048)
        {
            return Err("qualification probe differs from the isolated candidate owner".into());
        }
        let attempts: Vec<Attempt> = self
            .engine
            .store()
            .state()
            .records
            .values()
            .filter(|r| r.collection == Collection::Attempt)
            .map(Record::decode)
            .collect::<std::result::Result<_, _>>()?;
        if attempts.len() >= 2
            || attempts
                .iter()
                .any(|a| a.phase != ReservationState::Settled)
            || (attempts.is_empty() != matches!(probe, Probe::ToolCall))
        {
            return Err("conformance probes are exactly one fresh call and one settled continuation; no replay".into());
        }
        if let Probe::Continuation(call) = probe {
            let captures: Vec<ArtifactDescriptor> = self
                .engine
                .store()
                .state()
                .records
                .values()
                .filter(|r| r.collection == Collection::Artifact)
                .map(Record::decode::<ArtifactDescriptor>)
                .collect::<std::result::Result<Vec<_>, _>>()?
                .into_iter()
                .filter(|a| {
                    a.spec.scope == binding.scope
                        && a.spec.schema == "conformance-normalized-response/1"
                        && a.state == CaptureState::Complete
                })
                .collect();
            if captures.len() != 1 || captures[0].length > ByteCount::new(1024 * 1024) {
                return Err("conformance predecessor evidence unavailable".into());
            }
            let mut bytes = Vec::new();
            self.engine.store().spool().read(&captures[0], &mut bytes)?;
            let first: ResultBody = serde_json::from_slice(&bytes)?;
            if first.status != vcp_models::stream::Status::Completed
                || first.served_model.as_deref() != Some(candidate.price.model.as_str())
                || attempts[0].provider_request.as_deref() != Some(first.response_id.as_str())
                || first.calls != vec![call.clone()]
            {
                return Err(
                    "conformance continuation differs from the accounted prior call".into(),
                );
            }
        }
        let body = conformance::body(candidate, self.config.output_ceiling, probe)?;
        self.capture(
            &binding.scope,
            Channel::Evidence,
            &canonical_bytes(candidate)?,
            "unqualified-provider-candidate/1",
        )?;
        let (attempt, body, _, _) = self.admit(binding, body)?;
        Ok((attempt, body))
    }
    pub(crate) fn complete_conformance(
        &mut self,
        binding: &ThreadBinding,
        attempt: &AttemptId,
        normalized: &ResultBody,
    ) -> Result<()> {
        let mut writer = self
            .streams
            .remove(attempt)
            .ok_or("conformance capture missing")?;
        let descriptor = writer.finalize()?;
        drop(writer);
        self.command(
            Command::AttachArtifact {
                descriptor: descriptor.clone(),
            },
            Some(binding.scope.task.clone()),
            Revision::ZERO,
        )?;
        self.capture(
            &binding.scope,
            Channel::Evidence,
            &canonical_bytes(normalized)?,
            "conformance-normalized-response/1",
        )?;
        let amount = normalized
            .usage
            .as_ref()
            .and_then(|u| u.cost.clone())
            .ok_or("conformance response lacks actual cost; liability retained")?;
        let actor = self.actor();
        self.runtime.block_on(vcp_budget::observe(
            self.engine.store_mut(),
            UsageObservation {
                id: ObservationId::new(),
                scope: binding.scope.clone(),
                attempt: attempt.clone(),
                provider_request: normalized.response_id.clone(),
                mode: UsageMode::Cumulative {
                    version: Units::new(1),
                },
                amount,
                final_usage: true,
                raw: descriptor.spec.id,
                correction: None,
            },
            &actor,
        ))?;
        Ok(())
    }
}
