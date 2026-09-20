// SPDX-License-Identifier: Apache-2.0
//! One explicitly loaded local backup capability and one bounded background
//! operation per owner. Accepted preparation is not a durable snapshot claim.
use super::{backup_run::Capabilities, CanonicalHost};
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
    capabilities: Arc<Capabilities>,
    git: Arc<Git>,
    automatic: bool,
    progress: Option<Progress>,
    cancel: Option<Arc<AtomicBool>>,
}
impl CanonicalHost {
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
    pub fn cancel_backup(&self, operation: &CommandId) -> Result<Progress> {
        let slot = self
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
        }
        Ok(progress.clone())
    }
    #[cfg(windows)]
    pub fn start_backup(&self, operation: CommandId, retry: bool) -> Result<Progress> {
        let (capabilities, git, cancel, progress) = {
            let mut slot = self
                .backup
                .lock()
                .map_err(|_| "backup manager unavailable")?;
            let loaded = slot.as_mut().ok_or("backup signing material not loaded")?;
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
            )
        };
        let host = self.clone();
        tokio::spawn(async move {
            let cleanup = capabilities.clone();
            let cancellation = cancel.clone();
            let mut result = async {
                let inputs = if retry {
                    None
                } else {
                    Some(host.capture_backup_inputs(git, cancel.clone()).await?)
                };
                {
                    let mut slot = host
                        .backup
                        .lock()
                        .map_err(|_| "backup manager unavailable")?;
                    let loaded = slot.as_mut().ok_or("backup capability unloaded")?;
                    loaded.progress = Some(Progress {
                        operation: operation.clone(),
                        phase: Phase::Publishing,
                        error: None,
                    });
                }
                let job = host
                    .publish_backup(capabilities, operation.clone(), inputs, cancel)
                    .await?;
                if job.stage != vcp_store::snapshot_jobs::Stage::Published || job.active {
                    return Err("backup did not finish local publication".to_owned());
                }
                Ok(())
            }
            .await;
            if result.is_err() && cancellation.load(Ordering::Acquire) {
                if let Err(error) = host.release_cancelled_backup(cleanup, operation.clone()) {
                    result = Err(format!(
                        "{}; retained cleanup obligation: {error}",
                        result.err().unwrap_or_default()
                    ));
                }
            }
            if let Ok(mut slot) = host.backup.lock() {
                if let Some(loaded) = slot.as_mut() {
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
        });
        Ok(progress)
    }
}
