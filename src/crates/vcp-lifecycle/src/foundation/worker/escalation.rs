// SPDX-License-Identifier: Apache-2.0
//! Automatic escalation consumes host-recorded results and admitted counters.
use super::*;
use std::collections::BTreeSet;
use vcp_context::manifest::{Content, Part};
use vcp_models::{escalation as model, routing::ModelEndpoint};

#[derive(Clone)]
pub(super) struct Pending {
    pub plan: model::Plan,
    pub handoff: Option<model::Handoff>,
}
pub(super) struct Evidence {
    pub excluded: BTreeSet<ModelEndpoint>,
    pub trigger: Option<(Attempt, model::Trigger)>,
    pub counters: model::Counters,
    pub required_capabilities: BTreeSet<String>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Pair {
    sequence: u64,
    attempt: AttemptId,
    parts: [Part; 2],
    sources: Vec<ArtifactId>,
}
impl Context {
    pub(super) fn declare_escalation(
        &mut self,
        input: crate::foundation::routing_state::declarations::Input,
    ) -> Result<serde_json::Value> {
        use crate::foundation::routing_state::declarations;
        if self.interrupted_capture
            || self
                .routing
                .as_ref()
                .and_then(|r| r.configuration.escalation.as_ref())
                .is_none()
        {
            return Err(
                "owner declaration requires configured escalation and reconciled capture".into(),
            );
        }
        let access = self.routing_access();
        let declared_task: Task = self
            .engine
            .store()
            .state()
            .record(Collection::Task, input.task.as_str(), &access.workspace)?
            .decode()?;
        if declared_task.root != self.config.root_task {
            return Err("owner declaration root differs from current host".into());
        }
        if let Some(existing) = declarations::existing(self.engine.store(), &access, &input)? {
            return Ok(serde_json::to_value(existing)?);
        }
        let (task, _) = declarations::validate(self.engine.store(), &access, &input)?;
        let artifact = self.capture(
            &task.scope,
            Channel::Evidence,
            &canonical_bytes(&input)?,
            declarations::SCHEMA,
        )?;
        let result = self.runtime.block_on(declarations::record(
            self.engine.store_mut(),
            &access,
            input,
            artifact.spec.id,
            now(),
        ))?;
        Ok(serde_json::to_value(result)?)
    }
    pub(super) fn escalation_evidence(
        &mut self,
        binding: &ThreadBinding,
        policy: &model::Policy,
    ) -> Result<Evidence> {
        let access = self.routing_access();
        let mut required_capabilities = BTreeSet::new();
        let admissions = crate::foundation::routing_state::admitted_escalations(
            self.engine.store(),
            &access,
            &binding.scope,
        )
        .map_err(|e| -> Failure { e.into() })?;
        let mut counters = admissions
            .last()
            .map(|r| r.plan.after.clone())
            .unwrap_or_default();
        let attempts: Vec<Attempt> = self
            .engine
            .store()
            .state()
            .records
            .values()
            .filter(|r| r.collection == Collection::Attempt)
            .map(Record::decode)
            .collect::<std::result::Result<_, _>>()?;
        counters.total_attempts = u32::try_from(
            attempts
                .iter()
                .filter(|a| {
                    a.scope.workspace == binding.scope.workspace && a.root == self.config.root_task
                })
                .count(),
        )?;
        let switched: BTreeSet<_> = admissions
            .iter()
            .filter(|r| r.plan.trigger.class() != model::Class::TransportRetry)
            .map(|r| &r.attempt)
            .collect();
        counters.transport_retries = u32::try_from(
            attempts
                .iter()
                .filter(|a| {
                    a.scope.workspace == binding.scope.workspace
                        && a.root == self.config.root_task
                        && a.previous.is_some()
                        && !switched.contains(&a.id)
                })
                .count(),
        )?;
        if counters.total_attempts >= policy.max_total_attempts || now() >= policy.deadline {
            return Err("escalation root attempt limit or deadline reached".into());
        }
        let task_admissions: Vec<_> = admissions
            .iter()
            .filter(|r| r.scope.task == binding.scope.task)
            .collect();
        let excluded = task_admissions
            .iter()
            .filter(|r| r.plan.trigger.class() == model::Class::QualitySwitch)
            .map(|r| r.plan.previous.clone())
            .collect();
        let consumed: BTreeSet<_> = task_admissions
            .iter()
            .flat_map(|r| r.plan.trigger.evidence.iter().cloned())
            .collect();
        // Explicit owner input is claimed durably before any new selection or
        // reservation. A failed scheduling attempt does not replay this signal.
        if let Some((declaration, previous)) =
            self.runtime
                .block_on(crate::foundation::routing_state::declarations::claim(
                    self.engine.store_mut(),
                    &access,
                    &binding.scope.task,
                    now(),
                ))?
        {
            if let crate::foundation::routing_state::declarations::Kind::UnsupportedCapability {
                capability,
            } = &declaration.input.declaration
            {
                required_capabilities.insert(capability.clone());
            }
            return Ok(Evidence {
                excluded,
                trigger: Some((previous, declaration.trigger())),
                counters,
                required_capabilities,
            });
        }
        let mut pairs = Vec::new();
        let mut bytes_total = 0usize;
        for row in self.engine.store().state().records.values().filter(|r| {
            r.collection == Collection::Artifact && r.workspace == binding.scope.workspace
        }) {
            let descriptor: ArtifactDescriptor = row.decode()?;
            if descriptor.spec.scope != binding.scope
                || descriptor.spec.schema != "canonical-coding-pair/1"
            {
                continue;
            }
            if pairs.len() >= 4096 || descriptor.length.get() > 1024 * 1024 {
                return Err("escalation history bound".into());
            }
            let mut bytes = Vec::new();
            vcp_audit::history::History::read_artifact(
                self.engine.store(),
                &self.history_access(),
                &descriptor.spec.id,
                &mut bytes,
            )?;
            bytes_total = bytes_total
                .checked_add(bytes.len())
                .ok_or("escalation history overflow")?;
            if bytes_total > 16 * 1024 * 1024 {
                return Err("escalation history bytes bound".into());
            }
            let pair: Pair = serde_json::from_slice(&bytes)?;
            if pair.parts.iter().any(|p| p.scope != binding.scope) || pair.sources.len() > 4096 {
                return Err("escalation pair scope/bound".into());
            }
            pairs.push((pair, descriptor.spec.id));
        }
        pairs.sort_by_key(|(p, _)| p.sequence);
        if pairs.windows(2).any(|p| p[0].0.sequence == p[1].0.sequence) {
            return Err("duplicate escalation pair sequence".into());
        }
        let mut evidence = Vec::new();
        let mut kind = None;
        let mut predecessor = None;
        let task: Task = self
            .engine
            .store()
            .state()
            .record(
                Collection::Task,
                binding.scope.task.as_str(),
                &binding.scope.workspace,
            )?
            .decode()?;
        for (pair, id) in pairs.iter().rev() {
            if consumed.contains(id) {
                break;
            }
            let attempt = attempts
                .iter()
                .find(|a| a.id == pair.attempt)
                .ok_or("escalation pair lacks canonical attempt")?;
            if attempt.scope != binding.scope || attempt.steering != task.steering {
                break;
            }
            let (
                Content::ToolCall {
                    id: call_id, name, ..
                },
                Content::ToolResult {
                    id: result_id,
                    output,
                },
            ) = (&pair.parts[0].content, &pair.parts[1].content)
            else {
                return Err("invalid canonical escalation pair".into());
            };
            if call_id != result_id {
                return Err("orphan escalation result".into());
            }
            let result: serde_json::Value = serde_json::from_str(output)?;
            let observed = if name == "vcp_verify" && result.get("verification").is_some() {
                let verification: vcp_domain::verification::Verification =
                    serde_json::from_value(result["verification"].clone())?;
                let canonical: vcp_domain::verification::Verification = self
                    .engine
                    .store()
                    .state()
                    .record(
                        Collection::Verification,
                        verification.id.as_str(),
                        &binding.scope.workspace,
                    )?
                    .decode()?;
                if canonical != verification || canonical.redaction.is_some() {
                    return Err("escalation verification changed or redacted".into());
                }
                if verification.scope != binding.scope || verification.steering != task.steering {
                    break;
                }
                if verification.checks.iter().any(|c| {
                    matches!(
                        c.outcome,
                        vcp_domain::verification::CheckOutcome::Failed { .. }
                    )
                }) {
                    Some(model::TriggerKind::FailedVerification)
                } else {
                    None
                }
            } else if result["complete"] == false
                && result["error"].is_string()
                && result["stale"] != true
            {
                Some(model::TriggerKind::InvalidToolOutput)
            } else {
                None
            };
            let Some(observed) = observed else {
                break;
            };
            if kind.as_ref().is_some_and(|kind| kind != &observed) {
                break;
            }
            kind = Some(observed);
            if predecessor.is_none() {
                predecessor = Some(attempt.clone());
            }
            evidence.push(id.clone());
            if evidence.len() >= 64 {
                break;
            }
        }
        let trigger = match (predecessor, kind) {
            (Some(attempt), Some(kind))
                if evidence.len() >= policy.minimum_repeated_failures as usize =>
            {
                Some((
                    attempt,
                    model::Trigger {
                        kind,
                        observations: evidence.len() as u32,
                        evidence,
                    },
                ))
            }
            _ => None,
        };
        Ok(Evidence {
            excluded,
            trigger,
            counters,
            required_capabilities,
        })
    }
}
