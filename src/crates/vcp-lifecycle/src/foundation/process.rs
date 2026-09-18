// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{collections::BTreeMap, ffi::OsString};
use vcp_domain::effect::{Effect, EffectState};
use vcp_store::contract::Collection;
pub struct CanonicalProcess {
    host: CanonicalHost,
    binding: ThreadBinding,
    effect: ToolRunId,
    execution: ExecutionId,
    process: Option<crate::process::Process>,
    stdout: Option<Arc<OutputCapture>>,
    stderr: Option<Arc<OutputCapture>>,
    finished: bool,
}
pub struct ProcessOutcome {
    pub effect: ToolRunId,
    pub exit_code: Option<i32>,
    pub stdout: ArtifactDescriptor,
    pub stderr: ArtifactDescriptor,
    pub stdout_tail: Vec<u8>,
    pub stderr_tail: Vec<u8>,
}
impl CanonicalHost {
    /// Private trusted-host entry point. P2 owns the user-facing policy broker;
    /// this adapter requires current canonical task revision before recording
    /// and launching the exact native operation supplied by that host.
    pub fn spawn_process(
        &self,
        thread: ThreadId,
        expected_task: Revision,
        executable: &Path,
        args: &[OsString],
        cwd: &Path,
        environment: &BTreeMap<OsString, OsString>,
        display_limit: usize,
    ) -> Result<CanonicalProcess, String> {
        if self.worker.fenced() {
            return Err("canonical capture is fenced".into());
        }
        let binding = self.binding(thread)?;
        let operation=vcp_protocol::canonical_bytes(&serde_json::json!({"executable":executable,"args":args,"cwd":cwd,"environment":environment.iter().collect::<Vec<_>>()})).map_err(|error|error.to_string())?;
        let effect = ToolRunId::new();
        let execution = ExecutionId::new();
        self.command(
            Command::ProposeEffect {
                id: effect.clone(),
                operation_digest: vcp_protocol::digest_bytes(&operation),
            },
            Some(binding.scope.task.clone()),
            expected_task,
        )?;
        for (revision, next) in [
            EffectState::Validated,
            EffectState::Authorized,
            EffectState::DispatchRecorded,
        ]
        .into_iter()
        .enumerate()
        {
            self.command(
                Command::AdvanceEffect {
                    id: effect.clone(),
                    next,
                    reason: "explicit canonical host native operation".into(),
                    execution: if revision == 2 {
                        Some(execution.clone())
                    } else {
                        None
                    },
                    exit_code: None,
                    observed_changes: vec![],
                },
                Some(binding.scope.task.clone()),
                Revision::new(revision as u64),
            )?;
        }
        let stdout = Arc::new(self.open_output(thread, Channel::Stdout)?);
        let stderr = Arc::new(self.open_output(thread, Channel::Stderr)?);
        let out = stdout.clone();
        let err = stderr.clone();
        let process = self.runtime.spawn_process_with_capture(
            thread,
            executable,
            args,
            cwd,
            environment,
            display_limit,
            Some(Arc::new(move |bytes| {
                out.write(bytes).map_err(std::io::Error::other)
            })),
            Some(Arc::new(move |bytes| {
                err.write(bytes).map_err(std::io::Error::other)
            })),
        );
        let mut owned = CanonicalProcess {
            host: self.clone(),
            binding,
            effect,
            execution,
            process: None,
            stdout: Some(stdout),
            stderr: Some(stderr),
            finished: false,
        };
        owned.process = Some(process.map_err(|error| error.to_string())?);
        owned.advance(EffectState::Running, None, vec![])?;
        Ok(owned)
    }
}
impl CanonicalProcess {
    fn advance(
        &self,
        next: EffectState,
        exit_code: Option<i32>,
        artifacts: Vec<ArtifactId>,
    ) -> Result<(), String> {
        let id = self.effect.clone();
        let task = self.binding.scope.task.clone();
        let execution = self.execution.clone();
        self.host.worker.run_cleanup(move |context| {
            let effect: Effect = context
                .engine
                .store()
                .state()
                .record(Collection::Effect, id.as_str(), &context.config.workspace)?
                .decode()?;
            context.command(
                Command::AdvanceEffect {
                    id,
                    next,
                    reason: "native owner/process observation".into(),
                    execution: Some(execution),
                    exit_code,
                    observed_changes: artifacts,
                },
                Some(task),
                effect.revision,
            )?;
            Ok(())
        })
    }
    pub async fn wait(mut self) -> Result<ProcessOutcome, String> {
        let outcome = self
            .process
            .take()
            .ok_or("process consumed")?
            .wait()
            .await
            .map_err(|error| error.to_string())?;
        let stdout = Arc::try_unwrap(self.stdout.take().unwrap())
            .map_err(|_| "stdout capture still in use")?
            .finish()?;
        let stderr = Arc::try_unwrap(self.stderr.take().unwrap())
            .map_err(|_| "stderr capture still in use")?
            .finish()?;
        let next = if outcome.exit_code == Some(0) {
            EffectState::Succeeded
        } else {
            EffectState::Failed
        };
        self.advance(
            next,
            outcome.exit_code,
            vec![stdout.spec.id.clone(), stderr.spec.id.clone()],
        )?;
        self.finished = true;
        Ok(ProcessOutcome {
            effect: self.effect.clone(),
            exit_code: outcome.exit_code,
            stdout,
            stderr,
            stdout_tail: outcome.stdout.bytes,
            stderr_tail: outcome.stderr.bytes,
        })
    }
}
impl Drop for CanonicalProcess {
    fn drop(&mut self) {
        if !self.finished {
            self.process.take();
            if self
                .advance(EffectState::OutcomeUnknown, None, vec![])
                .is_err()
            {
                self.host.worker.fence();
            }
        }
    }
}
