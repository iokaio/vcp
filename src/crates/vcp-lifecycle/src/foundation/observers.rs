// SPDX-License-Identifier: Apache-2.0
//! Explicit owner-driven local observation. No timer service or model request.
use super::*;
use serde::{Deserialize, Serialize};
pub use vcp_engine::observers::budget::Limits;

struct PendingObservation {
    host: CanonicalHost,
    work: Option<vcp_engine::observers::subscription::Work>,
}
impl Drop for PendingObservation {
    fn drop(&mut self) {
        if let Some(work) = self.work.take() {
            let _ = self
                .host
                .worker
                .run_cleanup(move |context| context.abandon_observer(work));
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub limits: Limits,
}
impl Configuration {
    pub fn validate(&self) -> Result<(), String> {
        self.limits.validate().map_err(|e| e.to_string())
    }
}

impl CanonicalHost {
    pub fn configure_observers(&self, configuration: Configuration) -> Result<(), String> {
        configuration.validate()?;
        self.worker
            .run(move |context| context.configure_observers(configuration))
    }
    /// Read-only, including during pause. Reading never drains the queue.
    pub fn observer_status(&self, thread: ThreadId) -> Result<serde_json::Value, String> {
        let binding = self.binding(thread)?;
        let runtime = self.runtime.clone();
        self.worker.run_cleanup(move |context| {
            let state = runtime
                .0
                .state
                .lock()
                .map_err(|_| "observer runtime fence unavailable")?;
            let held = !state.attached || state.held(thread);
            let mut status = context.observer_status(&binding)?;
            if held {
                if let Some(proposals) = status["state"]["proposals"].as_array_mut() {
                    for proposal in proposals {
                        proposal["disposition"] = serde_json::json!("historical");
                    }
                }
            }
            status["runtime_held"] = held.into();
            Ok(status)
        })
    }
    /// One bounded safe-point poll; the only wait is the configured debounce.
    /// An interrupted durable attempt is never reconstructed or replayed.
    pub async fn poll_observers(&self, thread: ThreadId) -> Result<serde_json::Value, String> {
        self.poll_observers_inner(thread, None).await
    }
    #[cfg(feature = "qualification")]
    pub async fn qualification_poll_observers_after_compute(
        &self,
        thread: ThreadId,
        arrived: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    ) -> Result<serde_json::Value, String> {
        self.poll_observers_inner(thread, Some((arrived, release)))
            .await
    }
    async fn poll_observers_inner(
        &self,
        thread: ThreadId,
        after_compute: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
    ) -> Result<serde_json::Value, String> {
        let enabled = self.worker.run(|context| Ok(context.observers_enabled()))?;
        if !enabled {
            return Ok(serde_json::json!({"enabled":false,"provider_requests":0}));
        }
        let binding = self.binding(thread)?;
        let generation = match scheduler::generation(&self.runtime, thread) {
            Ok(value) => value,
            Err(_) => return self.observer_status(thread),
        };
        let runtime = self.runtime.clone();
        let scoped = binding.clone();
        let began = std::time::Instant::now();
        let mut selected = self
            .worker
            .run(move |context| context.prepare_observer(&scoped, &runtime, thread, generation))?;
        if let worker::observers::Selection::Debounce(millis) = selected {
            tokio::time::sleep(Duration::from_millis(millis)).await;
            let runtime = self.runtime.clone();
            let scoped = binding.clone();
            selected = self.worker.run(move |context| {
                context.prepare_observer(&scoped, &runtime, thread, generation)
            })?;
        }
        if let worker::observers::Selection::Compute(prepared) = selected {
            let work = prepared.work;
            let mut pending = PendingObservation {
                host: self.clone(),
                work: Some(work.clone()),
            };
            let remaining = work.deadline.get().saturating_sub(worker::now().get());
            let computation = tokio::task::spawn_blocking(move || {
                // A timed-out waiter cannot admit another physical computation
                // until this pure worker has actually stopped.
                let _permit = prepared.permit;
                prepared.source.compute()
            });
            let result =
                match tokio::time::timeout(Duration::from_millis(remaining), computation).await {
                    Ok(Ok(Ok(evidence))) => Ok(evidence),
                    Ok(Ok(Err(error))) => Err(error),
                    _ => Err("local observer computation expired or unavailable".into()),
                };
            if let Some((arrived, release)) = after_compute {
                arrived.notify_one();
                release.notified().await;
            }
            let elapsed = began.elapsed().as_millis().min(u64::MAX as u128) as u64;
            let runtime = self.runtime.clone();
            let scoped = binding.clone();
            self.worker.run_cleanup(move |context| {
                context
                    .complete_observer(&scoped, &runtime, thread, generation, work, result, elapsed)
            })?;
            pending.work = None;
        }
        self.observer_status(thread)
    }
}
