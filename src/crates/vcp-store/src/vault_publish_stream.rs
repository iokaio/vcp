// SPDX-License-Identifier: Apache-2.0
//! Signed-age/2 adapters for the existing local trust and publication boundary.
//! No production snapshot job selects this format before source qualification.
use super::*;
use crate::vault_crypto::stream::{self, Authenticated, StreamManifest};

/// Issued only after full transport authentication against independently owned
/// local trust. Semantic archive replay and canonical restore admission remain
/// separate required gates before calling the checkpoint advancement method.
pub(crate) struct VerifiedStreamRestore {
    authenticated: Authenticated,
    revision: u64,
}
impl VerifiedStreamRestore {
    pub(crate) fn manifest(&self) -> &StreamManifest {
        &self.authenticated.manifest
    }
    pub(crate) fn reader(&mut self) -> Result<impl Read + '_> {
        self.authenticated.reader()
    }
}
impl LocalTrust {
    pub(crate) fn encrypt_stream(
        &self,
        keys: &VerifiedKeys,
        staging: &PrivateStaging,
        manifest: StreamManifest,
        input: impl Read,
        expected: u64,
        limits: Limits,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<FinalizedCiphertext> {
        self.check_revision(expected)?;
        if keys.public() != &self.public.selected {
            return Err(Error::Access);
        }
        self.matches(
            &manifest.clone().into(),
            &keys.public().writer,
            &keys.public().recipient,
        )?;
        Ok(stream::encrypt_checked(
            staging,
            &keys.recipient,
            &keys.writer,
            manifest,
            input,
            limits,
            check,
        )?
        .into_finalized())
    }
    pub(crate) fn verify_stream_restore(
        &self,
        staging: &PrivateStaging,
        path: &Path,
        copy: &RecoveryCopy,
        limits: Limits,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<VerifiedStreamRestore> {
        let keys = LocalKeys::import(copy)?;
        let authenticated =
            stream::decrypt_checked(staging, path, keys.identity(), &self.trust(), limits, check)?;
        Ok(VerifiedStreamRestore {
            authenticated,
            revision: self.public.revision,
        })
    }
    pub(crate) fn advance_after_stream_restore(
        &mut self,
        restored: &VerifiedStreamRestore,
        expected: u64,
    ) -> Result<()> {
        self.check_revision(expected)?;
        if restored.revision != expected {
            return Err(Error::Conflict("restore trust changed after validation"));
        }
        let next = self.next()?;
        self.public.checkpoint = Checkpoint {
            sequence: restored.authenticated.manifest.sequence,
            deletion: restored.authenticated.manifest.deletion,
            parent: Some(digest_bytes(&canonical_bytes(
                &restored.authenticated.manifest,
            )?)),
        };
        self.public.revision = next;
        Ok(())
    }
}
#[cfg(test)]
#[path = "vault_publish_stream_tests.rs"]
mod tests;
