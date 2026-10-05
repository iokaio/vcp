// SPDX-License-Identifier: Apache-2.0
//! Trusted metadata-only charge retrieval, outside canonical transactions.
use super::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, future::Future, pin::Pin, time::Instant};
use vcp_models::reconciliation::Expected;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingCharge {
    pub attempt: AttemptId,
    pub scope: Scope,
    pub expected: Expected,
}

pub enum ReceiptFetch {
    Found(Vec<u8>),
    /// Includes 404 and temporary metadata absence; it never means zero charge.
    Unavailable,
}

pub trait ReceiptSource: Send + Sync {
    /// Implementations perform only an authenticated, bounded generation GET.
    /// They must reject redirects, avoid inference, and omit credentials/errors
    /// from diagnostics and retained bytes.
    fn fetch<'a>(
        &'a self,
        request_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<ReceiptFetch, String>> + Send + 'a>>;
}

pub(super) struct ReceiptRuntime {
    source: Arc<dyn ReceiptSource>,
    checked: tokio::sync::Mutex<BTreeMap<AttemptId, Instant>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ReconciliationStatus {
    Settled { attempt: AttemptId, amount: Micros },
    Unknown { attempt: AttemptId },
}

#[derive(Default, Debug, Serialize)]
pub struct ReconciliationReport {
    pub observations: Vec<ReconciliationStatus>,
}

impl CanonicalHost {
    pub fn configure_receipt_source(&self, source: Arc<dyn ReceiptSource>) -> Result<(), String> {
        let mut configured = self
            .receipt_source
            .lock()
            .map_err(|_| "charge receipt source lock failed")?;
        if configured.is_some() {
            return Err("charge receipt transport already configured".into());
        }
        *configured = Some(Arc::new(ReceiptRuntime {
            source,
            checked: tokio::sync::Mutex::new(BTreeMap::new()),
        }));
        Ok(())
    }

    pub fn pending_provider_charges(&self) -> Result<Vec<PendingCharge>, String> {
        self.worker
            .run_cleanup(|context| context.pending_provider_charges())
    }

    /// Trusted host API. Raw receipt bytes are revalidated against a durable
    /// failed identity, exact admission and current history access before usage.
    pub fn reconcile_provider_receipt(
        &self,
        pending: PendingCharge,
        raw: Vec<u8>,
    ) -> Result<ReconciliationStatus, String> {
        self.worker
            .run_cleanup(move |context| context.reconcile_provider_receipt(pending, raw))
    }

    pub async fn reconcile_pending(
        &self,
        thread: ThreadId,
    ) -> Result<ReconciliationReport, String> {
        // Resolve the registered owner, including paused owners. Accounting is
        // permitted while paused; this API cannot restart inference or tools.
        let _binding = self.binding(thread)?;
        self.reconcile_pending_inner().await
    }

    /// Billing-only maintenance for an exclusively owned paused root. This
    /// deliberately needs no retained thread or admission registration: an
    /// unfinished capture may fence inference while still requiring settlement.
    pub async fn reconcile_root_pending(&self) -> Result<ReconciliationReport, String> {
        self.worker.run_cleanup(|context| {
            let task: vcp_domain::task::Task = context
                .engine
                .store()
                .current()
                .record(
                    vcp_store::contract::Collection::Task,
                    context.config.root_task.as_str(),
                    &context.config.workspace,
                )?
                .decode()?;
            if task.scope.session != context.config.session
                || task.scope.workspace != context.config.workspace
                || task.scope.task != context.config.root_task
                || task.parent.is_some()
                || task.root != task.scope.task
                || task.state != vcp_domain::task::TaskState::Paused
            {
                return Err("cost reconciliation requires the selected paused root task".into());
            }
            Ok(())
        })?;
        self.reconcile_pending_inner().await
    }

    async fn reconcile_pending_inner(&self) -> Result<ReconciliationReport, String> {
        let replayed = self
            .worker
            .run_cleanup(|context| context.replay_provider_receipts())?;
        let mut report = ReconciliationReport {
            observations: replayed,
        };
        let source = self
            .receipt_source
            .lock()
            .map_err(|_| "charge receipt source lock failed")?
            .clone();
        let Some(source) = source else {
            return Ok(report);
        };
        let pending = self.pending_provider_charges()?;
        // One host retrieves at a time, and attempts get at most one lookup per
        // thirty seconds. Other processes have separate canonical ownership.
        let mut checked = source.checked.lock().await;
        let current: std::collections::BTreeSet<_> =
            pending.iter().map(|p| p.attempt.clone()).collect();
        checked.retain(|attempt, _| current.contains(attempt));
        let mut looked_up = 0;
        for pending in pending {
            if looked_up >= 3 {
                break;
            }
            if checked
                .get(&pending.attempt)
                .is_some_and(|last| last.elapsed() < Duration::from_secs(30))
            {
                continue;
            }
            checked.insert(pending.attempt.clone(), Instant::now());
            looked_up += 1;
            // The source timeout is additionally bounded here, regardless of a
            // transport implementation's policy. No store lock crosses await.
            let fetched = tokio::time::timeout(
                Duration::from_secs(2),
                source.source.fetch(&pending.expected.request_id),
            )
            .await;
            let status = match fetched {
                Ok(Ok(ReceiptFetch::Found(raw))) => self
                    .reconcile_provider_receipt(pending.clone(), raw)
                    .unwrap_or(ReconciliationStatus::Unknown {
                        attempt: pending.attempt.clone(),
                    }),
                _ => ReconciliationStatus::Unknown {
                    attempt: pending.attempt.clone(),
                },
            };
            report.observations.push(status);
        }
        Ok(report)
    }
}
