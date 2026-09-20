// SPDX-License-Identifier: Apache-2.0
//! Developer-only local custody. Secret types deliberately implement neither
//! Debug nor serde. Host enrollment must bypass argv, prompts and transcripts;
//! ordinary project/restored configuration cannot mint VerifiedKeys.
use crate::{
    private_paths::{self, Directory},
    Error, Result,
};
use age::{
    secrecy::ExposeSecret,
    x25519::{Identity, Recipient},
};
use ed25519_dalek::{Signer, SigningKey};
use rand::TryRngCore;
use serde::{Deserialize, Serialize};
use std::{
    fmt::Write as _,
    fs::OpenOptions,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};
use vcp_domain::CommandId;
use vcp_protocol::{canonical_bytes, digest_bytes};
use zeroize::Zeroizing;

const HEADER: &str = "# VCP recovery bundle 1";
const WRITER: &str = "# VCP writer Ed25519 ";
const RECOVERY_BYTES: u64 = 4096;
const RECOVERY_DOMAIN: &[u8] = b"vcp-independent-recovery-check-v1\0";

pub struct RecoveryDirectory(Arc<Directory>);
impl RecoveryDirectory {
    /// `forbidden` must include every known/declared sync root, workspace,
    /// canonical data root and plaintext snapshot staging root. Unknown sync
    /// software cannot be discovered universally by this path check.
    pub fn open(path: &Path, forbidden: &[PathBuf]) -> Result<Self> {
        Ok(Self(Arc::new(Directory::open(path, forbidden)?)))
    }
    pub fn open_copy(&self, id: &str) -> Result<RecoveryCopy> {
        if id.len() != 36
            || id.bytes().enumerate().any(|(n, b)| {
                if [8, 13, 18, 23].contains(&n) {
                    b != b'-'
                } else {
                    !b.is_ascii_hexdigit()
                }
            })
        {
            return Err(Error::Access);
        }
        let copy = RecoveryCopy {
            directory: self.0.clone(),
            id: id.to_owned(),
        };
        copy.read()?;
        Ok(copy)
    }
}
pub struct RecoveryCopy {
    directory: Arc<Directory>,
    id: String,
}
impl RecoveryCopy {
    /// The opaque reference/path may be shown by the dedicated local key UI;
    /// it is not secret material and must never be interpreted as a vault name.
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn path(&self) -> PathBuf {
        self.directory.path.join(format!("{}.recovery", self.id))
    }
    fn read(&self) -> Result<LocalKeys> {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1).custom_flags(0x0020_0000);
        }
        let file = options
            .open(self.path())
            .map_err(|_| Error::Unavailable("recovery copy unavailable"))?;
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || private_paths::redirected(&metadata)
            || metadata.len() > RECOVERY_BYTES
        {
            return Err(Error::Access);
        }
        let mut bytes = Zeroizing::new(Vec::new());
        file.take(RECOVERY_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Unavailable("recovery copy could not be read"))?;
        if bytes.len() as u64 > RECOVERY_BYTES {
            return Err(Error::Limit("recovery material"));
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| Error::Corruption("invalid recovery encoding"))?;
        let mut lines = text.lines();
        if lines.next() != Some(HEADER) {
            return Err(Error::Incompatible);
        }
        let identity: Identity = lines
            .next()
            .ok_or(Error::Corruption("missing recovery identity"))?
            .parse()
            .map_err(|_| Error::Corruption("invalid recovery identity"))?;
        let writer = lines
            .next()
            .and_then(|line| line.strip_prefix(WRITER))
            .ok_or(Error::Corruption("missing writer recovery"))?;
        if writer.len() != 64 || lines.next().is_some() {
            return Err(Error::Corruption("invalid writer recovery"));
        }
        let mut seed = Zeroizing::new([0u8; 32]);
        for (i, pair) in writer.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            let digit = |b: u8| match b {
                b'0'..=b'9' => Some(b - b'0'),
                b'a'..=b'f' => Some(b - b'a' + 10),
                _ => None,
            };
            seed[i] = (digit(pair[0]).ok_or(Error::Corruption("invalid writer recovery"))? << 4)
                | digit(pair[1]).ok_or(Error::Corruption("invalid writer recovery"))?;
        }
        Ok(LocalKeys {
            identity,
            writer: SigningKey::from_bytes(&seed),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicKeys {
    pub key_ref: String,
    pub recipient: String,
    pub writer: [u8; 32],
}
pub struct LocalKeys {
    identity: Identity,
    writer: SigningKey,
}
impl LocalKeys {
    pub(crate) fn identity(&self) -> &Identity {
        &self.identity
    }
    pub fn generate() -> Result<Self> {
        let mut seed = Zeroizing::new([0u8; 32]);
        rand::rngs::OsRng
            .try_fill_bytes(seed.as_mut())
            .map_err(|_| Error::Unavailable("writer entropy unavailable"))?;
        Ok(Self {
            identity: Identity::generate(),
            writer: SigningKey::from_bytes(&seed),
        })
    }
    pub fn import(copy: &RecoveryCopy) -> Result<Self> {
        copy.read()
    }
    pub fn public(&self) -> Result<PublicKeys> {
        let recipient = self.identity.to_public().to_string();
        let writer = self.writer.verifying_key().to_bytes();
        let key_ref = digest_bytes(&canonical_bytes(&(
            "vcp-key-reference/1",
            &recipient,
            writer,
        ))?);
        Ok(PublicKeys {
            key_ref,
            recipient,
            writer,
        })
    }
    pub fn export_recovery(&self, directory: &RecoveryDirectory) -> Result<RecoveryCopy> {
        let copy = RecoveryCopy {
            directory: directory.0.clone(),
            id: CommandId::new().to_string(),
        };
        let identity = self.identity.to_string();
        let mut text = Zeroizing::new(format!("{HEADER}\n{}\n{WRITER}", identity.expose_secret()));
        let seed = Zeroizing::new(self.writer.to_bytes());
        for byte in seed.iter() {
            write!(&mut *text, "{byte:02x}")
                .map_err(|_| Error::Unavailable("recovery export formatting"))?;
        }
        text.push('\n');
        private_paths::write_private(&copy.path(), text.as_bytes())?;
        Ok(copy)
    }
    /// Always rereads the independently saved copy. The cached process secret
    /// alone is never sufficient to construct the publication capability.
    pub fn verify_recovery(self, copy: &RecoveryCopy) -> Result<VerifiedKeys> {
        let recovered = copy.read()?;
        let public = self.public()?;
        if recovered.public()? != public {
            return Err(Error::Conflict(
                "independent recovery does not match selected keys",
            ));
        }
        let mut challenge = [0u8; 32];
        rand::rngs::OsRng
            .try_fill_bytes(&mut challenge)
            .map_err(|_| Error::Unavailable("recovery challenge entropy unavailable"))?;
        let ciphertext = age::encrypt(&self.identity.to_public(), &challenge)
            .map_err(|_| Error::Unavailable("recovery encryption check failed"))?;
        if age::decrypt(&recovered.identity, &ciphertext)
            .map_err(|_| Error::Corruption("independent recovery decryption failed"))?
            != challenge
        {
            return Err(Error::Corruption("independent recovery round trip failed"));
        }
        let message = [RECOVERY_DOMAIN, &challenge].concat();
        self.writer
            .verifying_key()
            .verify_strict(&message, &recovered.writer.sign(&message))
            .map_err(|_| Error::Corruption("independent writer recovery failed"))?;
        Ok(VerifiedKeys {
            recipient: self.identity.to_public(),
            writer: self.writer,
            public,
        })
    }
}
/// Neither deserializable nor constructible from ordinary configuration. It
/// contains the signing secret required by unattended backups, but no retained
/// decryption secret. Process restart requires a fresh explicit local import;
/// an optional OS credential cache is deliberately not implemented here.
pub struct VerifiedKeys {
    pub(crate) recipient: Recipient,
    pub(crate) writer: SigningKey,
    public: PublicKeys,
}
impl VerifiedKeys {
    pub fn public(&self) -> &PublicKeys {
        &self.public
    }
}
