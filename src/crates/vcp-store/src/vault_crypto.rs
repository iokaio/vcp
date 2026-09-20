// SPDX-License-Identifier: Apache-2.0
//! Bounded signed age envelope, using P0-04's qualified age 0.11.2 and
//! ed25519-dalek 2.2.0 primitives. Adapted from VCP's Apache-2.0 storage-spike
//! vault.rs; this production format is deliberately domain/version separated.
//! Key enrollment, recovery verification, snapshot pinning and publication are
//! caller responsibilities. This module never publishes into a vault.
use crate::{private_paths::Directory, Error, Result};
use age::x25519::{Identity, Recipient};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
use vcp_domain::{CommandId, WorkspaceId};
use vcp_protocol::{canonical_bytes, digest_bytes};

pub const FORMAT: &str = "vcp-signed-age/1";
const DOMAIN: &[u8] = b"vcp-portable-manifest-signature-v1\0";
#[derive(Clone, Copy)]
pub struct Limits {
    pub plaintext_bytes: usize,
    pub payload_bytes: usize,
    pub ciphertext_bytes: usize,
    pub objects: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            plaintext_bytes: 16 * 1024 * 1024,
            payload_bytes: 4 * 1024 * 1024,
            ciphertext_bytes: 16 * 1024 * 1024 + 65536,
            objects: 1024,
        }
    }
}
impl Limits {
    fn validate(self) -> Result<()> {
        if self.plaintext_bytes == 0
            || self.plaintext_bytes > 64 * 1024 * 1024
            || self.payload_bytes == 0
            || self.payload_bytes > self.plaintext_bytes
            || self.ciphertext_bytes == 0
            || self.ciphertext_bytes > 65 * 1024 * 1024
            || self.objects == 0
            || self.objects > 4096
        {
            return Err(Error::Limit("vault crypto limits"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Object {
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    pub workspace: WorkspaceId,
    /// Independently enrolled lineage identity, a SHA-256-shaped opaque token.
    pub lineage: String,
    pub sequence: u64,
    pub deletion: u64,
    pub parent: Option<String>,
    /// Names are lowercase content hashes, never extracted filesystem paths.
    pub objects: BTreeMap<String, Object>,
}
pub struct Trust {
    pub workspace: WorkspaceId,
    pub lineage: String,
    pub writers: BTreeSet<[u8; 32]>,
    /// Exclusive lower bound: a known head cannot be replayed as newer work.
    pub minimum_sequence: u64,
    pub minimum_deletion: u64,
    pub parent: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    writer: [u8; 32],
    body: Vec<u8>,
    signature: Vec<u8>,
    payloads: BTreeMap<String, Vec<u8>>,
}
pub struct Restored {
    pub manifest: Manifest,
    pub payloads: BTreeMap<String, Vec<u8>>,
    pub writer: [u8; 32],
}
struct BoundedOutput {
    file: File,
    remaining: usize,
}
impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.remaining {
            return Err(std::io::Error::other("ciphertext output limit"));
        }
        let count = self.file.write(bytes)?;
        self.remaining -= count;
        Ok(count)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn redirected(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}
fn regular(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || redirected(&metadata) {
        return Err(Error::Access);
    }
    Ok(())
}

/// Parent must enroll this directory outside repositories and all known/declared
/// sync roots before construction, and protect its ancestors against concurrent
/// replacement. The supplied forbidden roots must include the intended vault.
/// Checking this capability is not universal synchronization-program detection.
pub struct PrivateStaging {
    directory: Arc<Directory>,
}
impl PrivateStaging {
    pub fn open(directory: &Path, forbidden_roots: &[PathBuf]) -> Result<Self> {
        Ok(Self {
            directory: Arc::new(Directory::open(directory, forbidden_roots)?),
        })
    }
}

/// No serde/Clone or public constructor: only successful encryption finalization
/// and local fsync can create the value accepted by the future publisher.
pub struct FinalizedCiphertext {
    path: PathBuf,
    file: File,
    sha256: String,
    bytes: u64,
    pub(crate) manifest: Manifest,
    pub(crate) writer: [u8; 32],
    pub(crate) recipient: String,
    _staging: Arc<Directory>,
}
impl FinalizedCiphertext {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn format(&self) -> &'static str {
        FORMAT
    }
    /// Copy only the held finalized object, rejecting any staging modification.
    /// Destination publication remains the caller's separate durable operation.
    pub fn copy_ciphertext(&mut self, mut destination: impl Write) -> Result<()> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut remaining = self.bytes;
        let mut digest = Sha256::new();
        let mut buffer = [0u8; 65536];
        while remaining > 0 {
            let count = self
                .file
                .read(&mut buffer[..remaining.min(65536) as usize])?;
            if count == 0 {
                return Err(Error::Corruption("finalized ciphertext shortened"));
            }
            digest.update(&buffer[..count]);
            destination.write_all(&buffer[..count])?;
            remaining -= count as u64;
        }
        if self.file.read(&mut buffer[..1])? != 0
            || format!("{:x}", digest.finalize()) != self.sha256
        {
            return Err(Error::Corruption("finalized ciphertext modified"));
        }
        Ok(())
    }
}
fn validate(
    manifest: &Manifest,
    payloads: &BTreeMap<String, Vec<u8>>,
    limits: Limits,
) -> Result<()> {
    if manifest.format != FORMAT {
        return Err(Error::Incompatible);
    }
    if !hash(&manifest.lineage)
        || manifest.sequence == 0
        || manifest.parent.as_ref().is_some_and(|p| !hash(p))
        || manifest.objects.len() > limits.objects
        || manifest.objects.len() != payloads.len()
    {
        return Err(Error::Corruption("snapshot inventory shape"));
    }
    let mut total = 0usize;
    for (name, object) in &manifest.objects {
        if !hash(name) || object.sha256 != *name {
            return Err(Error::Corruption("snapshot object name"));
        }
        let payload = payloads
            .get(name)
            .ok_or(Error::Corruption("missing snapshot object"))?;
        total = total
            .checked_add(payload.len())
            .ok_or(Error::Limit("snapshot payload overflow"))?;
        if total > limits.payload_bytes {
            return Err(Error::Limit("snapshot payload bytes"));
        }
        if object.bytes != payload.len() as u64 || digest_bytes(payload) != object.sha256 {
            return Err(Error::Corruption("snapshot payload integrity"));
        }
    }
    Ok(())
}
pub fn encrypt(
    staging: &PrivateStaging,
    recipient: &Recipient,
    writer: &SigningKey,
    manifest: Manifest,
    payloads: BTreeMap<String, Vec<u8>>,
    limits: Limits,
) -> Result<FinalizedCiphertext> {
    limits.validate()?;
    validate(&manifest, &payloads, limits)?;
    let body = canonical_bytes(&manifest)?;
    if body.len() > limits.plaintext_bytes {
        return Err(Error::Limit("snapshot manifest bytes"));
    }
    let signature = writer
        .sign(&[DOMAIN, body.as_slice()].concat())
        .to_bytes()
        .to_vec();
    let envelope = Envelope {
        writer: writer.verifying_key().to_bytes(),
        body,
        signature,
        payloads,
    };
    let plaintext = canonical_bytes(&envelope)?;
    if plaintext.len() > limits.plaintext_bytes {
        return Err(Error::Limit("snapshot plaintext bytes"));
    }
    let metadata = fs::symlink_metadata(&staging.directory.path)?;
    if !metadata.is_dir() || redirected(&metadata) {
        return Err(Error::Access);
    }
    let path = staging
        .directory
        .path
        .join(format!("{}.cipher", CommandId::new()));
    let mut options = OpenOptions::new();
    options.create_new(true).read(true).write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // The owned private staging object disappears on all failure/drop/crash
        // paths. A parent snapshot job retains its canonical pins for rebuild.
        options
            .share_mode(0)
            .access_mode(0xC001_0000)
            .custom_flags(0x0400_0000);
    }
    let file = options.open(&path)?;
    #[cfg(unix)]
    fs::remove_file(&path)?;
    let encryptor =
        age::Encryptor::with_recipients(std::iter::once(recipient as &dyn age::Recipient))
            .map_err(|_| Error::Unavailable("recipient encryption failed"))?;
    let mut encoder = encryptor
        .wrap_output(BoundedOutput {
            file,
            remaining: limits.ciphertext_bytes,
        })
        .map_err(|_| Error::Unavailable("ciphertext initialization failed"))?;
    encoder
        .write_all(&plaintext)
        .map_err(|_| Error::Unavailable("ciphertext write failed"))?;
    let mut file = encoder
        .finish()
        .map_err(|_| Error::Unavailable("ciphertext finalization failed"))?
        .file;
    file.sync_all()?;
    let bytes = file.metadata()?.len();
    if bytes > limits.ciphertext_bytes as u64 {
        return Err(Error::Limit("snapshot ciphertext bytes"));
    }
    file.seek(SeekFrom::Start(0))?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(FinalizedCiphertext {
        path,
        file,
        sha256: format!("{:x}", digest.finalize()),
        bytes,
        manifest,
        writer: writer.verifying_key().to_bytes(),
        recipient: recipient.to_string(),
        _staging: staging.directory.clone(),
    })
}

/// Read only local hydrated ciphertext. No tentative plaintext escapes before
/// age's full stream authentication, writer trust and inventory validation pass.
pub fn decrypt(
    path: &Path,
    identity: &Identity,
    trust: &Trust,
    limits: Limits,
) -> Result<Restored> {
    limits.validate()?;
    if !hash(&trust.lineage)
        || trust.writers.is_empty()
        || trust.writers.len() > 256
        || trust.parent.as_ref().is_some_and(|p| !hash(p))
    {
        return Err(Error::Access);
    }
    let ciphertext = crate::private_paths::read_public_ciphertext(path, limits.ciphertext_bytes)?;
    let decryptor = age::Decryptor::new(ciphertext.as_slice())
        .map_err(|_| Error::Corruption("invalid age ciphertext"))?;
    let decoder = decryptor
        .decrypt(std::iter::once(identity as &dyn age::Identity))
        .map_err(|_| Error::Corruption("recovery identity cannot decrypt"))?;
    let mut plaintext = Vec::new();
    decoder
        .take(limits.plaintext_bytes as u64 + 1)
        .read_to_end(&mut plaintext)
        .map_err(|_| Error::Corruption("ciphertext incomplete or authentication failed"))?;
    if plaintext.len() > limits.plaintext_bytes {
        return Err(Error::Limit("snapshot plaintext input"));
    }
    let envelope: Envelope = serde_json::from_slice(&plaintext)
        .map_err(|_| Error::Corruption("invalid protected envelope"))?;
    if canonical_bytes(&envelope)? != plaintext {
        return Err(Error::Corruption(
            "noncanonical or duplicate envelope fields",
        ));
    }
    if !trust.writers.contains(&envelope.writer) {
        return Err(Error::Access);
    }
    if envelope.signature.len() != 64 || envelope.body.len() > limits.plaintext_bytes {
        return Err(Error::Corruption("invalid writer signature envelope"));
    }
    let key = VerifyingKey::from_bytes(&envelope.writer)
        .map_err(|_| Error::Corruption("invalid enrolled writer"))?;
    let signature = Signature::from_slice(&envelope.signature)
        .map_err(|_| Error::Corruption("invalid writer signature"))?;
    key.verify_strict(&[DOMAIN, envelope.body.as_slice()].concat(), &signature)
        .map_err(|_| Error::Corruption("writer authentication failed"))?;
    let manifest: Manifest = serde_json::from_slice(&envelope.body)
        .map_err(|_| Error::Corruption("invalid protected inventory"))?;
    if canonical_bytes(&manifest)? != envelope.body {
        return Err(Error::Corruption(
            "noncanonical or duplicate inventory fields",
        ));
    }
    if manifest.workspace != trust.workspace
        || manifest.lineage != trust.lineage
        || manifest.parent != trust.parent
        || manifest.sequence <= trust.minimum_sequence
        || manifest.deletion < trust.minimum_deletion
    {
        return Err(Error::Conflict(
            "snapshot workspace, lineage or freshness mismatch",
        ));
    }
    validate(&manifest, &envelope.payloads, limits)?;
    Ok(Restored {
        manifest,
        payloads: envelope.payloads,
        writer: envelope.writer,
    })
}

/// Durable receipt of an already finalized encoder, retained only in canonical
/// job metadata. This is not a public plaintext-to-publisher constructor.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Finalization {
    pub manifest: Manifest,
    pub writer: [u8; 32],
    pub recipient: String,
    pub sha256: String,
    pub bytes: u64,
}
impl FinalizedCiphertext {
    pub(crate) fn finalization(&self) -> Finalization {
        Finalization {
            manifest: self.manifest.clone(),
            writer: self.writer,
            recipient: self.recipient.clone(),
            sha256: self.sha256.clone(),
            bytes: self.bytes,
        }
    }
    pub(crate) fn reopen(
        path: &Path,
        receipt: &Finalization,
        directory: Arc<Directory>,
    ) -> Result<Self> {
        regular(path)?;
        if receipt.bytes > 65 * 1024 * 1024 || !hash(&receipt.sha256) {
            return Err(Error::Limit("persisted ciphertext"));
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1).custom_flags(0x0020_0000);
        }
        let file = options.open(path)?;
        if redirected(&file.metadata()?) || file.metadata()?.len() != receipt.bytes {
            return Err(Error::Corruption("persisted ciphertext identity"));
        }
        let mut value = Self {
            path: path.to_owned(),
            file,
            sha256: receipt.sha256.clone(),
            bytes: receipt.bytes,
            manifest: receipt.manifest.clone(),
            writer: receipt.writer,
            recipient: receipt.recipient.clone(),
            _staging: directory,
        };
        value.copy_ciphertext(std::io::sink())?;
        Ok(value)
    }
}
