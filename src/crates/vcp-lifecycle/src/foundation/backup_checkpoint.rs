// SPDX-License-Identifier: Apache-2.0
//! Explicit local backup capture, including dirty and selected untracked files.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use vcp_repository::{git::Git, observation::Observation, Root};
use vcp_store::snapshot_inputs::Checkpoint;

pub(super) struct Cut {
    pub(super) root: Root,
    pub(super) workspace: Workspace,
    pub(super) scope: Scope,
    pub(super) controller: ControllerId,
    pub(super) epoch: OwnerEpoch,
}
pub(super) struct Prepared {
    pub(super) cut: Cut,
    pub(super) observed: Observation,
}
impl CanonicalHost {
    /// Explicit user maintenance works while paused and never starts model work.
    /// Git is an independently configured, bounded native read capability.
    pub async fn capture_backup_checkpoint(
        &self,
        git: Arc<Git>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Checkpoint, String> {
        if cancelled.load(Ordering::Acquire) || self.scheduler.busy() {
            return Err("backup checkpoint requires quiescent local effects".into());
        }
        let cut = self.worker.run(|context| context.backup_cut())?;
        let observed = cut
            .root
            .observe(
                Some(&git),
                &vcp_repository::discovery::Limits {
                    entries: 4096,
                    depth: 32,
                    file_bytes: 4 * 1024 * 1024,
                    total_bytes: 8 * 1024 * 1024,
                    skip_generated: true,
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        let current_git = git.observe(&cut.root).await.map_err(|e| e.to_string())?;
        if observed.git.as_ref().map(|value| &value.snapshot) != Some(&current_git.snapshot) {
            return Err("Git checkpoint changed during backup preparation".into());
        }
        if cancelled.load(Ordering::Acquire) || self.scheduler.busy() {
            return Err("backup checkpoint cancelled or local effects changed".into());
        }
        self.worker.run(move |context| {
            context.accept_backup_checkpoint(Prepared { cut, observed }, &cancelled)
        })
    }
}
