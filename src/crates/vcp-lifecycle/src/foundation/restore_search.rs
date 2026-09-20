// SPDX-License-Identifier: Apache-2.0
//! Explicit model-free rebuild after restore/rebind. Does not trust the workspace,
//! refresh source fingerprints, resume tasks, or interpret archived instructions.
use super::{memory_vectors, Config};
use std::result::Result;
use std::{
    cell::RefCell,
    collections::BTreeSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use vcp_domain::{
    accounting::LocalResources,
    artifact::ArtifactDescriptor,
    task::Task,
    workspace::{Trust, Workspace},
    *,
};
use vcp_memory::{
    access::Access,
    local_resources::{self, Measurements, Workload},
    publication::{self, Publisher},
    retrieval,
    search_record::{self, ChunkerSpec, Inventory, SearchKind},
};
use vcp_store::{contract::Collection, Store};

#[derive(Clone, Debug, serde::Serialize)]
pub struct Readiness {
    pub lexical_ready: bool,
    pub generation: GenerationId,
    pub indexed_records: usize,
    pub excluded_records: usize,
    pub source_reauthorization_required: bool,
    pub semantic_pending: bool,
    pub degraded: Vec<String>,
    pub rebuilt: bool,
}
struct Cancel {
    flag: Arc<AtomicBool>,
    armed: bool,
}
impl Drop for Cancel {
    fn drop(&mut self) {
        if self.armed {
            self.flag.store(true, Ordering::Release);
        }
    }
}
fn same_inventory(a: &Inventory, b: &Inventory) -> bool {
    let mut records = b.records.clone();
    for (new, old) in records.iter_mut().zip(&a.records) {
        if new.kind == SearchKind::Source {
            new.watermark = old.watermark;
        }
    }
    a.workspace == b.workspace
        && a.authority == b.authority
        && a.deletion == b.deletion
        && a.chunker_digest == b.chunker_digest
        && a.records == records
        && a.exclusions == b.exclusions
}
fn now() -> Timestamp {
    Timestamp::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64,
    )
}
/// Caller holds the workspace selection lease. This opens an exclusive canonical
/// store, so a live owner prevents rebuilding. Repeated calls reopen a matching
/// current lexical generation without appending another generation or receipt.
pub async fn rebuild_after_restore(
    config: &Config,
    cancelled: Arc<AtomicBool>,
) -> Result<Readiness, String> {
    let mut cancel = Cancel {
        flag: cancelled.clone(),
        armed: true,
    };
    if cancelled.load(Ordering::Acquire) {
        return Err("restore lexical rebuild cancelled".into());
    }
    let mut store = Store::open(&config.canonical_root, config.backend, &[])
        .await
        .map_err(|e| e.to_string())?;
    let result = rebuild(&mut store, config, cancelled).await;
    let closed = store.close().await.map_err(|e| e.to_string());
    match result {
        Ok(value) => {
            closed?;
            cancel.armed = false;
            Ok(value)
        }
        Err(error) => Err(error),
    }
}
async fn rebuild(
    store: &mut Store,
    config: &Config,
    cancelled: Arc<AtomicBool>,
) -> Result<Readiness, String> {
    let workspace: Workspace = store
        .state()
        .record(
            Collection::Workspace,
            config.workspace.as_str(),
            &config.workspace,
        )
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;
    if workspace.binding != config.binding {
        return Err("restore lexical workspace binding changed".into());
    }
    let task: Task = store
        .state()
        .record(
            Collection::Task,
            config.root_task.as_str(),
            &config.workspace,
        )
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;
    let access = Access {
        workspace: config.workspace.clone(),
        actor: config.actor.clone(),
        authority: workspace.authority,
        read: true,
        write: true,
        tasks: None,
    };
    let started = Instant::now();
    let check = || {
        if cancelled.load(Ordering::Acquire) || started.elapsed() >= Duration::from_secs(60) {
            Err(vcp_memory::Error::Conflict(
                "restore lexical rebuild cancelled or deadline",
            ))
        } else {
            Ok(())
        }
    };
    let sources =
        retrieval::source_bindings_with_check(store, &access, &check).map_err(|e| e.to_string())?;
    let inventory = search_record::inventory_with_check(
        store,
        &access,
        &sources.bindings,
        &ChunkerSpec::default(),
        search_record::Limits::default(),
        &check,
    )
    .map_err(|e| e.to_string())?;
    let represented: BTreeSet<_> = sources
        .bindings
        .iter()
        .map(|s| s.manifest.as_str())
        .collect();
    let stale_sources = store
        .state()
        .records
        .values()
        .filter(|r| r.collection == Collection::Artifact && r.workspace == config.workspace)
        .try_fold(false, |found, row| -> Result<bool, String> {
            let artifact: ArtifactDescriptor = row.decode().map_err(|e| e.to_string())?;
            Ok(found
                || (artifact.state == vcp_domain::artifact::CaptureState::Complete
                    && matches!(
                        artifact.spec.schema.as_str(),
                        "verification-plan/1"
                            | "verification-baseline/1"
                            | "verification-result/1"
                            | "vcp-memory-change/1"
                            | "vcp-workspace-checkpoint/1"
                    )
                    && !represented.contains(artifact.spec.id.as_str())))
        })?;
    let mut degraded: Vec<String> = sources.degraded.iter().map(|s| (*s).into()).collect();
    if stale_sources {
        degraded.push("retained_source_manifests_require_current_trusted_recapture".into());
    }
    if workspace.trust == Trust::Untrusted {
        degraded.push("workspace_remains_untrusted".into());
    }
    let describe = |view: &publication::View, rebuilt| Readiness {
        lexical_ready: true,
        generation: view.manifest.id.clone(),
        indexed_records: view.inventory.records.len(),
        excluded_records: view.inventory.exclusions.len(),
        source_reauthorization_required: stale_sources || !sources.complete,
        semantic_pending: !view.inventory.records.is_empty() && view.vector.is_none(),
        degraded: degraded.clone(),
        rebuilt,
    };
    let directory = store.canonical_anchor().join("search-generations");
    if directory.try_exists().map_err(|e| e.to_string())? {
        let publisher = Publisher::open_existing(&directory).map_err(|e| e.to_string())?;
        let recovered = publisher
            .recover_with_check(store, &access, &check)
            .map_err(|e| e.to_string())?;
        if let Some(view) = recovered.view {
            if same_inventory(&view.inventory, &inventory) {
                return Ok(describe(&view, false));
            }
        }
    }
    check().map_err(|e| e.to_string())?;
    let permit = memory_vectors::admission()?.acquire(Workload {
        rows: inventory.records.len().max(1),
        source_bytes: inventory.records.iter().map(|r| r.text.len()).sum(),
        batch: 1,
        load_model: false,
    })?;
    let publisher = Arc::new(Publisher::new(&directory).map_err(|e| e.to_string())?);
    let snapshot = publication::capture(store, &access, &task.scope, inventory.clone())
        .map_err(|e| e.to_string())?;
    let build = publisher.clone();
    let flag = cancelled.clone();
    let (validated, measured, cpu, cpu_source) =
        tokio::task::spawn_blocking(move || -> Result<_, String> {
            let before = memory_vectors::process_cpu_millis()?;
            let baseline: BTreeSet<_> = std::fs::read_dir(build.storage_root())
                .map_err(|e| e.to_string())?
                .map(|e| e.map(|e| e.path()))
                .collect::<std::io::Result<_>>()
                .map_err(|e| e.to_string())?;
            let measured = RefCell::new(Measurements::start());
            let failure = RefCell::new(None);
            let sample = || -> Result<(), String> {
                let mut bytes = 0u64;
                for entry in std::fs::read_dir(build.storage_root()).map_err(|e| e.to_string())? {
                    let path = entry.map_err(|e| e.to_string())?.path();
                    if !baseline.contains(&path) {
                        bytes = bytes
                            .checked_add(local_resources::temporary_disk_bytes(&path)?)
                            .ok_or("restore lexical disk count overflow")?;
                    }
                }
                measured
                    .borrow_mut()
                    .observe(local_resources::snapshot()?, bytes);
                permit.check_temporary_disk(bytes)
            };
            let barrier = |_: publication::Barrier| {
                if let Err(error) = sample() {
                    *failure.borrow_mut() = Some(error);
                    flag.store(true, Ordering::Release);
                }
                if started.elapsed() >= Duration::from_secs(60) {
                    flag.store(true, Ordering::Release);
                }
            };
            let mut result = (|| {
                sample()?;
                let prepared = build
                    .prepare(snapshot, None, &flag, &barrier)
                    .map_err(|e| e.to_string())?;
                build
                    .validate_prepared(prepared, &flag)
                    .map_err(|e| e.to_string())
            })();
            if let Err(error) = sample() {
                *failure.borrow_mut() = Some(error);
            }
            if let Some(error) = failure.into_inner() {
                result = Err(error);
            }
            // Preserve measured resource evidence even if the final CPU sampler fails.
            let (cpu, cpu_source) = match memory_vectors::process_cpu_millis() {
                Ok(after) => (after.saturating_sub(before), "process-cpu-delta"),
                Err(error) => {
                    result = Err(error);
                    (0, "cpu-delta-unavailable;zero-is-unmeasured")
                }
            };
            Ok((result, measured.into_inner(), cpu, cpu_source))
        })
        .await
        .map_err(|_| "restore lexical worker stopped")??;
    let observation = LocalResources {
        schema_version: 1,
        id: ObservationId::new(),
        scope: task.scope,
        agent: AgentId::parse("restore-lexical").map_err(|e| e.to_string())?,
        cpu_millis: Units::new(cpu),
        peak_ram: ByteCount::new(measured.sampled_build_peak_resident_bytes),
        disk: ByteCount::new(measured.sampled_build_peak_temporary_disk_bytes),
        source: format!(
            "{};restore-lexical;{cpu_source};sampled-process-resident",
            local_resources::ESTIMATE_VERSION
        ),
    };
    vcp_budget::record_local_resources(
        store,
        observation,
        &vcp_budget::Actor {
            id: config.actor.clone(),
            now: now(),
        },
    )
    .await
    .map_err(|e| e.to_string())?;
    let validated = validated?;
    check().map_err(|e| e.to_string())?;
    publisher
        .activate(store, &access, &validated, now(), &|_| {})
        .await
        .map_err(|e| e.to_string())?;
    let recovered = publisher
        .recover_with_check(store, &access, &check)
        .map_err(|e| e.to_string())?;
    let view = recovered
        .view
        .ok_or("published lexical generation failed reopening")?;
    if view.manifest.id != validated.manifest().id || !same_inventory(&view.inventory, &inventory) {
        return Err("published lexical inventory differs".into());
    }
    Ok(describe(&view, true))
}
