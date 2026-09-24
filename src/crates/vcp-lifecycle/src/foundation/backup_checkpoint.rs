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
struct CancelOnDrop(Option<Arc<AtomicBool>>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(cancelled) = &self.0 {
            cancelled.store(true, Ordering::Release);
        }
    }
}
impl CanonicalHost {
    pub async fn capture_backup_generation(
        &self,
        publisher: Arc<vcp_memory::publication::Publisher>,
        view: vcp_memory::publication::View,
        cancelled: Arc<AtomicBool>,
    ) -> Result<vcp_store::snapshot_inputs::GenerationInput, String> {
        self.capture_backup_generation_fenced(publisher, view, cancelled, None)
            .await
    }
    pub(crate) async fn capture_backup_generation_fenced(
        &self,
        publisher: Arc<vcp_memory::publication::Publisher>,
        view: vcp_memory::publication::View,
        cancelled: Arc<AtomicBool>,
        fence: Option<backup_run::PublicFence>,
    ) -> Result<vcp_store::snapshot_inputs::GenerationInput, String> {
        let mut guard = CancelOnDrop(Some(cancelled.clone()));
        let authority = fence.clone();
        let cut = self.worker.run(move |context| {
            backup_run::check_fence(authority.as_ref(), context)?;
            context.backup_cut()
        })?;
        let stopped = cancelled.clone();
        let files = tokio::task::spawn_blocking(move || publisher.snapshot_files(view, &stopped))
            .await
            .map_err(|_| "backup generation worker failed")?
            .map_err(|e| e.to_string())?;
        let value = self.worker.run(move |context| {
            context.accept_backup_generation(cut, files, &cancelled, fence.as_ref())
        })?;
        guard.0 = None;
        Ok(value)
    }
    /// Explicit user maintenance works while paused and never starts model work.
    /// Git is an independently configured, bounded native read capability.
    pub async fn capture_backup_checkpoint(
        &self,
        git: Arc<Git>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Checkpoint, String> {
        self.capture_backup_checkpoint_fenced(git, cancelled, None)
            .await
    }
    pub(crate) async fn capture_backup_checkpoint_fenced(
        &self,
        git: Arc<Git>,
        cancelled: Arc<AtomicBool>,
        fence: Option<backup_run::PublicFence>,
    ) -> Result<Checkpoint, String> {
        let mut guard = CancelOnDrop(Some(cancelled.clone()));
        if cancelled.load(Ordering::Acquire) || self.scheduler.busy() {
            return Err("backup checkpoint requires quiescent local effects".into());
        }
        let authority = fence.clone();
        let cut = self.worker.run(move |context| {
            backup_run::check_fence(authority.as_ref(), context)?;
            context.backup_cut()
        })?;
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
        if cancelled.load(Ordering::Acquire) || self.scheduler.busy() {
            return Err("backup checkpoint cancelled or local effects changed".into());
        }
        let authority = fence.clone();
        self.worker.run(move |context| {
            backup_run::check_fence(authority.as_ref(), context)?;
            context.backup_cut()?;
            Ok(())
        })?;
        let current_git = git.observe(&cut.root).await.map_err(|e| e.to_string())?;
        if observed.git.as_ref().map(|value| &value.snapshot) != Some(&current_git.snapshot) {
            return Err("Git checkpoint changed during backup preparation".into());
        }
        if cancelled.load(Ordering::Acquire) || self.scheduler.busy() {
            return Err("backup checkpoint cancelled or local effects changed".into());
        }
        let value = self.worker.run(move |context| {
            context.accept_backup_checkpoint(Prepared { cut, observed }, &cancelled, fence.as_ref())
        })?;
        guard.0 = None;
        Ok(value)
    }
}
