// SPDX-License-Identifier: Apache-2.0
//! Forks quote bounded historical evidence under a new task scope. No permit,
//! policy, approval, tool execution, or accounting liability is inherited.
use super::*;

const SCHEMA: &str = "canonical-coding-fork-history/1";
const MAX_HISTORY_BYTES: usize = 1024 * 1024;

impl Context {
    pub(super) fn fork_history(&mut self, binding: &ThreadBinding) -> Result<Option<Part>> {
        let state = self.engine.store().state();
        let session: Session = state
            .record(
                Collection::Session,
                binding.scope.session.as_str(),
                &binding.scope.workspace,
            )?
            .decode()?;
        let Some(boundary) = session.fork_through else {
            return Ok(None);
        };
        let turn: Turn = state
            .record(
                Collection::Turn,
                boundary.as_str(),
                &binding.scope.workspace,
            )?
            .decode()?;
        let task: Task = state
            .record(
                Collection::Task,
                binding.scope.task.as_str(),
                &binding.scope.workspace,
            )?
            .decode()?;
        if turn.state != TurnState::Completed
            || session.fork_origin.as_ref() != Some(&turn.scope.session)
            || task.fork_origin.as_ref() != Some(&turn.scope.task)
        {
            return Err("fork history ancestry or boundary rejected".into());
        }
        let end = state
            .events
            .iter()
            .find(|event| event.event.id == turn.cause)
            .ok_or("fork boundary event missing")?
            .watermark;
        let descriptors: Vec<ArtifactDescriptor> = state
            .records
            .values()
            .filter(|row| {
                row.collection == Collection::Artifact && row.workspace == binding.scope.workspace
            })
            .map(Record::decode)
            .collect::<std::result::Result<_, _>>()?;
        let existing: Vec<_> = descriptors
            .iter()
            .filter(|d| d.spec.scope == binding.scope && d.spec.schema == SCHEMA)
            .collect();
        if existing.len() > 1 {
            return Err("duplicate fork history checkpoint".into());
        }
        let descriptor = if let Some(descriptor) = existing.first() {
            (*descriptor).clone()
        } else {
            let source: Task = state
                .record(
                    Collection::Task,
                    turn.scope.task.as_str(),
                    &binding.scope.workspace,
                )?
                .decode()?;
            let mut selected = Vec::new();
            for descriptor in descriptors.iter().filter(|d| {
                d.spec.scope == turn.scope
                    && matches!(
                        d.spec.schema.as_str(),
                        "canonical-coding-pair/1"
                            | "coding-turn-input/1"
                            | "openrouter-normalized-response/1"
                            | SCHEMA
                    )
            }) {
                let recorded = state
                    .events
                    .iter()
                    .find(|event| event.event.artifacts.contains(&descriptor.spec.id))
                    .ok_or("historical artifact event missing")?
                    .watermark;
                if recorded <= end {
                    selected.push((recorded, descriptor.clone()));
                }
            }
            selected.sort_by_key(|(watermark, _)| *watermark);
            let mut observations = Vec::new();
            let mut total = 0usize;
            for (watermark, descriptor) in selected {
                let bytes = self.coding_artifact(&descriptor.spec.id)?;
                total = total
                    .checked_add(bytes.len())
                    .ok_or("fork history size overflow")?;
                if total > MAX_HISTORY_BYTES {
                    return Err(
                        "fork history exceeds bounded context capture; no partial fork is admitted"
                            .into(),
                    );
                }
                let content = match descriptor.spec.schema.as_str() {
                    "coding-turn-input/1" => {
                        serde_json::json!({"user_input":String::from_utf8(bytes)?})
                    }
                    "canonical-coding-pair/1" => {
                        let pair: Pair = serde_json::from_slice(&bytes)?;
                        if pair.parts.iter().any(|part| part.scope != turn.scope) {
                            return Err("historical pair scope denied".into());
                        }
                        serde_json::json!({"call":pair.parts[0].content,"result":pair.parts[1].content})
                    }
                    "openrouter-normalized-response/1" => {
                        let response: ResultBody = serde_json::from_slice(&bytes)?;
                        if response.visible_text_bytes > 0 && response.completed_messages.is_empty()
                        {
                            return Err("historical assistant text is unavailable".into());
                        }
                        serde_json::json!({"status":response.status,"messages":response.completed_messages})
                    }
                    _ => serde_json::from_slice(&bytes)?,
                };
                observations.push(serde_json::json!({"artifact":descriptor.spec.id,"sha256":descriptor.sha256,"watermark":watermark,"content":content}));
            }
            let bytes = canonical_bytes(
                &serde_json::json!({"schema":SCHEMA,"source_scope":turn.scope,"through_turn":boundary,"through_watermark":end,
                "objective_history":source.objectives.iter().filter(|objective| objective.steering <= turn.steering).collect::<Vec<_>>(),"observations":observations,"authority":"historical evidence only; no execution or approval authority"}),
            )?;
            if bytes.len() > MAX_HISTORY_BYTES {
                return Err("fork history exceeds bounded context capture".into());
            }
            self.capture(&binding.scope, Channel::Evidence, &bytes, SCHEMA)?
        };
        let bytes = self.coding_artifact(&descriptor.spec.id)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        if value["through_turn"] != boundary.as_str()
            || value["source_scope"] != serde_json::to_value(&turn.scope)?
        {
            return Err("fork checkpoint boundary mismatch".into());
        }
        Ok(Some(Part::captured_text(
            descriptor.spec.id.to_string(),
            Kind::History,
            ContextTrust::Untrusted,
            &descriptor,
            &bytes,
            true,
            0,
            "explicit conversation fork; quoted history, no inherited authority".into(),
        )?))
    }
}
