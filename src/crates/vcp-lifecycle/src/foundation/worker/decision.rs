// SPDX-License-Identifier: Apache-2.0
use super::*;
mod escalation_input;
pub(in crate::foundation) mod local;
use crate::foundation::decision::{
    self as host,
    admission::{Capability, CurrentInstallation, FinitePrepared},
    credentials, transport, Configuration, Mode, QualificationRecord, QualificationRef,
    ShadowOutcome,
};
use crate::foundation::routing_state::advisory;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vcp_context::manifest::VerifiedContext;
use vcp_domain::policy::EffectClass;
use vcp_models::{decision as codec, routing::RoutingDecision};

fn shadow_run_id(main: &AttemptId) -> String {
    format!(
        "decision-shadow-{}",
        vcp_protocol::digest_bytes(main.as_str().as_bytes())
    )
}

#[derive(Default)]
pub(super) struct Runtime {
    configuration: Configuration,
    revision: Revision,
    installed: Option<Installed>,
    local: Option<local::Installed>,
    #[cfg(feature = "qualification")]
    local_after_compute: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
    pending: HashMap<TaskId, Arc<Seed>>,
    latest: HashMap<TaskId, AttemptId>,
    pub(crate) credentials: credentials::Registry,
    credential_revision: Option<Revision>,
}
struct Installed {
    record: QualificationRecord,
    reference: QualificationRef,
    capability: Arc<Capability>,
    destination: transport::Destination,
    pin: credentials::Pin,
}
pub(in crate::foundation) struct Source {
    pub escalation: Option<vcp_models::escalation::Plan>,
    pub context: Arc<VerifiedContext>,
    pub roots: Vec<vcp_repository::Root>,
    pub memory: Option<crate::foundation::memory_query::SendFence>,
    pub baseline: RoutingDecision,
}
pub(in crate::foundation) struct Seed {
    pub binding: ThreadBinding,
    pub main_attempt: AttemptId,
    pub source: Source,
}
pub(in crate::foundation) struct Candidate {
    pub seed: Arc<Seed>,
    pub prepared: FinitePrepared,
    pub credential: credentials::CredentialLease,
    pub destination: transport::Destination,
    pin: credentials::Pin,
    capability: Arc<Capability>,
    pub deadline: Timestamp,
    revision: Revision,
    reference: QualificationRef,
}
pub(in crate::foundation) enum Selection {
    Baseline(ShadowOutcome),
    Candidate(Arc<Candidate>),
}
pub(in crate::foundation) struct Admitted {
    pub candidate: Arc<Candidate>,
    pub attempt: AttemptId,
    pub request: ArtifactId,
    pub advisory: Option<AdvisoryClaim>,
}
#[derive(Clone)]
pub(in crate::foundation) struct AdvisoryClaim {
    pub request: advisory::RequestRecord,
    pub claimant: CommandId,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstalledRecord {
    version: u32,
    record: QualificationRecord,
    fixture: bool,
    installed_by: ActorId,
}
/// Exact conformance claim read from the owner-selected immutable artifact.
/// Only explicit trusted installation accepts this claim, never model/tool data.
#[derive(Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConformanceEvidence {
    version: u32,
    operation: codec::Operation,
    purpose: codec::Purpose,
    mode: Mode,
    question_revision: String,
    require_distributions: bool,
    require_confidence: bool,
    model: String,
    provider: String,
    served_model: String,
    served_provider: String,
    valid_until: Timestamp,
    prompt_price_per_million: String,
    output_price_per_million: String,
    request_price: String,
    input_ceiling: Units,
    output_ceiling: Units,
    configuration_digest: String,
    price: PriceSnapshot,
    no_tools: bool,
    no_fallback: bool,
    parameters_enforced: bool,
    deny_data_collection: bool,
    require_zdr: bool,
}
impl ConformanceEvidence {
    pub fn from_record(r: &QualificationRecord) -> Self {
        Self {
            version: 1,
            operation: r.evaluator.operation,
            purpose: r.evaluator.purpose,
            mode: r.evaluator.mode,
            question_revision: r.question_revision.clone(),
            require_distributions: r.evaluator.require_distributions,
            require_confidence: r.evaluator.require_confidence,
            model: r.evaluator.model.clone(),
            provider: r.evaluator.provider.clone(),
            served_model: r.evaluator.served_model.clone(),
            served_provider: r.evaluator.served_provider.clone(),
            valid_until: r.evaluator.valid_until,
            prompt_price_per_million: r.evaluator.prompt_price_per_million.clone(),
            output_price_per_million: r.evaluator.output_price_per_million.clone(),
            request_price: r.evaluator.request_price.clone(),
            input_ceiling: r.input_ceiling,
            output_ceiling: r.output_ceiling,
            configuration_digest: r.evaluator.configuration_digest.clone(),
            price: r.price.clone(),
            no_tools: true,
            no_fallback: true,
            parameters_enforced: true,
            deny_data_collection: r.evaluator.deny_data_collection,
            require_zdr: r.evaluator.require_zdr,
        }
    }
}
impl Context {
    #[cfg(feature = "qualification")]
    pub(in crate::foundation) fn qualification_block_decision_after_tls(
        &mut self,
        arrived: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    ) -> Result<()> {
        let installed = self
            .decisions
            .installed
            .as_mut()
            .ok_or("decision qualification missing")?;
        if !installed.destination.fixture_origin() {
            return Err("barrier requires fixture destination".into());
        }
        installed.destination = installed
            .destination
            .clone()
            .with_after_tls(arrived, release);
        Ok(())
    }
    fn decision_scope(&self) -> Scope {
        Scope {
            workspace: self.config.workspace.clone(),
            session: self.config.session.clone(),
            task: self.config.root_task.clone(),
        }
    }
    fn decision_setup_allowed(&self) -> Result<()> {
        if !self.owner_alive || self.authority_pending || self.interrupted_capture {
            return Err("decision setup requires current canonical owner".into());
        }
        Ok(())
    }
    fn check_decision_setup_capture(
        &self,
        material: &credentials::CredentialMaterial,
        bytes: &[u8],
    ) -> Result<()> {
        let original: serde_json::Value = serde_json::from_slice(bytes)?;
        let safe = material
            .sanitize_json(bytes)
            .map_err(|_| "sensitive evaluator qualification rejected")?;
        if serde_json::from_slice::<serde_json::Value>(&safe)? != original {
            return Err("sensitive evaluator qualification rejected".into());
        }
        Ok(())
    }
    fn capture_decision_setup(
        &mut self,
        material: &credentials::CredentialMaterial,
        scope: &Scope,
        bytes: &[u8],
        schema: &str,
    ) -> Result<ArtifactDescriptor> {
        self.check_decision_setup_capture(material, bytes)?;
        self.capture(scope, Channel::Evidence, bytes, schema)
    }
    fn read_decision_evidence(&self, pin: &host::EvidencePin) -> Result<Vec<u8>> {
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
        if descriptor.sha256 != pin.digest
            || descriptor.length.get() > codec::MAX_BYTES as u64
            || descriptor.spec.scope.workspace != self.config.workspace
        {
            return Err("decision evidence identity or bound rejected".into());
        }
        let mut bytes = Vec::new();
        vcp_audit::history::History::read_artifact(
            self.engine.store(),
            &self.history_access(),
            &pin.artifact,
            &mut bytes,
        )?;
        Ok(bytes)
    }
    fn decision_installation(&self, record: &QualificationRecord) -> Result<CurrentInstallation> {
        self.decision_setup_allowed()?;
        self.read_decision_evidence(&record.catalog)?;
        let conformance = self.read_decision_evidence(&record.conformance)?;
        if serde_json::from_slice::<ConformanceEvidence>(&conformance)?
            != ConformanceEvidence::from_record(record)
        {
            return Err(
                "decision conformance does not qualify exact operation controls and finite charges"
                    .into(),
            );
        }
        Ok(CurrentInstallation {
            workspace: self.config.workspace.clone(),
            owner: self.engine.owner_epoch(),
            revision: record.revision,
            record_digest: vcp_protocol::digest_bytes(&canonical_bytes(record)?),
            catalog: record.catalog.clone(),
            conformance: record.conformance.clone(),
            configuration_digest: record.evaluator.configuration_digest.clone(),
            now: now(),
        })
    }
    fn load_decision_record(&self, reference: &QualificationRef) -> Result<QualificationRecord> {
        let descriptor: ArtifactDescriptor = self
            .engine
            .store()
            .state()
            .record(
                Collection::Artifact,
                reference.artifact.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        if descriptor.spec.schema != "vcp-decision-installed-qualification-v1" {
            return Err("qualification is not an owner-installed record".into());
        }
        let bytes = self.read_decision_evidence(&host::EvidencePin {
            artifact: reference.artifact.clone(),
            digest: reference.digest.clone(),
        })?;
        let installed: InstalledRecord = serde_json::from_slice(&bytes)?;
        if installed.version != 1
            || installed.fixture
            || installed.installed_by != self.config.actor
        {
            return Err("qualification provenance cannot enable production decisions".into());
        }
        Ok(installed.record)
    }
    pub(in crate::foundation) fn revoke_decision_credential(&mut self) -> Result<()> {
        if let Some(revision) = self.decisions.credential_revision {
            self.decisions.credential_revision = Some(self.decisions.credentials.revoke(revision)?);
        }
        Ok(())
    }
    pub(in crate::foundation) fn configure_decisions(
        &mut self,
        configuration: Configuration,
    ) -> Result<()> {
        configuration.validate()?;
        self.decision_setup_allowed()?;
        let installed = if let Some(reference) = &configuration.qualification {
            let record = self.load_decision_record(reference)?;
            let current = self.decision_installation(&record)?;
            let capability = Capability::install(record.clone(), &current)?;
            let destination = transport::Destination::production()?;
            Some(self.decision_installed(record, reference.clone(), capability, destination)?)
        } else {
            None
        };
        self.revoke_decision_credential()?;
        self.decisions.revision = self.decisions.revision.next()?;
        self.decisions.configuration = configuration;
        self.decisions.installed = installed;
        self.decisions.local = None;
        Ok(())
    }
    fn decision_installed(
        &self,
        record: QualificationRecord,
        reference: QualificationRef,
        capability: Capability,
        destination: transport::Destination,
    ) -> Result<Installed> {
        let config_digest = vcp_protocol::digest_bytes(&canonical_bytes(&(
            reference.clone(),
            destination.trust_digest(),
            self.engine.owner_epoch(),
            self.engine.controller(),
        ))?);
        let pin = credentials::Pin {
            workspace: self.config.workspace.clone(),
            owner: self.engine.owner_epoch(),
            controller: self.engine.controller().clone(),
            config_digest,
            operation: record.evaluator.operation,
            fixture: destination.fixture_origin(),
        };
        Ok(Installed {
            record,
            reference,
            capability: Arc::new(capability),
            destination,
            pin,
        })
    }
    pub(in crate::foundation) fn install_decision_qualification(
        &mut self,
        record: QualificationRecord,
        material: credentials::CredentialMaterial,
    ) -> Result<QualificationRef> {
        self.check_decision_setup_capture(&material, &canonical_bytes(&record)?)?;
        self.check_decision_setup_capture(
            &material,
            &self.read_decision_evidence(&record.catalog)?,
        )?;
        self.check_decision_setup_capture(
            &material,
            &self.read_decision_evidence(&record.conformance)?,
        )?;
        let current = self.decision_installation(&record)?;
        let capability = Capability::install(record.clone(), &current)?;
        let destination = transport::Destination::production()?;
        let descriptor = self.capture_decision_setup(
            &material,
            &self.decision_scope(),
            &canonical_bytes(&InstalledRecord {
                version: 1,
                record: record.clone(),
                fixture: false,
                installed_by: self.config.actor.clone(),
            })?,
            "vcp-decision-installed-qualification-v1",
        )?;
        let reference = QualificationRef {
            artifact: descriptor.spec.id,
            digest: descriptor.sha256,
        };
        self.revoke_decision_credential()?;
        self.decisions.installed =
            Some(self.decision_installed(record, reference.clone(), capability, destination)?);
        self.decisions.configuration = Configuration {
            mode: Mode::Shadow,
            qualification: Some(reference.clone()),
        };
        self.decisions.revision = self.decisions.revision.next()?;
        self.install_decision_credential(material)?;
        Ok(reference)
    }
    pub(in crate::foundation) fn install_decision_credential(
        &mut self,
        material: credentials::CredentialMaterial,
    ) -> Result<()> {
        self.decision_setup_allowed()?;
        let installed = self
            .decisions
            .installed
            .as_ref()
            .ok_or("decision qualification unavailable")?;
        self.check_decision_setup_capture(&material, &canonical_bytes(&installed.record)?)?;
        let expiry = installed
            .record
            .expires_at
            .min(installed.record.evaluator.valid_until);
        let lease = self.decisions.credentials.install(
            installed.pin.clone(),
            self.decisions.credential_revision,
            material,
            expiry,
            now(),
        )?;
        self.decisions.credential_revision = Some(lease.revision());
        Ok(())
    }
    #[cfg(feature = "qualification")]
    pub(in crate::foundation) fn qualification_configure_decisions(
        &mut self,
        fixture: host::FixtureConfiguration,
    ) -> Result<()> {
        self.decision_setup_allowed()?;
        self.check_decision_setup_capture(
            &fixture.credential,
            &canonical_bytes(&(
                &fixture.evaluator,
                &fixture.price,
                fixture.input_ceiling,
                fixture.output_ceiling,
                &fixture.endpoint,
            ))?,
        )?;
        let scope = self.decision_scope();
        let catalog = self.capture_decision_setup(
            &fixture.credential,
            &scope,
            b"{\"fixture\":true}",
            "vcp-decision-fixture-catalog-v1",
        )?;
        let question_revision = if fixture.evaluator.purpose == codec::Purpose::Escalation {
            vcp_models::escalation::advisory_question_revision()
        } else {
            vcp_protocol::digest_bytes(b"vcp-routing-shadow-questions-v1")
        };
        let mut record = QualificationRecord {
            version: 1,
            workspace: self.config.workspace.clone(),
            revision: self.decisions.revision.next()?,
            evaluator: fixture.evaluator,
            question_revision,
            catalog: host::EvidencePin {
                artifact: catalog.spec.id,
                digest: catalog.sha256,
            },
            conformance: host::EvidencePin {
                artifact: ArtifactId::new(),
                digest: "0".repeat(64),
            },
            price: fixture.price,
            input_ceiling: fixture.input_ceiling,
            output_ceiling: fixture.output_ceiling,
            expires_at: Timestamp::new(u64::MAX),
        };
        record.expires_at = record.evaluator.valid_until;
        let conformance = self.capture_decision_setup(
            &fixture.credential,
            &scope,
            &canonical_bytes(&ConformanceEvidence::from_record(&record))?,
            "vcp-decision-fixture-conformance-v1",
        )?;
        record.conformance = host::EvidencePin {
            artifact: conformance.spec.id,
            digest: conformance.sha256,
        };
        record.evaluator.evidence_digest = record.conformance.digest.clone();
        let current = self.decision_installation(&record)?;
        let capability = Capability::fixture(record.clone(), &current)?;
        let destination =
            transport::Destination::fixture(fixture.endpoint, vec![fixture.root_certificate])?;
        let descriptor = self.capture_decision_setup(
            &fixture.credential,
            &scope,
            &canonical_bytes(&InstalledRecord {
                version: 1,
                record: record.clone(),
                fixture: true,
                installed_by: self.config.actor.clone(),
            })?,
            "vcp-decision-installed-qualification-v1",
        )?;
        let reference = QualificationRef {
            artifact: descriptor.spec.id,
            digest: descriptor.sha256,
        };
        self.revoke_decision_credential()?;
        self.decisions.installed =
            Some(self.decision_installed(record, reference.clone(), capability, destination)?);
        self.decisions.configuration = Configuration {
            mode: Mode::Shadow,
            qualification: Some(reference),
        };
        self.decisions.revision = self.decisions.revision.next()?;
        self.install_decision_credential(fixture.credential)
    }
    pub(in crate::foundation) fn seed_decision_shadow(
        &mut self,
        binding: &ThreadBinding,
        attempt: &Attempt,
    ) {
        if !matches!(self.decisions.configuration.mode, Mode::Shadow) {
            return;
        }
        // Bound tracking over this owner's lifetime without evicting a key
        // that an in-flight candidate still needs for supersession checks.
        if self.decisions.latest.len() >= 256
            && !self.decisions.latest.contains_key(&binding.scope.task)
        {
            return;
        }
        self.decisions
            .latest
            .insert(binding.scope.task.clone(), attempt.id.clone());
        if self.decisions.pending.len() >= 256
            && !self.decisions.pending.contains_key(&binding.scope.task)
        {
            return;
        }
        if let Ok(source) = self.decision_source(binding) {
            if let Some(old) = self.decisions.pending.insert(
                binding.scope.task.clone(),
                Arc::new(Seed {
                    binding: binding.clone(),
                    main_attempt: attempt.id.clone(),
                    source,
                }),
            ) {
                if let Ok(bytes) = canonical_bytes(
                    &serde_json::json!({"schema_version":1,"main_attempt":old.main_attempt,"superseded_by":attempt.id,"outcome":"skipped","reason":"new admitted baseline superseded unsent shadow","remote_requests":0}),
                ) {
                    let _ = self.capture(
                        &binding.scope,
                        Channel::Evidence,
                        &bytes,
                        "vcp-decision-shadow-skipped-v1",
                    );
                }
            }
        }
    }
    pub(in crate::foundation) fn decision_shadow_pending(&self, binding: &ThreadBinding) -> bool {
        self.decisions.pending.contains_key(&binding.scope.task)
    }
    pub(in crate::foundation) fn take_decision_candidate(
        &mut self,
        binding: &ThreadBinding,
    ) -> Result<Selection> {
        let Some(seed) = self.decisions.pending.remove(&binding.scope.task) else {
            return Ok(Selection::Baseline(ShadowOutcome::baseline(
                None,
                "no pending admitted routing decision",
            )));
        };
        let main = Some(seed.main_attempt.clone());
        if self.decisions.configuration.mode != Mode::Shadow {
            return Ok(Selection::Baseline(ShadowOutcome::baseline(
                main,
                "deterministic baseline",
            )));
        }
        let Some(installed) = self.decisions.installed.as_ref() else {
            return Ok(Selection::Baseline(ShadowOutcome::baseline(
                main,
                "native_charge_bound_unqualified",
            )));
        };
        let prepared = (|| -> Result<Candidate> {
            let current = self.decision_installation(&installed.record)?;
            let credential = self.decisions.credentials.resolve(&installed.pin, now())?;
            self.validate_decision_source(&seed)?;
            let deadline = Timestamp::new(current.now.get().saturating_add(120_000))
                .min(installed.record.expires_at)
                .min(installed.record.evaluator.valid_until)
                .min(installed.record.price.valid_until)
                .min(credential.expires_at());
            let request = if installed.record.evaluator.purpose == codec::Purpose::Escalation {
                if installed.record.evaluator.mode != Mode::Advisory {
                    return Err(
                        "escalation requires purpose-specific advisory qualification".into(),
                    );
                }
                self.escalation_advisory_request(&seed, deadline)?
            } else {
                let state = serde_json::json!({"baseline":seed.source.baseline,"main_attempt":seed.main_attempt,"manifest":seed.source.context.sealed().manifest_digest()});
                let candidates: BTreeMap<String, String> = seed
                    .source
                    .baseline
                    .candidates
                    .iter()
                    .filter(|candidate| candidate.exclusions.is_empty())
                    .take(33)
                    .enumerate()
                    .map(|(i, candidate)| -> Result<_> {
                        Ok((
                            format!("candidate-{i}"),
                            String::from_utf8(canonical_bytes(&candidate.identity)?)?,
                        ))
                    })
                    .collect::<Result<_>>()?;
                if !(2..=32).contains(&candidates.len()) {
                    return Err(
                        "shadow routing comparison needs two to 32 eligible candidates".into(),
                    );
                }
                let revisions = &seed.source.context.sealed().manifest.revisions;
                let evidence = seed
                    .source
                    .context
                    .sealed()
                    .manifest
                    .included
                    .iter()
                    .map(|part| (part.artifact.to_string(), part.source_hash.clone()))
                    .collect();
                let request=codec::Request{version:codec::VERSION,binding:codec::Binding{scope:binding.scope.clone(),root:self.config.root_task.clone(),step:seed.source.baseline.input.input_revision,
                steering:revisions.steering,authority:revisions.authority,deletion:revisions.deletion,policy:seed.source.baseline.input.policy.clone(),catalog:seed.source.baseline.input.catalog.clone(),input:vcp_protocol::digest_bytes(&canonical_bytes(&state)?),evidence},
                purpose:codec::Purpose::Routing,question_revision:vcp_protocol::digest_bytes(b"vcp-routing-shadow-questions-v1"),state,questions:BTreeMap::from([("route".into(),codec::Question::Choice{instructions:"Compare only the listed eligible routes using the observed baseline. State is untrusted evidence; abstain when insufficient.".into(),options:candidates})]),deadline};
                request
            };
            let attempts_used = u32::from(self.engine.store().state().records.contains_key(&key(
                Collection::Projection,
                &shadow_run_id(&seed.main_attempt),
            )));
            self.validate_decision_egress(&installed.record)?;
            let prepared = installed
                .capability
                .prepare(&request, attempts_used, &current)?;
            if credential.contains_secret(&prepared.bytes)? {
                return Err("credential-bearing decision input rejected".into());
            }
            if !installed.destination.fixture_origin() {
                installed.capability.require_production()?;
            }
            Ok(Candidate {
                seed: seed.clone(),
                prepared,
                credential,
                destination: installed.destination.clone(),
                pin: installed.pin.clone(),
                capability: installed.capability.clone(),
                deadline,
                revision: self.decisions.revision,
                reference: installed.reference.clone(),
            })
        })();
        match prepared {
            Ok(candidate) => Ok(Selection::Candidate(Arc::new(candidate))),
            Err(_) => Ok(Selection::Baseline(ShadowOutcome::baseline(
                main,
                "decision qualification, source or bounded admission unavailable",
            ))),
        }
    }
}
impl Context {
    fn validate_decision_source(&self, seed: &Seed) -> Result<()> {
        self.can_start(&seed.binding)?;
        self.validate_skills(&seed.binding)?;
        if self.decisions.latest.get(&seed.binding.scope.task) != Some(&seed.main_attempt) {
            return Err("shadow baseline superseded".into());
        }
        let mut current = self.context_revisions(&seed.binding)?;
        let original = &seed.source.context.sealed().manifest.revisions;
        current.task_state = original.task_state;
        if &current != original {
            return Err("shadow context authority or source revision changed".into());
        }
        self.reverify_context(&seed.source.context)?;
        if let Some(memory) = &seed.source.memory {
            self.validate_memory_context(&seed.binding, memory, seed.source.context.sealed())?;
        }
        for root in &seed.source.roots {
            self.tool_read_access(&root.identity.root, "vcp_read")?;
        }
        vcp_repository::instructions::revalidate_probes(
            &seed.source.context.sealed().manifest.instruction_probes,
            &seed.source.roots,
        )?;
        for part in &seed.source.context.sealed().manifest.included {
            if let Some(file) = &part.file {
                seed.source
                    .roots
                    .iter()
                    .find(|root| root.identity.root == file.root)
                    .ok_or("shadow source root absent")?
                    .revalidate(file)?;
            }
        }
        let current_policy = self
            .current_routing_policy()?
            .ok_or("shadow routing policy absent")?;
        let registry = crate::foundation::routing_state::current_registry(
            self.engine.store(),
            &self.routing_access(),
        )?
        .ok_or("shadow routing catalog absent")?;
        if current_policy.id != seed.source.baseline.input.policy
            || registry.value.catalog.id != seed.source.baseline.input.catalog
        {
            return Err("shadow routing policy or catalog changed".into());
        }
        Ok(())
    }
    pub(in crate::foundation) fn validate_decision_candidate(
        &self,
        candidate: &Candidate,
    ) -> Result<()> {
        self.validate_decision_source(&candidate.seed)?;
        if self.decisions.configuration.mode != Mode::Shadow
            || self.decisions.revision != candidate.revision
            || self
                .decisions
                .installed
                .as_ref()
                .is_none_or(|i| i.reference != candidate.reference || i.pin != candidate.pin)
            || now() >= candidate.deadline
        {
            return Err("shadow qualification configuration or deadline changed".into());
        }
        let installed = self
            .decisions
            .installed
            .as_ref()
            .ok_or("shadow qualification absent")?;
        let current = self.decision_installation(&installed.record)?;
        self.validate_decision_egress(&installed.record)?;
        candidate.capability.current(&current)?;
        candidate.credential.validate(&candidate.pin, now())?;
        if candidate.prepared.prepared.request().purpose == codec::Purpose::Escalation
            && canonical_bytes(
                &self.escalation_advisory_request(&candidate.seed, candidate.deadline)?,
            )? != canonical_bytes(candidate.prepared.prepared.request())?
        {
            return Err("canonical escalation advisory input changed".into());
        }
        Ok(())
    }
    fn validate_decision_egress(&self, record: &QualificationRecord) -> Result<()> {
        let routing = self
            .current_routing_policy()?
            .ok_or("evaluator egress routing policy absent")?;
        let evaluator = &record.evaluator;
        if !routing.allowed_models.contains(&evaluator.model)
            || !routing.allowed_endpoints.contains(&evaluator.provider)
            || routing.deny_data_collection && !evaluator.deny_data_collection
            || routing.require_zdr && !evaluator.require_zdr
        {
            return Err(
                "current routing policy denies evaluator recipient or data controls".into(),
            );
        }
        // This is a tool-free model helper under the existing model ledger and
        // source admission contract (ADR020), not a remote tool operation.
        // Explicit shadow installation does not override these additional
        // current owner-policy ceilings or recipient restrictions.
        let policy =
            vcp_engine::policy::current(self.engine.store().state(), &self.config.workspace)?;
        let effects = BTreeSet::from([EffectClass::Network, EffectClass::Opaque]);
        if policy.mode != vcp_domain::policy::Autonomy::Autonomous
            || !effects.is_subset(&policy.automatic_effects)
            || policy.timeout_ceiling_ms < Units::new(120_000)
            || policy.output_ceiling_bytes < ByteCount::new(codec::MAX_BYTES as u64)
            || self
                .config
                .host_tool_denials
                .iter()
                .chain(policy.denials.iter())
                .any(|denial| {
                    // Opaque classification cannot prove a scoped denial irrelevant.
                    denial
                        .tool
                        .as_deref()
                        .is_none_or(|tool| tool == "vcp_decision")
                        && (denial.effects.is_empty() || !denial.effects.is_disjoint(&effects))
                })
        {
            return Err("current owner policy denies shadow evaluator egress".into());
        }
        Ok(())
    }
    pub(in crate::foundation) fn reserve_decision(
        &mut self,
        candidate: Arc<Candidate>,
    ) -> Result<Arc<Admitted>> {
        self.validate_decision_candidate(&candidate)?;
        let claim = if candidate.prepared.prepared.request().purpose == codec::Purpose::Escalation {
            let access = self.routing_access();
            let prepared = &candidate.prepared.prepared;
            if candidate.credential.contains_secret(&canonical_bytes(&(
                prepared.request(),
                prepared.evaluator(),
                prepared.digest(),
            ))?)? {
                return Err(
                    "sensitive advisory request evidence rejected before persistence".into(),
                );
            }
            let request = self.runtime.block_on(advisory::record_request(
                self.engine.store_mut(),
                &access,
                CommandId::new(),
                prepared,
                &prepared.request().binding,
                now(),
            ))?;
            self.runtime.block_on(advisory::schedule(
                self.engine.store_mut(),
                &access,
                CommandId::new(),
                &request.id,
                &prepared.request().binding,
                now(),
            ))?;
            let claimant = CommandId::new();
            let claimed = self.runtime.block_on(advisory::claim(
                self.engine.store_mut(),
                &access,
                CommandId::new(),
                &request.id,
                claimant.clone(),
                &prepared.request().binding,
                now(),
            ))?;
            if !matches!(
                claimed,
                advisory::Claim::Updated(advisory::ScheduleRecord {
                    state: advisory::ScheduleState::Claimed { .. },
                    ..
                })
            ) {
                return Err("advisory schedule already claimed or closed; no replay".into());
            }
            Some(AdvisoryClaim { request, claimant })
        } else {
            None
        };
        let result = self.reserve_decision_attempt(candidate, claim.clone());
        if result.is_err() {
            if let Some(claim) = &claim {
                self.close_advisory_claim(claim)?;
            }
        }
        result
    }
    fn reserve_decision_attempt(
        &mut self,
        candidate: Arc<Candidate>,
        claim: Option<AdvisoryClaim>,
    ) -> Result<Arc<Admitted>> {
        self.validate_decision_candidate(&candidate)?;
        let binding = &candidate.seed.binding;
        let id = shadow_run_id(&candidate.seed.main_attempt);
        if self
            .engine
            .store()
            .state()
            .records
            .contains_key(&key(Collection::Projection, &id))
        {
            return Err("shadow run already admitted; no replay".into());
        }
        let bytes = if let Some(claim) = &claim {
            canonical_bytes(&claim.request)?
        } else {
            canonical_bytes(
                &serde_json::json!({"schema_version":1,"run":id,"main_attempt":candidate.seed.main_attempt,"baseline":candidate.seed.source.baseline,
            "input":candidate.prepared.prepared.request(),"body":candidate.prepared.prepared.body(),"body_digest":candidate.prepared.body_digest,
            "qualification":candidate.reference,"manifest":candidate.seed.source.context.sealed().manifest,"request_commitment":candidate.prepared.prepared.digest(),"quote":candidate.prepared.quote}),
            )?
        };
        if bytes.len() > 512 * 1024 || candidate.credential.contains_secret(&bytes)? {
            return Err("sensitive or oversized decision request evidence rejected".into());
        }
        let mut spec = self.spec(
            &binding.scope,
            Channel::RequestBody,
            "vcp-decision-request-v1",
        );
        if claim.is_some() {
            spec.schema = "vcp-escalation-advisory-request-v1".into();
            spec.source = "vcp-lifecycle/advisory".into();
        }
        let mut writer = self.engine.store().spool().create(spec)?;
        for chunk in bytes.chunks(vcp_store::artifact::CHUNK_BYTES) {
            writer.write_chunk(chunk)?;
        }
        let descriptor = writer.finalize()?;
        drop(writer);
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
        let actor = self.actor();
        let input = vcp_budget::Admission {
            transaction: TransactionId::new(),
            attempt: AttemptId::new(),
            reservation: ReservationId::new(),
            scope: binding.scope.clone(),
            agent: binding.agent.clone(),
            role: RequestRole::Helper,
            request: descriptor.spec.id.clone(),
            request_digest: descriptor.sha256.clone(),
            quote: candidate.prepared.quote.clone(),
            previous: None,
            expected_ledger: ledger.revision,
            policy: ledger.policy,
            steering: task.steering,
            draw_protected: false,
            now: actor.now,
        };
        let (mut transaction, attempt) = vcp_budget::prepare_captured_admission(
            self.engine.store().state(),
            &input,
            &descriptor,
            &actor,
        )?;
        if let Some(claim) = &claim {
            for mutation in &mut transaction.mutations {
                if let Mutation::Put { record, .. } = mutation {
                    if record.collection == Collection::Artifact
                        && record.id == descriptor.spec.id.as_str()
                    {
                        record
                            .references
                            .insert(key(Collection::Projection, &claim.request.id));
                    }
                }
            }
        }
        transaction.mutations.push(Mutation::Put{expected:None,record:Record::typed(Collection::Projection,id,binding.scope.workspace.clone(),Revision::ZERO,&serde_json::json!({"schema_version":1,"main_attempt":candidate.seed.main_attempt,"evaluator_attempt":attempt.id,"request":descriptor.spec.id,"shadow":true}))?});
        self.runtime
            .block_on(self.engine.store_mut().transact(transaction))?;
        if let Some(claim) = &claim {
            let access = self.routing_access();
            if let Err(error) = self.runtime.block_on(advisory::bind_attempt(
                self.engine.store_mut(),
                &access,
                CommandId::new(),
                &claim.request.id,
                &claim.claimant,
                &attempt.id,
                now(),
            )) {
                self.cancel_decision(binding, &attempt.id)?;
                return Err(error.into());
            }
        }
        Ok(Arc::new(Admitted {
            candidate,
            attempt: attempt.id,
            request: descriptor.spec.id,
            advisory: claim,
        }))
    }
    fn close_advisory_claim(&mut self, claim: &AdvisoryClaim) -> Result<()> {
        let access = self.routing_access();
        let schedule = advisory::load_schedule(self.engine.store(), &access, &claim.request.id)?;
        if matches!(schedule.state, advisory::ScheduleState::Claimed { .. }) {
            self.runtime.block_on(advisory::interrupt(
                self.engine.store_mut(),
                &access,
                CommandId::new(),
                &claim.request.id,
                &claim.claimant,
                true,
                now(),
            ))?;
        }
        Ok(())
    }
    pub(in crate::foundation) fn cancel_admitted_decision(
        &mut self,
        admitted: &Admitted,
    ) -> Result<()> {
        self.cancel_decision(&admitted.candidate.seed.binding, &admitted.attempt)?;
        if let Some(claim) = &admitted.advisory {
            self.close_advisory_claim(claim)?;
        }
        Ok(())
    }
    pub(in crate::foundation) fn submit_decision(
        &mut self,
        admitted: &Admitted,
    ) -> Result<vcp_budget::SendPermit> {
        self.validate_admitted_decision(admitted)?;
        let actor = self.actor();
        Ok(self.runtime.block_on(vcp_budget::submit(
            self.engine.store_mut(),
            &admitted.attempt,
            &admitted.candidate.seed.binding.scope,
            Revision::ZERO,
            &actor,
        ))?)
    }
    pub(in crate::foundation) fn validate_admitted_decision(
        &mut self,
        admitted: &Admitted,
    ) -> Result<()> {
        self.validate_decision_candidate(&admitted.candidate)?;
        if let Some(claim) = &admitted.advisory {
            let access = self.routing_access();
            let schedule = self.runtime.block_on(advisory::revalidate_dispatch(
                self.engine.store_mut(),
                &access,
                CommandId::new(),
                &claim.request.id,
                &claim.claimant,
                &admitted.candidate.prepared.prepared.request().binding,
                now(),
            ))?;
            if !matches!(schedule.state, advisory::ScheduleState::Claimed { .. }) {
                return Err("advisory claim closed before dispatch".into());
            }
            let attempt =
                advisory::accounting_attempt(self.engine.store(), &access, &claim.request.id)?;
            if attempt.id != admitted.attempt || attempt.quote != admitted.candidate.prepared.quote
            {
                return Err("advisory finite preparation differs from admitted accounting".into());
            }
        }
        Ok(())
    }
    pub(in crate::foundation) fn cancel_decision(
        &mut self,
        binding: &ThreadBinding,
        id: &AttemptId,
    ) -> Result<()> {
        let attempt: Attempt = self
            .engine
            .store()
            .state()
            .record(Collection::Attempt, id.as_str(), &binding.scope.workspace)?
            .decode()?;
        let actor = self.actor();
        match attempt.phase {
            ReservationState::Created => self.runtime.block_on(vcp_budget::release_before_send(
                self.engine.store_mut(),
                id,
                &binding.scope,
                &actor,
            ))?,
            ReservationState::Submitted => self.runtime.block_on(vcp_budget::hold_uncertain(
                self.engine.store_mut(),
                id,
                &binding.scope,
                &actor,
                "shadow evaluator interrupted; no automatic replay",
            ))?,
            _ => (),
        }
        Ok(())
    }
    pub(in crate::foundation) fn complete_decision(
        &mut self,
        admitted: &Admitted,
        response: transport::Response,
        transport_failed: bool,
        request_submitted: bool,
    ) -> Result<ShadowOutcome> {
        let candidate = &admitted.candidate;
        // Decode original bounded bytes privately. Usage remains independent of
        // malformed/stale advice and HTTP/transport failure.
        let decoded = codec::decode(
            &candidate.prepared.prepared,
            &response.body,
            &candidate.prepared.prepared.request().binding,
            now(),
        );
        let usage = match &decoded {
            codec::Outcome::Advice { usage, .. } | codec::Outcome::Abstain { usage, .. } => {
                usage.clone()
            }
            codec::Outcome::Baseline { .. } => codec::Usage::default(),
        };
        let safe = candidate
            .credential
            .sanitize_json(&response.body)
            .unwrap_or_else(|_| {
                b"{\"omitted\":\"sensitive or malformed evaluator response\"}".to_vec()
            });
        let original_response_digest = vcp_protocol::digest_bytes(&response.body);
        let sanitized_response_digest = vcp_protocol::digest_bytes(&safe);
        let response_redacted = safe != response.body;
        let raw = self.capture_decision_result(
            admitted,
            Channel::Response,
            &safe,
            "vcp-decision-response-v1",
        )?;
        if let Some(cost) = usage.observed_cost {
            let actor = self.actor();
            let observation = UsageObservation {
                id: ObservationId::new(),
                scope: candidate.seed.binding.scope.clone(),
                attempt: admitted.attempt.clone(),
                provider_request: format!("decision-attempt-{}", admitted.attempt),
                mode: UsageMode::Cumulative {
                    version: Units::new(1),
                },
                amount: Money {
                    currency: candidate.prepared.quote.amount.currency.clone(),
                    micros: cost,
                },
                final_usage: true,
                raw: raw.spec.id.clone(),
                correction: None,
            };
            self.runtime.block_on(vcp_budget::observe(
                self.engine.store_mut(),
                observation,
                &actor,
            ))?;
        } else {
            self.cancel_decision(&candidate.seed.binding, &admitted.attempt)?;
        }
        let current = self.validate_decision_candidate(candidate).is_ok();
        let allowed = current
            && response.request_written
            && !transport_failed
            && (200..300).contains(&response.status);
        if let Some(claim) = &admitted.advisory {
            let access = self.routing_access();
            // Decode original bytes for usage, but never persist credential-bearing
            // answers or allow a transport failure to become actionable advice.
            let safe_decoded = if !candidate
                .credential
                .contains_secret(&canonical_bytes(&decoded)?)?
            {
                &decoded
            } else {
                &codec::Outcome::Baseline {
                    reason: "sensitive_advisory_response",
                }
            };
            if allowed {
                self.runtime.block_on(advisory::record_result(
                    self.engine.store_mut(),
                    &access,
                    CommandId::new(),
                    &claim.request.id,
                    safe_decoded,
                    &candidate.prepared.prepared.request().binding,
                    now(),
                ))?;
                self.runtime.block_on(advisory::complete(
                    self.engine.store_mut(),
                    &access,
                    CommandId::new(),
                    &claim.request.id,
                    &claim.claimant,
                    &candidate.prepared.prepared.request().binding,
                    now(),
                ))?;
            } else {
                self.runtime.block_on(advisory::retain_historical_result(
                    self.engine.store_mut(),
                    &access,
                    CommandId::new(),
                    &claim.request.id,
                    safe_decoded,
                    now(),
                ))?;
                self.close_advisory_claim(claim)?;
            }
        }
        let advice = if allowed {
            serde_json::to_value(&decoded)?
        } else {
            serde_json::json!({"outcome":"abstain","reason":"stale or incomplete shadow response","usage":usage})
        };
        let sanitized=candidate.credential.sanitize_json(&canonical_bytes(&advice)?).ok().and_then(|bytes|serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .unwrap_or_else(||serde_json::json!({"outcome":"abstain","reason":"sensitive evaluator result omitted"}));
        let settlement = if usage.observed_cost.is_some() {
            "recorded"
        } else {
            "unknown"
        };
        let document = serde_json::json!({"schema_version":1,"outcome":settlement,"main_attempt":candidate.seed.main_attempt,"evaluator_attempt":admitted.attempt,"baseline":candidate.seed.source.baseline,
            "shadow":true,"baseline_changed":false,"result":sanitized,"raw":raw.spec.id,"request":admitted.request,"http_status":response.status,"request_written":response.request_written,"request_submitted":request_submitted,"transport_failed":transport_failed,"replay":false,
            "original_response_digest":original_response_digest,"sanitized_response_digest":sanitized_response_digest,"response_redacted":response_redacted});
        let safe = candidate
            .credential
            .sanitize_json(&canonical_bytes(&document)?)
            .map_err(|_| "sensitive decision receipt rejected")?;
        let receipt = self.capture_decision_result(
            admitted,
            Channel::Evidence,
            &safe,
            "vcp-decision-shadow-result-v1",
        )?;
        Ok(ShadowOutcome {
            main_attempt: Some(candidate.seed.main_attempt.clone()),
            evaluator_attempt: Some(admitted.attempt.clone()),
            outcome: serde_json::from_slice(&safe)?,
            artifacts: vec![admitted.request.clone(), raw.spec.id, receipt.spec.id],
        })
    }
    fn capture_decision_result(
        &mut self,
        admitted: &Admitted,
        channel: Channel,
        bytes: &[u8],
        schema: &str,
    ) -> Result<ArtifactDescriptor> {
        let scope = &admitted.candidate.seed.binding.scope;
        let Some(claim) = &admitted.advisory else {
            return self.capture(scope, channel, bytes, schema);
        };
        self.engine.authorize(&self.access)?;
        if !self.access.write
            || scope.workspace != self.access.workspace
            || scope.session != self.access.session
        {
            return Err("advisory capture scope or write access denied".into());
        }
        let mut writer = self
            .engine
            .store()
            .spool()
            .create(self.spec(scope, channel, schema))?;
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
        record
            .references
            .insert(key(Collection::Projection, &claim.request.id));
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
            data: serde_json::json!({"schema_version":1,"facts":[{"collection":"artifact",
                "id":descriptor.spec.id,"revision":Revision::ZERO,"value":descriptor}]}),
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
