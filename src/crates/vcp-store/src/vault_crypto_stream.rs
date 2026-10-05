// SPDX-License-Identifier: Apache-2.0
//! Signed-age/2 transport prerequisite. The opaque archive stream is staged
//! privately until authenticated EOF; semantic neutral-history/2 validation is
//! a separate mandatory step. No snapshot/restore owner calls this yet.
use super::*;

pub(crate) const FORMAT: &str = "vcp-signed-age/2";
const DOMAIN: &[u8] = b"vcp-portable-manifest-signature-v2\0";
const MAGIC: &[u8; 8] = b"VCPAGE02";
const FRAME_BYTES: usize = 128 * 1024;
const HEADER_BYTES: usize = 16 * 1024;

/// Operation bounds for the streamed format. These govern checked byte counts,
/// never allocation sizes; each frame/header retains its separate fixed bound.
#[derive(Clone, Copy)]
pub(crate) struct StreamLimits {
    pub(crate) plaintext_bytes: u64,
    pub(crate) payload_bytes: u64,
    pub(crate) ciphertext_bytes: u64,
}
impl StreamLimits {
    pub(crate) fn for_payload(bytes: u64) -> Result<Self> {
        let frames = bytes.div_ceil(FRAME_BYTES as u64);
        let plaintext_bytes = frames
            .checked_mul(36)
            .and_then(|overhead| bytes.checked_add(overhead))
            .and_then(|bytes| bytes.checked_add(HEADER_BYTES as u64 + 64))
            .ok_or(Error::Limit("streaming plaintext size"))?;
        // age uses 64KiB chunks with a 16-byte tag. The extra 64KiB covers
        // the single-recipient age header and final chunk, without buffering it.
        let ciphertext_bytes = plaintext_bytes
            .div_ceil(65536)
            .checked_mul(16)
            .and_then(|overhead| plaintext_bytes.checked_add(overhead))
            .and_then(|bytes| bytes.checked_add(65536))
            .ok_or(Error::Limit("streaming ciphertext size"))?;
        Ok(Self {
            plaintext_bytes,
            payload_bytes: bytes,
            ciphertext_bytes,
        })
    }
    fn validate(self) -> Result<()> {
        if self.plaintext_bytes == 0
            || self.payload_bytes > self.plaintext_bytes
            || self.ciphertext_bytes == 0
        {
            return Err(Error::Limit("streaming transport bounds"));
        }
        Ok(())
    }
}
impl From<Limits> for StreamLimits {
    fn from(value: Limits) -> Self {
        Self {
            plaintext_bytes: value.plaintext_bytes as u64,
            payload_bytes: value.payload_bytes as u64,
            ciphertext_bytes: value.ciphertext_bytes as u64,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootDescriptor {
    pub format: String,
    pub sha256: String,
    /// Length of the exact bounded root descriptor, never the total archive.
    pub descriptor_bytes: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamManifest {
    pub format: String,
    pub workspace: WorkspaceId,
    pub lineage: String,
    pub sequence: u64,
    pub deletion: u64,
    pub parent: Option<String>,
    pub archive_root: RootDescriptor,
    /// Commitment to the complete ordered neutral archive wire stream.
    pub payload: Object,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    writer: [u8; 32],
    manifest: StreamManifest,
    signature: Vec<u8>,
}
pub(super) fn validate(manifest: &StreamManifest, limits: StreamLimits) -> Result<()> {
    limits.validate()?;
    if manifest.format != FORMAT || manifest.archive_root.format != "vcp-neutral-history/2" {
        return Err(Error::Incompatible);
    }
    if !hash(&manifest.lineage)
        || manifest.sequence == 0
        || manifest.parent.as_ref().is_some_and(|value| !hash(value))
        || !hash(&manifest.archive_root.sha256)
        || manifest.archive_root.descriptor_bytes == 0
        || manifest.archive_root.descriptor_bytes > FRAME_BYTES as u64
        || !hash(&manifest.payload.sha256)
        || manifest.payload.bytes > limits.payload_bytes
    {
        return Err(Error::Corruption("streaming snapshot manifest"));
    }
    Ok(())
}
fn trust_manifest(header: &Header, trust: &Trust, limits: StreamLimits) -> Result<()> {
    if !hash(&trust.lineage)
        || trust.writers.is_empty()
        || trust.writers.len() > 256
        || trust.parent.as_ref().is_some_and(|value| !hash(value))
        || !trust.writers.contains(&header.writer)
    {
        return Err(Error::Access);
    }
    validate(&header.manifest, limits)?;
    let key = VerifyingKey::from_bytes(&header.writer)
        .map_err(|_| Error::Corruption("invalid enrolled writer"))?;
    let signature = Signature::from_slice(&header.signature)
        .map_err(|_| Error::Corruption("invalid streaming writer signature"))?;
    let body = canonical_bytes(&header.manifest)?;
    key.verify_strict(&[DOMAIN, body.as_slice()].concat(), &signature)
        .map_err(|_| Error::Corruption("streaming writer authentication failed"))?;
    let manifest = &header.manifest;
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
    Ok(())
}
fn temporary(staging: &PrivateStaging, suffix: &str) -> Result<(PathBuf, File)> {
    let metadata = fs::symlink_metadata(&staging.directory.path)?;
    if !metadata.is_dir() || redirected(&metadata) {
        return Err(Error::Access);
    }
    let path = staging
        .directory
        .path
        .join(format!("{}.{}", CommandId::new(), suffix));
    let mut options = OpenOptions::new();
    options.create_new(true).read(true).write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options
            .share_mode(0)
            .access_mode(0xC001_0000)
            .custom_flags(0x0400_0000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(&path)?;
    #[cfg(unix)]
    fs::remove_file(&path)?;
    Ok((path, file))
}
struct Counted<T> {
    inner: T,
    count: u64,
    limit: u64,
}
impl<T> Counted<T> {
    fn new(inner: T, limit: u64) -> Self {
        Self {
            inner,
            count: 0,
            limit,
        }
    }
    fn advance(&mut self, bytes: usize) -> std::io::Result<()> {
        self.count = self
            .count
            .checked_add(bytes as u64)
            .filter(|count| *count <= self.limit)
            .ok_or_else(|| std::io::Error::other("streaming plaintext limit"))?;
        Ok(())
    }
}
impl<T: Write> Write for Counted<T> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > self.limit - self.count {
            return Err(std::io::Error::other("streaming plaintext limit"));
        }
        let count = self.inner.write(bytes)?;
        self.advance(count)?;
        Ok(count)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
impl<T: Read> Read for Counted<T> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        // Permit one byte beyond the bound only to detect overflow, never to
        // return a false EOF and accidentally bypass age's final authentication.
        let limit = (bytes.len() as u64).min((self.limit - self.count).saturating_add(1)) as usize;
        let count = self.inner.read(&mut bytes[..limit])?;
        self.advance(count)?;
        Ok(count)
    }
}
fn write_frames_checked(
    mut output: impl Write,
    mut input: impl Read,
    payload: &Object,
    check: &dyn Fn() -> Result<()>,
) -> Result<()> {
    let mut buffer = vec![0u8; FRAME_BYTES];
    let mut digest = Sha256::new();
    let mut total = 0u64;
    let mut frames = 0u64;
    loop {
        check()?;
        let mut used = 0;
        while used < buffer.len() {
            check()?;
            match input.read(&mut buffer[used..]) {
                Ok(0) => break,
                Ok(count) => used += count,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
        if used == 0 {
            break;
        }
        total = total
            .checked_add(used as u64)
            .filter(|bytes| *bytes <= payload.bytes)
            .ok_or(Error::Corruption("archive input exceeds commitment"))?;
        frames = frames
            .checked_add(1)
            .ok_or(Error::Limit("archive frame count"))?;
        digest.update(&buffer[..used]);
        output.write_all(&(used as u32).to_be_bytes())?;
        output.write_all(&Sha256::digest(&buffer[..used]))?;
        output.write_all(&buffer[..used])?;
    }
    let digest = digest.finalize();
    if total != payload.bytes || format!("{digest:x}") != payload.sha256 {
        return Err(Error::Corruption("archive input differs from commitment"));
    }
    output.write_all(&0u32.to_be_bytes())?;
    output.write_all(&frames.to_be_bytes())?;
    output.write_all(&total.to_be_bytes())?;
    output.write_all(&digest)?;
    Ok(())
}
fn fixed<const N: usize>(input: &mut impl Read) -> Result<[u8; N]> {
    let mut bytes = [0; N];
    input.read_exact(&mut bytes)?;
    Ok(bytes)
}
fn read_frames_checked(
    mut input: impl Read,
    mut output: impl Write,
    payload: &Object,
    check: &dyn Fn() -> Result<()>,
) -> Result<()> {
    let mut total = 0u64;
    let mut frames = 0u64;
    let mut digest = Sha256::new();
    let mut buffer = vec![0u8; FRAME_BYTES];
    loop {
        check()?;
        let bytes = u32::from_be_bytes(fixed(&mut input)?) as usize;
        if bytes == 0 {
            break;
        }
        // Canonical framing: every non-final frame is full; exact payload length
        // fixes every expected frame size and prevents alternate frame splits.
        let expected = (payload.bytes - total).min(FRAME_BYTES as u64) as usize;
        if bytes != expected || bytes > FRAME_BYTES {
            return Err(Error::Corruption("archive frame length"));
        }
        let hash = fixed::<32>(&mut input)?;
        input.read_exact(&mut buffer[..bytes])?;
        if Sha256::digest(&buffer[..bytes]).as_slice() != hash {
            return Err(Error::Corruption("archive frame digest"));
        }
        digest.update(&buffer[..bytes]);
        output.write_all(&buffer[..bytes])?;
        total += bytes as u64;
        frames += 1;
    }
    let declared_frames = u64::from_be_bytes(fixed(&mut input)?);
    let declared_total = u64::from_be_bytes(fixed(&mut input)?);
    let declared_digest = fixed::<32>(&mut input)?;
    let digest = digest.finalize();
    if frames != declared_frames
        || total != declared_total
        || total != payload.bytes
        || digest.as_slice() != declared_digest
        || format!("{digest:x}") != payload.sha256
    {
        return Err(Error::Corruption("archive footer commitment"));
    }
    if input.read(&mut [0u8; 1])? != 0 {
        return Err(Error::Corruption("trailing streaming archive bytes"));
    }
    Ok(())
}

pub(crate) struct Encrypted {
    path: PathBuf,
    file: File,
    bytes: u64,
    sha256: String,
    pub(crate) manifest: StreamManifest,
    pub(crate) writer: [u8; 32],
    recipient: String,
    _staging: Arc<Directory>,
}
impl Encrypted {
    /// Consumes only a completely finalized, synced encoder result. Publication
    /// uses the existing ciphertext capability and cannot accept raw streams.
    pub(crate) fn into_finalized(self) -> FinalizedCiphertext {
        FinalizedCiphertext {
            path: self.path,
            file: self.file,
            bytes: self.bytes,
            sha256: self.sha256,
            manifest: self.manifest.into(),
            writer: self.writer,
            recipient: self.recipient,
            _staging: self._staging,
        }
    }
    pub(crate) fn copy_ciphertext(&mut self, mut output: impl Write) -> Result<()> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut digest = Sha256::new();
        let mut bytes = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            let count = self.file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            bytes += count as u64;
            if bytes > self.bytes {
                return Err(Error::Corruption("streaming ciphertext grew"));
            }
            digest.update(&buffer[..count]);
            output.write_all(&buffer[..count])?;
        }
        if bytes != self.bytes || format!("{:x}", digest.finalize()) != self.sha256 {
            return Err(Error::Corruption("streaming ciphertext changed"));
        }
        Ok(())
    }
}
pub(crate) fn encrypt(
    staging: &PrivateStaging,
    recipient: &Recipient,
    writer: &SigningKey,
    manifest: StreamManifest,
    input: impl Read,
    limits: impl Into<StreamLimits>,
) -> Result<Encrypted> {
    encrypt_checked(staging, recipient, writer, manifest, input, limits, &|| {
        Ok(())
    })
}
pub(crate) fn encrypt_checked(
    staging: &PrivateStaging,
    recipient: &Recipient,
    writer: &SigningKey,
    manifest: StreamManifest,
    input: impl Read,
    limits: impl Into<StreamLimits>,
    check: &dyn Fn() -> Result<()>,
) -> Result<Encrypted> {
    check()?;
    let limits = limits.into();
    validate(&manifest, limits)?;
    let body = canonical_bytes(&manifest)?;
    let header = Header {
        writer: writer.verifying_key().to_bytes(),
        signature: writer
            .sign(&[DOMAIN, body.as_slice()].concat())
            .to_bytes()
            .to_vec(),
        manifest: manifest.clone(),
    };
    let header = canonical_bytes(&header)?;
    if header.len() > HEADER_BYTES {
        return Err(Error::Limit("streaming crypto header"));
    }
    let (path, file) = temporary(staging, "cipher")?;
    let encryptor =
        age::Encryptor::with_recipients(std::iter::once(recipient as &dyn age::Recipient))
            .map_err(|_| Error::Unavailable("recipient encryption failed"))?;
    let encoder = encryptor
        .wrap_output(BoundedOutput {
            file,
            remaining: limits.ciphertext_bytes,
            #[cfg(feature = "qualification")]
            storage_full_after: staging.storage_full_after,
        })
        .map_err(|_| Error::Unavailable("ciphertext initialization failed"))?;
    let mut output = Counted::new(encoder, limits.plaintext_bytes);
    output.write_all(MAGIC)?;
    output.write_all(&(header.len() as u32).to_be_bytes())?;
    output.write_all(&header)?;
    write_frames_checked(&mut output, input, &manifest.payload, check)?;
    check()?;
    let mut file = output
        .inner
        .finish()
        .map_err(|_| Error::Unavailable("ciphertext finalization failed"))?
        .file;
    file.sync_all()?;
    let bytes = file.metadata()?.len();
    file.seek(SeekFrom::Start(0))?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        check()?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(Encrypted {
        path,
        file,
        bytes,
        sha256: format!("{:x}", digest.finalize()),
        manifest,
        writer: writer.verifying_key().to_bytes(),
        recipient: recipient.to_string(),
        _staging: staging.directory.clone(),
    })
}

/// Constructed only after complete age authentication and signed transport
/// commitments. This is historical input, never restore activation authority.
pub(crate) struct Authenticated {
    file: File,
    pub(crate) manifest: StreamManifest,
    pub(crate) writer: [u8; 32],
    _staging: Arc<Directory>,
}
impl Authenticated {
    pub(crate) fn reader(&mut self) -> Result<impl Read + '_> {
        self.file.seek(SeekFrom::Start(0))?;
        Ok(&mut self.file)
    }
}
pub(crate) fn decrypt(
    staging: &PrivateStaging,
    path: &Path,
    identity: &Identity,
    trust: &Trust,
    limits: impl Into<StreamLimits>,
) -> Result<Authenticated> {
    decrypt_checked(staging, path, identity, trust, limits, &|| Ok(()))
}
pub(crate) fn decrypt_checked(
    staging: &PrivateStaging,
    path: &Path,
    identity: &Identity,
    trust: &Trust,
    limits: impl Into<StreamLimits>,
    check: &dyn Fn() -> Result<()>,
) -> Result<Authenticated> {
    decrypt_exact_checked(staging, path, identity, trust, limits, None, check)
}
pub(crate) fn decrypt_exact_checked(
    staging: &PrivateStaging,
    path: &Path,
    identity: &Identity,
    trust: &Trust,
    limits: impl Into<StreamLimits>,
    expected: Option<&Object>,
    check: &dyn Fn() -> Result<()>,
) -> Result<Authenticated> {
    check()?;
    let limits = limits.into();
    limits.validate()?;
    let mut ciphertext =
        crate::private_paths::PublicCiphertext::open_stream(path, limits.ciphertext_bytes)?;
    let decryptor = age::Decryptor::new(&mut ciphertext)
        .map_err(|_| Error::Corruption("invalid age ciphertext"))?;
    let decoder = decryptor
        .decrypt(std::iter::once(identity as &dyn age::Identity))
        .map_err(|_| Error::Corruption("recovery identity cannot decrypt"))?;
    let mut input = Counted::new(decoder, limits.plaintext_bytes);
    if &fixed::<8>(&mut input)? != MAGIC {
        return Err(Error::Incompatible);
    }
    let bytes = u32::from_be_bytes(fixed(&mut input)?) as usize;
    if bytes == 0 || bytes > HEADER_BYTES {
        return Err(Error::Limit("streaming crypto header"));
    }
    let mut header = vec![0; bytes];
    input.read_exact(&mut header)?;
    let parsed: Header = serde_json::from_slice(&header)
        .map_err(|_| Error::Corruption("streaming crypto header"))?;
    if canonical_bytes(&parsed)? != header {
        return Err(Error::Corruption("noncanonical streaming crypto header"));
    }
    trust_manifest(&parsed, trust, limits)?;
    let (_, mut file) = temporary(staging, "plain")?;
    // Tentative bytes only reach an exclusive delete-on-close private file.
    // The required EOF read also authenticates age's final stream chunk.
    read_frames_checked(&mut input, &mut file, &parsed.manifest.payload, check)?;
    drop(input);
    ciphertext.finish()?;
    if let Some(expected) = expected {
        ciphertext.finish_identity(expected.bytes, &expected.sha256)?;
    }
    file.sync_all()?;
    file.seek(SeekFrom::Start(0))?;
    check()?;
    Ok(Authenticated {
        file,
        manifest: parsed.manifest,
        writer: parsed.writer,
        _staging: staging.directory.clone(),
    })
}

#[cfg(test)]
fn write_frames(output: impl Write, input: impl Read, payload: &Object) -> Result<()> {
    write_frames_checked(output, input, payload, &|| Ok(()))
}
#[cfg(test)]
fn read_frames(input: impl Read, output: impl Write, payload: &Object) -> Result<()> {
    read_frames_checked(input, output, payload, &|| Ok(()))
}

#[cfg(test)]
#[path = "vault_crypto_stream_tests.rs"]
mod tests;
