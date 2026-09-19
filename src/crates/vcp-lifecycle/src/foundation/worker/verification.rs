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
}
struct Candidate {
    verification: Verification,
    revisions: Revisions,
    manifest: Manifest,
    prepared: Vec<Arc<vcp_tools::process::Prepared>>,
    accounting: String,
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
    fn verification_observe(&self, binding: &ThreadBinding) -> Result<(Root, Observation)> {
        // Match every native coding read ceiling before opening the root.
        self.tool_identity(binding, "vcp_exec")?;
        let root_id = RootId::parse(binding.scope.workspace.as_str())?;
        for tool in ["vcp_read", "vcp_list", "vcp_search", "vcp_patch"] {
            self.tool_read_access(&root_id, tool)?;
        }
        let root = self.tool_root()?;
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
        for group in affected.chunks(256) {
            let instructions = root.instructions(group, &[], 256 * 1024)?;
            // The manifest's absence/version probes also fence instruction files
            // ignored by ordinary discovery. No parent grant is inferred.
            for probe in instructions.probes {
                if !observed.manifest.ignore_dependencies.contains(&probe) {
                    observed.manifest.ignore_dependencies.push(probe);
                }
            }
            for document in instructions.documents {
                if !observed
                    .sources
                    .iter()
                    .any(|s| s.version.path == document.source.version.path)
                {
                    extra_bytes += document.source.bytes.len();
                    if extra_bytes > 256 * 1024 {
                        return Err("verification instruction capture ceiling".into());
                    }
                    observed.sources.push(document.source);
                }
            }
        }
        observed
            .sources
            .sort_by(|a, b| a.version.path.cmp(&b.version.path));
        observed.manifest.files = observed.sources.iter().map(|s| s.version.clone()).collect();
        observed
            .manifest
            .ignore_dependencies
            .sort_by(|a, b| a.path.cmp(&b.path));
        vcp_repository::instructions::revalidate_probes(
            &observed.manifest.ignore_dependencies,
            std::slice::from_ref(&root),
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
        self.can_start(binding)?;
        if self.verification.contains_key(&binding.scope.task)
            || !self.streams.is_empty()
            || config.rationale.trim().is_empty()
            || config.rationale.len() > 4096
        {
            return Err("verification setup must be bounded, fresh and idle".into());
        }
        let (_, observed) = self.verification_observe(binding)?;
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
        Ok((cost, vcp_protocol::digest_bytes(&canonical_bytes(&rows)?)))
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
        let fresh = current_revisions
            .as_ref()
            .is_ok_and(|r| *r == run.revisions)
            && after
                .as_ref()
                .is_ok_and(|(_, after)| after.manifest == run.before.manifest);
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
            issues.push("analysis completion requires cited evidence".into());
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
            "accepted_revisions":run.revisions,"cost":cost,"accounting_digest":accounting,
            "scope":"bounded native disk sources; no editor buffers; excluded files and external dynamic dependencies are not qualified",
            "source_artifacts":outputs,"applicability":if fresh {"current"} else {"stale"}
        }))?, "verification-result/1")?;
        outputs.push(report.spec.id);
        outputs.sort();
        outputs.dedup();
        let fingerprint = Fingerprint {
            repository: run.before.digest,
            buffers: vcp_protocol::digest_bytes(b"no-editor-buffers/native-cli-v1"),
            environment: run.environment,
        };
        let verification = Verification {
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
        let expected = if fresh {
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
        if fresh {
            let revisions = self.context_revisions(binding)?;
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
            || self.verification_accounting()?.1 != candidate.accounting
        {
            return Err(
                "completion evidence is stale after task, authority or accounting changes".into(),
            );
        }
        let (root, current) = self.verification_observe(binding)?;
        if current.manifest != candidate.manifest {
            return Err("completion evidence is stale after a source/configuration edit".into());
        }
        let _sources = current
            .manifest
            .files
            .iter()
            .map(|f| root.pin_version(f))
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
        let evidence = candidate.verification.id.clone();
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
