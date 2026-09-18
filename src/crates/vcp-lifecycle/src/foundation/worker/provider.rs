// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_context::manifest::{Revisions, Sealed, VerifiedContext};
use vcp_models::{catalog::Snapshot, request, stream};
use vcp_repository::Root;

pub(super) struct Provider {
    snapshot: Snapshot,
    prepared: HashMap<TaskId, Ready>,
    pub streams: HashMap<AttemptId, stream::Stream>,
    pub timeout: Duration,
}
struct Ready {
    context: VerifiedContext,
    schemas: serde_json::Value,
    roots: Vec<Root>,
}
pub(super) struct Prepared {
    pub body: serde_json::Value,
    pub stream: stream::Stream,
}

impl Context {
    pub fn configure_provider(
        &mut self,
        snapshot: Snapshot,
        raw: Vec<u8>,
        timeout: Duration,
    ) -> Result<()> {
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
        if !self.streams.is_empty() {
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
        self.config.input_ceiling = Units::new(
            self.config
                .input_ceiling
                .get()
                .min(snapshot.max_input.get()),
        );
        self.provider = Some(Provider {
            snapshot,
            prepared: HashMap::new(),
            streams: HashMap::new(),
            timeout,
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
        let provider = self
            .provider
            .as_ref()
            .ok_or("OpenRouter configuration required")?;
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
        request::validate_sealed(&ready.context, &provider.snapshot, &ready.schemas, now())?;
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
            context: self.verify_context(sealed)?,
            schemas,
            roots,
        };
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
        let ready = self
            .provider
            .as_mut()
            .ok_or("OpenRouter configuration required after reopen")?
            .prepared
            .remove(&binding.scope.task)
            .ok_or("fresh sealed context required for every retained attempt")?;
        self.validate_ready(binding, &ready)?;
        let ready = Ready {
            context: self.verify_context(ready.context.into_sealed())?,
            schemas: ready.schemas,
            roots: ready.roots,
        };
        if retained["model"].as_str() != Some(&ready.context.sealed().manifest.envelope.model) {
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
        Ok(Prepared { body, stream })
    }
    pub(super) fn complete_provider(
        &mut self,
        binding: &ThreadBinding,
        attempt: &AttemptId,
        response_id: &str,
    ) -> Result<()> {
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
        self.capture(
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
                raw: descriptor.spec.id,
                correction: None,
            },
            &actor,
        ))?;
        if normalized
            .served_model
            .as_ref()
            .is_some_and(|model| model != &self.config.price.model)
        {
            self.pause_root("observed served model differs from admitted model pin")?;
        }
        Ok(())
    }
}
