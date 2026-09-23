// SPDX-License-Identifier: Apache-2.0
//! Explicit, exclusive, credential-free local memory work. This capability does
//! not expose a host, attach an executable thread, or resume any task.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use vcp_memory::{publication::Publisher, retrieval, search_record::ChunkerSpec};
use vcp_store::contract::Collection;

pub struct LocalMemory {
    host: CanonicalHost,
    thread: ThreadId,
    root: PathBuf,
}

struct ScanControl {
    external: Arc<AtomicBool>,
    owner_alive: Arc<AtomicBool>,
    worker: worker::Worker,
    started: std::time::Instant,
    timeout: Duration,
}
impl ScanControl {
    fn check(&self) -> vcp_memory::Result<()> {
        if self.external.load(Ordering::Acquire)
            || !self.owner_alive.load(Ordering::Acquire)
            || self.worker.fenced()
            || self.started.elapsed() >= self.timeout
        {
            return Err(vcp_memory::Error::Conflict(
                "local memory cancelled, closed or deadline",
            ));
        }
        Ok(())
    }
}

/// Reject remote/device paths before any filesystem access and reparse points
/// in every existing ancestor. Missing local assets remain a visible loader
/// failure (or an explicitly requested lexical-only build).
fn local_path(path: &Path) -> Result<PathBuf, String> {
    use std::{
        os::windows::fs::MetadataExt,
        path::{Component, Prefix},
    };
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err("parent traversal is not a local memory path".into());
    }
    let path = std::path::absolute(path).map_err(|e| e.to_string())?;
    let drive = match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive,
            _ => return Err("local memory requires a local drive path".into()),
        },
        _ => return Err("local memory requires a local drive path".into()),
    };
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDriveTypeW(root: *const u16) -> u32;
    }
    let drive_root = [u16::from(drive), u16::from(b':'), u16::from(b'\\'), 0];
    // SAFETY: the terminated local drive root remains alive for this call.
    if unsafe { GetDriveTypeW(drive_root.as_ptr()) } == 4 {
        return Err("mapped network memory paths are rejected".into());
    }
    for ancestor in path.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(metadata) if metadata.file_attributes() & 0x400 != 0 => {
                return Err("local memory path crosses a reparse point".into())
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::local_path;
    use std::path::Path;

    #[test]
    fn local_memory_paths_reject_traversal_remote_and_device_names_before_io() {
        for path in [
            r"..\assets",
            r"C:\safe\..\assets",
            r"\\server\share\assets",
            r"\\?\UNC\server\share\assets",
            r"\\.\pipe\memory",
        ] {
            assert!(local_path(Path::new(path)).is_err(), "{path}");
        }
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing-assets");
        assert_eq!(
            local_path(&missing).unwrap(),
            std::path::absolute(missing).unwrap()
        );
    }
}

fn local_assets(path: &Path) -> Result<PathBuf, String> {
    let path = local_path(path)?;
    // The pinned asset layout is bounded and shallow; inspect directories too,
    // so a nested model asset cannot redirect a nominally local load.
    let mut pending = vec![path.clone()];
    let mut entries = 0;
    while let Some(directory) = pending.pop() {
        let children = match std::fs::read_dir(&directory) {
            Ok(children) => children,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        };
        for child in children {
            entries += 1;
            if entries > 128 {
                return Err("local embedding asset directory exceeds inspection bound".into());
            }
            let child = child.map_err(|e| e.to_string())?;
            let checked = local_path(&child.path())?;
            if child.file_type().map_err(|e| e.to_string())?.is_dir() {
                pending.push(checked);
            }
        }
    }
    Ok(path)
}

pub fn open_local_memory(config: Config) -> Result<(LocalMemory, CanonicalOwner), String> {
    let root = local_path(&config.canonical_root)?;
    let (mut host, owner) = CanonicalHost::open(config)?;
    let binding = host.worker.run(|context| {
        let task: vcp_domain::task::Task = context
            .engine
            .store()
            .state()
            .record(
                Collection::Task,
                context.config.root_task.as_str(),
                &context.config.workspace,
            )?
            .decode()?;
        if task.parent.is_some() || task.scope.session != context.config.session {
            return Err("local memory requires the configured root task".into());
        }
        let binding = ThreadBinding {
            scope: task.scope,
            agent: AgentId::new(),
            role: RequestRole::Memory,
        };
        context.local_memory_only = true;
        context.can_start_memory(&binding)?;
        Ok(binding)
    })?;
    host.local_memory_only = true;
    let thread = ThreadId::new();
    host.register(thread, binding)?;
    Ok((LocalMemory { host, thread, root }, owner))
}

impl CanonicalHost {
    pub(super) fn memory_admission_generation(&self, thread: ThreadId) -> Result<u64, String> {
        if !self.memory_owner_alive.load(Ordering::Acquire) || self.worker.fenced() {
            return Err("local memory owner is closed".into());
        }
        if self.local_memory_only {
            self.binding(thread)?;
            Ok(0)
        } else {
            self.runtime
                .admission_generation(thread)
                .map_err(|e| format!("{e:?}"))
        }
    }
}

impl LocalMemory {
    pub async fn build(
        &self,
        assets: PathBuf,
        allow_lexical_only: bool,
        cancelled: Arc<AtomicBool>,
    ) -> Result<serde_json::Value, String> {
        self.check(&cancelled)?;
        let assets = local_assets(&assets)?;
        let binding = self.host.binding(self.thread)?;
        let control = self.scan_control(cancelled.clone(), Duration::from_secs(60));
        let sources = self.host.worker.run(move |context| {
            context.can_start_memory(&binding)?;
            Ok(retrieval::source_bindings_with_check(
                context.engine.store(),
                &context.memory_access(),
                &|| control.check(),
            )?)
        })?;
        if !sources.complete {
            return Err(format!(
                "retained source bindings are incomplete: {:?}",
                sources.degraded
            ));
        }
        let publisher = Arc::new(
            Publisher::new(&self.root.join("search-generations")).map_err(|e| e.to_string())?,
        );
        let manager = self.host.publication_manager(self.thread, publisher)?;
        let private_root = local_path(&self.root.join("local-memory-work"))?;
        std::fs::create_dir_all(&private_root).map_err(|e| e.to_string())?;
        let outcome = self
            .host
            .publish_memory(
                self.thread,
                manager,
                memory_publication::Request {
                    vectors: memory_vectors::Request {
                        assets,
                        private_root,
                        sources: sources.bindings,
                        chunker: ChunkerSpec::default(),
                        cancelled,
                    },
                    allow_lexical_only,
                },
            )
            .await?;
        let (status, reason) = match outcome.status {
            memory_publication::Status::Published => ("published", None),
            memory_publication::Status::LexicalOnly(reason) => ("lexical_only", Some(reason)),
            memory_publication::Status::Deferred(reason) => ("deferred", Some(reason)),
            memory_publication::Status::Cancelled => ("cancelled", None),
        };
        Ok(
            serde_json::json!({"status":status,"reason":reason,"generation":outcome.manifest,"resources":outcome.resources,"publication_resources":outcome.publication_resources}),
        )
    }

    fn check(&self, cancelled: &AtomicBool) -> Result<(), String> {
        if cancelled.load(Ordering::Acquire) {
            return Err("local memory cancelled".into());
        }
        self.host.memory_admission_generation(self.thread)?;
        let binding = self.host.binding(self.thread)?;
        self.host
            .worker
            .run(move |context| context.can_start_memory(&binding))
    }

    fn scan_control(&self, external: Arc<AtomicBool>, timeout: Duration) -> Arc<ScanControl> {
        Arc::new(ScanControl {
            external,
            owner_alive: self.host.memory_owner_alive.clone(),
            worker: self.host.worker.clone(),
            started: std::time::Instant::now(),
            timeout,
        })
    }

    pub async fn query(
        &self,
        assets: PathBuf,
        request: retrieval::Request,
        cancelled: Arc<AtomicBool>,
    ) -> Result<serde_json::Value, String> {
        request.validate().map_err(|e| e.to_string())?;
        self.check(&cancelled)?;
        let assets = local_assets(&assets)?;
        let workspace = self.host.binding(self.thread)?.scope.workspace;
        if request.workspace != workspace {
            return Err("local memory query workspace denied".into());
        }
        let embedded = self
            .host
            .embed_memory_query(self.thread, assets, request.text.clone(), cancelled.clone())
            .await?;
        let Some(embedding) = embedded.embedding else {
            return Ok(
                serde_json::json!({"status":"failed","reason":embedded.failure,"resources":embedded.resources}),
            );
        };
        self.check(&cancelled)?;
        let publisher = Arc::new(
            Publisher::open_existing(&self.root.join("search-generations"))
                .map_err(|e| e.to_string())?,
        );
        let (recovery, reopen_resources) = self
            .host
            .open_memory_view(self.thread, publisher, cancelled.clone())
            .await?;
        let binding = self.host.binding(self.thread)?;
        let checked = binding.clone();
        // Include retained binding discovery in the caller's retrieval deadline;
        // carry this same clock through search and canonical materialization.
        let control =
            self.scan_control(cancelled.clone(), Duration::from_millis(request.timeout_ms));
        let capture_control = control.clone();
        let (captured, sources) = self.host.worker.run(move |context| {
            context.can_start_memory(&checked)?;
            let access = context.memory_access();
            let sources =
                retrieval::source_bindings_with_check(context.engine.store(), &access, &|| {
                    capture_control.check()
                })?;
            let captured = retrieval::capture(
                context.engine.store(),
                &access,
                &request,
                &sources.bindings,
                &ChunkerSpec::default(),
                &|| capture_control.check().is_err(),
            )?;
            Ok((captured, sources))
        })?;
        let host = self.host.clone();
        let thread = self.thread;
        let external = cancelled.clone();
        let stage = Arc::new(AtomicBool::new(false));
        struct Cancel(Arc<AtomicBool>);
        impl Drop for Cancel {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let _guard = Cancel(stage.clone());
        let search_control = control.clone();
        let (selected, resources) = tokio::task::spawn_blocking(move || {
            let stopped = || {
                stage.load(Ordering::Acquire)
                    || external.load(Ordering::Acquire)
                    || host.memory_admission_generation(thread).is_err()
                    || search_control.check().is_err()
            };
            memory_query_resources::run(
                &host,
                binding,
                vcp_memory::local_resources::Workload {
                    rows: recovery
                        .view
                        .as_ref()
                        .and_then(|v| v.vector.as_ref())
                        .map_or(1, |v| v.rows().len().max(1)),
                    source_bytes: 0,
                    batch: 1,
                    load_model: false,
                },
                |_sample| {
                    retrieval::search(
                        captured,
                        recovery.view.as_ref(),
                        Some(retrieval::QueryVector {
                            specification: &embedding.specification,
                            values: &embedding.values,
                        }),
                        &stopped,
                    )
                    .map_err(|e| e.to_string())
                },
            )
        })
        .await
        .map_err(|_| "local memory query worker failed")??;
        self.check(&cancelled)?;
        let selected = selected?;
        let binding = self.host.binding(self.thread)?;
        let response = self.host.worker.run(move |context| {
            context.can_start_memory(&binding)?;
            let mut response = retrieval::finish(
                context.engine.store(),
                &context.memory_access(),
                selected,
                &|| control.check().is_err(),
            )?;
            response.rebuild_required |= !sources.complete;
            response.degraded.extend(sources.degraded);
            Ok(response)
        })?;
        Ok(
            serde_json::json!({"status":"queried","response":response,"embedding_resources":embedded.resources,"reopen_resources":reopen_resources,"resources":resources}),
        )
    }
}
