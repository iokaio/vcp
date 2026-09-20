// SPDX-License-Identifier: Apache-2.0
//! Explicit local enrollment and ciphertext-only publication foundation. The
//! canonical snapshot job must durably retain its pins and this admission before
//! lengthy copy; publication here does not claim cloud transfer or restore.
use crate::{
    keys::{LocalKeys, PublicKeys, RecoveryCopy, VerifiedKeys},
    private_paths::{self, Directory},
    vault_crypto::{self, FinalizedCiphertext, Limits, Manifest, PrivateStaging, Restored, Trust},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
};
use vcp_domain::{CommandId, WorkspaceId};
use vcp_protocol::{canonical_bytes, digest_bytes};

fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub sequence: u64,
    pub deletion: u64,
    pub parent: Option<String>,
}
impl Checkpoint {
    fn validate(&self) -> Result<()> {
        if self.parent.as_ref().is_some_and(|p| !hash(p))
            || (self.sequence > 0 && self.parent.is_none())
        {
            return Err(Error::Access);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicConfiguration {
    pub version: u32,
    pub revision: u64,
    pub workspace: WorkspaceId,
    pub lineage: String,
    pub selected: PublicKeys,
    pub writers: BTreeSet<[u8; 32]>,
    pub checkpoint: Checkpoint,
    pub recovery_verified: bool,
    /// Local pins cannot prove a globally newest cloud object on a fresh host.
    pub global_newest_known: bool,
}
/// No Deserialize: repository/restored configuration does not enroll trust.
/// The embedding host invokes enrollment only on its dedicated developer path.
pub struct LocalTrust {
    public: PublicConfiguration,
}
/// Exact trusted head verification cannot be used to advance or activate a
/// descendant. In particular it never lowers the normal replay floor.
pub struct VerifiedHead {
    manifest_digest: String,
}
impl VerifiedHead {
    pub fn manifest_digest(&self) -> &str {
        &self.manifest_digest
    }
}
pub struct VerifiedRestore {
    restored: Restored,
    revision: u64,
}
impl VerifiedRestore {
    pub fn restored(&self) -> &Restored {
        &self.restored
    }
}
impl LocalTrust {
    /// Only the independently owned local trust journal may reconstruct authority.
    pub(crate) fn from_local_configuration(public: PublicConfiguration) -> Result<Self> {
        public.checkpoint.validate()?;
        let recipient: age::x25519::Recipient = public
            .selected
            .recipient
            .parse()
            .map_err(|_| Error::Access)?;
        let key_ref = digest_bytes(&canonical_bytes(&(
            "vcp-key-reference/1",
            &public.selected.recipient,
            public.selected.writer,
        ))?);
        if public.version != 1
            || !hash(&public.lineage)
            || !public.recovery_verified
            || public.global_newest_known
            || public.writers.len() > 256
            || recipient.to_string() != public.selected.recipient
            || key_ref != public.selected.key_ref
        {
            return Err(Error::Access);
        }
        for writer in public
            .writers
            .iter()
            .chain(std::iter::once(&public.selected.writer))
        {
            let key = ed25519_dalek::VerifyingKey::from_bytes(writer).map_err(|_| Error::Access)?;
            if key.is_weak() {
                return Err(Error::Access);
            }
        }
        Ok(Self { public })
    }
    pub fn verify_known_head(
        &self,
        path: &Path,
        recovery: &RecoveryCopy,
        manifest: &Manifest,
        limits: Limits,
    ) -> Result<VerifiedHead> {
        let digest = digest_bytes(&canonical_bytes(manifest)?);
        if self.public.checkpoint.parent.as_ref() != Some(&digest)
            || manifest.sequence != self.public.checkpoint.sequence
            || manifest.deletion != self.public.checkpoint.deletion
            || manifest.workspace != self.public.workspace
            || manifest.lineage != self.public.lineage
            || manifest.sequence == 0
        {
            return Err(Error::Conflict(
                "snapshot is not the independently trusted head",
            ));
        }
        let keys = LocalKeys::import(recovery)?;
        let restored = vault_crypto::decrypt(
            path,
            keys.identity(),
            &Trust {
                workspace: self.public.workspace.clone(),
                lineage: self.public.lineage.clone(),
                writers: self.public.writers.clone(),
                minimum_sequence: manifest.sequence - 1,
                minimum_deletion: self.public.checkpoint.deletion,
                parent: manifest.parent.clone(),
            },
            limits,
        )?;
        if &restored.manifest != manifest {
            return Err(Error::Corruption("verified head differs"));
        }
        Ok(VerifiedHead {
            manifest_digest: digest,
        })
    }
    pub fn advance_after_publication(&mut self, receipt: &Published, expected: u64) -> Result<()> {
        self.check_revision(expected)?;
        let proof = &receipt.proof;
        if proof.revision != expected {
            return Err(Error::Conflict("publication trust changed"));
        }
        self.matches(&proof.manifest, &proof.writer, &proof.recipient)?;
        let next = self.next()?;
        self.public.checkpoint = Checkpoint {
            sequence: proof.manifest.sequence,
            deletion: proof.manifest.deletion,
            parent: Some(digest_bytes(&canonical_bytes(&proof.manifest)?)),
        };
        self.public.revision = next;
        Ok(())
    }
    pub fn enroll(
        keys: &VerifiedKeys,
        workspace: WorkspaceId,
        lineage: String,
        checkpoint: Checkpoint,
    ) -> Result<Self> {
        checkpoint.validate()?;
        if !hash(&lineage) {
            return Err(Error::Access);
        }
        Self::from_local_configuration(PublicConfiguration {
            version: 1,
            revision: 0,
            workspace,
            lineage,
            selected: keys.public().clone(),
            writers: BTreeSet::from([keys.public().writer]),
            checkpoint,
            recovery_verified: true,
            global_newest_known: false,
        })
    }
    pub fn configuration(&self) -> &PublicConfiguration {
        &self.public
    }
    fn check_revision(&self, expected: u64) -> Result<()> {
        if self.public.revision != expected {
            return Err(Error::Conflict("developer trust revision changed"));
        }
        Ok(())
    }
    fn next(&self) -> Result<u64> {
        self.public
            .revision
            .checked_add(1)
            .ok_or(Error::Limit("trust revision"))
    }
    fn trust(&self) -> Trust {
        Trust {
            workspace: self.public.workspace.clone(),
            lineage: self.public.lineage.clone(),
            writers: self.public.writers.clone(),
            minimum_sequence: self.public.checkpoint.sequence,
            minimum_deletion: self.public.checkpoint.deletion,
            parent: self.public.checkpoint.parent.clone(),
        }
    }
    pub fn rotate(&mut self, keys: &VerifiedKeys, expected: u64) -> Result<()> {
        self.check_revision(expected)?;
        if keys.public() == &self.public.selected {
            return Ok(());
        }
        if self.public.writers.len() >= 256 && !self.public.writers.contains(&keys.public().writer)
        {
            return Err(Error::Limit("enrolled writers"));
        }
        let next = self.next()?;
        self.public.selected = keys.public().clone();
        self.public.writers.insert(keys.public().writer);
        self.public.revision = next;
        Ok(())
    }
    pub fn revoke_writer(&mut self, writer: [u8; 32], expected: u64) -> Result<()> {
        self.check_revision(expected)?;
        if !self.public.writers.contains(&writer) {
            return Ok(());
        }
        let next = self.next()?;
        self.public.writers.remove(&writer);
        self.public.revision = next;
        Ok(())
    }
    /// Called under the canonical retention/publication fence. Already-admitted
    /// copies remain separately tracked cleanup obligations; new admissions fail.
    pub fn raise_deletion_floor(&mut self, deletion: u64, expected: u64) -> Result<()> {
        self.check_revision(expected)?;
        if deletion < self.public.checkpoint.deletion {
            return Err(Error::Conflict("deletion floor cannot roll back"));
        }
        if deletion == self.public.checkpoint.deletion {
            return Ok(());
        }
        let next = self.next()?;
        self.public.checkpoint.deletion = deletion;
        self.public.revision = next;
        Ok(())
    }
    fn matches(&self, manifest: &Manifest, writer: &[u8; 32], recipient: &str) -> Result<()> {
        if manifest.workspace != self.public.workspace
            || manifest.lineage != self.public.lineage
            || manifest.sequence <= self.public.checkpoint.sequence
            || manifest.deletion < self.public.checkpoint.deletion
            || manifest.parent != self.public.checkpoint.parent
            || !self.public.writers.contains(writer)
            || recipient != self.public.selected.recipient
            || writer != &self.public.selected.writer
        {
            return Err(Error::Conflict(
                "writer, recipient, lineage or deletion admission changed",
            ));
        }
        Ok(())
    }
    pub fn encrypt(
        &self,
        keys: &VerifiedKeys,
        staging: &PrivateStaging,
        manifest: Manifest,
        payloads: BTreeMap<String, Vec<u8>>,
        expected: u64,
        limits: Limits,
    ) -> Result<FinalizedCiphertext> {
        self.check_revision(expected)?;
        if keys.public() != &self.public.selected {
            return Err(Error::Access);
        }
        self.matches(&manifest, &keys.public().writer, &keys.public().recipient)?;
        vault_crypto::encrypt(
            staging,
            &keys.recipient,
            &keys.writer,
            manifest,
            payloads,
            limits,
        )
    }
    pub fn verify_restore(
        &self,
        path: &Path,
        copy: &RecoveryCopy,
        limits: Limits,
    ) -> Result<VerifiedRestore> {
        let keys = LocalKeys::import(copy)?;
        let restored = vault_crypto::decrypt(path, keys.identity(), &self.trust(), limits)?;
        Ok(VerifiedRestore {
            restored,
            revision: self.public.revision,
        })
    }
    /// Parent activation commits this update with its canonical restore receipt.
    /// The source is a verified opaque receipt, never archive-supplied trust.
    pub fn advance_after_restore(
        &mut self,
        restored: &VerifiedRestore,
        expected: u64,
    ) -> Result<()> {
        self.check_revision(expected)?;
        if restored.revision != expected {
            return Err(Error::Conflict("restore trust changed after validation"));
        }
        let next = self.next()?;
        self.public.checkpoint = Checkpoint {
            sequence: restored.restored.manifest.sequence,
            deletion: restored.restored.manifest.deletion,
            parent: Some(digest_bytes(&canonical_bytes(&restored.restored.manifest)?)),
        };
        self.public.revision = next;
        Ok(())
    }
    /// The embedding canonical controller records the admission before copying;
    /// revocation/purge ordered before this call prevents publication.
    pub fn admit(
        &self,
        object: &FinalizedCiphertext,
        operation: CommandId,
        expected: u64,
    ) -> Result<PublicationPermit> {
        self.check_revision(expected)?;
        let name = operation.as_str();
        if name.len() != 36
            || name.bytes().enumerate().any(|(n, b)| {
                if [8, 13, 18, 23].contains(&n) {
                    b != b'-'
                } else {
                    !b.is_ascii_hexdigit()
                }
            })
        {
            return Err(Error::Access);
        }
        self.matches(&object.manifest, &object.writer, &object.recipient)?;
        Ok(PublicationPermit {
            proof: PublicationProof {
                operation: operation.clone(),
                ciphertext: object.sha256().into(),
                bytes: object.bytes(),
                manifest: object.manifest.clone(),
                writer: object.writer,
                recipient: object.recipient.clone(),
                revision: expected,
            },
            operation,
            ciphertext: object.sha256().into(),
            bytes: object.bytes(),
            manifest: digest_bytes(&canonical_bytes(&object.manifest)?),
            key_ref: self.public.selected.key_ref.clone(),
            revision: expected,
            sequence: object.manifest.sequence,
            deletion: object.manifest.deletion,
        })
    }
}
pub struct PublicationPermit {
    proof: PublicationProof,
    operation: CommandId,
    ciphertext: String,
    bytes: u64,
    manifest: String,
    key_ref: String,
    revision: u64,
    sequence: u64,
    deletion: u64,
}
#[derive(Clone, Debug)]
struct PublicationProof {
    operation: CommandId,
    ciphertext: String,
    bytes: u64,
    manifest: Manifest,
    writer: [u8; 32],
    recipient: String,
    revision: u64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Published {
    #[serde(skip)]
    proof: PublicationProof,
    pub operation: CommandId,
    pub object: String,
    pub ciphertext_sha256: String,
    pub bytes: u64,
    pub key_ref: String,
    pub trust_revision: u64,
    pub sequence: u64,
    pub deletion: u64,
    pub locally_published: bool,
    pub transfer_observed: bool,
    pub restore_verified: bool,
    pub cancellation_after_publication: bool,
}
impl Published {
    pub(crate) fn matches_job(
        &self,
        operation: &CommandId,
        receipt: &crate::vault_crypto::Finalization,
        revision: u64,
    ) -> bool {
        self.proof.operation == *operation
            && self.proof.ciphertext == receipt.sha256
            && self.proof.bytes == receipt.bytes
            && self.proof.manifest == receipt.manifest
            && self.proof.writer == receipt.writer
            && self.proof.recipient == receipt.recipient
            && self.proof.revision == revision
            && self.operation == *operation
            && self.ciphertext_sha256 == receipt.sha256
            && self.bytes == receipt.bytes
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    BeforeCopy,
    CiphertextBytes(u64),
    BeforeFinalize,
    LocallyPublished,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureCode {
    Cancelled,
    IdentityConflict,
    VaultUnavailable,
    CopyFailed,
    CleanupPending,
}
pub struct PublicationFailure {
    pub code: FailureCode,
    pub cleanup_pending: Option<OwnedPartial>,
}
impl std::fmt::Debug for PublicationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublicationFailure")
            .field("code", &self.code)
            .field(
                "cleanup_pending",
                &self.cleanup_pending.as_ref().map(|p| p.object.as_str()),
            )
            .finish()
    }
}
pub struct OwnedPartial {
    file: File,
    directory: Arc<Directory>,
    object: String,
}
impl OwnedPartial {
    pub fn object(&self) -> &str {
        &self.object
    }
    pub fn cleanup(self) -> std::result::Result<(), Self> {
        if private_paths::remove_owned(&self.file, &self.directory.path.join(&self.object)).is_err()
        {
            return Err(self);
        }
        Ok(())
    }
}
pub struct Vault {
    directory: Arc<Directory>,
}
impl Vault {
    /// Reconstruct a typed publication receipt only from an independently
    /// admitted finalized object and an exact, already existing vault copy.
    /// This operation never recreates a missing object or writes vault bytes.
    pub fn reconcile(
        &self,
        source: &FinalizedCiphertext,
        permit: &PublicationPermit,
    ) -> Result<Published> {
        if source.sha256() != permit.ciphertext
            || source.bytes() != permit.bytes
            || source.manifest != permit.proof.manifest
            || source.writer != permit.proof.writer
            || source.recipient != permit.proof.recipient
        {
            return Err(Error::Conflict("publication reconciliation source differs"));
        }
        let object = format!("{}.age", permit.operation);
        existing(&self.directory.path.join(&object), permit)?;
        Ok(published(permit, object, false))
    }
    pub fn open(path: &Path, private_roots: &[PathBuf]) -> Result<Self> {
        Ok(Self {
            directory: Arc::new(Directory::open_cloud(path, private_roots)?),
        })
    }
    /// Canonical public directory; this capability holds its native ancestry.
    pub fn directory(&self) -> &Path {
        &self.directory.path
    }
    /// Only the finalized production ciphertext capability is accepted here.
    /// No plaintext path/byte-stream overload exists.
    pub fn publish(
        &self,
        source: &mut FinalizedCiphertext,
        permit: &PublicationPermit,
        cancelled: &dyn Fn() -> bool,
        observe: &dyn Fn(Phase),
    ) -> std::result::Result<Published, PublicationFailure> {
        let fail = |code| PublicationFailure {
            code,
            cleanup_pending: None,
        };
        if source.sha256() != permit.ciphertext
            || source.bytes() != permit.bytes
            || canonical_bytes(&source.manifest)
                .map(|bytes| digest_bytes(&bytes))
                .ok()
                .as_ref()
                != Some(&permit.manifest)
        {
            return Err(fail(FailureCode::IdentityConflict));
        }
        let object = format!("{}.age", permit.operation);
        let path = self.directory.path.join(&object);
        observe(Phase::BeforeCopy);
        if cancelled() {
            return Err(fail(FailureCode::Cancelled));
        }
        let mut options = OpenOptions::new();
        options.create_new(true).read(true).write(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options
                .share_mode(1)
                .access_mode(0xC001_0000)
                .custom_flags(0x0020_0000);
        }
        let file = match options.open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if existing(&path, permit).is_err() {
                    return Err(fail(FailureCode::IdentityConflict));
                }
                return Ok(published(permit, object, cancelled()));
            }
            Err(_) => return Err(fail(FailureCode::VaultUnavailable)),
        };
        let partial = OwnedPartial {
            file,
            directory: self.directory.clone(),
            object: object.clone(),
        };
        finish_copy(source, permit, partial, cancelled, observe)
    }
}
fn finish_copy(
    source: &mut FinalizedCiphertext,
    permit: &PublicationPermit,
    mut partial: OwnedPartial,
    cancelled: &dyn Fn() -> bool,
    observe: &dyn Fn(Phase),
) -> std::result::Result<Published, PublicationFailure> {
    let object = partial.object.clone();
    let fail = |code| PublicationFailure {
        code,
        cleanup_pending: None,
    };
    let result = (|| -> Result<()> {
        let mut copied = 0u64;
        struct Output<'a> {
            file: &'a mut File,
            copied: &'a mut u64,
            cancelled: &'a dyn Fn() -> bool,
            observe: &'a dyn Fn(Phase),
        }
        impl Write for Output<'_> {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if (self.cancelled)() {
                    return Err(std::io::Error::other("publication cancelled"));
                }
                let count = self.file.write(bytes)?;
                *self.copied += count as u64;
                (self.observe)(Phase::CiphertextBytes(*self.copied));
                Ok(count)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.file.flush()
            }
        }
        source.copy_ciphertext(Output {
            file: &mut partial.file,
            copied: &mut copied,
            cancelled,
            observe,
        })?;
        observe(Phase::BeforeFinalize);
        if cancelled() {
            return Err(Error::Unavailable("publication cancelled"));
        }
        partial.file.sync_all()?;
        if copied != permit.bytes
            || file_digest(&mut partial.file, permit.bytes)? != permit.ciphertext
        {
            return Err(Error::Corruption("vault copy integrity"));
        }
        Ok(())
    })();
    if result.is_err() {
        let code = if cancelled() {
            FailureCode::Cancelled
        } else {
            FailureCode::CopyFailed
        };
        return match partial.cleanup() {
            Ok(()) => Err(fail(code)),
            Err(partial) => Err(PublicationFailure {
                code: FailureCode::CleanupPending,
                cleanup_pending: Some(partial),
            }),
        };
    }
    drop(partial);
    observe(Phase::LocallyPublished);
    Ok(published(permit, object, cancelled()))
}
fn published(permit: &PublicationPermit, object: String, cancelled: bool) -> Published {
    Published {
        proof: permit.proof.clone(),
        operation: permit.operation.clone(),
        object,
        ciphertext_sha256: permit.ciphertext.clone(),
        bytes: permit.bytes,
        key_ref: permit.key_ref.clone(),
        trust_revision: permit.revision,
        sequence: permit.sequence,
        deletion: permit.deletion,
        locally_published: true,
        transfer_observed: false,
        restore_verified: false,
        cancellation_after_publication: cancelled,
    }
}
fn file_digest(file: &mut File, bytes: u64) -> Result<String> {
    let metadata = file.metadata()?;
    if !metadata.is_file() || !private_paths::allowed_handle(file, true)? || metadata.len() != bytes
    {
        return Err(Error::Access);
    }
    file.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::new();
    let mut remaining = bytes;
    let mut buffer = [0u8; 65536];
    while remaining != 0 {
        let count = file.read(&mut buffer[..remaining.min(65536) as usize])?;
        if count == 0 {
            return Err(Error::Corruption("vault copy truncated"));
        }
        hash.update(&buffer[..count]);
        remaining -= count as u64;
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn existing(path: &Path, permit: &PublicationPermit) -> Result<()> {
    if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(Error::Access);
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1).custom_flags(0x0020_0000);
    }
    let mut file = options.open(path)?;
    if file_digest(&mut file, permit.bytes)? != permit.ciphertext {
        return Err(Error::Conflict("existing vault object differs"));
    }
    Ok(())
}

/// Public diagnostics of an owned native ciphertext object. Only the canonical
/// admitted job journal supplies this receipt back to recovery.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyIdentity {
    operation: CommandId,
    native: String,
    ciphertext: String,
    bytes: u64,
}
impl CopyIdentity {
    pub(crate) fn matches(
        &self,
        operation: &CommandId,
        finalization: &crate::vault_crypto::Finalization,
    ) -> bool {
        self.operation == *operation
            && self.ciphertext == finalization.sha256
            && self.bytes == finalization.bytes
            && !self.native.is_empty()
            && self.native.len() <= 128
    }
}
fn native_identity(file: &File) -> Result<String> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let mut value = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::zeroed();
        // SAFETY: live file handle and correctly sized writable output buffer.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), value.as_mut_ptr()) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: successful API call initialized the whole output structure.
        let value = unsafe { value.assume_init() };
        Ok(format!(
            "{:08x}:{:08x}{:08x}:{:08x}{:08x}",
            value.dwVolumeSerialNumber,
            value.nFileIndexHigh,
            value.nFileIndexLow,
            value.ftCreationTime.dwHighDateTime,
            value.ftCreationTime.dwLowDateTime
        ))
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = file.metadata()?;
        Ok(format!("{}:{}", m.dev(), m.ino()))
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = file;
        Err(Error::Incompatible)
    }
}
fn matching_prefix(source: &mut FinalizedCiphertext, file: &mut File) -> Result<()> {
    let length = file.metadata()?.len();
    if length > source.bytes() {
        return Err(Error::Corruption("vault partial exceeds final ciphertext"));
    }
    file.seek(SeekFrom::Start(0))?;
    struct Prefix<'a> {
        file: &'a mut File,
        remaining: u64,
    }
    impl Write for Prefix<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let n = self.remaining.min(bytes.len() as u64) as usize;
            if n > 0 {
                let mut expected = vec![0; n];
                self.file.read_exact(&mut expected)?;
                if expected != bytes[..n] {
                    return Err(std::io::Error::other("vault partial differs"));
                }
                self.remaining -= n as u64;
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    source.copy_ciphertext(Prefix {
        file,
        remaining: length,
    })
}
impl Vault {
    /// Persist native ownership through the canonical owner callback before the
    /// first ciphertext byte. The lengthy copy then runs without owner access.
    /// Recovery never truncates an unrecorded or replaced same-name file. A
    /// create-to-receipt crash therefore remains an explicit conflict/pending
    /// obligation rather than guessed ownership of a foreign object.
    #[allow(clippy::too_many_arguments)]
    pub async fn publish_recorded<F, Fut>(
        &self,
        source: &mut FinalizedCiphertext,
        permit: &PublicationPermit,
        prior: Option<&CopyIdentity>,
        on_owned: F,
        cancelled: &dyn Fn() -> bool,
        observe: &dyn Fn(Phase),
    ) -> std::result::Result<Published, PublicationFailure>
    where
        F: FnOnce(CopyIdentity) -> Fut,
        Fut: std::future::Future<Output = Result<()>>,
    {
        let fail = |code| PublicationFailure {
            code,
            cleanup_pending: None,
        };
        if source.sha256() != permit.ciphertext
            || source.bytes() != permit.bytes
            || source.finalization().manifest != permit.proof.manifest
        {
            return Err(fail(FailureCode::IdentityConflict));
        }
        let object = format!("{}.age", permit.operation);
        let path = self.directory.path.join(&object);
        observe(Phase::BeforeCopy);
        if cancelled() {
            return Err(fail(FailureCode::Cancelled));
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options
                .share_mode(1)
                .access_mode(0xC001_0000)
                .custom_flags(0x0020_0000);
        }
        let mut file = match options.open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if existing(&path, permit).is_ok() {
                    return Ok(published(permit, object, cancelled()));
                }
                let prior = prior.ok_or_else(|| fail(FailureCode::IdentityConflict))?;
                options.create_new(false);
                let mut file = options
                    .open(&path)
                    .map_err(|_| fail(FailureCode::VaultUnavailable))?;
                if !private_paths::allowed_handle(&file, true)
                    .map_err(|_| fail(FailureCode::VaultUnavailable))?
                    || !prior.matches(&permit.operation, &source.finalization())
                    || native_identity(&file).ok().as_ref() != Some(&prior.native)
                    || matching_prefix(source, &mut file).is_err()
                {
                    return Err(fail(FailureCode::IdentityConflict));
                }
                file
            }
            Err(_) => return Err(fail(FailureCode::VaultUnavailable)),
        };
        let identity = CopyIdentity {
            operation: permit.operation.clone(),
            native: native_identity(&file).map_err(|_| fail(FailureCode::CopyFailed))?,
            ciphertext: permit.ciphertext.clone(),
            bytes: permit.bytes,
        };
        let partial = OwnedPartial {
            file: file
                .try_clone()
                .map_err(|_| fail(FailureCode::CopyFailed))?,
            directory: self.directory.clone(),
            object,
        };
        if on_owned(identity).await.is_err() {
            drop(file);
            return match partial.cleanup() {
                Ok(()) => Err(fail(FailureCode::CopyFailed)),
                Err(partial) => Err(PublicationFailure {
                    code: FailureCode::CleanupPending,
                    cleanup_pending: Some(partial),
                }),
            };
        }
        if file
            .set_len(0)
            .and_then(|()| file.seek(SeekFrom::Start(0)).map(|_| ()))
            .is_err()
        {
            return Err(PublicationFailure {
                code: FailureCode::CleanupPending,
                cleanup_pending: Some(partial),
            });
        }
        drop(file);
        finish_copy(source, permit, partial, cancelled, observe)
    }
}
