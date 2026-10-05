// SPDX-License-Identifier: Apache-2.0
//! Test-only retained-layout oracle. Production Store never uses this owner;
//! migration fixtures need actual legacy frames after public Store moves to 3.
use crate::{
    artifact::{immutable_file, Spool, DEFAULT_ARTIFACT_LIMIT},
    backend::Backend,
    canonical_lock::CanonicalLock,
    contract::*,
    BackendKind, Error, Result, StoreDiagnostics,
};
use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
};
use vcp_domain::artifact::ArtifactDescriptor;
use vcp_protocol::canonical_bytes;

pub(crate) struct LegacyFixture {
    pub(crate) backend: Backend,
    pub(crate) state: State,
    pub(crate) base: State,
    pub(crate) root: PathBuf,
    pub(crate) kind: BackendKind,
    pub(crate) spool: Spool,
    pub(crate) forbidden_roots: Vec<PathBuf>,
    pub(crate) poisoned: bool,
    diagnostics: StoreDiagnostics,
    lock: CanonicalLock,
}
impl LegacyFixture {
    pub(crate) async fn open(
        root: &Path,
        kind: BackendKind,
        forbidden: &[PathBuf],
    ) -> Result<Self> {
        let lock = CanonicalLock::acquire(root, forbidden)?;
        let root = lock.root().to_owned();
        if !root.join("format.json").exists() {
            for name in ["canonical.frames", "canonical.sqlite", "spool"] {
                if root.join(name).exists() {
                    return Err(Error::Corruption("legacy fixture marker missing"));
                }
            }
            immutable_file(
                &root.join("format.json"),
                &canonical_bytes(&serde_json::json!({
                    "version": if root.join("replay-base.json").exists() { 2 } else { 1 }, "backend": kind
                }))?,
            )?;
        }
        if !matches!(
            crate::store_format::read(&lock, kind)?,
            crate::store_format::Selection::Legacy(_)
        ) {
            return Err(Error::Incompatible);
        }
        let base = crate::replay_base::ReplayBase::load(&root)?;
        let mut diagnostics = StoreDiagnostics::new(kind);
        let (backend, state, _) =
            Backend::open_observed(&root, kind, base.as_ref(), &mut diagnostics).await?;
        let spool = match Spool::open(&root.join("spool"), forbidden, DEFAULT_ARTIFACT_LIMIT) {
            Ok(spool) => spool,
            Err(error) => {
                let _ = backend.close().await;
                return Err(error);
            }
        };
        for row in state
            .records
            .values()
            .filter(|row| row.collection == Collection::Artifact)
        {
            let checked = (|| {
                let descriptor: ArtifactDescriptor = row.decode()?;
                if descriptor.state != vcp_domain::artifact::CaptureState::Purged {
                    spool.verify(&descriptor)?;
                }
                Ok::<_, Error>(())
            })();
            if let Err(error) = checked {
                let _ = backend.close().await;
                return Err(error);
            }
        }
        Ok(Self {
            backend,
            state,
            base: base.map(|base| base.state).unwrap_or_default(),
            root,
            kind,
            spool,
            forbidden_roots: lock.forbidden().to_vec(),
            poisoned: false,
            diagnostics,
            lock,
        })
    }
    pub(crate) fn state(&self) -> &State {
        &self.state
    }
    pub(crate) fn current_state(&self) -> Arc<crate::CurrentState> {
        Arc::new(crate::CurrentState::from_state(&self.state))
    }
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
    pub(crate) fn spool(&self) -> &Spool {
        &self.spool
    }
    pub(crate) fn diagnostics(&self) -> &StoreDiagnostics {
        &self.diagnostics
    }
    pub(crate) fn healthy(&self) -> bool {
        !self.poisoned
    }
    pub(crate) fn canonical_lock(&self) -> &CanonicalLock {
        &self.lock
    }
    pub(crate) fn try_snapshot_cleanup_guard(&self) -> Result<Option<File>> {
        if self.poisoned {
            return Err(Error::Unavailable("legacy fixture uncertain"));
        }
        crate::store::snapshot_pin::cleanup(&self.root)
    }
    pub(crate) async fn close(self) -> Result<()> {
        let Self { backend, lock, .. } = self;
        let result = backend.close().await;
        drop(lock);
        result
    }
}
impl reference::ReferenceStore for LegacyFixture {
    fn state(&self) -> &State {
        &self.state
    }
    async fn transact(&mut self, transaction: Transaction) -> Result<Receipt> {
        if self.poisoned {
            return Err(Error::Unavailable("legacy fixture uncertain"));
        }
        let (next, commit) = self.state.prepare_reference(&transaction)?;
        if self.state.transactions.contains_key(&transaction.id) {
            return Ok(commit.receipt);
        }
        for mutation in &transaction.mutations {
            if let Mutation::Put { record, .. } = mutation {
                if record.collection == Collection::Artifact {
                    let descriptor: ArtifactDescriptor = record.decode()?;
                    self.spool.verify(&descriptor)?;
                }
            }
        }
        self.poisoned = true;
        self.backend.append(&commit, &next, &|_| {}).await?;
        self.state = next;
        self.poisoned = false;
        Ok(commit.receipt)
    }
}
