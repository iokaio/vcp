// SPDX-License-Identifier: Apache-2.0
//! One explicitly loaded local backup capability and one bounded background
//! operation per owner. Accepted preparation is not a durable snapshot claim.
use super::{
    backup_run::{Capabilities, PublicFence},
    CanonicalHost,
};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use vcp_domain::CommandId;
use vcp_repository::git::Git;
type Result<T> = std::result::Result<T, String>;
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Preparing,
    Publishing,
    Finished,
    Failed,
    Cancelled,
}
#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub operation: CommandId,
    pub phase: Phase,
    pub error: Option<String>,
}
impl Progress {
    pub fn running(&self) -> bool {
        matches!(self.phase, Phase::Preparing | Phase::Publishing)
    }
    pub fn done(&self) -> bool {
        !self.running()
    }
}
pub(super) struct Loaded {
    identity: CapabilityIdentity,
    capabilities: Arc<Capabilities>,
    git: Arc<Git>,
    automatic: bool,
    progress: Option<Progress>,
    cancel: Option<Arc<AtomicBool>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityIdentity {
    pub reference: String,
    pub generation: u64,
    pub configuration_revision: u64,
}
#[derive(Clone, Debug)]
pub struct ManagerSnapshot {
    pub capability: Option<CapabilityIdentity>,
    pub busy: bool,
    pub progress: Option<Progress>,
}
impl CanonicalHost {
    pub fn backup_manager_snapshot(&self) -> Result<ManagerSnapshot> {
        let slot = self
            .backup
            .lock()
            .map_err(|_| "backup manager unavailable")?;
        let progress = slot
            .as_ref()
            .and_then(|loaded| loaded.progress.as_ref())
            .map(|value| Progress {
                operation: value.operation.clone(),
                phase: value.phase.clone(),
                error: None,
            });
        Ok(ManagerSnapshot {
            capability: slot.as_ref().map(|loaded| loaded.identity.clone()),
            busy: progress.as_ref().is_some_and(Progress::running),
            progress,
        })
    }
    pub fn unload_backup(&self) -> Result<()> {
        let mut slot = self
            .backup
            .lock()
            .map_err(|_| "backup manager unavailable")?;
        if slot
            .as_ref()
            .and_then(|loaded| loaded.progress.as_ref())
            .is_some_and(Progress::running)
        {
            return Err("backup operation already active".into());
        }
        *slot = None;
        Ok(())
    }
    pub fn backup_configuration(&self) -> Result<super::Config> {
        self.worker
            .run_cleanup(|context| Ok(context.config.clone()))
    }
    pub fn load_backup(
        &self,
        capabilities: Arc<Capabilities>,
        git: Arc<Git>,
        automatic: bool,
    ) -> Result<()> {
        self.load_backup_with_revision(capabilities, git, automatic, 0)
    }
    pub fn load_backup_with_revision(
        &self,
        capabilities: Arc<Capabilities>,
        git: Arc<Git>,
        automatic: bool,
        configuration_revision: u64,
    ) -> Result<()> {
        let mut slot = self
            .backup
            .lock()
            .map_err(|_| "backup manager unavailable")?;
        if slot
            .as_ref()
            .and_then(|loaded| loaded.progress.as_ref())
            .is_some_and(Progress::running)
        {
            return Err("backup operation already active".into());
        }
        *slot = Some(Loaded {
            identity: CapabilityIdentity {
                reference: CommandId::new().to_string(),
                generation: 1,
                configuration_revision,
            },
            capabilities,
            git,
            automatic,
            progress: None,
            cancel: None,
        });
        Ok(())
    }
    pub fn backup_automatic_enabled(&self) -> Result<bool> {
        Ok(self
            .backup
            .lock()
            .map_err(|_| "backup manager unavailable")?
            .as_ref()
            .is_some_and(|loaded| loaded.automatic))
    }
    pub fn backup_progress(&self) -> Result<Option<Progress>> {
        Ok(self
            .backup
            .lock()
            .map_err(|_| "backup manager unavailable")?
            .as_ref()
            .and_then(|loaded| loaded.progress.clone()))
    }
    /// Signal only: safe while the canonical worker serializes public intent.
    /// Inactive cleanup remains a separate reconciliation obligation.
    pub(crate) fn request_backup_stop(&self, operation: &CommandId) -> Result<bool> {
        let slot = self
            .backup
            .lock()
            .map_err(|_| "backup manager unavailable")?;
        let Some(loaded) = slot.as_ref() else {
            return Ok(false);
        };
        if loaded
            .progress
            .as_ref()
            .is_none_or(|value| &value.operation != operation)
        {
            return Ok(false);
        }
        let Some(cancelled) = &loaded.cancel else {
            return Ok(false);
        };
        cancelled.store(true, Ordering::Release);
        Ok(true)
    }
    pub fn cancel_backup(&self, operation: &CommandId) -> Result<Progress> {
        let mut slot = self
            .backup
            .lock()
            .map_err(|_| "backup manager unavailable")?;
        let loaded = slot.as_ref().ok_or("backup signing material not loaded")?;
        let progress = loaded
            .progress
            .as_ref()
            .filter(|value| value.operation == *operation)
            .ok_or("backup operation is not owned by this controller")?;
        if let Some(cancel) = &loaded.cancel {
            cancel.store(true, Ordering::Release);
            return Ok(progress.clone());
        }
        let capabilities = loaded.capabilities.clone();
        let identity = loaded.identity.clone();
        // Reserve cleanup, then drop the manager lock before waiting for the
        // worker. Public fence callbacks may inspect this same manager.
        if let Some(loaded) = slot.as_mut() {
            loaded.progress = Some(Progress {
                operation: operation.clone(),
                phase: Phase::Publishing,
                error: None,
            });
            loaded.cancel = Some(Arc::new(AtomicBool::new(true)));
        }
        drop(slot);
        let result = self.release_cancelled_backup(capabilities, operation.clone());
        let progress = Progress {
            operation: operation.clone(),
            phase: if result.is_ok() {
                Phase::Cancelled
            } else {
                Phase::Failed
            },
            error: result.as_ref().err().cloned(),
        };
        let mut slot = self
            .backup
            .lock()
            .map_err(|_| "backup manager unavailable")?;
        if let Some(loaded) = slot.as_mut() {
            if loaded.identity == identity
                && loaded
                    .progress
                    .as_ref()
                    .is_some_and(|value| value.operation == *operation)
            {
                loaded.progress = Some(progress.clone());
                loaded.cancel = None;
            }
        }
        result?;
        Ok(progress)
    }
    #[cfg(windows)]
    pub fn start_backup(&self, operation: CommandId, retry: bool) -> Result<Progress> {
        self.start_backup_inner(operation, retry, None, None)
    }
    #[cfg(windows)]
    pub(crate) fn start_backup_fenced(
        &self,
        operation: CommandId,
        retry: bool,
        expected: &CapabilityIdentity,
        fence: PublicFence,
    ) -> Result<Progress> {
        self.start_backup_inner(operation, retry, Some(expected.clone()), Some(fence))
    }
    #[cfg(windows)]
    fn start_backup_inner(
        &self,
        operation: CommandId,
        retry: bool,
        expected: Option<CapabilityIdentity>,
        fence: Option<PublicFence>,
    ) -> Result<Progress> {
        let (capabilities, git, cancel, progress, identity) = {
            let mut slot = self
                .backup
                .lock()
                .map_err(|_| "backup manager unavailable")?;
            let loaded = slot.as_mut().ok_or("backup signing material not loaded")?;
            if expected
                .as_ref()
                .is_some_and(|expected| expected != &loaded.identity)
            {
                return Err("backup capability changed".into());
            }
            if fence.as_ref().is_some_and(PublicFence::cancelled) {
                return Err("public backup authority cancelled".into());
            }
            if let Some(progress) = &loaded.progress {
                if progress.running() {
                    if progress.operation == operation {
                        return Ok(progress.clone());
                    }
                    return Err("backup operation already active".into());
                }
            }
            let cancel = Arc::new(AtomicBool::new(false));
            let progress = Progress {
                operation: operation.clone(),
                phase: Phase::Preparing,
                error: None,
            };
            loaded.progress = Some(progress.clone());
            loaded.cancel = Some(cancel.clone());
            (
                loaded.capabilities.clone(),
                loaded.git.clone(),
                cancel,
                progress,
                loaded.identity.clone(),
            )
        };
        let host = self.clone();
        tokio::spawn(async move {
            let cleanup = capabilities.clone();
            let cancellation = cancel.clone();
            let cleanup_fence = fence.clone();
            let mut result = async {
                let inputs = if retry {
                    None
                } else {
                    Some(
                        host.capture_backup_inputs_fenced(git, cancel.clone(), fence.clone())
                            .await?,
                    )
                };
                {
                    let mut slot = host
                        .backup
                        .lock()
                        .map_err(|_| "backup manager unavailable")?;
                    let loaded = slot.as_mut().ok_or("backup capability unloaded")?;
                    if loaded.identity != identity
                        || loaded
                            .progress
                            .as_ref()
                            .is_none_or(|value| value.operation != operation)
                    {
                        return Err("backup operation replaced".into());
                    }
                    loaded.progress = Some(Progress {
                        operation: operation.clone(),
                        phase: Phase::Publishing,
                        error: None,
                    });
                }
                let job = host
                    .publish_backup_fenced(capabilities, operation.clone(), inputs, cancel, fence)
                    .await?;
                if job.stage != vcp_store::snapshot_jobs::Stage::Published || job.active {
                    return Err("backup did not finish local publication".to_owned());
                }
                Ok(())
            }
            .await;
            if result.is_err()
                && (cancellation.load(Ordering::Acquire)
                    || cleanup_fence.as_ref().is_some_and(PublicFence::cancelled))
            {
                if let Err(error) = host.release_cancelled_backup(cleanup, operation.clone()) {
                    result = Err(format!(
                        "{}; retained cleanup obligation: {error}",
                        result.err().unwrap_or_default()
                    ));
                }
            }
            if let Ok(mut slot) = host.backup.lock() {
                if let Some(loaded) = slot.as_mut() {
                    if loaded.identity == identity
                        && loaded
                            .progress
                            .as_ref()
                            .is_some_and(|value| value.operation == operation)
                    {
                        loaded.progress = Some(Progress {
                            operation,
                            phase: if result.is_ok() {
                                Phase::Finished
                            } else {
                                Phase::Failed
                            },
                            error: result.err(),
                        });
                        loaded.cancel = None;
                    }
                }
            }
        });
        Ok(progress)
    }
}
