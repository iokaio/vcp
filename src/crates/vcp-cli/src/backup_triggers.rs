// SPDX-License-Identifier: Apache-2.0
//! Local backup hooks never load keys or admit task/model execution.
use std::time::Duration;
use vcp_domain::{task::TaskState, CommandId};
use vcp_lifecycle::foundation::CanonicalHost;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Pause,
    Completion,
    Exit,
}
impl Reason {
    fn label(self) -> &'static str {
        match self {
            Self::Pause => "controlled pause",
            Self::Completion => "completion",
            Self::Exit => "controlled exit",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Admission {
    Disabled,
    MissingLoadedCapability,
    Start,
}
fn admission(configured: bool, loaded_enabled: bool) -> Admission {
    if loaded_enabled {
        Admission::Start
    } else if configured {
        Admission::MissingLoadedCapability
    } else {
        Admission::Disabled
    }
}
/// Tracks canonical state edges, including controls submitted by another CLI.
/// The persisted flag is public setup metadata, never a secret-cache request.
pub struct Triggers {
    configured: bool,
    state: Option<TaskState>,
}
impl Triggers {
    pub fn new(configured: bool) -> Self {
        Self {
            configured,
            state: None,
        }
    }
    fn edge(&mut self, state: TaskState) -> Option<Reason> {
        if self.state.replace(state) == Some(state) {
            return None;
        }
        match state {
            TaskState::Paused => Some(Reason::Pause),
            TaskState::Completed => Some(Reason::Completion),
            _ => None,
        }
    }
    pub fn observe(&mut self, host: &CanonicalHost, state: TaskState) -> Option<String> {
        self.edge(state)
            .and_then(|reason| self.trigger(host, reason))
    }
    fn trigger(&self, host: &CanonicalHost, reason: Reason) -> Option<String> {
        let result = (|| -> Result<Option<String>, String> {
            match admission(self.configured, host.backup_automatic_enabled()?) {
                Admission::Disabled => Ok(None),
                Admission::MissingLoadedCapability => Ok(Some(format!(
                    "Backup pending after {}: explicitly load the configured verified signing key in this owner; no work scheduled.", reason.label()
                ))),
                Admission::Start => {
                    if let Some(progress) = host.backup_progress()? {
                        if progress.running() {
                            return Ok(Some(format!("Backup {} already in progress; {} shares its pending operation.", progress.operation, reason.label())));
                        }
                    }
                    let progress = host.start_backup(CommandId::new(), false)?;
                    Ok(Some(format!("Backup {} requested after {}; inspect backup status for publication and transfer state.", progress.operation, reason.label())))
                }
            }
        })();
        match result {
            Ok(message) => message,
            Err(_) => Some(format!("Backup pending after {}: local backup admission unavailable; inspect backup status. Local task state is preserved.", reason.label())),
        }
    }
    /// Controlled shutdown permits at most two seconds of background local work.
    /// Cancellation preserves already admitted jobs for explicit reconciliation.
    pub async fn shutdown(&self, host: &CanonicalHost) {
        if let Some(message) = self.trigger(host, Reason::Exit) {
            eprintln!("vcp: {message}");
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            let progress = match host.backup_progress() {
                Ok(Some(progress)) if progress.running() => progress,
                _ => return,
            };
            if tokio::time::Instant::now() >= deadline {
                let _ = host.cancel_backup(&progress.operation);
                eprintln!("vcp: backup {} remains pending at controlled exit; inspect its retained status before retrying. No background daemon was started.", progress.operation);
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_hooks_require_explicit_loaded_capability() {
        assert_eq!(admission(false, false), Admission::Disabled);
        assert_eq!(admission(true, false), Admission::MissingLoadedCapability);
        assert_eq!(admission(false, true), Admission::Start);
        assert_eq!(admission(true, true), Admission::Start);
    }
    #[test]
    fn canonical_pause_and_completion_edges_fire_once_without_resuming() {
        let mut triggers = Triggers::new(true);
        assert_eq!(triggers.edge(TaskState::Running), None);
        assert_eq!(triggers.edge(TaskState::Paused), Some(Reason::Pause));
        assert_eq!(triggers.edge(TaskState::Paused), None);
        assert_eq!(triggers.edge(TaskState::Running), None);
        assert_eq!(triggers.edge(TaskState::Paused), Some(Reason::Pause));
        assert_eq!(
            triggers.edge(TaskState::Completed),
            Some(Reason::Completion)
        );
        assert_eq!(triggers.edge(TaskState::Completed), None);
        assert_eq!(triggers.edge(TaskState::Failed), None);
        assert_eq!(triggers.edge(TaskState::Cancelled), None);
    }
    #[tokio::test]
    async fn configured_hooks_without_loaded_keys_leave_canonical_state_unchanged() {
        use vcp_domain::{accounting::*, ids::*, revision::*, workspace::*};
        use vcp_lifecycle::foundation::Config;
        use vcp_store::BackendKind;
        for backend in [BackendKind::Files, BackendKind::Sqlite] {
            let temp = tempfile::tempdir().unwrap();
            let currency: Currency = "USD".to_owned().try_into().unwrap();
            let config = Config {
                canonical_root: temp.path().join("canonical"),
                backend,
                workspace: WorkspaceId::new(),
                session: SessionId::new(),
                binding: Binding {
                    host: HostId::new(),
                    root: temp.path().to_string_lossy().into_owned(),
                    repository: "backup-trigger-fixture".into(),
                    worktree: "main".into(),
                    revision: Revision::ZERO,
                },
                actor: ActorId::new(),
                root_task: TaskId::new(),
                cap: Money {
                    currency: currency.clone(),
                    micros: Micros::new(1000),
                },
                protected: Micros::ZERO,
                price: PriceSnapshot {
                    id: "a".repeat(64),
                    provider: "fixture".into(),
                    model: "fixture/model".into(),
                    currency,
                    capability: "b".repeat(64),
                    valid_until: Timestamp::new(u64::MAX),
                    rates: [
                        ChargeCategory::Input,
                        ChargeCategory::Output,
                        ChargeCategory::CacheRead,
                        ChargeCategory::CacheWrite,
                        ChargeCategory::Request,
                        ChargeCategory::ProviderTool,
                    ]
                    .into_iter()
                    .map(|kind| {
                        (
                            kind,
                            Rate {
                                micros: Micros::ZERO,
                                per_units: Units::new(1),
                            },
                        )
                    })
                    .collect(),
                },
                input_ceiling: Units::new(4096),
                output_ceiling: Units::new(1024),
                artifact_limit: ByteCount::new(vcp_store::artifact::DEFAULT_ARTIFACT_LIMIT),
                host_tool_denials: vec![],
            };
            let (host, owner) = CanonicalHost::open(config).unwrap();
            let before = vcp_protocol::canonical_bytes(&host.snapshot().unwrap()).unwrap();
            let mut triggers = Triggers::new(true);
            assert!(triggers
                .observe(&host, TaskState::Paused)
                .unwrap()
                .contains("pending"));
            assert!(triggers
                .observe(&host, TaskState::Completed)
                .unwrap()
                .contains("pending"));
            triggers.shutdown(&host).await;
            assert!(host.backup_progress().unwrap().is_none());
            assert_eq!(
                vcp_protocol::canonical_bytes(&host.snapshot().unwrap()).unwrap(),
                before
            );
            owner.close().await.unwrap();
        }
    }
}
