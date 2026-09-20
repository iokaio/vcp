// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::foundation::routing::Configuration;
use crate::foundation::routing_state;
use vcp_models::{
    catalog::Snapshot,
    routing::{self, RoutingDecision, RoutingInput},
};
use vcp_protocol::digest_bytes;

pub(super) struct Runtime {
    pub configuration: Configuration,
    pub prepared: HashMap<TaskId, RoutingDecision>,
    #[cfg(windows)]
    pub pending: HashMap<TaskId, super::escalation::Pending>,
}
impl Context {
    pub(super) fn require_configured_routing(&self) -> Result<()> {
        if self.routing.is_none()
            && routing_state::current_policy(self.engine.store(), &self.routing_access())
                .map_err(|e| -> Failure { e.into() })?
                .is_some()
        {
            return Err(
                "persisted routing policy requires explicit routing configuration after reopen"
                    .into(),
            );
        }
        Ok(())
    }
    pub fn routing_control(
        &mut self,
        request: crate::foundation::routing::Request,
    ) -> Result<serde_json::Value> {
        if !self.owner_alive || self.authority_pending {
            return Err("routing controls require current authority owner".into());
        }
        let access = self.routing_access();
        let ceilings = self
            .routing
            .as_ref()
            .map(|runtime| runtime.configuration.policy.clone());
        self.runtime
            .block_on(crate::foundation::routing::execute(
                self.engine.store_mut(),
                &access,
                request,
                ceilings.as_ref(),
                now(),
            ))
            .map_err(|e| -> Failure { e.into() })
    }
    pub(super) fn routing_access(&self) -> vcp_memory::access::Access {
        let history = self.history_access();
        vcp_memory::access::Access {
            workspace: history.workspace,
            actor: self.config.actor.clone(),
            authority: history.authority,
            read: true,
            write: true,
            tasks: history.tasks,
        }
    }
    pub fn configure_routing(&mut self, configuration: Configuration) -> Result<()> {
        if !self.owner_alive || self.authority_pending || self.provider.is_none() {
            return Err("routing requires a configured current provider owner".into());
        }
        configuration
            .validate()
            .map_err(|e| -> Failure { e.into() })?;
        if configuration
            .catalog
            .entries
            .iter()
            .filter_map(|entry| entry.snapshot.as_ref())
            .any(|snapshot| snapshot.price.currency != self.config.cap.currency)
        {
            return Err("routing currency differs from the root budget".into());
        }
        let scope = Scope {
            workspace: self.config.workspace.clone(),
            session: self.config.session.clone(),
            task: self.config.root_task.clone(),
        };
        let access = self.routing_access();
        let previous = routing_state::current_registry(self.engine.store(), &access)
            .map_err(|e| -> Failure { e.into() })?;
        if previous
            .as_ref()
            .is_none_or(|record| record.value.catalog.id != configuration.catalog.id)
        {
            let raw = self.capture(
                &scope,
                Channel::Evidence,
                &canonical_bytes(&configuration.raw_catalogs)?,
                "routing-catalog-sources/1",
            )?;
            self.runtime
                .block_on(routing_state::publish_registry(
                    self.engine.store_mut(),
                    &access,
                    previous.map(|r| r.revision),
                    configuration.catalog.clone(),
                    raw.spec.id,
                    now(),
                ))
                .map_err(|e| -> Failure { e.into() })?;
        }
        if routing_state::current_policy(self.engine.store(), &access)
            .map_err(|e| -> Failure { e.into() })?
            .is_none()
        {
            self.runtime
                .block_on(routing_state::initialize_policy(
                    self.engine.store_mut(),
                    &access,
                    configuration.policy.clone(),
                    now(),
                ))
                .map_err(|e| -> Failure { e.into() })?;
        }
        self.routing = Some(Runtime {
            configuration,
            prepared: HashMap::new(),
            #[cfg(windows)]
            pending: HashMap::new(),
        });
        Ok(())
    }

    pub(super) fn current_routing_policy(&self) -> Result<Option<routing::Policy>> {
        let Some(runtime) = &self.routing else {
            return Ok(None);
        };
        let access = self.routing_access();
        let current = routing_state::current_policy(self.engine.store(), &access)
            .map_err(|e| -> Failure { e.into() })?
            .ok_or("canonical routing policy unavailable")?;
        Ok(Some(
            routing_state::effective_policy(current.value, &runtime.configuration.policy)
                .map_err(|e| -> Failure { e.into() })?,
        ))
    }

    #[cfg(windows)]
    pub(super) fn select_coding_snapshot(
        &mut self,
        binding: &ThreadBinding,
        parts: &[vcp_context::manifest::Part],
        schemas: &serde_json::Value,
    ) -> Result<Snapshot> {
        self.require_configured_routing()?;
        let Some(runtime) = self.routing.as_ref() else {
            return Ok(self
                .provider
                .as_ref()
                .ok_or("provider missing")?
                .snapshot
                .clone());
        };
        let mut configuration = runtime.configuration.clone();
        self.routing
            .as_mut()
            .ok_or("routing configuration missing")?
            .pending
            .remove(&binding.scope.task);
        configuration.policy = self
            .current_routing_policy()?
            .ok_or("routing policy unavailable")?;
        let registry = routing_state::current_registry(self.engine.store(), &self.routing_access())
            .map_err(|e| -> Failure { e.into() })?
            .ok_or("routing registry unavailable")?;
        configuration.catalog = registry.value.catalog;
        self.can_start(binding)?;
        self.ensure_coding_ledger()?;
        let ledger = vcp_budget::ledger(self.engine.store().state(), &binding.scope)?;
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
        let mut escalation = configuration
            .escalation
            .as_ref()
            .map(|policy| self.escalation_evidence(binding, policy))
            .transpose()?;
        if let Some(evidence) = &mut escalation {
            if let Some((previous, _)) = &evidence.trigger {
                evidence.excluded.insert(routing::ModelEndpoint {
                    model: previous.quote.price.model.clone(),
                    endpoint: previous.quote.price.provider.clone(),
                });
            }
        }
        let context_bytes = canonical_bytes(&(parts, schemas))?;
        // Compare actual provider serialization after portable compaction.
        // Use the largest encoding as a conservative common bound; final
        // assembly and atomic admission still check the selected exact bytes.
        let estimated_input = configuration
            .catalog
            .entries
            .iter()
            .filter_map(|candidate| candidate.snapshot.as_ref())
            .filter_map(|snapshot| {
                let envelope = vcp_models::request::envelope(
                    snapshot,
                    self.config.output_ceiling,
                    Units::new(512),
                    now(),
                )
                .ok()?;
                vcp_models::request::encode(parts, &envelope, schemas, snapshot)
                    .ok()
                    .map(|body| body.len() as u64)
            })
            .max()
            .unwrap_or(context_bytes.len() as u64);
        let available = ledger
            .cap
            .get()
            .checked_sub(ledger.settled.get())
            .and_then(|v| v.checked_sub(ledger.active.get()))
            .and_then(|v| v.checked_sub(ledger.unresolved.get()))
            .unwrap_or(0);
        let input = RoutingInput {
            retry_pin: self
                .provider
                .as_ref()
                .and_then(|provider| provider.retries.get(&binding.scope.task))
                .map(|retry| {
                    let attempt: Attempt = self
                        .engine
                        .store()
                        .state()
                        .record(
                            Collection::Attempt,
                            retry.predecessor.as_str(),
                            &binding.scope.workspace,
                        )?
                        .decode()?;
                    Ok::<_, Failure>(routing::ModelEndpoint {
                        model: attempt.quote.price.model,
                        endpoint: attempt.quote.price.provider,
                    })
                })
                .transpose()?,
            excluded: escalation
                .as_ref()
                .map(|e| e.excluded.clone())
                .unwrap_or_default(),
            workspace: binding.scope.workspace.clone(),
            root: task.root,
            task: binding.scope.task.clone(),
            input_revision: task.revision,
            steering: task.steering,
            input_digest: digest_bytes(&context_bytes),
            catalog: configuration.catalog.id.clone(),
            policy: configuration.policy.id.clone(),
            role: binding.role,
            task_class: configuration.task_class.clone(),
            now: now(),
            required_capabilities: ["responses_text_tools".to_string()].into_iter().collect(),
            input_tokens: Units::new(estimated_input),
            output_tokens: self.config.output_ceiling,
            available: Money {
                currency: ledger.currency.clone(),
                micros: Micros::new(available),
            },
            protected_verification: ledger.protected,
            estimates: configuration.estimates.clone(),
        };
        let decision = routing::select(&configuration.catalog, &configuration.policy, &input)?;
        self.capture(
            &binding.scope,
            Channel::Evidence,
            &canonical_bytes(&decision)?,
            "routing-selection/1",
        )?;
        let snapshot = decision
            .selected_snapshot(&configuration.catalog)?
            .ok_or("no qualified model satisfies routing policy, context and budget")?
            .clone();
        if let (Some(policy), Some(evidence)) = (&configuration.escalation, escalation) {
            if let Some((previous, trigger)) = evidence.trigger {
                let current = self.context_revisions(binding)?;
                let barrier = vcp_models::escalation::Barrier {
                    expected: current.clone(),
                    current,
                    state: vcp_models::escalation::SchedulingState::Running,
                    now: input.now,
                    not_before: input.now,
                };
                let previous_model = routing::ModelEndpoint {
                    model: previous.quote.price.model.clone(),
                    endpoint: previous.quote.price.provider.clone(),
                };
                let outcome = vcp_models::escalation::evaluate(
                    policy,
                    &evidence.counters,
                    &trigger,
                    &barrier,
                    previous.id,
                    &previous_model,
                    &decision,
                    &configuration.catalog,
                    &configuration.policy,
                    &ledger,
                    Micros::ZERO,
                )?;
                let vcp_models::escalation::Outcome::Ready { plan } = outcome else {
                    return Err(format!("bounded escalation blocked: {outcome:?}").into());
                };
                self.routing
                    .as_mut()
                    .ok_or("routing configuration missing")?
                    .pending
                    .insert(
                        binding.scope.task.clone(),
                        super::escalation::Pending {
                            plan,
                            handoff: None,
                        },
                    );
            }
        }
        self.routing
            .as_mut()
            .ok_or("routing configuration missing")?
            .prepared
            .insert(binding.scope.task.clone(), decision);
        Ok(snapshot)
    }

    pub(super) fn record_routing_attempt(
        &mut self,
        binding: &ThreadBinding,
        decision: &RoutingDecision,
        request_digest: &str,
        attempt: AttemptId,
    ) -> Result<()> {
        let access = self.routing_access();
        self.runtime
            .block_on(routing_state::record_decision(
                self.engine.store_mut(),
                &access,
                &binding.scope,
                decision,
                request_digest,
                attempt,
                now(),
            ))
            .map_err(|e| -> Failure { e.into() })
    }
}
