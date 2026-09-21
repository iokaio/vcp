// SPDX-License-Identifier: Apache-2.0
//! Observation-only recovery. Neither absence of a receipt nor a PID permits
//! redispatch or termination. Evidence is scoped to the recorded execution.
use super::*;
use serde_json::{json, Value};
use vcp_domain::effect::{Effect, EffectState};

impl super::super::CanonicalHost {
    /// Reinspect pending canonical effects while paused, without scheduling work.
    /// The returned artifacts explain per-resource certainty and retained unknowns.
    pub fn reconcile_effects(&self) -> std::result::Result<Vec<ArtifactDescriptor>, String> {
        let runtime = self.runtime.clone();
        let scheduler = self.scheduler.clone();
        self.worker.run_cleanup(move |context| {
            let tasks: Vec<Task> = context
                .engine
                .store()
                .state()
                .records
                .values()
                .filter(|row| row.collection == Collection::Task)
                .map(Record::decode)
                .collect::<std::result::Result<_, _>>()?;
            if tasks.iter().any(|task| task.state == TaskState::Running) {
                return Err("pause all tasks before recovery observations".into());
            }
            // A paused task can still have a draining producer. Its receipt
            // owns terminalization until the scheduler/work permit is released.
            // Scanning sooner could settle the effect ahead of its late result.
            {
                let state = runtime.0.state.lock().map_err(|_| "lifecycle poisoned")?;
                if scheduler.busy()
                    || state.startups_in_flight != 0
                    || state.entries.values().any(|entry| entry.starts != 0)
                    || state.work.iter().any(|work| work.receipt.is_none())
                {
                    return Err(
                        "recovery waits for scheduler and result producers to become quiescent"
                            .into(),
                    );
                }
            }
            #[cfg(windows)]
            for jobs in runtime
                .0
                .jobs
                .lock()
                .map_err(|_| "process registry poisoned")?
                .values()
            {
                for job in jobs {
                    if job.active_process_count()? != 0 {
                        return Err("owned processes must stop before file reconciliation".into());
                    }
                }
            }
            #[cfg(not(windows))]
            let _ = runtime;
            context.reconcile_effects()
        })
    }
}

#[cfg(all(test, windows))]
#[path = "recovery_tests.rs"]
mod tests;

impl Context {
    pub(super) fn revalidate_resume_environment(&mut self, binding: &ThreadBinding) -> Result<()> {
        if !self.owner_alive {
            return Err("owner is closed; reopen before resume".into());
        }
        if self.provider_required {
            let provider = self
                .provider
                .as_ref()
                .ok_or("resume requires current provider configuration")?;
            provider.snapshot.current(now())?;
            #[cfg(windows)]
            {
                let workspace: Workspace = self
                    .engine
                    .store()
                    .state()
                    .record(
                        Collection::Workspace,
                        self.config.workspace.as_str(),
                        &self.config.workspace,
                    )?
                    .decode()?;
                if workspace.trust != Trust::Trusted {
                    return Err("resume requires current workspace trust".into());
                }
                self.child_context_scope(binding)?;
                let root = self.task_root(&binding.scope.task)?;
                self.tool_read_access(&root.identity.root, "vcp_read")?;
                let scan = root.discover(&vcp_repository::discovery::Limits::default())?;
                if !scan.complete {
                    return Err("resume repository observation is incomplete".into());
                }
                let mut affected: Vec<_> = scan
                    .sources
                    .iter()
                    .map(|source| std::path::PathBuf::from(&source.version.path))
                    .collect();
                if affected.is_empty() {
                    affected.push(std::path::PathBuf::from("AGENTS.md"));
                }
                let parents = self.instruction_parents(binding)?;
                let mut probes = Vec::new();
                for chunk in affected.chunks(256) {
                    for probe in root.instructions(chunk, &parents, 256 * 1024)?.probes {
                        if !probes.contains(&probe) {
                            probes.push(probe);
                        }
                    }
                }
                self.capture(&binding.scope,Channel::Evidence,&canonical_bytes(&json!({"schema_version":1,
                    "sources":scan.sources.iter().map(|source|&source.version).collect::<Vec<_>>(),
                    "instruction_probes":probes,"exclusions":scan.exclusions,
                    "provider":"current catalog checked; fresh captured context required at next send"}))?,"vcp-resume-observation-v1")?;
            }
        }
        #[cfg(not(windows))]
        let _ = binding;
        Ok(())
    }
    fn recovery_artifact(&self, artifact: &ArtifactDescriptor) -> Result<Value> {
        let mut bytes = Vec::new();
        vcp_audit::history::History::read_artifact(
            self.engine.store(),
            &self.history_access(),
            &artifact.spec.id,
            &mut bytes,
        )?;
        Ok(serde_json::from_slice(&bytes)?)
    }
    pub fn reconcile_effects(&mut self) -> Result<Vec<ArtifactDescriptor>> {
        let effects: Vec<Effect> = self
            .engine
            .store()
            .state()
            .records
            .values()
            .filter(|row| row.collection == Collection::Effect)
            .map(Record::decode)
            .collect::<std::result::Result<_, _>>()?;
        let artifacts: Vec<ArtifactDescriptor> = self
            .engine
            .store()
            .state()
            .records
            .values()
            .filter(|row| row.collection == Collection::Artifact)
            .map(Record::decode)
            .collect::<std::result::Result<_, _>>()?;
        let mut reports = vec![];
        for mut effect in effects {
            if !matches!(
                effect.state,
                EffectState::DispatchRecorded | EffectState::Running | EffectState::OutcomeUnknown
            ) {
                continue;
            }
            let mut observations = vec![];
            let mut terminal = None;
            let mut terminal_conflict = false;
            let mut unresolved_staging = false;
            let mut exit_code = None;
            let mut plan = None;
            for artifact in artifacts.iter().filter(|a| a.spec.scope == effect.scope) {
                let schema = artifact.spec.schema.as_str();
                if !matches!(
                    schema,
                    "vcp-prepared-tool-v2"
                        | "vcp-process-start-v1"
                        | "vcp-process-outcome-v1"
                        | "vcp-file-outcome-v1"
                ) {
                    continue;
                }
                let value = self.recovery_artifact(artifact)?;
                if schema == "vcp-prepared-tool-v2"
                    && effect.observed_changes.contains(&artifact.spec.id)
                {
                    let operation: vcp_domain::policy::Operation =
                        serde_json::from_value(value["prepared"]["operation"].clone())?;
                    let prepared = vcp_policy::Prepared::new(operation)?;
                    if prepared.digest() == effect.operation_digest
                        && prepared.operation().scope == effect.scope
                    {
                        plan = Some(value["prepared"].clone());
                    }
                } else if effect.execution.is_some()
                    && value["execution"] == json!(effect.execution)
                {
                    if schema == "vcp-process-start-v1" {
                        #[cfg(windows)]
                        let current = process_identity(
                            value["process_id"]
                                .as_u64()
                                .and_then(|id| u32::try_from(id).ok()),
                        );
                        #[cfg(not(windows))]
                        let current = json!({"status":"unsupported"});
                        let identity_matches = value["process_identity"]["created"].is_u64()
                            && value["process_identity"]["created"] == current["created"];
                        observations.push(json!({"class":"process", "execution":effect.execution,
                            "process_id":value["process_id"], "certainty":"unknown",
                            "current_process":current,"creation_identity_matches":identity_matches,
                            "reason":"query-only PID observation; no signal or replay without prior owned handle proof"}));
                    } else if value["effect"] == json!(effect.id) {
                        if schema == "vcp-file-outcome-v1"
                            && value["observation"]["complete"] != true
                        {
                            if let Some(path) = value["observation"]["staging_path"].as_str() {
                                #[cfg(windows)]
                                {
                                    unresolved_staging |= self
                                        .staging_unresolved(&effect.scope.task, path)
                                        .unwrap_or(true);
                                }
                                #[cfg(not(windows))]
                                {
                                    let _ = path;
                                    unresolved_staging = true;
                                }
                            }
                        }
                        observations.push(
                            json!({"receipt":artifact.spec.id,"schema":schema,"observation":value}),
                        );
                        if schema == "vcp-process-outcome-v1"
                            && value["owned_processes_remaining"] == 0
                            && value["output_complete"].is_boolean()
                            && value.get("exit_code").is_some()
                        {
                            let observed_exit = value["exit_code"]
                                .as_i64()
                                .and_then(|v| i32::try_from(v).ok());
                            let observed_terminal = Some(
                                if observed_exit == Some(0) && value["output_complete"] == true {
                                    EffectState::Succeeded
                                } else {
                                    EffectState::Failed
                                },
                            );
                            terminal_conflict |= terminal.is_some()
                                && (terminal != observed_terminal || exit_code != observed_exit);
                            terminal = observed_terminal;
                            exit_code = observed_exit;
                        }
                    }
                }
            }
            #[cfg(windows)]
            if let Some(plan) = plan {
                if plan["operation"]["tool"] == "vcp_patch" {
                    let (files, outcome) = self.reconcile_files(&plan);
                    observations.extend(files);
                    terminal = outcome;
                } else if matches!(
                    plan["operation"]["tool"].as_str(),
                    Some("vcp_read" | "vcp_list" | "vcp_search")
                ) {
                    terminal = Some(EffectState::Cancelled);
                    observations.push(json!({"class":"read","certainty":"cancelled","reason":"read-only observation interrupted; no mutation to replay"}));
                }
            }
            #[cfg(not(windows))]
            let _ = plan;
            if terminal_conflict {
                terminal = None;
                observations.push(json!({"certainty":"unknown","reason":"conflicting terminal receipts for execution"}));
            }
            if unresolved_staging {
                terminal = None;
                observations.push(json!({"certainty":"unknown","reason":"retained file receipt reports an unresolved staging resource"}));
            }
            let report = self.capture(&effect.scope, Channel::Evidence,
                &canonical_bytes(&json!({"schema_version":1,"effect":effect.id,"execution":effect.execution,
                    "outcome":terminal.unwrap_or(EffectState::OutcomeUnknown),"observations":observations,
                    "replayed":false}))?, "vcp-effect-reconciliation-v1")?;
            effect.observed_changes.push(report.spec.id.clone());
            // DispatchRecorded has no direct terminal edge. Preserve the unknown
            // boundary before settling from separately retained observations.
            if effect.state != EffectState::OutcomeUnknown {
                self.command(
                    Command::AdvanceEffect {
                        id: effect.id.clone(),
                        next: EffectState::OutcomeUnknown,
                        reason: "recovery scan; no effect replay".into(),
                        execution: effect.execution.clone(),
                        exit_code: None,
                        observed_changes: effect.observed_changes.clone(),
                    },
                    Some(effect.scope.task.clone()),
                    effect.revision,
                )?;
                effect.revision = effect.revision.next()?;
            }
            if let Some(next) = terminal {
                self.command(Command::AdvanceEffect { id:effect.id, next,
                    reason:"reconciled from retained execution receipts and current native observations".into(),
                    execution:effect.execution, exit_code, observed_changes:effect.observed_changes },
                    Some(effect.scope.task), effect.revision)?;
            }
            reports.push(report);
        }
        Ok(reports)
    }

    #[cfg(windows)]
    fn staging_unresolved(&self, task: &TaskId, path: &str) -> Result<bool> {
        let workspace: Workspace = self
            .engine
            .store()
            .state()
            .record(
                Collection::Workspace,
                self.config.workspace.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        if workspace.trust != Trust::Trusted {
            return Err("workspace trust prevents recovery inspection".into());
        }
        let root = self.recovery_task_root(task)?;
        self.tool_read_access(&root.identity.root, "vcp_patch")?;
        match root.hold(Some(std::path::Path::new(path)), false) {
            Err(vcp_repository::Error::Io(error))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(false)
            }
            Ok(_) => Ok(true),
            Err(error) => Err(error.into()),
        }
    }
    #[cfg(windows)]
    fn reconcile_files(&self, plan: &Value) -> (Vec<Value>, Option<EffectState>) {
        let result = (|| -> Result<(Vec<Value>, Option<EffectState>)> {
            let workspace: Workspace = self
                .engine
                .store()
                .state()
                .record(
                    Collection::Workspace,
                    self.config.workspace.as_str(),
                    &self.config.workspace,
                )?
                .decode()?;
            if workspace.trust != Trust::Trusted {
                return Err("workspace trust prevents file recovery reads".into());
            }
            let operation: vcp_domain::policy::Operation =
                serde_json::from_value(plan["operation"].clone())?;
            let root = self.recovery_task_root(&operation.scope.task)?;
            self.tool_read_access(&RootId::parse(self.config.workspace.as_str())?, "vcp_patch")?;
            if operation.binding != root.identity.binding
                || operation.host != self.config.binding.host
            {
                return Err("file recovery binding changed".into());
            }
            let changes = plan["changes"]
                .as_array()
                .ok_or("missing prepared file changes")?;
            let mut files = vec![];
            let mut applied = 0;
            let mut unmodified = 0;
            for change in changes {
                let path = change["path"].as_str().ok_or("missing prepared path")?;
                let destination = change["rename_to"].as_str();
                let after: Option<Vec<u8>> = serde_json::from_value(change["after"].clone())?;
                let before: Option<vcp_repository::FileVersion> =
                    serde_json::from_value(change["before"].clone())?;
                let observe = |path: &str| -> Result<Option<vcp_repository::Source>> {
                    match root.read(std::path::Path::new(path), 1024 * 1024) {
                        Ok(source) => Ok(Some(source)),
                        Err(vcp_repository::Error::Io(e))
                            if e.kind() == std::io::ErrorKind::NotFound =>
                        {
                            Ok(None)
                        }
                        Err(e) => Err(e.into()),
                    }
                };
                let source = observe(path)?;
                let target = if let Some(destination) = destination {
                    observe(destination)?
                } else {
                    None
                };
                let actual = if destination.is_some() {
                    target.as_ref()
                } else {
                    source.as_ref()
                };
                let case_only =
                    destination.is_some_and(|destination| path.eq_ignore_ascii_case(destination));
                let renamed_case = if case_only {
                    let destination = std::path::Path::new(destination.unwrap());
                    let parent = destination.parent().ok_or("rename parent missing")?;
                    let _held = root.hold(
                        if parent.as_os_str().is_empty() {
                            None
                        } else {
                            Some(parent)
                        },
                        true,
                    )?;
                    let mut matches = false;
                    for (index, entry) in std::fs::read_dir(root.path().join(parent))?.enumerate() {
                        if index >= 10_000 {
                            return Err("rename observation exceeds directory bound".into());
                        }
                        matches |= Some(entry?.file_name().as_os_str()) == destination.file_name();
                    }
                    matches
                } else {
                    false
                };
                let matches_after =
                    match (&after, actual) {
                        (Some(expected), Some(actual)) => expected == &actual.bytes,
                        (None, None) => true,
                        _ => false,
                    } && (destination.is_none() || source.is_none() || renamed_case);
                let matches_before = match (&before, &source) {
                    (Some(expected), Some(actual)) => expected == &actual.version,
                    (None, None) => true,
                    _ => false,
                } && (target.is_none() || (case_only && !renamed_case));
                let certainty = if matches_after {
                    applied += 1;
                    "applied"
                } else if matches_before {
                    unmodified += 1;
                    "unapplied"
                } else {
                    "conflicted"
                };
                files.push(json!({"class":"file","path":path,"destination":destination,"certainty":certainty,
                    "interpretation":"current observed state; matching bytes do not prove causal authorship",
                    "before":before,"intended_sha256":after.as_deref().map(vcp_protocol::digest_bytes),
                    "observed":source.as_ref().map(|s|&s.version),"observed_destination":target.as_ref().map(|s|&s.version)}));
            }
            let outcome = if !changes.is_empty() && applied == changes.len() {
                Some(EffectState::Succeeded)
            } else if !changes.is_empty() && applied + unmodified == changes.len() {
                Some(EffectState::Failed)
            } else {
                None
            };
            Ok((files, outcome))
        })();
        result.unwrap_or_else(|error| {
            (
                vec![json!({"class":"file","certainty":"unknown","reason":error.to_string()})],
                None,
            )
        })
    }
}

/// A PID is only a diagnostic locator. Birth time distinguishes recycled IDs;
/// query failures and absent handles cannot prove any external effect outcome.
#[cfg(windows)]
pub(crate) fn process_identity(pid: Option<u32>) -> Value {
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        fn GetProcessTimes(
            handle: isize,
            created: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn GetExitCodeProcess(handle: isize, code: *mut u32) -> i32;
        fn CloseHandle(handle: isize) -> i32;
    }
    let Some(pid) = pid else {
        return json!({"status":"no_pid"});
    };
    let handle = unsafe { OpenProcess(0x1000, 0, pid) }; // PROCESS_QUERY_LIMITED_INFORMATION only.
    if handle == 0 {
        return json!({"status":"absent_or_unobservable","error":std::io::Error::last_os_error().raw_os_error()});
    }
    let (mut created, mut exit, mut kernel, mut user) = (
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
    );
    let mut code = 0;
    let success = unsafe {
        GetProcessTimes(handle, &mut created, &mut exit, &mut kernel, &mut user) != 0
            && GetExitCodeProcess(handle, &mut code) != 0
    };
    unsafe {
        CloseHandle(handle);
    }
    if !success {
        return json!({"status":"unobservable"});
    }
    json!({"status":"observed","pid":pid,"created":((created.high as u64)<<32)|created.low as u64,"exit_code":code})
}
