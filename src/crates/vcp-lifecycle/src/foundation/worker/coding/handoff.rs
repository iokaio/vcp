// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_context::{
    handoff::{Discarded, Packet},
    manifest::Sealed,
};
use vcp_domain::effect::Effect;

impl Context {
    pub(in crate::foundation::worker) fn ensure_coding_ledger(&mut self) -> Result<()> {
        if self
            .engine
            .store()
            .state()
            .records
            .values()
            .any(|r| r.collection == Collection::Ledger && r.id == self.config.root_task.as_str())
        {
            return Ok(());
        }
        let root: Task = self
            .engine
            .store()
            .state()
            .record(
                Collection::Task,
                self.config.root_task.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        let actor = self.actor();
        self.runtime.block_on(vcp_budget::initialize(
            self.engine.store_mut(),
            root.scope,
            self.config.cap.clone(),
            self.config.protected,
            None,
            &actor,
        ))?;
        Ok(())
    }
    /// Every outgoing coding boundary has an explicit captured continuation.
    /// No provider opaque state is promoted to instructions or fabricated text.
    pub(super) fn capture_coding_handoff(
        &mut self,
        binding: &ThreadBinding,
        sealed: &Sealed,
    ) -> Result<()> {
        let state = self
            .coding
            .get(&binding.scope.task)
            .ok_or("coding setup missing")?;
        let mut ids: std::collections::BTreeSet<_> = sealed
            .manifest
            .included
            .iter()
            .map(|p| p.artifact.clone())
            .collect();
        ids.extend(state.history_sources.clone());
        if let Some(sources) = &state.final_response {
            ids.extend(sources.clone());
        }
        // Compacted originals remain dependencies even when absent from the
        // destination's rendered conversation.
        ids.extend(state.history.iter().map(|p| p.artifact.clone()));
        if ids.len() > 4096 {
            return Err("handoff reference count exceeds bound".into());
        }
        let records = &self.engine.store().state().records;
        let ledger: Ledger = self
            .engine
            .store()
            .state()
            .record(
                Collection::Ledger,
                self.config.root_task.as_str(),
                &binding.scope.workspace,
            )?
            .decode()?;
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
        let effects = records
            .values()
            .filter(|r| r.collection == Collection::Effect)
            .map(Record::decode::<Effect>)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|e| e.scope.workspace == binding.scope.workspace)
            .collect::<Vec<_>>();
        let questions: Vec<_> = records
            .values()
            .filter(|r| {
                r.collection == Collection::Approval && r.workspace == binding.scope.workspace
            })
            .map(|r| r.value.clone())
            .collect();
        let mut current_state =
            serde_json::json!({"task":task,"effects":effects,"questions":questions});
        if self.verification.contains_key(&binding.scope.task) {
            current_state["continuity"] = self.continuity_facts(binding)?.0;
            let (workspace, references) = self.handoff_workspace(binding)?;
            if current_state["continuity"]["current_source"] != workspace["current"] {
                return Err("workspace changed during handoff capture".into());
            }
            current_state["workspace"] = workspace;
            ids.extend(references);
            if ids.len() > 4096 {
                return Err("handoff reference count exceeds bound".into());
            }
        }
        let mut references = Vec::new();
        let mut discarded = Vec::new();
        let mut total_bytes = 0u64;
        for id in ids {
            let descriptor: ArtifactDescriptor = self
                .engine
                .store()
                .state()
                .record(Collection::Artifact, id.as_str(), &binding.scope.workspace)?
                .decode()?;
            total_bytes = total_bytes
                .checked_add(descriptor.length.get())
                .ok_or("handoff byte count overflow")?;
            if total_bytes > 64 * 1024 * 1024 {
                return Err("handoff reference bytes exceed read bound".into());
            }
            let bytes = self.coding_artifact(&id)?;
            if descriptor.spec.scope != binding.scope
                || descriptor.sha256 != vcp_protocol::digest_bytes(&bytes)
            {
                return Err("handoff source changed or outside task".into());
            }
            if descriptor.spec.schema == "responses-sse-observed-through-terminal/1" {
                discarded.push(Discarded {
                    artifact: id.clone(),
                    field: "provider_native_response_state".into(),
                    sha256: descriptor.sha256.clone(),
                    reason: "The native response representation, including opaque fields, is retained only by reference; portable context uses attributed normalized parts".into(),
                });
            }
            // SSE payloads retain all provider fields. Identify opaque output
            // items and opaque reasoning fields by path/hash, not their contents.
            let response_text =
                if descriptor.spec.schema == "responses-sse-observed-through-terminal/1" {
                    std::str::from_utf8(&bytes).ok()
                } else {
                    None
                };
            if let Some(text) = response_text {
                for (line, value) in text.lines().enumerate().filter_map(|(line, text)| {
                    text.strip_prefix("data:")
                        .and_then(|data| {
                            serde_json::from_str::<serde_json::Value>(data.trim()).ok()
                        })
                        .map(|v| (line, v))
                }) {
                    let prefix = format!("sse/{line}");
                    collect_opaque(&id, &prefix, &value, &mut discarded)?;
                }
            }
            references.push(descriptor);
        }
        let packet = Packet::new(
            sealed.manifest.clone(),
            ledger,
            current_state,
            references,
            discarded,
        )?;
        self.validate_continuity_ready(binding)?;
        if let Some(pending) = self
            .routing
            .as_mut()
            .and_then(|runtime| runtime.pending.get_mut(&binding.scope.task))
        {
            pending.handoff = Some(vcp_models::escalation::bind_handoff(
                &pending.plan,
                &packet,
                sealed,
            )?);
        }
        self.capture(
            &binding.scope,
            Channel::Evidence,
            &canonical_bytes(&packet)?,
            "canonical-context-handoff/1",
        )?;
        Ok(())
    }
}

fn collect_opaque(
    id: &ArtifactId,
    path: &str,
    value: &serde_json::Value,
    discarded: &mut Vec<Discarded>,
) -> Result<()> {
    let mut record = |path: String, value: &serde_json::Value| -> Result<()> {
        if discarded.len() >= 4096 {
            return Err("handoff opaque field limit".into());
        }
        discarded.push(Discarded { artifact: id.clone(), field: path,
            sha256: vcp_protocol::digest_bytes(&canonical_bytes(value)?),
            reason: "Provider-specific state is retained in raw evidence but excluded from portable context; no replacement reasoning is invented".into() });
        Ok(())
    };
    if let Some(object) = value.as_object() {
        for key in [
            "encrypted_content",
            "reasoning_details",
            "reasoning",
            "signature",
        ] {
            if let Some(field) = object.get(key) {
                record(format!("{path}/{key}"), field)?;
            }
        }
        if let Some(item) = object.get("item") {
            if item
                .get("type")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|kind| !matches!(kind, "message" | "function_call"))
            {
                record(format!("{path}/item"), item)?;
            }
        }
        if let Some(response) = object.get("response") {
            collect_opaque(id, &format!("{path}/response"), response, discarded)?;
        }
        if let Some(output) = object.get("output").and_then(serde_json::Value::as_array) {
            for (index, item) in output.iter().enumerate() {
                collect_opaque(id, &format!("{path}/output/{index}"), item, discarded)?;
            }
        }
    }
    Ok(())
}
