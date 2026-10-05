// SPDX-License-Identifier: Apache-2.0
//! Effective execution metadata is captured by the trusted owner, never inferred
//! from a reopened artifact or used as a substitute for controller admission.
use super::*;
use sha2::{Digest, Sha256};
use std::io::Read;

impl Context {
    pub(in crate::foundation) fn configure_execution_constraints(
        &mut self,
        deadline: vcp_domain::Limit<Timestamp>,
    ) -> Result<()> {
        if !self.owner_alive || self.authority_pending || !self.streams.is_empty() {
            return Err("execution constraint configuration requires an available owner".into());
        }
        if deadline.finite().is_some_and(|deadline| *deadline <= now()) {
            return Err("effective execution deadline elapsed".into());
        }
        let scope = Scope {
            workspace: self.config.workspace.clone(),
            session: self.config.session.clone(),
            task: self.config.root_task.clone(),
        };
        let accepted =
            self.runtime
                .block_on(vcp_engine::public_start::retained_start_budget_store(
                    self.engine.store(),
                    &scope,
                ))?;
        let mut executable = std::fs::File::open(std::env::current_exe()?)?;
        let mut hash = Sha256::new();
        let mut chunk = [0u8; 64 * 1024];
        loop {
            let count = executable.read(&mut chunk)?;
            if count == 0 {
                break;
            }
            hash.update(&chunk[..count]);
        }
        self.initialize_root_budget()?;
        let ledger: vcp_domain::accounting::Ledger = self
            .engine
            .store()
            .current()
            .record(Collection::Ledger, scope.task.as_str(), &scope.workspace)?
            .decode()?;
        if ledger.scope != scope || ledger.cap != self.config.cap.micros {
            return Err("effective constraint ledger mismatch".into());
        }
        if deadline.is_unbounded() {
            self.suspend_retained_child_execution_time(&scope)?;
        }
        let original = accepted.map(|accepted| serde_json::json!({"accepted_at":accepted.accepted_at,"budget":accepted.budget}));
        self.capture(&scope, Channel::Evidence, &vcp_protocol::canonical_bytes(&serde_json::json!({
            "version":1,"scope":scope,"recorded_at":now(),"original_acceptance":original,
            "effective":{"cap":self.config.cap,"deadline":deadline},
            "ledger_revision":ledger.revision,"policy_revision":ledger.policy,
            "execution_revision":{"package_version":env!("CARGO_PKG_VERSION"),"executable_sha256":format!("{:x}",hash.finalize())},
            "origin":"trusted execution owner configuration","authority":"diagnostic evidence only"
        }))?, "execution-constraints/1")?;
        self.execution_deadline = Some(deadline);
        Ok(())
    }

    fn suspend_retained_child_execution_time(&mut self, scope: &Scope) -> Result<()> {
        let Some(graph) =
            vcp_engine::agents::graph(self.engine.store().current(), scope, &scope.task)?
        else {
            return Ok(());
        };
        let mut selected = std::collections::BTreeSet::new();
        for (id, spec) in &graph.children {
            let task: Task = self
                .engine
                .store()
                .current()
                .record(Collection::Task, id.as_str(), &scope.workspace)?
                .decode()?;
            if !task.state.terminal()
                && !spec.deadline.is_unbounded()
                && !graph.execution_time.contains_key(id)
            {
                selected.insert(id.clone());
            }
        }
        if selected.is_empty() {
            return Ok(());
        }
        let originals: std::collections::BTreeMap<_, _> = selected
            .iter()
            .map(|id| (id.clone(), graph.children[id].deadline))
            .collect();
        let evidence = self.capture(scope, Channel::Evidence,
            &vcp_protocol::canonical_bytes(&serde_json::json!({
                "version":1,"scope":scope,"original_graph_revision":graph.revision,
                "original_deadlines":originals,"effective_deadline":vcp_domain::Limit::<Timestamp>::Unbounded,
                "actor":self.config.actor,"origin":"explicit trusted owner execution configuration",
                "authority":"requested native transition; canonical graph revision is authoritative"
            }))?, "child-execution-time/1")?;
        let facts = vcp_engine::HostFacts {
            now: now(),
            policy: vcp_engine::policy::current(self.engine.store().current(), &scope.workspace)?
                .revision,
            may_execute: true,
            resume: None,
        };
        self.runtime
            .block_on(self.engine.suspend_child_execution_time(
                scope,
                vcp_engine::agents::NativeExecutionTimeEvidence {
                    expected_graph: graph.revision,
                    children: selected,
                    evidence: evidence.spec.id,
                },
                &self.access,
                &facts,
            ))?;
        Ok(())
    }
}
