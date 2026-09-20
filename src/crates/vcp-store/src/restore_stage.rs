// SPDX-License-Identifier: Apache-2.0
//! Durable private restore acquisition and validation. Reading status never
//! decrypts, extracts, imports, changes writer trust or activates a data root.
use crate::{
    artifact::read_bounded,
    keys::RecoveryCopy,
    portable_snapshot::Archive,
    private_paths::{self, Directory},
    vault_crypto::Limits,
    vault_publish::{LocalTrust, VerifiedRestore},
    BackendKind, Error, Result,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};
use vcp_domain::{ActorId, CommandId, Timestamp, TransactionId, WorkspaceId};
use vcp_protocol::{canonical_bytes, digest_bytes};

const MAX_ENTRIES: usize = 32;
const MAX_CIPHERTEXT: usize = 65 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Pending,
    Acquired,
    Validated,
    Importing,
    Imported,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub version: u32,
    pub operation: CommandId,
    pub workspace: WorkspaceId,
    pub revision: u64,
    pub trust_revision: u64,
    pub trust_digest: String,
    pub ciphertext: String,
    pub bytes: u64,
    pub stage: Stage,
    pub manifest: Option<String>,
    pub inventory: Option<String>,
    pub import: Option<ImportIntent>,
    pub canonical_imported: bool,
    pub search_ready: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportIntent {
    pub root: String,
    pub root_digest: String,
    pub backend: BackendKind,
    pub actor: ActorId,
    pub timestamp: Timestamp,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    previous: String,
    status: Status,
}
pub struct Restore {
    directory: Directory,
    status: Status,
    digest: String,
    _owner: File,
}
/// An authenticated archive and its independent local trust proof. Neither
/// serialized status nor extracted files can construct this capability.
pub struct Validated {
    pub(crate) archive: Archive,
    pub(crate) proof: VerifiedRestore,
    operation: CommandId,
    trust_revision: u64,
}
impl Validated {
    pub fn archive(&self) -> &Archive {
        &self.archive
    }
    pub fn proof(&self) -> &VerifiedRestore {
        &self.proof
    }
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn immutable(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        if read_bounded(path, bytes.len())? == bytes {
            return Ok(());
        }
        return Err(Error::Conflict("restore private file differs"));
    }
    let temporary = path.with_extension(format!("{}.partial", TransactionId::new()));
    private_paths::write_private(&temporary, bytes)?;
    if let Err(error) = fs::hard_link(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    let _ = fs::remove_file(temporary);
    Ok(())
}
impl Restore {
    /// The directory must already exist outside the workspace, vault and all
    /// declared sync roots. One local owner serializes this operation's journal.
    pub fn open(directory: &Path, forbidden: &[PathBuf]) -> Result<Self> {
        let directory = Directory::open(directory, forbidden)?;
        let owner = Self::owner(&directory)?;
        let mut entries = Vec::new();
        let mut count = 0usize;
        for entry in fs::read_dir(&directory.path)? {
            count += 1;
            if count > MAX_ENTRIES * 3 + 8 {
                return Err(Error::Limit("restore directory entries"));
            }
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_str().ok_or(Error::Access)?;
            if name.starts_with("restore-") && name.ends_with(".json") {
                entries.push(entry.path());
            }
        }
        entries.sort();
        if entries.is_empty() || entries.len() > MAX_ENTRIES {
            return Err(Error::Corruption("restore journal missing or oversized"));
        }
        let mut previous = "0".repeat(64);
        let mut prior: Option<Status> = None;
        for (index, path) in entries.iter().enumerate() {
            let bytes = read_bounded(path, 16384)?;
            let entry: Entry = serde_json::from_slice(&bytes)?;
            if path.file_name().and_then(|s| s.to_str())
                != Some(Self::filename(index as u64).as_str())
                || canonical_bytes(&entry)? != bytes
                || entry.previous != previous
                || entry.status.revision != index as u64
            {
                return Err(Error::Corruption("restore journal chain"));
            }
            Self::validate(&entry.status)?;
            if let Some(old) = prior.as_ref() {
                Self::transition(old, &entry.status)?;
            } else if entry.status.stage != Stage::Pending {
                return Err(Error::Corruption("restore initial stage"));
            }
            previous = digest_bytes(&bytes);
            prior = Some(entry.status);
        }
        Ok(Self {
            directory,
            status: prior.ok_or(Error::Corruption("restore journal missing"))?,
            digest: previous,
            _owner: owner,
        })
    }
    pub fn begin(
        directory: &Path,
        forbidden: &[PathBuf],
        operation: CommandId,
        trust: &LocalTrust,
        ciphertext: String,
        bytes: u64,
    ) -> Result<Self> {
        let directory = Directory::open(directory, forbidden)?;
        let owner = Self::owner(&directory)?;
        if fs::read_dir(&directory.path)?
            .any(|e| e.map_or(true, |e| e.file_name() != "restore-owner.lock"))
        {
            return Err(Error::Conflict("restore directory is not empty"));
        }
        let status = Status {
            version: 1,
            operation,
            workspace: trust.configuration().workspace.clone(),
            revision: 0,
            trust_revision: trust.configuration().revision,
            trust_digest: digest_bytes(&canonical_bytes(trust.configuration())?),
            ciphertext,
            bytes,
            stage: Stage::Pending,
            manifest: None,
            inventory: None,
            import: None,
            canonical_imported: false,
            search_ready: false,
        };
        Self::validate(&status)?;
        let entry = canonical_bytes(&Entry {
            previous: "0".repeat(64),
            status: status.clone(),
        })?;
        immutable(&directory.path.join(Self::filename(0)), &entry)?;
        Ok(Self {
            directory,
            status,
            digest: digest_bytes(&entry),
            _owner: owner,
        })
    }
    fn owner(directory: &Directory) -> Result<File> {
        let path = directory.path.join("restore-owner.lock");
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if !metadata.is_file() || private_paths::redirected(&metadata) {
                return Err(Error::Access);
            }
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x0020_0000);
        }
        let file = options.open(path)?;
        if private_paths::redirected(&file.metadata()?) {
            return Err(Error::Access);
        }
        file.try_lock()
            .map_err(|_| Error::Conflict("restore already owned"))?;
        Ok(file)
    }
    fn filename(revision: u64) -> String {
        format!("restore-{revision:020}.json")
    }
    pub fn status(&self) -> &Status {
        &self.status
    }
    fn validate(status: &Status) -> Result<()> {
        if status.version != 1
            || status.revision >= MAX_ENTRIES as u64
            || !hash(&status.ciphertext)
            || !hash(&status.trust_digest)
            || status.bytes == 0
            || status.bytes > MAX_CIPHERTEXT as u64
            || status.canonical_imported != (status.stage == Stage::Imported)
            || status.search_ready
            || status.manifest.as_ref().is_some_and(|s| !hash(s))
            || status.inventory.as_ref().is_some_and(|s| !hash(s))
            || matches!(
                status.stage,
                Stage::Validated | Stage::Importing | Stage::Imported
            ) && (status.manifest.is_none() || status.inventory.is_none())
            || matches!(status.stage, Stage::Importing | Stage::Imported) != status.import.is_some()
            || status.import.as_ref().is_some_and(|i| {
                let s = Path::new(&i.root)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                !Path::new(&i.root).is_absolute()
                    || i.root.len() > 32768
                    || digest_bytes(i.root.as_bytes()) != i.root_digest
                    || s.len() != 36
                    || s.bytes().enumerate().any(|(i, b)| {
                        if [8, 13, 18, 23].contains(&i) {
                            b != b'-'
                        } else {
                            !b.is_ascii_hexdigit()
                        }
                    })
            })
        {
            return Err(Error::Corruption("restore status shape"));
        }
        Ok(())
    }
    fn transition(old: &Status, new: &Status) -> Result<()> {
        if new.revision != old.revision + 1
            || old.operation != new.operation
            || old.workspace != new.workspace
            || old.trust_revision != new.trust_revision
            || old.trust_digest != new.trust_digest
            || old.ciphertext != new.ciphertext
            || old.bytes != new.bytes
            || old.manifest.is_some() && old.manifest != new.manifest
            || old.inventory.is_some() && old.inventory != new.inventory
            || old.import.is_some() && old.import != new.import
            || !matches!(
                (old.stage, new.stage),
                (Stage::Pending, Stage::Acquired)
                    | (Stage::Acquired, Stage::Validated)
                    | (Stage::Validated, Stage::Importing)
                    | (Stage::Importing, Stage::Imported)
            )
        {
            return Err(Error::Corruption("restore stage transition"));
        }
        Ok(())
    }
    fn advance(&mut self, mut next: Status) -> Result<()> {
        next.revision = self
            .status
            .revision
            .checked_add(1)
            .ok_or(Error::Limit("restore revision"))?;
        Self::validate(&next)?;
        Self::transition(&self.status, &next)?;
        let bytes = canonical_bytes(&Entry {
            previous: self.digest.clone(),
            status: next.clone(),
        })?;
        immutable(
            &self.directory.path.join(Self::filename(next.revision)),
            &bytes,
        )?;
        self.digest = digest_bytes(&bytes);
        self.status = next;
        Ok(())
    }
    pub fn acquire(&mut self, source: &Path, cancelled: &dyn Fn() -> bool) -> Result<()> {
        if cancelled() {
            return Err(Error::Unavailable("restore cancelled"));
        }
        if self.status.stage != Stage::Pending {
            return Err(Error::Conflict("restore acquisition stage"));
        }
        let bytes = read_bounded(source, self.status.bytes as usize)?;
        if bytes.len() as u64 != self.status.bytes || digest_bytes(&bytes) != self.status.ciphertext
        {
            return Err(Error::Corruption("restore ciphertext identity"));
        }
        if cancelled() {
            return Err(Error::Unavailable("restore cancelled"));
        }
        immutable(&self.directory.path.join("ciphertext.age"), &bytes)?;
        let mut next = self.status.clone();
        next.stage = Stage::Acquired;
        self.advance(next)
    }
    pub fn authenticate(
        &mut self,
        trust: &LocalTrust,
        recovery: &RecoveryCopy,
        limits: Limits,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Validated> {
        if cancelled() {
            return Err(Error::Unavailable("restore cancelled"));
        }
        if !matches!(
            self.status.stage,
            Stage::Acquired | Stage::Validated | Stage::Importing | Stage::Imported
        ) || trust.configuration().revision != self.status.trust_revision
            || trust.configuration().workspace != self.status.workspace
        {
            return Err(Error::Conflict("restore trust or stage changed"));
        }
        let path = self.directory.path.join("ciphertext.age");
        let bytes = read_bounded(&path, self.status.bytes as usize)?;
        if bytes.len() as u64 != self.status.bytes || digest_bytes(&bytes) != self.status.ciphertext
        {
            return Err(Error::Corruption("restore acquired ciphertext changed"));
        }
        let proof = trust.verify_restore(&path, recovery, limits)?;
        if cancelled() {
            return Err(Error::Unavailable("restore cancelled"));
        }
        let restored = proof.restored();
        let mut inventories = restored.payloads.iter().filter_map(|(hash, bytes)| {
            let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
            (value.get("format")?.as_str()? == "vcp-neutral-history/1").then_some(hash.clone())
        });
        let inventory = inventories
            .next()
            .ok_or(Error::Corruption("restore inventory missing"))?;
        if inventories.next().is_some() {
            return Err(Error::Corruption("ambiguous restore inventory"));
        }
        let archive = Archive::decode(restored.payloads.clone(), &inventory)?;
        if archive.workspace() != &self.status.workspace {
            return Err(Error::Access);
        }
        let manifest = digest_bytes(&canonical_bytes(&restored.manifest)?);
        if self.status.stage == Stage::Acquired {
            let mut next = self.status.clone();
            next.manifest = Some(manifest.clone());
            next.inventory = Some(inventory.clone());
            next.stage = Stage::Validated;
            self.advance(next)?;
        } else if self.status.manifest.as_ref() != Some(&manifest)
            || self.status.inventory.as_ref() != Some(&inventory)
        {
            return Err(Error::Corruption("restore validation receipt differs"));
        }
        Ok(Validated {
            archive,
            proof,
            operation: self.status.operation.clone(),
            trust_revision: self.status.trust_revision,
        })
    }
    /// Persist the exact destination and sanitizer facts before materializing
    /// any plaintext canonical files. Restart repeats only this same import.
    #[allow(clippy::too_many_arguments)] // Explicit independent trust, destination and cancellation boundaries.
    pub async fn import(
        &mut self,
        validated: &Validated,
        trust: &LocalTrust,
        backend: BackendKind,
        destination: &Path,
        actor: ActorId,
        timestamp: Timestamp,
        forbidden: &[PathBuf],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<crate::restore_import::Imported> {
        if cancelled() {
            return Err(Error::Unavailable("restore import cancelled"));
        }
        if !matches!(
            self.status.stage,
            Stage::Validated | Stage::Importing | Stage::Imported
        ) || validated.operation != self.status.operation
            || validated.trust_revision != self.status.trust_revision
            || digest_bytes(&canonical_bytes(trust.configuration())?) != self.status.trust_digest
            || trust.configuration().revision != self.status.trust_revision
            || trust.configuration().workspace != self.status.workspace
        {
            return Err(Error::Conflict("restore import proof or trust changed"));
        }
        let destination_text = destination.to_str().ok_or(Error::Access)?;
        if !destination.is_absolute()
            || destination.file_name().and_then(|s| s.to_str())
                != Some(self.status.operation.as_str())
        {
            return Err(Error::Access);
        }
        let _parent = Directory::open(destination.parent().ok_or(Error::Access)?, forbidden)?;
        if self.status.stage == Stage::Validated {
            if destination.exists() {
                return Err(Error::Conflict("restore destination already exists"));
            }
            let mut next = self.status.clone();
            next.stage = Stage::Importing;
            next.import = Some(ImportIntent {
                root: destination_text.to_owned(),
                root_digest: digest_bytes(destination_text.as_bytes()),
                backend,
                actor,
                timestamp,
            });
            self.advance(next)?;
        }
        let intent = self
            .status
            .import
            .as_ref()
            .ok_or(Error::Corruption("restore import intent missing"))?;
        if backend != intent.backend || intent.root != destination_text {
            return Err(Error::Conflict("restore backend changed"));
        }
        let imported = crate::restore_import::prepare(
            validated,
            destination,
            forbidden,
            intent.backend,
            &self.status.operation,
            &intent.actor,
            intent.timestamp,
            cancelled,
        )
        .await?;
        if self.status.stage == Stage::Importing {
            let mut next = self.status.clone();
            next.stage = Stage::Imported;
            next.canonical_imported = true;
            self.advance(next)?;
        }
        Ok(imported)
    }
}
