// SPDX-License-Identifier: Apache-2.0
//! Native ownership exists before canonical replay. This capability grants a
//! held physical root, never trust in a decoded state/history descriptor.
use crate::{artifact::reject_link, Error, Result};
use std::{
    fs::{self, File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
use vcp_domain::ControllerId;
use vcp_protocol::canonical_bytes;

pub(crate) struct CanonicalLock {
    root: PathBuf,
    forbidden: Vec<PathBuf>,
    owner: File,
}
impl CanonicalLock {
    /// Preserve Store's existing pre-creation forbidden-ancestor check and
    /// native exclusive lock. No public constructor accepts an arbitrary File.
    pub(crate) fn acquire(root: &Path, forbidden_roots: &[PathBuf]) -> Result<Self> {
        let absolute = std::path::absolute(root)?;
        let ancestor = absolute
            .ancestors()
            .find(|p| p.exists())
            .ok_or(Error::Access)?
            .canonicalize()?;
        for forbidden in forbidden_roots {
            if ancestor.starts_with(forbidden.canonicalize()?) {
                return Err(Error::Access);
            }
        }
        fs::create_dir_all(root)?;
        reject_link(root)?;
        let root = root.canonicalize()?;
        let owner_path = root.join("owner.lock");
        if owner_path.exists() {
            reject_link(&owner_path)?;
        }
        let mut owner = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&owner_path)?;
        owner
            .try_lock()
            .map_err(|_| Error::Conflict("canonical root already has an owner"))?;
        let diagnostic = canonical_bytes(
            &serde_json::json!({"pid":std::process::id(),"nonce":ControllerId::new(),"format":1}),
        )?;
        owner.seek(SeekFrom::Start(0))?;
        owner.write_all(&diagnostic)?;
        owner.set_len(diagnostic.len() as u64)?;
        owner.sync_all()?;
        let forbidden = forbidden_roots
            .iter()
            .map(|path| path.canonicalize())
            .collect::<std::io::Result<Vec<_>>>()?;
        Ok(Self {
            root,
            forbidden,
            owner,
        })
    }
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
    pub(crate) fn forbidden(&self) -> &[PathBuf] {
        &self.forbidden
    }
    pub(crate) fn verify(&self) -> Result<()> {
        let path = self.root.join("owner.lock");
        reject_link(&path)?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x0020_0000).share_mode(1 | 2);
        }
        let owner = options.open(path)?;
        if !crate::private_paths::allowed_handle(&owner, false)?
            || crate::vault_publish::native_identity(&owner)?
                != crate::vault_publish::native_identity(&self.owner)?
        {
            return Err(Error::Corruption("canonical owner identity changed"));
        }
        Ok(())
    }
    pub(crate) fn verify_child(&self, child: &Path) -> Result<()> {
        if child.parent() != Some(self.root.as_path())
            || self
                .forbidden
                .iter()
                .any(|root| child.starts_with(root) || root.starts_with(child))
        {
            return Err(Error::Access);
        }
        self.verify()
    }
}
impl std::ops::Deref for CanonicalLock {
    type Target = File;
    fn deref(&self) -> &File {
        &self.owner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{private_paths::Directory, BackendKind, Store};
    #[tokio::test]
    async fn pre_replay_lock_excludes_live_store_and_checks_native_root_and_forbidden_children() {
        for kind in [BackendKind::Files, BackendKind::Sqlite] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("canonical");
            let workspace = temp.path().join("workspace");
            fs::create_dir(&workspace).unwrap();
            let lock = CanonicalLock::acquire(&root, &[workspace.clone()]).unwrap();
            assert_eq!(lock.forbidden(), [workspace.canonicalize().unwrap()]);
            assert!(matches!(
                Store::open(&root, kind, &[workspace.clone()]).await,
                Err(Error::Conflict("canonical root already has an owner"))
            ));
            assert!(CanonicalLock::acquire(&root, &[]).is_err());
            let directory = Directory::locked_child(&lock, "history-pages").unwrap();
            assert_eq!(directory.path, lock.root().join("history-pages"));
            assert!(lock.verify_child(&temp.path().join("outside")).is_err());
            for name in ["..", "../other", "history/pages", "C:", ""] {
                assert!(Directory::locked_child(&lock, name).is_err());
            }
            drop(directory);
            drop(lock);
            let store = Store::open(&root, kind, &[workspace.clone()])
                .await
                .unwrap();
            assert!(CanonicalLock::acquire(&root, &[]).is_err());
            store.close().await.unwrap();
            assert!(
                CanonicalLock::acquire(&workspace.join("blocked"), &[workspace.clone()]).is_err()
            );
            assert!(!workspace.join("blocked").exists());
        }
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        let forbidden = root.join("history-pages");
        fs::create_dir_all(&forbidden).unwrap();
        let lock = CanonicalLock::acquire(&root, &[forbidden.clone()]).unwrap();
        assert!(Directory::locked_child(&lock, "history-pages").is_err());
        assert_eq!(fs::read_dir(forbidden).unwrap().count(), 0);
    }
}
