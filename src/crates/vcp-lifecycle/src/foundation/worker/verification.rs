// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::foundation::verification::{ObservedCheck, VerificationConfig};
use vcp_context::manifest::Revisions;
use vcp_domain::{effect::*, verification::*};
use vcp_repository::{
    observation::{Manifest, Observation},
    Root,
};
use vcp_tools::verification::Plan;

#[derive(serde::Serialize, serde::Deserialize)]
struct Baseline {
    manifest: Manifest,
    sources: Vec<ArtifactId>,
}
pub(super) struct Setup {
    config: VerificationConfig,
    baseline: Baseline,
    baseline_artifact: ArtifactId,
    candidates: HashMap<VerificationId, Candidate>,
    latest: Option<VerificationId>,
}
struct Candidate {
    verification: Verification,
    revisions: Revisions,
    manifest: Manifest,
    prepared: Vec<Arc<vcp_tools::process::Prepared>>,
    accounting: String,
    effects: String,
}
pub(crate) struct Run {
    pub plans: Vec<Plan>,
    pub plan_artifact: ArtifactId,
    before: Observation,
    revisions: Revisions,
    citations: Vec<ArtifactId>,
    source_artifacts: Vec<ArtifactId>,
    environment: String,
}
impl Context {
    /// An accounted read-only final answer needs an observed integrity proof,
    /// not a model-invented empty check result. Called only under the quiescent
    /// completion fence; never dispatches processes or replaces failed checks.
    pub(super) fn verify_unchanged_analysis(
        &mut self,
        binding: &ThreadBinding,
        citations: Vec<ArtifactId>,
    ) -> Result<VerificationId> {
        self.can_start(binding)?;
        self.verification_paths(binding)?;
        let setup = self
            .verification
            .get(&binding.scope.task)
            .ok_or("verification is not configured")?;
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
        if setup.latest.is_some()
            || !setup.config.requirements.is_empty()
            || task.editing
            || !task.required_checks.is_empty()
        {
            return Err("configured or editing checks require observed verification".into());
        }
        let (_, observed) = self.verification_observe(binding)?;
        if observed.manifest != setup.baseline.manifest {
            return Err("changed source requires observed verification".into());
        }
        let run = self.begin_verification(binding, citations)?;
        if !run.plans.is_empty() {
            return Err("discovered checks require explicit verification".into());
        }
        Ok(self.finish_verification(binding, run, vec![])?.id)
    }
    pub(super) fn latest_verification(&self, binding: &ThreadBinding) -> Result<VerificationId> {
        self.verification
            .get(&binding.scope.task)
            .and_then(|s| s.latest.clone())
            .ok_or_else(|| "no observed verification for this owner".into())
    }
    pub(super) fn verification_paths(
        &self,
        binding: &ThreadBinding,
    ) -> Result<Vec<std::path::PathBuf>> {
        self.tool_identity(binding, "vcp_verify")?;
        let policy =
            vcp_engine::policy::current(self.engine.store().state(), &binding.scope.workspace)?;
        // A verification runner can have opaque effects. A named workflow
        // ceiling must not disappear when its process uses the vcp_exec broker.
        if self
            .config
            .host_tool_denials
            .iter()
            .chain(policy.denials.iter())
            .any(|rule| rule.tool.as_deref() == Some("vcp_verify"))
        {
            return Err("current policy denies the verification workflow".into());
        }
        let setup = self
            .verification
            .get(&binding.scope.task)
            .ok_or("verification is not configured")?;
        let (_, observed) = self.verification_observe(binding)?;
        let mut paths: Vec<_> = observed
            .manifest
            .files
            .iter()
            .map(|f| {
                std::path::Path::new(&f.path)
                    .parent()
                    .unwrap_or(std::path::Path::new(""))
                    .join(".vcp-context-scope")
            })
            .collect();
        paths.extend(
            setup
                .config
                .requirements
                .iter()
                .map(|r| std::path::PathBuf::from(&r.manifest)),
        );
        if paths.is_empty() {
            paths.push("AGENTS.md".into());
        }
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
    fn verification_effects(&self, binding: &ThreadBinding) -> Result<String> {
        let effects = self
            .engine
            .store()
            .state()
            .records
            .values()
            .filter(|r| r.collection == Collection::Effect)
            .map(Record::decode::<Effect>)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|e| e.scope.workspace == binding.scope.workspace)
            .collect::<Vec<_>>();
        Ok(vcp_protocol::digest_bytes(&canonical_bytes(&effects)?))
    }
    pub(super) fn continuity_facts(
        &self,
        binding: &ThreadBinding,
    ) -> Result<(serde_json::Value, Manifest)> {
        let setup = self
            .verification
            .get(&binding.scope.task)
            .ok_or("continuity requires an owner verification baseline")?;
        self.verification_bytes(&binding.scope, &setup.baseline_artifact)?;
        for source in &setup.baseline.sources {
            self.verification_bytes(&binding.scope, source)?;
        }
        let (_, current) = self.verification_observe(binding)?;
        let records = &self.engine.store().state().records;
        let effects = records
            .values()
            .filter(|r| r.collection == Collection::Effect)
            .map(Record::decode::<Effect>)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|e| e.scope.workspace == binding.scope.workspace)
            .collect::<Vec<_>>();
        let attempts = records
            .values()
            .filter(|r| r.collection == Collection::Attempt)
            .map(Record::decode::<Attempt>)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|a| a.root == self.config.root_task)
            .collect::<Vec<_>>();
        let mut uncertain_attempts = Vec::new();
        for attempt in attempts.iter().filter(|a| {
            !matches!(
                a.phase,
                ReservationState::Settled
                    | ReservationState::Released
                    | ReservationState::ExplicitlyResolved
            )
        }) {
            let reservation: Reservation = self
                .engine
                .store()
                .state()
                .record(
                    Collection::Reservation,
                    attempt.reservation.as_str(),
                    &attempt.scope.workspace,
                )?
                .decode()?;
            uncertain_attempts.push(serde_json::json!({
                "id":attempt.id,"task":attempt.scope.task,"role":attempt.role,"phase":attempt.phase,
                "reservation":attempt.reservation,"request":attempt.request,"previous":attempt.previous,
                "provider_request":attempt.provider_request,"quoted_reserve":attempt.quote.amount,
                "charged":attempt.charged,"held_liability":reservation.liability,"uncertainty":attempt.uncertain,
                "source_records_sha256":vcp_protocol::digest_bytes(&canonical_bytes(&(attempt,&reservation))?)
            }));
        }
        let effect_summaries = effects
            .iter()
            .map(|e| {
                serde_json::json!({
                    "id":e.id,"task":e.scope.task,"state":e.state,"exit_code":e.exit_code
                })
            })
            .collect::<Vec<_>>();
        let unresolved_effects = effects
            .iter()
            .filter(|e| {
                !matches!(
                    e.state,
                    EffectState::Succeeded | EffectState::Failed | EffectState::Cancelled
                )
            })
            .collect::<Vec<_>>();
        let stopped_effects = effects
            .iter()
            .filter(|e| matches!(e.state, EffectState::Failed | EffectState::Cancelled))
            .collect::<Vec<_>>();
        let ledger = records
            .values()
            .find(|r| r.collection == Collection::Ledger && r.id == self.config.root_task.as_str())
            .map(Record::decode::<Ledger>)
            .transpose()?;
        let checks = records
            .values()
            .filter(|r| r.collection == Collection::Verification)
            .map(Record::decode::<Verification>)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|v| v.scope == binding.scope)
            .collect::<Vec<_>>();
        for report in &checks {
            for artifact in report
                .outputs
                .iter()
                .chain(report.checks.iter().map(|c| &c.output))
            {
                self.verification_bytes(&binding.scope, artifact)?;
            }
        }
        let facts = serde_json::json!({
            "schema":"canonical-current-continuity/1",
            "meaning":"Current observed state kept outside historical summaries. Check applicability must be revalidated before completion.",
            "original_base":setup.baseline.manifest,"base_artifact":setup.baseline_artifact,
            "current_source":current.manifest,"effects":effect_summaries,"unresolved_effects":unresolved_effects,"stopped_effects":stopped_effects,
            "effect_history_digest":vcp_protocol::digest_bytes(&canonical_bytes(&effects)?),
            "uncertain_attempts":uncertain_attempts,"attempt_count":attempts.len(),
            "attempt_history_digest":vcp_protocol::digest_bytes(&canonical_bytes(&attempts)?),
            "ledger":ledger,"verification_records":checks
        });
        Ok((facts, current.manifest))
    }
    /// Capture the actual destination-side bytes as well as the original base.
    /// Reuse complete same-task captures by hash; every returned dependency is
    /// read again under current history access before the packet is published.
    pub(super) fn handoff_workspace(
        &mut self,
        binding: &ThreadBinding,
    ) -> Result<(serde_json::Value, Vec<ArtifactId>)> {
        let setup = self
            .verification
            .get(&binding.scope.task)
            .ok_or("verification baseline missing")?;
        let base = setup.baseline.manifest.clone();
        let mut references = setup.baseline.sources.clone();
        references.push(setup.baseline_artifact.clone());
        let (_, observed) = self.verification_observe(binding)?;
        let existing: Vec<ArtifactDescriptor> = self
            .engine
            .store()
            .state()
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
            .map(Record::decode)
            .collect::<std::result::Result<_, _>>()?;
        let mut current = Vec::new();
        for source in &observed.sources {
            let id = if let Some(artifact) = existing.iter().find(|a| {
                a.spec.scope == binding.scope
                    && a.state == CaptureState::Complete
                    && a.sha256 == source.version.sha256
                    && a.length == source.version.bytes
            }) {
                artifact.spec.id.clone()
            } else {
                self.capture(
                    &binding.scope,
                    Channel::Evidence,
                    &source.bytes,
                    "handoff-source/1",
                )?
                .spec
                .id
            };
            current.push(serde_json::json!({"version":source.version,"artifact":id}));
            references.push(id);
        }
        let changed = base
            .files
            .iter()
            .chain(observed.manifest.files.iter())
            .filter(|file| !base.files.contains(file) || !observed.manifest.files.contains(file))
            .map(|file| file.path.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let delta = changed
            .iter()
            .map(|path| {
                serde_json::json!({"path":path,
            "before":base.files.iter().find(|f| &f.path == path),
            "after":observed.manifest.files.iter().find(|f| &f.path == path)})
            })
            .collect::<Vec<_>>();
        Ok((
            serde_json::json!({"base":base,"current":observed.manifest,"sources":current,
            "changes":delta,"meaning":"Observed before/after file versions and captured bytes; authorship is not inferred"}),
            references,
        ))
    }
    pub fn check_verification_command(
        &self,
        command: &Command,
        _task: Option<&TaskId>,
    ) -> Result<()> {
        if matches!(
            command,
            Command::Transition {
                next: TaskState::Completed,
                ..
            }
        ) {
            return Err("native host tasks complete through current verification evidence".into());
        }
        Ok(())
    }
    fn verification_bytes(&self, scope: &Scope, id: &ArtifactId) -> Result<Vec<u8>> {
        let artifact: ArtifactDescriptor = self
            .engine
            .store()
            .state()
            .record(Collection::Artifact, id.as_str(), &scope.workspace)?
            .decode()?;
        if artifact.spec.scope != *scope || artifact.state != CaptureState::Complete {
            return Err("verification citation scope or completeness rejected".into());
        }
        let mut bytes = Vec::new();
        vcp_audit::history::History::read_artifact(
            self.engine.store(),
            &self.history_access(),
            id,
            &mut bytes,
        )?;
        Ok(bytes)
    }
    pub(super) fn verification_observe(
        &self,
        binding: &ThreadBinding,
    ) -> Result<(Root, Observation)> {
        self.verification_observe_setup(binding, false)
    }
    fn verification_observe_setup(
        &self,
        binding: &ThreadBinding,
        held: bool,
    ) -> Result<(Root, Observation)> {
        self.child_context_scope(binding)?;
        // Match every native coding read ceiling before opening the root.
        if held {
            self.child_held_setup_access(binding)?;
            self.tool_read_access(
                &RootId::parse(binding.scope.workspace.as_str())?,
                "vcp_exec",
            )?;
        } else {
            self.tool_identity(binding, "vcp_exec")?;
        }
        let root_id = RootId::parse(binding.scope.workspace.as_str())?;
        for tool in ["vcp_read", "vcp_list", "vcp_search", "vcp_patch"] {
            self.tool_read_access(&root_id, tool)?;
        }
        let root = self.task_root(&binding.scope.task)?;
        let mut observed = self
            .runtime
            .block_on(root.observe(None, &vcp_repository::discovery::Limits::default()))?;
        let mut affected: Vec<_> = observed
            .manifest
            .files
            .iter()
            .map(|f| std::path::PathBuf::from(&f.path))
            .collect();
        if affected.is_empty() {
            affected.push("AGENTS.md".into());
        }
        let mut extra_bytes = 0usize;
        let parents = self.instruction_parents(binding)?;
        for group in affected.chunks(256) {
            let instructions = root.instructions(group, &parents, 256 * 1024)?;
            // The manifest's absence/version probes also fence instruction files
            // ignored by ordinary discovery and explicit parent grants.
            for probe in instructions.probes {
                if !observed.manifest.ignore_dependencies.contains(&probe) {
                    observed.manifest.ignore_dependencies.push(probe);
                }
            }
            for document in instructions.documents {
                if !observed.sources.iter().any(|s| {
                    s.version.root == document.source.version.root
                        && s.version.path == document.source.version.path
                }) {
                    extra_bytes += document.source.bytes.len();
                    if extra_bytes > 256 * 1024 {
                        return Err("verification instruction capture ceiling".into());
                    }
                    observed.sources.push(document.source);
                }
            }
        }
        observed.sources.sort_by(|a, b| {
            (&a.version.root, &a.version.path).cmp(&(&b.version.root, &b.version.path))
        });
        // Parent versions live in instruction probes; they are not ordinary
        // workspace files or additional check-discovery/search roots.
        observed.manifest.files = observed
            .sources
            .iter()
            .filter(|s| s.version.root == root.identity.root)
            .map(|s| s.version.clone())
            .collect();
        observed
            .manifest
            .ignore_dependencies
            .sort_by(|a, b| (&a.root, &a.path).cmp(&(&b.root, &b.path)));
        let mut roots = parents;
        roots.push(root.clone());
        vcp_repository::instructions::revalidate_probes(
            &observed.manifest.ignore_dependencies,
            &roots,
        )?;
        observed.digest = vcp_protocol::digest_bytes(&canonical_bytes(&observed.manifest)?);
        Ok((root, observed))
    }
    fn verification_sources(
        &mut self,
        scope: &Scope,
        observation: &Observation,
    ) -> Result<Vec<ArtifactId>> {
        observation
            .sources
            .iter()
            .map(|source| {
                Ok(self
                    .capture(
                        scope,
                        Channel::Evidence,
                        &source.bytes,
                        "verification-source/1",
                    )?
                    .spec
                    .id)
            })
            .collect()
    }
    pub fn configure_verification(
        &mut self,
        binding: &ThreadBinding,
        config: VerificationConfig,
    ) -> Result<()> {
        self.configure_verification_setup(binding, config, false)
    }
    pub(super) fn parent_verification_config(
        &self,
        binding: &ThreadBinding,
    ) -> Result<VerificationConfig> {
        self.verification
            .get(&binding.scope.task)
            .map(|state| state.config.clone())
            .ok_or_else(|| "parent verification is not configured".into())
    }
    pub(super) fn configure_verification_setup(
        &mut self,
        binding: &ThreadBinding,
        config: VerificationConfig,
        held: bool,
    ) -> Result<()> {
        if held {
            self.child_held_setup_access(binding)?;
        } else {
            self.can_start(binding)?;
        }
        if self.verification.contains_key(&binding.scope.task)
            || self.task_has_streams(binding)?
            || config.rationale.trim().is_empty()
            || config.rationale.len() > 4096
        {
            return Err("verification setup must be bounded, fresh and idle".into());
        }
        let (_, observed) = self.verification_observe_setup(binding, held)?;
        vcp_tools::verification::discover(&observed, &config.requirements)?;
        for requirement in &config.requirements {
            if let Some(profile) = self.process_profiles.get(&requirement.profile) {
                if profile.mode() != vcp_tools::process::Mode::Direct
                    || profile.terminal().is_some()
                {
                    return Err("verification runner requires a direct nonterminal profile".into());
                }
            }
        }
        let saved = self
            .engine
            .store()
            .state()
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
            .map(Record::decode::<ArtifactDescriptor>)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|a| a.spec.scope == binding.scope && a.spec.schema == "verification-baseline/1")
            .collect::<Vec<_>>();
        let (baseline, baseline_artifact) = match saved.as_slice() {
            [] => {
                if self
                    .engine
                    .store()
                    .state()
                    .records
                    .values()
                    .filter(|r| r.collection == Collection::Effect)
                    .map(Record::decode::<Effect>)
                    .collect::<std::result::Result<Vec<_>, _>>()?
                    .iter()
                    .any(|e| e.scope == binding.scope)
                {
                    return Err("initial verification baseline must precede task effects".into());
                }
                let baseline = Baseline {
                    manifest: observed.manifest.clone(),
                    sources: self.verification_sources(&binding.scope, &observed)?,
                };
                let artifact = self
                    .capture(
                        &binding.scope,
                        Channel::Evidence,
                        &canonical_bytes(&baseline)?,
                        "verification-baseline/1",
                    )?
                    .spec
                    .id;
                (baseline, artifact)
            }
            [saved] => {
                let baseline: Baseline = serde_json::from_slice(
                    &self.verification_bytes(&binding.scope, &saved.spec.id)?,
                )?;
                for id in &baseline.sources {
                    self.verification_bytes(&binding.scope, id)?;
                }
                (baseline, saved.spec.id.clone())
            }
            _ => return Err("ambiguous original verification baseline".into()),
        };
        self.capture(
            &binding.scope,
            Channel::Evidence,
            &canonical_bytes(&config)?,
            "verification-configuration/1",
        )?;
        self.verification.insert(
            binding.scope.task.clone(),
            Setup {
                config,
                baseline,
                baseline_artifact,
                candidates: HashMap::new(),
                latest: None,
            },
        );
        Ok(())
    }
    pub fn begin_verification(
        &mut self,
        binding: &ThreadBinding,
        citations: Vec<ArtifactId>,
    ) -> Result<Run> {
        if citations.len() > 256 {
            return Err("verification citation ceiling".into());
        }
        let revisions = self.context_revisions(binding)?;
        for id in &citations {
            self.verification_bytes(&binding.scope, id)?;
        }
        let (_, before) = self.verification_observe(binding)?;
        let setup = self
            .verification
            .get(&binding.scope.task)
            .ok_or("verification is not configured")?;
        let plans = vcp_tools::verification::discover(&before, &setup.config.requirements)?;
        let profiles: Vec<_> = plans
            .iter()
            .map(|plan| {
                (
                    plan.request.profile.clone(),
                    self.process_profiles.get(&plan.request.profile),
                )
            })
            .collect();
        let environment = vcp_protocol::digest_bytes(&canonical_bytes(&profiles)?);
        let source_artifacts = self.verification_sources(&binding.scope, &before)?;
        let plan_artifact = self.capture(&binding.scope,Channel::Evidence,&canonical_bytes(&serde_json::json!({
            "plans":plans,"revisions":revisions,"before":before.manifest,"source_artifacts":source_artifacts,"environment":environment
        }))?,"verification-plan/1")?.spec.id;
        Ok(Run {
            plans,
            plan_artifact,
            before,
            revisions,
            citations,
            source_artifacts,
            environment,
        })
    }
    pub fn verification_check_intent(
        &mut self,
        binding: &ThreadBinding,
        plan: ArtifactId,
        effect: ToolRunId,
        index: usize,
    ) -> Result<ArtifactId> {
        self.can_start(binding)?;
        self.verification_bytes(&binding.scope, &plan)?;
        Ok(self
            .capture(
                &binding.scope,
                Channel::Evidence,
                &canonical_bytes(&serde_json::json!({"plan":plan,"check":index,"effect":effect}))?,
                "verification-check-intent/1",
            )?
            .spec
            .id)
    }
    fn verification_ledger(&self) -> Result<Option<Ledger>> {
        self.engine
            .store()
            .state()
            .records
            .values()
            .find(|r| r.collection == Collection::Ledger && r.id == self.config.root_task.as_str())
            .map(Record::decode::<Ledger>)
            .transpose()
            .map_err(Into::into)
    }
    fn verification_accounting(&self) -> Result<(CostCertainty, String)> {
        let rows: Vec<Attempt> = self
            .engine
            .store()
            .state()
            .records
            .values()
            .filter(|r| r.collection == Collection::Attempt)
            .map(Record::decode::<Attempt>)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|a| a.root == self.config.root_task)
            .collect();
        let uncertain: Vec<_> = rows
            .iter()
            .filter(|a| {
                !matches!(
                    a.phase,
                    ReservationState::Settled
                        | ReservationState::Released
                        | ReservationState::ExplicitlyResolved
                )
            })
            .map(|a| a.id.clone())
            .collect();
        let cost = if uncertain.is_empty() {
            CostCertainty::Known
        } else {
            CostCertainty::Uncertain {
                attempts: uncertain,
                reason: "canonical accounting has outstanding reservations or reconciliation"
                    .into(),
            }
        };
        Ok((
            cost,
            vcp_protocol::digest_bytes(&canonical_bytes(&(rows, self.verification_ledger()?))?),
        ))
    }
    pub fn finish_verification(
        &mut self,
        binding: &ThreadBinding,
        run: Run,
        observed: Vec<ObservedCheck>,
    ) -> Result<Verification> {
        let setup = self
            .verification
            .get(&binding.scope.task)
            .ok_or("verification setup lost")?;
        let baseline_artifact = setup.baseline_artifact.clone();
        let baseline = setup.baseline.manifest.clone();
        let baseline_sources = setup.baseline.sources.clone();
        let current_revisions = self.context_revisions(binding);
        let after = self.verification_observe(binding);
        let mut issues = Vec::new();
        let (editor_buffers, buffers_unverified) = self.editor_verification_buffers(binding)?;
        let fresh = current_revisions
            .as_ref()
            .is_ok_and(|r| *r == run.revisions)
            && after
                .as_ref()
                .is_ok_and(|(_, after)| after.manifest == run.before.manifest)
            && !buffers_unverified;
        if buffers_unverified {
            issues.push("checks covered native disk only; dirty or uncertain editor buffers require reconciliation".into());
        }
        if !fresh {
            issues.push(
                "check evidence is stale: source, task or authority changed during verification"
                    .into(),
            );
        }
        if !baseline.bounded_scan_complete || !run.before.manifest.bounded_scan_complete {
            issues.push("verification source scope is incomplete".into());
        }
        // Exclusions are explicit; no claim is made about excluded dependencies.
        // A new exclusion or disappearance cannot silently become analysis-only.
        let changed: Vec<_> = baseline
            .files
            .iter()
            .chain(run.before.manifest.files.iter())
            .filter(|file| {
                !baseline.files.contains(file) || !run.before.manifest.files.contains(file)
            })
            .map(|file| file.path.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
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
        let editing = task.editing || baseline != run.before.manifest;
        if editing && observed.is_empty() {
            issues.push("changed work requires discovered checks; an initial analysis flag cannot bypass verification".into());
        }
        if !editing && run.citations.is_empty() {
            issues.push("analysis completion requires cited evidence; call vcp_verify again with relevant artifact IDs from the evidence field of successful read, list or search results".into());
        }
        for path in &changed {
            if !run.plans.iter().any(|plan| {
                plan.directory.is_empty() || path.starts_with(&format!("{}/", plan.directory))
            }) {
                issues.push(format!(
                    "changed path is outside every configured check project: {path}"
                ));
            }
        }
        let mut outputs = run.citations;
        outputs.push(run.plan_artifact);
        outputs.extend(run.source_artifacts);
        outputs.extend(baseline_sources);
        outputs.push(baseline_artifact);
        let mut checks = Vec::new();
        let mut prepared = Vec::new();
        for check in observed {
            let receipt = self.capture(&binding.scope, Channel::Evidence, &canonical_bytes(&serde_json::json!({
                "plan":check.plan,"effect":check.effect,"outcome":check.outcome,"exit_code":check.exit_code,"artifacts":check.artifacts,
                "applicability":if fresh {"current"} else {"stale"},
                "native_preparation":check.prepared.as_ref().map(|p| p.evidence()).transpose()?.map(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes)).transpose()?
            }))?, "verification-check/1")?;
            outputs.extend(check.artifacts);
            checks.push(Check {
                specification: check.plan.specification,
                outcome: check.outcome,
                output: receipt.spec.id,
                exit_code: check.exit_code,
            });
            if let Some(plan) = check.prepared {
                prepared.push(plan);
            }
        }
        let effects: Vec<Effect> = self
            .engine
            .store()
            .state()
            .records
            .values()
            .filter(|r| r.collection == Collection::Effect)
            .map(Record::decode::<Effect>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let unresolved_effects = effects
            .into_iter()
            .filter(|e| {
                e.scope.workspace == binding.scope.workspace
                    && !matches!(
                        e.state,
                        EffectState::Succeeded | EffectState::Failed | EffectState::Cancelled
                    )
            })
            .map(|e| e.id)
            .collect();
        let (cost, accounting) = self.verification_accounting()?;
        let report = self.capture(&binding.scope, Channel::Evidence, &canonical_bytes(&serde_json::json!({
            "before":run.before.manifest,"after":after.as_ref().ok().map(|(_,o)| &o.manifest),
            "observation_error":after.as_ref().err().map(ToString::to_string),"changed_paths":changed,"editing":editing,
            "accepted_revisions":run.revisions,"cost":cost,"accounting_digest":accounting,"ledger":self.verification_ledger()?,
            "scope":"bounded native disk sources; editor buffers are not executed; excluded files and external dynamic dependencies are not qualified",
            "representation":"disk_only", "editor_buffers_unverified":buffers_unverified,
            "editor_buffers_fingerprint":editor_buffers,
            "source_artifacts":outputs,"applicability":if fresh {"current"} else {"stale"}
        }))?, "verification-result/1")?;
        outputs.push(report.spec.id);
        outputs.sort();
        outputs.dedup();
        let fingerprint = Fingerprint {
            repository: run.before.digest,
            buffers: editor_buffers,
            environment: run.environment,
        };
        let verification = Verification {
            redaction: None,
            id: VerificationId::new(),
            scope: binding.scope.clone(),
            steering: run.revisions.steering,
            fingerprint: fingerprint.clone(),
            outputs,
            checks,
            unresolved_effects,
            outstanding_issues: issues,
            cost,
        };
        // Re-observing identical native inputs is not a task change. Advancing
        // revision here would invalidate otherwise current evidence and split
        // exact repeated checks into unrelated source revisions.
        let expected = if fresh && task.fingerprint != fingerprint {
            self.command(
                Command::ObserveFingerprint { fingerprint },
                Some(binding.scope.task.clone()),
                task.revision,
            )?;
            task.revision.next()?
        } else {
            task.revision
        };
        self.command(
            Command::RecordVerification {
                verification: verification.clone(),
            },
            Some(binding.scope.task.clone()),
            expected,
        )?;
        self.verification
            .get_mut(&binding.scope.task)
            .unwrap()
            .latest = Some(verification.id.clone());
        if fresh {
            let revisions = self.context_revisions(binding)?;
            let effects = self.verification_effects(binding)?;
            self.verification
                .get_mut(&binding.scope.task)
                .unwrap()
                .candidates
                .insert(
                    verification.id.clone(),
                    Candidate {
                        verification: verification.clone(),
                        revisions,
                        manifest: run.before.manifest,
                        prepared,
                        accounting,
                        effects,
                    },
                );
        }
        Ok(verification)
    }
    pub fn complete_verified(
        &mut self,
        binding: &ThreadBinding,
        id: &VerificationId,
    ) -> Result<CommandReceipt> {
        let revisions = self.context_revisions(binding)?;
        if self.editor_verification_buffers(binding)?.1 {
            return Err(
                "disk-only evidence cannot complete a task with dirty or uncertain editor buffers"
                    .into(),
            );
        }
        // Effects in another task do not necessarily advance this task's
        // revision. Recheck the entire canonical workspace at the commit fence.
        for record in self
            .engine
            .store()
            .state()
            .records
            .values()
            .filter(|r| r.collection == Collection::Effect)
        {
            let effect: Effect = record.decode()?;
            if effect.scope.workspace == binding.scope.workspace
                && !matches!(
                    effect.state,
                    EffectState::Succeeded | EffectState::Failed | EffectState::Cancelled
                )
            {
                return Err(
                    "current workspace effects require reconciliation before completion".into(),
                );
            }
        }
        let candidate = self
            .verification
            .get(&binding.scope.task)
            .and_then(|s| s.candidates.get(id))
            .ok_or("no current owner verification capability; verify again")?;
        if revisions != candidate.revisions
            || self.verification_effects(binding)? != candidate.effects
        {
            return Err(
                "completion evidence is stale after task, authority or effect changes".into(),
            );
        }
        let (root, current) = self.verification_observe(binding)?;
        if current.manifest != candidate.manifest {
            return Err("completion evidence is stale after a source/configuration edit".into());
        }
        let mut roots = self.instruction_parents(binding)?;
        roots.push(root);
        let _sources = current
            .sources
            .iter()
            .map(|source| {
                roots
                    .iter()
                    .find(|r| r.identity.root == source.version.root)
                    .ok_or_else(|| {
                        vcp_repository::Error::Scope("verification source root missing".into())
                    })?
                    .pin_version(&source.version)
            })
            .collect::<vcp_repository::Result<Vec<_>>>()?;
        let _processes = candidate
            .prepared
            .iter()
            .map(|p| p.pin())
            .collect::<vcp_tools::Result<Vec<_>>>()?;
        // Existing source bytes stay deny-write/delete pinned through the durable
        // transition. Recheck directory membership/ignore probes under those pins.
        let (_, pinned) = self.verification_observe(binding)?;
        if pinned.manifest != candidate.manifest {
            return Err("completion source membership changed while pinning".into());
        }
        for id in candidate
            .verification
            .outputs
            .iter()
            .chain(candidate.verification.checks.iter().map(|c| &c.output))
        {
            self.verification_bytes(&binding.scope, id)?;
        }
        if !candidate.verification.outstanding_issues.is_empty() {
            return Err("verification has unresolved completion issues".into());
        }
        let required: Vec<_> = self.verification[&binding.scope.task]
            .config
            .requirements
            .iter()
            .map(|r| format!("{}#test", r.manifest))
            .collect();
        if !candidate
            .verification
            .satisfies(&required, !required.is_empty())
        {
            return Err("required verification did not pass".into());
        }
        let mut evidence = candidate.verification.id.clone();
        let (cost, accounting) = self.verification_accounting()?;
        if accounting != candidate.accounting {
            // The final accounted prose response does not change tested source.
            // Preserve the original report and capture a new current-cost record.
            let mut refreshed = candidate.verification.clone();
            refreshed.id = VerificationId::new();
            refreshed.cost = cost;
            let receipt = self.capture(&binding.scope, Channel::Evidence,
                &canonical_bytes(&serde_json::json!({"previous_verification":evidence,"current_accounting_digest":accounting,"cost":refreshed.cost,"ledger":self.verification_ledger()?,"accepted_revisions":revisions}))?,
                "verification-completion-refresh/1")?;
            refreshed.outputs.push(receipt.spec.id);
            evidence = refreshed.id.clone();
            self.command(
                Command::RecordVerification {
                    verification: refreshed,
                },
                Some(binding.scope.task.clone()),
                revisions.task_state,
            )?;
        }
        self.command(
            Command::Transition {
                next: TaskState::Completed,
                reason: "current native sources, acceptance checks and canonical receipts verified"
                    .into(),
                verification: Some(evidence),
            },
            Some(binding.scope.task.clone()),
            revisions.task_state,
        )
    }
}
