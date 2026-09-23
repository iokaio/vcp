// SPDX-License-Identifier: Apache-2.0
//! Explicit owner-admitted local CPU work. Status inspection never calls this.
use super::*;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        OnceLock,
    },
    time::Instant,
};
use vcp_memory::{
    embedding::{self, LocalEmbedding, Specification},
    local_resources::{self, Admission, Measurements, Workload},
    search_record::{self, ChunkerSpec, Inventory, SourceBinding},
    vector::Component,
};
use vcp_store::contract::Collection;

static ADMISSION: OnceLock<Result<Admission, String>> = OnceLock::new();
pub(super) fn admission() -> Result<&'static Admission, String> {
    ADMISSION
        .get_or_init(|| Admission::new(local_resources::Limits::default()))
        .as_ref()
        .map_err(Clone::clone)
}

pub struct Request {
    /// Trusted host-selected, explicitly provisioned local model directory.
    pub assets: PathBuf,
    /// Existing owner-private directory; each call creates its own new child.
    pub private_root: PathBuf,
    pub sources: Vec<SourceBinding>,
    pub chunker: ChunkerSpec,
    pub cancelled: Arc<AtomicBool>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Ready,
    Deferred(String),
    Cancelled,
    Failed(String),
}
pub struct Built {
    pub directory: PathBuf,
    pub checksum: String,
    pub inventory: Inventory,
    pub specification: String,
    pub rows: usize,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct ResourceReport {
    pub observation: ObservationId,
    pub estimate_version: String,
    pub declared_ram_bytes: u64,
    pub declared_temporary_disk_bytes: u64,
    /// Process-wide CPU delta includes concurrent interactive threads.
    pub process_cpu_millis: u64,
    pub elapsed_millis: u64,
    pub samples: u64,
    pub maximum_sample_gap_millis: u64,
    pub sampled_resident_peak_bytes: u64,
    pub sampled_private_commit_peak_bytes: u64,
    pub sampled_mapped_address_peak_bytes: u64,
    pub sampled_temporary_disk_peak_bytes: u64,
    pub process_lifetime_resident_peak_bytes: u64,
    pub process_lifetime_private_commit_peak_bytes: u64,
    pub observation_retained: bool,
    pub observation_error: Option<String>,
}
pub struct Outcome {
    pub status: Status,
    pub built: Option<Built>,
    pub resources: Option<ResourceReport>,
}
impl Outcome {
    fn early(status: Status) -> Self {
        Self {
            status,
            built: None,
            resources: None,
        }
    }
}
struct AbortOnDrop(Arc<AtomicBool>);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

fn private_directory(root: &Path) -> Result<PathBuf, String> {
    let metadata =
        std::fs::symlink_metadata(root).map_err(|_| "local vector private root missing")?;
    use std::os::windows::fs::MetadataExt;
    if !metadata.is_dir() || metadata.file_attributes() & 0x400 != 0 {
        return Err("local vector private root is not an owned directory".into());
    }
    let root = root
        .canonicalize()
        .map_err(|_| "local vector root cannot be resolved")?;
    let directory = root.join(GenerationId::new().as_str());
    std::fs::create_dir(&directory)
        .map_err(|_| "local vector private directory creation failed")?;
    if directory
        .canonicalize()
        .map_err(|_| "local vector private directory cannot be resolved")?
        .parent()
        != Some(root.as_path())
    {
        return Err("local vector private directory escaped root".into());
    }
    Ok(directory)
}

/// Reads only process accounting counters; this is not a CPU allocation quota.
pub(super) fn process_cpu_millis() -> Result<u64, String> {
    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn GetProcessTimes(
            process: isize,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }
    let mut creation = FileTime { low: 0, high: 0 };
    let mut exit = FileTime { low: 0, high: 0 };
    let mut kernel = FileTime { low: 0, high: 0 };
    let mut user = FileTime { low: 0, high: 0 };
    // SAFETY: pseudo process handle is valid; all outputs are initialized writable FILETIMEs.
    if unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    } == 0
    {
        return Err("process CPU observation unavailable".into());
    }
    let ticks = |time: FileTime| (u64::from(time.high) << 32) | u64::from(time.low);
    Ok(ticks(kernel).saturating_add(ticks(user)) / 10_000)
}

impl CanonicalHost {
    /// CPU inference and graph construction run outside the canonical owner
    /// thread. The permit remains inside the blocking task through model drop,
    /// including when the caller drops this future. In-flight native units are
    /// not preemptible; cancellation stops the next qualified bounded unit.
    pub async fn build_memory_vectors(
        &self,
        thread: ThreadId,
        request: Request,
    ) -> Result<Outcome, String> {
        if request.cancelled.load(Ordering::Acquire) {
            return Ok(Outcome::early(Status::Cancelled));
        }
        if self.scheduler.busy() {
            return Ok(Outcome::early(Status::Deferred(
                "interactive work is active or queued".into(),
            )));
        }
        let binding = self.binding(thread)?;
        let generation = match self.memory_admission_generation(thread) {
            Ok(value) => value,
            Err(_) => {
                return Ok(Outcome::early(Status::Deferred(
                    "local vector owner is paused or closed".into(),
                )))
            }
        };
        let checked = binding.clone();
        let sources = request.sources;
        let chunker = request.chunker;
        let inventory = match self.worker.run(move |context| {
            context.can_start_memory(&checked)?;
            if checked.scope.task != context.config.root_task {
                return Err("workspace vector maintenance requires the root owner".into());
            }
            if !context.engine.store().spool().unfinished()?.is_empty() {
                return Err("capture recovery incomplete".into());
            }
            Ok(search_record::inventory(
                context.engine.store(),
                &context.memory_access(),
                &sources,
                &chunker,
                search_record::Limits::default(),
            )?)
        }) {
            Ok(inventory) => inventory,
            Err(error) => return Ok(Outcome::early(Status::Deferred(error))),
        };
        let chunks = embedding::inventory_chunks(&inventory).map_err(|error| error.to_string())?;
        if chunks.is_empty() {
            return Ok(Outcome::early(Status::Deferred(
                "no currently eligible vector sources".into(),
            )));
        }
        let source_bytes = chunks.iter().map(|row| row.text.len()).sum();
        let workload = Workload {
            rows: chunks.len(),
            source_bytes,
            batch: 16,
            load_model: true,
        };
        let permit = match admission()?.acquire(workload) {
            Ok(permit) => permit,
            Err(reason) => return Ok(Outcome::early(Status::Deferred(reason))),
        };
        let aborted = Arc::new(AtomicBool::new(false));
        let _guard = AbortOnDrop(aborted.clone());
        let host = self.clone();
        tokio::task::spawn_blocking(move || {
            let stopped = || {
                request.cancelled.load(Ordering::Acquire)
                    || aborted.load(Ordering::Acquire)
                    || host.worker.fenced()
                    || host.memory_admission_generation(thread).ok() != Some(generation)
            };
            if stopped() {
                return Ok(Outcome::early(Status::Cancelled));
            }
            if host.scheduler.busy() {
                return Ok(Outcome::early(Status::Deferred(
                    "interactive work is active or queued".into(),
                )));
            }
            let started = Instant::now();
            let cpu_start = process_cpu_millis()?;
            let directory = private_directory(&request.private_root)?;
            let observations = std::cell::RefCell::new(Measurements::start());
            observations.borrow_mut().sample(&directory)?;
            let failure = std::cell::RefCell::new(None::<Status>);
            let admitted_authority = inventory.authority;
            let admitted_deletion = inventory.deletion;
            let checkpoint = || {
                let state = if stopped() {
                    Some(Status::Cancelled)
                } else if host.scheduler.busy() {
                    Some(Status::Deferred("interactive work arrived".into()))
                } else if started.elapsed() > Duration::from_secs(60) {
                    Some(Status::Deferred("local vector work deadline".into()))
                } else {
                    None
                };
                if let Some(state) = state {
                    *failure.borrow_mut() = Some(state);
                    return true;
                }
                let checked = binding.clone();
                if host
                    .worker
                    .run(move |context| {
                        context.can_start_memory(&checked)?;
                        let workspace: Workspace = context
                            .engine
                            .store()
                            .state()
                            .record(
                                Collection::Workspace,
                                checked.scope.workspace.as_str(),
                                &checked.scope.workspace,
                            )?
                            .decode()?;
                        if workspace.authority != admitted_authority
                            || workspace.deletion != admitted_deletion
                        {
                            return Err("local vector source authority changed".into());
                        }
                        Ok(())
                    })
                    .is_err()
                {
                    *failure.borrow_mut() = Some(Status::Cancelled);
                    return true;
                }
                if let Err(reason) = observations
                    .borrow_mut()
                    .sample(&directory)
                    .and_then(|()| local_resources::temporary_disk_bytes(&directory))
                    .and_then(|bytes| permit.check_temporary_disk(bytes))
                {
                    *failure.borrow_mut() = Some(Status::Failed(reason));
                    return true;
                }
                false
            };
            let result = (|| -> Result<Built, String> {
                if checkpoint() {
                    return Err("local vector admission changed".into());
                }
                let mut model = LocalEmbedding::load(&request.assets, &checkpoint)
                    .map_err(|error| error.to_string())?;
                let rows = model
                    .embed(&chunks, &checkpoint)
                    .map_err(|error| error.to_string())?;
                let count = rows.len();
                let component = Component::build(
                    inventory.workspace.clone(),
                    Specification::qualified(),
                    rows,
                    &checkpoint,
                )
                .map_err(|error| error.to_string())?;
                let existing = local_resources::temporary_disk_bytes(&directory)?;
                permit.check_temporary_disk(existing)?;
                let available = permit
                    .estimate()
                    .temporary_disk_bytes
                    .checked_sub(existing)
                    .ok_or("local vector disk reservation exhausted")?;
                let checksum = component
                    .save_private_bounded(&directory.join("vectors.json"), available, &checkpoint)
                    .map_err(|error| error.to_string())?;
                // Recheck canonical authority, deletion and the captured source
                // watermark after CPU work; any intervening mutation requires replan.
                if checkpoint() {
                    return Err("local vector admission changed".into());
                }
                let captured = inventory.clone();
                let checked = binding.clone();
                host.worker.run(move |context| {
                    context.can_start_memory(&checked)?;
                    let workspace: Workspace = context
                        .engine
                        .store()
                        .state()
                        .record(
                            Collection::Workspace,
                            captured.workspace.as_str(),
                            &captured.workspace,
                        )?
                        .decode()?;
                    if workspace.authority != captured.authority
                        || workspace.deletion != captured.deletion
                    {
                        return Err("local vector authority or deletion changed".into());
                    }
                    // Any canonical source change during the bounded build requires
                    // replan; this conservative fence cannot publish stale coverage.
                    if context.engine.store().state().watermark != captured.watermark {
                        return Err("canonical source snapshot changed during vector build".into());
                    }
                    Ok(())
                })?;
                // Model and component are dropped before the enclosing permit.
                drop(component);
                drop(model);
                Ok(Built {
                    directory: directory.clone(),
                    checksum,
                    inventory,
                    specification: Specification::qualified()
                        .digest()
                        .map_err(|error| error.to_string())?,
                    rows: count,
                })
            })();
            let sample_result = observations.borrow_mut().sample(&directory);
            let measured = observations.into_inner();
            let cpu = process_cpu_millis()?.saturating_sub(cpu_start);
            let estimate = permit.estimate();
            let status = failure.into_inner().unwrap_or_else(|| match &result {
                Ok(_) => Status::Ready,
                Err(reason) => Status::Failed(reason.clone()),
            });
            let status = match sample_result {
                Ok(()) => status,
                Err(reason) => Status::Failed(reason),
            };
            let id = ObservationId::new();
            let mut report = ResourceReport {
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
                scope: binding.scope.clone(),
                agent: binding.agent.clone(),
                cpu_millis: Units::new(cpu),
                peak_ram: ByteCount::new(report.sampled_resident_peak_bytes),
                disk: ByteCount::new(report.sampled_temporary_disk_peak_bytes),
                source: format!(
                    "{};process-cpu-delta;sampled-process-resident",
                    local_resources::ESTIMATE_VERSION
                ),
            };
            let outcome = host.worker.run_cleanup(move |context| {
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
                        observation,
                        &actor,
                    ))?;
                Ok(())
            });
            match outcome {
                Ok(()) => report.observation_retained = true,
                Err(reason) => report.observation_error = Some(reason),
            }
            drop(permit);
            let status = if stopped() { Status::Cancelled } else { status };
            let built = if status == Status::Ready {
                result.ok()
            } else {
                None
            };
            Ok(Outcome {
                status,
                built,
                resources: Some(report),
            })
        })
        .await
        .map_err(|_| "local vector worker failed".to_string())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_owners_share_one_local_permit_and_release_it() {
        let workload = Workload {
            rows: 1,
            source_bytes: 100,
            batch: 1,
            load_model: true,
        };
        let one = admission().unwrap().acquire(workload).unwrap();
        let another_owner = admission().unwrap().clone();
        assert!(another_owner.acquire(workload).is_err());
        drop(one);
        assert!(another_owner.acquire(workload).is_ok());
    }
}
