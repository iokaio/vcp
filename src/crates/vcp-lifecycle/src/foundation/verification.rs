// SPDX-License-Identifier: Apache-2.0
//! Owner-only verification requests. Project/model text cannot construct proof.
use super::*;
use vcp_domain::verification::{CheckOutcome, Verification};
use vcp_tools::verification::{Plan, Requirement};

/// Scheduling information produced at the failing canonical boundary, never
/// inferred from diagnostic text. Only missing/stale proof permits rechecking.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionRejection {
    MissingVerification,
    StaleVerification,
    FailedChecks,
    UnresolvedEffects,
    Boundary,
}

#[derive(Clone, Debug)]
pub struct CompletionFailure {
    pub kind: CompletionRejection,
    pub message: String,
}

impl CompletionFailure {
    pub(crate) fn new(kind: CompletionRejection, message: &str) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}
impl std::fmt::Display for CompletionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}
impl std::error::Error for CompletionFailure {}

pub enum CompletionAttempt {
    Completed(CommandReceipt),
    Rejected(CompletionFailure),
}

#[derive(Clone, Default)]
pub enum VerificationSelection {
    #[default]
    Completion,
    Focused {
        affected_paths: Vec<String>,
        failed_checks: Vec<String>,
    },
}

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
/// Ephemeral same-invocation presentation, never canonical completion proof or
/// an artifact-read capability. Each entry is populated only after the current
/// history access checks for both captured streams succeed.
pub(super) struct VerificationPresentation {
    pub verification: Verification,
    pub diagnostics: Vec<serde_json::Value>,
}
impl CanonicalHost {
    pub fn completion_repair_feedback(&self, thread: ThreadId) -> Result<Option<String>, String> {
        let span = self.diagnostic_span(thread, super::execution_diagnostics::Phase::Repair)?;
        let binding = self.binding(thread)?;
        let result = self
            .worker
            .run(move |context| context.completion_repair_feedback(&binding));
        span.finish(&result);
        result
    }
    /// Current-owner completion checks, independent of a model reissuing
    /// vcp_verify after instruction refresh. No tool-call identity is replayed.
    pub async fn verify_for_completion(&self, thread: ThreadId) -> Result<Verification, String> {
        self.owner_verification_ready(thread)?;
        // Keep the same unsupported-hook boundary as the model-facing workflow.
        if self.has_tool_hooks(thread)? {
            return Err(
                "vcp_verify is unavailable with configured before_tool_authorization hooks".into(),
            );
        }
        let binding = self.binding(thread)?;
        let citations = self
            .worker
            .run(move |context| context.prepare_completion_verification(&binding))?;
        let observed = self.verify_before_publish(thread, citations, || {}).await?;
        self.finish_owner_verification(thread, observed.verification)
            .await
    }
    async fn finish_owner_verification(
        &self,
        thread: ThreadId,
        report: Verification,
    ) -> Result<Verification, String> {
        let outcomes = self
            .gate_event(
                thread,
                vcp_extensions::hooks::registry::HookEvent::AfterVerification,
                format!("owner-verify-{}", report.id),
                format!("owner-verify-{}", report.id),
                0,
                report
                    .outputs
                    .iter()
                    .cloned()
                    .chain(report.checks.iter().map(|check| check.output.clone()))
                    .collect(),
                serde_json::json!({"verification":report.id,"owner_scheduled":true}),
            )
            .await
            .and_then(|outcomes| super::hooks::adapters::gate_decision(&outcomes).map(|_| ()));
        if let Err(error) = outcomes {
            self.worker
                .run_cleanup(|context| context.pause_verification_hook())?;
            return Err(error);
        }
        Ok(report)
    }
    pub(super) fn owner_verification_ready(&self, thread: ThreadId) -> Result<(), String> {
        self.binding(thread)?;
        let runtime = self.runtime.clone();
        let scheduler = self.scheduler.clone();
        self.worker.run(move |_| {
            let state = runtime.0.state.lock().map_err(|_| "lifecycle poisoned")?;
            if !state.attached
                || state.sealing
                || state.startups_in_flight != 0
                || state.entries.values().any(|entry| entry.starts != 0)
                || state.work.iter().any(|work| work.receipt.is_none())
                || scheduler.busy()
            {
                return Err("owner verification requires quiescent retained work".into());
            }
            Ok(())
        })
    }
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
        Ok(self
            .verify_before_publish(thread, citations, || {})
            .await?
            .verification)
    }

    /// Owner-only diagnostic subset. Even full fallback remains diagnostic:
    /// it cannot mint the capability required by final completion.
    pub async fn verify_focused(
        &self,
        thread: ThreadId,
        affected_paths: Vec<String>,
        failed_checks: Vec<String>,
    ) -> Result<Verification, String> {
        self.owner_verification_ready(thread)?;
        if self.has_tool_hooks(thread)? {
            return Err(
                "vcp_verify is unavailable with configured before_tool_authorization hooks".into(),
            );
        }
        let report = self
            .verify_selected(
                thread,
                vec![],
                VerificationSelection::Focused {
                    affected_paths,
                    failed_checks,
                },
                || {},
            )
            .await?
            .verification;
        self.finish_owner_verification(thread, report).await
    }

    pub(super) async fn verify_for_coding(
        &self,
        thread: ThreadId,
        citations: Vec<ArtifactId>,
    ) -> Result<VerificationPresentation, String> {
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
        Ok(self
            .verify_before_publish(thread, citations, before_publish)
            .await?
            .verification)
    }

    async fn verify_before_publish(
        &self,
        thread: ThreadId,
        citations: Vec<ArtifactId>,
        before_publish: impl FnOnce(),
    ) -> Result<VerificationPresentation, String> {
        self.verify_selected(
            thread,
            citations,
            VerificationSelection::Completion,
            before_publish,
        )
        .await
    }

    async fn verify_selected(
        &self,
        thread: ThreadId,
        citations: Vec<ArtifactId>,
        selection: VerificationSelection,
        before_publish: impl FnOnce(),
    ) -> Result<VerificationPresentation, String> {
        let span =
            self.diagnostic_span(thread, super::execution_diagnostics::Phase::Verification)?;
        let result = self
            .verify_selected_inner(thread, citations, selection, before_publish)
            .await;
        let passed = result.as_ref().is_ok_and(|report| {
            report.verification.outstanding_issues.is_empty()
                && report.verification.unresolved_effects.is_empty()
                && report
                    .verification
                    .checks
                    .iter()
                    .all(|check| check.outcome == CheckOutcome::Passed)
        });
        if passed {
            span.succeeded();
        } else {
            span.failed();
        }
        result
    }
    async fn verify_selected_inner(
        &self,
        thread: ThreadId,
        citations: Vec<ArtifactId>,
        selection: VerificationSelection,
        before_publish: impl FnOnce(),
    ) -> Result<VerificationPresentation, String> {
        if self.mcp_connections_present() {
            return Err("disconnect MCP processes before verification".into());
        }
        let binding = self.binding(thread)?;
        let scoped = binding.clone();
        let run = self.worker.run(move |context| {
            context.begin_selected_verification(&scoped, citations, selection)
        })?;
        let mut checks = Vec::new();
        let mut diagnostics = Vec::new();
        // At most 32 owner-configured checks. Divide an 8-KiB raw tail budget
        // across both streams before decoding. Keep individual streams small:
        // the established presentation repeats the tail in decoding metadata,
        // and byte-ceiling context estimates account for both copies.
        let stream_limit = 2048.min(8192 / (run.plans.len().max(1) * 2));
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
                    let stdout = self.read_artifact(outcome.stdout.spec.id.clone())?;
                    let stderr = self.read_artifact(outcome.stderr.spec.id.clone())?;
                    // No arbitrary artifact lookup: these are precisely the
                    // streams just produced by this authorized check. Preserve
                    // current read gates before exposing their bounded tails.
                    let streams = outcome.bounded_output_presentation(stream_limit);
                    diagnostics.push(serde_json::json!({
                        "specification":plan.specification,"effect":outcome.effect,
                        "evidence":outcome.evidence.spec.id,"exit_code":outcome.exit_code,
                        "reason":outcome.reason,"stdout":streams["stdout"],"stderr":streams["stderr"],
                    }));
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
        let verification = self
            .worker
            .run_cleanup(move |context| context.finish_verification(&binding, run, checks))?;
        let _ = self.poll_observers(thread).await;
        Ok(VerificationPresentation {
            verification,
            diagnostics,
        })
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
        match self.try_complete_when_quiescent(thread, verification)? {
            CompletionAttempt::Completed(receipt) => Ok(receipt),
            CompletionAttempt::Rejected(failure) => Err(failure.to_string()),
        }
    }

    pub fn try_complete_coding_turn(&self, thread: ThreadId) -> Result<CompletionAttempt, String> {
        self.try_complete_when_quiescent(thread, None)
    }

    pub fn try_complete_verified(
        &self,
        thread: ThreadId,
        verification: VerificationId,
    ) -> Result<CompletionAttempt, String> {
        self.try_complete_when_quiescent(thread, Some(verification))
    }

    fn try_complete_when_quiescent(
        &self,
        thread: ThreadId,
        verification: Option<VerificationId>,
    ) -> Result<CompletionAttempt, String> {
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
            let result = (|| {
                let verification = match verification {
                    Some(id) => id,
                    None => context.coding_completion(&binding)?,
                };
                context.complete_verified(&binding, &verification)
            })();
            Ok(match result {
                Ok(receipt) => CompletionAttempt::Completed(receipt),
                Err(error) => CompletionAttempt::Rejected(
                    error
                        .downcast_ref::<CompletionFailure>()
                        .cloned()
                        .unwrap_or_else(|| {
                            CompletionFailure::new(
                                CompletionRejection::Boundary,
                                &error.to_string(),
                            )
                        }),
                ),
            })
        })
    }
}
