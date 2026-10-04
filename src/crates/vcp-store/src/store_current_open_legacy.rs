// SPDX-License-Identifier: Apache-2.0
//! The legacy source exists only during full replay and migration. No resident
//! historical State is returned as an alternative live owner.
use super::*;

impl Opened {
    pub(crate) async fn from_legacy(
        lock: CanonicalLock,
        mut backend: Backend,
        source: State,
        base: Option<crate::replay_base::ReplayBase>,
        artifact_limit: u64,
        diagnostics: &mut crate::StoreDiagnostics,
    ) -> Result<Self> {
        let kind = match &backend {
            Backend::Files(_) => BackendKind::Files,
            Backend::Sqlite(_) => BackendKind::Sqlite,
        };
        let pinned = (|| {
            let root = Directory::locked_root(&lock)?;
            // Keep the physical original file pinned across asynchronous shutdown
            // and reopening. Publication cannot switch a pathname to a different
            // file in that interval on Windows.
            let data_path = root.path.join(if kind == BackendKind::Files {
                "canonical.frames"
            } else {
                "canonical.sqlite"
            });
            crate::artifact::reject_link(&data_path)?;
            let mut options = OpenOptions::new();
            options.read(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options.custom_flags(0x0020_0000).share_mode(1 | 2);
            }
            let data = options.open(data_path)?;
            if !crate::private_paths::allowed_handle(&data, false)? {
                return Err(Error::Access);
            }
            if let Backend::Files(journal) = &backend {
                if crate::vault_publish::native_identity(&data)?
                    != crate::vault_publish::native_identity(&journal.file)?
                {
                    return Err(Error::Corruption("legacy canonical data identity changed"));
                }
            }
            let root_pin = crate::store::snapshot_pin::acquire(lock.root())?;
            Ok::<_, Error>((root, data, root_pin))
        })();
        let (root, data, root_pin) = match pinned {
            Ok(pinned) => pinned,
            Err(error) => {
                let _ = backend.close().await;
                return Err(error);
            }
        };
        let result = async {
            let base_watermark = base
                .as_ref()
                .map(|base| base.state.watermark)
                .unwrap_or_default();
            let mut reader = backend
                .history(lock.root(), &source, base_watermark)
                .await?;
            let staged = async {
                match &mut backend {
                    Backend::Files(_) => {
                        let directory = Directory::locked_child(&lock, "history-pages")?;
                        Origin::stage(
                            &mut io::Files::new(&directory),
                            &source,
                            base.as_ref(),
                            &mut reader,
                        )
                        .await
                    }
                    Backend::Sqlite(db) => {
                        let mut transaction = db.begin_with("BEGIN IMMEDIATE").await?;
                        let staged = async {
                            initialize_sqlite(&mut transaction).await?;
                            Origin::stage(
                                &mut io::Sqlite::new(&mut transaction),
                                &source,
                                base.as_ref(),
                                &mut reader,
                            )
                            .await
                        }
                        .await;
                        match staged {
                            Ok(staged) => {
                                transaction.commit().await?;
                                Ok(staged)
                            }
                            Err(error) => {
                                let _ = transaction.rollback().await;
                                Err(error)
                            }
                        }
                    }
                }
            }
            .await;
            let closed = reader.close().await;
            match staged {
                Err(error) => Err(error),
                Ok((_, digest, _)) => closed.map(|()| digest),
            }
        }
        .await;
        // Neither the old history DTO nor its base survives into the native
        // owner. The old connection finishes before the held lock is released.
        drop(source);
        drop(base);
        let closed = backend.close().await;
        let origin = match result {
            Err(error) => return Err(error),
            Ok(origin) => {
                closed?;
                origin
            }
        };
        // Full native replay remains mandatory, including the staged origin,
        // every original body and all materialized rows. A supplied source DTO
        // cannot grant admission merely by having a matching watermark.
        let result =
            Self::open_with_artifact_limit(lock, kind, &origin, artifact_limit, diagnostics).await;
        drop(root_pin);
        drop(data);
        drop(root);
        result
    }
}

#[cfg(test)]
#[path = "store_current_open_legacy_tests.rs"]
mod tests;
