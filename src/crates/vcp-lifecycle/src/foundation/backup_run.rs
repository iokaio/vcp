// SPDX-License-Identifier: Apache-2.0
//! Canonical transitions are serialized with dispatch; archive, encryption and
//! vault I/O run away from the canonical worker. No model is involved.
use super::{backup::Setup, CanonicalHost};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use vcp_domain::CommandId;
use vcp_store::{
    keys::VerifiedKeys,
    snapshot_inputs::Inputs,
    snapshot_jobs::{Job, Jobs, Stage},
    trust_store::TrustStore,
    vault_crypto::PrivateStaging,
    vault_publish::Vault,
};

type Result<T> = std::result::Result<T, String>;
pub struct Capabilities {
    trust: Mutex<TrustStore>,
    keys: VerifiedKeys,
    jobs: Jobs,
    staging: PrivateStaging,
    vault: Vault,
}
impl Capabilities {
    /// Trusted local setup and independently verified file capability only.
    pub fn open(
        trust: TrustStore,
        keys: VerifiedKeys,
        setup: &Setup,
        workspace: &Path,
        canonical: &Path,
    ) -> Result<Self> {
        if trust.trust().configuration().workspace != setup.workspace
            || trust.trust().configuration().selected != *keys.public()
        {
            return Err("backup keys do not match independent enrollment".into());
        }
        let mut private = vec![
            workspace.to_owned(),
            canonical.to_owned(),
            trust.directory().to_owned(),
        ];
        let mut forbidden = private.clone();
        forbidden.push(setup.vault.clone());
        forbidden.extend(setup.sync_roots.clone());
        let jobs = Jobs::open(&setup.staging, &forbidden).map_err(|e| e.to_string())?;
        let staging =
            PrivateStaging::open(&setup.staging, &forbidden).map_err(|e| e.to_string())?;
        private.push(setup.staging.clone());
        let vault = Vault::open(&setup.vault, &private).map_err(|e| e.to_string())?;
        Ok(Self {
            trust: Mutex::new(trust),
            keys,
            jobs,
            staging,
            vault,
        })
    }
}
struct CancelOnDrop(Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
fn check(cancelled: &AtomicBool) -> Result<()> {
    if cancelled.load(Ordering::Acquire) {
        Err("backup cancelled; inspect retained job before cleanup or retry".into())
    } else {
        Ok(())
    }
}

impl CanonicalHost {
    pub(super) fn release_cancelled_backup(
        &self,
        capabilities: Arc<Capabilities>,
        operation: CommandId,
    ) -> Result<()> {
        self.worker.run_cleanup(move |context| {
            let store = context.engine.store_mut();
            if !store.state().records.values().any(|row| {
                row.collection == vcp_store::contract::Collection::SnapshotPin
                    && row.id == operation.as_str()
                    && row.workspace == context.config.workspace
            }) {
                return Ok(());
            }
            let job = Jobs::inspect(store, &operation, &context.config.workspace)?;
            if job.stage == Stage::Published {
                let trust = capabilities
                    .trust
                    .lock()
                    .map_err(|_| "backup trust unavailable")?;
                if !Jobs::checkpoint_matches(
                    store,
                    &operation,
                    &context.config.workspace,
                    trust.trust(),
                )? {
                    return Err(
                        "published backup requires independent checkpoint reconciliation".into(),
                    );
                }
            }
            context.runtime.block_on(capabilities.jobs.release(
                store,
                &operation,
                &context.config.workspace,
                true,
            ))?;
            Ok(())
        })
    }
    #[cfg(windows)]
    pub async fn capture_backup_inputs(
        &self,
        git: Arc<vcp_repository::git::Git>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Inputs> {
        check(&cancelled)?;
        let checkpoint = self
            .capture_backup_checkpoint(git, cancelled.clone())
            .await?;
        let (snapshot, access, path, active) = self.worker.run(|context| {
            let cut = context.backup_cut()?;
            let state = context.engine.store().state();
            let active = state
                .records
                .values()
                .find(|row| {
                    row.collection == vcp_store::contract::Collection::Generation
                        && row.workspace == cut.workspace.id
                        && row.value["document_type"] == "vcp_search_active_v1"
                })
                .map(|row| row.decode::<vcp_domain::search::Active>())
                .transpose()?
                .map(|value| value.generation);
            Ok((
                context.engine.store().snapshot()?,
                vcp_memory::access::Access {
                    workspace: cut.workspace.id,
                    actor: context.config.actor.clone(),
                    authority: cut.workspace.authority,
                    read: true,
                    write: false,
                    tasks: None,
                },
                context.config.canonical_root.join("search-generations"),
                active,
            ))
        })?;
        let stop = cancelled.clone();
        let recovered = tokio::task::spawn_blocking(move || -> Result<_> {
            if stop.load(Ordering::Acquire) {
                return Err("backup cancelled".into());
            }
            if active.is_none() || !path.exists() {
                return Ok(None);
            }
            let publisher = Arc::new(
                vcp_memory::publication::Publisher::open_existing(&path)
                    .map_err(|e| e.to_string())?,
            );
            let recovery = publisher
                .recover_snapshot_with_check(&snapshot, &access, &|| {
                    if stop.load(Ordering::Acquire) {
                        Err(vcp_memory::Error::Access)
                    } else {
                        Ok(())
                    }
                })
                .map_err(|e| e.to_string())?;
            // A prior-generation fallback is not the active snapshot component.
            Ok(recovery
                .view
                .filter(|view| Some(&view.manifest.id) == active.as_ref())
                .map(|view| (publisher, view)))
        })
        .await
        .map_err(|_| "backup generation reopen worker stopped")??;
        let mut generations = Vec::new();
        if let Some((publisher, view)) = recovered {
            generations.push(
                self.capture_backup_generation(publisher, view, cancelled.clone())
                    .await?,
            );
        }
        check(&cancelled)?;
        Ok(Inputs {
            checkpoint: Some(checkpoint),
            generations,
        })
    }
    /// A missing job requires newly captured native Inputs. A retry uses the
    /// durable original cut and never silently replaces it with today's state.
    pub async fn publish_backup(
        &self,
        capabilities: Arc<Capabilities>,
        operation: CommandId,
        inputs: Option<Inputs>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Job> {
        let _cancel = CancelOnDrop(cancelled.clone());
        check(&cancelled)?;
        let id = operation.clone();
        let (existing, capture) = self.worker.run(move |context| {
            #[cfg(windows)]
            context.backup_cut()?;
            let store = context.engine.store_mut();
            let workspace = &context.config.workspace;
            if store.state().records.values().any(|row| {
                row.collection == vcp_store::contract::Collection::SnapshotPin
                    && row.id == id.as_str()
                    && row.workspace == *workspace
            }) {
                let job = Jobs::inspect(store, &id, workspace)?;
                if inputs.as_ref().is_some_and(|value| value != &job.inputs) {
                    return Err("backup operation inputs changed".into());
                }
                return Ok((Some(job), None));
            }
            let inputs = inputs.ok_or("new backup requires native checkpoint inputs")?;
            Ok((None, Some(Jobs::capture_inputs(store, workspace, inputs)?)))
        })?;
        let mut job = if let Some(job) = existing {
            job
        } else {
            let capture = capture.ok_or("backup input capture missing")?;
            let stop = cancelled.clone();
            let prepared = tokio::task::spawn_blocking(move || {
                Jobs::prepare_inputs(capture, &|| stop.load(Ordering::Acquire))
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|_| "backup input validation worker stopped")??;
            check(&cancelled)?;
            let caps = capabilities.clone();
            let id = operation.clone();
            let stop=cancelled.clone();
            self.worker.run(move |context| {
                if stop.load(Ordering::Acquire){return Err("backup cancelled before durable input admission".into());}
                #[cfg(windows)]
                context.backup_cut()?;
                let trust = caps.trust.lock().map_err(|_| "backup trust unavailable")?;
                let captured = context.runtime.block_on(caps.jobs.begin_prepared(
                    context.engine.store_mut(),
                    id,
                    &context.config.workspace,
                    trust.trust(),
                    prepared,
                ))?;
                Ok(captured.job().clone())
            })?
        };
        if !job.active {
            return Ok(job);
        }
        if job.stage == Stage::Cancelled {
            return Err("backup operation was cancelled".into());
        }
        if job.stage == Stage::Captured {
            let caps = capabilities.clone();
            let cut = job.clone();
            let capture = self.worker.run(move |context| {
                Ok(caps.jobs.resume_capture(context.engine.store(), &cut)?)
            })?;
            let caps = capabilities.clone();
            let stop = cancelled.clone();
            let prepared = tokio::task::spawn_blocking(move || {
                caps.jobs
                    .prepare_detached(capture, &|| stop.load(Ordering::Acquire))
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|_| "backup archive worker stopped")??;
            check(&cancelled)?;
            let caps = capabilities.clone();
            job = self.worker.run(move |context| {
                Ok(context.runtime.block_on(caps.jobs.accept_prepared(
                    context.engine.store_mut(),
                    &context.config.workspace,
                    prepared,
                ))?)
            })?;
        }
        if job.stage == Stage::ArchiveReady {
            let caps = capabilities.clone();
            let cut = job.clone();
            let stop = cancelled.clone();
            let encrypted = tokio::task::spawn_blocking(move || {
                let trust = caps.trust.lock().map_err(|_| "backup trust unavailable")?;
                caps.jobs
                    .encrypt(&cut, trust.trust(), &caps.keys, &caps.staging, &|| {
                        stop.load(Ordering::Acquire)
                    })
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|_| "backup encryption worker stopped")??;
            check(&cancelled)?;
            let caps = capabilities.clone();
            job = self.worker.run(move |context| {
                Ok(context.runtime.block_on(caps.jobs.accept_encrypted(
                    context.engine.store_mut(),
                    &context.config.workspace,
                    encrypted,
                ))?)
            })?;
        }
        if matches!(job.stage, Stage::CiphertextReady | Stage::Admitted) {
            check(&cancelled)?;
            let caps = capabilities.clone();
            let cut = job.clone();
            let prepared = tokio::task::spawn_blocking(move || {
                let trust = caps.trust.lock().map_err(|_| "backup trust unavailable")?;
                caps.jobs
                    .prepare_admission(&cut, trust.trust())
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|_| "backup admission preparation worker stopped")??;
            let caps = capabilities.clone();
            let stop = cancelled.clone();
            let (admitted, mut encrypted, permit) = self.worker.run(move |context| {
                #[cfg(windows)]
                context.backup_cut()?;
                if stop.load(Ordering::Acquire) {
                    return Err("backup cancelled before admission".into());
                }
                let trust = caps.trust.lock().map_err(|_| "backup trust unavailable")?;
                Ok(context.runtime.block_on(caps.jobs.admit_prepared(
                    context.engine.store_mut(),
                    &context.config.workspace,
                    trust.trust(),
                    prepared,
                ))?)
            })?;
            let caps = capabilities.clone();
            let recorder = capabilities.clone();
            let worker = self.worker.clone();
            let id = operation.clone();
            let stop = cancelled.clone();
            let receipt = tokio::task::spawn_blocking(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| e.to_string())?;
                runtime
                    .block_on(caps.vault.publish_recorded(
                        &mut encrypted,
                        &permit,
                        admitted.copy_identity(),
                        move |identity| async move {
                            worker
                                .run(move |context| {
                                    context.runtime.block_on(recorder.jobs.record_copy(
                                        context.engine.store_mut(),
                                        &id,
                                        &context.config.workspace,
                                        identity,
                                    ))?;
                                    Ok(())
                                })
                                .map_err(|_| {
                                    vcp_store::Error::Unavailable(
                                        "backup copy admission could not be recorded",
                                    )
                                })
                        },
                        &|| stop.load(Ordering::Acquire),
                        &|_| {},
                    ))
                    .map_err(|failure| format!("backup publication incomplete: {failure:?}"))
            })
            .await
            .map_err(|_| "backup publication worker stopped")??;
            let caps = capabilities.clone();
            let retained = receipt.clone();
            job = self.worker.run(move |context| {
                Ok(context.runtime.block_on(caps.jobs.complete(
                    context.engine.store_mut(),
                    &context.config.workspace,
                    &retained,
                ))?)
            })?;
            let caps = capabilities.clone();
            tokio::task::spawn_blocking(move || {
                let mut trust = caps.trust.lock().map_err(|_| "backup trust unavailable")?;
                let revision = trust.trust().configuration().revision;
                trust
                    .update(revision, |local| {
                        local.advance_after_publication(&receipt, revision)
                    })
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|_| "backup checkpoint worker stopped")??;
        }
        if job.stage == Stage::Published {
            let caps = capabilities.clone();
            let id = operation.clone();
            let reconciled = self.worker.run(move |context| {
                let trust = caps.trust.lock().map_err(|_| "backup trust unavailable")?;
                if Jobs::checkpoint_matches(
                    context.engine.store(),
                    &id,
                    &context.config.workspace,
                    trust.trust(),
                )? {
                    return Ok(None);
                }
                Ok(Some(Jobs::publication_reconciliation(
                    context.engine.store(),
                    &id,
                    &context.config.workspace,
                )?))
            })?;
            if let Some(proof) = reconciled {
                let caps = capabilities.clone();
                tokio::task::spawn_blocking(move || {
                    let mut trust = caps.trust.lock().map_err(|_| "backup trust unavailable")?;
                    let receipt = caps
                        .jobs
                        .reconcile_publication(proof, trust.trust(), &caps.vault)
                        .map_err(|e| e.to_string())?;
                    let revision = trust.trust().configuration().revision;
                    trust
                        .update(revision, |local| {
                            local.advance_after_publication(&receipt, revision)
                        })
                        .map_err(|e| e.to_string())
                })
                .await
                .map_err(|_| "backup reconciliation worker stopped")??;
            }
            let caps = capabilities;
            let id = operation;
            job = self.worker.run_cleanup(move |context| {
                Ok(context.runtime.block_on(caps.jobs.release(
                    context.engine.store_mut(),
                    &id,
                    &context.config.workspace,
                    false,
                ))?)
            })?;
        }
        Ok(job)
    }
}
