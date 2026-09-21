// SPDX-License-Identifier: Apache-2.0
//! Explicit context selection. Read-only inspectors use the raw retrieval API;
//! they never call this capturing host adapter or start query inference.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use vcp_context::manifest::{Part, Sealed};
use vcp_memory::{
    publication::View,
    retrieval,
    search_record::{ChunkerSpec, SourceBinding},
};

/// Only the canonical worker can issue this source capability. Serializing a
/// retrieval response or copying its public Fence does not create a send permit.
#[derive(Clone)]
pub(super) struct SendFence {
    pub(super) controller: ControllerId,
    pub(super) epoch: OwnerEpoch,
    pub(super) scope: Scope,
    pub(super) source: retrieval::Fence,
    pub(super) part: Part,
    pub(super) routing_policy: Option<String>,
}

pub struct Selection {
    pub(super) response: retrieval::Response,
    pub resources: Option<super::memory_vectors::ResourceReport>,
    pub(super) fence: Option<SendFence>,
}
impl Selection {
    pub fn response(&self) -> &retrieval::Response {
        &self.response
    }
    /// Add this complete evidence part to ordinary context assembly. Metadata,
    /// dispute/inference labels and evidence references remain inside the
    /// captured bytes; callers cannot silently remove them from the capability.
    pub fn part(&self) -> Option<&Part> {
        self.fence.as_ref().map(|f| &f.part)
    }
}

/// A local query vector carries the exact admitted input digest and model spec.
/// Construction is restricted to the resource-accounted host embedding path.
pub struct QueryEmbedding {
    pub(super) text_digest: String,
    pub(super) specification: String,
    pub(super) values: Vec<f32>,
    pub(super) controller: ControllerId,
    pub(super) epoch: OwnerEpoch,
    pub(super) scope: Scope,
}

pub struct Query {
    pub request: retrieval::Request,
    pub sources: Vec<SourceBinding>,
    pub chunker: ChunkerSpec,
    pub embedding: Option<QueryEmbedding>,
    pub cancelled: Arc<AtomicBool>,
}

struct CancelOnDrop(Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

impl CanonicalHost {
    /// Explicit native component reopen under the same root pin registry used
    /// by publication/GC. Canonical snapshot capture is short; checksums and
    /// native readers open outside the canonical worker under local admission.
    pub async fn open_memory_view(
        &self,
        thread: ThreadId,
        publisher: Arc<vcp_memory::publication::Publisher>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<
        (
            vcp_memory::publication::Recovery,
            super::memory_vectors::ResourceReport,
        ),
        String,
    > {
        let binding = self.binding(thread)?;
        let generation = self
            .runtime
            .admission_generation(thread)
            .map_err(|_| "memory reader owner is held")?;
        if cancelled.load(Ordering::Acquire) || self.scheduler.busy() {
            return Err("memory reader deferred or cancelled".into());
        }
        let checked = binding.clone();
        let (snapshot, access) = self.worker.run(move |context| {
            context.can_start(&checked)?;
            Ok((context.engine.store().snapshot()?, context.memory_access()))
        })?;
        let host = self.clone();
        let stopped = Arc::new(AtomicBool::new(false));
        let _guard = CancelOnDrop(stopped.clone());
        let stage = stopped.clone();
        let mut work = tokio::task::spawn_blocking(move || {
            let stop = || {
                stage.load(Ordering::Acquire)
                    || host.scheduler.busy()
                    || host.worker.fenced()
                    || host.runtime.admission_generation(thread).ok() != Some(generation)
            };
            super::memory_query_resources::run(
                &host,
                binding,
                vcp_memory::local_resources::Workload {
                    rows: 1024,
                    source_bytes: 0,
                    batch: 1,
                    load_model: false,
                },
                |_sample| {
                    if stop() {
                        return Err("memory reader cancelled before reopen".into());
                    }
                    let recovery = publisher
                        .recover_snapshot_with_check(&snapshot, &access, &|| {
                            if stop() {
                                Err(vcp_memory::Error::Conflict("memory reader cancelled"))
                            } else {
                                Ok(())
                            }
                        })
                        .map_err(|e| e.to_string())?;
                    if stop() {
                        return Err("memory reader cancelled after reopen".into());
                    }
                    Ok(recovery)
                },
            )
        });
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        let deadline = tokio::time::sleep(Duration::from_secs(60));
        tokio::pin!(deadline);
        let (recovery, resources) = loop {
            tokio::select! {
                result = &mut work => break result.map_err(|_| "memory reader worker failed")??,
                _ = &mut deadline, if !stopped.load(Ordering::Acquire) => { stopped.store(true, Ordering::Release); }
                _ = tick.tick() => { if cancelled.load(Ordering::Acquire) || self.worker.fenced() || self.scheduler.busy()
                    || self.runtime.admission_generation(thread).ok() != Some(generation) { stopped.store(true, Ordering::Release); } }
            }
        };
        if stopped.load(Ordering::Acquire) || cancelled.load(Ordering::Acquire) {
            return Err("memory reader cancelled before return".into());
        }
        Ok((recovery?, resources))
    }

    /// The supplied View is already opened and pinned by the trusted root
    /// publication manager. This method neither rebuilds nor loads a model.
    pub async fn query_memory_context(
        &self,
        thread: ThreadId,
        view: Arc<View>,
        query: Query,
    ) -> Result<Selection, String> {
        let binding = self.binding(thread)?;
        let generation = self
            .runtime
            .admission_generation(thread)
            .map_err(|_| "memory query owner is held or closed")?;
        if self.scheduler.busy() {
            return Err("memory query yields to interactive work".into());
        }
        let external = query.cancelled.clone();
        if external.load(Ordering::Acquire) {
            return Err("memory query cancelled".into());
        }
        let checked = binding.clone();
        let mut request = query.request;
        let embedding = query.embedding;
        let prepared = self.worker.run(move |context| {
            context.can_start(&checked)?;
            request.validate()?;
            let (routing_policy, limits) = context.memory_query_policy()?;
            request.results = request.results.min(limits.results as usize);
            request.tokens = request.tokens.min(limits.tokens.get() as usize);
            request.bytes = request.bytes.min(limits.bytes.get() as usize);
            if let Some(vector) = &embedding {
                if vector.controller != *context.engine.controller()
                    || vector.epoch != context.engine.owner_epoch()
                    || vector.scope != checked.scope
                    || vector.text_digest != vcp_protocol::digest_bytes(request.text.as_bytes())
                {
                    return Err("query embedding belongs to another input or owner".into());
                }
            }
            context.validate_memory_bindings(&checked, &query.sources)?;
            let captured = retrieval::capture(
                context.engine.store(),
                &context.memory_access(),
                &request,
                &query.sources,
                &query.chunker,
                &|| false,
            )?;
            Ok((captured, embedding, routing_policy))
        })?;
        let routing_policy = prepared.2.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let _cancel_on_drop = CancelOnDrop(cancelled.clone());
        let stage_cancel = cancelled.clone();
        let host = self.clone();
        let measured_binding = binding.clone();
        let mut work = tokio::task::spawn_blocking(move || {
            let (captured, embedding, _) = prepared;
            let stopped = || {
                stage_cancel.load(Ordering::Acquire)
                    || host.worker.fenced()
                    || host.scheduler.busy()
                    || host.runtime.admission_generation(thread).ok() != Some(generation)
            };
            let vector = embedding.as_ref().map(|e| retrieval::QueryVector {
                specification: &e.specification,
                values: &e.values,
            });
            super::memory_query_resources::run(
                &host,
                measured_binding,
                vcp_memory::local_resources::Workload {
                    rows: view.vector.as_ref().map_or(1, |v| v.rows().len().max(1)),
                    source_bytes: 0,
                    batch: 1,
                    load_model: false,
                },
                |_sample| {
                    retrieval::search(captured, Some(&view), vector, &stopped)
                        .map_err(|e| e.to_string())
                },
            )
        });
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        let (selected, resources) = loop {
            tokio::select! {
                result = &mut work => break result.map_err(|_| "memory query worker failed")??,
                _ = tick.tick() => {
                    if external.load(Ordering::Acquire) || self.worker.fenced() || self.scheduler.busy()
                        || self.runtime.admission_generation(thread).ok() != Some(generation) {
                        cancelled.store(true, Ordering::Release);
                    }
                }
            }
        };
        let selected = selected?;
        let runtime = self.runtime.clone();
        self.worker.run(move |context| {
            context.can_start(&binding)?;
            if context.memory_query_policy()?.0 != routing_policy {
                return Err("memory query routing policy changed before materialization".into());
            }
            if external.load(Ordering::Acquire)
                || cancelled.load(Ordering::Acquire)
                || runtime.admission_generation(thread).ok() != Some(generation)
            {
                return Err("memory query cancelled before materialization".into());
            }
            let response = retrieval::finish(
                context.engine.store(),
                &context.memory_access(),
                selected,
                &|| external.load(Ordering::Acquire),
            )?;
            let mut selection =
                context.capture_memory_selection(&binding, response, routing_policy)?;
            selection.resources = Some(resources);
            Ok(selection)
        })
    }

    /// Prepare the ordinary provider context with its opaque selected-source
    /// fence. The worker repeats canonical and live-filesystem checks at send.
    pub fn prepare_memory_context(
        &self,
        thread: ThreadId,
        sealed: Sealed,
        schemas: serde_json::Value,
        roots: Vec<vcp_repository::Root>,
        selection: Selection,
    ) -> Result<(), String> {
        let binding = self.binding(thread)?;
        self.worker.run(move |context| {
            context.prepare_context_with_memory(&binding, sealed, schemas, roots, selection.fence)
        })
    }
}
