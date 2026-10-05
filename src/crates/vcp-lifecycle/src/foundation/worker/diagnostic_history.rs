// SPDX-License-Identifier: Apache-2.0
//! Retain the bounded execution owner's observations after explicit draining.
//! These events grant no execution, retry, completion or accounting authority.
use super::*;
use std::collections::BTreeMap;
use vcp_protocol::event::{EventInput, EventKind};

impl Context {
    pub(in crate::foundation) fn retain_execution_diagnostics(&mut self) -> Result<()> {
        let snapshot = self.diagnostics.snapshot();
        if !snapshot.available || snapshot.observations.is_empty() {
            return Ok(());
        }
        let scopes: BTreeMap<_, _> = snapshot
            .observations
            .iter()
            .map(|observation| (observation.scope.task.clone(), observation.scope.clone()))
            .collect();
        let source_watermark = self.engine.store().current().watermark;
        for scope in scopes.into_values() {
            if scope.workspace != self.config.workspace {
                return Err("diagnostic retention workspace mismatch".into());
            }
            let task: Task = self
                .engine
                .store()
                .current()
                .record(Collection::Task, scope.task.as_str(), &scope.workspace)?
                .decode()?;
            if task.scope != scope
                || task.redaction.is_some()
                || !vcp_memory::retention::recall_allowed(
                    self.engine.store().current(),
                    &scope.workspace,
                    &vcp_memory::retention::Target::Record(key(
                        Collection::Task,
                        scope.task.as_str(),
                    )),
                )?
            {
                // Erased task details must not be recreated by shutdown telemetry.
                continue;
            }
            let data = serde_json::json!({
                "version":1, "execution_diagnostics":snapshot.clone().for_scope(&scope),
                "source_watermark":source_watermark, "capture_boundary":"owner_drained",
                "authority":"diagnostic evidence only"
            });
            // The collector is bounded to 256 observations. Reject an unexpected
            // encoded expansion rather than silently losing explanatory evidence.
            if canonical_bytes(&data)?.len() > 256 * 1024 {
                return Err("execution diagnostic snapshot exceeds 256 KiB".into());
            }
            let transaction = Transaction {
                id: TransactionId::new(),
                expected_watermark: self.engine.store().current().watermark,
                mutations: vec![],
                events: vec![EventInput {
                    id: EventId::new(),
                    workspace: scope.workspace,
                    session: scope.session,
                    task: Some(scope.task),
                    actor: self.config.actor.clone(),
                    correlation: CommandId::new(),
                    causation: None,
                    timestamp: now(),
                    kind: EventKind::Diagnostic,
                    artifacts: vec![],
                    data,
                    metadata: None,
                }],
                command: None,
            };
            self.runtime
                .block_on(self.engine.store_mut().transact(transaction))?;
        }
        Ok(())
    }
}
