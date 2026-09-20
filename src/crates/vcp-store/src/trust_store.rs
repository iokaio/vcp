// SPDX-License-Identifier: Apache-2.0
//! Independently enrolled local writer/head knowledge. This directory is never
//! imported from a backup or chosen by repository configuration. The hash chain
//! detects damage; it does not claim protection from the local account owner or
//! knowledge of a globally newest remote snapshot.
use crate::{
    artifact::read_bounded,
    private_paths::{self, Directory},
    vault_publish::{LocalTrust, PublicConfiguration},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::Path,
};
use vcp_protocol::{canonical_bytes, digest_bytes};

const MAX_ENTRY: usize = 65536;
const MAX_ENTRIES: usize = 4096;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    version: u32,
    ordinal: u64,
    previous: String,
    configuration: PublicConfiguration,
}
pub struct TrustStore {
    directory: Directory,
    trust: LocalTrust,
    ordinal: u64,
    digest: String,
    _owner: File,
}
fn immutable_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = path.with_extension(format!("{}.partial", vcp_domain::TransactionId::new()));
    private_paths::write_private(&temporary, bytes)?;
    if let Err(error) = fs::hard_link(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    // File content was flushed before atomic create-only publication. On
    // Windows we claim forced-process recovery, not hardware directory flush.
    // An abandoned alternate name cannot become authority and may be cleaned
    // on a later maintenance pass; publication has already succeeded here.
    let _ = fs::remove_file(&temporary);
    Ok(())
}
fn filename(ordinal: u64) -> String {
    format!("trust-{ordinal:020}.json")
}
fn owner(directory: &Directory) -> Result<File> {
    let path = directory.path.join("trust-owner.lock");
    if let Ok(meta) = fs::symlink_metadata(&path) {
        if !meta.is_file() || private_paths::redirected(&meta) {
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
        .map_err(|_| Error::Conflict("local trust already owned"))?;
    Ok(file)
}
fn entries(directory: &Directory) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    for (count, entry) in fs::read_dir(&directory.path)?.enumerate() {
        if count > MAX_ENTRIES * 2 {
            return Err(Error::Limit("local trust directory entries"));
        }
        let entry = entry?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or(Error::Corruption("local trust filename"))?;
        if name.starts_with("trust-") && name != "trust-owner.lock" {
            // A crash before create-only publication may leave an owned public
            // metadata staging file. It is not a committed trust revision.
            if name.ends_with(".partial") {
                continue;
            }
            if !name.ends_with(".json")
                || name.len() != 31
                || !name.as_bytes()[6..26].iter().all(u8::is_ascii_digit)
            {
                return Err(Error::Corruption("local trust entry name"));
            }
            files.push(entry.path());
            if files.len() > MAX_ENTRIES {
                return Err(Error::Limit("local trust history"));
            }
        }
    }
    files.sort();
    Ok(files)
}
impl TrustStore {
    /// Independently selected local directory held by this journal's owner.
    pub fn directory(&self) -> &Path {
        &self.directory.path
    }
    /// The host obtains `trust` only through explicit verified key enrollment.
    /// Existing local knowledge is never overwritten by enrollment or restore.
    pub fn enroll(
        path: &Path,
        forbidden: &[std::path::PathBuf],
        trust: LocalTrust,
    ) -> Result<Self> {
        let directory = Directory::open(path, forbidden)?;
        let owner = owner(&directory)?;
        if !entries(&directory)?.is_empty() {
            return Err(Error::Conflict("local trust already enrolled"));
        }
        if trust.configuration().revision != 0 {
            return Err(Error::Conflict("initial trust revision"));
        }
        let configuration = trust.configuration().clone();
        LocalTrust::from_local_configuration(configuration.clone())?;
        let entry = Entry {
            version: 1,
            ordinal: 0,
            previous: "0".repeat(64),
            configuration,
        };
        let bytes = canonical_bytes(&entry)?;
        if bytes.len() > MAX_ENTRY {
            return Err(Error::Limit("local trust entry"));
        }
        immutable_file(&directory.path.join(filename(0)), &bytes)?;
        Ok(Self {
            directory,
            trust,
            ordinal: 0,
            digest: digest_bytes(&bytes),
            _owner: owner,
        })
    }
    /// Only a separately selected private local directory is accepted. Restore
    /// archives have no operation that imports these records into this path.
    pub fn open(path: &Path, forbidden: &[std::path::PathBuf]) -> Result<Self> {
        let directory = Directory::open(path, forbidden)?;
        let owner = owner(&directory)?;
        let files = entries(&directory)?;
        if files.is_empty() {
            return Err(Error::Unavailable("local trust is not enrolled"));
        }
        let mut previous = "0".repeat(64);
        let mut selected: Option<LocalTrust> = None;
        for (ordinal, path) in files.iter().enumerate() {
            if path.file_name().and_then(|n| n.to_str()) != Some(filename(ordinal as u64).as_str())
            {
                return Err(Error::Corruption("local trust history gap"));
            }
            let bytes = read_bounded(path, MAX_ENTRY)?;
            let entry: Entry = serde_json::from_slice(&bytes)?;
            if entry.version != 1
                || entry.ordinal != ordinal as u64
                || entry.previous != previous
                || canonical_bytes(&entry)? != bytes
            {
                return Err(Error::Corruption("local trust chain"));
            }
            if let Some(prior) = &selected {
                let prior = prior.configuration();
                let next = &entry.configuration;
                if next.revision
                    != prior
                        .revision
                        .checked_add(1)
                        .ok_or(Error::Limit("trust revision"))?
                    || next.workspace != prior.workspace
                    || next.lineage != prior.lineage
                    || next.checkpoint.sequence < prior.checkpoint.sequence
                    || next.checkpoint.deletion < prior.checkpoint.deletion
                    || (next.checkpoint.sequence == prior.checkpoint.sequence
                        && next.checkpoint.parent != prior.checkpoint.parent)
                {
                    return Err(Error::Corruption("local trust transition"));
                }
            } else if entry.configuration.revision != 0 {
                return Err(Error::Corruption("initial trust revision"));
            }
            selected = Some(LocalTrust::from_local_configuration(entry.configuration)?);
            previous = digest_bytes(&bytes);
        }
        Ok(Self {
            directory,
            trust: selected.ok_or(Error::Corruption("missing local trust"))?,
            ordinal: (files.len() - 1) as u64,
            digest: previous,
            _owner: owner,
        })
    }
    pub fn trust(&self) -> &LocalTrust {
        &self.trust
    }
    /// Mutate a temporary capability, flush the exact next immutable record,
    /// then expose it in memory. A failed write preserves current authority.
    /// A lost success reply is reconciled by reopening and checking revision.
    pub fn update(
        &mut self,
        expected: u64,
        change: impl FnOnce(&mut LocalTrust) -> Result<()>,
    ) -> Result<()> {
        if self.trust.configuration().revision != expected {
            return Err(Error::Conflict("local trust revision changed"));
        }
        let mut next = LocalTrust::from_local_configuration(self.trust.configuration().clone())?;
        change(&mut next)?;
        if canonical_bytes(next.configuration())? == canonical_bytes(self.trust.configuration())? {
            return Ok(());
        }
        let prior = self.trust.configuration();
        let config = next.configuration();
        if config.revision
            != expected
                .checked_add(1)
                .ok_or(Error::Limit("trust revision"))?
            || config.workspace != prior.workspace
            || config.lineage != prior.lineage
            || config.checkpoint.sequence < prior.checkpoint.sequence
            || config.checkpoint.deletion < prior.checkpoint.deletion
            || (config.checkpoint.sequence == prior.checkpoint.sequence
                && config.checkpoint.parent != prior.checkpoint.parent)
        {
            return Err(Error::Conflict("invalid local trust transition"));
        }
        if self.ordinal as usize + 1 >= MAX_ENTRIES {
            return Err(Error::Limit("local trust history"));
        }
        let ordinal = self.ordinal + 1;
        let entry = Entry {
            version: 1,
            ordinal,
            previous: self.digest.clone(),
            configuration: config.clone(),
        };
        let bytes = canonical_bytes(&entry)?;
        if bytes.len() > MAX_ENTRY {
            return Err(Error::Limit("local trust entry"));
        }
        immutable_file(&self.directory.path.join(filename(ordinal)), &bytes)?;
        self.trust = next;
        self.ordinal = ordinal;
        self.digest = digest_bytes(&bytes);
        Ok(())
    }
}
