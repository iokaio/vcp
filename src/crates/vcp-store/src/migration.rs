// SPDX-License-Identifier: Apache-2.0
//! Controlled local activation. Old roots remain recovery material until an
//! explicit retention operation; they never receive implicit merged writes.
use crate::{
    artifact::{immutable_file, read_bounded, reject_link},
    BackendKind, Barrier, Error, Result, Store,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};
use vcp_domain::{TransactionId, Watermark};
use vcp_protocol::{canonical_bytes, digest_bytes};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Activation {
    version: u32,
    generation: u64,
    root: TransactionId,
    backend: BackendKind,
    watermark: Watermark,
    logical_sha256: String,
    previous: String,
}
pub struct ActiveRoot {
    store: Store,
    directory: PathBuf,
    activation: Activation,
    forbidden: Vec<PathBuf>,
    poisoned: bool,
    #[cfg(feature = "qualification")]
    observer: Option<crate::backend::Observer>,
    _owner: File,
}
impl ActiveRoot {
    pub async fn open(
        directory: &Path,
        preference: Option<BackendKind>,
        forbidden: &[PathBuf],
    ) -> Result<Self> {
        let absolute = std::path::absolute(directory)?;
        let ancestor = absolute
            .ancestors()
            .find(|p| p.exists())
            .ok_or(Error::Access)?
            .canonicalize()?;
        for root in forbidden {
            if ancestor.starts_with(root.canonicalize()?) {
                return Err(Error::Access);
            }
        }
        fs::create_dir_all(directory)?;
        reject_link(directory)?;
        let directory = directory.canonicalize()?;
        let lock = directory.join("activation.lock");
        if lock.exists() {
            reject_link(&lock)?;
        }
        let owner = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock)?;
        owner
            .try_lock()
            .map_err(|_| Error::Conflict("activation already owned"))?;
        let roots = directory.join("roots");
        fs::create_dir_all(&roots)?;
        reject_link(&roots)?;
        let mut pointers = fs::read_dir(&directory)?
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .filter(|e| {
                e.file_name().to_string_lossy().starts_with("activation-")
                    && e.path().extension().is_some_and(|x| x == "json")
            })
            .map(|e| e.path())
            .collect::<Vec<_>>();
        pointers.sort();
        let mut prior = "0".repeat(64);
        let mut latest = None;
        for (index, path) in pointers.iter().enumerate() {
            let bytes = read_bounded(path, 4096)?;
            let activation: Activation = serde_json::from_slice(&bytes)?;
            if activation.version != 1
                || activation.generation != index as u64
                || activation.previous != prior
                || path.file_name().unwrap()
                    != format!("activation-{:020}.json", activation.generation).as_str()
            {
                return Err(Error::Corruption("activation chain"));
            }
            prior = digest_bytes(&bytes);
            latest = Some(activation);
        }
        let (store, activation) = if let Some(activation) = latest {
            if preference.is_some_and(|kind| kind != activation.backend) {
                return Err(Error::Incompatible);
            }
            let root = roots.join(activation.root.as_str());
            if !root.is_dir() {
                return Err(Error::Corruption("active root missing"));
            }
            let store = Store::open(&root, activation.backend, forbidden).await?;
            if store.state().watermark < activation.watermark {
                return Err(Error::Corruption("active root regressed"));
            }
            if store.prefix_digest(activation.watermark)? != activation.logical_sha256 {
                return Err(Error::Corruption("activated snapshot differs"));
            }
            (store, activation)
        } else {
            let kind = preference.unwrap_or(BackendKind::Sqlite);
            let id = TransactionId::new();
            let store = Store::open(&roots.join(id.as_str()), kind, forbidden).await?;
            let activation = Activation {
                version: 1,
                generation: 0,
                root: id,
                backend: kind,
                watermark: store.state().watermark,
                logical_sha256: digest_bytes(&canonical_bytes(store.state())?),
                previous: prior,
            };
            immutable_file(
                &directory.join("activation-00000000000000000000.json"),
                &canonical_bytes(&activation)?,
            )?;
            (store, activation)
        };
        Ok(Self {
            store,
            directory,
            activation,
            forbidden: forbidden.to_vec(),
            poisoned: false,
            #[cfg(feature = "qualification")]
            observer: None,
            _owner: owner,
        })
    }
    pub fn store(&self) -> &Store {
        &self.store
    }
    pub fn store_mut(&mut self) -> Result<&mut Store> {
        if self.poisoned {
            Err(Error::Unavailable("reopen to reconcile activation"))
        } else {
            Ok(&mut self.store)
        }
    }
    #[cfg(feature = "qualification")]
    pub fn observe(&mut self, observer: crate::backend::Observer) {
        self.observer = Some(observer);
    }
    fn barrier(&self, barrier: Barrier) {
        #[cfg(feature = "qualification")]
        if let Some(observer) = &self.observer {
            observer(barrier);
        }
        #[cfg(not(feature = "qualification"))]
        let _ = barrier;
    }
    pub async fn switch(&mut self, kind: BackendKind) -> Result<PathBuf> {
        if self.poisoned {
            return Err(Error::Unavailable("reopen to reconcile activation"));
        }
        if kind == self.activation.backend {
            return Err(Error::Conflict("backend already active"));
        }
        let id = TransactionId::new();
        let destination = self.directory.join("roots").join(id.as_str());
        let replacement = self
            .store
            .convert(&destination, kind, &self.forbidden)
            .await?;
        self.barrier(Barrier::BeforeValidation);
        drop(replacement);
        // A fresh open, not the converter's success flag, validates the candidate.
        let replacement = Store::open(&destination, kind, &self.forbidden).await?;
        if replacement.state() != self.store.state() {
            return Err(Error::Corruption("replacement differs from pinned source"));
        }
        let activation = Activation {
            version: 1,
            generation: self
                .activation
                .generation
                .checked_add(1)
                .ok_or(Error::Limit("activation generation"))?,
            root: id,
            backend: kind,
            watermark: replacement.state().watermark,
            logical_sha256: digest_bytes(&canonical_bytes(replacement.state())?),
            previous: digest_bytes(&canonical_bytes(&self.activation)?),
        };
        self.barrier(Barrier::BeforeActivation);
        self.poisoned = true;
        immutable_file(
            &self
                .directory
                .join(format!("activation-{:020}.json", activation.generation)),
            &canonical_bytes(&activation)?,
        )?;
        self.barrier(Barrier::AfterActivation);
        let recovery = self.store.root().to_owned();
        self.store = replacement;
        self.activation = activation;
        self.poisoned = false;
        Ok(recovery)
    }
}
