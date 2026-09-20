// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_context::manifest::{Revisions, Sealed, VerifiedContext};
use vcp_models::{catalog::Snapshot, request, stream};
use vcp_repository::Root;

pub(super) struct Provider {
    pub(super) snapshot: Snapshot,
    prepared: HashMap<TaskId, Ready>,
    pub streams: HashMap<AttemptId, stream::Stream>,
    pub timeout: Duration,
    active: HashMap<TaskId, Ready>,
    pub(super) retries: HashMap<TaskId, PendingRetry>,
}
pub(super) struct PendingRetry {
    pub predecessor: AttemptId,
    pub count: u32,
    pub deadline: std::time::Instant,
    not_before: std::time::Instant,
}
struct Ready {
    #[cfg(windows)]
    escalation: Option<super::escalation::Pending>,
    snapshot: Snapshot,
    routing: Option<vcp_models::routing::RoutingDecision>,
    context: VerifiedContext,
    schemas: serde_json::Value,
    roots: Vec<Root>,
    #[cfg(windows)]
    memory: Option<crate::foundation::memory_query::SendFence>,
}
pub(super) struct Prepared {
    #[cfg(windows)]
    pub escalation: Option<super::escalation::Pending>,
    pub body: serde_json::Value,
    pub stream: stream::Stream,
    pub snapshot: Snapshot,
    pub routing: Option<vcp_models::routing::RoutingDecision>,
}

impl Context {
    pub fn configure_provider(
        &mut self,
        snapshot: Snapshot,
        raw: Vec<u8>,
        timeout: Duration,
    ) -> Result<()> {
        if !self.owner_alive || self.authority_pending {
            return Err("provider configuration waits for current authority owner".into());
        }
        #[cfg(windows)]
        if !self.coding.is_empty() {
            return Err("provider refresh requires fresh coding owner setup".into());
        }
        if timeout.is_zero() || timeout > Duration::from_secs(120) {
            return Err("provider deadline must be within 120 seconds".into());
        }
        snapshot.current(now())?;
        if snapshot
            != Snapshot::from_endpoints(
                &raw,
                snapshot.observed_at,
                snapshot.valid_until,
                snapshot.compatibility.clone(),
            )?
        {
            return Err("provider snapshot differs from captured endpoint catalog".into());
        }
        if !self.streams.is_empty()
            || self
                .provider
                .as_ref()
                .is_some_and(|p| !p.retries.is_empty())
        {
            return Err("provider refresh waits for active attempts".into());
        }
        if snapshot.price.currency != self.config.cap.currency
            || self.config.output_ceiling > snapshot.max_output
        {
            return Err("provider currency/output differs from host ceiling".into());
        }
        let scope = Scope {
            workspace: self.config.workspace.clone(),
            session: self.config.session.clone(),
            task: self.config.root_task.clone(),
        };
        // Publication of this marker prevents reopen from falling back to the
        // private P1 synthetic codec before explicit provider reconfiguration.
        let catalog = self.capture(&scope, Channel::Evidence, &raw, "openrouter-endpoints/1")?;
        self.capture(&scope,Channel::Evidence,&canonical_bytes(&serde_json::json!({"snapshot":snapshot,"catalog_artifact":catalog.spec.id,"timeout_ms":timeout.as_millis()}))?,"openrouter-provider-configuration/1")?;
        self.config.price = snapshot.price.clone();
        self.provider = Some(Provider {
            snapshot,
            prepared: HashMap::new(),
            streams: HashMap::new(),
            timeout,
            active: HashMap::new(),
            retries: HashMap::new(),
        });
        self.provider_required = true;
        Ok(())
    }
    pub fn context_revisions(&self, binding: &ThreadBinding) -> Result<Revisions> {
        self.can_start(binding)?;
        let state = self.engine.store().state();
        let workspace: Workspace = state
            .record(
                Collection::Workspace,
                self.config.workspace.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        let task: Task = state
            .record(
                Collection::Task,
                binding.scope.task.as_str(),
                &binding.scope.workspace,
            )?
            .decode()?;
        // Tool authority and money admission are independent revisions. Budget
        // admission rechecks its own ledger policy later in the same worker.
        let policy = vcp_engine::policy::optional(state, &binding.scope.workspace)?
            .map_or(PolicyRevision::ZERO, |policy| policy.revision);
        Ok(Revisions {
            scope: binding.scope.clone(),
            steering: task.steering,
            policy,
            authority: workspace.authority,
            deletion: workspace.deletion,
            binding: workspace.binding.revision,
            // Skills/memory are not enabled in the P2 scaffold. Nonzero versions
            // need their owning canonical adapters rather than trusted prose.
            instructions: Revision::ZERO,
            tools: Revision::ZERO,
            skills: Revision::ZERO,
            memory: Revision::ZERO,
            task_state: task.revision,
        })
    }
    fn verify_context(&self, sealed: Sealed) -> Result<VerifiedContext> {
        let verified = sealed.verify_captures(|scope, id, limit| {
            let read = || -> Result<(ArtifactDescriptor, Vec<u8>)> {
                let descriptor: ArtifactDescriptor = self
                    .engine
                    .store()
                    .state()
                    .record(Collection::Artifact, id.as_str(), &scope.workspace)?
                    .decode()?;
                if descriptor.length.get() != limit || &descriptor.spec.scope != scope {
                    return Err("captured source scope/size differs".into());
                }
                let mut bytes = Vec::new();
                vcp_audit::history::History::read_artifact(
                    self.engine.store(),
                    &self.history_access(),
                    id,
                    &mut bytes,
                )?;
                Ok((descriptor, bytes))
            };
            read().map_err(|_| vcp_context::manifest::Error::Stale)
        })?;
        Ok(verified)
    }
    fn validate_ready(&self, binding: &ThreadBinding, ready: &Ready) -> Result<()> {
        #[cfg(windows)]
        self.validate_continuity_ready(binding)?;
        self.validate_ready_context(binding, ready)
    }
    fn validate_retry_sources(&self, binding: &ThreadBinding, ready: &Ready) -> Result<()> {
        // Reservation/failure accounting makes the prior dynamic facts stale.
        // Fence sources and authority here; coding admission reassembles current
        // facts and a fresh handoff before the next reservation is created.
        #[cfg(windows)]
        self.validate_continuity_sources(binding)?;
        self.validate_ready_context(binding, ready)
    }
    fn validate_ready_context(&self, binding: &ThreadBinding, ready: &Ready) -> Result<()> {
        if let Some(decision) = &ready.routing {
            let current_catalog = crate::foundation::routing_state::current_registry(
                self.engine.store(),
                &self.routing_access(),
            )
            .map_err(|e| -> Failure { e.into() })?;
            if self
                .current_routing_policy()?
                .as_ref()
                .is_none_or(|policy| policy.id != decision.input.policy)
                || current_catalog
                    .as_ref()
                    .is_none_or(|registry| registry.value.catalog.id != decision.input.catalog)
                || decision.selected.as_ref().is_none_or(|selected| {
                    selected.model != ready.snapshot.compatibility.model
                        || selected.endpoint != ready.snapshot.compatibility.endpoint
                })
            {
                return Err("routing policy or selected endpoint changed before admission".into());
            }
            let catalog = &current_catalog
                .ok_or("routing catalog unavailable")?
                .value
                .catalog;
            let policy = self
                .current_routing_policy()?
                .ok_or("routing policy unavailable")?;
            let ledger = vcp_budget::ledger(self.engine.store().state(), &binding.scope)?;
            let available = Money {
                currency: ledger.currency.clone(),
                micros: Micros::new(
                    ledger
                        .cap
                        .get()
                        .saturating_sub(ledger.settled.get())
                        .saturating_sub(ledger.active.get())
                        .saturating_sub(ledger.unresolved.get()),
                ),
            };
            decision.validate_selected_at(catalog, &policy, now(), available, ledger.protected)?;
        }
        #[cfg(windows)]
        self.instruction_parents(binding)?;
        #[cfg(windows)]
        if let Some(memory) = &ready.memory {
            self.validate_memory_context(binding, memory, ready.context.sealed())?;
        }
        let current = self.context_revisions(binding)?;
        let sealed = ready.context.sealed();
        sealed.revalidate(
            &current,
            &sealed.manifest.envelope,
            &ready.schemas,
            |manifest| {
                if vcp_repository::instructions::revalidate_probes(
                    &manifest.instruction_probes,
                    &ready.roots,
                )
                .is_err()
                {
                    return false;
                }
                manifest.included.iter().all(|part| {
                    part.file.as_ref().is_none_or(|file| {
                        ready
                            .roots
                            .iter()
                            .find(|r| r.identity.root == file.root)
                            .is_some_and(|r| {
                                r.identity.workspace == binding.scope.workspace
                                    && r.revalidate(file).is_ok()
                            })
                    })
                })
            },
        )?;
        request::validate_sealed(&ready.context, &ready.snapshot, &ready.schemas, now())?;
        if sealed.manifest.envelope.output != self.config.output_ceiling {
            return Err("sealed output differs from host ceiling".into());
        }
        Ok(())
    }
    pub fn prepare_context(
        &mut self,
        binding: &ThreadBinding,
        sealed: Sealed,
        schemas: serde_json::Value,
        roots: Vec<Root>,
    ) -> Result<()> {
        self.prepare_context_checked(
            binding,
            sealed,
            schemas,
            roots,
            #[cfg(windows)]
            None,
            None,
        )
    }
    #[cfg(windows)]
    pub fn prepare_context_with_memory(
        &mut self,
        binding: &ThreadBinding,
        sealed: Sealed,
        schemas: serde_json::Value,
        roots: Vec<Root>,
        memory: Option<crate::foundation::memory_query::SendFence>,
    ) -> Result<()> {
        self.prepare_context_checked(binding, sealed, schemas, roots, memory, None)
    }
    #[cfg(windows)]
    pub(super) fn prepare_routed_context(
        &mut self,
        binding: &ThreadBinding,
        sealed: Sealed,
        schemas: serde_json::Value,
        roots: Vec<Root>,
        snapshot: Snapshot,
    ) -> Result<()> {
        self.prepare_context_checked(binding, sealed, schemas, roots, None, Some(snapshot))
    }
    fn prepare_context_checked(
        &mut self,
        binding: &ThreadBinding,
        sealed: Sealed,
        schemas: serde_json::Value,
        roots: Vec<Root>,
        #[cfg(windows)] memory: Option<crate::foundation::memory_query::SendFence>,
        snapshot: Option<Snapshot>,
    ) -> Result<()> {
        self.require_configured_routing()?;
        #[cfg(windows)]
        for part in &sealed.manifest.included {
            let descriptor: ArtifactDescriptor = self
                .engine
                .store()
                .state()
                .record(
                    Collection::Artifact,
                    part.artifact.as_str(),
                    &binding.scope.workspace,
                )?
                .decode()?;
            if descriptor.spec.schema == "memory-context/1"
                && memory.as_ref().is_none_or(|f| &f.part != part)
            {
                return Err("memory context requires its opaque source capability".into());
            }
        }
        if roots.len() > 33 {
            return Err("too many registered source roots".into());
        }
        let canonical_root = std::path::Path::new(&self.config.binding.root).canonicalize()?;
        let mut seen = std::collections::BTreeSet::new();
        for root in &roots {
            if root.identity.workspace != binding.scope.workspace
                || root.identity.binding != self.config.binding.revision
                || !seen.insert(root.identity.root.clone())
                || (root.path() != canonical_root && !canonical_root.starts_with(root.path()))
            {
                return Err("context root identity differs from canonical binding".into());
            }
        }
        for part in &sealed.manifest.included {
            if let Some(file) = &part.file {
                let root = roots
                    .iter()
                    .find(|root| root.identity.root == file.root)
                    .ok_or("file source has no registered root")?;
                if root.path() != canonical_root
                    && (file.path != "AGENTS.md"
                        || part.kind != vcp_context::manifest::Kind::ProjectInstruction)
                {
                    return Err("parent root grants instruction reads only".into());
                }
            }
        }
        let ready = Ready {
            #[cfg(windows)]
            escalation: self
                .routing
                .as_mut()
                .and_then(|runtime| runtime.pending.remove(&binding.scope.task)),
            snapshot: snapshot.unwrap_or(
                self.provider
                    .as_ref()
                    .ok_or("OpenRouter configuration required")?
                    .snapshot
                    .clone(),
            ),
            routing: self
                .routing
                .as_mut()
                .and_then(|runtime| runtime.prepared.remove(&binding.scope.task)),
            context: self.verify_context(sealed)?,
            schemas,
            roots,
            #[cfg(windows)]
            memory,
        };
        if self.routing.is_some() && ready.routing.is_none() {
            return Err(
                "automatic routing requires a current qualified selection for this task".into(),
            );
        }
        #[cfg(windows)]
        if ready
            .escalation
            .as_ref()
            .is_some_and(|pending| pending.handoff.is_none())
        {
            return Err("escalation requires a captured validated handoff".into());
        }
        self.validate_ready(binding, &ready)?;
        let provider = self
            .provider
            .as_mut()
            .ok_or("OpenRouter configuration required")?;
        if provider.prepared.len() >= 256 || provider.prepared.contains_key(&binding.scope.task) {
            return Err("context already prepared or queue full".into());
        }
        provider.prepared.insert(binding.scope.task.clone(), ready);
        Ok(())
    }
    pub(super) fn admit_context(
        &mut self,
        binding: &ThreadBinding,
        retained: &serde_json::Value,
    ) -> Result<Prepared> {
        #[cfg(windows)]
        if let Err(error) = self.check_coding_bounds() {
            self.pause_root("canonical root coding limit requires attention")?;
            return Err(error);
        }
        #[cfg(windows)]
        if self.coding.contains_key(&binding.scope.task) {
            if let Err(error) = self.assemble_coding_context(binding) {
                self.pause_root("canonical coding context or request limit requires attention")?;
                return Err(error);
            }
        }
        let provider = self
            .provider
            .as_mut()
            .ok_or("OpenRouter configuration required after reopen")?;
        #[cfg(windows)]
        let reassembled = self.coding.contains_key(&binding.scope.task);
        #[cfg(not(windows))]
        let reassembled = false;
        let ready = if let Some(retry) = provider.retries.get(&binding.scope.task) {
            let now = std::time::Instant::now();
            if now < retry.not_before || now >= retry.deadline {
                return Err("retry timer is not eligible".into());
            }
            if reassembled {
                provider.prepared.remove(&binding.scope.task)
            } else {
                provider.active.remove(&binding.scope.task)
            }
        } else {
            provider.prepared.remove(&binding.scope.task)
        }
        .ok_or("fresh sealed context required for every retained attempt")?;
        self.validate_ready(binding, &ready)?;
        let ready = Ready {
            #[cfg(windows)]
            escalation: ready.escalation,
            snapshot: ready.snapshot,
            routing: ready.routing,
            context: self.verify_context(ready.context.into_sealed())?,
            schemas: ready.schemas,
            roots: ready.roots,
            #[cfg(windows)]
            memory: ready.memory,
        };
        if ready.routing.is_none()
            && retained["model"].as_str() != Some(&ready.context.sealed().manifest.envelope.model)
        {
            return Err("retained request model differs from seal".into());
        }
        let sealed = ready.context.sealed();
        let body = serde_json::from_slice(sealed.body())?;
        let stream = stream::Stream::new(request::Tools::parse(&ready.schemas)?);
        self.capture(
            &binding.scope,
            Channel::Evidence,
            &canonical_bytes(&sealed.manifest)?,
            "context-manifest/1",
        )?;
        // Recheck after durable capture, ordered with all canonical commands.
        self.validate_ready(binding, &ready)?;
        let snapshot = ready.snapshot.clone();
        let routing = ready.routing.clone();
        #[cfg(windows)]
        let escalation = ready.escalation.clone();
        self.provider
            .as_mut()
            .ok_or("provider configuration missing")?
            .active
            .insert(binding.scope.task.clone(), ready);
        Ok(Prepared {
            body,
            stream,
            snapshot,
            routing,
            #[cfg(windows)]
            escalation,
        })
    }
    #[cfg(windows)]
    pub(super) fn validate_memory_send(&self, binding: &ThreadBinding) -> Result<()> {
        if let Some(ready) = self
            .provider
            .as_ref()
            .and_then(|p| p.active.get(&binding.scope.task))
        {
            self.require_configured_routing()?;
            if let Some(decision) = &ready.routing {
                let policy = self
                    .current_routing_policy()?
                    .ok_or("routing configuration missing at send fence")?;
                let registry = crate::foundation::routing_state::current_registry(
                    self.engine.store(),
                    &self.routing_access(),
                )
                .map_err(|e| -> Failure { e.into() })?
                .ok_or("routing registry missing at send fence")?;
                if policy.id != decision.input.policy
                    || registry.value.catalog.id != decision.input.catalog
                {
                    return Err("routing policy or catalog changed at send fence".into());
                }
                ready.snapshot.current(now())?;
                if decision
                    .selected_snapshot(&registry.value.catalog)?
                    .is_none_or(|snapshot| snapshot != &ready.snapshot)
                {
                    return Err("routing snapshot changed at send fence".into());
                }
                // The exact reservation is already active. Recheck expiring
                // evidence using the recorded pre-reservation allocation;
                // atomic budget admission owns the current money check.
                decision.validate_selected_at(
                    &registry.value.catalog,
                    &policy,
                    now(),
                    decision.input.available.clone(),
                    decision.input.protected_verification,
                )?;
            }
            if let Some(memory) = &ready.memory {
                self.validate_memory_context(binding, memory, ready.context.sealed())?;
            }
        }
        Ok(())
    }
    pub fn schedule_retry(
        &mut self,
        binding: &ThreadBinding,
        attempt: AttemptId,
        count: u32,
        deadline: std::time::Instant,
        failure: vcp_models::retry::Failure,
        retry_after_ms: Option<u64>,
    ) -> Result<Option<Duration>> {
        self.can_start(binding)?;
        let Some(provider) = self.provider.as_ref() else {
            return Ok(None);
        };
        if provider.retries.contains_key(&binding.scope.task) {
            return Err("retry already scheduled".into());
        }
        if let Some(policy) = self
            .routing
            .as_ref()
            .and_then(|routing| routing.configuration.escalation.as_ref())
        {
            let admissions = crate::foundation::routing_state::admitted_escalations(
                self.engine.store(),
                &self.routing_access(),
                &binding.scope,
            )
            .map_err(|e| -> Failure { e.into() })?;
            let switched: std::collections::BTreeSet<_> = admissions
                .iter()
                .filter(|record| {
                    record.plan.trigger.class() != vcp_models::escalation::Class::TransportRetry
                })
                .map(|record| &record.attempt)
                .collect();
            let attempts: Vec<Attempt> = self
                .engine
                .store()
                .state()
                .records
                .values()
                .filter(|r| r.collection == Collection::Attempt)
                .map(Record::decode)
                .collect::<std::result::Result<_, _>>()?;
            let root: Vec<_> = attempts
                .iter()
                .filter(|a| {
                    a.root == self.config.root_task && a.scope.workspace == binding.scope.workspace
                })
                .collect();
            let retries = root
                .iter()
                .filter(|a| a.previous.is_some() && !switched.contains(&a.id))
                .count();
            if retries >= policy.max_transport_retries as usize
                || root.len() >= policy.max_total_attempts as usize
                || now() >= policy.deadline
            {
                return Ok(None);
            }
        }
        self.validate_retry_sources(
            binding,
            provider
                .active
                .get(&binding.scope.task)
                .ok_or("retry source context missing")?,
        )?;
        let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) else {
            return Ok(None);
        };
        let now = now();
        let policy = vcp_models::retry::Policy {
            max_retries: 2,
            base_delay_ms: 100,
            max_delay_ms: 5_000,
            deadline: Timestamp::new(now.get().saturating_add(remaining.as_millis() as u64)),
        };
        let Some(retry) = policy.next(
            attempt.clone(),
            count,
            now,
            failure,
            true,
            retry_after_ms,
            true,
        )?
        else {
            return Ok(None);
        };
        let delay = Duration::from_millis(retry.not_before.get() - now.get());
        self.retain_unknown(
            binding,
            &attempt,
            "provider failure retained before bounded retry",
            false,
        )?;
        self.capture(
            &binding.scope,
            Channel::Evidence,
            &canonical_bytes(&serde_json::json!({
                "predecessor": attempt, "retry": count + 1, "failure": failure,
                "not_before": retry.not_before, "deadline": policy.deadline,
                "prior_liability_unresolved": true
            }))?,
            "provider-retry/1",
        )?;
        self.provider
            .as_mut()
            .ok_or("provider configuration missing")?
            .retries
            .insert(
                binding.scope.task.clone(),
                PendingRetry {
                    predecessor: attempt,
                    count: count + 1,
                    deadline,
                    not_before: std::time::Instant::now() + delay,
                },
            );
        Ok(Some(delay))
    }
    pub fn retry_current(&self, binding: &ThreadBinding, attempt: &AttemptId) -> Result<()> {
        self.can_start(binding)?;
        let provider = self
            .provider
            .as_ref()
            .ok_or("provider configuration missing")?;
        let retry = provider
            .retries
            .get(&binding.scope.task)
            .ok_or("retry cancelled")?;
        if &retry.predecessor != attempt || std::time::Instant::now() >= retry.deadline {
            return Err("retry predecessor/deadline changed".into());
        }
        self.validate_retry_sources(
            binding,
            provider
                .active
                .get(&binding.scope.task)
                .ok_or("retry source context missing")?,
        )
    }
    pub fn cancel_retry(&mut self, binding: &ThreadBinding, attempt: &AttemptId) -> Result<()> {
        if self.provider.as_ref().is_some_and(|p| {
            p.retries
                .get(&binding.scope.task)
                .is_some_and(|r| &r.predecessor == attempt)
        }) {
            let provider = self
                .provider
                .as_mut()
                .ok_or("provider configuration missing")?;
            provider.retries.remove(&binding.scope.task);
            provider.active.remove(&binding.scope.task);
            self.pause_root("provider retry cancelled; prior liability retained")?;
        }
        Ok(())
    }
    pub(super) fn complete_provider(
        &mut self,
        binding: &ThreadBinding,
        attempt: &AttemptId,
        response_id: &str,
    ) -> Result<()> {
        if let Some(provider) = self.provider.as_mut() {
            provider.active.remove(&binding.scope.task);
        }
        let parser = self
            .provider
            .as_mut()
            .ok_or("provider configuration missing")?
            .streams
            .remove(attempt)
            .ok_or("provider response missing")?;
        let normalized = parser.finish()?;
        if normalized.response_id != response_id {
            return Err("retained/normalized response identity differs".into());
        }
        let mut writer = self
            .streams
            .remove(attempt)
            .ok_or("response capture missing")?;
        let descriptor = writer.finalize()?;
        drop(writer);
        self.command(
            Command::AttachArtifact {
                descriptor: descriptor.clone(),
            },
            Some(binding.scope.task.clone()),
            Revision::ZERO,
        )?;
        let normalized_capture = self.capture(
            &binding.scope,
            Channel::Evidence,
            &canonical_bytes(&normalized)?,
            "openrouter-normalized-response/1",
        )?;
        let Some(amount) = normalized
            .usage
            .as_ref()
            .and_then(|usage| usage.cost.clone())
        else {
            return self.unknown(binding, attempt, "provider response omitted observed cost");
        };
        let actor = self.actor();
        self.runtime.block_on(vcp_budget::observe(
            self.engine.store_mut(),
            UsageObservation {
                id: ObservationId::new(),
                scope: binding.scope.clone(),
                attempt: attempt.clone(),
                provider_request: response_id.into(),
                mode: UsageMode::Cumulative {
                    version: Units::new(1),
                },
                amount,
                final_usage: true,
                raw: descriptor.spec.id.clone(),
                correction: None,
            },
            &actor,
        ))?;
        let admitted = vcp_budget::attempt(
            self.engine.store().state(),
            attempt,
            &binding.scope.workspace,
        )?;
        if normalized
            .served_model
            .as_ref()
            .is_some_and(|model| model != &admitted.quote.price.model)
        {
            self.pause_root("observed served model differs from admitted model pin")?;
        }
        #[cfg(windows)]
        if self.coding.contains_key(&binding.scope.task) {
            self.complete_coding_response(
                binding,
                attempt,
                normalized,
                vec![descriptor.spec.id, normalized_capture.spec.id],
            )?;
        }
        Ok(())
    }
}
