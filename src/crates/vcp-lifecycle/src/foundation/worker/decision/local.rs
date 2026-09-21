// SPDX-License-Identifier: Apache-2.0
//! Owner-selected frozen local shadow fits. No transport or model accounting.
use super::*;
use crate::foundation::routing_state::{local_stall, HistoryWindow};
use host::EvidencePin;
use vcp_memory::retention::{recall_allowed, Target};

const FIT_SCHEMA: &str = "vcp-local-stall-installed-fit-v1";
const RESULT_SCHEMA: &str = "vcp-local-stall-shadow-result-v1";
const MAX_BYTES: usize = 512 * 1024;

pub(super) struct Installed {
    pin: EvidencePin,
    fit: Arc<local_stall::Fit>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstalledDocument {
    schema_version: u32,
    installed_by: ActorId,
    fit: local_stall::Fit,
}

pub(in crate::foundation) struct Work {
    pub prepared: local_stall::Prepared,
    pub deadline: Timestamp,
    pub main_attempt: AttemptId,
    #[cfg(feature = "qualification")]
    pub local_after_compute: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
    seed: Arc<Seed>,
    pin: EvidencePin,
    fit: Arc<local_stall::Fit>,
    revision: Revision,
    window: HistoryWindow,
}

fn result_id(main: &AttemptId) -> Result<ArtifactId> {
    Ok(ArtifactId::parse(format!(
        "local-stall-{}",
        vcp_protocol::digest_bytes(main.as_str().as_bytes())
    ))?)
}

impl Context {
    #[cfg(feature = "qualification")]
    pub(in crate::foundation) fn qualification_block_local_after_compute(
        &mut self,
        arrived: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    ) -> Result<()> {
        self.decision_setup_allowed()?;
        self.decisions.local_after_compute = Some((arrived, release));
        Ok(())
    }
    fn local_access(&self, task: &TaskId) -> Result<vcp_memory::access::Access> {
        let mut access = self.routing_access();
        if !access.allows_task(task) {
            return Err("local fit task access denied".into());
        }
        access.tasks = Some(BTreeSet::from([task.clone()]));
        Ok(access)
    }

    pub(in crate::foundation) fn fit_local_shadow(
        &self,
        binding: &ThreadBinding,
        window: HistoryWindow,
        minimum_samples: u64,
    ) -> Result<local_stall::Fit> {
        self.decision_setup_allowed()?;
        self.can_start(binding)?;
        Ok(local_stall::fit(
            self.engine.store(),
            &self.local_access(&binding.scope.task)?,
            window,
            minimum_samples,
        )?)
    }

    /// Explicit fitting/installation is the only boundary that rebuilds counts.
    pub(in crate::foundation) fn install_local_shadow(
        &mut self,
        fit: local_stall::Fit,
    ) -> Result<EvidencePin> {
        self.decision_setup_allowed()?;
        let access = self.local_access(&fit.task)?;
        local_stall::validate_install(self.engine.store(), &access, &fit)?;
        let task: Task = self
            .engine
            .store()
            .state()
            .record(Collection::Task, fit.task.as_str(), &self.config.workspace)?
            .decode()?;
        let mut references = BTreeSet::from([key(Collection::Task, fit.task.as_str())]);
        references.extend(
            fit.source_verifications
                .iter()
                .map(|v| key(Collection::Verification, v.as_str())),
        );
        let document = InstalledDocument {
            schema_version: 1,
            installed_by: self.config.actor.clone(),
            fit,
        };
        let bytes = canonical_bytes(&document)?;
        let descriptor = self.capture_local_artifact(
            &task.scope,
            ArtifactId::new(),
            &bytes,
            FIT_SCHEMA,
            references,
        )?;
        let pin = EvidencePin {
            artifact: descriptor.spec.id,
            digest: descriptor.sha256,
        };
        self.decisions.local = Some(Installed {
            pin: pin.clone(),
            fit: Arc::new(document.fit),
        });
        self.decisions.revision = self.decisions.revision.next()?;
        self.decisions.configuration.mode = Mode::Shadow;
        Ok(pin)
    }

    fn read_local_fit(&self, pin: &EvidencePin) -> Result<local_stall::Fit> {
        let descriptor: ArtifactDescriptor = self
            .engine
            .store()
            .state()
            .record(
                Collection::Artifact,
                pin.artifact.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        if descriptor.spec.schema != FIT_SCHEMA
            || descriptor.sha256 != pin.digest
            || descriptor.length.get() > MAX_BYTES as u64
            || descriptor.state != vcp_domain::artifact::CaptureState::Complete
            || !recall_allowed(
                self.engine.store().state(),
                &self.config.workspace,
                &Target::Record(key(Collection::Artifact, pin.artifact.as_str())),
            )?
        {
            return Err("local fit artifact identity or retention changed".into());
        }
        let mut bytes = Vec::new();
        vcp_audit::history::History::read_artifact(
            self.engine.store(),
            &self.history_access(),
            &pin.artifact,
            &mut bytes,
        )?;
        if vcp_protocol::digest_bytes(&bytes) != pin.digest {
            return Err("local fit bytes changed".into());
        }
        let document: InstalledDocument = serde_json::from_slice(&bytes)?;
        if document.schema_version != 1
            || document.installed_by != self.config.actor
            || document.fit.task != descriptor.spec.scope.task
        {
            return Err("local fit is not the current owner's installed task artifact".into());
        }
        local_stall::validate(
            self.engine.store(),
            &self.local_access(&document.fit.task)?,
            &document.fit,
        )?;
        Ok(document.fit)
    }

    /// Selection after reopen is explicit and never executes saved triggers.
    pub(in crate::foundation) fn select_local_shadow(&mut self, pin: EvidencePin) -> Result<()> {
        self.decision_setup_allowed()?;
        let fit = self.read_local_fit(&pin)?;
        // Retained artifacts are untrusted inputs. Explicit selection proves
        // their parameters again; ordinary trigger evaluation never refits.
        local_stall::validate_install(self.engine.store(), &self.local_access(&fit.task)?, &fit)?;
        self.decisions.local = Some(Installed {
            pin,
            fit: Arc::new(fit),
        });
        self.decisions.revision = self.decisions.revision.next()?;
        self.decisions.configuration.mode = Mode::Shadow;
        Ok(())
    }

    pub(in crate::foundation) fn prepare_local_shadow(
        &self,
        binding: &ThreadBinding,
    ) -> Result<Option<Arc<Work>>> {
        if self.decisions.configuration.mode != Mode::Shadow {
            return Ok(None);
        }
        let Some(installed) = &self.decisions.local else {
            return Ok(None);
        };
        let Some(seed) = self.decisions.pending.get(&binding.scope.task) else {
            return Ok(None);
        };
        if seed.source.escalation.is_none() {
            return Ok(None);
        }
        if installed.fit.task != binding.scope.task {
            return Ok(None);
        }
        let id = result_id(&seed.main_attempt)?;
        if self
            .engine
            .store()
            .state()
            .records
            .contains_key(&key(Collection::Artifact, id.as_str()))
        {
            return Ok(None);
        }
        self.validate_decision_source(seed)?;
        if self.read_local_fit(&installed.pin)? != *installed.fit {
            return Err("local installed fit changed".into());
        }
        let start = now();
        let window = HistoryWindow {
            from: Some(installed.fit.source_window.until),
            until: start,
        };
        let prepared = local_stall::prepare_evaluation(
            self.engine.store(),
            &self.local_access(&binding.scope.task)?,
            &installed.fit,
            &binding.scope.task,
            window.clone(),
        )?;
        Ok(Some(Arc::new(Work {
            prepared,
            deadline: Timestamp::new(start.get().saturating_add(2000)),
            main_attempt: seed.main_attempt.clone(),
            #[cfg(feature = "qualification")]
            local_after_compute: self.decisions.local_after_compute.clone(),
            seed: seed.clone(),
            pin: installed.pin.clone(),
            fit: installed.fit.clone(),
            revision: self.decisions.revision,
            window,
        })))
    }

    pub(in crate::foundation) fn complete_local_shadow(
        &mut self,
        work: Arc<Work>,
        outcome: local_stall::Outcome,
    ) -> Result<ShadowOutcome> {
        if now() >= work.deadline
            || self.decisions.configuration.mode != Mode::Shadow
            || self.decisions.revision != work.revision
            || self
                .decisions
                .local
                .as_ref()
                .is_none_or(|i| i.pin != work.pin)
            || self
                .decisions
                .pending
                .get(&work.seed.binding.scope.task)
                .is_none_or(|s| s.main_attempt != work.main_attempt)
        {
            return self.skip_local_shadow(
                &work,
                "local shadow deadline, selection or pending trigger changed",
            );
        }
        if self.validate_decision_source(&work.seed).is_err() {
            return self.skip_local_shadow(&work, "local shadow source changed before consumption");
        }
        if self.read_local_fit(&work.pin)? != *work.fit {
            return Err("local selected fit changed".into());
        }
        let current = match local_stall::prepare_evaluation(
            self.engine.store(),
            &self.local_access(&work.fit.task)?,
            &work.fit,
            &work.fit.task,
            work.window.clone(),
        ) {
            Ok(current) => current,
            Err(_) => {
                return self.skip_local_shadow(
                    &work,
                    "local shadow observations changed before consumption",
                )
            }
        };
        if current != work.prepared
            || outcome.fit != work.fit.id
            || outcome.task != work.fit.task
            || outcome.source_window != work.window
            || outcome.serving_qualified
        {
            return self.skip_local_shadow(&work, "local shadow input changed during computation");
        }
        if now() >= work.deadline {
            return self
                .skip_local_shadow(&work, "local shadow deadline expired during revalidation");
        }
        let id = result_id(&work.main_attempt)?;
        if self
            .engine
            .store()
            .state()
            .records
            .contains_key(&key(Collection::Artifact, id.as_str()))
        {
            return Err("local shadow already retained; no replay".into());
        }
        let mut references = BTreeSet::from([
            key(Collection::Task, work.fit.task.as_str()),
            key(Collection::Artifact, work.pin.artifact.as_str()),
            key(Collection::Attempt, work.main_attempt.as_str()),
        ]);
        references.extend(
            work.seed
                .source
                .context
                .sealed()
                .manifest
                .included
                .iter()
                .map(|part| key(Collection::Artifact, part.artifact.as_str())),
        );
        if let Some(plan) = &work.seed.source.escalation {
            references.extend(
                plan.trigger
                    .evidence
                    .iter()
                    .map(|id| key(Collection::Artifact, id.as_str())),
            );
        }
        references.extend(
            work.prepared
                .source_verifications
                .iter()
                .map(|v| key(Collection::Verification, v.as_str())),
        );
        let document = serde_json::json!({"schema_version":1,"shadow":true,"baseline_changed":false,
            "main_attempt":work.main_attempt,"baseline":work.seed.source.baseline,"fit":work.pin,
            "local_statistics":outcome,"provider_requests":0,"remote_requests":0,"replay":false});
        let descriptor = self.capture_local_artifact(
            &work.seed.binding.scope,
            id,
            &canonical_bytes(&document)?,
            RESULT_SCHEMA,
            references,
        )?;
        Ok(ShadowOutcome {
            main_attempt: Some(work.main_attempt.clone()),
            evaluator_attempt: None,
            outcome: document,
            artifacts: vec![descriptor.spec.id],
        })
    }

    fn skip_local_shadow(&mut self, work: &Work, reason: &str) -> Result<ShadowOutcome> {
        // Stale statistics are deliberately omitted. If access or retained
        // training sources were revoked, even historical capture is denied.
        self.decision_setup_allowed()?;
        self.read_local_fit(&work.pin)?;
        let id = result_id(&work.main_attempt)?;
        if self
            .engine
            .store()
            .state()
            .records
            .contains_key(&key(Collection::Artifact, id.as_str()))
        {
            return Err("local shadow already retained; no replay".into());
        }
        let document = serde_json::json!({"schema_version":1,"outcome":"skipped","reason":reason,
            "shadow":true,"historical_only":true,"baseline_changed":false,"main_attempt":work.main_attempt,
            "fit":work.pin,"provider_requests":0,"remote_requests":0,"replay":false});
        let references = BTreeSet::from([
            key(Collection::Task, work.fit.task.as_str()),
            key(Collection::Attempt, work.main_attempt.as_str()),
            key(Collection::Artifact, work.pin.artifact.as_str()),
        ]);
        let descriptor = self.capture_local_artifact(
            &work.seed.binding.scope,
            id,
            &canonical_bytes(&document)?,
            "vcp-local-stall-shadow-skipped-v1",
            references,
        )?;
        Ok(ShadowOutcome {
            main_attempt: Some(work.main_attempt.clone()),
            evaluator_attempt: None,
            outcome: document,
            artifacts: vec![descriptor.spec.id],
        })
    }

    fn capture_local_artifact(
        &mut self,
        scope: &Scope,
        id: ArtifactId,
        bytes: &[u8],
        schema: &str,
        references: BTreeSet<String>,
    ) -> Result<ArtifactDescriptor> {
        self.engine.authorize(&self.access)?;
        if !self.access.write
            || scope.workspace != self.access.workspace
            || scope.session != self.access.session
            || bytes.len() > MAX_BYTES
            || references.len() > 4098
        {
            return Err("local artifact scope, size or write access denied".into());
        }
        let mut spec = self.spec(scope, Channel::Evidence, schema);
        spec.id = id;
        let mut writer = self.engine.store().spool().create(spec)?;
        for chunk in bytes.chunks(vcp_store::artifact::CHUNK_BYTES) {
            writer.write_chunk(chunk)?;
        }
        let descriptor = writer.finalize()?;
        drop(writer);
        let mut record = Record::typed(
            Collection::Artifact,
            descriptor.spec.id.as_str(),
            scope.workspace.clone(),
            Revision::ZERO,
            &descriptor,
        )?;
        record.references.extend(references);
        let event = vcp_protocol::event::EventInput {
            id: EventId::new(),
            workspace: scope.workspace.clone(),
            session: scope.session.clone(),
            task: Some(scope.task.clone()),
            actor: self.config.actor.clone(),
            correlation: CommandId::new(),
            causation: None,
            timestamp: now(),
            kind: vcp_protocol::event::EventKind::ArtifactAttached,
            artifacts: vec![descriptor.spec.id.clone()],
            data: serde_json::json!({"schema_version":1,"facts":[{"collection":"artifact","id":descriptor.spec.id,"revision":Revision::ZERO,"value":descriptor}]}),
            metadata: None,
        };
        let transaction = Transaction {
            id: TransactionId::new(),
            expected_watermark: self.engine.store().state().watermark,
            mutations: vec![Mutation::Put {
                record,
                expected: None,
            }],
            events: vec![event],
            command: None,
        };
        self.runtime
            .block_on(self.engine.store_mut().transact(transaction))?;
        Ok(descriptor)
    }
}
