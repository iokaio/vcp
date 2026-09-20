// SPDX-License-Identifier: Apache-2.0
//! Explicit read-only search. Paused roots can inspect retained canonical
//! evidence. No model, indexing, maintenance or canonical mutation starts here.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

struct InspectionControl {
    started: Instant,
    timeout: Duration,
    cancelled: Arc<AtomicBool>,
}
impl InspectionControl {
    fn new(request: &retrieval::Request) -> Result<Self, String> {
        request.validate().map_err(|e| e.to_string())?;
        Ok(Self {
            started: Instant::now(),
            timeout: Duration::from_millis(request.timeout_ms),
            cancelled: Arc::new(AtomicBool::new(false)),
        })
    }
    fn stopped(&self) -> bool {
        self.cancelled.load(Ordering::Acquire) || self.started.elapsed() >= self.timeout
    }
    fn check(&self) -> vcp_memory::Result<()> {
        if self.stopped() {
            Err(vcp_memory::Error::Conflict(
                "memory inspection cancelled or deadline",
            ))
        } else {
            Ok(())
        }
    }
}
struct CancelInspection(Arc<AtomicBool>);
impl Drop for CancelInspection {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
use vcp_memory::{publication::Publisher, retrieval, search_record::ChunkerSpec};

impl CanonicalHost {
    pub async fn inspect_memory(
        &self,
        request: retrieval::Request,
    ) -> Result<retrieval::Response, String> {
        let control = Arc::new(InspectionControl::new(&request)?);
        let _guard = CancelInspection(control.cancelled.clone());
        let capture_control = control.clone();
        let capture_worker = self.worker.clone();
        // Inspection is allowed while the owner is paused. Read authorization is
        // checked by capture/finish, independently of task execution admission.
        let (captured, snapshot, access, directory, sources) = self.worker.run(move |context| {
            let mut access = context.memory_access();
            access.write = false;
            let check = || {
                capture_control.check()?;
                if capture_worker.fenced() {
                    return Err(vcp_memory::Error::Conflict(
                        "memory inspection owner closed",
                    ));
                }
                Ok(())
            };
            let bindings =
                retrieval::source_bindings_with_check(context.engine.store(), &access, &check)?;
            let captured = retrieval::capture(
                context.engine.store(),
                &access,
                &request,
                &bindings.bindings,
                &ChunkerSpec::default(),
                &|| check().is_err(),
            )?;
            Ok((
                captured,
                context.engine.store().snapshot()?,
                access,
                context.config.canonical_root.join("search-generations"),
                bindings,
            ))
        })?;
        let search_control = control.clone();
        let search_worker = self.worker.clone();
        let selected = tokio::task::spawn_blocking(move || -> Result<_, String> {
            let check = || {
                search_control.check()?;
                if search_worker.fenced() {
                    return Err(vcp_memory::Error::Conflict(
                        "memory inspection owner closed",
                    ));
                }
                Ok(())
            };
            check().map_err(|e| e.to_string())?;
            // Immediate admission, no maintenance wait queue. This bounded
            // read has no durable resource receipt, preserving read-only state.
            let _permit = super::memory_vectors::admission()?.acquire(
                vcp_memory::local_resources::Workload {
                    rows: 1024,
                    source_bytes: 0,
                    batch: 1,
                    load_model: false,
                },
            )?;
            let publisher = match std::fs::symlink_metadata(&directory) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.to_string()),
                Ok(_) => Some(Publisher::open_existing(&directory).map_err(|e| e.to_string())?),
            };
            let recovery = publisher
                .as_ref()
                .map(|p| p.recover_snapshot_with_check(&snapshot, &access, &check))
                .transpose()
                .map_err(|e| e.to_string())?;
            // Both manager and View share the canonical-root reader registry;
            // the retained View pin outlives all native candidate operations.
            retrieval::search(
                captured,
                recovery.as_ref().and_then(|r| r.view.as_ref()),
                None,
                &|| check().is_err(),
            )
            .map_err(|e| e.to_string())
        })
        .await
        .map_err(|_| "memory inspection worker failed")??;
        let finish_worker = self.worker.clone();
        self.worker.run(move |context| {
            let stopped = || control.stopped() || finish_worker.fenced();
            let mut access = context.memory_access();
            access.write = false;
            let mut response =
                retrieval::finish(context.engine.store(), &access, selected, &stopped)?;
            response.rebuild_required |= !sources.complete;
            response.degraded.extend(sources.degraded);
            Ok(response)
        })
    }
}

/// Offline CLI inspection uses the same retained-source derivation, noncreating
/// component open and lexical-only query. Its caller already owns the Store.
/// No inference or write is hidden behind a missing generation/assets condition.
pub fn inspect_store(
    store: &vcp_store::Store,
    access: &vcp_memory::access::Access,
    canonical_root: &std::path::Path,
    request: &retrieval::Request,
) -> Result<retrieval::Response, String> {
    let control = InspectionControl::new(request)?;
    let check = || control.check();
    let sources =
        retrieval::source_bindings_with_check(store, access, &check).map_err(|e| e.to_string())?;
    let directory = canonical_root.join("search-generations");
    let _permit =
        super::memory_vectors::admission()?.acquire(vcp_memory::local_resources::Workload {
            rows: 1024,
            source_bytes: 0,
            batch: 1,
            load_model: false,
        })?;
    let publisher = match std::fs::symlink_metadata(&directory) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.to_string()),
        Ok(_) => Some(Publisher::open_existing(&directory).map_err(|e| e.to_string())?),
    };
    let recovery = publisher
        .as_ref()
        .map(|p| p.recover_with_check(store, access, &check))
        .transpose()
        .map_err(|e| e.to_string())?;
    let mut response = retrieval::query(
        store,
        access,
        recovery.as_ref().and_then(|r| r.view.as_ref()),
        request,
        &sources.bindings,
        &ChunkerSpec::default(),
        None,
        &|| control.stopped(),
    )
    .map_err(|e| e.to_string())?;
    response.rebuild_required |= !sources.complete;
    response.degraded.extend(sources.degraded);
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn dropped_inspection_cancels_detached_blocking_work() {
        let control = Arc::new(InspectionControl {
            started: Instant::now(),
            timeout: Duration::from_secs(30),
            cancelled: Arc::new(AtomicBool::new(false)),
        });
        let guard = CancelInspection(control.cancelled.clone());
        let worker = control.clone();
        let (resume, wait) = std::sync::mpsc::channel();
        let detached = tokio::task::spawn_blocking(move || {
            wait.recv().unwrap();
            worker.check()
        });
        drop(guard);
        resume.send(()).unwrap();
        assert!(detached.await.unwrap().is_err());
        let expired = InspectionControl {
            started: Instant::now() - Duration::from_secs(1),
            timeout: Duration::from_millis(1),
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        assert!(expired.check().is_err());
    }
}
