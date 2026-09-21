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
    /// Explicit offline fitting; inference never updates these parameters.
    pub fn fit_local_shadow(
        &self,
        thread: ThreadId,
        window: routing_state::HistoryWindow,
        minimum_samples: u64,
    ) -> Result<routing_state::local_stall::Fit, String> {
        let binding = self.binding(thread)?;
        self.worker
            .run(move |context| context.fit_local_shadow(&binding, window, minimum_samples))
    }
    pub fn install_local_shadow(
        &self,
        fit: routing_state::local_stall::Fit,
    ) -> Result<EvidencePin, String> {
        self.worker
            .run(move |context| context.install_local_shadow(fit))
    }
    /// Reopen starts without a selected fit. Selection validates retained sources.
    pub fn select_local_shadow(&self, pin: EvidencePin) -> Result<(), String> {
        self.worker
            .run(move |context| context.select_local_shadow(pin))
    }
    #[cfg(feature = "qualification")]
    pub fn qualification_block_local_after_compute(
        &self,
        arrived: std::sync::Arc<tokio::sync::Notify>,
        release: std::sync::Arc<tokio::sync::Notify>,
    ) -> Result<(), String> {
        self.worker
            .run(move |context| context.qualification_block_local_after_compute(arrived, release))
    }
    pub async fn evaluate_pending_routing_shadow(
        &self,
        thread: ThreadId,
    ) -> Result<ShadowOutcome, String> {
        self.evaluate_pending_decision_shadow(thread).await
    }
    /// Evaluate the installed purpose against the admitted canonical seed.
    /// Escalation-purpose advice is retained for comparison only; it never
    /// changes routing, required checks or escalation limits.
    pub async fn evaluate_pending_decision_shadow(
        &self,
        thread: ThreadId,
    ) -> Result<ShadowOutcome, String> {
        let local = self.evaluate_pending_local_shadow(thread).await;
        let mut outcome = self.evaluate_pending_remote_shadow(thread).await?;
        if let Some(local) = local {
            if let Some(object) = outcome.outcome.as_object_mut() {
                if let Some(statistics) = local.outcome.get("local_statistics") {
                    object.insert("local_statistics".into(), statistics.clone());
                }
                object.insert("local_shadow".into(), local.outcome);
                object.insert("producer_agreement".into(), serde_json::json!("abstained: local suspicion and remote advice have no shared calibrated comparison"));
                object.insert(
                    "composition".into(),
                    serde_json::json!(
                        "independent_shadow_evidence; no action or probability averaging"
                    ),
                );
            }
            outcome.artifacts.extend(local.artifacts);
        }
        Ok(outcome)
    }

    async fn evaluate_pending_local_shadow(&self, thread: ThreadId) -> Option<ShadowOutcome> {
        let binding = match self.binding(thread) {
            Ok(binding) => binding,
            Err(_) => return None,
        };
        let work = match self
            .worker
            .run(move |context| context.prepare_local_shadow(&binding))
        {
            Ok(Some(work)) => work,
            Ok(None) => return None,
            Err(error) => return Some(ShadowOutcome::baseline(None, &error)),
        };
        let main = Some(work.main_attempt.clone());
        let prepared = work.prepared.clone();
        let computation = tokio::task::spawn_blocking(move || prepared.compute());
        // Dropping or timing out this bounded pure computation cannot publish a
        // result, dispatch a provider, or mutate the canonical owner.
        let result =
            match tokio::time::timeout(std::time::Duration::from_secs(2), computation).await {
                Ok(Ok(Ok(outcome))) => outcome,
                Ok(Ok(Err(error))) => return Some(ShadowOutcome::baseline(main, &error)),
                Ok(Err(_)) => {
                    return Some(ShadowOutcome::baseline(
                        main,
                        "local shadow computation unavailable",
                    ))
                }
                Err(_) => {
                    return Some(ShadowOutcome::baseline(
                        main,
                        "local shadow deadline elapsed",
                    ))
                }
            };
        #[cfg(feature = "qualification")]
        if let Some((arrived, release)) = &work.local_after_compute {
            arrived.notify_one();
            release.notified().await;
        }
        Some(
            self.worker
                .run(move |context| context.complete_local_shadow(work, result))
                .unwrap_or_else(|error| ShadowOutcome::baseline(main, &error)),
        )
    }

    async fn evaluate_pending_remote_shadow(
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
            admitted: admitted.clone(),
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
        let checked = admitted.clone();
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
                    let admitted = checked.clone();
                    move |context| context.validate_admitted_decision(&admitted)
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
    admitted: std::sync::Arc<worker::decision::Admitted>,
    finished: bool,
}
impl Drop for ShadowGuard {
    fn drop(&mut self) {
        if !self.finished {
            let admitted = self.admitted.clone();
            if self
                .worker
                .run_cleanup(move |context| context.cancel_admitted_decision(&admitted))
                .and_then(|()| self.runtime.complete())
                .is_err()
            {
                self.worker.fence();
            }
        }
    }
}
