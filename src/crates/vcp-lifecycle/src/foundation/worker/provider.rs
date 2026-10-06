// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_context::manifest::{Revisions, Sealed, VerifiedContext};
use vcp_models::{catalog::Snapshot, request, stream};
use vcp_protocol::digest_bytes;
use vcp_repository::Root;

pub(super) struct Provider {
    pub(super) snapshot: Snapshot,
    prepared: HashMap<TaskId, Ready>,
    pub streams: HashMap<AttemptId, stream::Stream>,
    pub timeout: Duration,
    active: HashMap<TaskId, Ready>,
    pub(super) retries: HashMap<TaskId, PendingRetry>,
    pub(super) error_sources: HashMap<AttemptId, vcp_models::retry::LimitSource>,
    pub(super) failures: HashMap<AttemptId, vcp_models::retry::ProviderFailure>,
    queued_deadlines: HashMap<TaskId, std::time::Instant>,
}
pub(super) struct PendingRetry {
    pub predecessor: AttemptId,
    pub count: u32,
    pub deadline: Option<std::time::Instant>,
    not_before: std::time::Instant,
    /// Owner-approved failover is distinct from an ordinary pinned retry.
    pub switch_owner_model: bool,
    pub owner_excluded: std::collections::BTreeSet<vcp_models::routing::ModelEndpoint>,
}
struct Ready {
    #[cfg(windows)]
    escalation: Option<super::escalation::Pending>,
    snapshot: Snapshot,
    routing: Option<vcp_models::routing::RoutingDecision>,
    context: Arc<VerifiedContext>,
    schemas: serde_json::Value,
    roots: Vec<Root>,
    #[cfg(windows)]
    memory: Option<crate::foundation::memory_query::SendFence>,
}
pub(super) struct Prepared {
    /// Copied from the validated sealed request; never rewritten after admission.
    pub output_ceiling: Units,
    #[cfg(windows)]
    pub escalation: Option<super::escalation::Pending>,
    pub body: serde_json::Value,
    pub stream: stream::Stream,
    pub snapshot: Snapshot,
    pub routing: Option<vcp_models::routing::RoutingDecision>,
}

impl Context {
    pub(super) fn provider_queue_deadline(
        &self,
        binding: &ThreadBinding,
    ) -> Option<std::time::Instant> {
        self.provider
            .as_ref()
            .and_then(|provider| provider.queued_deadlines.get(&binding.scope.task).copied())
    }
    pub(super) fn provider_queue_current(&self, binding: &ThreadBinding) -> Result<()> {
        if self
            .provider_queue_deadline(binding)
            .is_some_and(|deadline| std::time::Instant::now() >= deadline)
        {
            return Err("provider admission deadline expired before submission".into());
        }
        Ok(())
    }
    pub(in crate::foundation) fn set_provider_queue_deadline(
        &mut self,
        binding: &ThreadBinding,
        deadline: Option<std::time::Instant>,
    ) -> Result<()> {
        if let Some(provider) = self.provider.as_mut() {
            if let Some(deadline) = deadline {
                // Admission runs serially on this worker. Completed admissions
                // retain their original deadline in the transport permit; only
                // this binding needs its worker fence through final submission.
                let now = std::time::Instant::now();
                provider
                    .queued_deadlines
                    .retain(|task, existing| task == &binding.scope.task || *existing > now);
                if provider.queued_deadlines.len() >= 256
                    && !provider.queued_deadlines.contains_key(&binding.scope.task)
                {
                    return Err("provider admission deadline queue full".into());
                }
                provider
                    .queued_deadlines
                    .insert(binding.scope.task.clone(), deadline);
            } else {
                provider.queued_deadlines.remove(&binding.scope.task);
            }
        }
        self.provider_queue_current(binding)
    }
    #[cfg(windows)]
    pub(in crate::foundation) fn provider_queue_remaining(
        &self,
        binding: &ThreadBinding,
    ) -> Result<Option<Duration>> {
        self.can_start(binding)?;
        let timeout = self
            .provider
            .as_ref()
            .ok_or("provider queue configuration missing")?
            .timeout;
        if self.execution_deadline == Some(vcp_domain::Limit::Unbounded) {
            return Ok(self.coding_remaining());
        }
        Ok(Some(
            self.coding_remaining()
                .map_or(timeout, |remaining| remaining.min(timeout)),
        ))
    }
    #[cfg(windows)]
    pub(in crate::foundation) fn prepare_rotation_routes(
        &mut self,
        binding: &ThreadBinding,
    ) -> Result<Option<crate::foundation::provider_pacing::Routes>> {
        let Some(policy) = self
            .routing
            .as_ref()
            .and_then(|runtime| runtime.configuration.rotation.clone())
        else {
            return Ok(None);
        };
        if policy.sets(binding.role).is_empty() {
            return Ok(None);
        }
        if !self.coding.contains_key(&binding.scope.task) {
            return Err("rotation requires the canonical portable coding context".into());
        }
        self.routing
            .as_mut()
            .ok_or("rotation configuration missing")?
            .rotation_selected
            .remove(&binding.scope.task);
        // Assemble once without a billable attempt to obtain the complete
        // context/capability/budget exclusions; final admission reassembles.
        self.routing
            .as_mut()
            .ok_or("rotation configuration missing")?
            .rotation_preview = true;
        let assembled = self.assemble_coding_context(binding);
        self.routing
            .as_mut()
            .ok_or("rotation configuration missing")?
            .rotation_preview = false;
        assembled?;
        let ready = self
            .provider
            .as_mut()
            .ok_or("rotation provider missing")?
            .prepared
            .remove(&binding.scope.task)
            .ok_or("rotation preview context missing")?;
        let decision = ready.routing.ok_or("rotation preview decision missing")?;
        let catalog = self
            .current_routing_catalog()?
            .ok_or("rotation catalog missing")?;
        let sets = policy
            .sets(binding.role)
            .iter()
            .map(|set| {
                set.members
                    .iter()
                    .filter(|member| {
                        decision.candidates.iter().any(|candidate| {
                            &candidate.identity == *member && candidate.exclusions.is_empty()
                        }) && catalog.snapshot(member).is_some_and(|snapshot| {
                            self.config.cap.micros.is_unbounded() || vcp_models::rotation::reference_cost(
                                snapshot,
                                policy.reference_input_tokens,
                                policy.reference_output_tokens,
                            )
                            .is_ok_and(|cost| {
                                cost.currency == set.max_reference_request_cost.currency
                                    && !set.max_reference_request_cost.micros.exceeds(&cost.micros)
                            })
                        })
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        if sets.iter().all(Vec::is_empty) {
            return Err("no approved rotation member satisfies context, privacy, price and remaining budget".into());
        }
        let key = digest_bytes(&canonical_bytes(&(policy, binding.role))?);
        Ok(Some(crate::foundation::provider_pacing::Routes {
            policy_key: key,
            sets,
            estimated_tokens: decision
                .input
                .input_tokens
                .get()
                .saturating_add(decision.input.output_tokens.get()),
            waiter_id: WorkspaceId::new().to_string(),
            queued_at_ms: now().get(),
            deadline: None,
        }))
    }
    #[cfg(windows)]
    pub(in crate::foundation) fn select_rotation_route(
        &mut self,
        binding: &ThreadBinding,
        selection: crate::foundation::provider_pacing::Selection,
    ) -> Result<()> {
        self.can_start(binding)?;
        let route = selection.selected.clone();
        let runtime = self.routing.as_mut().ok_or("rotation runtime missing")?;
        let policy = runtime
            .configuration
            .rotation
            .as_ref()
            .ok_or("rotation policy missing")?;
        if !policy
            .sets(binding.role)
            .iter()
            .any(|set| set.members.contains(&route))
        {
            return Err("rotation selection outside captured role sets".into());
        }
        runtime
            .rotation_selected
            .insert(binding.scope.task.clone(), route);
        self.capture(
            &binding.scope,
            Channel::Evidence,
            &canonical_bytes(&selection)?,
            "provider-rotation/1",
        )?;
        Ok(())
    }
    pub(in crate::foundation) fn provider_limit_source(
        &self,
        attempt: &AttemptId,
    ) -> Option<vcp_models::retry::LimitSource> {
        self.provider
            .as_ref()
            .and_then(|provider| provider.error_sources.get(attempt).copied())
    }
    pub(in crate::foundation) fn provider_attempt_endpoint(
        &self,
        binding: &ThreadBinding,
        id: &AttemptId,
    ) -> Result<vcp_models::routing::ModelEndpoint> {
        let attempt: Attempt = self
            .engine
            .store()
            .current()
            .record(Collection::Attempt, id.as_str(), &binding.scope.workspace)?
            .decode()?;
        if attempt.scope != binding.scope {
            return Err("provider failure attempt scope differs".into());
        }
        Ok(vcp_models::routing::ModelEndpoint {
            model: attempt.quote.price.model,
            endpoint: attempt.quote.price.provider,
        })
    }
    #[cfg(windows)]
    pub(super) fn decision_source(
        &self,
        binding: &ThreadBinding,
    ) -> Result<super::decision::Source> {
        let ready = self
            .provider
            .as_ref()
            .and_then(|provider| provider.active.get(&binding.scope.task))
            .ok_or("actual admitted context missing")?;
        self.reverify_context(&ready.context)?;
        Ok(super::decision::Source {
            context: ready.context.clone(),
            roots: ready.roots.clone(),
            memory: ready.memory.clone(),
            escalation: ready
                .escalation
                .as_ref()
                .map(|pending| pending.plan.clone()),
            baseline: ready
                .routing
                .clone()
                .ok_or("deterministic routing decision missing")?,
        })
    }
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
        if timeout.is_zero() || timeout > Duration::from_secs(360) {
            return Err("provider deadline must be within 360 seconds".into());
        }
        snapshot.current(now())?;
        if snapshot != snapshot.rebuild_captured(&raw)?
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
        if snapshot.price.currency != self.config.cap.currency {
            return Err("provider currency differs from host ledger".into());
        }
        let snapshot = snapshot.for_execution(&raw, self.config.cap.micros)?;
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
            error_sources: HashMap::new(),
            failures: HashMap::new(),
            queued_deadlines: HashMap::new(),
        });
        self.provider_required = true;
        Ok(())
    }
    pub fn context_revisions(&self, binding: &ThreadBinding) -> Result<Revisions> {
        self.can_start(binding)?;
        let state = self.engine.store().current();
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
        #[cfg(windows)]
        let skills = self.skill_revision(&binding.scope)?;
        #[cfg(not(windows))]
        let skills = Revision::ZERO;
        Ok(Revisions {
            scope: binding.scope.clone(),
            steering: task.steering,
            policy,
            authority: workspace.authority,
            deletion: workspace.deletion,
            binding: workspace.binding.revision,
            // Skill revisions come from canonical activation state. File and
            // memory dependencies additionally use their owning send fences.
            instructions: Revision::ZERO,
            tools: Revision::ZERO,
            skills,
            memory: Revision::ZERO,
            task_state: task.revision,
        })
    }
    pub(in crate::foundation) fn verify_context(&self, sealed: Sealed) -> Result<VerifiedContext> {
        Ok(sealed
            .verify_captures(|scope, id, limit| self.resolve_context_capture(scope, id, limit))?)
    }
    pub(in crate::foundation) fn reverify_context(&self, context: &VerifiedContext) -> Result<()> {
        Ok(context
            .reverify_captures(|scope, id, limit| self.resolve_context_capture(scope, id, limit))?)
    }
    fn resolve_context_capture(
        &self,
        scope: &Scope,
        id: &ArtifactId,
        limit: u64,
    ) -> vcp_context::manifest::Result<(ArtifactDescriptor, Vec<u8>)> {
        let read = || -> Result<(ArtifactDescriptor, Vec<u8>)> {
            let descriptor: ArtifactDescriptor = self
                .engine
                .store()
                .current()
                .record(Collection::Artifact, id.as_str(), &scope.workspace)?
                .decode()?;
            if descriptor.length.get() != limit || &descriptor.spec.scope != scope {
                return Err("captured source scope/size differs".into());
            }
            let mut bytes = Vec::new();
            self.runtime
                .block_on(vcp_audit::history::History::read_artifact(
                    self.engine.store(),
                    &self.history_access(),
                    id,
                    &mut bytes,
                ))?;
            Ok((descriptor, bytes))
        };
        read().map_err(|_| vcp_context::manifest::Error::Stale)
    }
    fn validate_ready(&self, binding: &ThreadBinding, ready: &Ready) -> Result<()> {
        #[cfg(windows)]
        self.validate_continuity_ready(binding)?;
        self.validate_ready_context(binding, ready, true)
    }
    fn validate_retry_sources(&self, binding: &ThreadBinding, ready: &Ready) -> Result<()> {
        // Reservation/failure accounting makes the prior dynamic facts stale.
        // Fence sources and authority here; coding admission reassembles current
        // facts and a fresh handoff before the next reservation is created.
        #[cfg(windows)]
        self.validate_continuity_sources(binding)?;
        self.validate_ready_context(binding, ready, false)
    }
    fn validate_ready_context(
        &self,
        binding: &ThreadBinding,
        ready: &Ready,
        admitting: bool,
    ) -> Result<()> {
        #[cfg(windows)]
        self.child_model_scope(binding, &ready.snapshot.compatibility.model)?;
        #[cfg(windows)]
        self.validate_skills(binding)?;
        if let Some(decision) = &ready.routing {
            let selected = decision
                .selected
                .as_ref()
                .ok_or("routing selected candidate missing")?;
            if !decision.input.candidate_requests.is_empty()
                && decision.input.request_for(selected)
                    != Some((
                        ready.context.sealed().manifest.input_estimate,
                        ready.context.sealed().manifest.envelope.output,
                    ))
            {
                return Err("sealed request differs from selected candidate allocation".into());
            }
            let current_catalog = self.current_routing_catalog()?;
            if self
                .current_routing_policy()?
                .as_ref()
                .is_none_or(|policy| policy.id != decision.input.policy)
                || current_catalog
                    .as_ref()
                    .is_none_or(|catalog| catalog.id != decision.input.catalog)
                || decision.selected.as_ref().is_none_or(|selected| {
                    selected.model != ready.snapshot.compatibility.model
                        || selected.endpoint != ready.snapshot.compatibility.endpoint
                })
            {
                return Err("routing policy or selected endpoint changed before admission".into());
            }
            let catalog = &current_catalog.ok_or("routing catalog unavailable")?;
            let policy = self
                .current_routing_policy()?
                .ok_or("routing policy unavailable")?;
            let ledger = vcp_budget::ledger(self.engine.store().current(), &binding.scope)?;
            let replay_owner_choice = !admitting
                && self
                    .routing
                    .as_ref()
                    .is_some_and(|runtime| !runtime.configuration.owner_assignments.is_empty());
            let available = if replay_owner_choice {
                // Only replay the preceding owner choice here. Its reservation
                // is already active or unresolved; requiring it a second time
                // can incorrectly block a cheaper in-set replacement. Retry
                // selection and admission still check the current balance.
                decision.input.available.clone()
            } else {
                MonetaryLimit {
                    currency: ledger.currency.clone(),
                    micros: ledger.remaining_before_protected()?,
                }
            };
            self.revalidate_routing_selection(
                decision,
                catalog,
                &policy,
                available,
                ledger.protected,
            )?;
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
        let reasoning_effort = if ready.routing.is_some() {
            self.current_reasoning_effort()?
        } else {
            None
        };
        let validation_now = now();
        let mut encoding = self.encoding_diagnostic(
            binding,
            crate::foundation::execution_diagnostics::EncodingPurpose::SealedValidation,
            &ready.snapshot,
            reasoning_effort,
        );
        encoding.request(&ready.context.sealed().manifest.request_sha256);
        request::validate_sealed_with_effort_observed(
            &ready.context,
            &ready.snapshot,
            &ready.schemas,
            validation_now,
            reasoning_effort,
            &mut encoding.work.borrow_mut(),
        )?;
        drop(encoding);
        let output_ceiling = if ready.routing.is_some() {
            self.current_output_ceiling()?
        } else {
            self.config.output_ceiling
        };
        if let Some(allocation) = &sealed.manifest.allocation {
            allocation.validate(
                sealed.manifest.envelope.input_capacity()?,
                sealed.manifest.envelope.output,
            )?;
            if allocation.host_output_ceiling != output_ceiling
                || allocation.output_limit > output_ceiling
            {
                return Err("sealed allocation exceeds current host ceiling".into());
            }
            #[cfg(windows)]
            if !self.coding_allocation_matches(binding, allocation) {
                return Err("sealed allocation differs from current controller decision".into());
            }
        } else if sealed.manifest.envelope.output != output_ceiling
            || sealed.manifest.envelope.output > self.config.output_ceiling
        {
            return Err("sealed output differs from effective host ceiling".into());
        }
        if ready.routing.is_some()
            && sealed.manifest.input_estimate > self.current_input_ceiling()?
        {
            return Err("sealed input exceeds effective host ceiling".into());
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
                .current()
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
        #[cfg(windows)]
        let canonical_root = self.task_root(&binding.scope.task)?.path().to_owned();
        #[cfg(not(windows))]
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
            context: Arc::new(self.verify_context(sealed)?),
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
        let span = self.begin_diagnostic(binding, crate::foundation::execution_diagnostics::Phase::PolicyAdmission, None);
        let result = self.admit_context_inner(binding, retained);
        span.finish(&result);
        result
    }
    fn admit_context_inner(&mut self, binding: &ThreadBinding, retained: &serde_json::Value) -> Result<Prepared> {
        #[cfg(windows)]
        if let Err(error) = self.check_coding_bounds() {
            self.pause_root("canonical root coding limit requires attention")?;
            return Err(error);
        }
        #[cfg(windows)]
        if self.coding.contains_key(&binding.scope.task) {
            let span = self.begin_diagnostic(binding, crate::foundation::execution_diagnostics::Phase::ContextAssembly, None);
            let assembled = self.assemble_coding_context(binding);
            span.finish(&assembled);
            if let Err(error) = assembled {
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
            if now < retry.not_before || retry.deadline.is_some_and(|deadline| now >= deadline) {
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
            context: {
                self.reverify_context(&ready.context)?;
                ready.context
            },
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
        let output_ceiling = ready.context.sealed().manifest.envelope.output;
        let routing = ready.routing.clone();
        #[cfg(windows)]
        let escalation = ready.escalation.clone();
        self.provider
            .as_mut()
            .ok_or("provider configuration missing")?
            .active
            .insert(binding.scope.task.clone(), ready);
        Ok(Prepared {
            output_ceiling,
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
        self.validate_skills(binding)?;
        if let Some(ready) = self
            .provider
            .as_ref()
            .and_then(|p| p.active.get(&binding.scope.task))
        {
            self.child_model_scope(binding, &ready.snapshot.compatibility.model)?;
            self.require_configured_routing()?;
            if let Some(decision) = &ready.routing {
                let policy = self
                    .current_routing_policy()?
                    .ok_or("routing configuration missing at send fence")?;
                let catalog = self
                    .current_routing_catalog()?
                    .ok_or("routing registry missing at send fence")?;
                if policy.id != decision.input.policy || catalog.id != decision.input.catalog {
                    return Err("routing policy or catalog changed at send fence".into());
                }
                ready.snapshot.current(now())?;
                if decision
                    .selected_snapshot(&catalog)?
                    .is_none_or(|snapshot| snapshot != &ready.snapshot)
                {
                    return Err("routing snapshot changed at send fence".into());
                }
                // The exact reservation is already active. Recheck expiring
                // evidence using the recorded pre-reservation allocation;
                // atomic budget admission owns the current money check.
                self.revalidate_routing_selection(
                    decision,
                    &catalog,
                    &policy,
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
        deadline: Option<std::time::Instant>,
        failure: vcp_models::retry::Failure,
        http_status: Option<u16>,
        retry_after_ms: Option<u64>,
    ) -> Result<Option<Duration>> {
        // Retain the failure even when no retry is allowed. The final permit
        // drop must explain why the task paused, not just report unknown cost.
        if let Some(provider) = self.provider.as_mut() {
            let limit_source = provider.error_sources.remove(&attempt);
            provider.failures.insert(
                attempt.clone(),
                vcp_models::retry::ProviderFailure {
                    failure,
                    http_status,
                    limit_source,
                    retry_after_ms,
                },
            );
        }
        self.can_start(binding)?;
        let Some(provider) = self.provider.as_ref() else {
            return Ok(None);
        };
        let limit_source = provider
            .failures
            .get(&attempt)
            .and_then(|value| value.limit_source);
        if matches!(
            limit_source,
            Some(
                vcp_models::retry::LimitSource::OpenrouterKeyLimit
                    | vcp_models::retry::LimitSource::OpenrouterCredits
            )
        ) {
            return Ok(None);
        }
        let rotation = self
            .routing
            .as_ref()
            .and_then(|runtime| runtime.configuration.rotation.as_ref())
            .is_some_and(|policy| !policy.sets(binding.role).is_empty());
        if provider.retries.contains_key(&binding.scope.task) {
            return Err("retry already scheduled".into());
        }
        if let Some(policy) = self.current_escalation_policy()? {
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
                .current()
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
        let now_instant = std::time::Instant::now();
        if deadline.is_some_and(|deadline| now_instant >= deadline) {
            return Ok(None);
        }
        let now = now();
        let effective_deadline = deadline.map_or(vcp_domain::Limit::Unbounded, |deadline| {
            vcp_domain::Limit::Finite(Timestamp::new(
                now.get().saturating_add(
                    deadline
                        .saturating_duration_since(now_instant)
                        .as_millis()
                        .min(u128::from(u64::MAX)) as u64,
                ),
            ))
        });
        let mut policy = vcp_models::retry::Policy::for_failure(
            self.config.max_transport_retries,
            effective_deadline,
            failure,
        );
        let independent_failover = rotation
            && matches!(
                failure,
                vcp_models::retry::Failure::RateLimit | vcp_models::retry::Failure::Transient
            )
            && !matches!(
                limit_source,
                Some(vcp_models::retry::LimitSource::OpenrouterInFlightBudget)
            );
        if independent_failover {
            policy.base_delay_ms = 1;
            policy.max_delay_ms = 1;
        }
        let Some(retry) = policy.next(
            attempt.clone(),
            count,
            now,
            failure,
            true,
            if independent_failover {
                None
            } else {
                retry_after_ms
            },
            true,
        )?
        else {
            return Ok(None);
        };
        let ready = self
            .provider
            .as_ref()
            .and_then(|provider| provider.active.get(&binding.scope.task))
            .ok_or("retry source context missing")?;
        let owner = self
            .routing
            .as_ref()
            .filter(|runtime| !runtime.configuration.owner_assignments.is_empty());
        let mut owner_excluded = if owner.is_some() {
            ready
                .routing
                .as_ref()
                .map(|decision| decision.input.excluded.clone())
                .unwrap_or_default()
        } else {
            std::collections::BTreeSet::new()
        };
        let switch_owner_model = cfg!(windows)
            && owner.is_some()
            && matches!(
                failure,
                vcp_models::retry::Failure::RateLimit | vcp_models::retry::Failure::Transient
            );
        if switch_owner_model && !rotation {
            let decision = ready
                .routing
                .as_ref()
                .ok_or("owner fallback requires a retained routing decision")?;
            let failed = decision
                .selected
                .as_ref()
                .ok_or("owner fallback requires an exact failed identity")?;
            owner_excluded.insert(failed.clone());
            let assigned = owner
                .into_iter()
                .flat_map(|runtime| &runtime.configuration.owner_assignments)
                .find(|assignment| assignment.role == binding.role)
                .map(|assignment| assignment.candidates.as_slice())
                .unwrap_or(&[]);
            let catalog = self
                .current_routing_catalog()?
                .ok_or("owner fallback catalog missing")?;
            let policy = self
                .current_routing_policy()?
                .ok_or("owner fallback policy missing")?;
            let ledger = vcp_budget::ledger(self.engine.store().current(), &binding.scope)?;
            let mut input = decision.input.clone();
            input.now = now;
            input.retry_pin = None;
            input.excluded = owner_excluded.clone();
            input.catalog = catalog.id.clone();
            input.policy = policy.id.clone();
            // A failed submitted request is never treated as free. Moving its
            // active reservation to unknown liability preserves this balance.
            input.available = MonetaryLimit {
                currency: ledger.currency.clone(),
                micros: ledger.remaining_before_protected()?,
            };
            input.protected_verification = ledger.protected;
            if vcp_models::routing::select_owner_set(&catalog, &policy, &input, assigned)?
                .selected
                .is_none()
            {
                return Ok(None);
            }
        }
        if rotation {
            owner_excluded.clear();
        }
        let delay = if rotation
            && matches!(
                failure,
                vcp_models::retry::Failure::RateLimit | vcp_models::retry::Failure::Transient
            )
            && !matches!(
                limit_source,
                Some(vcp_models::retry::LimitSource::OpenrouterInFlightBudget)
            ) {
            Duration::ZERO
        } else {
            Duration::from_millis(retry.not_before.get() - now.get())
        };
        self.retain_unknown(
            binding,
            &attempt,
            "provider failure retained before bounded retry",
            false,
        )?;
        if http_status.is_none()
            && matches!(
                failure,
                vcp_models::retry::Failure::Transient | vcp_models::retry::Failure::Timeout
            )
        {
            self.record_empty_response_retry(&attempt)?;
        }
        if let Some(status) = http_status {
            self.record_retry_disposition(&attempt, Some(status))?;
        }
        self.capture(
            &binding.scope,
            Channel::Evidence,
            &canonical_bytes(&serde_json::json!({
                "predecessor": attempt, "retry": count + 1, "failure": failure,
                "not_before": retry.not_before, "deadline": policy.deadline,
                "prior_liability_unresolved": true,
                "owner_set_fallback": switch_owner_model,
                "excluded_owner_endpoints": owner_excluded,
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
                    switch_owner_model,
                    owner_excluded,
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
        if &retry.predecessor != attempt
            || retry
                .deadline
                .is_some_and(|deadline| std::time::Instant::now() >= deadline)
        {
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
    pub(super) fn settle_rejected_provider(
        &mut self,
        binding: &ThreadBinding,
        attempt: &AttemptId,
    ) -> Result<()> {
        let provider = self
            .provider
            .as_mut()
            .ok_or("provider configuration missing")?;
        let parser = provider
            .streams
            .remove(attempt)
            .ok_or("provider response missing")?;
        let rejected = parser
            .rejected_usage()
            .cloned()
            .ok_or("rejected usage missing")?;
        provider.active.remove(&binding.scope.task);
        let amount = rejected
            .usage
            .cost
            .clone()
            .ok_or("rejected usage omitted observed cost")?;
        let mut writer = self
            .streams
            .remove(attempt)
            .ok_or("response capture missing")?;
        // The captured prefix contains a complete, structurally validated
        // terminal. Rejected tool arguments do not make those bytes partial.
        let descriptor = writer.finalize()?;
        drop(writer);
        self.command(
            Command::AttachArtifact {
                descriptor: descriptor.clone(),
            },
            Some(binding.scope.task.clone()),
            Revision::ZERO,
        )?;
        self.capture(
            &binding.scope,
            Channel::Evidence,
            &canonical_bytes(&rejected)?,
            "openrouter-rejected-response-usage/1",
        )?;
        let actor = self.actor();
        self.runtime.block_on(vcp_budget::observe(
            self.engine.store_mut(),
            UsageObservation {
                id: ObservationId::new(),
                scope: binding.scope.clone(),
                attempt: attempt.clone(),
                provider_request: rejected.response_id,
                mode: UsageMode::Cumulative {
                    version: Units::new(1),
                },
                amount,
                final_usage: true,
                raw: descriptor.spec.id,
                correction: None,
            },
            &actor,
        ))?;
        // No normalized answer or tool proposals cross this accounting-only
        // boundary, even when other output items were individually valid.
        self.pause_root("provider tool arguments rejected; final observed cost retained")?;
        Ok(())
    }
    pub(super) fn complete_provider(
        &mut self,
        binding: &ThreadBinding,
        attempt: &AttemptId,
        response_id: &str,
    ) -> Result<()> {
        let ready = self
            .provider
            .as_mut()
            .and_then(|provider| provider.active.remove(&binding.scope.task));
        #[cfg(windows)]
        let mcp_provenance = ready.map(|ready| {
            crate::foundation::mcp::Provenance::from_context(
                ready.context,
                ready.roots,
                ready.memory,
            )
        });
        #[cfg(not(windows))]
        let _ = ready;
        let parser = self
            .provider
            .as_mut()
            .ok_or("provider configuration missing")?
            .streams
            .remove(attempt)
            .ok_or("provider response missing")?;
        let generation = parser.observed_generation().cloned();
        let normalized = parser.finish_observed_terminal()?;
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
        #[cfg(windows)]
        self.observe_coding_allocation(binding, attempt, &normalized)?;
        if let Some(amount) = normalized
            .usage
            .as_ref()
            .and_then(|usage| usage.cost.clone())
        {
            let actor = self.actor();
            #[cfg(feature = "qualification")]
            self.qualification_model_dispatch_point(
                crate::foundation::model_dispatch_qualification::Point::BeforeSettlement,
                attempt,
            )?;
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
        } else {
            // The response writer is already finalized and removed. Persist the
            // receipt lookup identity before retain_unknown, whose interrupted
            // capture path otherwise has no writer from which to create it.
            self.record_failed_charge(attempt, &descriptor, None, generation.as_ref())?;
            let reason = normalized.terminal_diagnostic.as_ref().map_or_else(
                || "provider response omitted observed cost; submitted charge remains unresolved and requires accounting reconciliation".to_owned(),
                |diagnostic| format!("{}; provider response omitted observed cost; submitted charge remains unresolved and requires accounting reconciliation", diagnostic.summary()),
            );
            // A validated terminal is stronger evidence than the retained
            // client's generic error classification for that terminal.
            if let Some(provider) = self.provider.as_mut() {
                provider.failures.remove(attempt);
                provider.error_sources.remove(attempt);
            }
            self.retain_unknown(binding, attempt, &reason, false)?;
            // Only a fully observed, validated complete response can proceed
            // with financial-only uncertainty. Unknown send/outcome, incomplete
            // output and availability-liability fences retain their behavior.
            let complete = normalized.status == stream::Status::Completed
                && normalized.terminal_diagnostic.is_none();
            let unbounded = vcp_budget::ledger(self.engine.store().current(), &binding.scope)?
                .cap
                .is_unbounded();
            if !complete || !unbounded {
                return self.pause_root(&reason);
            }
        }
        let admitted = vcp_budget::attempt(
            self.engine.store().current(),
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
        if let Some(diagnostic) = &normalized.terminal_diagnostic {
            #[cfg(windows)]
            if matches!(
                diagnostic,
                stream::TerminalDiagnostic::Incomplete {
                    reason: Some(stream::IncompleteReason::MaxOutputTokens)
                }
            ) && self.coding.contains_key(&binding.scope.task)
                && self.queue_output_continuation(
                    binding,
                    attempt,
                    vec![
                        descriptor.spec.id.clone(),
                        normalized_capture.spec.id.clone(),
                    ],
                )?
            {
                return Ok(());
            }
            self.pause_root(&format!(
                "{}; final observed cost retained",
                diagnostic.summary()
            ))?;
            return Ok(());
        }
        #[cfg(windows)]
        if self.coding.contains_key(&binding.scope.task) {
            let financial_proof = normalized
                .usage
                .as_ref()
                .and_then(|usage| usage.cost.as_ref())
                .is_none()
                && admitted.phase == ReservationState::ReconciliationPending
                && normalized
                    .served_model
                    .as_ref()
                    .is_none_or(|model| model == &admitted.quote.price.model);
            self.complete_coding_response(
                binding,
                attempt,
                normalized,
                vec![
                    descriptor.spec.id.clone(),
                    normalized_capture.spec.id.clone(),
                ],
                mcp_provenance,
            )?;
            if financial_proof {
                self.record_completed_financial_uncertainty(
                    &admitted,
                    response_id,
                    descriptor,
                    normalized_capture,
                )?;
            }
        }
        Ok(())
    }
}
