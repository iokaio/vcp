// SPDX-License-Identifier: Apache-2.0
//! Shared process admission and measured local query work. Resource observations
//! are retained canonically; counters are sampled, not allocator quotas.
use super::memory_vectors::{self, ResourceReport};
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use vcp_memory::local_resources::{self, Measurements, Workload};

pub(super) fn run<T>(
    host: &CanonicalHost,
    binding: ThreadBinding,
    workload: Workload,
    operation: impl FnOnce(&dyn Fn() -> Result<(), String>) -> Result<T, String>,
) -> Result<(Result<T, String>, ResourceReport), String> {
    let permit = memory_vectors::admission()?.acquire(workload)?;
    let started = std::time::Instant::now();
    let cpu_start = memory_vectors::process_cpu_millis()?;
    let measurements = std::cell::RefCell::new(Measurements::start());
    let sampling_errors = std::cell::RefCell::new(Vec::new());
    let sample = || -> Result<(), String> {
        match local_resources::snapshot() {
            Ok(row) => {
                measurements.borrow_mut().observe(row, 0);
                Ok(())
            }
            Err(error) => {
                sampling_errors.borrow_mut().push(error.clone());
                Err(error)
            }
        }
    };
    sample()?;
    let mut result = operation(&sample);
    if let Err(error) = sample() {
        result = Err(error);
    }
    // Once native work ran, sampler failure must not erase its durable resource
    // evidence. Retain available counters and label unavailable CPU explicitly.
    let (cpu, cpu_known) = match memory_vectors::process_cpu_millis() {
        Ok(value) => (value.saturating_sub(cpu_start), true),
        Err(error) => {
            sampling_errors.borrow_mut().push(error.clone());
            result = Err(error);
            (0, false)
        }
    };
    let measured = measurements.into_inner();
    let sampling_errors = sampling_errors.into_inner();
    let estimate = permit.estimate();
    let id = ObservationId::new();
    let mut report = ResourceReport {
        observation: id.clone(),
        estimate_version: local_resources::ESTIMATE_VERSION.into(),
        declared_ram_bytes: estimate.ram_bytes,
        declared_temporary_disk_bytes: estimate.temporary_disk_bytes,
        process_cpu_millis: cpu,
        elapsed_millis: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        samples: measured.samples,
        maximum_sample_gap_millis: measured.maximum_gap.as_millis().min(u128::from(u64::MAX))
            as u64,
        sampled_resident_peak_bytes: measured.sampled_build_peak_resident_bytes,
        sampled_private_commit_peak_bytes: measured.sampled_build_peak_private_commit_bytes,
        sampled_mapped_address_peak_bytes: measured.sampled_build_peak_mapped_address_bytes,
        sampled_temporary_disk_peak_bytes: 0,
        process_lifetime_resident_peak_bytes: measured.process_lifetime_peak_resident_bytes,
        process_lifetime_private_commit_peak_bytes: measured
            .process_lifetime_peak_private_commit_bytes,
        observation_retained: false,
        observation_error: (!sampling_errors.is_empty()).then(|| sampling_errors.join("; ")),
    };
    let resources = LocalResources {
        schema_version: 1,
        id,
        scope: binding.scope.clone(),
        agent: binding.agent.clone(),
        cpu_millis: Units::new(cpu),
        peak_ram: ByteCount::new(report.sampled_resident_peak_bytes),
        disk: ByteCount::ZERO,
        source: format!(
            "{};query;{};sampled-process-resident",
            local_resources::ESTIMATE_VERSION,
            if cpu_known {
                "process-cpu-delta"
            } else {
                "cpu-unavailable-zero-placeholder"
            }
        ),
    };
    match host.worker.run_cleanup(move |context| {
        context.validate_binding(&binding)?;
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
                resources,
                &actor,
            ))?;
        Ok(())
    }) {
        Ok(()) => report.observation_retained = true,
        Err(error) => {
            report.observation_error = Some(match report.observation_error.take() {
                Some(prior) => format!("{prior}; resource receipt: {error}"),
                None => error,
            })
        }
    }
    drop(permit);
    if !report.observation_retained {
        result = Err("local query resource observation was not retained".into());
    }
    Ok((result, report))
}

pub struct EmbeddingOutcome {
    pub embedding: Option<super::memory_query::QueryEmbedding>,
    pub failure: Option<String>,
    pub resources: Option<ResourceReport>,
}
impl CanonicalHost {
    /// Explicit opt-in local inference. No inspector calls this API. File-only
    /// assets are supplied by the trusted adapter; missing files remain a failure.
    pub async fn embed_memory_query(
        &self,
        thread: ThreadId,
        assets: PathBuf,
        text: String,
        cancelled: Arc<AtomicBool>,
    ) -> Result<EmbeddingOutcome, String> {
        if text.len() > vcp_memory::embedding::CHUNK_BYTES {
            return Err("local query input exceeds exact embedding chunk limit".into());
        }
        let binding = self.binding(thread)?;
        let generation = self
            .runtime
            .admission_generation(thread)
            .map_err(|_| "memory query owner is held or closed")?;
        if cancelled.load(Ordering::Acquire) || self.scheduler.busy() {
            return Err("local query deferred or cancelled".into());
        }
        let checked = binding.clone();
        let (controller, epoch) = self.worker.run(move |context| {
            context.can_start(&checked)?;
            Ok((
                context.engine.controller().clone(),
                context.engine.owner_epoch(),
            ))
        })?;
        let host = self.clone();
        let stage_cancel = Arc::new(AtomicBool::new(false));
        struct Cancel(Arc<AtomicBool>);
        impl Drop for Cancel {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let _guard = Cancel(stage_cancel.clone());
        let flag = stage_cancel.clone();
        let mut task = tokio::task::spawn_blocking(move || {
            let start = std::time::Instant::now();
            let stopped = || {
                flag.load(Ordering::Acquire)
                    || host.worker.fenced()
                    || host.scheduler.busy()
                    || host.runtime.admission_generation(thread).ok() != Some(generation)
                    || start.elapsed() >= Duration::from_secs(60)
            };
            if stopped() {
                return Err("local query cancelled before model load".into());
            }
            let scope = binding.scope.clone();
            run(
                &host,
                binding,
                Workload {
                    rows: 1,
                    source_bytes: text.len(),
                    batch: 1,
                    load_model: true,
                },
                |sample| {
                    if stopped() {
                        return Err("local query cancelled before model load".into());
                    }
                    let model = vcp_memory::embedding::LocalEmbedding::load(&assets, &stopped)
                        .map_err(|e| e.to_string())?;
                    sample()?;
                    let values = model.query(&text, &stopped);
                    // Sample while model/runtime allocations are still resident,
                    // including on inference cancellation or failure.
                    sample()?;
                    let values = values.map_err(|e| e.to_string())?;
                    let specification =
                        model.specification().digest().map_err(|e| e.to_string())?;
                    drop(model);
                    if stopped() {
                        return Err("local query cancelled after inference".into());
                    }
                    Ok(super::memory_query::QueryEmbedding {
                        text_digest: vcp_protocol::digest_bytes(text.as_bytes()),
                        specification,
                        values,
                        controller,
                        epoch,
                        scope,
                    })
                },
            )
        });
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        let deadline = tokio::time::sleep(Duration::from_secs(60));
        tokio::pin!(deadline);
        let result = loop {
            tokio::select! {
                result = &mut task => break result.map_err(|_| "query embedding worker failed")?,
                _ = &mut deadline, if !stage_cancel.load(Ordering::Acquire) => { stage_cancel.store(true, Ordering::Release); }
                _ = tick.tick() => { if cancelled.load(Ordering::Acquire) || self.worker.fenced() || self.scheduler.busy()
                    || self.runtime.admission_generation(thread).ok() != Some(generation) { stage_cancel.store(true, Ordering::Release); } }
            }
        };
        match result {
            Err(reason) => Ok(EmbeddingOutcome {
                embedding: None,
                failure: Some(reason),
                resources: None,
            }),
            Ok((result, resources)) => {
                let result = if stage_cancel.load(Ordering::Acquire)
                    || cancelled.load(Ordering::Acquire)
                    || self.runtime.admission_generation(thread).ok() != Some(generation)
                {
                    Err("local query cancelled before return".into())
                } else {
                    result
                };
                Ok(match result {
                    Ok(embedding) => EmbeddingOutcome {
                        embedding: Some(embedding),
                        failure: None,
                        resources: Some(resources),
                    },
                    Err(reason) => EmbeddingOutcome {
                        embedding: None,
                        failure: Some(reason),
                        resources: Some(resources),
                    },
                })
            }
        }
    }
}
