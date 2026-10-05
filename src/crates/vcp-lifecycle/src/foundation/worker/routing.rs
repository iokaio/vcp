// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::collections::BTreeMap;
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
    pub rotation_selected: HashMap<TaskId, routing::ModelEndpoint>,
    pub rotation_preview: bool,
    #[cfg(windows)]
    pub pending: HashMap<TaskId, super::escalation::Pending>,
}
impl Context {
    /// Admission and send fences replay the same owner preference boundary used
    /// for selection, rather than treating it as empirical optimizer evidence.
    pub(super) fn revalidate_routing_selection(
        &self,
        decision: &RoutingDecision,
        catalog: &routing::CatalogRevision,
        policy: &routing::Policy,
        available: MonetaryLimit,
        protected: Micros,
    ) -> Result<()> {
        let owner = self
            .routing
            .as_ref()
            .filter(|runtime| !runtime.configuration.owner_assignments.is_empty());
        let Some(owner) = owner else {
            return decision
                .validate_selected_at(catalog, policy, now(), available, protected)
                .map_err(|error| error.into());
        };
        decision.validate()?;
        if let Some(rotation) = owner.configuration.rotation.as_ref().filter(|policy| {
            !available.micros.is_unbounded() && !owner.rotation_preview && !policy.sets(decision.input.role).is_empty()
        }) {
            let selected = decision
                .selected
                .as_ref()
                .ok_or("rotation selected identity missing")?;
            let set = rotation
                .sets(decision.input.role)
                .iter()
                .find(|set| set.members.contains(selected))
                .ok_or("rotation selection outside captured role sets")?;
            let snapshot = catalog
                .snapshot(selected)
                .ok_or("rotation selected snapshot missing")?;
            let reference = vcp_models::rotation::reference_cost(
                snapshot,
                rotation.reference_input_tokens,
                rotation.reference_output_tokens,
            )?;
            if reference.currency != set.max_reference_request_cost.currency
                || set.max_reference_request_cost.micros.exceeds(&reference.micros)
            {
                return Err("rotation selected tariff exceeds captured choice-set ceiling".into());
            }
        }
        let ordered = owner
            .configuration
            .owner_assignments
            .iter()
            .find(|assignment| assignment.role == decision.input.role)
            .map(|assignment| assignment.candidates.as_slice())
            .unwrap_or(&[]);
        let mut input = decision.input.clone();
        input.now = now();
        input.available = available;
        input.protected_verification = protected;
        let refreshed = routing::select_owner_set(catalog, policy, &input, ordered)?;
        if !refreshed.candidates.iter().any(|candidate| {
            Some(&candidate.identity) == decision.selected.as_ref()
                && candidate.exclusions.is_empty()
        }) {
            return Err(
                "selected model is no longer eligible within its owner-selected set".into(),
            );
        }
        Ok(())
    }
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
        if let crate::foundation::routing::Request::DeclareEscalation { declaration } = &request {
            #[cfg(windows)]
            return self.declare_escalation(declaration.clone());
            #[cfg(not(windows))]
            return Err("owner escalation declarations require the Windows coding host".into());
        }
        let access = self.routing_access();
        let ceilings = self.routing_ceilings()?;
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
    pub fn configure_routing(&mut self, mut configuration: Configuration) -> Result<()> {
        if !self.owner_alive || self.authority_pending || self.provider.is_none() {
            return Err("routing requires a configured current provider owner".into());
        }
        configuration
            .validate()
            .map_err(|e| -> Failure { e.into() })?;
        // Reinterpret original endpoint metadata under this owner's explicit
        // financial contract before publishing the effective routing revision.
        let mut sources = BTreeMap::new();
        let mut changed = false;
        for candidate in &mut configuration.catalog.entries {
            if let Some(snapshot) = &mut candidate.snapshot {
                let raw = configuration.raw_catalogs.get(&snapshot.id)
                    .ok_or("routing snapshot original metadata missing")?;
                let effective = snapshot.for_execution(raw.as_bytes(), self.config.cap.micros)?;
                changed |= *snapshot != effective;
                sources.insert(effective.id.clone(), raw.clone());
                *snapshot = effective;
            }
        }
        if changed {
            configuration.catalog = routing::CatalogRevision::create(
                configuration.catalog.parent.clone(), configuration.catalog.observed_at,
                configuration.catalog.effective_at, configuration.catalog.entries,
            )?;
        }
        configuration.raw_catalogs = sources;
        if self.config.cap.micros.is_unbounded() {
            if let Some(rotation) = &mut configuration.rotation {
                for role in &mut rotation.roles {
                    for set in &mut role.sets {
                        set.max_reference_request_cost.micros = vcp_domain::Limit::Unbounded;
                    }
                }
            }
        }
        configuration.validate().map_err(|e| -> Failure { e.into() })?;
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
        if !configuration.owner_assignments.is_empty()
            && previous
                .as_ref()
                .is_none_or(|record| record.value.catalog.id != configuration.catalog.id)
        {
            // Materialized account/task profiles do not own the workspace's
            // publication sequence. Preserve their exact candidates while
            // linking this owner selection to the current canonical revision.
            configuration.catalog = routing::CatalogRevision::create(
                previous
                    .as_ref()
                    .map(|record| record.value.catalog.id.clone()),
                configuration.catalog.observed_at,
                configuration.catalog.effective_at,
                configuration.catalog.entries,
            )?;
        }
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
            rotation_selected: HashMap::new(),
            rotation_preview: false,
            #[cfg(windows)]
            pending: HashMap::new(),
        });
        Ok(())
    }

    pub(super) fn current_routing_policy(&self) -> Result<Option<routing::Policy>> {
        let Some(ceilings) = self.routing_ceilings()? else {
            return Ok(None);
        };
        // Explicit owner role assignments are frozen with the task profile.
        // Workspace optimizer history cannot silently replace a task's selected
        // set when another task changes account or project defaults.
        if self
            .routing
            .as_ref()
            .is_some_and(|runtime| !runtime.configuration.owner_assignments.is_empty())
        {
            return Ok(Some(ceilings));
        }
        let access = self.routing_access();
        let current = routing_state::current_policy(self.engine.store(), &access)
            .map_err(|e| -> Failure { e.into() })?
            .ok_or("canonical routing policy unavailable")?;
        Ok(Some(
            routing_state::effective_policy(current.value, &ceilings)
                .map_err(|e| -> Failure { e.into() })?,
        ))
    }

    pub(super) fn current_routing_catalog(&self) -> Result<Option<routing::CatalogRevision>> {
        if let Some(runtime) = self
            .routing
            .as_ref()
            .filter(|runtime| !runtime.configuration.owner_assignments.is_empty())
        {
            // Canonical publication captures this task's metadata and source
            // bytes. Later workspace catalog heads cannot replace its choices.
            return Ok(Some(runtime.configuration.catalog.clone()));
        }
        routing_state::current_registry(self.engine.store(), &self.routing_access())
            .map(|registry| registry.map(|record| record.value.catalog))
            .map_err(|error| error.into())
    }

    pub(super) fn routing_ceilings(&self) -> Result<Option<routing::Policy>> {
        self.routing
            .as_ref()
            .map(|runtime| {
                let mut policy = runtime.configuration.policy.clone();
                policy.input_tokens = Some(
                    policy
                        .input_tokens
                        .unwrap_or(self.config.input_ceiling)
                        .min(self.config.input_ceiling),
                );
                policy.escalation_limits =
                    runtime.configuration.escalation.as_ref().map(|configured| {
                        let ceiling = routing::EscalationLimits::from_policy(
                            configured,
                            self.config.max_transport_retries,
                        );
                        policy
                            .escalation_limits
                            .as_ref()
                            .map_or_else(|| ceiling.clone(), |selected| selected.clamp(&ceiling))
                    });
                policy.output_tokens = Some(
                    policy
                        .output_tokens
                        .unwrap_or(self.config.output_ceiling)
                        .min(self.config.output_ceiling),
                );
                policy.seal().map_err(|error| -> Failure { error.into() })
            })
            .transpose()
    }

    pub(super) fn current_output_ceiling(&self) -> Result<Units> {
        Ok(self
            .current_routing_policy()?
            .and_then(|policy| policy.output_tokens)
            .unwrap_or(self.config.output_ceiling)
            .min(self.config.output_ceiling))
    }

    pub(super) fn current_input_ceiling(&self) -> Result<Units> {
        Ok(self
            .current_routing_policy()?
            .and_then(|policy| policy.input_tokens)
            .unwrap_or(self.config.input_ceiling)
            .min(self.config.input_ceiling))
    }

    pub(super) fn current_escalation_policy(
        &self,
    ) -> Result<Option<vcp_models::escalation::Policy>> {
        let Some(mut policy) = self
            .routing
            .as_ref()
            .and_then(|runtime| runtime.configuration.escalation.clone())
        else {
            return Ok(None);
        };
        if let Some(limits) = self
            .current_routing_policy()?
            .and_then(|policy| policy.escalation_limits)
        {
            limits.apply(&mut policy);
        }
        policy.max_transport_retries = policy
            .max_transport_retries
            .min(self.config.max_transport_retries);
        policy.validate()?;
        Ok(Some(policy))
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
        if configuration
            .rotation
            .as_ref()
            .is_some_and(|policy| !policy.sets(binding.role).is_empty())
            && !runtime.rotation_preview
            && !runtime.rotation_selected.contains_key(&binding.scope.task)
        {
            return Err("rotation requires coordinated asynchronous route admission".into());
        }
        self.routing
            .as_mut()
            .ok_or("routing configuration missing")?
            .pending
            .remove(&binding.scope.task);
        configuration.policy = self
            .current_routing_policy()?
            .ok_or("routing policy unavailable")?;
        configuration.escalation = self.current_escalation_policy()?;
        configuration.catalog = self
            .current_routing_catalog()?
            .ok_or("routing registry unavailable")?;
        self.can_start(binding)?;
        self.ensure_coding_ledger()?;
        let ledger = vcp_budget::ledger(self.engine.store().current(), &binding.scope)?;
        let task: Task = self
            .engine
            .store()
            .current()
            .record(
                Collection::Task,
                binding.scope.task.as_str(),
                &binding.scope.workspace,
            )?
            .decode()?;
        let mut required_capabilities = routing_state::declarations::admitted_capabilities(
            self.engine.store(),
            &self.routing_access(),
            &binding.scope,
            task.steering,
        )?;
        required_capabilities.insert("responses_text_tools".into());
        let mut escalation = configuration
            .escalation
            .as_ref()
            .map(|policy| self.escalation_evidence(binding, policy))
            .transpose()?;
        if let Some(evidence) = &mut escalation {
            required_capabilities.extend(evidence.required_capabilities.iter().cloned());
            if let Some((previous, _)) = &evidence.trigger {
                evidence.excluded.insert(routing::ModelEndpoint {
                    model: previous.quote.price.model.clone(),
                    endpoint: previous.quote.price.provider.clone(),
                });
            }
        }
        let context_bytes = canonical_bytes(&(parts, schemas))?;
        let sizing_revisions = self.context_revisions(binding)?;
        // Size each candidate with its own codec and output allowance before
        // selection. Smaller qualified candidates do not inherit the primary's
        // larger response ceiling or another candidate's encoding size.
        let candidate_requests: Vec<routing::CandidateRequest> = configuration
            .catalog
            .entries
            .iter()
            .filter_map(|candidate| {
                let snapshot = candidate.snapshot.as_ref()?;
                let allocation = self.coding_request_allocation(binding, snapshot).ok()?;
                let envelope = vcp_models::request::envelope(
                    snapshot,
                    allocation.output_limit,
                    Units::new(512),
                    now(),
                )
                .ok()?;
                vcp_context::selection::assemble_with_input_target(
                    parts.to_vec(),
                    sizing_revisions.clone(),
                    envelope,
                    schemas.clone(),
                    Vec::new(),
                    &vcp_context::selection::Utf8ByteCeiling,
                    Some(allocation.input_target),
                    |selected, envelope, schemas| {
                        vcp_models::request::encode_with_effort(
                            selected,
                            envelope,
                            schemas,
                            snapshot,
                            configuration.policy.reasoning_effort,
                        )
                        .map_err(|_| {
                            vcp_context::manifest::Error::Incompatible("candidate provider codec")
                        })
                    },
                )
                .ok()
                .map(|sealed| routing::CandidateRequest {
                    candidate: candidate.identity.clone(),
                    input_tokens: sealed.manifest.input_estimate,
                    output_tokens: allocation.output_limit,
                })
            })
            .collect();
        let estimated_input = candidate_requests
            .iter()
            .map(|request| request.input_tokens.get())
            .max()
            .ok_or("no candidate request can be encoded")?;
        let output_ceiling = candidate_requests
            .iter()
            .map(|request| request.output_tokens)
            .max()
            .ok_or("no candidate output allocation")?;
        let available = ledger.remaining_before_protected()?;
        if !configuration.owner_assignments.is_empty() {
            for estimate in &mut configuration.estimates {
                if let Some(request) = candidate_requests
                    .iter()
                    .find(|request| request.candidate == estimate.candidate)
                {
                    estimate.first_attempt.input = request.input_tokens;
                    estimate.first_attempt.output = request.output_tokens;
                }
            }
        }
        let rotation_selected = self
            .routing
            .as_ref()
            .and_then(|runtime| runtime.rotation_selected.get(&binding.scope.task))
            .cloned();
        let input = RoutingInput {
            retry_pin: rotation_selected.or(self
                .provider
                .as_ref()
                .and_then(|provider| provider.retries.get(&binding.scope.task))
                .filter(|retry| !retry.switch_owner_model)
                .map(|retry| {
                    let attempt: Attempt = self
                        .engine
                        .store()
                        .current()
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
                .transpose()?),
            excluded: {
                let mut excluded = escalation
                    .as_ref()
                    .map(|e| e.excluded.clone())
                    .unwrap_or_default();
                if let Some(retry) = self
                    .provider
                    .as_ref()
                    .and_then(|provider| provider.retries.get(&binding.scope.task))
                {
                    excluded.extend(retry.owner_excluded.iter().cloned());
                }
                excluded
            },
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
            required_capabilities,
            input_tokens: Units::new(estimated_input),
            output_tokens: output_ceiling,
            candidate_requests,
            available: MonetaryLimit {
                currency: ledger.currency.clone(),
                micros: available,
            },
            protected_verification: ledger.protected,
            estimates: configuration.estimates.clone(),
        };
        let decision = if configuration.owner_assignments.is_empty() {
            routing::select(&configuration.catalog, &configuration.policy, &input)?
        } else {
            let assigned = configuration
                .owner_assignments
                .iter()
                .find(|assignment| assignment.role == input.role)
                .map(|assignment| assignment.candidates.as_slice())
                .unwrap_or(&[]);
            routing::select_owner_set(
                &configuration.catalog,
                &configuration.policy,
                &input,
                assigned,
            )?
        };
        self.capture(
            &binding.scope,
            Channel::Evidence,
            &canonical_bytes(&decision)?,
            "routing-selection/1",
        )?;
        let snapshot = decision
            .selected_snapshot(&configuration.catalog)?
            .ok_or("no eligible model in the selected set satisfies compatibility, context and budget; ask the owner to change the selected set before using an outside model")?
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
