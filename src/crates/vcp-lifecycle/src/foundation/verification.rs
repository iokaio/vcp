// SPDX-License-Identifier: Apache-2.0
//! Owner-only verification requests. Project/model text cannot construct proof.
use super::*;
use vcp_domain::verification::{CheckOutcome, Verification};
use vcp_tools::verification::{Plan, Requirement};

#[derive(Clone, serde::Serialize)]
pub struct VerificationConfig {
    pub requirements: Vec<Requirement>,
    /// Analysis-only work must cite complete canonical artifacts. Empty is
    /// permitted at setup; citations are supplied with the verification request.
    pub rationale: String,
}
pub(crate) struct ObservedCheck {
    pub effect: Option<ToolRunId>,
    pub plan: Plan,
    pub outcome: CheckOutcome,
    pub exit_code: Option<i32>,
    pub artifacts: Vec<ArtifactId>,
    pub prepared: Option<Arc<vcp_tools::process::Prepared>>,
}
impl CanonicalHost {
    /// Establish the original source baseline before effects. Reopen restores
    /// the baseline but never restores a completion capability or runner profile.
    pub fn configure_verification(
        &self,
        thread: ThreadId,
        config: VerificationConfig,
    ) -> Result<(), String> {
        let binding = self.binding(thread)?;
        self.worker
            .run(move |context| context.configure_verification(&binding, config))
    }
    pub async fn verify(
        &self,
        thread: ThreadId,
        citations: Vec<ArtifactId>,
    ) -> Result<Verification, String> {
        self.verify_before_publish(thread, citations, || {}).await
    }

    /// Qualification-only control injection after checks, before publication.
    #[cfg(feature = "qualification")]
    pub async fn verify_with_publish_observer(
        &self,
        thread: ThreadId,
        citations: Vec<ArtifactId>,
        before_publish: impl FnOnce(),
    ) -> Result<Verification, String> {
        self.verify_before_publish(thread, citations, before_publish)
            .await
    }

    async fn verify_before_publish(
        &self,
        thread: ThreadId,
        citations: Vec<ArtifactId>,
        before_publish: impl FnOnce(),
    ) -> Result<Verification, String> {
        if self.mcp_connections_present() {
            return Err("disconnect MCP processes before verification".into());
        }
        let binding = self.binding(thread)?;
        let scoped = binding.clone();
        let run = self
            .worker
            .run(move |context| context.begin_verification(&scoped, citations))?;
        let mut checks = Vec::new();
        for (index, plan) in run.plans.iter().enumerate() {
            let mut check = ObservedCheck {
                effect: None,
                plan: plan.clone(),
                outcome: CheckOutcome::NotRun {
                    reason: "no execution receipt".into(),
                },
                exit_code: None,
                artifacts: vec![],
                prepared: None,
            };
            if let Some(reason) = &plan.not_run {
                check.outcome = CheckOutcome::NotRun {
                    reason: reason.clone(),
                };
            } else {
                let mut dispatched = false;
                let observed: Result<(), String> = async {
                    let proposal = self.prepare_process(thread, plan.request.clone())?;
                    check.effect = Some(proposal.effect().clone());
                    check.prepared = Some(proposal.prepared.clone());
                    if !matches!(proposal.decision, vcp_policy::Decision::Allow { .. }) {
                        return Err(format!(
                            "check requires current process authority: {:?}; approval {:?}",
                            proposal.decision, proposal.question
                        ));
                    }
                    let scoped = binding.clone();
                    let intent = run.plan_artifact.clone();
                    let effect = proposal.effect().clone();
                    let receipt = self.worker.run(move |context| {
                        context.verification_check_intent(&scoped, intent, effect, index)
                    })?;
                    check.artifacts.push(receipt);
                    let process = self.dispatch_process(proposal)?;
                    dispatched = true;
                    let outcome = process.wait().await?;
                    check.exit_code = outcome.exit_code;
                    check.artifacts.extend([
                        outcome.evidence.spec.id.clone(),
                        outcome.stdout.spec.id.clone(),
                        outcome.stderr.spec.id.clone(),
                    ]);
                    let stdout = self.read_artifact(outcome.stdout.spec.id)?;
                    let stderr = self.read_artifact(outcome.stderr.spec.id)?;
                    check.outcome = vcp_tools::verification::evaluate(
                        plan,
                        outcome.exit_code,
                        &stdout,
                        &stderr,
                        outcome.reason.as_deref(),
                    );
                    Ok(())
                }
                .await;
                if let Err(reason) = observed {
                    // A failed launch can follow durable dispatch intent. The
                    // broker reconciles it to OutcomeUnknown; absence of a
                    // returned process handle must not imply no execution.
                    let may_have_dispatched = check.effect.as_ref().is_some_and(|id| {
                        self.snapshot()
                            .and_then(|state| {
                                let effect: vcp_domain::effect::Effect = state
                                    .record(
                                        vcp_store::contract::Collection::Effect,
                                        id.as_str(),
                                        &binding.scope.workspace,
                                    )
                                    .map_err(|e| e.to_string())?
                                    .decode()
                                    .map_err(|e| e.to_string())?;
                                Ok(effect.execution.is_some()
                                    || matches!(
                                        effect.state,
                                        vcp_domain::effect::EffectState::DispatchRecorded
                                            | vcp_domain::effect::EffectState::Running
                                            | vcp_domain::effect::EffectState::OutcomeUnknown
                                    ))
                            })
                            .unwrap_or(true)
                    });
                    check.outcome = if dispatched || may_have_dispatched {
                        CheckOutcome::Failed { reason }
                    } else {
                        CheckOutcome::NotRun { reason }
                    };
                }
            }
            checks.push(check);
        }
        before_publish();
        self.worker
            .run_cleanup(move |context| context.finish_verification(&binding, run, checks))
    }
    /// Call after retained work has drained. This takes the same worker/lifecycle
    /// locks used for admission; neither an active turn nor an old saved report
    /// can race a current completion decision.
    pub fn complete_verified(
        &self,
        thread: ThreadId,
        verification: VerificationId,
    ) -> Result<CommandReceipt, String> {
        self.complete_when_quiescent(thread, Some(verification))
    }
    pub(super) fn complete_when_quiescent(
        &self,
        thread: ThreadId,
        verification: Option<VerificationId>,
    ) -> Result<CommandReceipt, String> {
        if self.mcp_connections_present() {
            return Err("disconnect MCP processes before completion".into());
        }
        let binding = self.binding(thread)?;
        let runtime = self.runtime.clone();
        let scheduler = self.scheduler.clone();
        self.worker.run(move |context| {
            let state = runtime.0.state.lock().map_err(|_| "lifecycle poisoned")?;
            if !state.attached
                || state.sealing
                || state.startups_in_flight != 0
                || state.entries.values().any(|entry| entry.starts != 0)
                || state.work.iter().any(|work| work.receipt.is_none())
                || scheduler.busy()
            {
                return Err("completion requires quiescent retained work".into());
            }
            // Select final-response evidence under the same owner/admission
            // fence as completion. A new turn cannot clear it between lookups.
            let verification = match verification {
                Some(id) => id,
                None => context.coding_completion(&binding)?,
            };
            context.complete_verified(&binding, &verification)
        })
    }
}
