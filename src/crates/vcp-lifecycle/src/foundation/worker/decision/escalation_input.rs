// SPDX-License-Identifier: Apache-2.0
//! Escalation shadow inputs come only from admitted plans and retained evidence.
use super::*;
use vcp_context::manifest::{Content, Part};
use vcp_models::escalation::{self as model, AdvisoryAction, AdvisoryObservation, ObservationKind};

const MAX_OBSERVATION_BYTES: u64 = 4096;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pair {
    sequence: u64,
    attempt: AttemptId,
    parts: [Part; 2],
    sources: Vec<ArtifactId>,
}

impl Context {
    pub(super) fn escalation_advisory_request(
        &self,
        seed: &Seed,
        deadline: Timestamp,
    ) -> Result<codec::Request> {
        let plan = seed
            .source
            .escalation
            .as_ref()
            .ok_or("admitted escalation plan absent")?;
        let kind = match plan.trigger.kind {
            model::TriggerKind::InvalidToolOutput => ObservationKind::Error,
            model::TriggerKind::FailedVerification => ObservationKind::Check,
            _ => {
                return Err("escalation trigger has no canonical advisory evidence mapping".into())
            }
        };
        if plan.trigger.evidence.is_empty() || plan.trigger.evidence.len() > 64 {
            return Err("escalation advisory evidence count bound".into());
        }
        let task: Task = self
            .engine
            .store()
            .state()
            .record(
                Collection::Task,
                seed.binding.scope.task.as_str(),
                &seed.binding.scope.workspace,
            )?
            .decode()?;
        if task.scope != seed.binding.scope || task.root != self.config.root_task {
            return Err("escalation advisory task scope changed".into());
        }
        let revisions = self.context_revisions(&seed.binding)?;
        let mut evidence = BTreeMap::new();
        let mut observations = Vec::new();
        let mut sequences = BTreeSet::new();
        for id in &plan.trigger.evidence {
            if !vcp_memory::retention::recall_allowed(
                self.engine.store().state(),
                &seed.binding.scope.workspace,
                &vcp_memory::retention::Target::Record(key(Collection::Artifact, id.as_str())),
            )? {
                return Err("escalation advisory evidence excluded by retention".into());
            }
            let descriptor: ArtifactDescriptor = self
                .engine
                .store()
                .state()
                .record(
                    Collection::Artifact,
                    id.as_str(),
                    &seed.binding.scope.workspace,
                )?
                .decode()?;
            if descriptor.spec.scope != seed.binding.scope
                || descriptor.spec.schema != "canonical-coding-pair/1"
                || descriptor.state != CaptureState::Complete
                || descriptor.length.get() > MAX_OBSERVATION_BYTES
            {
                return Err(
                    "escalation advisory evidence scope, completeness or size bound".into(),
                );
            }
            let mut bytes = Vec::new();
            vcp_audit::history::History::read_artifact(
                self.engine.store(),
                &self.history_access(),
                id,
                &mut bytes,
            )?;
            let (pair, name, result) = observed_pair(&bytes, &seed.binding.scope)?;
            if !sequences.insert(pair.sequence) {
                return Err("duplicate escalation advisory evidence sequence".into());
            }
            let attempt: Attempt = self
                .engine
                .store()
                .state()
                .record(
                    Collection::Attempt,
                    pair.attempt.as_str(),
                    &seed.binding.scope.workspace,
                )?
                .decode()?;
            if attempt.scope != seed.binding.scope || attempt.steering != task.steering {
                return Err("escalation advisory evidence attempt changed".into());
            }
            match kind {
                ObservationKind::Check => {
                    if name != "vcp_verify" {
                        return Err("escalation advisory verification tool mismatch".into());
                    }
                    let verification: vcp_domain::verification::Verification =
                        serde_json::from_value(result["verification"].clone())?;
                    let canonical: vcp_domain::verification::Verification = self
                        .engine
                        .store()
                        .state()
                        .record(
                            Collection::Verification,
                            verification.id.as_str(),
                            &seed.binding.scope.workspace,
                        )?
                        .decode()?;
                    if canonical != verification
                        || canonical.redaction.is_some()
                        || canonical.scope != seed.binding.scope
                        || canonical.steering != task.steering
                        || !canonical.checks.iter().any(|check| {
                            matches!(
                                check.outcome,
                                vcp_domain::verification::CheckOutcome::Failed { .. }
                            )
                        })
                    {
                        return Err(
                            "escalation advisory verification is not a current retained failure"
                                .into(),
                        );
                    }
                }
                ObservationKind::Error => {
                    if (name == "vcp_verify" && result.get("verification").is_some())
                        || result["complete"] != false
                        || !result["error"].is_string()
                        || result["stale"] == true
                    {
                        return Err("escalation advisory invalid-output evidence changed".into());
                    }
                }
                _ => unreachable!(),
            }
            if evidence
                .insert(id.to_string(), descriptor.sha256.clone())
                .is_some()
            {
                return Err("duplicate escalation advisory evidence artifact".into());
            }
            observations.push(AdvisoryObservation {
                evidence: id.to_string(),
                source_revision: descriptor.sha256,
                kind,
                // Preserve the entire observed pair, including error and tool
                // arguments. Oversized evidence abstains instead of truncating.
                summary: String::from_utf8(bytes)?,
            });
        }
        let selected = match plan.trigger.class() {
            model::Class::TransportRetry => AdvisoryAction::Retry,
            model::Class::QualitySwitch => AdvisoryAction::Escalate,
            model::Class::Decomposition => AdvisoryAction::Replan,
        };
        let request = model::advisory_request(&model::AdvisoryInput {
            binding: codec::Binding {
                scope: task.scope,
                root: task.root,
                step: task.revision,
                steering: task.steering,
                authority: revisions.authority,
                deletion: revisions.deletion,
                policy: seed.source.baseline.input.policy.clone(),
                catalog: seed.source.baseline.input.catalog.clone(),
                input: String::new(),
                evidence,
            },
            trigger: plan.trigger.clone(),
            counters: plan.after.clone(),
            observations,
            permitted_actions: BTreeSet::from([selected, AdvisoryAction::Stop]),
            // Conservative shadow guardrails, not findings that review was
            // requested or checks succeeded. No result may weaken these gates.
            required_review: true,
            hard_failure: true,
            checks_complete: false,
            deadline,
        })?;
        request.validate(now())?;
        Ok(request)
    }
}

fn observed_pair(bytes: &[u8], scope: &Scope) -> Result<(Pair, String, serde_json::Value)> {
    if bytes.len() as u64 > MAX_OBSERVATION_BYTES {
        return Err("escalation advisory observation bytes bound".into());
    }
    let pair: Pair = serde_json::from_slice(bytes)?;
    if pair.parts.iter().any(|part| &part.scope != scope) || pair.sources.len() > 4096 {
        return Err("escalation advisory pair scope or source bound".into());
    }
    let (Content::ToolCall { id: call, name, .. }, Content::ToolResult { id: result, output }) =
        (&pair.parts[0].content, &pair.parts[1].content)
    else {
        return Err("escalation advisory requires a canonical call/result pair".into());
    };
    if call != result {
        return Err("escalation advisory orphan tool result".into());
    }
    let name = name.clone();
    let result = serde_json::from_str(output)?;
    Ok((pair, name, result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vcp_context::manifest::{Kind, Trust};

    fn pair(scope: &Scope) -> serde_json::Value {
        let call = Part {
            id: "call".into(),
            scope: scope.clone(),
            kind: Kind::ToolCall,
            trust: Trust::Observed,
            artifact: ArtifactId::new(),
            source_hash: "a".repeat(64),
            source_length: ByteCount::new(1),
            start: ByteCount::ZERO,
            end: ByteCount::new(1),
            content: Content::ToolCall {
                id: "paired".into(),
                name: "vcp_read".into(),
                arguments: serde_json::json!({"path":"missing.rs"}),
            },
            mandatory: true,
            rank: 0,
            reason: "observed pair".into(),
            applicable_paths: vec![],
            file: None,
        };
        let mut result = call.clone();
        result.id = "result".into();
        result.kind = Kind::ToolResult;
        result.content = Content::ToolResult {
            id: "paired".into(),
            output: r#"{"complete":false,"error":"missing file"}"#.into(),
        };
        serde_json::json!({"sequence":1,"attempt":AttemptId::new(),"parts":[call,result],"sources":[]})
    }

    #[test]
    fn observed_pair_preserves_failure_and_rejects_scope_pairing_and_size_changes() {
        let scope = Scope {
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
            task: TaskId::new(),
        };
        let original = pair(&scope);
        let bytes = canonical_bytes(&original).unwrap();
        let (_, tool, result) = observed_pair(&bytes, &scope).unwrap();
        assert_eq!(tool, "vcp_read");
        assert_eq!(result["error"], "missing file");
        assert_eq!(result["complete"], false);
        let mut changed = original.clone();
        changed["parts"][1]["scope"]["task"] = serde_json::to_value(TaskId::new()).unwrap();
        assert!(observed_pair(&canonical_bytes(&changed).unwrap(), &scope).is_err());
        let mut changed = original.clone();
        changed["parts"][1]["content"]["id"] = serde_json::json!("orphan");
        assert!(observed_pair(&canonical_bytes(&changed).unwrap(), &scope).is_err());
        let mut changed = original;
        changed["parts"][1]["content"]["output"] = serde_json::json!("x".repeat(4097));
        assert!(observed_pair(&canonical_bytes(&changed).unwrap(), &scope).is_err());
    }
}
