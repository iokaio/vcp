// SPDX-License-Identifier: Apache-2.0
//! Cold-open factory using real native ownership before any history is trusted.
//! Public Store consumes this admitted, payload-free owner.
use super::*;
use crate::{canonical_lock::CanonicalLock, history_index::io, private_paths::Directory};
#[path = "store_current_open_legacy.rs"]
mod legacy;
#[path = "store_current_owner.rs"]
mod owner;
#[path = "store_current_prefix.rs"]
pub(crate) mod prefix;

pub(crate) struct Opened {
    pub(crate) backend: Backend,
    pub(crate) owner: std::sync::Arc<DurableOwner>,
    pub(crate) pages: Option<Directory>,
    pub(crate) spool: crate::artifact::Spool,
    pub(crate) artifact_limit: u64,
    pub(crate) poisoned: bool,
    pub(crate) diagnostics: crate::StoreDiagnostics,
    pub(crate) current: std::sync::OnceLock<std::sync::Arc<crate::CurrentState>>,
    pub(crate) origin_digest: String,
    pub(crate) prefixes: Vec<crate::replay_base::PrefixCommitment>,
    _data: File,
    _root: Directory,
    _lock: CanonicalLock,
}
impl Opened {
    pub(crate) async fn history(&self) -> Result<crate::backend::history::CurrentCommitReader<'_>> {
        self.ensure_healthy()?;
        self.backend.history_current(&self._lock, &self.owner).await
    }
    /// Transfers the actual exclusive owner lock for the backend lifetime.
    /// Opens only existing data and performs full replay; a supplied descriptor
    /// digest is not an admission certificate or permission to skip its prefix.
    pub(crate) async fn open(
        lock: CanonicalLock,
        kind: BackendKind,
        origin_digest: &str,
        diagnostics: &mut crate::StoreDiagnostics,
    ) -> Result<Self> {
        Self::open_with_artifact_limit(
            lock,
            kind,
            origin_digest,
            crate::artifact::DEFAULT_ARTIFACT_LIMIT,
            diagnostics,
        )
        .await
    }
    pub(crate) async fn open_with_artifact_limit(
        lock: CanonicalLock,
        kind: BackendKind,
        origin_digest: &str,
        artifact_limit: u64,
        diagnostics: &mut crate::StoreDiagnostics,
    ) -> Result<Self> {
        if artifact_limit == 0 || artifact_limit > crate::artifact::DEFAULT_ARTIFACT_LIMIT {
            return Err(Error::Limit("artifact capacity"));
        }
        let root = Directory::locked_root(&lock)?;
        for name in [
            "canonical.sqlite",
            "canonical.sqlite-wal",
            "canonical.sqlite-shm",
            "canonical.frames",
        ] {
            match crate::artifact::reject_link(&root.path.join(name)) {
                Ok(()) => {}
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        let data_path = root.path.join(match kind {
            BackendKind::Files => "canonical.frames",
            BackendKind::Sqlite => "canonical.sqlite",
        });
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x0020_0000).share_mode(1 | 2);
        }
        let data = options.open(&data_path)?;
        if !crate::private_paths::allowed_handle(&data, false)? {
            return Err(Error::Access);
        }
        let started = Instant::now();
        let base = crate::replay_base::ReplayBase::load(&root.path);
        diagnostics.replay_base.record(started, base.is_ok());
        let base = base?;
        let replay_started = Instant::now();
        let result: Result<(Backend, DurableOwner, Option<Directory>)> = async {
            match kind {
                BackendKind::Files => {
                    // Missing persisted page storage is corruption, never an
                    // invitation to manufacture a fresh empty generation.
                    crate::artifact::reject_link(&root.path.join("history-pages"))?;
                    let directory = Directory::locked_child(&lock, "history-pages")?;
                    let chain = base
                        .as_ref()
                        .map(|base| base.chain())
                        .transpose()?
                        .unwrap_or_else(|| "0".repeat(64));
                    let mut journal = Journal {
                        file: OpenOptions::new().read(true).write(true).open(&data_path)?,
                        root: root.path.clone(),
                        chain: chain.clone(),
                        initial_chain: chain,
                        base_watermark: base
                            .as_ref()
                            .map(|base| base.state.watermark)
                            .unwrap_or_default(),
                        #[cfg(test)]
                        write_budget: None,
                    };
                    if crate::vault_publish::native_identity(&data)?
                        != crate::vault_publish::native_identity(&journal.file)?
                    {
                        return Err(Error::Corruption("canonical data identity changed"));
                    }
                    let mut pages = io::Files::new(&directory);
                    let origin = Origin::load(&mut pages, origin_digest).await?;
                    let owner = journal
                        .replay_current_all(&mut pages, &origin, base.as_ref(), diagnostics)
                        .await?;
                    Ok((Backend::Files(journal), owner, Some(directory)))
                }
                BackendKind::Sqlite => {
                    let options = SqliteConnectOptions::new()
                        .filename(&data_path)
                        .create_if_missing(false)
                        .journal_mode(SqliteJournalMode::Wal)
                        .synchronous(SqliteSynchronous::Full)
                        .foreign_keys(true)
                        .busy_timeout(Duration::from_millis(100));
                    let mut db = SqliteConnection::connect_with(&options)
                        .await
                        .map_err(sql_error)?;
                    let result = async {
                        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
                            .fetch_one(&mut db)
                            .await?;
                        if version != if base.is_some() { 2 } else { 1 } {
                            return Err(Error::Incompatible);
                        }
                        let origin =
                            Origin::load(&mut io::Sqlite::new(&mut db), origin_digest).await?;
                        replay_sqlite_all(&mut db, &origin, base.as_ref(), diagnostics).await
                    }
                    .await;
                    let owner = match result {
                        Ok(owner) => owner,
                        Err(error) => {
                            let _ = db.close().await;
                            return Err(error);
                        }
                    };
                    let mut backend = Backend::Sqlite(db);
                    if let Err(error) = backend.configuration().await {
                        let _ = backend.close().await;
                        return Err(error);
                    }
                    Ok((backend, owner, None))
                }
            }
        }
        .await;
        diagnostics.replay.record(replay_started, result.is_ok());
        let (backend, owner, pages) = result?;
        if let Err(error) = lock.verify() {
            let _ = backend.close().await;
            return Err(error);
        }
        let artifact_started = Instant::now();
        let spool = (|| {
            let spool = crate::artifact::Spool::open(
                &root.path.join("spool"),
                lock.forbidden(),
                artifact_limit,
            )?;
            for record in owner
                .semantic()
                .current()
                .records
                .values()
                .filter(|row| row.collection == Collection::Artifact)
            {
                let descriptor: vcp_domain::artifact::ArtifactDescriptor = record.decode()?;
                if descriptor.state != vcp_domain::artifact::CaptureState::Purged {
                    spool.verify(&descriptor)?;
                    diagnostics.verified_artifacts =
                        diagnostics.verified_artifacts.saturating_add(1);
                }
            }
            Ok::<_, Error>(spool)
        })();
        diagnostics
            .artifact_verification
            .record(artifact_started, spool.is_ok());
        let spool = match spool {
            Ok(spool) => spool,
            Err(error) => {
                let _ = backend.close().await;
                return Err(error);
            }
        };
        // `base` and its complete archival State are released here. The returned
        // owner contains only current records and authenticated durable roots.
        diagnostics.current_watermark = owner.semantic().current().watermark.get();
        Ok(Self {
            backend,
            prefixes: base
                .as_ref()
                .map(|value| value.prefixes.clone())
                .unwrap_or_default(),
            owner: std::sync::Arc::new(owner),
            pages,
            spool,
            artifact_limit,
            poisoned: false,
            diagnostics: diagnostics.clone(),
            current: std::sync::OnceLock::new(),
            origin_digest: origin_digest.to_owned(),
            _data: data,
            _root: root,
            _lock: lock,
        })
    }
    pub(crate) async fn close_retaining_lock(self) -> (CanonicalLock, Result<()>) {
        let Self {
            backend,
            owner,
            pages,
            _data,
            _root,
            _lock,
            ..
        } = self;
        let result = backend.close().await;
        drop((owner, pages, _data, _root));
        (_lock, result)
    }
    pub(crate) async fn close(self) -> Result<()> {
        let Self {
            backend,
            owner,
            pages,
            _data,
            _root,
            _lock,
            ..
        } = self;
        let result = backend.close().await;
        drop((owner, pages, _data, _root, _lock));
        result
    }
}
