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

/// Search-only failure classification; no public wire types cross this seam.
#[derive(Debug)]
pub(super) enum SearchError {
    Memory(vcp_memory::Error),
    Resource,
    Worker,
}
impl std::fmt::Display for SearchError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Memory(e) => write!(out, "{e}"),
            Self::Resource => out.write_str("memory inspection resources unavailable"),
            Self::Worker => out.write_str("memory inspection worker failed"),
        }
    }
}
pub(super) async fn search_captured(
    captured: retrieval::Capture,
    access: vcp_memory::access::Access,
    directory: std::path::PathBuf,
    stopped: Arc<dyn Fn() -> bool + Send + Sync>,
) -> std::result::Result<retrieval::Selection, SearchError> {
    tokio::task::spawn_blocking(move || {
        let check = || {
            if stopped() {
                Err(vcp_memory::Error::Conflict("memory inspection interrupted"))
            } else {
                Ok(())
            }
        };
        check().map_err(SearchError::Memory)?;
        let _permit = super::memory_vectors::admission()
            .map_err(|_| SearchError::Resource)?
            .acquire(vcp_memory::local_resources::Workload {
                rows: 1024,
                source_bytes: 0,
                batch: 1,
                load_model: false,
            })
            .map_err(|_| SearchError::Resource)?;
        let publisher = match std::fs::symlink_metadata(&directory) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err(SearchError::Worker),
            Ok(_) => Some(Publisher::open_existing(&directory).map_err(SearchError::Memory)?),
        };
        match publisher {
            Some(publisher) => publisher.search_captured_with_check(&access, captured, &check),
            None => retrieval::search(captured, None, None, &|| stopped()),
        }
        .map_err(SearchError::Memory)
    })
    .await
    .map_err(|_| SearchError::Worker)?
}

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
        let (captured, access, directory, sources) = self.worker.run(move |context| {
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
                access,
                context.config.canonical_root.join("search-generations"),
                bindings,
            ))
        })?;
        let search_control = control.clone();
        let search_worker = self.worker.clone();
        let selected = search_captured(
            captured,
            access,
            directory,
            Arc::new(move || search_control.stopped() || search_worker.fenced()),
        )
        .await
        .map_err(|e| e.to_string())?;
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
