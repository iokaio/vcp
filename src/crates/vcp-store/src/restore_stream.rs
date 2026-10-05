// SPDX-License-Identifier: Apache-2.0
//! Streamed format branch of the existing restore operation. Transport proof,
//! exact wire closure and full canonical replay precede any import authority.
use super::*;
use crate::{
    history_index::io::Files,
    portable_snapshot::{complete, wire},
    vault_crypto::{stream::StreamLimits, Object, PrivateStaging},
    vault_publish::stream::VerifiedStreamRestore,
};

pub(crate) struct ValidatedStream {
    pub(crate) restored: complete::Restored,
    pub(crate) proof: VerifiedStreamRestore,
    pub(crate) source: Directory,
    pub(crate) replayed: Directory,
    _attempt: Directory,
}
impl Restore {
    pub(super) async fn authenticate_stream(
        &self,
        trust: &LocalTrust,
        recovery: &RecoveryCopy,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<ValidatedStream>> {
        let check = || {
            if cancelled() {
                Err(Error::Unavailable("restore cancelled"))
            } else {
                Ok(())
            }
        };
        check()?;
        let path = self
            .directory
            .path
            .join(format!("validated-{}", TransactionId::new()));
        fs::create_dir(&path)?;
        let attempt = Directory::open(&path, &self.forbidden)?;
        for name in ["crypto", "source", "replayed", "closure"] {
            fs::create_dir(path.join(name))?;
        }
        let staging = PrivateStaging::open(&path.join("crypto"), &self.forbidden)?;
        // The acquired physical byte count independently bounds all streaming
        // work. A signed manifest further fixes the exact payload/frame count.
        let limits = StreamLimits {
            plaintext_bytes: self.status.bytes,
            payload_bytes: self.status.bytes,
            ciphertext_bytes: self.status.bytes,
        };
        let mut proof = match trust.verify_stream_restore_exact(
            &staging,
            &self.directory.path.join("ciphertext.age"),
            recovery,
            limits,
            &Object {
                sha256: self.status.ciphertext.clone(),
                bytes: self.status.bytes,
            },
            &check,
        ) {
            Ok(proof) => proof,
            Err(Error::Incompatible) => return Ok(None),
            Err(error) => return Err(error),
        };
        let source = Directory::open(&path.join("source"), &self.forbidden)?;
        let replayed = Directory::open(&path.join("replayed"), &self.forbidden)?;
        let closure = Directory::open(&path.join("closure"), &self.forbidden)?;
        let mut source_pages = Files::new(&source);
        let manifest = proof.manifest().clone();
        let untrusted = wire::read(
            proof.reader()?,
            &manifest.archive_root,
            &manifest.payload,
            &mut source_pages,
            &check,
        )
        .await?;
        let restored = complete::Archive::restore(
            untrusted,
            &mut source_pages,
            &mut Files::new(&replayed),
            &mut Files::new(&closure),
            &path.join("spool"),
            &self.forbidden,
            crate::artifact::DEFAULT_ARTIFACT_LIMIT,
            &self.status.workspace,
            &check,
        )
        .await?;
        check()?;
        Ok(Some(ValidatedStream {
            restored,
            proof,
            source,
            replayed,
            _attempt: attempt,
        }))
    }
}
