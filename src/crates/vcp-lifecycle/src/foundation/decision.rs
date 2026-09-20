// SPDX-License-Identifier: Apache-2.0
//! Optional canonical shadow comparisons. Advice never changes the main route.
pub(crate) mod admission;
pub mod credentials;
pub(crate) mod transport;
pub use super::worker::decision::ConformanceEvidence;
use super::*;
pub use admission::{EvidencePin, QualificationRecord};
pub use credentials::CredentialMaterial;
use serde::{Deserialize, Serialize};
#[cfg(feature = "qualification")]
use std::sync::Arc;
#[cfg(feature = "qualification")]
use vcp_domain::{accounting::PriceSnapshot, revision::Units};
pub use vcp_models::decision::Mode;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationRef {
    pub artifact: ArtifactId,
    pub digest: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub mode: Mode,
    pub qualification: Option<QualificationRef>,
}
impl Configuration {
    pub fn validate(&self) -> Result<(), String> {
        if self.mode == Mode::Advisory {
            return Err(
                "only disabled, deterministic or shadow decision modes are supported".into(),
            );
        }
        if self
            .qualification
            .as_ref()
            .is_some_and(|r| !vcp_domain::accounting::valid_hash(&r.digest))
        {
            return Err("invalid decision qualification reference".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct ShadowOutcome {
    pub main_attempt: Option<AttemptId>,
    pub evaluator_attempt: Option<AttemptId>,
    pub outcome: serde_json::Value,
    pub artifacts: Vec<ArtifactId>,
}
impl ShadowOutcome {
    pub(crate) fn baseline(main: Option<AttemptId>, reason: &str) -> Self {
        Self {
            main_attempt: main,
            evaluator_attempt: None,
            outcome: serde_json::json!({"outcome":"baseline","reason":reason,"shadow":true,"baseline_changed":false}),
            artifacts: vec![],
        }
    }
}
#[cfg(feature = "qualification")]
pub struct FixtureConfiguration {
    pub evaluator: vcp_models::decision::QualifiedEvaluator,
    pub price: PriceSnapshot,
    pub input_ceiling: Units,
    pub output_ceiling: Units,
    pub endpoint: String,
    pub root_certificate: Vec<u8>,
    pub credential: CredentialMaterial,
}
impl CanonicalHost {
    pub fn revoke_decision_credential(&self) -> Result<(), String> {
        self.worker
            .run(|context| context.revoke_decision_credential())
    }
    #[cfg(feature = "qualification")]
    pub fn qualification_block_decision_after_tls(
        &self,
        arrived: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    ) -> Result<(), String> {
        self.worker
            .run(move |context| context.qualification_block_decision_after_tls(arrived, release))
    }
    pub fn configure_decisions(&self, configuration: Configuration) -> Result<(), String> {
        configuration.validate()?;
        self.worker
            .run(move |context| context.configure_decisions(configuration))
    }
    /// Trusted owner setup: installs evidence and credentials and explicitly
    /// enables Shadow mode. This is not a read-only evidence import. CLI profile
    /// selection remains reference-only and disabled unless chosen explicitly.
    pub fn install_decision_qualification(
        &self,
        record: QualificationRecord,
        material: CredentialMaterial,
    ) -> Result<QualificationRef, String> {
        self.worker
            .run(move |context| context.install_decision_qualification(record, material))
    }
    pub fn install_decision_credential(&self, material: CredentialMaterial) -> Result<(), String> {
        self.worker
            .run(move |context| context.install_decision_credential(material))
    }
    #[cfg(feature = "qualification")]
    pub fn qualification_configure_decisions(
        &self,
        configuration: FixtureConfiguration,
    ) -> Result<(), String> {
        self.worker
            .run(move |context| context.qualification_configure_decisions(configuration))
    }
    pub fn decision_shadow_pending(&self, thread: ThreadId) -> Result<bool, String> {
        let binding = self.binding(thread)?;
        self.worker
            .run(move |context| Ok(context.decision_shadow_pending(&binding)))
    }
    pub async fn evaluate_pending_routing_shadow(
        &self,
        thread: ThreadId,
    ) -> Result<ShadowOutcome, String> {
        let binding = self.binding(thread)?;
        let scoped = binding.clone();
        let selected = self
            .worker
            .run(move |context| context.take_decision_candidate(&scoped))?;
        let worker::decision::Selection::Candidate(candidate) = selected else {
            let worker::decision::Selection::Baseline(outcome) = selected else {
                unreachable!()
            };
            return Ok(outcome);
        };
        let generation = self
            .runtime
            .admission_generation(thread)
            .map_err(|e| format!("{e:?}"))?;
        let mut runtime = HostWorkAdmission::admit(
            &self.runtime,
            thread,
            HostWorkKind::Model,
            "canonical-decision-shadow",
        )?;
        let capture = candidate.clone();
        let admitted = match self
            .worker
            .run(move |context| context.reserve_decision(capture))
        {
            Ok(admitted) => admitted,
            Err(error) => {
                runtime.complete()?;
                return Ok(ShadowOutcome::baseline(
                    Some(candidate.seed.main_attempt.clone()),
                    &error,
                ));
            }
        };
        let mut guard = ShadowGuard {
            worker: self.worker.clone(),
            runtime,
            binding: binding.clone(),
            attempt: admitted.attempt.clone(),
            finished: false,
        };
        let sent = admitted.clone();
        let _send = self
            .worker
            .run(move |context| context.submit_decision(&sent))?;
        let deadline = tokio::time::Instant::now()
            + std::time::Duration::from_millis(
                candidate.deadline.get().saturating_sub(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis()
                        .try_into()
                        .unwrap_or(u64::MAX),
                ),
            );
        let check = self.worker.clone();
        let checked = candidate.clone();
        let response = transport::send(
            self.runtime.clone(),
            thread,
            generation,
            transport::Request {
                operation: candidate.prepared.prepared.evaluator().operation,
                body: candidate.prepared.bytes.clone(),
                body_digest: candidate.prepared.body_digest.clone(),
                deadline,
            },
            candidate.credential.clone(),
            candidate.destination.clone(),
            move || {
                check.run({
                    let candidate = checked.clone();
                    move |context| context.validate_decision_candidate(&candidate)
                })
            },
        )
        .await;
        let completed = admitted.clone();
        let outcome = match response {
            Ok(response) => self.worker.run_cleanup(move |context| {
                context.complete_decision(&completed, response, false, true)
            })?,
            Err(failure) => {
                let completed = admitted.clone();
                self.worker.run_cleanup(move |context| {
                    context.complete_decision(
                        &completed,
                        transport::Response {
                            status: failure.status.unwrap_or(0),
                            body: failure.body,
                            request_written: failure.request_written,
                        },
                        true,
                        failure.request_submitted,
                    )
                })?
            }
        };
        guard.runtime.complete()?;
        guard.finished = true;
        Ok(outcome)
    }
}
struct ShadowGuard {
    worker: worker::Worker,
    runtime: Box<dyn HostWorkPermit>,
    binding: ThreadBinding,
    attempt: AttemptId,
    finished: bool,
}
impl Drop for ShadowGuard {
    fn drop(&mut self) {
        if !self.finished {
            let binding = self.binding.clone();
            let attempt = self.attempt.clone();
            if self
                .worker
                .run_cleanup(move |context| context.cancel_decision(&binding, &attempt))
                .and_then(|()| self.runtime.complete())
                .is_err()
            {
                self.worker.fence();
            }
        }
    }
}
