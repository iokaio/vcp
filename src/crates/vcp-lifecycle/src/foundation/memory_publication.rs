// SPDX-License-Identifier: Apache-2.0
//! Explicit owner publication: CPU preparation outside the canonical worker.
use super::memory_vectors;
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use vcp_memory::{
    embedding,
    local_resources::{self, Measurements, Workload},
    publication::{self, Publisher},
    search_record,
};
use vcp_store::contract::Receipt;

/// Construct once per owner. Publisher::new shares the underlying pin registry
/// for the same canonical root even if another trusted adapter constructs it.
pub struct Manager {
    publisher: Arc<Publisher>,
    controller: ControllerId,
    epoch: OwnerEpoch,
    root: Scope,
}
pub struct Request {
    pub vectors: memory_vectors::Request,
    /// Explicit degradation policy: failed assets may still publish lexical
    /// coverage, with zero semantic sequence and no acknowledged index intents.
    pub allow_lexical_only: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Published,
    LexicalOnly(String),
    Deferred(String),
    Cancelled,
}
pub struct Outcome {
    pub status: Status,
    pub manifest: Option<vcp_domain::search::Generation>,
    pub receipt: Option<Receipt>,
    pub resources: Option<memory_vectors::ResourceReport>,
    pub publication_resources: Option<memory_vectors::ResourceReport>,
}
struct CancelStage(Arc<AtomicBool>);
impl Drop for CancelStage {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

impl CanonicalHost {
    pub fn publication_manager(
        &self,
        thread: ThreadId,
        publisher: Arc<Publisher>,
    ) -> Result<Arc<Manager>, String> {
        let binding = self.binding(thread)?;
        self.worker.run(move |context| {
            context.can_start(&binding)?;
            if binding.scope.task != context.config.root_task {
                return Err("publication requires root owner".into());
            }
            Ok(Arc::new(Manager {
                publisher,
                controller: context.engine.controller().clone(),
                epoch: context.engine.owner_epoch(),
                root: binding.scope,
            }))
        })
    }

    pub async fn publish_memory(
        &self,
        thread: ThreadId,
        manager: Arc<Manager>,
        request: Request,
    ) -> Result<Outcome, String> {
        let binding = self.binding(thread)?;
        if binding.scope != manager.root {
            return Err("publication manager belongs to another root".into());
        }
        let checked = manager.clone();
        self.worker.run(move |context| {
            if context.engine.controller() != &checked.controller
                || context.engine.owner_epoch() != checked.epoch
            {
                return Err("publication manager belongs to another owner epoch".into());
            }
            Ok(())
        })?;
        let sources = request.vectors.sources.clone();
        let chunker = request.vectors.chunker.clone();
        let external = request.vectors.cancelled.clone();
        let generation = self
            .runtime
            .admission_generation(thread)
            .map_err(|_| "publication owner is held or closed")?;
        let initial_sources = sources.clone();
        let initial_chunker = chunker.clone();
        let initial_binding = binding.clone();
        let empty = self.worker.run(move |context| {
            context.can_start(&initial_binding)?;
            let inventory = search_record::inventory(
                context.engine.store(),
                &context.memory_access(),
                &initial_sources,
                &initial_chunker,
                search_record::Limits::default(),
            )?;
            Ok(inventory.records.is_empty())
        })?;
        let vector_result = if empty {
            memory_vectors::Outcome {
                status: memory_vectors::Status::Ready,
                built: None,
                resources: None,
            }
        } else {
            self.build_memory_vectors(thread, request.vectors).await?
        };
        let early = |status, resources| Outcome {
            status,
            manifest: None,
            receipt: None,
            resources,
            publication_resources: None,
        };
        let degradation = match &vector_result.status {
            memory_vectors::Status::Ready => None,
            memory_vectors::Status::Failed(reason) if request.allow_lexical_only => {
                Some(reason.clone())
            }
            memory_vectors::Status::Cancelled => {
                return Ok(early(Status::Cancelled, vector_result.resources))
            }
            memory_vectors::Status::Deferred(reason) | memory_vectors::Status::Failed(reason) => {
                return Ok(early(
                    Status::Deferred(reason.clone()),
                    vector_result.resources,
                ));
            }
        };
        if self.scheduler.busy() {
            return Ok(early(
                Status::Deferred("interactive work is queued".into()),
                vector_result.resources,
            ));
        }
        if external.load(Ordering::Acquire)
            || self.runtime.admission_generation(thread).ok() != Some(generation)
        {
            return Ok(early(Status::Cancelled, vector_result.resources));
        }
        if !empty
            && vector_result
                .resources
                .as_ref()
                .is_none_or(|report| !report.observation_retained)
        {
            return Ok(early(
                Status::Deferred("local resource observation was not retained".into()),
                vector_result.resources,
            ));
        }
        let checked = binding.clone();
        let prior = vector_result
            .built
            .as_ref()
            .map(|built| built.inventory.clone());
        let manager_check = manager.clone();
        let vector_observation = vector_result
            .resources
            .as_ref()
            .map(|report| report.observation.clone());
        let (snapshot, workload) = self.worker.run(move |context| {
            context.can_start(&checked)?;
            if context.engine.controller() != &manager_check.controller
                || context.engine.owner_epoch() != manager_check.epoch
            {
                return Err("publication owner epoch changed".into());
            }
            let access = context.memory_access();
            let fresh = search_record::inventory(
                context.engine.store(),
                &access,
                &sources,
                &chunker,
                search_record::Limits::default(),
            )?;
            if empty && !fresh.records.is_empty() {
                return Err("empty publication inventory changed".into());
            }
            if let Some(prior) = prior {
                let mut comparable = fresh.records.clone();
                for (current, previous) in comparable.iter_mut().zip(&prior.records) {
                    if current.kind == search_record::SearchKind::Source {
                        current.watermark = previous.watermark;
                    }
                }
                if fresh.workspace != prior.workspace
                    || fresh.authority != prior.authority
                    || fresh.deletion != prior.deletion
                    || fresh.chunker_digest != prior.chunker_digest
                    || comparable != prior.records
                    || fresh.exclusions != prior.exclusions
                    || fresh.watermark != prior.watermark.next()?
                {
                    return Err("source inventory changed beyond the local resource receipt".into());
                }
                let events: Vec<_> = context
                    .engine
                    .store()
                    .state()
                    .events
                    .iter()
                    .filter(|event| event.watermark > prior.watermark)
                    .collect();
                if events.len() != 1
                    || events[0].event.kind
                        != vcp_protocol::event::EventKind::LocalResourcesObserved
                    || events[0].event.data["local_resources"]["id"]
                        != serde_json::json!(vector_observation)
                {
                    return Err("publication cut advanced beyond resource observation".into());
                }
            }
            let chunks = embedding::inventory_chunks(&fresh)?;
            let workload = Workload {
                rows: chunks.len().max(1),
                source_bytes: chunks.iter().map(|chunk| chunk.text.len()).sum(),
                batch: 16,
                load_model: false,
            };
            Ok((
                publication::capture(context.engine.store(), &access, &checked.scope, fresh)?,
                workload,
            ))
        })?;
        // Reuse the same process-wide admission pool for lexical preparation and
        // component verification; no separate scheduler or hidden wait queue.
        let permit = match memory_vectors::admission()?.acquire(workload) {
            Ok(permit) => permit,
            Err(reason) => return Ok(early(Status::Deferred(reason), vector_result.resources)),
        };
        let stage_cancel = Arc::new(AtomicBool::new(false));
        let _cancel_on_drop = CancelStage(stage_cancel.clone());
        let stage_flag = stage_cancel.clone();
        let stage_publisher = manager.publisher.clone();
        let built = vector_result.built;
        let observer = self.clone();
        let stage_binding = binding.clone();
        let mut stage = tokio::task::spawn_blocking(move || -> Result<_, String> {
            let started = std::time::Instant::now();
            let cpu_start = memory_vectors::process_cpu_millis()?;
            let baseline: std::collections::BTreeSet<_> =
                std::fs::read_dir(stage_publisher.storage_root())
                    .map_err(|e| e.to_string())?
                    .map(|entry| entry.map(|e| e.path()))
                    .collect::<std::io::Result<_>>()
                    .map_err(|e| e.to_string())?;
            let measurements = std::cell::RefCell::new(Measurements::start());
            // Samples count only this stage's newly created private generation.
            // This is a sampled overrun check, not an OS filesystem quota.
            let sample = || -> Result<(), String> {
                let mut disk = 0u64;
                for entry in
                    std::fs::read_dir(stage_publisher.storage_root()).map_err(|e| e.to_string())?
                {
                    let path = entry.map_err(|e| e.to_string())?.path();
                    if !baseline.contains(&path) {
                        disk = disk
                            .checked_add(local_resources::temporary_disk_bytes(&path)?)
                            .ok_or("publication disk count overflow")?;
                    }
                }
                measurements
                    .borrow_mut()
                    .observe(local_resources::snapshot()?, disk);
                permit.check_temporary_disk(disk)
            };
            let sample_error = std::cell::RefCell::new(None);
            let barrier = |_: publication::Barrier| {
                if let Err(error) = sample() {
                    *sample_error.borrow_mut() = Some(error);
                    stage_flag.store(true, Ordering::Release);
                }
                if started.elapsed() >= Duration::from_secs(60)
                    || observer.scheduler.busy()
                    || observer.worker.fenced()
                    || observer.runtime.admission_generation(thread).ok() != Some(generation)
                {
                    stage_flag.store(true, Ordering::Release);
                }
            };
            let mut result = (|| {
                sample()?;
                let prepared = match built {
                    Some(built) => stage_publisher.prepare_with_vectors(
                        snapshot,
                        &built.directory.join("vectors.json"),
                        &built.checksum,
                        &stage_flag,
                        &barrier,
                    ),
                    None => stage_publisher.prepare(snapshot, None, &stage_flag, &barrier),
                }
                .map_err(|e| e.to_string())?;
                stage_publisher
                    .validate_prepared(prepared, &stage_flag)
                    .map_err(|e| e.to_string())
            })();
            let mut resource_error = sample_error.borrow_mut().take();
            if let Err(error) = sample() {
                resource_error = Some(error);
            }
            let resource_failed = resource_error.is_some();
            if let Some(error) = resource_error {
                result = Err(error);
            }
            let measured = measurements.into_inner();
            let cpu = memory_vectors::process_cpu_millis()?.saturating_sub(cpu_start);
            let estimate = permit.estimate();
            let id = ObservationId::new();
            let mut report = memory_vectors::ResourceReport {
                observation: id.clone(),
                estimate_version: local_resources::ESTIMATE_VERSION.into(),
                declared_ram_bytes: estimate.ram_bytes,
                declared_temporary_disk_bytes: estimate.temporary_disk_bytes,
                process_cpu_millis: cpu,
                elapsed_millis: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                samples: measured.samples,
                maximum_sample_gap_millis: measured
                    .maximum_gap
                    .as_millis()
                    .min(u128::from(u64::MAX)) as u64,
                sampled_resident_peak_bytes: measured.sampled_build_peak_resident_bytes,
                sampled_private_commit_peak_bytes: measured.sampled_build_peak_private_commit_bytes,
                sampled_mapped_address_peak_bytes: measured.sampled_build_peak_mapped_address_bytes,
                sampled_temporary_disk_peak_bytes: measured.sampled_build_peak_temporary_disk_bytes,
                process_lifetime_resident_peak_bytes: measured.process_lifetime_peak_resident_bytes,
                process_lifetime_private_commit_peak_bytes: measured
                    .process_lifetime_peak_private_commit_bytes,
                observation_retained: false,
                observation_error: None,
            };
            let observation = LocalResources {
                schema_version: 1,
                id,
                scope: stage_binding.scope.clone(),
                agent: stage_binding.agent.clone(),
                cpu_millis: Units::new(cpu),
                peak_ram: ByteCount::new(report.sampled_resident_peak_bytes),
                disk: ByteCount::new(report.sampled_temporary_disk_peak_bytes),
                source: format!(
                    "{};publication;process-cpu-delta;sampled-process-resident",
                    local_resources::ESTIMATE_VERSION
                ),
            };
            match observer.worker.run_cleanup(move |context| {
                context.validate_binding(&stage_binding)?;
                let actor = vcp_budget::Actor {
                    id: context.config.actor.clone(),
                    now: Timestamp::new(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis()
                            .min(u128::from(u64::MAX)) as u64,
                    ),
                };
                context
                    .runtime
                    .block_on(vcp_budget::record_local_resources(
                        context.engine.store_mut(),
                        observation,
                        &actor,
                    ))?;
                Ok(())
            }) {
                Ok(()) => report.observation_retained = true,
                Err(error) => report.observation_error = Some(error),
            }
            drop(permit);
            Ok((result, report, resource_failed))
        });
        // Native calls remain nonpreemptible; checkpoints bound subsequent work
        // and this deadline requests cancellation even while a batch is running.
        let deadline = tokio::time::sleep(Duration::from_secs(60));
        tokio::pin!(deadline);
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        let (validated, publication_resources, resource_failed) = loop {
            tokio::select! {
                result=&mut stage=>break result.map_err(|_|"publication preparation worker failed")??,
                _=&mut deadline, if !stage_cancel.load(Ordering::Acquire)=>{
                    stage_cancel.store(true, Ordering::Release);
                }
                _=tick.tick()=>{
                    if external.load(Ordering::Acquire)||self.worker.fenced()||self.scheduler.busy()
                        ||self.runtime.admission_generation(thread).ok()!=Some(generation) {stage_cancel.store(true,Ordering::Release);}
                }
            }
        };
        let early_stage = |status, resources| Outcome {
            status,
            manifest: None,
            receipt: None,
            resources,
            publication_resources: Some(publication_resources.clone()),
        };
        if !publication_resources.observation_retained {
            return Ok(early_stage(
                Status::Deferred("publication resource observation was not retained".into()),
                vector_result.resources,
            ));
        }
        let validated = match validated {
            Ok(validated) => validated,
            Err(reason) => {
                return Ok(early_stage(
                    if resource_failed {
                        Status::Deferred(reason)
                    } else if stage_cancel.load(Ordering::Acquire) {
                        Status::Cancelled
                    } else {
                        Status::Deferred(reason)
                    },
                    vector_result.resources,
                ))
            }
        };
        if stage_cancel.load(Ordering::Acquire)
            || external.load(Ordering::Acquire)
            || self.runtime.admission_generation(thread).ok() != Some(generation)
        {
            return Ok(early_stage(Status::Cancelled, vector_result.resources));
        }
        let manifest = validated.manifest().clone();
        let checked = binding;
        let owning = manager;
        let runtime = self.runtime.clone();
        let stage_observation = publication_resources.observation.clone();
        let receipt = self.worker.run(move |context| {
            context.can_start(&checked)?;
            let following: Vec<_> = context
                .engine
                .store()
                .state()
                .events
                .iter()
                .filter(|event| event.watermark > validated.manifest().canonical_watermark)
                .collect();
            if following.len() != 1
                || following[0].event.kind != vcp_protocol::event::EventKind::LocalResourcesObserved
                || following[0].event.data["local_resources"]["id"]
                    != serde_json::json!(stage_observation)
            {
                return Err("publication cut advanced beyond its resource observation".into());
            }
            if context.engine.controller() != &owning.controller
                || context.engine.owner_epoch() != owning.epoch
                || runtime.admission_generation(thread).ok() != Some(generation)
                || external.load(Ordering::Acquire)
                || context.engine.store().state().watermark
                    != validated.manifest().canonical_watermark.next()?
            {
                return Err("publication admission or canonical cut changed".into());
            }
            let access = context.memory_access();
            let now = Timestamp::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
                    .min(u128::from(u64::MAX)) as u64,
            );
            Ok(context.runtime.block_on(owning.publisher.activate(
                context.engine.store_mut(),
                &access,
                &validated,
                now,
                &|_| {},
            ))?)
        })?;
        Ok(Outcome {
            status: degradation.map_or(Status::Published, Status::LexicalOnly),
            manifest: Some(manifest),
            receipt: Some(receipt),
            resources: vector_result.resources,
            publication_resources: Some(publication_resources),
        })
    }
}
